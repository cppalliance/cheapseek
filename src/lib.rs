//! CheapSeek is a test server that speaks the OpenAI chat-completions API
//! and answers by rule, with zero inference.
//!
//! A test of a gateway or an agent harness points at it in place of a real
//! model. Every reply is computed from the request alone, so the same
//! request always gets the same bytes, and one server can answer any number
//! of tests at once.
//!
//! The `cheapseek` executable binds a loopback port, warms the chat bot,
//! prints the base URL, and calls [`serve`]. A test in another package can
//! skip the process and do the same on a listener of its own.
//!
//! ```no_run
//! # async fn example() -> std::io::Result<()> {
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
//! cheapseek::warm_up().await;
//! println!("http://{}/v1", listener.local_addr()?);
//! cheapseek::serve(listener, std::future::pending()).await
//! # }
//! ```

mod api;
mod brain;
mod reply;
mod sse;
mod tokens;
mod wire;

use std::future::Future;
use std::io;

use axum::Router;
use axum::serve::ListenerExt as _;
use tokio::net::TcpListener;

/// Builds the router that holds every route of the API.
///
/// [`serve`] runs this router on a listener. A caller that has an HTTP
/// stack of its own can mount the router there instead.
pub fn router() -> Router {
    api::router()
}

/// Runs one throwaway turn through the chat bot.
///
/// The bot compiles its rules the first time each one runs, and the first
/// turn of a process pays for most of them: it takes seconds where a later
/// turn takes milliseconds. A caller runs this once before it tells anyone
/// the server is up, so the first request is not the slow one.
pub async fn warm_up() {
    brain::reply(0, vec!["Hello.".to_owned()]).await;
}

/// Serves the API on `listener` until `shutdown` completes.
///
/// Requests that are in flight when `shutdown` completes run to their end.
///
/// # Errors
/// Returns the I/O error that stopped the server.
pub async fn serve<F>(listener: TcpListener, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    // A stream sends many small frames. Without this option the kernel holds
    // each one back until the previous one is acknowledged, which on a
    // loopback connection adds tens of milliseconds to every frame.
    let listener = listener.tap_io(|stream| {
        // The option only changes timing, so a connection that refuses it
        // is served as it is.
        let _ = stream.set_nodelay(true);
    });
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown)
        .await
}
