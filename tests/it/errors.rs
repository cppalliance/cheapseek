//! Requests that reach no handler: each one gets OpenAI's error envelope.

use serde_json::json;

use crate::support::Server;

#[tokio::test]
async fn an_unknown_path_gets_a_404_in_the_openai_error_envelope() {
    let server = Server::start().await;
    let (status, body) = server.get("/v1/nothing-here").await;
    assert_eq!(status, 404);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Invalid URL (GET /v1/nothing-here)",
                "type": "invalid_request_error",
                "param": null,
                "code": null
            }
        })
    );
}

#[tokio::test]
async fn a_path_outside_v1_gets_the_same_envelope() {
    let server = Server::start().await;
    let (status, body) = server.post("/models", &json!({})).await;
    assert_eq!(status, 404);
    assert_eq!(
        body["error"]["message"],
        json!("Invalid URL (POST /models)")
    );
    assert_eq!(body["error"]["type"], json!("invalid_request_error"));
}

#[tokio::test]
async fn the_wrong_method_on_a_known_path_gets_a_405_in_the_envelope() {
    let server = Server::start().await;
    let (status, body) = server.post("/v1/models", &json!({})).await;
    assert_eq!(status, 405);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Not allowed to POST on /v1/models. (HINT: Perhaps you meant to use a different HTTP method?)",
                "type": "invalid_request_error",
                "param": null,
                "code": null
            }
        })
    );
}
