//! A reply as a stream of server-sent events, in the order OpenAI sends
//! them:
//!
//! 1. a chunk with the role and empty content,
//! 2. one chunk per token of the text,
//! 3. a chunk with an empty delta and the finish reason,
//! 4. when the request asked for the usage, a chunk with no choices and the
//!    usage of the whole reply,
//! 5. the line `data: [DONE]`.
//!
//! The reply is whole before the first event leaves, so the stream never
//! waits between events.

use std::convert::Infallible;

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use futures_util::stream;

use crate::reply::Reply;
use crate::tokens;
use crate::wire::{ChatChunk, Delta};

/// The last event of every stream.
const DONE: &str = "data: [DONE]\n\n";

/// The events of `reply`, in order. Each one is a whole event: the `data:`
/// line and the blank line that ends it. `with_usage` says whether the
/// request asked for the usage.
///
/// # Errors
/// Returns the error of a chunk that could not be written as JSON.
pub(crate) fn events(reply: &Reply, with_usage: bool) -> serde_json::Result<Vec<String>> {
    let chunk = |delta, finish| ChatChunk::new(&reply.id, &reply.model, delta, finish, with_usage);
    let mut events = vec![event(&chunk(Delta::role(), None))?];
    for token in tokens::split(&reply.text) {
        events.push(event(&chunk(Delta::Content { content: token }, None))?);
    }
    events.push(event(&chunk(Delta::Empty {}, Some(reply.finish)))?);
    if with_usage {
        let usage = ChatChunk::usage(&reply.id, &reply.model, reply.usage);
        events.push(event(&usage)?);
    }
    events.push(DONE.to_owned());
    Ok(events)
}

/// One chunk as one event.
fn event(chunk: &ChatChunk<'_>) -> serde_json::Result<String> {
    Ok(format!("data: {}\n\n", serde_json::to_string(chunk)?))
}

/// The HTTP response that sends `events`, each one as a frame of its own.
pub(crate) fn response(events: Vec<String>) -> Response {
    let frames = events.into_iter().map(Ok::<String, Infallible>);
    let headers = [
        (CONTENT_TYPE, "text/event-stream"),
        (CACHE_CONTROL, "no-cache"),
    ];
    (headers, Body::from_stream(stream::iter(frames))).into_response()
}

#[cfg(test)]
#[path = "sse-tests.rs"]
mod tests;
