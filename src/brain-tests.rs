use std::sync::atomic::{AtomicBool, Ordering};

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

/// The reply of a brain that keeps no bot yet: every turn is replayed.
fn fresh(seed: i64, turns: &[String]) -> String {
    Brain::default().replay(seed, turns)
}

/// How many turns each bot of `brain` has read, in order.
fn kept_reads(brain: &Brain) -> Vec<usize> {
    let mut reads: Vec<usize> = brain.lock().bots.iter().map(|bot| bot.read).collect();
    reads.sort_unstable();
    reads
}

#[test]
fn a_four_turn_conversation_matches_the_recorded_replies_word_for_word() {
    for count in 1..=CONVERSATION.len() {
        let (user, recorded) = CONVERSATION[count - 1];
        assert_eq!(fresh(0, &turns(count)), recorded, "the reply to {user:?}");
    }
}

#[test]
fn the_name_comes_back_only_when_the_turn_that_told_it_is_replayed() {
    assert_eq!(fresh(0, &turns(4)), "You're Sam.");
    let without_the_name = fresh(0, &turns(4)[1..]);
    assert!(
        !without_the_name.contains("Sam"),
        "the bot was never told the name: {without_the_name}"
    );
}

#[test]
fn twenty_fresh_bots_on_twenty_threads_agree() {
    let workers: Vec<_> = (0..20)
        .map(|_| std::thread::spawn(|| fresh(0, &turns(4))))
        .collect();
    for worker in workers {
        let reply = worker.join().expect("the replay ends without a panic");
        assert_eq!(reply, "You're Sam.");
    }
}

#[test]
fn twenty_threads_that_share_one_brain_agree() {
    let brain = Arc::new(Brain::default());
    let workers: Vec<_> = (0..20)
        .map(|index| {
            let brain = Arc::clone(&brain);
            // Half of the threads ask for the whole conversation and half
            // for its first three turns, so they also compete for one bot.
            std::thread::spawn(move || {
                let count = 3 + index % 2;
                (count, brain.replay(0, &turns(count)))
            })
        })
        .collect();
    for worker in workers {
        let (count, reply) = worker.join().expect("the replay ends without a panic");
        assert_eq!(reply, CONVERSATION[count - 1].1);
    }
}

#[test]
fn the_second_request_of_a_growing_conversation_runs_one_bot_turn() {
    let brain = Brain::default();
    assert_eq!(brain.replay(0, &turns(3)), CONVERSATION[2].1);
    assert_eq!(kept_reads(&brain), [3]);

    // The brain has a bot that read three turns, so the fourth turn is the
    // only one left to run.
    let Found::Bot { read, bot } = brain.find(&Conversation::new(0, &turns(4))) else {
        panic!("the brain kept the bot of the first request");
    };
    assert_eq!(read, 3);
    assert_eq!(
        kept_reads(&brain),
        Vec::<usize>::new(),
        "the bot was taken out"
    );
    drop(bot);

    // The same through the whole call: the kept bot moves on to four turns.
    assert_eq!(brain.replay(0, &turns(3)), CONVERSATION[2].1);
    assert_eq!(brain.replay(0, &turns(4)), CONVERSATION[3].1);
    assert_eq!(kept_reads(&brain), [4]);
}

#[test]
fn replies_match_with_the_kept_bots_and_without_them() {
    let brain = Brain::default();
    // A growing conversation, then the same requests backwards, then each
    // one again.
    let counts = [1, 2, 3, 4, 4, 3, 2, 1, 1, 3, 2, 4];
    for count in counts {
        let kept = brain.replay(0, &turns(count));
        assert_eq!(kept, fresh(0, &turns(count)), "{count} turns");
        assert_eq!(kept, CONVERSATION[count - 1].1);
    }
}

#[test]
fn an_identical_request_gets_the_kept_reply() {
    let brain = Brain::default();
    brain.replay(0, &turns(2));
    let Found::Reply(reply) = brain.find(&Conversation::new(0, &turns(2))) else {
        panic!("the brain kept the reply of the first request");
    };
    assert_eq!(reply, CONVERSATION[1].1);
    assert_eq!(kept_reads(&brain), [2], "the bot stays for the next turn");
}

#[test]
fn a_conversation_with_another_seed_or_other_turns_shares_no_bot() {
    let brain = Brain::default();
    brain.replay(0, &turns(2));
    let found = |seed: i64, turns: &[String]| {
        !matches!(brain.find(&Conversation::new(seed, turns)), Found::Nothing)
    };
    assert!(!found(1, &turns(3)), "another seed");
    assert!(!found(0, &turns(1)), "a shorter conversation");
    let mut other = turns(3);
    other[1].push_str(" And garlic?");
    assert!(!found(0, &other), "a turn that only starts the same");
    other = turns(3);
    other[0] = "Hi, I'm Pat.".to_owned();
    assert!(!found(0, &other), "another first turn");
    assert!(found(0, &turns(3)), "the same conversation, one turn on");
}

#[test]
fn the_brain_keeps_a_limited_number_of_bots_and_drops_the_one_used_longest_ago() {
    let brain = Brain::default();
    let key = |index: usize| format!("0\nconversation {index}");
    let keep = |index: usize| {
        brain.keep(KeptBot {
            key: key(index),
            read: 1,
            bot: Box::default(),
            reply: format!("reply {index}"),
            used: 0,
        });
    };
    let has = |index: usize| brain.lock().bots.iter().any(|bot| bot.key == key(index));
    for index in 0..KEPT_BOTS {
        keep(index);
    }
    assert_eq!(brain.lock().bots.len(), KEPT_BOTS);

    // A look at the oldest bot makes it the newest.
    let oldest = Conversation::new(0, &one("conversation 0"));
    assert!(matches!(brain.find(&oldest), Found::Reply(reply) if reply == "reply 0"));
    keep(KEPT_BOTS);
    assert_eq!(brain.lock().bots.len(), KEPT_BOTS);
    assert!(has(0), "the bot that was just used stays");
    assert!(!has(1), "the bot that was used longest ago goes");
    assert!(has(2));
    assert!(has(KEPT_BOTS));

    // A bot of the same conversation is replaced.
    keep(2);
    assert_eq!(brain.lock().bots.len(), KEPT_BOTS);
}

#[test]
fn text_that_differs_only_in_line_endings_gets_the_same_reply() {
    let plain = fresh(0, &one("Why do onions make you cry?"));
    assert_eq!(fresh(0, &one("Why do onions\nmake you cry?")), plain);
    assert_eq!(fresh(0, &one("Why do onions\r\nmake you cry?\r\n")), plain);
    assert_eq!(fresh(0, &one("\tWhy do onions  make you cry?\r")), plain);
    assert!(plain.starts_with("Cutting an onion"), "{plain}");

    // Such text is also one conversation to the brain.
    let brain = Brain::default();
    brain.replay(0, &one("Why do onions\r\nmake you cry?"));
    let same = Conversation::new(0, &one("Why do onions make you cry?"));
    assert!(matches!(brain.find(&same), Found::Reply(_)));
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
    let reply = fresh(0, &one(&json));
    assert!(!reply.is_empty());
    assert_eq!(fresh(0, &one(&json)), reply);
}

#[test]
fn an_empty_turn_gets_a_reply_from_the_bot() {
    let reply = fresh(0, &one(""));
    assert!(!reply.is_empty());
    assert_eq!(fresh(0, &one("  \r\n")), reply);
    // A second empty turn is a turn of its own.
    let brain = Brain::default();
    brain.replay(0, &one(""));
    let two = Conversation::new(0, &[String::new(), String::new()]);
    assert!(matches!(brain.find(&two), Found::Bot { read: 1, .. }));
}

#[test]
fn a_conversation_without_turns_gets_the_fallback_line_and_keeps_no_bot() {
    let brain = Brain::default();
    assert_eq!(brain.replay(0, &[]), "Hello!");
    assert_eq!(kept_reads(&brain), Vec::<usize>::new());
}

#[test]
fn a_seed_at_either_end_of_its_range_is_accepted() {
    assert_eq!(fresh(i64::MIN, &turns(1)), CONVERSATION[0].1);
    assert_eq!(fresh(i64::MAX, &turns(1)), CONVERSATION[0].1);
}

#[test]
fn work_that_panics_has_no_result() {
    assert_eq!(guard::<String>(|| panic!("a rule failed")), None);
    assert_eq!(guard(|| Some(7)), Some(7));
    assert_eq!(guard::<u8>(|| None), None);
}

#[test]
fn what_the_work_holds_is_dropped_when_the_work_panics() {
    /// Stands in for a bot: it reports when it is dropped.
    struct Held(Arc<AtomicBool>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let held = Held(Arc::clone(&dropped));
    let outcome = guard::<Held>(move || {
        // The work owns the value, as the replay owns its bot.
        let held = held;
        assert!(!held.0.load(Ordering::SeqCst));
        panic!("a rule failed");
    });
    assert!(outcome.is_none());
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn the_async_reply_is_the_replay() {
    let brain = Arc::new(Brain::default());
    assert_eq!(
        Arc::clone(&brain).reply(0, turns(1)).await,
        CONVERSATION[0].1
    );
    assert_eq!(brain.reply(0, turns(2)).await, CONVERSATION[1].1);
}
