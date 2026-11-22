//! The routes of the API and the handlers behind them.

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::reply;
use crate::wire::{
    ChatCompletion, ChatRequest, ErrorBody, Model, ModelList, ModelObject, RequestError,
};

/// The largest request body the server reads, in bytes. A test of a long
/// conversation sends megabytes of history, so the limit sits far above the
/// 2 MiB that the HTTP library allows by default.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Builds the router: the model routes, the chat route, and an OpenAI error
/// envelope for every request that reaches no handler.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/models/{id}", get(get_model))
        .route("/v1/chat/completions", post(chat_completions))
        .fallback(unknown_path)
        .method_not_allowed_fallback(wrong_method)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

/// A failed request: the HTTP status and the error envelope that goes with
/// it.
#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    /// The request names a model the server lacks.
    fn model_not_found(id: &str) -> ApiError {
        ApiError {
            status: StatusCode::NOT_FOUND,
            body: ErrorBody::invalid_request(
                format!("The model `{id}` does not exist."),
                Some("model".to_owned()),
                Some("model_not_found"),
            ),
        }
    }

    /// The path is outside the API.
    fn invalid_url(method: &Method, uri: &Uri) -> ApiError {
        ApiError {
            status: StatusCode::NOT_FOUND,
            body: ErrorBody::invalid_request(
                format!("Invalid URL ({method} {})", uri.path()),
                None,
                None,
            ),
        }
    }

    /// The path is in the API, and the method is the wrong one for it.
    fn method_not_allowed(method: &Method, uri: &Uri) -> ApiError {
        ApiError {
            status: StatusCode::METHOD_NOT_ALLOWED,
            body: ErrorBody::invalid_request(
                format!(
                    "Not allowed to {method} on {}. (HINT: Perhaps you meant to use a different HTTP method?)",
                    uri.path()
                ),
                None,
                None,
            ),
        }
    }

    /// The body could not be read: it is larger than the limit, or the
    /// connection failed while it arrived.
    fn unreadable_body(rejection: &BytesRejection) -> ApiError {
        ApiError {
            status: rejection.status(),
            body: ErrorBody::invalid_request(
                format!(
                    "The request body could not be read: {}.",
                    rejection.body_text()
                ),
                None,
                None,
            ),
        }
    }
}

impl From<RequestError> for ApiError {
    fn from(error: RequestError) -> ApiError {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody::invalid_request(error.message, error.param, error.code),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

async fn list_models() -> Json<ModelList> {
    Json(ModelList::all())
}

async fn get_model(Path(id): Path<String>) -> Result<Json<ModelObject>, ApiError> {
    Model::from_id(&id)
        .map(|model| Json(model.object()))
        .ok_or_else(|| ApiError::model_not_found(&id))
}

/// Answers a chat request. The body is read as plain bytes, so that a body
/// the server cannot parse gets the error envelope like every other failure.
async fn chat_completions(
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<ChatCompletion>, ApiError> {
    let body = body.map_err(|rejection| ApiError::unreadable_body(&rejection))?;
    let request = ChatRequest::parse(&body)?;
    let model =
        Model::from_id(&request.model).ok_or_else(|| ApiError::model_not_found(&request.model))?;
    let reply = reply::reply(&request, model, &body).await;
    Ok(Json(reply.into_completion()))
}

async fn unknown_path(method: Method, uri: Uri) -> ApiError {
    ApiError::invalid_url(&method, &uri)
}

async fn wrong_method(method: Method, uri: Uri) -> ApiError {
    ApiError::method_not_allowed(&method, &uri)
}
