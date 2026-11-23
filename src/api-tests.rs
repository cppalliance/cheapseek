use serde_json::{Value, json};

use super::*;

/// The status and the JSON body of the response that `error` becomes.
async fn response_of(error: ApiError) -> (StatusCode, Value) {
    let response = error.into_response();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body that is in memory");
    let body = serde_json::from_slice(&bytes).expect("a JSON body");
    (status, body)
}

#[tokio::test]
async fn a_reply_that_the_server_cannot_write_gets_a_500_with_a_server_error() {
    // Every reply of the server can be written as JSON, so no request
    // reaches this error. A failed parse stands in for the failed write.
    let cause = serde_json::from_str::<Value>("{").expect_err("an object that never ends");
    let (status, body) = response_of(ApiError::internal(&cause)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body,
        json!({
            "error": {
                "message": format!("The server could not write its reply: {cause}."),
                "type": "server_error",
                "param": null,
                "code": null
            }
        })
    );
}

#[tokio::test]
async fn a_request_error_becomes_a_400_with_its_param_and_code() {
    let error = RequestError {
        message: "Missing required parameter: 'messages'.".to_owned(),
        param: Some("messages".to_owned()),
        code: Some("missing_required_parameter"),
    };
    let (status, body) = response_of(ApiError::from(error)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
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
