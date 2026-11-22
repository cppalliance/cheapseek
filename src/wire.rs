//! The JSON shapes of the API: what a request holds and what a reply holds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The Unix time that every reply reports as its creation time:
/// 2026-11-22T00:00:00Z. It is a constant, so the same request always gets
/// the same bytes.
pub(crate) const CREATED: u64 = 1_795_305_600;

/// The owner that every model reports.
const OWNER: &str = "cheapseek";

/// The fingerprint that every chat completion reports. A real backend
/// changes its fingerprint when its configuration changes. This one has a
/// single configuration.
const FINGERPRINT: &str = "fp_cheapseek";

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
    param: Option<String>,
    code: Option<&'static str>,
}

impl ErrorBody {
    /// The envelope of a request the caller got wrong. `param` names the
    /// request field at fault, and `code` is the short machine-readable name
    /// of the failure.
    pub(crate) fn invalid_request(
        message: String,
        param: Option<String>,
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

/// What is wrong with the body of a chat request. The API answers it with a
/// 400 and the error envelope.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RequestError {
    /// The sentence the caller reads.
    pub(crate) message: String,
    /// The request field at fault, when one field is.
    pub(crate) param: Option<String>,
    /// The short machine-readable name of the failure.
    pub(crate) code: Option<&'static str>,
}

impl RequestError {
    fn new(message: String, param: Option<&str>, code: Option<&'static str>) -> RequestError {
        RequestError {
            message,
            param: param.map(str::to_owned),
            code,
        }
    }
}

/// The body of `POST /v1/chat/completions`. A field the server has no use
/// for, such as `temperature`, is accepted and ignored.
#[derive(Debug, Deserialize)]
pub(crate) struct ChatRequest {
    /// The model name. It picks the way the reply is made.
    pub(crate) model: String,
    /// The conversation so far.
    pub(crate) messages: Vec<Message>,
    /// The most tokens the reply may hold. This is the older name of
    /// `max_completion_tokens`.
    #[serde(default)]
    max_tokens: Option<u64>,
    /// The most tokens the reply may hold.
    #[serde(default)]
    max_completion_tokens: Option<u64>,
    /// The texts that end the reply where one of them would begin.
    #[serde(default)]
    stop: Option<Stop>,
    /// The seed for the bot's choice of wording.
    #[serde(default)]
    seed: Option<i64>,
    /// How many choices the caller wants. The server returns one.
    #[serde(default)]
    n: Option<u64>,
    /// Whether the caller wants the reply as a stream.
    #[serde(default)]
    stream: Option<bool>,
}

/// The most stop sequences a request may name, as OpenAI limits them.
const MAX_STOPS: usize = 4;

impl ChatRequest {
    /// Reads and checks the body of a chat request.
    ///
    /// # Errors
    /// Returns what is wrong when the body is not a JSON object, lacks
    /// `model` or `messages`, holds a field of the wrong shape, or asks for
    /// something the server refuses.
    pub(crate) fn parse(body: &[u8]) -> Result<ChatRequest, RequestError> {
        let value: Value = serde_json::from_slice(body).map_err(|error| unparsable(&error))?;
        let Value::Object(fields) = &value else {
            return Err(RequestError::new(
                "We could not parse the JSON body of your request. (HINT: The body must be a JSON object.)".to_owned(),
                None,
                None,
            ));
        };
        if !fields.contains_key("model") {
            return Err(RequestError::new(
                "you must provide a model parameter".to_owned(),
                None,
                None,
            ));
        }
        if !fields.contains_key("messages") {
            return Err(RequestError::new(
                "Missing required parameter: 'messages'.".to_owned(),
                Some("messages"),
                Some("missing_required_parameter"),
            ));
        }
        let request: ChatRequest =
            serde_path_to_error::deserialize(value).map_err(|error| wrong_shape(&error))?;
        request.check()?;
        Ok(request)
    }

    /// The checks that need the whole request.
    fn check(&self) -> Result<(), RequestError> {
        if self.messages.is_empty() {
            return Err(RequestError::new(
                "Invalid 'messages': empty array. Expected an array with minimum length 1, but got an empty array instead.".to_owned(),
                Some("messages"),
                Some("empty_array"),
            ));
        }
        for (name, limit) in [
            ("max_tokens", self.max_tokens),
            ("max_completion_tokens", self.max_completion_tokens),
            ("n", self.n),
        ] {
            if limit == Some(0) {
                return Err(RequestError::new(
                    format!(
                        "Invalid '{name}': integer below minimum value. Expected a value >= 1, but got 0 instead."
                    ),
                    Some(name),
                    Some("integer_below_min_value"),
                ));
            }
        }
        if let Some(n) = self.n.filter(|n| *n > 1) {
            return Err(unsupported("n", &n.to_string()));
        }
        if self.stream == Some(true) {
            return Err(unsupported("stream", "true"));
        }
        let stops = self.stops().len();
        if stops > MAX_STOPS {
            return Err(RequestError::new(
                format!(
                    "Invalid 'stop': array too long. Expected an array with maximum length {MAX_STOPS}, but got an array with length {stops} instead."
                ),
                Some("stop"),
                Some("array_above_max_length"),
            ));
        }
        Ok(())
    }

    /// The most tokens the reply may hold, when the request sets a limit.
    /// `max_completion_tokens` wins over its older name.
    pub(crate) fn token_limit(&self) -> Option<u64> {
        self.max_completion_tokens.or(self.max_tokens)
    }

    /// The stop sequences, in the order the request names them. An empty
    /// sequence stops nothing and is left out.
    pub(crate) fn stops(&self) -> Vec<&str> {
        let all: Vec<&str> = match &self.stop {
            None => Vec::new(),
            Some(Stop::One(stop)) => vec![stop.as_str()],
            Some(Stop::Many(stops)) => stops.iter().map(String::as_str).collect(),
        };
        all.into_iter().filter(|stop| !stop.is_empty()).collect()
    }

    /// The seed for the bot's choice of wording. A request that names none
    /// gets seed 0, so it is as repeatable as one that does.
    pub(crate) fn seed(&self) -> i64 {
        self.seed.unwrap_or(0)
    }
}

/// The error for a body that is not JSON.
fn unparsable(error: &serde_json::Error) -> RequestError {
    RequestError::new(
        format!("We could not parse the JSON body of your request. (HINT: {error}.)"),
        None,
        None,
    )
}

/// The error for a field whose value has the wrong shape. The path to the
/// field goes in the message, and the top-level field goes in `param`.
fn wrong_shape(error: &serde_path_to_error::Error<serde_json::Error>) -> RequestError {
    let path = error.path().to_string();
    let reason = error.inner().to_string();
    let code = if reason.starts_with("invalid type") {
        "invalid_type"
    } else {
        "invalid_value"
    };
    let top = path
        .split(['.', '['])
        .next()
        .filter(|name| !name.is_empty() && *name != "?");
    RequestError::new(format!("Invalid '{path}': {reason}."), top, Some(code))
}

/// The error for a value that OpenAI accepts and this server refuses.
fn unsupported(name: &str, value: &str) -> RequestError {
    RequestError::new(
        format!("Unsupported value: '{name}' does not support {value} with this model."),
        Some(name),
        Some("unsupported_value"),
    )
}

/// The `stop` field: one text, or a list of them.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Stop {
    One(String),
    Many(Vec<String>),
}

/// One message of the conversation.
#[derive(Debug, Deserialize)]
pub(crate) struct Message {
    /// Who wrote the message.
    pub(crate) role: Role,
    /// What the message says. An assistant message that only calls tools
    /// has none.
    #[serde(default)]
    content: Option<Content>,
}

impl Message {
    /// The text of the message: its string, or the text of each of its
    /// parts on a line of its own. A part that is not text shows as its
    /// type in square brackets, such as `[image_url]`.
    pub(crate) fn text(&self) -> String {
        match &self.content {
            None => String::new(),
            Some(Content::Text(text)) => text.clone(),
            Some(Content::Parts(parts)) => {
                parts.iter().map(Part::text).collect::<Vec<_>>().join("\n")
            }
        }
    }
}

/// Who wrote a message.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    /// Instructions from the application.
    System,
    /// Instructions from the application, under the newer name.
    Developer,
    /// The person, or the harness that speaks for one.
    User,
    /// The model.
    Assistant,
    /// The result of a tool call.
    Tool,
    /// The result of a function call, the older form of a tool call.
    Function,
}

impl Role {
    /// The name of the role, as a request writes it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::Developer => "developer",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::Function => "function",
        }
    }
}

/// The `content` of a message: one string, or a list of parts.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<Part>),
}

/// One part of a message's content. Only a text part has words the reply
/// rules read. Any other part is known by its type alone.
#[derive(Debug, Deserialize)]
struct Part {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

impl Part {
    fn text(&self) -> String {
        match &self.text {
            Some(text) if self.kind == "text" => text.clone(),
            _ => format!("[{}]", self.kind),
        }
    }
}

/// The reply of `POST /v1/chat/completions`, as one JSON object.
#[derive(Debug, Serialize)]
pub(crate) struct ChatCompletion {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: [Choice; 1],
    usage: Usage,
    system_fingerprint: &'static str,
}

impl ChatCompletion {
    /// The completion that holds one assistant message.
    pub(crate) fn new(
        id: String,
        model: String,
        content: String,
        finish_reason: FinishReason,
        usage: Usage,
    ) -> ChatCompletion {
        ChatCompletion {
            id,
            object: "chat.completion",
            created: CREATED,
            model,
            choices: [Choice {
                index: 0,
                message: AssistantMessage {
                    role: "assistant",
                    content: Some(content),
                    refusal: None,
                },
                logprobs: None,
                finish_reason,
            }],
            usage,
            system_fingerprint: FINGERPRINT,
        }
    }
}

/// The one choice of a completion. `logprobs` is always `null`: the server
/// has no probabilities to report.
#[derive(Debug, Serialize)]
struct Choice {
    index: u32,
    message: AssistantMessage,
    logprobs: Option<()>,
    finish_reason: FinishReason,
}

/// The message the model wrote. `refusal` is always `null`.
#[derive(Debug, Serialize)]
struct AssistantMessage {
    role: &'static str,
    content: Option<String>,
    refusal: Option<()>,
}

/// Why the reply ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FinishReason {
    /// The reply is whole, or a stop sequence ended it.
    Stop,
    /// The token limit cut the reply.
    Length,
}

/// The token counts of a request and its reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the ones OpenAI writes"
)]
pub(crate) struct Usage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

impl Usage {
    /// The usage of a prompt and a reply of these sizes.
    pub(crate) const fn new(prompt_tokens: u64, completion_tokens: u64) -> Usage {
        Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

#[cfg(test)]
#[path = "wire-tests.rs"]
mod tests;
