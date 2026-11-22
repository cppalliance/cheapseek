//! Chat completions as a stream of server-sent events.

use serde_json::{Value, json};

use crate::support::{CHAT, Server, client};

/// The reply the bot gives to the greeting below.
const GREETING_REPLY: &str = "Nice to meet you, Sam. What can I help with?";

/// A chat request that greets the bot and asks for a stream, with `extra`
/// fields added.
fn greeting(extra: &Value) -> Value {
    let mut body = json!({
        "model": "cheapseek",
        "stream": true,
        "messages": [{ "role": "user", "content": "Hi, I'm Sam." }]
    });
    for (name, value) in extra.as_object().expect("an object of extra fields") {
        body[name] = value.clone();
    }
    body
}

/// A stream as the test reads it: the response headers that matter, and the
/// JSON of every event before the `[DONE]` line.
struct Stream {
    content_type: String,
    chunks: Vec<Value>,
    text: String,
}

impl Stream {
    /// Sends `body` to the chat route and reads the whole stream.
    async fn read(server: &Server, body: &Value) -> Stream {
        let response = client()
            .post(server.url(CHAT))
            .json(body)
            .send()
            .await
            .expect("the server answers");
        assert_eq!(response.status().as_u16(), 200);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let text = response.text().await.expect("a readable stream");

        let mut events: Vec<&str> = text.split("\n\n").collect();
        assert_eq!(events.pop(), Some(""), "the stream ends with a blank line");
        assert_eq!(events.pop(), Some("data: [DONE]"), "{text}");
        let chunks = events
            .into_iter()
            .map(|event| {
                let json = event.strip_prefix("data: ").expect("a data line");
                serde_json::from_str(json).expect("a JSON chunk")
            })
            .collect();
        Stream {
            content_type,
            chunks,
            text,
        }
    }

    /// The content of every delta that holds content, in order.
    fn contents(&self) -> Vec<&str> {
        self.chunks
            .iter()
            .filter_map(|chunk| chunk["choices"][0]["delta"]["content"].as_str())
            .collect()
    }
}

#[tokio::test]
async fn a_stream_sends_the_role_one_token_per_delta_the_finish_and_done() {
    let server = Server::start().await;
    let stream = Stream::read(&server, &greeting(&json!({}))).await;
    assert_eq!(stream.content_type, "text/event-stream");

    let deltas: Vec<&Value> = stream
        .chunks
        .iter()
        .map(|chunk| &chunk["choices"][0]["delta"])
        .collect();
    let (first, rest) = deltas.split_first().expect("a first chunk");
    assert_eq!(
        **first,
        json!({ "role": "assistant", "content": "", "refusal": null })
    );
    let (last, tokens) = rest.split_last().expect("a finish chunk");
    assert_eq!(**last, json!({}));
    let tokens: Vec<Value> = tokens.iter().map(|delta| (*delta).clone()).collect();
    let expected: Vec<Value> = [
        "Nice", " to", " meet", " you", ",", " Sam", ".", " What", " can", " I", " help", " with",
        "?",
    ]
    .iter()
    .map(|token| json!({ "content": token }))
    .collect();
    assert_eq!(tokens, expected);

    let finishes: Vec<&Value> = stream
        .chunks
        .iter()
        .map(|chunk| &chunk["choices"][0]["finish_reason"])
        .collect();
    let (finish, before) = finishes.split_last().expect("a finish chunk");
    assert_eq!(**finish, json!("stop"));
    assert!(before.iter().all(|reason| reason.is_null()));
}

#[tokio::test]
async fn every_chunk_of_a_stream_names_the_same_completion() {
    let server = Server::start().await;
    let stream = Stream::read(&server, &greeting(&json!({}))).await;
    let id = stream.chunks[0]["id"].as_str().expect("an id").to_owned();
    assert!(id.starts_with("chatcmpl-"), "{id}");
    for chunk in &stream.chunks {
        assert_eq!(chunk["id"], id.as_str());
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert_eq!(chunk["created"], 1_795_305_600);
        assert_eq!(chunk["model"], "cheapseek");
        assert_eq!(chunk["system_fingerprint"], "fp_cheapseek");
        assert_eq!(chunk["choices"][0]["index"], 0);
        assert_eq!(chunk["choices"][0]["logprobs"], Value::Null);
        assert_eq!(chunk.get("usage"), None, "usage was not asked for");
    }
}

#[tokio::test]
async fn the_deltas_joined_are_the_reply_and_their_count_is_the_completion_tokens() {
    let server = Server::start().await;
    let stream = Stream::read(&server, &greeting(&json!({}))).await;
    let mut plain = greeting(&json!({}));
    plain["stream"] = json!(false);
    let (_, whole) = server.chat(&plain).await;

    // The first delta holds empty content, so it adds nothing to the text
    // and is not a token.
    let contents = stream.contents();
    assert_eq!(contents.concat(), GREETING_REPLY);
    assert_eq!(whole["choices"][0]["message"]["content"], GREETING_REPLY);
    assert_eq!(
        whole["usage"]["completion_tokens"],
        contents.len() as u64 - 1
    );
}

#[tokio::test]
async fn the_usage_chunk_appears_only_on_request() {
    let server = Server::start().await;
    let stream = Stream::read(
        &server,
        &greeting(&json!({ "stream_options": { "include_usage": true } })),
    )
    .await;
    let (last, before) = stream.chunks.split_last().expect("chunks");
    assert_eq!(last["choices"], json!([]));
    assert_eq!(
        last["usage"],
        json!({ "prompt_tokens": 9, "completion_tokens": 13, "total_tokens": 22 })
    );
    for chunk in before {
        assert_eq!(chunk["usage"], Value::Null, "{chunk}");
        assert_eq!(chunk["choices"].as_array().map(Vec::len), Some(1));
    }
    assert_eq!(
        before.last().expect("a finish chunk")["choices"][0]["finish_reason"],
        "stop"
    );

    let without = Stream::read(
        &server,
        &greeting(&json!({ "stream_options": { "include_usage": false } })),
    )
    .await;
    assert!(
        without
            .chunks
            .iter()
            .all(|chunk| chunk.get("usage").is_none())
    );
}

#[tokio::test]
async fn a_token_limit_ends_the_stream_with_length() {
    let server = Server::start().await;
    let stream = Stream::read(&server, &greeting(&json!({ "max_tokens": 2 }))).await;
    assert_eq!(stream.contents(), ["", "Nice", " to"]);
    let last = stream.chunks.last().expect("a finish chunk");
    assert_eq!(last["choices"][0]["finish_reason"], "length");
}

#[tokio::test]
async fn the_echo_model_streams_the_prompt() {
    let server = Server::start().await;
    let stream = Stream::read(
        &server,
        &json!({
            "model": "cheapseek-echo",
            "stream": true,
            "messages": [{ "role": "user", "content": "Hello, world!" }]
        }),
    )
    .await;
    assert_eq!(
        stream.contents(),
        ["", "user", ":", " Hell", "o", ",", " worl", "d", "!"]
    );
}

#[tokio::test]
async fn the_same_request_twice_gets_the_same_stream_bytes() {
    let server = Server::start().await;
    let body = greeting(&json!({ "stream_options": { "include_usage": true } }));
    let first = Stream::read(&server, &body).await;
    let second = Stream::read(&server, &body).await;
    assert_eq!(first.text, second.text);
}

#[tokio::test]
async fn stream_options_without_a_stream_get_a_400() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "stream_options": { "include_usage": true },
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["param"], "stream_options");
}

#[tokio::test]
async fn a_request_the_server_refuses_gets_json_even_when_it_asks_for_a_stream() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "no-such-model",
            "stream": true,
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "model_not_found");
}
