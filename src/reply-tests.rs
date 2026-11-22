use super::*;

fn request(json: &str) -> ChatRequest {
    ChatRequest::parse(json.as_bytes()).expect("a valid request")
}

#[test]
fn the_prompt_is_rendered_one_message_to_an_entry() {
    let request = request(
        r#"{"model":"cheapseek-echo","messages":[
            {"role":"system","content":"Be brief."},
            {"role":"user","content":"hi there"},
            {"role":"assistant","content":null},
            {"role":"user","content":[
                {"type":"text","text":"look at this"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}}
            ]}
        ]}"#,
    );
    assert_eq!(
        render(&request),
        "system: Be brief.\nuser: hi there\nassistant: \nuser: look at this\n[image_url]"
    );
}

#[test]
fn the_bot_reads_the_user_messages_and_no_others() {
    let request = request(
        r#"{"model":"cheapseek","messages":[
            {"role":"system","content":"Be brief."},
            {"role":"user","content":"one"},
            {"role":"assistant","content":"reply"},
            {"role":"user","content":"two"}
        ]}"#,
    );
    assert_eq!(user_turns(&request), ["one", "two"]);
}

#[test]
fn text_with_no_stop_and_no_limit_is_whole() {
    assert_eq!(
        shorten("Nice to meet you.".to_owned(), &[], None),
        ("Nice to meet you.".to_owned(), FinishReason::Stop)
    );
}

#[test]
fn the_text_ends_where_the_first_stop_sequence_begins() {
    assert_eq!(
        shorten("Nice to meet you, Sam.".to_owned(), &["Sam"], None),
        ("Nice to meet you, ".to_owned(), FinishReason::Stop)
    );
    assert_eq!(
        shorten("one two three".to_owned(), &["three", "two"], None),
        ("one ".to_owned(), FinishReason::Stop)
    );
    assert_eq!(
        shorten("one two three".to_owned(), &["four"], None),
        ("one two three".to_owned(), FinishReason::Stop)
    );
}

#[test]
fn the_token_limit_cuts_the_text_and_reports_length() {
    assert_eq!(
        shorten("Nice to meet you.".to_owned(), &[], Some(2)),
        ("Nice to".to_owned(), FinishReason::Length)
    );
    assert_eq!(
        shorten("Nice to meet you.".to_owned(), &[], Some(5)),
        ("Nice to meet you.".to_owned(), FinishReason::Stop)
    );
}

#[test]
fn a_stop_sequence_applies_before_the_token_limit() {
    assert_eq!(
        shorten("one two three four".to_owned(), &["three"], Some(1)),
        ("one".to_owned(), FinishReason::Length)
    );
    assert_eq!(
        shorten("one two three four".to_owned(), &["three"], Some(2)),
        ("one two ".to_owned(), FinishReason::Stop)
    );
}

#[test]
fn the_fingerprint_is_fnv_1a() {
    assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fingerprint(b"foobar"), 0x8594_4171_f739_67e8);
}

#[tokio::test]
async fn the_echo_reply_holds_as_many_tokens_as_its_prompt() {
    let body =
        r#"{"model":"cheapseek-echo","messages":[{"role":"user","content":"Hello, world!"}]}"#;
    let request = request(body);
    let reply = reply(&request, Model::Echo, body.as_bytes()).await;
    assert_eq!(reply.text, "user: Hello, world!");
    assert_eq!(reply.finish, FinishReason::Stop);
    // `user`, `:`, and the six tokens of the text.
    assert_eq!(reply.usage, Usage::new(8, 8));
    assert_eq!(
        reply.id,
        format!("chatcmpl-{:016x}", fingerprint(body.as_bytes()))
    );
}
