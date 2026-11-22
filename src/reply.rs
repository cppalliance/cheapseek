//! The reply rules: how a request becomes a reply.
//!
//! A reply is computed from the request alone. The model name picks the way
//! it is made. The echo model returns the prompt as received. The chat
//! model takes the first rule that fits:
//!
//! 1. **Script step.** The last user message holds directives, and one of
//!    them is for this turn: the model says or calls what it tells.
//! 2. **Tool results.** The request ends with tool results: the model
//!    quotes them, so a tool loop ends.
//! 3. **Tool asked for.** The request forces a tool, or the last user
//!    message names one: the model calls it.
//! 4. **JSON wanted.** The request asks for a JSON form: the model writes
//!    it.
//! 5. **Chat.** The bot answers the user turns.
//!
//! The stop sequences and the token limit then shorten the reply, and the
//! token rule counts what is left.

use serde_json::{Map, Value, json};

use std::sync::Arc;

use crate::brain::Brain;
use crate::directive::{self, Script, Step};
use crate::wire::{
    AssistantMessage, ChatCompletion, ChatRequest, FinishReason, FunctionTool, Message, Model,
    RequestError, ResponseFormat, Role, ToolCall, ToolChoice, ToolMode, Usage,
};
use crate::{schema, tokens};

/// What the server answers to one chat request, before it is written out as
/// one JSON object or as a stream.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    /// The id of the completion, the same in every chunk of a stream.
    pub(crate) id: String,
    /// The model name, as the request wrote it.
    pub(crate) model: String,
    /// What the assistant message holds, already shortened.
    pub(crate) output: Output,
    /// Why the reply ends where it does.
    pub(crate) finish: FinishReason,
    /// The token counts of the prompt and of the output.
    pub(crate) usage: Usage,
}

/// What the assistant message of a reply holds.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// Text.
    Text(String),
    /// Tool calls, which the caller runs.
    Calls(Vec<ToolCall>),
}

impl Output {
    /// The tokens of the output. A tool call counts the tokens of its name
    /// and of its arguments.
    fn tokens(&self) -> u64 {
        match self {
            Output::Text(text) => tokens::count(text),
            Output::Calls(calls) => calls.iter().map(call_tokens).sum(),
        }
    }
}

/// The tokens of one tool call: those of its name and of its arguments.
fn call_tokens(call: &ToolCall) -> u64 {
    tokens::count(&call.function.name) + tokens::count(&call.function.arguments)
}

impl Reply {
    /// The reply as the one JSON object of a request that does not stream.
    pub(crate) fn into_completion(self) -> ChatCompletion {
        let message = match self.output {
            Output::Text(text) => AssistantMessage::text(text),
            Output::Calls(calls) => AssistantMessage::calls(calls),
        };
        ChatCompletion::new(self.id, self.model, message, self.finish, self.usage)
    }
}

/// Makes the reply to `request`, which names `model`. `body` is the request
/// as it arrived, and the ids of the reply are hashes of it.
///
/// # Errors
/// Returns what is wrong when a tool message and a tool call do not pair
/// up, or when a directive in the last user message cannot be read.
pub(crate) async fn reply(
    request: &ChatRequest,
    model: Model,
    body: &[u8],
    brain: &Arc<Brain>,
) -> Result<Reply, RequestError> {
    check_tool_messages(&request.messages)?;
    let prompt = render(request);
    let output = match model {
        Model::Echo => Output::Text(prompt.clone()),
        Model::Chat => answer(request, body, brain).await?,
    };
    let (output, finish) = shorten(output, &request.stops(), request.token_limit());
    Ok(Reply {
        id: format!("chatcmpl-{:016x}", fingerprint(body, 0)),
        model: request.model.clone(),
        usage: Usage::new(tokens::count(&prompt), output.tokens()),
        output,
        finish,
    })
}

/// What the chat model answers: the first rule of the module's list that
/// fits the request.
async fn answer(
    request: &ChatRequest,
    body: &[u8],
    brain: &Arc<Brain>,
) -> Result<Output, RequestError> {
    let messages = &request.messages;
    let last_user = messages
        .iter()
        .rposition(|message| message.role == Role::User);
    let (script, turn) = match last_user {
        Some(index) => {
            let script = directive::parse(&messages[index].text())
                .map_err(|reason| bad_directive(&reason))?;
            // Each assistant message after the user message is a turn the
            // model has already taken.
            let taken = &messages[index + 1..];
            let turn = taken
                .iter()
                .filter(|message| message.role == Role::Assistant)
                .count();
            (script, turn)
        }
        None => (Script::default(), 0),
    };
    let seed = request.seed().unsigned_abs();
    let call = |index: usize, name: &str, arguments: Option<&Map<String, Value>>| {
        let arguments = arguments.map_or_else(
            || made_arguments(request, name, seed),
            |arguments| Value::Object(arguments.clone()).to_string(),
        );
        let salt = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        let id = format!("call_{:016x}", fingerprint(body, salt));
        ToolCall::new(id, name.to_owned(), arguments)
    };
    if let Some(step) = script.steps.get(turn) {
        return Ok(match step {
            Step::Say(text) => Output::Text(text.clone()),
            Step::Call(calls) => Output::Calls(
                calls
                    .iter()
                    .enumerate()
                    .map(|(index, step)| call(index, &step.name, step.arguments.as_ref()))
                    .collect(),
            ),
        });
    }
    if let Some(quoted) = quoted_results(messages) {
        return Ok(Output::Text(quoted));
    }
    if let Some(name) = asked_tool(request, &script.rest) {
        return Ok(Output::Calls(vec![call(0, name, None)]));
    }
    let text = text(request, last_user, script.rest, brain).await;
    Ok(Output::Text(text))
}

/// The error for a directive that opens and cannot be read.
fn bad_directive(reason: &str) -> RequestError {
    RequestError::new(
        format!("Invalid directive in the last user message: {reason}."),
        Some("messages"),
        Some("invalid_directive"),
    )
}

/// The arguments for a call to the tool `name`, made from the schema the
/// request gives for it. A tool the request does not offer, and a tool
/// without a schema, get an empty object.
fn made_arguments(request: &ChatRequest, name: &str, seed: u64) -> String {
    request
        .functions()
        .find(|function| function.name == name)
        .and_then(|function| function.parameters.as_ref())
        .map_or_else(
            || json!({}),
            |parameters| schema::generate(parameters, seed),
        )
        .to_string()
}

/// The reply that ends a tool loop: each tool result of the request's last
/// round, as the tool's name and what it returned, on a line of its own.
/// `None` when the request does not end with tool results.
fn quoted_results(messages: &[Message]) -> Option<String> {
    let round = messages
        .iter()
        .rposition(|message| message.role == Role::Assistant)?
        + 1;
    let results = &messages[round..];
    let all_results = results.iter().all(|message| is_result(message.role));
    if results.is_empty() || !all_results {
        return None;
    }
    let lines: Vec<String> = (round..messages.len())
        .map(|index| format!("{}: {}", tool_name(messages, index), messages[index].text()))
        .collect();
    Some(lines.join("\n"))
}

/// Whether a message of this role holds the result of a tool call.
const fn is_result(role: Role) -> bool {
    matches!(role, Role::Tool | Role::Function)
}

/// The name of the tool whose result the message at `index` holds: the name
/// in the tool call that the message answers. A `function` message names
/// its function itself.
fn tool_name(messages: &[Message], index: usize) -> &str {
    let message = &messages[index];
    let id = message.tool_call_id.as_deref();
    let called = messages[..index]
        .iter()
        .rev()
        .flat_map(Message::tool_calls)
        .find(|call| Some(call.id.as_str()) == id)
        .map(|call| call.function.name.as_str());
    called.or(message.name.as_deref()).or(id).unwrap_or("tool")
}

/// The tool the request asks the model to call, when it asks for one: the
/// function that `tool_choice` forces, or a function whose name the user's
/// text holds. `required` with no name in the text takes the first
/// function.
fn asked_tool<'a>(request: &'a ChatRequest, user_text: &str) -> Option<&'a str> {
    let named = || named_tool(request, user_text).map(|function| function.name.as_str());
    match &request.tool_choice {
        Some(ToolChoice::Mode(ToolMode::None)) => None,
        Some(ToolChoice::Function { function, .. }) => Some(&function.name),
        Some(ToolChoice::Mode(ToolMode::Required)) => named().or_else(|| {
            request
                .functions()
                .next()
                .map(|function| function.name.as_str())
        }),
        Some(ToolChoice::Mode(ToolMode::Auto)) | None => named(),
    }
}

/// The function whose name `text` holds as whole words. When the text holds
/// several, the name with the most words wins, and then the function the
/// request offers first.
fn named_tool<'a>(request: &'a ChatRequest, text: &str) -> Option<&'a FunctionTool> {
    let said = words(text);
    let mut best: Option<(usize, &FunctionTool)> = None;
    for function in request.functions() {
        let name = words(&function.name);
        let held = !name.is_empty() && said.windows(name.len()).any(|window| window == name);
        if held && best.is_none_or(|(length, _)| name.len() > length) {
            best = Some((name.len(), function));
        }
    }
    best.map(|(_, function)| function)
}

/// The words of `text`, in lower case. A word ends at any character that is
/// not a letter or a digit, and where a capital follows a small letter or a
/// digit. So `get_weather`, `get weather` and `getWeather` are the same two
/// words.
fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut after_small = false;
    for character in text.chars() {
        let boundary = !character.is_alphanumeric() || (character.is_uppercase() && after_small);
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if character.is_alphanumeric() {
            word.extend(character.to_lowercase());
        }
        after_small = character.is_lowercase() || character.is_numeric();
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The text the chat model answers with when no rule about tools fits:
/// JSON in the form the request asks for, or the bot's reply. `last_user`
/// is the index of the last user message, and `rest` is its text without
/// directives.
async fn text(
    request: &ChatRequest,
    last_user: Option<usize>,
    rest: String,
    brain: &Arc<Brain>,
) -> String {
    let format = request.response_format.as_ref();
    if let Some(ResponseFormat::JsonSchema { json_schema }) = format {
        // A format that names no schema allows any JSON, and an empty
        // object is the plainest.
        let seed = request.seed().unsigned_abs();
        let value = json_schema
            .schema
            .as_ref()
            .map_or_else(|| json!({}), |schema| schema::generate(schema, seed));
        return value.to_string();
    }
    let turns = bot_turns(&request.messages, last_user, rest);
    let reply = Arc::clone(brain).reply(request.seed(), turns).await;
    match format {
        Some(ResponseFormat::JsonObject) => json!({ "reply": reply }).to_string(),
        _ => reply,
    }
}

/// The turns the bot reads: the text of each user message, in order,
/// without its directives. `last_user` is the index of the last user
/// message, and `rest` is its text without directives.
fn bot_turns(messages: &[Message], last_user: Option<usize>, rest: String) -> Vec<String> {
    let mut turns: Vec<String> = messages
        .iter()
        .enumerate()
        .filter(|(index, message)| message.role == Role::User && Some(*index) != last_user)
        .map(|(_, message)| directive::strip(&message.text()))
        .collect();
    if last_user.is_some() {
        turns.push(rest);
    }
    turns
}

/// Checks that tool calls and tool results pair up, as OpenAI checks them:
/// every tool message answers a call of an earlier assistant message, and
/// every call of an assistant message has a tool message after it.
fn check_tool_messages(messages: &[Message]) -> Result<(), RequestError> {
    for (index, message) in messages.iter().enumerate() {
        if message.role == Role::Tool {
            let Some(id) = &message.tool_call_id else {
                return Err(RequestError::new(
                    "Missing parameter 'tool_call_id': messages with role 'tool' must have a 'tool_call_id'.".to_owned(),
                    Some(&format!("messages.[{index}].tool_call_id")),
                    None,
                ));
            };
            let earlier = &messages[..index];
            if !earlier
                .iter()
                .flat_map(Message::tool_calls)
                .any(|call| call.id == *id)
            {
                // The misspelled word is OpenAI's.
                return Err(RequestError::new(
                    "Invalid parameter: messages with role 'tool' must be a response to a preceeding message with 'tool_calls'.".to_owned(),
                    Some(&format!("messages.[{index}].role")),
                    None,
                ));
            }
        }
        let later = &messages[index + 1..];
        let answered = |id: &str| {
            let answers = |message: &Message| {
                message.role == Role::Tool && message.tool_call_id.as_deref() == Some(id)
            };
            later.iter().any(answers)
        };
        let unanswered: Vec<&str> = message
            .tool_calls()
            .iter()
            .map(|call| call.id.as_str())
            .filter(|id| !answered(id))
            .collect();
        if !unanswered.is_empty() {
            return Err(RequestError::new(
                format!(
                    "An assistant message with 'tool_calls' must be followed by tool messages responding to each 'tool_call_id'. The following tool_call_ids did not have response messages: {}",
                    unanswered.join(", ")
                ),
                Some(&format!("messages.[{index}].role")),
                None,
            ));
        }
    }
    Ok(())
}

/// The prompt as the echo model returns it, one entry to a line:
///
/// - `tools:` and the tools as JSON, when the request offers any,
/// - `tool_choice:` and the choice as JSON, when the request makes one,
/// - each message as its role and its text,
/// - each tool call of an assistant message as `assistant: [call NAME
///   ARGUMENTS]`,
/// - each tool result as `tool NAME:` and its text.
///
/// The prompt tokens of every reply are the tokens of this text, so the
/// echo model's reply holds as many tokens as its prompt.
fn render(request: &ChatRequest) -> String {
    let mut entries = Vec::new();
    if !request.tools().is_empty() {
        let tools = serde_json::to_string(request.tools()).unwrap_or_default();
        entries.push(format!("tools: {tools}"));
    }
    if let Some(choice) = &request.tool_choice {
        let choice = serde_json::to_string(choice).unwrap_or_default();
        entries.push(format!("tool_choice: {choice}"));
    }
    for (index, message) in request.messages.iter().enumerate() {
        let role = message.role.name();
        let text = message.text();
        if is_result(message.role) {
            let name = tool_name(&request.messages, index);
            entries.push(format!("{role} {name}: {text}"));
            continue;
        }
        let calls = message.tool_calls();
        if calls.is_empty() || !text.is_empty() {
            entries.push(format!("{role}: {text}"));
        }
        for call in calls {
            let function = &call.function;
            entries.push(format!(
                "{role}: [call {} {}]",
                function.name, function.arguments
            ));
        }
    }
    entries.join("\n")
}

/// Shortens the output and says why it ends.
///
/// Text ends where the first stop sequence begins, and is then cut at the
/// token limit. A stop sequence is left out of the text, as OpenAI leaves it
/// out. Tool calls are cut at the token limit alone: the arguments of the
/// call that crosses the limit are cut there, and the calls after it are
/// dropped. A caller must not run a call whose arguments are cut, and the
/// finish reason `length` tells it so.
fn shorten(output: Output, stops: &[&str], limit: Option<u64>) -> (Output, FinishReason) {
    match output {
        Output::Text(mut text) => {
            let first_stop = stops.iter().filter_map(|stop| text.find(stop)).min();
            if let Some(end) = first_stop {
                text.truncate(end);
            }
            let cut = limit
                .and_then(|limit| tokens::cut(&text, limit))
                .map(str::len);
            let finish = if let Some(end) = cut {
                text.truncate(end);
                FinishReason::Length
            } else {
                FinishReason::Stop
            };
            (Output::Text(text), finish)
        }
        Output::Calls(calls) => {
            let Some(limit) = limit.filter(|limit| *limit < calls.iter().map(call_tokens).sum())
            else {
                return (Output::Calls(calls), FinishReason::ToolCalls);
            };
            (cut_calls(calls, limit), FinishReason::Length)
        }
    }
}

/// The calls that fit in `limit` tokens. A call whose name does not fit is
/// dropped with every call after it. When no call fits, the output is empty
/// text.
fn cut_calls(calls: Vec<ToolCall>, limit: u64) -> Output {
    let mut left = limit;
    let mut kept = Vec::new();
    for mut call in calls {
        let Some(after_name) = left.checked_sub(tokens::count(&call.function.name)) else {
            break;
        };
        let cut = tokens::cut(&call.function.arguments, after_name).map(str::len);
        if let Some(end) = cut {
            call.function.arguments.truncate(end);
        }
        left = after_name.saturating_sub(tokens::count(&call.function.arguments));
        kept.push(call);
        if cut.is_some() {
            break;
        }
    }
    if kept.is_empty() {
        Output::Text(String::new())
    } else {
        Output::Calls(kept)
    }
}

/// A 64-bit FNV-1a hash of `bytes`, then of `salt`. The standard library's
/// hashers may change between releases, and an id must stay the same for
/// the same request. Salt 0 gives the id of the completion, and each tool
/// call takes a salt of its own.
fn fingerprint(bytes: &[u8], salt: u64) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let hash = |hash: u64, byte: &u8| (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
    let body = bytes.iter().fold(OFFSET, hash);
    if salt == 0 {
        body
    } else {
        salt.to_le_bytes().iter().fold(body, hash)
    }
}

#[cfg(test)]
#[path = "reply-tests.rs"]
mod tests;
