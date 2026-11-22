//! The reply rules: how a request becomes a reply.
//!
//! A reply is computed from the request alone. The model name picks the way
//! it is made: the echo model returns the prompt as received, and the chat
//! model answers with the bot. The stop sequences and the token limit then
//! shorten the text, and the token rule counts what is left.

use serde_json::json;

use crate::wire::{
    ChatCompletion, ChatRequest, FinishReason, Message, Model, ResponseFormat, Role, Usage,
};
use crate::{brain, schema, tokens};

/// What the server answers to one chat request, before it is written out as
/// one JSON object or as a stream.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    /// The id of the completion, the same in every chunk of a stream.
    pub(crate) id: String,
    /// The model name, as the request wrote it.
    pub(crate) model: String,
    /// The text of the assistant message, already shortened.
    pub(crate) text: String,
    /// Why the text ends where it does.
    pub(crate) finish: FinishReason,
    /// The token counts of the prompt and of `text`.
    pub(crate) usage: Usage,
}

impl Reply {
    /// The reply as the one JSON object of a request that does not stream.
    pub(crate) fn into_completion(self) -> ChatCompletion {
        ChatCompletion::new(self.id, self.model, self.text, self.finish, self.usage)
    }
}

/// Makes the reply to `request`, which names `model`. `body` is the request
/// as it arrived, and the reply's id is a hash of it.
pub(crate) async fn reply(request: &ChatRequest, model: Model, body: &[u8]) -> Reply {
    let prompt = render(request);
    let text = match model {
        Model::Echo => prompt.clone(),
        Model::Chat => answer(request).await,
    };
    let (text, finish) = shorten(text, &request.stops(), request.token_limit());
    Reply {
        id: format!("chatcmpl-{:016x}", fingerprint(body)),
        model: request.model.clone(),
        usage: Usage::new(tokens::count(&prompt), tokens::count(&text)),
        text,
        finish,
    }
}

/// The text the chat model answers with. The first rule that fits the
/// request wins:
///
/// 1. A `response_format` of `json_schema` gets JSON made from the schema.
/// 2. A `response_format` of `json_object` gets the bot's reply inside a
///    JSON object.
/// 3. Any other request gets the bot's reply.
async fn answer(request: &ChatRequest) -> String {
    match &request.response_format {
        Some(ResponseFormat::JsonSchema { json_schema }) => {
            // A format that names no schema allows any JSON, and an empty
            // object is the plainest.
            let value = json_schema.schema.as_ref().map_or_else(
                || json!({}),
                |schema| schema::generate(schema, request.seed().unsigned_abs()),
            );
            value.to_string()
        }
        Some(ResponseFormat::JsonObject) => json!({ "reply": chat(request).await }).to_string(),
        Some(ResponseFormat::Text) | None => chat(request).await,
    }
}

/// The bot's reply to the user turns of the request.
async fn chat(request: &ChatRequest) -> String {
    brain::reply(request.seed(), user_turns(request)).await
}

/// The prompt as the echo model returns it: one entry per message, the role
/// and then the text, each entry on a line of its own.
///
/// The prompt tokens of every reply are the tokens of this text, so the
/// echo model's reply holds as many tokens as its prompt.
fn render(request: &ChatRequest) -> String {
    request
        .messages
        .iter()
        .map(|message| format!("{}: {}", message.role.name(), message.text()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of each user message, in order. These are the turns the bot
/// reads.
fn user_turns(request: &ChatRequest) -> Vec<String> {
    request
        .messages
        .iter()
        .filter(|message| message.role == Role::User)
        .map(Message::text)
        .collect()
}

/// Ends `text` where the first stop sequence begins, then cuts it at the
/// token limit. A stop sequence is left out of the text, as OpenAI leaves it
/// out.
fn shorten(mut text: String, stops: &[&str], limit: Option<u64>) -> (String, FinishReason) {
    let first_stop = stops.iter().filter_map(|stop| text.find(stop)).min();
    if let Some(end) = first_stop {
        text.truncate(end);
    }
    let cut = limit
        .and_then(|limit| tokens::cut(&text, limit))
        .map(str::len);
    match cut {
        Some(end) => {
            text.truncate(end);
            (text, FinishReason::Length)
        }
        None => (text, FinishReason::Stop),
    }
}

/// A 64-bit FNV-1a hash. The standard library's hashers may change between
/// releases, and an id must stay the same for the same request.
fn fingerprint(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
#[path = "reply-tests.rs"]
mod tests;
