//! The JSON shapes of the API: what a request holds and what a reply holds.

use serde::Serialize;

/// The Unix time that every reply reports as its creation time:
/// 2026-11-22T00:00:00Z. It is a constant, so the same request always gets
/// the same bytes.
pub(crate) const CREATED: u64 = 1_795_305_600;

/// The owner that every model reports.
const OWNER: &str = "cheapseek";

/// A model the server offers. The model name is how a request picks the way
/// its reply is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Model {
    /// Answers by the reply rules, with chat text from the bot.
    Chat,
    /// Returns the prompt as received.
    Echo,
}

impl Model {
    /// Every model, in the order the model list reports them.
    pub(crate) const ALL: [Model; 2] = [Model::Chat, Model::Echo];

    /// The name a request uses for this model.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Model::Chat => "cheapseek",
            Model::Echo => "cheapseek-echo",
        }
    }

    /// The model that `id` names. Names match exactly, so a misspelled name
    /// finds no model.
    pub(crate) fn from_id(id: &str) -> Option<Model> {
        Model::ALL.into_iter().find(|model| model.id() == id)
    }

    /// The entry the model list holds for this model.
    pub(crate) const fn object(self) -> ModelObject {
        ModelObject {
            id: self.id(),
            object: "model",
            created: CREATED,
            owned_by: OWNER,
        }
    }
}

/// One model, as `GET /v1/models/{id}` returns it.
#[derive(Debug, Serialize)]
pub(crate) struct ModelObject {
    id: &'static str,
    object: &'static str,
    created: u64,
    owned_by: &'static str,
}

/// The reply of `GET /v1/models`.
#[derive(Debug, Serialize)]
pub(crate) struct ModelList {
    object: &'static str,
    data: Vec<ModelObject>,
}

impl ModelList {
    /// The list of every model the server offers.
    pub(crate) fn all() -> ModelList {
        ModelList {
            object: "list",
            data: Model::ALL.into_iter().map(Model::object).collect(),
        }
    }
}

/// The body of every failed request: OpenAI's error envelope.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody {
    error: ErrorDetail,
}

/// The fields inside the error envelope. `param` and `code` are written as
/// `null` when the error names neither, as OpenAI writes them.
#[derive(Debug, Serialize)]
struct ErrorDetail {
    message: String,
    #[serde(rename = "type")]
    kind: &'static str,
    param: Option<&'static str>,
    code: Option<&'static str>,
}

impl ErrorBody {
    /// The envelope of a request the caller got wrong. `param` names the
    /// request field at fault, and `code` is the short machine-readable name
    /// of the failure.
    pub(crate) fn invalid_request(
        message: String,
        param: Option<&'static str>,
        code: Option<&'static str>,
    ) -> ErrorBody {
        ErrorBody {
            error: ErrorDetail {
                message,
                kind: "invalid_request_error",
                param,
                code,
            },
        }
    }
}

#[cfg(test)]
#[path = "wire-tests.rs"]
mod tests;
