//! Tool calls: the directives that script them, the requests that ask for
//! them, the results that end a loop, and the stream that sends them.

use serde_json::{Value, json};

use crate::support::{CHAT, Server, client};

/// The JSON Schema of the arguments of `get_weather`.
fn weather_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "city": { "type": "string", "minLength": 2 },
            "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] },
            "days": { "type": "integer", "minimum": 1, "maximum": 7 }
        },
        "required": ["city", "unit", "days"],
        "additionalProperties": false
    })
}

/// Two tools, as a harness would offer them.
fn tools() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get the weather for a city.",
                "parameters": weather_parameters()
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        }
    ])
}

/// A chat request with the two tools, these messages, and `extra` fields.
fn request(messages: Value, extra: &Value) -> Value {
    let mut body = json!({ "model": "cheapseek", "tools": tools() });
    body["messages"] = messages;
    for (name, value) in extra.as_object().expect("an object of extra fields") {
        body[name] = value.clone();
    }
    body
}

/// A request whose one message is the user saying `text`.
fn user(text: &str) -> Value {
    request(json!([{ "role": "user", "content": text }]), &json!({}))
}

/// The name and the arguments of each tool call of a reply, after checks
/// of everything else about a reply that calls tools.
fn calls(body: &Value) -> Vec<(String, Value)> {
    let choice = &body["choices"][0];
    assert_eq!(choice["finish_reason"], "tool_calls", "{body}");
    assert_eq!(choice["message"]["role"], "assistant");
    assert_eq!(choice["message"]["content"], Value::Null);
    choice["message"]["tool_calls"]
        .as_array()
        .expect("tool calls")
        .iter()
        .map(|call| {
            assert_eq!(call["type"], "function");
            let id = call["id"].as_str().expect("an id");
            let digits = id.strip_prefix("call_").expect("the id prefix");
            assert_eq!(digits.len(), 16, "{id}");
            let name = call["function"]["name"]
                .as_str()
                .expect("a name")
                .to_owned();
            let arguments = call["function"]["arguments"]
                .as_str()
                .expect("arguments as text");
            (
                name,
                serde_json::from_str(arguments).expect("arguments that are JSON"),
            )
        })
        .collect()
}

/// The text of a reply that says something.
fn content(body: &Value) -> &str {
    body["choices"][0]["message"]["content"]
        .as_str()
        .expect("text content")
}

/// The messages of the next round of a tool loop: `messages`, then the
/// assistant message of `reply`, then a result for each of its calls. The
/// result of a call to the tool `name` is `result of name`.
fn next_round(mut messages: Vec<Value>, reply: &Value) -> Vec<Value> {
    let message = reply["choices"][0]["message"].clone();
    let calls = message["tool_calls"]
        .as_array()
        .expect("tool calls")
        .clone();
    messages.push(message);
    for call in calls {
        let name = call["function"]["name"].as_str().expect("a name");
        messages.push(json!({
            "role": "tool",
            "tool_call_id": call["id"],
            "content": format!("result of {name}")
        }));
    }
    messages
}

#[tokio::test]
async fn a_directive_calls_a_tool_with_its_exact_arguments() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&user(
            r#"[[call read_file {"path":"notes.txt","lines":[[1,2],[3,4]]}]]"#,
        ))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        calls(&body),
        [(
            "read_file".to_owned(),
            json!({ "path": "notes.txt", "lines": [[1, 2], [3, 4]] })
        )]
    );
}

#[tokio::test]
async fn a_call_without_arguments_gets_arguments_that_fit_the_schema_of_the_tool() {
    let server = Server::start().await;
    let (_, body) = server.chat(&user("[[call get_weather]]")).await;
    let calls = calls(&body);
    assert_eq!(calls.len(), 1);
    let (name, arguments) = &calls[0];
    assert_eq!(name, "get_weather");
    let validator = jsonschema::validator_for(&weather_parameters()).expect("a valid schema");
    assert!(validator.is_valid(arguments), "{arguments}");
    assert_eq!(
        *arguments,
        json!({ "city": "aa", "days": 1, "unit": "celsius" })
    );
}

#[tokio::test]
async fn one_block_makes_two_calls_in_one_turn() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&user(
            r#"[[call read_file {"path":"a"}; call get_weather {"city":"Oslo"}]]"#,
        ))
        .await;
    assert_eq!(
        calls(&body),
        [
            ("read_file".to_owned(), json!({ "path": "a" })),
            ("get_weather".to_owned(), json!({ "city": "Oslo" }))
        ]
    );
    let ids = &body["choices"][0]["message"]["tool_calls"];
    assert_ne!(ids[0]["id"], ids[1]["id"]);
}

#[tokio::test]
async fn a_three_turn_script_calls_then_calls_then_says() {
    let server = Server::start().await;
    let script = r#"Do the job. [[call read_file {"path":"a.txt"}]] [[call get_weather {"city":"Oslo"}]] [[say All done.]]"#;
    let mut messages = vec![json!({ "role": "user", "content": script })];

    let (_, first) = server.chat(&request(json!(messages), &json!({}))).await;
    assert_eq!(
        calls(&first),
        [("read_file".to_owned(), json!({ "path": "a.txt" }))]
    );

    messages = next_round(messages, &first);
    let (_, second) = server.chat(&request(json!(messages), &json!({}))).await;
    assert_eq!(
        calls(&second),
        [("get_weather".to_owned(), json!({ "city": "Oslo" }))]
    );
    assert_ne!(
        first["choices"][0]["message"]["tool_calls"][0]["id"],
        second["choices"][0]["message"]["tool_calls"][0]["id"],
        "each round has ids of its own"
    );

    messages = next_round(messages, &second);
    let (_, third) = server.chat(&request(json!(messages), &json!({}))).await;
    assert_eq!(content(&third), "All done.");
    assert_eq!(third["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn tool_results_are_quoted_and_the_loop_ends() {
    let server = Server::start().await;
    let messages =
        vec![json!({ "role": "user", "content": "[[call read_file; call get_weather]]" })];
    let (_, first) = server.chat(&request(json!(messages), &json!({}))).await;
    let messages = next_round(messages, &first);
    let (status, second) = server.chat(&request(json!(messages), &json!({}))).await;
    assert_eq!(status, 200);
    assert_eq!(
        content(&second),
        "read_file: result of read_file\nget_weather: result of get_weather"
    );
    assert_eq!(second["choices"][0]["finish_reason"], "stop");
    assert_eq!(second["choices"][0]["message"].get("tool_calls"), None);
}

#[tokio::test]
async fn a_named_tool_choice_forces_that_tool() {
    let server = Server::start().await;
    let choice =
        json!({ "tool_choice": { "type": "function", "function": { "name": "read_file" } } });
    let (_, body) = server
        .chat(&request(
            json!([{ "role": "user", "content": "Hello." }]),
            &choice,
        ))
        .await;
    assert_eq!(
        calls(&body),
        [("read_file".to_owned(), json!({ "path": "a" }))]
    );
}

#[tokio::test]
async fn required_calls_the_tool_the_text_names_or_else_the_first_one() {
    let server = Server::start().await;
    let required = json!({ "tool_choice": "required" });
    let (_, body) = server
        .chat(&request(
            json!([{ "role": "user", "content": "Hello." }]),
            &required,
        ))
        .await;
    assert_eq!(calls(&body)[0].0, "get_weather");

    let (_, body) = server
        .chat(&request(
            json!([{ "role": "user", "content": "Read file a.txt." }]),
            &required,
        ))
        .await;
    assert_eq!(calls(&body)[0].0, "read_file");
}

#[tokio::test]
async fn each_of_the_three_spellings_of_a_tools_name_asks_for_it() {
    let server = Server::start().await;
    for text in [
        "Please run get_weather for Oslo.",
        "Please get weather for Oslo.",
        "Please call getWeather for Oslo.",
    ] {
        let (_, body) = server.chat(&user(text)).await;
        assert_eq!(calls(&body)[0].0, "get_weather", "{text}");
    }
}

#[tokio::test]
async fn text_that_names_no_tool_gets_a_chat_reply_even_with_tools_on_offer() {
    let server = Server::start().await;
    let (_, body) = server.chat(&user("Hi, I'm Sam.")).await;
    assert_eq!(
        content(&body),
        "Nice to meet you, Sam. What can I help with?"
    );
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn a_tool_choice_of_none_leads_to_chat() {
    let server = Server::start().await;
    let none = json!({ "tool_choice": "none" });
    let messages = json!([{ "role": "user", "content": "Please get weather for Oslo." }]);
    let (status, body) = server.chat(&request(messages, &none)).await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert!(!content(&body).is_empty());
    assert_eq!(body["choices"][0]["message"].get("tool_calls"), None);
}

#[tokio::test]
async fn nested_json_arrays_in_user_text_are_plain_text() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&user("Sort the rows [[3,1],[2,4]] and see [[the wiki]]."))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert!(!content(&body).is_empty());
}

#[tokio::test]
async fn a_directive_that_cannot_be_read_gets_a_400() {
    let server = Server::start().await;
    let (status, body) = server.chat(&user(r#"[[call read_file {"path":}]]"#)).await;
    assert_eq!(status, 400);
    let message = body["error"]["message"].as_str().expect("a message");
    assert!(
        message.starts_with("Invalid directive in the last user message: the arguments of the call to `read_file` are not a JSON object"),
        "{message}"
    );
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert_eq!(body["error"]["param"], "messages");
    assert_eq!(body["error"]["code"], "invalid_directive");
}

#[tokio::test]
async fn a_tool_message_that_answers_no_earlier_call_gets_a_400() {
    let server = Server::start().await;
    let messages = json!([
        { "role": "user", "content": "Hello." },
        { "role": "tool", "tool_call_id": "call_0000000000000000", "content": "a result" }
    ]);
    let (status, body) = server.chat(&request(messages, &json!({}))).await;
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Invalid parameter: messages with role 'tool' must be a response to a preceeding message with 'tool_calls'.",
                "type": "invalid_request_error",
                "param": "messages.[1].role",
                "code": null
            }
        })
    );
}

#[tokio::test]
async fn a_tool_call_without_its_result_gets_a_400() {
    let server = Server::start().await;
    let messages =
        vec![json!({ "role": "user", "content": "[[call read_file; call get_weather]]" })];
    let (_, first) = server.chat(&request(json!(messages), &json!({}))).await;
    let mut messages = next_round(messages, &first);
    messages.pop();
    let (status, body) = server.chat(&request(json!(messages), &json!({}))).await;
    assert_eq!(status, 400);
    let message = body["error"]["message"].as_str().expect("a message");
    assert!(
        message.contains("did not have response messages: call_"),
        "{message}"
    );
}

#[tokio::test]
async fn a_tool_choice_without_tools_gets_a_400() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "tool_choice": "auto",
            "messages": [{ "role": "user", "content": "Hello." }]
        }))
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["param"], "tool_choice");
}

#[tokio::test]
async fn a_tool_call_counts_its_name_and_its_arguments_as_completion_tokens() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&user(r#"[[call read_file {"path":"a"}]]"#))
        .await;
    // `read`, `_`, `file` are 3 tokens, and `{"path":"a"}` is 9.
    assert_eq!(body["usage"]["completion_tokens"], 12);
    assert_eq!(
        body["usage"]["total_tokens"].as_u64(),
        Some(
            body["usage"]["prompt_tokens"]
                .as_u64()
                .expect("prompt tokens")
                + 12
        )
    );
}

#[tokio::test]
async fn a_token_limit_cuts_the_arguments_of_a_call_with_length() {
    let server = Server::start().await;
    let mut body = user(r#"[[call read_file {"path":"a"}]]"#);
    body["max_tokens"] = json!(6);
    let (_, body) = server.chat(&body).await;
    assert_eq!(body["choices"][0]["finish_reason"], "length");
    let call = &body["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(call["function"]["name"], "read_file");
    // The name is 3 tokens, and `{`, `"` and `path` are the other 3.
    assert_eq!(call["function"]["arguments"], r#"{"path"#);
    assert_eq!(body["usage"]["completion_tokens"], 6);
}

#[tokio::test]
async fn a_streamed_call_reassembles_to_the_same_name_and_arguments() {
    let server = Server::start().await;
    let text = r#"[[call read_file {"path":"notes.txt"}; call get_weather]]"#;
    let (_, whole) = server.chat(&user(text)).await;
    let expected = calls(&whole);

    let mut streamed = user(text);
    streamed["stream"] = json!(true);
    streamed["stream_options"] = json!({ "include_usage": true });
    let response = client()
        .post(server.url(CHAT))
        .json(&streamed)
        .send()
        .await
        .expect("the server answers");
    let text = response.text().await.expect("a readable stream");
    let chunks: Vec<Value> = text
        .split("\n\n")
        .filter_map(|event| event.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).expect("a JSON chunk"))
        .collect();

    // A reader joins the pieces of each call by its index.
    let mut names = vec![String::new(); 2];
    let mut arguments = vec![String::new(); 2];
    let mut ids = vec![String::new(); 2];
    let mut finish = Value::Null;
    for chunk in &chunks {
        let Some(choice) = chunk["choices"].get(0) else {
            continue;
        };
        assert_eq!(choice["delta"].get("content").and_then(Value::as_str), None);
        if !choice["finish_reason"].is_null() {
            finish = choice["finish_reason"].clone();
        }
        for call in choice["delta"]["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let index =
                usize::try_from(call["index"].as_u64().expect("an index")).expect("a small index");
            ids[index].push_str(call["id"].as_str().unwrap_or_default());
            names[index].push_str(call["function"]["name"].as_str().unwrap_or_default());
            arguments[index].push_str(call["function"]["arguments"].as_str().unwrap_or_default());
        }
    }
    let joined: Vec<(String, Value)> = names
        .into_iter()
        .zip(&arguments)
        .map(|(name, arguments)| {
            (
                name,
                serde_json::from_str(arguments).expect("joined arguments that are JSON"),
            )
        })
        .collect();
    assert_eq!(joined, expected);
    assert_eq!(finish, "tool_calls");
    assert!(
        ids.iter()
            .all(|id| id.starts_with("call_") && id.len() == 21),
        "{ids:?}"
    );
    assert_eq!(
        chunks.last().expect("a usage chunk")["usage"]["completion_tokens"],
        whole["usage"]["completion_tokens"]
    );
}

#[tokio::test]
async fn the_echo_model_shows_the_tools_the_calls_and_the_results() {
    let server = Server::start().await;
    let messages =
        vec![json!({ "role": "user", "content": "[[call read_file {\"path\":\"a\"}]]" })];
    let (_, first) = server.chat(&request(json!(messages), &json!({}))).await;
    let mut echo = request(
        json!(next_round(messages, &first)),
        &json!({ "tool_choice": "auto" }),
    );
    echo["model"] = json!("cheapseek-echo");
    let (status, body) = server.chat(&echo).await;
    assert_eq!(status, 200);
    let lines: Vec<&str> = content(&body).lines().collect();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert!(
        lines[0].starts_with(r#"tools: [{"type":"function","function":{"name":"get_weather","#)
    );
    assert_eq!(lines[1], r#"tool_choice: "auto""#);
    assert_eq!(lines[2], r#"user: [[call read_file {"path":"a"}]]"#);
    assert_eq!(lines[3], r#"assistant: [call read_file {"path":"a"}]"#);
    assert_eq!(lines[4], "tool read_file: result of read_file");
}
