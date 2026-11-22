//! Tests of CheapSeek's public behavior. Each test talks to a real server
//! over HTTP, the way a gateway or a harness does.
#![expect(
    clippy::expect_used,
    reason = "a test helper panics when its setup fails, which fails the test with the reason"
)]

mod chat;
mod chat_errors;
mod client;
mod echo;
mod errors;
mod models;
mod process;
mod support;
