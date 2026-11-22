use super::*;

/// The error that parsing `json` gives.
fn error(json: &str) -> RequestError {
    ChatRequest::parse(json.as_bytes()).expect_err("an invalid request")
}

#[test]
fn every_model_is_found_by_its_own_name_and_by_no_other() {
    for model in Model::ALL {
        assert_eq!(Model::from_id(model.id()), Some(model));
    }
    assert_eq!(Model::from_id("cheapseek "), None);
    assert_eq!(Model::from_id("CheapSeek"), None);
    assert_eq!(Model::from_id(""), None);
}

#[test]
fn the_error_envelope_writes_an_absent_param_and_code_as_null() {
    let body = ErrorBody::invalid_request("wrong".to_owned(), None, None);
    let json = serde_json::to_string(&body).expect("the envelope is plain JSON");
    assert_eq!(
        json,
        r#"{"error":{"message":"wrong","type":"invalid_request_error","param":null,"code":null}}"#
    );
}

#[test]
fn a_request_with_fields_the_server_ignores_is_read() {
    let request = ChatRequest::parse(
        br#"{"model":"cheapseek","messages":[{"role":"user","content":"hi","name":"sam"}],
            "temperature":0.2,"top_p":1,"user":"u-1","chat_template_kwargs":{"enable_thinking":false},
            "max_tokens":null,"stop":null,"seed":null,"n":1,"stream":false}"#,
    )
    .expect("a valid request");
    assert_eq!(request.model, "cheapseek");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.token_limit(), None);
    assert_eq!(request.stops(), Vec::<&str>::new());
    assert_eq!(request.seed(), 0);
}

#[test]
fn the_newer_token_limit_wins_over_the_older_one() {
    let request = ChatRequest::parse(
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"max_tokens":9,"max_completion_tokens":3}"#,
    )
    .expect("a valid request");
    assert_eq!(request.token_limit(), Some(3));
}

#[test]
fn stop_is_one_text_or_a_list_and_an_empty_text_stops_nothing() {
    let one = ChatRequest::parse(
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"stop":"END"}"#,
    )
    .expect("a valid request");
    assert_eq!(one.stops(), ["END"]);
    let many = ChatRequest::parse(
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"stop":["a","","b"]}"#,
    )
    .expect("a valid request");
    assert_eq!(many.stops(), ["a", "b"]);
}

#[test]
fn a_body_that_is_not_json_names_the_parse_failure() {
    let error = error("{not json");
    assert!(
        error
            .message
            .starts_with("We could not parse the JSON body of your request. (HINT: "),
        "{}",
        error.message
    );
    assert_eq!((error.param, error.code), (None, None));
}

#[test]
fn a_body_that_is_not_an_object_is_refused() {
    assert!(error("[1,2]").message.contains("must be a JSON object"));
}

#[test]
fn a_missing_model_and_missing_messages_are_named() {
    assert_eq!(
        error(r#"{"messages":[]}"#).message,
        "you must provide a model parameter"
    );
    assert_eq!(
        error(r#"{"model":"cheapseek"}"#),
        RequestError {
            message: "Missing required parameter: 'messages'.".to_owned(),
            param: Some("messages".to_owned()),
            code: Some("missing_required_parameter"),
        }
    );
}

#[test]
fn an_empty_message_list_is_refused() {
    let error = error(r#"{"model":"cheapseek","messages":[]}"#);
    assert_eq!(error.param.as_deref(), Some("messages"));
    assert_eq!(error.code, Some("empty_array"));
}

#[test]
fn a_field_of_the_wrong_shape_is_named_with_its_path() {
    let role = error(r#"{"model":"cheapseek","messages":[{"role":"usr","content":"hi"}]}"#);
    assert!(
        role.message
            .starts_with("Invalid 'messages[0].role': unknown variant `usr`"),
        "{}",
        role.message
    );
    assert_eq!(role.param.as_deref(), Some("messages"));
    assert_eq!(role.code, Some("invalid_value"));

    let limit = error(
        r#"{"model":"cheapseek","messages":[{"role":"user","content":"hi"}],"max_tokens":"ten"}"#,
    );
    assert!(
        limit
            .message
            .starts_with("Invalid 'max_tokens': invalid type"),
        "{}",
        limit.message
    );
    assert_eq!(limit.param.as_deref(), Some("max_tokens"));
    assert_eq!(limit.code, Some("invalid_type"));
}

#[test]
fn a_limit_of_zero_is_below_the_minimum() {
    for name in ["max_tokens", "max_completion_tokens", "n"] {
        let body = format!(
            r#"{{"model":"cheapseek","messages":[{{"role":"user","content":"hi"}}],"{name}":0}}"#
        );
        let error = error(&body);
        assert_eq!(error.param.as_deref(), Some(name));
        assert_eq!(error.code, Some("integer_below_min_value"));
    }
}

#[test]
fn more_than_one_choice_is_unsupported() {
    let error = error(r#"{"model":"cheapseek","messages":[{"role":"user","content":"hi"}],"n":2}"#);
    assert_eq!(
        error,
        RequestError {
            message: "Unsupported value: 'n' does not support 2 with this model.".to_owned(),
            param: Some("n".to_owned()),
            code: Some("unsupported_value"),
        }
    );
}

#[test]
fn a_request_says_whether_it_streams_and_whether_it_wants_the_usage() {
    let read = |extra: &str| {
        let body = format!(
            r#"{{"model":"cheapseek","messages":[{{"role":"user","content":"hi"}}]{extra}}}"#
        );
        let request = ChatRequest::parse(body.as_bytes()).expect("a valid request");
        (request.streams(), request.includes_usage())
    };
    assert_eq!(read(""), (false, false));
    assert_eq!(read(r#","stream":false"#), (false, false));
    assert_eq!(
        read(r#","stream":null,"stream_options":null"#),
        (false, false)
    );
    assert_eq!(read(r#","stream":true"#), (true, false));
    assert_eq!(read(r#","stream":true,"stream_options":{}"#), (true, false));
    assert_eq!(
        read(r#","stream":true,"stream_options":{"include_usage":false}"#),
        (true, false)
    );
    assert_eq!(
        read(r#","stream":true,"stream_options":{"include_usage":true}"#),
        (true, true)
    );
}

#[test]
fn stream_options_without_a_stream_are_refused() {
    let error = error(
        r#"{"model":"cheapseek","messages":[{"role":"user","content":"hi"}],"stream_options":{"include_usage":true}}"#,
    );
    assert_eq!(
        error,
        RequestError {
            message: "The 'stream_options' parameter is only allowed when 'stream' is enabled."
                .to_owned(),
            param: Some("stream_options".to_owned()),
            code: None,
        }
    );
}

#[test]
fn more_than_four_stop_sequences_are_too_many() {
    let error = error(
        r#"{"model":"cheapseek","messages":[{"role":"user","content":"hi"}],"stop":["a","b","c","d","e"]}"#,
    );
    assert_eq!(error.param.as_deref(), Some("stop"));
    assert_eq!(error.code, Some("array_above_max_length"));
}

#[test]
fn a_part_that_is_not_text_shows_as_its_type() {
    let request = ChatRequest::parse(
        br#"{"model":"m","messages":[{"role":"user","content":[
            {"type":"text","text":"first"},
            {"type":"input_audio","input_audio":{"data":"AAAA","format":"wav"}},
            {"type":"text","text":"second"}
        ]}]}"#,
    )
    .expect("a valid request");
    assert_eq!(request.messages[0].text(), "first\n[input_audio]\nsecond");
}
