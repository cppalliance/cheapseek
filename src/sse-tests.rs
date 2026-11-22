use serde_json::{Value, json};

use super::*;
use crate::wire::{FinishReason, Usage};

fn reply(text: &str, finish: FinishReason) -> Reply {
    Reply {
        id: "chatcmpl-0123456789abcdef".to_owned(),
        model: "cheapseek".to_owned(),
        text: text.to_owned(),
        finish,
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
fn an_empty_reply_still_sends_the_role_and_the_finish() {
    let events = events(&reply("", FinishReason::Stop), false).expect("events");
    assert_eq!(chunks(&events).len(), 2);
}
