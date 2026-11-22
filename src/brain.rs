//! The chat brain: RustBot, a Rust port of PyBot, behind one function.
//!
//! RustBot keeps a conversation in a `Bot` value and reads one user line per
//! turn. The server keeps nothing between requests, so each call here builds
//! a fresh bot and feeds it the conversation's user turns in order. The
//! reply to the last turn is the answer.

use std::panic::{self, UnwindSafe};

use rustbot::{Bot, one_line};

/// The longest user turn the bot reads, in characters.
///
/// The bot's time per turn grows with the length of the line. On the machine
/// that measured it, a warm turn of 500 characters took about 150 ms and one
/// of 2,000 characters about half a second. A turn this short also stays far
/// below the lengths at which the bot's regex engine gives up.
const TURN_LIMIT: usize = 500;

/// The reply when the bot fails. It is the bot's own line for a turn that no
/// rule answers.
const FALLBACK: &str = "Hello!";

// Each call moves its bot to a worker thread, so the bot must be `Send`.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Bot>();
};

/// Replays a conversation's user turns through a fresh bot and returns the
/// bot's reply to the last turn, on one line.
///
/// `seed` picks the wording where a rule has more than one. The same seed and
/// the same turns always give the same reply. Each turn is collapsed to one
/// line and cut at [`TURN_LIMIT`] characters before the bot reads it. A
/// conversation with no turns, and a bot that panics, both give the
/// fallback line.
pub(crate) async fn reply(seed: i64, turns: Vec<String>) -> String {
    // A turn is CPU work that takes from milliseconds to seconds, so the
    // replay runs on a blocking thread and the async threads stay free.
    tokio::task::spawn_blocking(move || replay(seed, &turns))
        .await
        .unwrap_or_else(|_| FALLBACK.to_owned())
}

/// The blocking body of [`reply`].
fn replay(seed: i64, turns: &[String]) -> String {
    guard(|| {
        // The bot takes its seed as decimal digits and ignores the sign.
        let mut bot = Bot::new(Some(&seed.to_string())).ok()?;
        let mut last = None;
        for turn in turns {
            last = Some(bot.turn(&one_turn(turn)).reply);
        }
        last.map(|reply| one_line(&reply))
    })
}

/// Runs the bot's work, and gives the fallback line when the work panics or
/// has no reply.
///
/// PyBot skips a rule that fails and goes on with the others. RustBot lets
/// the panic through, and its `turn` changes the bot's memory before it
/// records the exchange, so a bot that panicked is half-updated. The bot is
/// built inside `work`, so the unwind drops it. The panic message goes to
/// stderr through the panic hook.
fn guard(work: impl FnOnce() -> Option<String> + UnwindSafe) -> String {
    match panic::catch_unwind(work) {
        Ok(Some(reply)) => reply,
        Ok(None) | Err(_) => FALLBACK.to_owned(),
    }
}

/// What the bot reads of one user turn: the text on one line, cut at
/// [`TURN_LIMIT`] characters.
///
/// The bot expects one line per turn, and a stray carriage return changes
/// which rule it picks. Every run of whitespace and control characters
/// becomes one space here, so text that differs only in its line endings
/// reads the same on every system.
fn one_turn(text: &str) -> String {
    let mut line = String::new();
    let mut length = 0;
    let mut gap = false;
    for character in text.chars() {
        if character.is_whitespace() || character.is_control() {
            // A gap before the first word is dropped.
            gap = length > 0;
            continue;
        }
        let needed = if gap { 2 } else { 1 };
        if length + needed > TURN_LIMIT {
            break;
        }
        if gap {
            line.push(' ');
            gap = false;
        }
        line.push(character);
        length += needed;
    }
    line
}

#[cfg(test)]
#[path = "brain-tests.rs"]
mod tests;
