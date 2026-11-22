use serde_json::{Value, json};

use super::*;
use crate::wire::{FinishReason, ToolCall, Usage};

fn reply(text: &str, finish: FinishReason) -> Reply {
    Reply {
        id: "chatcmpl-0123456789abcdef".to_owned(),
        model: "cheapseek".to_owned(),
        output: Output::Text(text.to_owned()),
        finish,
        usage: Usage::new(3, 2),
    }
}

/// A reply that calls these tools, each a name and its arguments.
fn calling(calls: &[(&str, &str)]) -> Reply {
    let calls = calls
        .iter()
        .enumerate()
        .map(|(index, (name, arguments))| {
            ToolCall::new(
                format!("call_{index}"),
                (*name).to_owned(),
                (*arguments).to_owned(),
            )
        })
        .collect();
    Reply {
        id: "chatcmpl-0123456789abcdef".to_owned(),
        model: "cheapseek".to_owned(),
        output: Output::Calls(calls),
        finish: FinishReason::ToolCalls,
        usage: Usage::new(3, 2),
    }
}

/// The JSON of each event but the last, which must be the `[DONE]` line.
fn chunks(events: &[String]) -> Vec<Value> {
    let (done, chunks) = events.split_last().expect("at least one event");
    assert_eq!(done, "data: [DONE]\n\n");
    chunks
        .iter()
        .map(|event| {
            let json = event
                .strip_prefix("data: ")
                .and_then(|rest| rest.strip_suffix("\n\n"))
                .expect("a data line and a blank line");
            assert!(!json.contains('\n'), "one event is one line: {json}");
            serde_json::from_str(json).expect("a JSON chunk")
        })
        .collect()
}

#[test]
fn the_events_are_the_role_then_one_token_each_then_the_finish_then_done() {
    let events = events(&reply("Hi there", FinishReason::Stop), false).expect("events");
    let chunks = chunks(&events);
    assert_eq!(
        chunks[0],
        json!({
            "id": "chatcmpl-0123456789abcdef",
            "object": "chat.completion.chunk",
            "created": 1_795_305_600,
            "model": "cheapseek",
            "system_fingerprint": "fp_cheapseek",
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant", "content": "", "refusal": null },
                "logprobs": null,
                "finish_reason": null
            }]
        })
    );
    let deltas: Vec<&Value> = chunks
        .iter()
        .map(|chunk| &chunk["choices"][0]["delta"])
        .collect();
    assert_eq!(
        deltas[1..],
        [
            &json!({ "content": "Hi" }),
            &json!({ "content": " ther" }),
            &json!({ "content": "e" }),
            &json!({})
        ]
    );
    let finishes: Vec<&Value> = chunks
        .iter()
        .map(|chunk| &chunk["choices"][0]["finish_reason"])
        .collect();
    assert_eq!(
        finishes,
        [
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &json!("stop")
        ]
    );
}

#[test]
fn a_stream_without_the_usage_option_never_names_usage() {
    let events = events(&reply("Hi", FinishReason::Stop), false).expect("events");
    for chunk in chunks(&events) {
        assert_eq!(chunk.get("usage"), None, "{chunk}");
    }
}

#[test]
fn the_usage_option_adds_a_last_chunk_with_no_choices_and_null_usage_before_it() {
    let events = events(&reply("Hi", FinishReason::Length), true).expect("events");
    let chunks = chunks(&events);
    let (last, before) = chunks.split_last().expect("chunks");
    for chunk in before {
        assert_eq!(chunk["usage"], Value::Null, "{chunk}");
        assert_eq!(chunk["choices"].as_array().map(Vec::len), Some(1));
    }
    assert_eq!(last["choices"], json!([]));
    assert_eq!(
        last["usage"],
        json!({ "prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5 })
    );
    assert_eq!(
        before.last().expect("a finish chunk")["choices"][0]["finish_reason"],
        "length"
    );
}

#[test]
fn a_tool_call_sends_its_id_and_name_first_and_then_its_arguments_token_by_token() {
    let events = events(&calling(&[("read", r#"{"a":1}"#)]), false).expect("events");
    let chunks = chunks(&events);
    let deltas: Vec<&Value> = chunks
        .iter()
        .map(|chunk| &chunk["choices"][0]["delta"])
        .collect();
    assert_eq!(
        *deltas[0],
        json!({ "role": "assistant", "content": null, "refusal": null })
    );
    assert_eq!(
        *deltas[1],
        json!({ "tool_calls": [{
            "index": 0,
            "id": "call_0",
            "type": "function",
            "function": { "name": "read", "arguments": "" }
        }] })
    );
    let pieces: Vec<&str> = deltas[2..deltas.len() - 1]
        .iter()
        .map(|delta| {
            let call = &delta["tool_calls"][0];
            assert_eq!(call["index"], 0);
            assert_eq!(call.get("id"), None, "the id comes once: {delta}");
            assert_eq!(
                call["function"].get("name"),
                None,
                "the name comes once: {delta}"
            );
            call["function"]["arguments"]
                .as_str()
                .expect("a piece of the arguments")
        })
        .collect();
    assert_eq!(pieces, ["{", "\"", "a", "\"", ":", "1", "}"]);
    assert_eq!(**deltas.last().expect("a finish chunk"), json!({}));
    assert_eq!(
        chunks.last().expect("a finish chunk")["choices"][0]["finish_reason"],
        "tool_calls"
    );
}

#[test]
fn two_tool_calls_stream_one_after_the_other_under_their_own_indexes() {
    let events = events(&calling(&[("a", "{}"), ("b", "{}")]), false).expect("events");
    let chunks = chunks(&events);
    let starts: Vec<(u64, &str)> = chunks
        .iter()
        .filter_map(|chunk| {
            let call = &chunk["choices"][0]["delta"]["tool_calls"][0];
            Some((call["index"].as_u64()?, call["function"]["name"].as_str()?))
        })
        .collect();
    assert_eq!(starts, [(0, "a"), (1, "b")]);
    let indexes: Vec<u64> = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["delta"]["tool_calls"][0]["index"].as_u64())
        .collect();
    // For each call: its start, then `{` and `}`.
    assert_eq!(indexes, [0, 0, 0, 1, 1, 1]);
}

#[test]
fn an_empty_reply_still_sends_the_role_and_the_finish() {
    let events = events(&reply("", FinishReason::Stop), false).expect("events");
    assert_eq!(chunks(&events).len(), 2);
}
