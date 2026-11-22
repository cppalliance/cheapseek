//! The echo model: the reply is the prompt as the server received it.

use serde_json::json;

use crate::support::Server;

#[tokio::test]
async fn the_echo_model_returns_the_prompt_one_message_to_an_entry() {
    let server = Server::start().await;
    let (status, body) = server
        .chat(&json!({
            "model": "cheapseek-echo",
            "messages": [
                { "role": "system", "content": "Be brief." },
                { "role": "user", "content": "hi there" }
            ]
        }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["model"], "cheapseek-echo");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "system: Be brief.\nuser: hi there"
    );
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn the_echo_reply_holds_as_many_tokens_as_its_prompt() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&json!({
            "model": "cheapseek-echo",
            "messages": [{ "role": "user", "content": "Hello, world!" }]
        }))
        .await;
    // `user`, `:`, ` Hell`, `o`, `,`, ` worl`, `d`, `!`
    assert_eq!(
        body["usage"],
        json!({ "prompt_tokens": 8, "completion_tokens": 8, "total_tokens": 16 })
    );
}

#[tokio::test]
async fn the_echo_shows_every_role_and_a_part_that_is_not_text_by_its_type() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&json!({
            "model": "cheapseek-echo",
            "messages": [
                { "role": "developer", "content": "Rules." },
                { "role": "user", "content": [
                    { "type": "text", "text": "What is in this picture?" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } }
                ]},
                { "role": "assistant", "content": null }
            ]
        }))
        .await;
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "developer: Rules.\nuser: What is in this picture?\n[image_url]\nassistant: "
    );
}

#[tokio::test]
async fn the_token_limit_cuts_the_echo_too() {
    let server = Server::start().await;
    let (_, body) = server
        .chat(&json!({
            "model": "cheapseek-echo",
            "max_tokens": 3,
            "messages": [{ "role": "user", "content": "Hello, world!" }]
        }))
        .await;
    assert_eq!(body["choices"][0]["message"]["content"], "user: Hell");
    assert_eq!(body["choices"][0]["finish_reason"], "length");
    assert_eq!(
        body["usage"],
        json!({ "prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11 })
    );
}
