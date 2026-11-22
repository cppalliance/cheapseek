//! The routes of the API and the handlers behind them.

use axum::extract::Path;
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::wire::{ErrorBody, Model, ModelList, ModelObject};

/// Builds the router: the model routes, and an OpenAI error envelope for
/// every request that reaches no handler.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/models/{id}", get(get_model))
        .fallback(unknown_path)
        .method_not_allowed_fallback(wrong_method)
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
    pub(crate) fn model_not_found(id: &str) -> ApiError {
        ApiError {
            status: StatusCode::NOT_FOUND,
            body: ErrorBody::invalid_request(
                format!("The model `{id}` does not exist."),
                Some("model"),
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

async fn unknown_path(method: Method, uri: Uri) -> ApiError {
    ApiError::invalid_url(&method, &uri)
}

async fn wrong_method(method: Method, uri: Uri) -> ApiError {
    ApiError::method_not_allowed(&method, &uri)
}
