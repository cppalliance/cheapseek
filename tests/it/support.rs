//! A server on a free loopback port for one test, and the HTTP calls a test
//! makes to it.

use std::net::{Ipv4Addr, SocketAddr};

use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// A server that lives as long as this value. Dropping it stops the server.
pub(crate) struct Server {
    base: String,
    stop: Option<oneshot::Sender<()>>,
}

impl Server {
    /// Starts a server on a free loopback port, inside the test's runtime.
    pub(crate) async fn start() -> Server {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let listener = TcpListener::bind(address)
            .await
            .expect("a free loopback port");
        let address = listener.local_addr().expect("the bound address");
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(async move {
            // Either a sent value or a dropped sender ends the server.
            let shutdown = async {
                let _ = stopped.await;
            };
            cheapseek::serve(listener, shutdown)
                .await
                .expect("the server runs until the test ends");
        });
        Server {
            base: format!("http://{address}"),
            stop: Some(stop),
        }
    }

    /// The full URL of `path`, which starts with a slash.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Sends a GET and returns the status and the JSON body.
    pub(crate) async fn get(&self, path: &str) -> (u16, Value) {
        read(client().get(self.url(path)).send().await).await
    }

    /// Sends a POST with a JSON body and returns the status and the JSON
    /// body of the reply.
    pub(crate) async fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        read(client().post(self.url(path)).json(body).send().await).await
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            // The server task may already be gone with the test's runtime.
            let _ = stop.send(());
        }
    }
}

/// An HTTP client that goes straight to the loopback port, whatever proxy
/// the environment names.
pub(crate) fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a plain HTTP client")
}

/// Sends a GET to a full URL and returns the status and the JSON body.
pub(crate) async fn get_url(url: &str) -> (u16, Value) {
    read(client().get(url).send().await).await
}

async fn read(response: reqwest::Result<reqwest::Response>) -> (u16, Value) {
    let response = response.expect("the server answers");
    let status = response.status().as_u16();
    let body = response.json().await.expect("a JSON body");
    (status, body)
}
