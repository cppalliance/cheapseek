//! Conversations that grow over many requests. The server keeps the bot of
//! a conversation to save time, and these tests check that the kept bots
//! never change a reply.

use futures_util::future::join_all;
use serde_json::{Value, json};

use crate::support::{Server, content};

/// The user turns of a conversation, and the reply the bot gives to each.
const TURNS: [(&str, &str); 4] = [
    (
        "Hi, I'm Sam.",
        "Nice to meet you, Sam. What can I help with?",
    ),
    (
        "Why do onions make you cry?",
        "Cutting an onion breaks its cells and releases a gas that irritates your eyes, so they make tears to wash it away. A sharp knife and a chilled onion release less of it, and the root end holds most of it, so cut that last.",
    ),
    (
        "How long should I boil an egg?",
        "Lower eggs into already boiling water: 6 minutes for a runny yolk, 7 for jammy, 9 to 10 for hard boiled. Cool them in cold water for a minute before peeling so the shell comes away cleanly.",
    ),
    ("What's my name?", "You're Sam."),
];

/// The request a chat application sends for its turn number `count`: the
/// first `count` user turns, with the assistant's reply after each one but
/// the last.
fn request(count: usize) -> String {
    let mut messages: Vec<Value> = Vec::new();
    for (index, (user, assistant)) in TURNS[..count].iter().enumerate() {
        messages.push(json!({ "role": "user", "content": user }));
        if index + 1 < count {
            messages.push(json!({ "role": "assistant", "content": assistant }));
        }
    }
    json!({ "model": "cheapseek", "messages": messages }).to_string()
}

#[tokio::test]
async fn a_growing_conversation_gets_the_replies_a_new_server_gives() {
    let server = Server::start().await;
    for count in 1..=TURNS.len() {
        let body = request(count);
        let kept = server.chat_bytes(&body).await;
        // A server that has seen no request holds no bot, so it replays
        // every turn.
        let new = Server::start().await.chat_bytes(&body).await;
        assert_eq!(kept.0, 200);
        assert_eq!(kept, new, "turn {count}");
        assert_eq!(content(&kept.1), TURNS[count - 1].1);
    }
}

#[tokio::test]
async fn requests_in_any_order_get_the_replies_of_the_growing_order() {
    let server = Server::start().await;
    for count in [4, 2, 2, 1, 3, 4, 1] {
        let (status, bytes) = server.chat_bytes(&request(count)).await;
        assert_eq!(status, 200);
        assert_eq!(content(&bytes), TURNS[count - 1].1, "turn {count}");
    }
}

#[tokio::test]
async fn identical_requests_at_once_get_the_same_bytes() {
    let server = Server::start().await;
    let body = request(4);
    let replies = join_all((0..8).map(|_| server.chat_bytes(&body))).await;
    for reply in &replies {
        assert_eq!(reply.0, 200);
        assert_eq!(reply, &replies[0]);
    }
    assert_eq!(content(&replies[0].1), "You're Sam.");
}

#[tokio::test]
async fn the_turns_of_two_conversations_at_once_stay_apart() {
    let server = Server::start().await;
    let introduce = |name: &str| {
        json!({
            "model": "cheapseek",
            "messages": [
                { "role": "user", "content": format!("Hi, I'm {name}.") },
                { "role": "assistant", "content": "Hello." },
                { "role": "user", "content": "What's my name?" }
            ]
        })
        .to_string()
    };
    let names = ["Sam", "Pat", "Kim", "Lee"];
    let bodies: Vec<String> = names.iter().map(|name| introduce(name)).collect();
    // Each conversation is sent twice, and all eight requests run at once.
    let requests = bodies
        .iter()
        .chain(&bodies)
        .map(|body| server.chat_bytes(body));
    let replies = join_all(requests).await;
    for (index, reply) in replies.iter().enumerate() {
        let name = names[index % names.len()];
        assert_eq!(content(&reply.1), format!("You're {name}."));
    }
}
