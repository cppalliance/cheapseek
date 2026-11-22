//! Directives: the commands a test writes into the prompt to script what
//! the model does.
//!
//! A directive is a block in double square brackets inside a user message:
//!
//! - `[[say TEXT]]` makes the model reply with exactly TEXT.
//! - `[[call NAME ARGUMENTS]]` makes the model call the tool NAME.
//!   ARGUMENTS is a JSON object. A call without it gets arguments made from
//!   the tool's schema.
//! - `[[call A {..}; call B {..}]]` makes the model call several tools in
//!   one turn.
//!
//! Each block is one turn of the model, so a message with three blocks is a
//! script of three turns.
//!
//! Only `[[say` and `[[call`, each followed by whitespace, open a block.
//! Any other text in double brackets, such as a nested JSON array, is
//! ordinary text. The text of a `say` block ends at the first `]]`.

use serde_json::{Map, Value};

/// What the model does in one turn of a script.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Reply with this exact text.
    Say(String),
    /// Call these tools, in this order.
    Call(Vec<Call>),
}

/// One tool call of a script.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Call {
    /// The name of the tool.
    pub(crate) name: String,
    /// The arguments the script gives. A call without them gets arguments
    /// made from the tool's schema.
    pub(crate) arguments: Option<Map<String, Value>>,
}

/// A user message, read for its directives.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Script {
    /// One step for each block, in order.
    pub(crate) steps: Vec<Step>,
    /// The message with every block taken out. This is what the chat bot
    /// reads.
    pub(crate) rest: String,
}

/// The verb that opens a block.
#[derive(Clone, Copy)]
enum Verb {
    Say,
    Call,
}

/// Reads the directives in `text`.
///
/// # Errors
/// Returns what is wrong with the first block that opens and cannot be
/// read.
pub(crate) fn parse(text: &str) -> Result<Script, String> {
    let mut script = Script::default();
    let mut tail = text;
    while let Some((before, verb, body)) = next_block(tail) {
        script.rest.push_str(before);
        let (step, after) = match verb {
            Verb::Say => say(body)?,
            Verb::Call => calls(body)?,
        };
        script.steps.push(step);
        tail = after;
    }
    script.rest.push_str(tail);
    Ok(script)
}

/// The text of a message with its directive blocks taken out. A message
/// whose blocks cannot be read is returned whole.
pub(crate) fn strip(text: &str) -> String {
    parse(text).map_or_else(|_| text.to_owned(), |script| script.rest)
}

/// Finds the first block in `text`: the text before it, its verb, and the
/// text after the verb, which starts with whitespace.
fn next_block(text: &str) -> Option<(&str, Verb, &str)> {
    text.match_indices("[[").find_map(|(start, _)| {
        let opened = &text[start + 2..];
        [("say", Verb::Say), ("call", Verb::Call)]
            .into_iter()
            .find_map(|(word, verb)| {
                let body = opened.strip_prefix(word)?;
                body.starts_with(char::is_whitespace)
                    .then_some((&text[..start], verb, body))
            })
    })
}

/// Reads the body of a `say` block: the text up to the first `]]`, after
/// the one whitespace character that follows the verb.
fn say(body: &str) -> Result<(Step, &str), String> {
    let mut characters = body.chars();
    characters.next();
    let text = characters.as_str();
    let end = text
        .find("]]")
        .ok_or("a `[[say` block has no closing `]]`")?;
    Ok((Step::Say(text[..end].to_owned()), &text[end + 2..]))
}

/// Reads the body of a `call` block: one call, or several with `; call`
/// between them, up to the closing `]]`.
fn calls(body: &str) -> Result<(Step, &str), String> {
    let mut calls = Vec::new();
    let mut tail = body;
    loop {
        let (call, after) = call(tail)?;
        let name = call.name.clone();
        calls.push(call);
        let after = after.trim_start();
        if let Some(after) = after.strip_prefix("]]") {
            return Ok((Step::Call(calls), after));
        }
        // Another call follows: `;`, then the word `call`, then whitespace.
        tail = after
            .strip_prefix(';')
            .map(str::trim_start)
            .and_then(|after| after.strip_prefix("call"))
            .filter(|after| after.starts_with(char::is_whitespace))
            .ok_or_else(|| {
                format!(
                    "after the call to `{name}` comes `]]` or `; call`, and the block has neither"
                )
            })?;
    }
}

/// Reads one call: the tool name, then the arguments when a `{` follows.
fn call(text: &str) -> Result<(Call, &str), String> {
    let text = text.trim_start();
    let length = text
        .find(|character: char| !(character.is_ascii_alphanumeric() || "_-.".contains(character)))
        .unwrap_or(text.len());
    if length == 0 {
        return Err("a `[[call` block names no tool".to_owned());
    }
    let (name, after) = text.split_at(length);
    let after = after.trim_start();
    if !after.starts_with('{') {
        let call = Call {
            name: name.to_owned(),
            arguments: None,
        };
        return Ok((call, after));
    }
    // The arguments may hold `]]` themselves, as the end of a nested array,
    // so a JSON parser finds where they end.
    let mut values = serde_json::Deserializer::from_str(after).into_iter::<Value>();
    match values.next() {
        Some(Ok(Value::Object(arguments))) => {
            let call = Call {
                name: name.to_owned(),
                arguments: Some(arguments),
            };
            Ok((call, &after[values.byte_offset()..]))
        }
        Some(Err(error)) => Err(format!(
            "the arguments of the call to `{name}` are not a JSON object: {error}"
        )),
        _ => Err(format!(
            "the arguments of the call to `{name}` are not a JSON object"
        )),
    }
}

#[cfg(test)]
#[path = "directive-tests.rs"]
mod tests;
