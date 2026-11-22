use super::*;

/// A conversation with the reply the bot gave to each turn, recorded at
/// seed 0. It stays clear of dates and sums, the two kinds of answer that
/// can differ from one day or one system to the next.
const CONVERSATION: [(&str, &str); 4] = [
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

/// The first `count` user turns of the recorded conversation.
fn turns(count: usize) -> Vec<String> {
    CONVERSATION[..count]
        .iter()
        .map(|(user, _)| (*user).to_owned())
        .collect()
}

fn one(text: &str) -> Vec<String> {
    vec![text.to_owned()]
}

#[test]
fn a_four_turn_conversation_matches_the_recorded_replies_word_for_word() {
    for count in 1..=CONVERSATION.len() {
        let (user, recorded) = CONVERSATION[count - 1];
        assert_eq!(replay(0, &turns(count)), recorded, "the reply to {user:?}");
    }
}

#[test]
fn the_name_comes_back_only_when_the_turn_that_told_it_is_replayed() {
    assert_eq!(replay(0, &turns(4)), "You're Sam.");
    let without_the_name = replay(0, &turns(4)[1..]);
    assert!(
        !without_the_name.contains("Sam"),
        "the bot was never told the name: {without_the_name}"
    );
}

#[test]
fn twenty_fresh_bots_on_twenty_threads_agree() {
    let workers: Vec<_> = (0..20)
        .map(|_| std::thread::spawn(|| replay(0, &turns(4))))
        .collect();
    for worker in workers {
        let reply = worker.join().expect("the replay ends without a panic");
        assert_eq!(reply, "You're Sam.");
    }
}

#[test]
fn text_that_differs_only_in_line_endings_gets_the_same_reply() {
    let plain = replay(0, &one("Why do onions make you cry?"));
    assert_eq!(replay(0, &one("Why do onions\nmake you cry?")), plain);
    assert_eq!(replay(0, &one("Why do onions\r\nmake you cry?\r\n")), plain);
    assert_eq!(replay(0, &one("\tWhy do onions  make you cry?\r")), plain);
    assert!(plain.starts_with("Cutting an onion"), "{plain}");
}

#[test]
fn a_turn_is_collapsed_to_one_line() {
    assert_eq!(one_turn("  a\tb\r\n\r\nc \u{0}d \u{1b}  "), "a b c d");
    assert_eq!(one_turn(" \r\n\t "), "");
    assert_eq!(one_turn(""), "");
}

#[test]
fn a_long_turn_is_cut_at_the_limit_on_a_character_boundary() {
    let accents = "\u{e9}".repeat(TURN_LIMIT + 50);
    assert_eq!(one_turn(&accents).chars().count(), TURN_LIMIT);

    // A cut that falls on a gap between words leaves no space at the end.
    let words = "abcd ".repeat(TURN_LIMIT);
    let cut = one_turn(&words);
    assert!(cut.chars().count() <= TURN_LIMIT);
    assert!(cut.ends_with("abcd"), "{cut:?}");
    assert!(words.starts_with(&cut));
}

#[test]
fn a_very_long_turn_of_json_gets_a_reply() {
    let json =
        r#"{"tool":"read_file","path":"src/main.rs","lines":[1,2,3],"ok":true} "#.repeat(400);
    let reply = replay(0, &one(&json));
    assert!(!reply.is_empty());
    assert_eq!(replay(0, &one(&json)), reply);
}

#[test]
fn an_empty_turn_gets_a_reply_from_the_bot() {
    let reply = replay(0, &one(""));
    assert!(!reply.is_empty());
    assert_eq!(replay(0, &one("  \r\n")), reply);
}

#[test]
fn a_conversation_without_turns_gets_the_fallback_line() {
    assert_eq!(replay(0, &[]), "Hello!");
}

#[test]
fn a_seed_at_either_end_of_its_range_is_accepted() {
    assert_eq!(replay(i64::MIN, &turns(1)), CONVERSATION[0].1);
    assert_eq!(replay(i64::MAX, &turns(1)), CONVERSATION[0].1);
}

#[test]
fn a_panic_in_the_bot_yields_the_fallback_line() {
    assert_eq!(guard(|| panic!("a rule failed")), "Hello!");
}

#[tokio::test]
async fn the_async_reply_is_the_replay() {
    assert_eq!(reply(0, turns(1)).await, CONVERSATION[0].1);
}
