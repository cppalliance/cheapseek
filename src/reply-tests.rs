use super::*;

fn request(json: &str) -> ChatRequest {
    ChatRequest::parse(json.as_bytes()).expect("a valid request")
}

/// A request to the chat model with these messages and `extra` fields. With
/// `tools`, it offers a `get_weather` and a `read_file` tool.
fn chat(messages: &str, tools: bool, extra: &str) -> ChatRequest {
    let tools = if tools {
        r#","tools":[
            {"type":"function","function":{"name":"get_weather","description":"Weather for a city.",
             "parameters":{"type":"object","properties":{"city":{"type":"string"},"days":{"type":"integer","minimum":1}},"required":["city","days"]}}},
            {"type":"function","function":{"name":"read_file",
             "parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
        ]"#
    } else {
        ""
    };
    request(&format!(
        r#"{{"model":"cheapseek","messages":{messages}{tools}{extra}}}"#
    ))
}

/// The output the chat model gives `request`.
async fn answer_of(request: &ChatRequest) -> Output {
    answer(request, b"body", &Arc::default())
        .await
        .expect("an answer")
}

/// The name and the arguments of each call of `output`.
fn calls_of(output: &Output) -> Vec<(&str, &str)> {
    let Output::Calls(calls) = output else {
        panic!("tool calls, and this is {output:?}");
    };
    calls
        .iter()
        .map(|call| {
            (
                call.function.name.as_str(),
                call.function.arguments.as_str(),
            )
        })
        .collect()
}

fn text_output(text: &str) -> Output {
    Output::Text(text.to_owned())
}

fn call(name: &str, arguments: &str) -> ToolCall {
    ToolCall::new(
        format!("call_{name}"),
        name.to_owned(),
        arguments.to_owned(),
    )
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
fn the_prompt_shows_the_tools_the_choice_the_calls_and_the_results() {
    let request = request(
        r#"{"model":"cheapseek-echo","tool_choice":"auto",
            "tools":[{"type":"function","function":{"name":"get_weather","parameters":{"type":"object"}}}],
            "messages":[
                {"role":"user","content":"Weather in Oslo?"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Oslo\"}"}}
                ]},
                {"role":"tool","tool_call_id":"call_1","content":"4 C and rain"}
            ]}"#,
    );
    assert_eq!(
        render(&request),
        [
            r#"tools: [{"type":"function","function":{"name":"get_weather","parameters":{"type":"object"}}}]"#,
            r#"tool_choice: "auto""#,
            "user: Weather in Oslo?",
            r#"assistant: [call get_weather {"city":"Oslo"}]"#,
            "tool get_weather: 4 C and rain",
        ]
        .join("\n")
    );
}

#[test]
fn the_bot_reads_the_user_messages_without_their_directives() {
    let request = request(
        r#"{"model":"cheapseek","messages":[
            {"role":"system","content":"Be brief."},
            {"role":"user","content":"one [[say first]]"},
            {"role":"assistant","content":"first"},
            {"role":"user","content":"two [[call read_file]]"}
        ]}"#,
    );
    let turns = bot_turns(&request.messages, Some(3), "two ".to_owned());
    assert_eq!(turns, ["one ", "two "]);
    assert_eq!(
        bot_turns(&request.messages[..1], None, String::new()),
        Vec::<String>::new()
    );
}

#[test]
fn text_with_no_stop_and_no_limit_is_whole() {
    assert_eq!(
        shorten(text_output("Nice to meet you."), &[], None),
        (text_output("Nice to meet you."), FinishReason::Stop)
    );
}

#[test]
fn the_text_ends_where_the_first_stop_sequence_begins() {
    assert_eq!(
        shorten(text_output("Nice to meet you, Sam."), &["Sam"], None),
        (text_output("Nice to meet you, "), FinishReason::Stop)
    );
    assert_eq!(
        shorten(text_output("one two three"), &["three", "two"], None),
        (text_output("one "), FinishReason::Stop)
    );
    assert_eq!(
        shorten(text_output("one two three"), &["four"], None),
        (text_output("one two three"), FinishReason::Stop)
    );
}

#[test]
fn the_token_limit_cuts_the_text_and_reports_length() {
    assert_eq!(
        shorten(text_output("Nice to meet you."), &[], Some(2)),
        (text_output("Nice to"), FinishReason::Length)
    );
    assert_eq!(
        shorten(text_output("Nice to meet you."), &[], Some(5)),
        (text_output("Nice to meet you."), FinishReason::Stop)
    );
}

#[test]
fn a_stop_sequence_applies_before_the_token_limit() {
    assert_eq!(
        shorten(text_output("one two three four"), &["three"], Some(1)),
        (text_output("one"), FinishReason::Length)
    );
    assert_eq!(
        shorten(text_output("one two three four"), &["three"], Some(2)),
        (text_output("one two "), FinishReason::Stop)
    );
}

#[test]
fn tool_calls_that_fit_the_limit_are_whole_and_stop_sequences_leave_them_alone() {
    // `read` is 1 token, and `{"a":1}` is 7.
    let calls = vec![call("read", r#"{"a":1}"#)];
    assert_eq!(
        shorten(Output::Calls(calls.clone()), &["a"], Some(8)),
        (Output::Calls(calls), FinishReason::ToolCalls)
    );
}

#[test]
fn the_token_limit_cuts_the_arguments_of_a_call_and_drops_the_calls_after_it() {
    let calls = vec![call("read", r#"{"a":1}"#), call("write", r#"{"b":2}"#)];
    assert_eq!(
        shorten(Output::Calls(calls.clone()), &[], Some(4)),
        (
            Output::Calls(vec![call("read", r#"{"a"#)]),
            FinishReason::Length
        )
    );
    // The first call fits whole, and the second keeps its name alone.
    assert_eq!(
        shorten(Output::Calls(calls.clone()), &[], Some(10)),
        (
            Output::Calls(vec![call("read", r#"{"a":1}"#), call("write", "")]),
            FinishReason::Length
        )
    );
    // The name of the second call does not fit, so the call is dropped.
    assert_eq!(
        shorten(Output::Calls(calls), &[], Some(8)),
        (
            Output::Calls(vec![call("read", r#"{"a":1}"#)]),
            FinishReason::Length
        )
    );
}

#[test]
fn a_limit_below_the_first_name_leaves_empty_text() {
    let calls = vec![call("get_weather", "{}")];
    assert_eq!(
        shorten(Output::Calls(calls), &[], Some(1)),
        (text_output(""), FinishReason::Length)
    );
}

#[test]
fn the_fingerprint_is_fnv_1a_and_a_salt_changes_it() {
    assert_eq!(fingerprint(b"", 0), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fingerprint(b"a", 0), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fingerprint(b"foobar", 0), 0x8594_4171_f739_67e8);
    assert_ne!(fingerprint(b"foobar", 1), fingerprint(b"foobar", 0));
    assert_ne!(fingerprint(b"foobar", 1), fingerprint(b"foobar", 2));
}

#[test]
fn the_three_spellings_of_a_name_are_the_same_words() {
    assert_eq!(words("get_weather"), ["get", "weather"]);
    assert_eq!(words("get weather"), ["get", "weather"]);
    assert_eq!(words("getWeather"), ["get", "weather"]);
    assert_eq!(
        words("Get-Weather.now2Go"),
        ["get", "weather", "now2", "go"]
    );
    assert_eq!(words("  ...  "), Vec::<String>::new());
}

#[test]
fn a_tool_is_named_only_by_its_whole_words() {
    let named = |text: &str| {
        let request = chat(r#"[{"role":"user","content":"x"}]"#, true, "");
        named_tool(&request, text).map(|function| function.name.clone())
    };
    assert_eq!(
        named("please get weather for Oslo").as_deref(),
        Some("get_weather")
    );
    assert_eq!(named("call getWeather now").as_deref(), Some("get_weather"));
    assert_eq!(named("use GET_WEATHER").as_deref(), Some("get_weather"));
    assert_eq!(
        named("read file a.txt, then get the weather").as_deref(),
        Some("read_file")
    );
    assert_eq!(named("forget weathering the storm"), None);
    assert_eq!(named("the weather is nice, get it?"), None);
    assert_eq!(named(""), None);
}

#[test]
fn tool_results_at_the_end_are_quoted_with_the_names_of_their_tools() {
    let request = request(
        r#"{"model":"cheapseek","messages":[
            {"role":"user","content":"go"},
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{}"}},
                {"id":"call_2","type":"function","function":{"name":"read_file","arguments":"{}"}}
            ]},
            {"role":"tool","tool_call_id":"call_2","content":"file text"},
            {"role":"tool","tool_call_id":"call_1","content":"4 C and rain"}
        ]}"#,
    );
    assert_eq!(
        quoted_results(&request.messages).as_deref(),
        Some("read_file: file text\nget_weather: 4 C and rain")
    );
    // A request that ends with the assistant or the user has no results to
    // quote.
    assert_eq!(quoted_results(&request.messages[..2]), None);
    assert_eq!(quoted_results(&request.messages[..1]), None);
}

#[test]
fn a_tool_message_that_answers_no_earlier_call_is_refused() {
    let error = |messages: &str| {
        let request = request(&format!(r#"{{"model":"cheapseek","messages":{messages}}}"#));
        check_tool_messages(&request.messages).expect_err("a refusal")
    };
    let unpaired = error(
        r#"[{"role":"user","content":"go"},{"role":"tool","tool_call_id":"call_9","content":"x"}]"#,
    );
    assert_eq!(
        unpaired.message,
        "Invalid parameter: messages with role 'tool' must be a response to a preceeding message with 'tool_calls'."
    );
    assert_eq!(unpaired.param.as_deref(), Some("messages.[1].role"));

    let no_id = error(r#"[{"role":"user","content":"go"},{"role":"tool","content":"x"}]"#);
    assert_eq!(no_id.param.as_deref(), Some("messages.[1].tool_call_id"));

    let unanswered = error(
        r#"[{"role":"user","content":"go"},
            {"role":"assistant","tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"a","arguments":"{}"}},
                {"id":"call_2","type":"function","function":{"name":"b","arguments":"{}"}}
            ]},
            {"role":"tool","tool_call_id":"call_1","content":"x"}]"#,
    );
    assert!(
        unanswered
            .message
            .ends_with("did not have response messages: call_2")
    );
    assert_eq!(unanswered.param.as_deref(), Some("messages.[1].role"));
}

#[tokio::test]
async fn a_script_step_is_picked_by_the_turns_the_model_has_taken() {
    let script = r#"[[call read_file {\"path\":\"a.txt\"}]] [[call get_weather]] [[say Done.]]"#;
    let first = chat(
        &format!(r#"[{{"role":"user","content":"{script}"}}]"#),
        true,
        "",
    );
    assert_eq!(
        calls_of(&answer_of(&first).await),
        [("read_file", r#"{"path":"a.txt"}"#)]
    );

    let round = |id: &str, name: &str| {
        format!(
            r#"{{"role":"assistant","tool_calls":[{{"id":"{id}","type":"function","function":{{"name":"{name}","arguments":"{{}}"}}}}]}},
               {{"role":"tool","tool_call_id":"{id}","content":"ok"}}"#
        )
    };
    let second = chat(
        &format!(
            r#"[{{"role":"user","content":"{script}"}},{}]"#,
            round("c1", "read_file")
        ),
        true,
        "",
    );
    // The script gives no arguments here, so they come from the schema.
    assert_eq!(
        calls_of(&answer_of(&second).await),
        [("get_weather", r#"{"city":"a","days":1}"#)]
    );

    let third = chat(
        &format!(
            r#"[{{"role":"user","content":"{script}"}},{},{}]"#,
            round("c1", "read_file"),
            round("c2", "get_weather")
        ),
        true,
        "",
    );
    assert_eq!(answer_of(&third).await, text_output("Done."));
}

#[tokio::test]
async fn a_script_that_has_run_out_gives_way_to_the_tool_results() {
    let request = chat(
        r#"[{"role":"user","content":"[[call read_file]]"},
            {"role":"assistant","tool_calls":[{"id":"c1","type":"function","function":{"name":"read_file","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"c1","content":"the file"}]"#,
        true,
        "",
    );
    assert_eq!(
        answer_of(&request).await,
        text_output("read_file: the file")
    );
}

#[tokio::test]
async fn one_block_with_two_calls_is_one_turn_with_two_ids() {
    let request = chat(
        r#"[{"role":"user","content":"[[call read_file {\"path\":\"a\"}; call read_file {\"path\":\"b\"}]]"}]"#,
        true,
        "",
    );
    let output = answer_of(&request).await;
    assert_eq!(
        calls_of(&output),
        [
            ("read_file", r#"{"path":"a"}"#),
            ("read_file", r#"{"path":"b"}"#)
        ]
    );
    let Output::Calls(calls) = output else {
        panic!("tool calls");
    };
    assert_ne!(calls[0].id, calls[1].id);
    assert!(calls[0].id.starts_with("call_"), "{}", calls[0].id);
}

#[tokio::test]
async fn a_tool_is_called_when_the_text_names_it_or_the_choice_forces_one() {
    let user = r#"[{"role":"user","content":"Please get weather for Oslo."}]"#;
    let named = chat(user, true, "");
    assert_eq!(
        calls_of(&answer_of(&named).await),
        [("get_weather", r#"{"city":"a","days":1}"#)]
    );

    let forced = chat(
        user,
        true,
        r#","tool_choice":{"type":"function","function":{"name":"read_file"}}"#,
    );
    assert_eq!(
        calls_of(&answer_of(&forced).await),
        [("read_file", r#"{"path":"a"}"#)]
    );

    let required = chat(
        r#"[{"role":"user","content":"Hello."}]"#,
        true,
        r#","tool_choice":"required""#,
    );
    assert_eq!(
        calls_of(&answer_of(&required).await),
        [("get_weather", r#"{"city":"a","days":1}"#)]
    );

    let none = chat(user, true, r#","tool_choice":"none""#);
    assert!(matches!(answer_of(&none).await, Output::Text(_)));
}

#[tokio::test]
async fn a_directive_calls_a_tool_the_request_does_not_offer_with_empty_arguments() {
    let request = chat(
        r#"[{"role":"user","content":"[[call launch_rocket]]"}]"#,
        false,
        "",
    );
    assert_eq!(
        calls_of(&answer_of(&request).await),
        [("launch_rocket", "{}")]
    );
}

#[tokio::test]
async fn a_directive_that_cannot_be_read_is_refused() {
    let request = chat(r#"[{"role":"user","content":"[[call ]]"}]"#, true, "");
    let error = answer(&request, b"body", &Arc::default())
        .await
        .expect_err("a refusal");
    assert_eq!(
        error.message,
        "Invalid directive in the last user message: a `[[call` block names no tool."
    );
    assert_eq!(error.code, Some("invalid_directive"));
}

#[tokio::test]
async fn the_echo_reply_holds_as_many_tokens_as_its_prompt() {
    let body =
        r#"{"model":"cheapseek-echo","messages":[{"role":"user","content":"Hello, world!"}]}"#;
    let request = request(body);
    let reply = reply(&request, Model::Echo, body.as_bytes(), &Arc::default())
        .await
        .expect("a reply");
    assert_eq!(reply.output, text_output("user: Hello, world!"));
    assert_eq!(reply.finish, FinishReason::Stop);
    // `user`, `:`, and the six tokens of the text.
    assert_eq!(reply.usage, Usage::new(8, 8));
    assert_eq!(
        reply.id,
        format!("chatcmpl-{:016x}", fingerprint(body.as_bytes(), 0))
    );
}

#[tokio::test]
async fn a_tool_call_counts_the_tokens_of_its_name_and_its_arguments() {
    let body =
        r#"{"model":"cheapseek","messages":[{"role":"user","content":"[[call read {\"a\":1}]]"}]}"#;
    let request = request(body);
    let reply = reply(&request, Model::Chat, body.as_bytes(), &Arc::default())
        .await
        .expect("a reply");
    assert_eq!(reply.finish, FinishReason::ToolCalls);
    assert_eq!(calls_of(&reply.output), [("read", r#"{"a":1}"#)]);
    // `read` is 1 token, and `{"a":1}` is 7.
    assert_eq!(reply.usage, Usage::new(tokens::count(&render(&request)), 8));
}
