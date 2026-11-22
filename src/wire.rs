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

    /// The envelope of a failure inside the server.
    pub(crate) fn server_error(message: String) -> ErrorBody {
        ErrorBody {
            error: ErrorDetail {
                message,
                kind: "server_error",
                param: None,
                code: None,
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
    /// An error with this message. `param` names the request field at
    /// fault, and `code` is the short machine-readable name of the failure.
    pub(crate) fn new(
        message: String,
        param: Option<&str>,
        code: Option<&'static str>,
    ) -> RequestError {
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
    /// What the caller wants added to the stream.
    #[serde(default)]
    stream_options: Option<StreamOptions>,
    /// The form the caller wants the reply text in.
    #[serde(default)]
    pub(crate) response_format: Option<ResponseFormat>,
    /// The tools the model may call.
    #[serde(default)]
    tools: Option<Vec<Tool>>,
    /// Whether the model may, must, or must not call a tool.
    #[serde(default)]
    pub(crate) tool_choice: Option<ToolChoice>,
}

/// One entry of the `tools` field. It is written back out by the echo
/// model, so it keeps the fields a reader of the prompt wants to see.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Tool {
    /// The kind of tool. The reply rules call tools of the kind `function`.
    #[serde(rename = "type")]
    kind: String,
    /// The function, when the tool is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    function: Option<FunctionTool>,
}

/// The function of a tool: what a model reads to decide on a call.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct FunctionTool {
    /// The name a call uses.
    pub(crate) name: String,
    /// What the function does, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// The JSON Schema of the arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parameters: Option<Value>,
}

/// The `tool_choice` field.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum ToolChoice {
    /// One of the three words.
    Mode(ToolMode),
    /// One function the model must call.
    Function {
        /// Always `function`.
        #[serde(rename = "type")]
        kind: String,
        /// The function to call.
        function: FunctionName,
    },
}

/// The words a `tool_choice` can be.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolMode {
    /// The model must not call a tool.
    None,
    /// The model may call a tool. This is what an absent field means.
    Auto,
    /// The model must call a tool.
    Required,
}

/// The name of the one function a `tool_choice` forces.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct FunctionName {
    /// The name of the function.
    pub(crate) name: String,
}

/// One tool call, as a request holds it in an assistant message and as a
/// reply holds it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct ToolCall {
    /// The id that the tool message with the result names.
    pub(crate) id: String,
    /// Always `function`.
    #[serde(rename = "type", default = "function_kind")]
    kind: String,
    /// The function and its arguments.
    pub(crate) function: FunctionCall,
}

impl ToolCall {
    /// A call to the function `name` with `arguments`, a JSON object as
    /// text.
    pub(crate) fn new(id: String, name: String, arguments: String) -> ToolCall {
        ToolCall {
            id,
            kind: function_kind(),
            function: FunctionCall { name, arguments },
        }
    }
}

/// The kind of every tool call the server reads or writes.
fn function_kind() -> String {
    "function".to_owned()
}

/// The function a tool call names, and its arguments.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct FunctionCall {
    /// The name of the function.
    pub(crate) name: String,
    /// The arguments: a JSON object, written as text.
    pub(crate) arguments: String,
}

/// The `response_format` field: the form the reply text takes.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ResponseFormat {
    /// Plain text, the same as no `response_format`.
    Text,
    /// Any JSON object.
    JsonObject,
    /// JSON that fits a schema.
    JsonSchema {
        /// The schema and its name.
        json_schema: JsonSchemaFormat,
    },
}

/// The `json_schema` field of a `response_format`.
#[derive(Debug, Deserialize)]
pub(crate) struct JsonSchemaFormat {
    /// The name of the format. OpenAI requires it, and the server reads it
    /// only to require it too.
    #[serde(rename = "name")]
    _name: String,
    /// The schema the reply must fit. A format without one allows any JSON.
    #[serde(default)]
    pub(crate) schema: Option<Value>,
}

/// The `stream_options` field.
#[derive(Debug, Deserialize)]
struct StreamOptions {
    /// Whether the stream ends with a chunk that holds the usage.
    #[serde(default)]
    include_usage: Option<bool>,
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
        if self.tool_choice.is_some() && self.tools().is_empty() {
            return Err(RequestError::new(
                "Invalid parameter: 'tool_choice' is only allowed when 'tools' are specified."
                    .to_owned(),
                Some("tool_choice"),
                None,
            ));
        }
        if self.stream_options.is_some() && !self.streams() {
            return Err(RequestError::new(
                "The 'stream_options' parameter is only allowed when 'stream' is enabled."
                    .to_owned(),
                Some("stream_options"),
                None,
            ));
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

    /// Whether the caller wants the reply as a stream of server-sent events.
    pub(crate) fn streams(&self) -> bool {
        self.stream == Some(true)
    }

    /// Whether the stream ends with a chunk that holds the usage.
    pub(crate) fn includes_usage(&self) -> bool {
        self.stream_options
            .as_ref()
            .and_then(|options| options.include_usage)
            == Some(true)
    }

    /// The tools the request offers, as it wrote them.
    pub(crate) fn tools(&self) -> &[Tool] {
        self.tools.as_deref().unwrap_or_default()
    }

    /// The functions among the tools, in the order the request names them.
    pub(crate) fn functions(&self) -> impl Iterator<Item = &FunctionTool> {
        self.tools()
            .iter()
            .filter_map(|tool| tool.function.as_ref())
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
    /// The tool calls of an assistant message.
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
    /// The id of the call that a tool message answers.
    #[serde(default)]
    pub(crate) tool_call_id: Option<String>,
    /// The name of the function that a `function` message answers.
    #[serde(default)]
    pub(crate) name: Option<String>,
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

    /// The tool calls of the message. Only an assistant message has any.
    pub(crate) fn tool_calls(&self) -> &[ToolCall] {
        self.tool_calls.as_deref().unwrap_or_default()
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
        message: AssistantMessage,
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
                message,
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

/// The message the model wrote: text, or tool calls. `refusal` is always
/// `null`.
#[derive(Debug, Serialize)]
pub(crate) struct AssistantMessage {
    role: &'static str,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    refusal: Option<()>,
}

impl AssistantMessage {
    /// A message that says `content`.
    pub(crate) const fn text(content: String) -> AssistantMessage {
        AssistantMessage {
            role: "assistant",
            content: Some(content),
            tool_calls: None,
            refusal: None,
        }
    }

    /// A message that calls tools and says nothing, so its content is
    /// `null`.
    pub(crate) const fn calls(tool_calls: Vec<ToolCall>) -> AssistantMessage {
        AssistantMessage {
            role: "assistant",
            content: None,
            tool_calls: Some(tool_calls),
            refusal: None,
        }
    }
}

/// Why the reply ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FinishReason {
    /// The reply is whole, or a stop sequence ended it.
    Stop,
    /// The token limit cut the reply.
    Length,
    /// The reply is tool calls, and the caller runs them next.
    ToolCalls,
}

/// One chunk of a streamed reply: the JSON of one server-sent event.
#[derive(Debug, Serialize)]
pub(crate) struct ChatChunk<'a> {
    id: &'a str,
    object: &'static str,
    created: u64,
    model: &'a str,
    system_fingerprint: &'static str,
    choices: Vec<ChunkChoice<'a>>,
    #[serde(skip_serializing_if = "ChunkUsage::is_absent")]
    usage: ChunkUsage,
}

/// The `usage` field of a chunk, as OpenAI writes it: left out unless the
/// request asked for the usage, then `null` in every chunk but the last.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChunkUsage {
    /// The request did not ask for the usage, so the field is left out.
    Absent,
    /// The usage comes in a later chunk. Written as `null`.
    Pending,
    /// The usage of the whole reply.
    Counts(Usage),
}

impl ChunkUsage {
    const fn is_absent(&self) -> bool {
        matches!(self, ChunkUsage::Absent)
    }
}

impl<'a> ChatChunk<'a> {
    /// A chunk of the reply `id` from `model` that holds `delta`, and the
    /// finish reason when the chunk is the one that ends the choice.
    /// `with_usage` says whether the request asked for the usage.
    pub(crate) fn new(
        id: &'a str,
        model: &'a str,
        delta: Delta<'a>,
        finish_reason: Option<FinishReason>,
        with_usage: bool,
    ) -> ChatChunk<'a> {
        ChatChunk {
            id,
            object: "chat.completion.chunk",
            created: CREATED,
            model,
            system_fingerprint: FINGERPRINT,
            choices: vec![ChunkChoice {
                index: 0,
                delta,
                logprobs: None,
                finish_reason,
            }],
            usage: if with_usage {
                ChunkUsage::Pending
            } else {
                ChunkUsage::Absent
            },
        }
    }

    /// The chunk that ends a stream whose request asked for the usage: no
    /// choices, and the usage of the whole reply.
    pub(crate) fn usage(id: &'a str, model: &'a str, usage: Usage) -> ChatChunk<'a> {
        ChatChunk {
            id,
            object: "chat.completion.chunk",
            created: CREATED,
            model,
            system_fingerprint: FINGERPRINT,
            choices: Vec::new(),
            usage: ChunkUsage::Counts(usage),
        }
    }
}

/// The one choice of a chunk. `finish_reason` is `null` until the chunk that
/// ends the choice.
#[derive(Debug, Serialize)]
struct ChunkChoice<'a> {
    index: u32,
    delta: Delta<'a>,
    logprobs: Option<()>,
    finish_reason: Option<FinishReason>,
}

/// What one chunk adds to the message.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum Delta<'a> {
    /// The first chunk: the role, and the content to add to.
    Role {
        /// Always `assistant`.
        role: &'static str,
        /// Empty text before text, and `null` before tool calls.
        content: Option<&'static str>,
        /// Always `null`.
        refusal: Option<()>,
    },
    /// One token of the text.
    Content {
        /// The token.
        content: &'a str,
    },
    /// The start of one tool call, or one more piece of its arguments.
    Call {
        /// The one call this chunk adds to.
        tool_calls: [CallDelta<'a>; 1],
    },
    /// Nothing: the chunk only ends the choice.
    Empty {},
}

/// What one chunk adds to one tool call. The first chunk of a call holds
/// its id, its kind and its name. Every later chunk holds one piece of the
/// arguments, and a reader joins the pieces.
#[derive(Debug, Serialize)]
pub(crate) struct CallDelta<'a> {
    index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    function: FunctionDelta<'a>,
}

/// The function part of a [`CallDelta`].
#[derive(Debug, Serialize)]
struct FunctionDelta<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    arguments: &'a str,
}

impl<'a> Delta<'a> {
    /// The delta of the first chunk of a stream. `text` says whether text
    /// follows: its content then starts empty, and before tool calls it is
    /// `null`.
    pub(crate) const fn role(text: bool) -> Delta<'static> {
        Delta::Role {
            role: "assistant",
            content: if text { Some("") } else { None },
            refusal: None,
        }
    }

    /// The delta that starts the tool call at `index`: its id and its name,
    /// with arguments to add to.
    pub(crate) const fn call_start(index: u32, id: &'a str, name: &'a str) -> Delta<'a> {
        Delta::Call {
            tool_calls: [CallDelta {
                index,
                id: Some(id),
                kind: Some("function"),
                function: FunctionDelta {
                    name: Some(name),
                    arguments: "",
                },
            }],
        }
    }

    /// The delta that adds one piece to the arguments of the tool call at
    /// `index`.
    pub(crate) const fn call_piece(index: u32, arguments: &'a str) -> Delta<'a> {
        Delta::Call {
            tool_calls: [CallDelta {
                index,
                id: None,
                kind: None,
                function: FunctionDelta {
                    name: None,
                    arguments,
                },
            }],
        }
    }
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
