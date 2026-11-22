//! The built executable: what it prints, and that it serves on the port it
//! names.

use std::process::Stdio;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::support::get_url;

/// How long the executable may take to print its line.
const START_LIMIT: Duration = Duration::from_secs(120);

#[tokio::test]
async fn the_built_executable_prints_one_url_line_and_answers_on_it() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cheapseek"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        // A failed assertion below must not leave the server running.
        .kill_on_drop(true)
        .spawn()
        .expect("the executable starts");
    let stdout = child.stdout.take().expect("a pipe to the child's stdout");
    let mut lines = BufReader::new(stdout).lines();

    let line = timeout(START_LIMIT, lines.next_line())
        .await
        .expect("a line within the time limit")
        .expect("a readable line")
        .expect("the executable prints before it ends");
    let port = line
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/v1"))
        .expect("the line is the base URL");
    assert!(port.parse::<u16>().is_ok_and(|port| port != 0), "{line}");

    let (status, body) = get_url(&format!("{line}/models")).await;
    assert_eq!(status, 200);
    assert_eq!(body["object"], json!("list"));

    child.kill().await.expect("the server stops");
    let rest = lines.next_line().await.expect("a readable end of output");
    assert_eq!(rest, None, "the base URL is the only line");
}

#[tokio::test]
async fn a_port_that_is_taken_ends_the_executable_with_an_error_and_no_url() {
    // The test holds the port for the whole run, so the executable can only
    // fail on it. That it fails shows that `--port` names the port it binds.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    let port = taken.local_addr().expect("the bound address").port();

    let child = Command::new(env!("CARGO_BIN_EXE_cheapseek"))
        .args(["--port", &port.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the executable starts");
    let output = timeout(START_LIMIT, child.wait_with_output())
        .await
        .expect("the executable ends within the time limit")
        .expect("the executable's output");

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("cheapseek: "), "{stderr}");
    drop(taken);
}
