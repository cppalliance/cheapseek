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

/// Conversations whose last turn the bot answers from the date or the time
/// of day, with the reply it gives at [`NOW`], recorded at seed 0.
const DATED: [(&[&str], &str); 4] = [
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
];

/// The turns of a conversation as the brain takes them.
fn owned(turns: &[&str]) -> Vec<String> {
    turns.iter().map(|turn| (*turn).to_owned()).collect()
}

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
fn a_dated_question_gets_the_reply_of_the_fixed_time() {
    for (turns, recorded) in DATED {
        assert_eq!(fresh(0, &owned(turns)), recorded, "{turns:?}");
    }
}

#[test]
fn a_kept_bot_reads_the_same_time_as_a_new_bot() {
    let brain = Brain::default();
    let (turns, recorded) = DATED[2];
    let turns = owned(turns);
    // The first request leaves a bot that has read one turn. The second
    // request runs the dated turn on that kept bot.
    brain.replay(0, &turns[..1]);
    assert_eq!(kept_reads(&brain), [1]);
    assert_eq!(brain.replay(0, &turns), recorded);
    assert_eq!(kept_reads(&brain), [2]);
}

#[test]
fn a_brain_at_another_time_gives_the_replies_of_that_time() {
    let later = Brain::at("2031-03-05T09:30:00+00:00[UTC]");
    let moon = later.replay(0, &owned(DATED[0].0));
    assert!(
        moon.starts_with("The next full moon is on Saturday 8 March, in 3 days."),
        "{moon}"
    );
    let garden = later.replay(0, &owned(DATED[1].0));
    assert!(garden.starts_with("It's March"), "{garden}");
    assert_eq!(
        later.replay(0, &owned(DATED[2].0)),
        "You told me you're 41."
    );
    // In March New York has moved its clocks forward and the UK has not.
    let spring = Brain::at("2031-03-20T15:00:00+00:00[UTC]");
    assert_eq!(
        spring.replay(0, &owned(DATED[3].0)),
        "New York is currently 4 hours behind the UK. It is 11am in New York now, and 3pm in the UK."
    );
}

#[test]
fn the_fixed_time_is_the_created_time_of_every_reply() {
    let (date, time) = NOW.split_once('T').expect("a date, then a time");
    assert_eq!(time, "00:00:00+00:00[UTC]", "midnight, in UTC");
    let mut numbers = date.split('-').map(str::parse::<u64>);
    let mut next = || {
        numbers
            .next()
            .expect("a year, a month and a day")
            .expect("a number")
    };
    let (year, month, day) = (next(), next(), next());

    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };
    let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days: u64 = (1970..year)
        .map(|year| if leap(year) { 366 } else { 365 })
        .sum();
    for (index, length) in (1..month).zip(months) {
        days += length + u64::from(index == 2 && leap(year));
    }
    days += day - 1;
    assert_eq!(days * 24 * 60 * 60, crate::wire::CREATED);
}

#[test]
fn a_time_that_the_bot_cannot_read_gives_the_fallback_line_and_keeps_no_bot() {
    let brain = Brain::at("the day after tomorrow");
    assert_eq!(brain.replay(0, &turns(1)), FALLBACK);
    assert_eq!(kept_reads(&brain), Vec::<usize>::new());
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
