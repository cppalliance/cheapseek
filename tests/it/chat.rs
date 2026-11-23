//! Chat completions as one JSON object: the reply, its usage, the limits,
//! and that the same request always gets the same bytes.

use serde_json::{Value, json};

use crate::support::Server;

/// The reply the bot gives to the greeting below, at any seed.
const GREETING_REPLY: &str = "Nice to meet you, Sam. What can I help with?";

/// A chat request that greets the bot, with `extra` fields added.
fn greeting(extra: &Value) -> Value {
    let mut body = json!({
        "model": "cheapseek",
        "messages": [{ "role": "user", "content": "Hi, I'm Sam." }]
    });
    for (name, value) in extra.as_object().expect("an object of extra fields") {
        body[name] = value.clone();
    }
    body
}

#[tokio::test]
async fn a_greeting_gets_a_chat_reply_that_stops_with_usage_that_adds_up() {
    let server = Server::start().await;
    let (status, body) = server.chat(&greeting(&json!({}))).await;
    assert_eq!(status, 200);

    let id = body["id"].as_str().expect("an id");
    let digits = id.strip_prefix("chatcmpl-").expect("the id prefix");
    assert_eq!(digits.len(), 16, "{id}");
    assert!(
        digits.chars().all(|digit| digit.is_ascii_hexdigit()),
        "{id}"
    );

    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["created"], 1_795_305_600);
    assert_eq!(body["model"], "cheapseek");
    assert_eq!(body["system_fingerprint"], "fp_cheapseek");
    assert_eq!(
        body["choices"],
        json!([{
            "index": 0,
            "message": { "role": "assistant", "content": GREETING_REPLY, "refusal": null },
            "logprobs": null,
            "finish_reason": "stop"
        }])
    );
    // The prompt is `user: Hi, I'm Sam.`, which is 9 tokens by the token
    // rule, and the reply is 13.
    assert_eq!(
        body["usage"],
        json!({ "prompt_tokens": 9, "completion_tokens": 13, "total_tokens": 22 })
    );
}

#[tokio::test]
async fn the_same_request_twice_gets_the_same_bytes() {
    let server = Server::start().await;
    let body = greeting(&json!({})).to_string();
    let first = server.chat_bytes(&body).await;
    let second = server.chat_bytes(&body).await;
    assert_eq!(first.0, 200);
    assert_eq!(first, second);
}

#[tokio::test]
async fn a_different_request_gets_a_different_id() {
    let server = Server::start().await;
    let (_, plain) = server.chat(&greeting(&json!({}))).await;
    let (_, seeded) = server.chat(&greeting(&json!({ "seed": 7 }))).await;
    assert_ne!(plain["id"], seeded["id"]);
    assert_eq!(plain["choices"], seeded["choices"]);
}

#[tokio::test]
async fn a_limit_of_two_tokens_cuts_the_reply_to_two_tokens_with_length() {
    let server = Server::start().await;
    for name in ["max_tokens", "max_completion_tokens"] {
        let (status, body) = server.chat(&greeting(&json!({ name: 2 }))).await;
        assert_eq!(status, 200);
        assert_eq!(body["choices"][0]["message"]["content"], "Nice to");
        assert_eq!(body["choices"][0]["finish_reason"], "length");
        assert_eq!(body["usage"]["completion_tokens"], 2);
    }
}

#[tokio::test]
async fn a_limit_the_reply_fits_in_leaves_it_whole() {
    let server = Server::start().await;
    let (_, body) = server.chat(&greeting(&json!({ "max_tokens": 13 }))).await;
    assert_eq!(body["choices"][0]["message"]["content"], GREETING_REPLY);
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn a_stop_sequence_ends_the_reply_before_it() {
    let server = Server::start().await;
    let (status, body) = server.chat(&greeting(&json!({ "stop": "Sam" }))).await;
    assert_eq!(status, 200);
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "Nice to meet you, "
    );
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert_eq!(body["usage"]["completion_tokens"], 5);

    let (_, body) = server
        .chat(&greeting(&json!({ "stop": ["What", "meet"] })))
        .await;
    assert_eq!(body["choices"][0]["message"]["content"], "Nice to ");
}

#[tokio::test]
async fn content_given_as_parts_reads_like_content_given_as_a_string() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "Hi, I'm Sam." }]
            }]
        }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["message"]["content"], GREETING_REPLY);
}

#[tokio::test]
async fn the_reply_depends_on_the_earlier_user_turns_of_the_request() {
    let server = Server::start().await;
    let question = json!({ "role": "user", "content": "What's my name?" });
    let (_, with_history) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [
                { "role": "system", "content": "You are a helpful assistant." },
                { "role": "user", "content": "Hi, I'm Sam." },
                { "role": "assistant", "content": GREETING_REPLY },
                question
            ]
        }))
        .await;
    assert_eq!(
        with_history["choices"][0]["message"]["content"],
        "You're Sam."
    );

    let (_, alone) = server
        .chat(&json!({ "model": "cheapseek", "messages": [question] }))
        .await;
    let alone = alone["choices"][0]["message"]["content"]
        .as_str()
        .expect("a reply");
    assert!(!alone.contains("Sam"), "{alone}");
}

#[tokio::test]
async fn a_request_without_a_user_message_gets_the_line_of_a_bot_that_heard_nothing() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [
                { "role": "system", "content": "Be brief. [[say This block is in no user message.]]" },
                { "role": "assistant", "content": "Ready." }
            ]
        }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn fields_the_server_has_no_use_for_are_accepted() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&greeting(&json!({
            "temperature": 0.2,
            "top_p": 0.9,
            "user": "test-user",
            "stream": false,
            "chat_template_kwargs": { "enable_thinking": false },
            "a_field_from_the_future": [1, 2, 3]
        })))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["message"]["content"], GREETING_REPLY);
}
