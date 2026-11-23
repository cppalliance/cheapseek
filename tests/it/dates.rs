//! Questions the bot answers from the date or the time of day. The bot's
//! clock stands still at the `created` time of every reply, so these replies
//! are the same on every day, in every time zone, and on a host without
//! time zone files.

use serde_json::{Value, json};

use crate::support::{CHAT, Executable, Server, content, post_bytes};

/// Conversations whose last turn the bot answers from the date or the time
/// of day, with the reply it gives. Each reply fits one moment alone:
/// 22 November 2026, midnight UTC.
const DATED: [(&[&str], &str); 6] = [
    (
        &["When is the next full moon?"],
        "The next full moon is on Tuesday 24 November, in 2 days. Full moons come round every 29.5 days, so the one after is about four weeks later; the moon looks biggest when it is low in the sky, which is an illusion rather than the moon being closer.",
    ),
    (
        &["What should I plant in the garden this month?"],
        "It's November, so here's what goes in. Plant garlic and broad beans if the ground is not frozen, mulch the empty beds, and order seed potatoes to chit after Christmas. Nothing else goes in outdoors.",
    ),
    (
        &["I was born in 1990.", "How old am I?"],
        "You told me you're 36.",
    ),
    (
        &["What is the time difference in New York?"],
        "New York is currently 5 hours behind the UK. It is 7pm in New York now, and 12am in the UK.",
    ),
    (
        &["When do the school summer holidays start?"],
        "English state schools break up in the third week of July and go back in the first days of September - the next one is Wednesday 21 July 2027. The exact day is on your school's term dates, and it can sit a day or two either side.",
    ),
    (
        &[
            "We got a dog in March 2024.",
            "How long have we had the dog?",
        ],
        "Two and a half years - since March.",
    ),
];

/// The request body of a conversation with these user turns.
fn request(turns: &[&str]) -> String {
    let messages: Vec<Value> = turns
        .iter()
        .map(|turn| json!({ "role": "user", "content": turn }))
        .collect();
    json!({ "model": "cheapseek", "messages": messages }).to_string()
}

#[tokio::test]
async fn a_dated_question_gets_the_reply_of_the_created_time() {
    let server = Server::start().await;
    for (turns, recorded) in DATED {
        let (status, bytes) = server.chat_bytes(&request(turns)).await;
        assert_eq!(status, 200);
        assert_eq!(content(&bytes), recorded, "{turns:?}");
        let body: Value = serde_json::from_slice(&bytes).expect("a JSON reply");
        assert_eq!(body["created"], json!(1_795_305_600_u64));
    }
}

#[tokio::test]
async fn a_host_in_another_time_zone_and_without_zone_files_gives_the_same_bytes() {
    // `TZ` names the time zone of the host, and here it is 14 hours ahead of
    // UTC, where the date is already the next one. `TZDIR` names the folder
    // of the host's zone files, and an empty folder stands for a host
    // without them.
    let empty = std::env::temp_dir().join(format!("cheapseek-no-zones-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("an empty folder");
    let variables = [
        ("TZ", "Pacific/Kiritimati"),
        ("TZDIR", empty.to_str().expect("a path in UTF-8")),
    ];
    let moved = Executable::start_with(&[], &variables).await;
    let library = Server::start().await;

    for (turns, recorded) in DATED {
        let body = request(turns);
        let from_moved = post_bytes(&format!("{}/chat/completions", moved.line), &body).await;
        let from_library = post_bytes(&library.url(CHAT), &body).await;
        assert_eq!(from_moved.0, 200);
        assert_eq!(from_moved, from_library, "{turns:?}");
        assert_eq!(content(&from_moved.1), recorded, "{turns:?}");
    }

    drop(moved);
    std::fs::remove_dir(&empty).expect("the folder is still empty");
}
