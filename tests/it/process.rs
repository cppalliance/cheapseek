//! The built executable: what it prints, that it serves on the port it
//! names, and that two processes give one request the same bytes.

use std::process::Stdio;

use serde_json::json;

use crate::support::{CHAT, Executable, Server, command, get_url, output_of, post_bytes};

#[tokio::test]
async fn the_built_executable_prints_one_url_line_and_answers_on_it() {
    let executable = Executable::start(&[]).await;
    let line = executable.line.clone();
    let port = line
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/v1"))
        .expect("the line is the base URL");
    assert!(port.parse::<u16>().is_ok_and(|port| port != 0), "{line}");

    let (status, body) = get_url(&format!("{line}/models")).await;
    assert_eq!(status, 200);
    assert_eq!(body["object"], json!("list"));

    let rest = executable.stop().await;
    assert_eq!(rest, Vec::<String>::new(), "the base URL is the only line");
}

#[tokio::test]
async fn a_port_that_is_taken_ends_the_executable_with_an_error_and_no_url() {
    // The test holds the port for the whole run, so the executable can only
    // fail on it. That it fails shows that `--port` names the port it binds.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    let port = taken.local_addr().expect("the bound address").port();

    let child = command(&["--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the executable starts");
    let output = output_of(child).await;

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("cheapseek: "), "{stderr}");
    drop(taken);
}

#[tokio::test]
async fn two_processes_and_the_library_give_one_request_the_same_bytes() {
    let body = r#"{"model":"cheapseek","messages":[
        {"role":"user","content":"Hi, I'm Sam."},
        {"role":"assistant","content":"Nice to meet you, Sam. What can I help with?"},
        {"role":"user","content":"Why do onions make you cry?"},
        {"role":"user","content":"What's my name?"}
    ]}"#;
    let (first, second) = tokio::join!(Executable::start(&[]), Executable::start(&[]));
    let library = Server::start().await;

    let from_first = post_bytes(&format!("{}/chat/completions", first.line), body).await;
    let from_second = post_bytes(&format!("{}/chat/completions", second.line), body).await;
    let from_library = post_bytes(&library.url(CHAT), body).await;

    assert_eq!(from_first.0, 200);
    assert_eq!(from_first, from_second);
    assert_eq!(from_first, from_library);
    let text = String::from_utf8_lossy(&from_first.1);
    assert!(text.contains("You're Sam."), "{text}");
}
