//! A server on a free loopback port for one test, the built executable as a
//! second kind of server, and the HTTP calls a test makes to either.

use std::net::{Ipv4Addr, SocketAddr};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, BufReader, Lines};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::time::timeout;

/// The path of the chat route.
pub(crate) const CHAT: &str = "/v1/chat/completions";

/// A server that lives as long as this value. Dropping it stops the server.
pub(crate) struct Server {
    origin: String,
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
            origin: format!("http://{address}"),
            stop: Some(stop),
        }
    }

    /// The full URL of `path`, which starts with a slash.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }

    /// The base URL an OpenAI client takes.
    pub(crate) fn base_url(&self) -> String {
        self.url("/v1")
    }

    /// Sends a GET and returns the status and the JSON body.
    pub(crate) async fn get(&self, path: &str) -> (u16, Value) {
        get_url(&self.url(path)).await
    }

    /// Sends a POST with a JSON body and returns the status and the JSON
    /// body of the reply.
    pub(crate) async fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        let response = client().post(self.url(path)).json(body).send().await;
        read_json(response).await
    }

    /// Sends a chat request and returns the status and the JSON body of the
    /// reply.
    pub(crate) async fn chat(&self, body: &Value) -> (u16, Value) {
        self.post(CHAT, body).await
    }

    /// Sends a chat request whose body is exactly `body`, and returns the
    /// status and the exact bytes of the reply.
    pub(crate) async fn chat_bytes(&self, body: &str) -> (u16, Vec<u8>) {
        post_bytes(&self.url(CHAT), body).await
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

/// How long the executable may take to print its line. It warms the bot
/// first, which takes seconds.
const START_LIMIT: Duration = Duration::from_secs(120);

/// The built executable, running. Dropping it kills the process, so a
/// failed assertion cannot leave a server behind.
pub(crate) struct Executable {
    /// The one line the executable printed: the base URL.
    pub(crate) line: String,
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Executable {
    /// Starts the executable with `arguments` and waits for its line.
    pub(crate) async fn start(arguments: &[&str]) -> Executable {
        Executable::start_with(arguments, &[]).await
    }

    /// Starts the executable with `arguments` and with `variables` added to
    /// its environment, and waits for its line.
    pub(crate) async fn start_with(arguments: &[&str], variables: &[(&str, &str)]) -> Executable {
        let mut child = command(arguments)
            .envs(variables.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the executable starts");
        let stdout = child.stdout.take().expect("a pipe to the child's stdout");
        let mut lines = BufReader::new(stdout).lines();
        let line = timeout(START_LIMIT, lines.next_line())
            .await
            .expect("a line within the time limit")
            .expect("a readable line")
            .expect("the executable prints before it ends");
        Executable { line, child, lines }
    }

    /// Sends the process the signal that Ctrl-C sends, waits for it to end,
    /// and returns its exit status and what it printed after its first line.
    #[cfg(unix)]
    pub(crate) async fn interrupt(mut self) -> (std::process::ExitStatus, Vec<String>) {
        let id = self
            .child
            .id()
            .expect("a process that still runs")
            .to_string();
        let sent = Command::new("kill")
            .args(["-INT", id.as_str()])
            .status()
            .await
            .expect("the kill command runs");
        assert!(sent.success(), "kill -INT {id}: {sent}");
        let status = timeout(START_LIMIT, self.child.wait())
            .await
            .expect("the executable ends within the time limit")
            .expect("the exit status of the executable");
        let mut rest = Vec::new();
        while let Some(line) = self.lines.next_line().await.expect("a readable line") {
            rest.push(line);
        }
        (status, rest)
    }

    /// Kills the process and returns what it printed after its first line.
    pub(crate) async fn stop(mut self) -> Vec<String> {
        self.child.kill().await.expect("the server stops");
        let mut rest = Vec::new();
        while let Some(line) = self.lines.next_line().await.expect("a readable line") {
            rest.push(line);
        }
        rest
    }
}

/// The command that runs the built executable with `arguments`. Its stdin is
/// closed, and the process dies with the value that owns it.
pub(crate) fn command(arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cheapseek"));
    command
        .args(arguments)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}

/// Waits for `child` to end and returns its output, within the start limit.
pub(crate) async fn output_of(child: Child) -> std::process::Output {
    timeout(START_LIMIT, child.wait_with_output())
        .await
        .expect("the executable ends within the time limit")
        .expect("the executable's output")
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
    read_json(client().get(url).send().await).await
}

/// Sends a POST whose body is exactly `body` to a full URL, and returns the
/// status and the exact bytes of the reply.
pub(crate) async fn post_bytes(url: &str, body: &str) -> (u16, Vec<u8>) {
    let response = client()
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_owned())
        .send()
        .await
        .expect("the server answers");
    let status = response.status().as_u16();
    let bytes = response.bytes().await.expect("a readable body");
    (status, bytes.to_vec())
}

/// The text of the reply in `bytes`, the body of a chat completion.
pub(crate) fn content(bytes: &[u8]) -> String {
    let body: Value = serde_json::from_slice(bytes).expect("a JSON reply");
    body["choices"][0]["message"]["content"]
        .as_str()
        .expect("text content")
        .to_owned()
}

async fn read_json(response: reqwest::Result<reqwest::Response>) -> (u16, Value) {
    let response = response.expect("the server answers");
    let status = response.status().as_u16();
    let body = response.json().await.expect("a JSON body");
    (status, body)
}
