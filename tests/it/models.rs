//! The model routes: the list, and one model by its name.

use serde_json::json;

use crate::support::Server;

#[tokio::test]
async fn the_model_list_holds_both_models_in_the_openai_shape() {
    let server = Server::start().await;
    let (status, body) = server.get("/v1/models").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({
            "object": "list",
            "data": [
                {
                    "id": "cheapseek",
                    "object": "model",
                    "created": 1_795_305_600,
                    "owned_by": "cheapseek"
                },
                {
                    "id": "cheapseek-echo",
                    "object": "model",
                    "created": 1_795_305_600,
                    "owned_by": "cheapseek"
                }
            ]
        })
    );
}

#[tokio::test]
async fn one_model_is_fetched_by_its_name() {
    let server = Server::start().await;
    let (status, body) = server.get("/v1/models/cheapseek-echo").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({
            "id": "cheapseek-echo",
            "object": "model",
            "created": 1_795_305_600,
            "owned_by": "cheapseek"
        })
    );
}

#[tokio::test]
async fn a_model_name_the_server_lacks_gets_model_not_found() {
    let server = Server::start().await;
    let (status, body) = server.get("/v1/models/gpt-4o").await;
    assert_eq!(status, 404);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "The model `gpt-4o` does not exist.",
                "type": "invalid_request_error",
                "param": "model",
                "code": "model_not_found"
            }
        })
    );
}

#[tokio::test]
async fn a_bearer_key_is_accepted_and_ignored() {
    let server = Server::start().await;
    let response = crate::support::client()
        .get(server.url("/v1/models"))
        .bearer_auth("sk-any-key-at-all")
        .send()
        .await
        .expect("the server answers");
    assert_eq!(response.status().as_u16(), 200);
}
