//! Replies in a requested form: JSON that fits a schema, and any JSON
//! object.

use serde_json::{Value, json};

use crate::support::{CHAT, Server, client};

/// The schema of a person, as an application would send it for structured
/// output.
fn person_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer", "minimum": 18 },
            "role": { "type": "string", "enum": ["admin", "member", "guest"] },
            "email": { "type": "string", "format": "email" },
            "tags": { "type": "array", "items": { "type": "string" }, "minItems": 2 },
            "nickname": { "type": "string" }
        },
        "required": ["name", "age", "role", "email", "tags"],
        "additionalProperties": false
    })
}

/// A chat request that asks for a person, with `extra` fields added.
fn person_request(extra: &Value) -> Value {
    let mut body = json!({
        "model": "cheapseek",
        "messages": [{ "role": "user", "content": "Extract the person." }],
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": "person", "strict": true, "schema": person_schema() }
        }
    });
    for (name, value) in extra.as_object().expect("an object of extra fields") {
        body[name] = value.clone();
    }
    body
}

/// The content of the reply, read as JSON.
fn content_json(body: &Value) -> Value {
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .expect("text content");
    serde_json::from_str(content).expect("content that is JSON")
}

/// Fails the test unless `value` fits `schema`, formats included.
fn assert_fits(value: &Value, schema: &Value) {
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .expect("a schema the validator accepts");
    if let Err(error) = validator.validate(value) {
        panic!("{value} does not fit the schema: {error}");
    }
}

#[tokio::test]
async fn a_json_schema_request_gets_json_that_fits_the_schema() {
    let server = Server::start().await;
    let (status, body) = server.chat(&person_request(&json!({}))).await;
    assert_eq!(status, 200);
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    let person = content_json(&body);
    assert_fits(&person, &person_schema());
    assert_eq!(
        person,
        json!({
            "age": 18,
            "email": "user@example.com",
            "name": "a",
            "role": "admin",
            "tags": ["a", "a"]
        })
    );
}

#[tokio::test]
async fn the_seed_of_the_request_picks_among_the_choices_of_the_schema() {
    let server = Server::start().await;
    let (_, body) = server.chat(&person_request(&json!({ "seed": 1 }))).await;
    let person = content_json(&body);
    assert_fits(&person, &person_schema());
    assert_eq!(person["role"], "member");
    assert_eq!(person["age"], 19);
    assert_eq!(person["name"], "b");
}

#[tokio::test]
async fn a_json_schema_request_gets_the_same_json_as_a_stream() {
    let server = Server::start().await;
    let (_, whole) = server.chat(&person_request(&json!({}))).await;
    let response = client()
        .post(server.url(CHAT))
        .json(&person_request(&json!({ "stream": true })))
        .send()
        .await
        .expect("the server answers");
    let text = response.text().await.expect("a readable stream");
    let streamed: String = text
        .split("\n\n")
        .filter_map(|event| event.strip_prefix("data: {"))
        .filter_map(|rest| serde_json::from_str::<Value>(&format!("{{{rest}")).ok())
        .filter_map(|chunk| {
            chunk["choices"][0]["delta"]["content"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        Value::String(streamed.clone()),
        whole["choices"][0]["message"]["content"]
    );
    let person: Value = serde_json::from_str(&streamed).expect("streamed content that is JSON");
    assert_fits(&person, &person_schema());
}

#[tokio::test]
async fn a_json_schema_format_without_a_schema_gets_an_empty_object() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [{ "role": "user", "content": "Anything." }],
            "response_format": { "type": "json_schema", "json_schema": { "name": "anything" } }
        }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(content_json(&body), json!({}));
}

#[tokio::test]
async fn a_json_object_request_gets_the_chat_reply_inside_an_object() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [{ "role": "user", "content": "Hi, I'm Sam." }],
            "response_format": { "type": "json_object" }
        }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        content_json(&body),
        json!({ "reply": "Nice to meet you, Sam. What can I help with?" })
    );
}

#[tokio::test]
async fn a_text_format_is_the_same_as_no_format() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [{ "role": "user", "content": "Hi, I'm Sam." }],
            "response_format": { "type": "text" }
        }))
        .await;
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "Nice to meet you, Sam. What can I help with?"
    );
}

#[tokio::test]
async fn the_token_limit_cuts_json_with_length() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&person_request(&json!({ "max_tokens": 4 })))
        .await;
    assert_eq!(body["choices"][0]["message"]["content"], r#"{"age""#);
    assert_eq!(body["choices"][0]["finish_reason"], "length");
}

#[tokio::test]
async fn the_echo_model_answers_with_the_prompt_whatever_the_format() {
    let server = Server::start().await;
    let mut request = person_request(&json!({}));
    request["model"] = json!("cheapseek-echo");
    let (_, body) = server.chat(&request).await;
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "user: Extract the person."
    );
}

#[tokio::test]
async fn a_format_the_server_cannot_read_gets_a_400() {
    let server = Server::start().await;
    let message = json!([{ "role": "user", "content": "hi" }]);
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": message,
            "response_format": { "type": "json_schema", "json_schema": { "schema": {} } }
        }))
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["param"], "response_format");
    let text = body["error"]["message"].as_str().expect("a message");
    assert!(text.contains("missing field `name`"), "{text}");

    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": message,
            "response_format": { "type": "yaml" }
        }))
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["param"], "response_format");
}
