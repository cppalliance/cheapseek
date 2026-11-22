//! Chat requests the server refuses: each one gets a status in the 400s and
//! OpenAI's error envelope.

use serde_json::{Value, json};

use crate::support::Server;

/// Checks that `body` is the error envelope and returns its four fields.
fn envelope(body: &Value) -> (&str, &Value, &Value) {
    let error = body["error"].as_object().expect("an error envelope");
    assert_eq!(error.len(), 4, "{body}");
    assert_eq!(error["type"], "invalid_request_error");
    let message = error["message"].as_str().expect("a message");
    (message, &error["param"], &error["code"])
}

#[tokio::test]
async fn a_body_that_is_not_json_gets_a_400() {
    let server = Server::start().await;
    let (status, bytes) = server.chat_bytes("{\"model\": \"cheapseek\", ").await;
    assert_eq!(status, 400);
    let body: Value = serde_json::from_slice(&bytes).expect("a JSON error body");
    let (message, param, code) = envelope(&body);
    assert!(
        message.starts_with("We could not parse the JSON body of your request."),
        "{message}"
    );
    assert_eq!((param, code), (&Value::Null, &Value::Null));
}

#[tokio::test]
async fn a_request_without_messages_gets_a_400_that_names_the_field() {
    let server = Server::start().await;
    let (status, body) = server.chat(&json!({ "model": "cheapseek" })).await;
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Missing required parameter: 'messages'.",
                "type": "invalid_request_error",
                "param": "messages",
                "code": "missing_required_parameter"
            }
        })
    );
}

#[tokio::test]
async fn a_request_without_a_model_gets_a_400() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({ "messages": [{ "role": "user", "content": "hi" }] }))
        .await;
    assert_eq!(status, 400);
    let (message, ..) = envelope(&body);
    assert_eq!(message, "you must provide a model parameter");
}

#[tokio::test]
async fn a_model_name_the_server_lacks_gets_a_404_with_model_not_found() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek-ecko",
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 404);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "The model `cheapseek-ecko` does not exist.",
                "type": "invalid_request_error",
                "param": "model",
                "code": "model_not_found"
            }
        })
    );
}

#[tokio::test]
async fn more_than_one_choice_gets_a_400() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "n": 2,
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 400);
    let (message, param, code) = envelope(&body);
    assert_eq!(
        message,
        "Unsupported value: 'n' does not support 2 with this model."
    );
    assert_eq!((param, code), (&json!("n"), &json!("unsupported_value")));
}

#[tokio::test]
async fn a_field_of_the_wrong_shape_gets_a_400_that_names_its_path() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "messages": [{ "role": "user", "content": "hi" }, { "role": "robot", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 400);
    let (message, param, code) = envelope(&body);
    assert!(
        message.starts_with("Invalid 'messages[1].role': unknown variant `robot`"),
        "{message}"
    );
    assert_eq!((param, code), (&json!("messages"), &json!("invalid_value")));
}

#[tokio::test]
async fn a_token_limit_of_zero_gets_a_400() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek",
            "max_tokens": 0,
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;
    assert_eq!(status, 400);
    let (_, param, code) = envelope(&body);
    assert_eq!(
        (param, code),
        (&json!("max_tokens"), &json!("integer_below_min_value"))
    );
}

#[tokio::test]
async fn a_get_on_the_chat_route_gets_a_405() {
    let server = Server::start().await;
    let (status, body) = server.get("/v1/chat/completions").await;
    assert_eq!(status, 405);
    let (message, ..) = envelope(&body);
    assert!(
        message.starts_with("Not allowed to GET on /v1/chat/completions."),
        "{message}"
    );
}
