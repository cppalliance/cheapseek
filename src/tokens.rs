//! The token rule: how text is cut into tokens.
//!
//! One rule gives every count in a reply: the `usage` numbers, the cut at
//! the token limit, and the pieces of a stream, which sends one token per
//! delta. A test can therefore work the numbers out by hand:
//!
//! - A run of letters and digits is cut into tokens of four characters. The
//!   last token of a run holds what is left.
//! - Every other visible character is one token.
//! - Whitespace belongs to the token that follows it. Whitespace at the end
//!   of the text belongs to the last token.
//!
//! So `Hello, world!` is six tokens: `Hell`, `o`, `,`, ` worl`, `d` and `!`.

/// How many letters and digits one token holds.
const RUN: usize = 4;

/// The byte offset at which each token of `text` ends, in order. Token `i`
/// starts where token `i - 1` ends, and the first starts at 0.
fn ends(text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    // The letters and digits in the token that is open. An open token may
    // also hold whitespace in front of them.
    let mut run = 0;
    let mut open = false;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if run > 0 {
                ends.push(index);
                run = 0;
            }
            open = true;
        } else if character.is_alphanumeric() {
            if run == RUN {
                ends.push(index);
                run = 0;
            }
            run += 1;
            open = true;
        } else {
            if run > 0 {
                ends.push(index);
                run = 0;
            }
            ends.push(index + character.len_utf8());
            open = false;
        }
    }
    if open {
        if run > 0 || ends.is_empty() {
            ends.push(text.len());
        } else if let Some(last) = ends.last_mut() {
            // Only whitespace is left, and it joins the last token.
            *last = text.len();
        }
    }
    ends
}

/// The number of tokens in `text`.
pub(crate) fn count(text: &str) -> u64 {
    u64::try_from(ends(text).len()).unwrap_or(u64::MAX)
}

/// The tokens of `text`, in order. Joined together they are `text`.
#[cfg(test)]
pub(crate) fn split(text: &str) -> Vec<&str> {
    let mut start = 0;
    ends(text)
        .into_iter()
        .map(|end| {
            let token = &text[start..end];
            start = end;
            token
        })
        .collect()
}

/// The first `limit` tokens of `text`, when `text` holds more than that.
/// `None` means the text fits within the limit.
pub(crate) fn cut(text: &str, limit: u64) -> Option<&str> {
    let ends = ends(text);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if ends.len() <= limit {
        return None;
    }
    let end = limit.checked_sub(1).map_or(0, |last| ends[last]);
    Some(&text[..end])
}

#[cfg(test)]
#[path = "tokens-tests.rs"]
mod tests;
