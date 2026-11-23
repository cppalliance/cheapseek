//! The chat brain: RustBot, a Rust port of PyBot, behind one call.
//!
//! RustBot keeps a conversation in a `Bot` value and reads one user line per
//! turn. A reply must depend on the request alone, so the reply to a
//! conversation is the one a fresh bot gives after it has read the
//! conversation's user turns in order.
//!
//! Replaying a conversation costs one bot turn for each user turn, on every
//! request. The brain therefore keeps the bots of the conversations it
//! answered last. A request that continues one of them takes that bot and
//! runs the new turns alone. A kept bot is in the state a fresh bot reaches
//! by replay, so the reply is the same either way: the kept bots save time
//! and change nothing.
//!
//! Some of the bot's rules read the date or the time of day. Every bot here
//! is given one fixed time, so those replies also depend on the request
//! alone.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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

/// How many bots the brain keeps. The bot that was used longest ago makes
/// room for a new one.
const KEPT_BOTS: usize = 64;

/// The time every bot reads in place of the system clock: the `created`
/// time of every reply, written with its time zone.
///
/// With the system clock, the bot would answer "When is the next full moon?"
/// differently from one day to the next. The time zone is UTC, so the date
/// the bot sees is the same wherever the server runs.
const NOW: &str = "2026-11-22T00:00:00+00:00[UTC]";

// Each call moves its bot to a worker thread, so the bot must be `Send`.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Bot>();
};

/// The chat bot, with the bots of the conversations it answered last.
pub(crate) struct Brain {
    kept: Mutex<Kept>,
    /// The time each new bot is given. It never changes, so a kept bot and a
    /// new bot read the same time.
    now: &'static str,
}

impl Default for Brain {
    fn default() -> Brain {
        Brain::at(NOW)
    }
}

/// The bots a brain keeps.
#[derive(Default)]
struct Kept {
    bots: Vec<KeptBot>,
    /// Counts the times the bots were looked at, so the brain knows which
    /// bot was used longest ago.
    uses: u64,
}

/// A bot that has read a conversation, and what it said last.
struct KeptBot {
    /// The key of the conversation the bot has read.
    key: String,
    /// How many turns the bot has read.
    read: usize,
    /// The bot is boxed, so a kept bot moves in and out of the list cheaply.
    bot: Box<Bot>,
    /// The bot's reply to the last turn it read.
    reply: String,
    /// The value of [`Kept::uses`] when the bot was last used.
    used: u64,
}

/// What the brain has for a conversation before it runs any turn.
enum Found {
    /// A bot has read this whole conversation, and this is its reply.
    Reply(String),
    /// A bot has read the first `read` turns of this conversation.
    Bot { read: usize, bot: Box<Bot> },
    /// No kept bot has read the start of this conversation.
    Nothing,
}

/// A conversation as the bot reads it.
struct Conversation {
    seed: i64,
    /// Each user turn on one line, cut at [`TURN_LIMIT`] characters.
    turns: Vec<String>,
    /// The seed, then each turn on a line of its own. A turn holds no line
    /// break, so two conversations have the same key only when their seeds
    /// and their turns are the same. The key of a conversation starts with
    /// the key of each of its earlier states.
    key: String,
}

impl Conversation {
    fn new(seed: i64, turns: &[String]) -> Conversation {
        let turns: Vec<String> = turns.iter().map(|turn| one_turn(turn)).collect();
        let mut key = seed.to_string();
        for turn in &turns {
            key.push('\n');
            key.push_str(turn);
        }
        Conversation { seed, turns, key }
    }

    /// Whether this conversation is the one with the key `earlier`, or that
    /// one with more turns after it.
    fn continues(&self, earlier: &str) -> bool {
        self.key
            .strip_prefix(earlier)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\n'))
    }
}

impl Brain {
    /// A brain whose bots read `now` as the time. The server uses [`NOW`]
    /// alone, and the tests use another time to show that the bots follow
    /// it.
    fn at(now: &'static str) -> Brain {
        Brain {
            kept: Mutex::default(),
            now,
        }
    }

    /// Returns the bot's reply to the last of a conversation's user turns,
    /// on one line.
    ///
    /// `seed` picks the wording where a rule has more than one. The same seed
    /// and the same turns always give the same reply, whatever the brain
    /// keeps. Each turn is collapsed to one line and cut at [`TURN_LIMIT`]
    /// characters before the bot reads it. A conversation with no turns, and
    /// a bot that panics, both give the fallback line.
    pub(crate) async fn reply(self: Arc<Self>, seed: i64, turns: Vec<String>) -> String {
        // A turn is CPU work that takes from milliseconds to seconds, so it
        // runs on a blocking thread and the async threads stay free.
        tokio::task::spawn_blocking(move || self.replay(seed, &turns))
            .await
            .unwrap_or_else(|_| FALLBACK.to_owned())
    }

    /// The blocking body of [`Brain::reply`].
    fn replay(&self, seed: i64, turns: &[String]) -> String {
        let conversation = Conversation::new(seed, turns);
        let (read, bot) = match self.find(&conversation) {
            Found::Reply(reply) => return reply,
            Found::Bot { read, bot } => (read, Some(bot)),
            Found::Nothing => (0, None),
        };
        let Conversation { seed, turns, key } = conversation;
        let total = turns.len();
        let now = self.now;
        // The lock is free while the bot runs, so a slow turn of one
        // conversation never holds up another.
        let outcome = guard(move || {
            let mut bot = match bot {
                Some(bot) => bot,
                None => Box::new(new_bot(seed, now)?),
            };
            let mut last = None;
            for turn in &turns[read..] {
                last = Some(bot.turn(turn).reply);
            }
            Some((bot, one_line(&last?)))
        });
        let Some((bot, reply)) = outcome else {
            return FALLBACK.to_owned();
        };
        self.keep(KeptBot {
            key,
            read: total,
            bot,
            reply: reply.clone(),
            used: 0,
        });
        reply
    }

    /// Looks for a kept bot that has read `conversation`, or its start. A
    /// bot that has read only the start is taken out, because the caller
    /// runs the next turns on it.
    fn find(&self, conversation: &Conversation) -> Found {
        let mut kept = self.lock();
        kept.uses += 1;
        let uses = kept.uses;
        // The bot that has read the most of the conversation is the one
        // with the longest key.
        let best = kept
            .bots
            .iter()
            .enumerate()
            .filter(|(_, bot)| conversation.continues(&bot.key))
            .max_by_key(|(_, bot)| bot.key.len())
            .map(|(index, _)| index);
        let Some(index) = best else {
            return Found::Nothing;
        };
        if kept.bots[index].key.len() == conversation.key.len() {
            let whole = &mut kept.bots[index];
            whole.used = uses;
            return Found::Reply(whole.reply.clone());
        }
        let start = kept.bots.swap_remove(index);
        Found::Bot {
            read: start.read,
            bot: start.bot,
        }
    }

    /// Keeps a bot for the next request of its conversation. A bot of the
    /// same conversation is replaced, and when the brain is full the bot
    /// that was used longest ago is dropped.
    fn keep(&self, mut bot: KeptBot) {
        let mut kept = self.lock();
        kept.uses += 1;
        bot.used = kept.uses;
        kept.bots.retain(|other| other.key != bot.key);
        kept.bots.push(bot);
        while kept.bots.len() > KEPT_BOTS {
            let oldest = kept
                .bots
                .iter()
                .enumerate()
                .min_by_key(|(_, bot)| bot.used)
                .map(|(index, _)| index);
            match oldest {
                Some(index) => drop(kept.bots.swap_remove(index)),
                None => break,
            }
        }
    }

    /// Locks the kept bots. The lock is held for list work alone and never
    /// while a bot runs, so a panic in a bot cannot poison it. A poisoned
    /// lock still holds a usable list.
    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A bot for a new conversation, which reads `now` as the time. `None` when
/// the bot refuses the seed or the time.
fn new_bot(seed: i64, now: &str) -> Option<Bot> {
    // The bot takes its seed as decimal digits and ignores the sign.
    let mut bot = Bot::new(Some(&seed.to_string())).ok()?;
    bot.set_now(Some(now)).ok()?;
    Some(bot)
}

/// Runs the bot's work, and gives `None` when the work panics or has no
/// result.
///
/// PyBot skips a rule that fails and goes on with the others. RustBot lets
/// the panic through, and its `turn` changes the bot's memory before it
/// records the exchange, so a bot that panicked is half-updated. The bot
/// lives inside `work`, so the unwind drops it and no later request reads
/// its memory. The panic message goes to stderr through the panic hook.
fn guard<T>(work: impl FnOnce() -> Option<T>) -> Option<T> {
    // The assertion is sound because nothing that `work` holds is read
    // after a panic: the unwind drops all of it.
    panic::catch_unwind(AssertUnwindSafe(work)).ok().flatten()
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
