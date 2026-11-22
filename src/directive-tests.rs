use serde_json::json;

use super::*;

/// A call step with these calls, each a name and its arguments as JSON.
fn call_step(calls: &[(&str, Option<Value>)]) -> Step {
    Step::Call(
        calls
            .iter()
            .map(|(name, arguments)| Call {
                name: (*name).to_owned(),
                arguments: arguments
                    .as_ref()
                    .map(|value| value.as_object().expect("an object").clone()),
            })
            .collect(),
    )
}

#[test]
fn a_message_without_blocks_has_no_steps_and_keeps_its_text() {
    let script = parse("What's the weather in Paris?").expect("a script");
    assert_eq!(script.steps, []);
    assert_eq!(script.rest, "What's the weather in Paris?");
}

#[test]
fn a_say_block_holds_its_exact_text() {
    let script = parse("[[say Done.]]").expect("a script");
    assert_eq!(script.steps, [Step::Say("Done.".to_owned())]);
    assert_eq!(script.rest, "");

    let script = parse("[[say  two  spaces\nand a line ]]").expect("a script");
    assert_eq!(
        script.steps,
        [Step::Say(" two  spaces\nand a line ".to_owned())]
    );
}

#[test]
fn the_text_of_a_say_block_ends_at_the_first_closing_brackets() {
    let script = parse("[[say a]] b]]").expect("a script");
    assert_eq!(script.steps, [Step::Say("a".to_owned())]);
    assert_eq!(script.rest, " b]]");
}

#[test]
fn a_call_block_holds_the_tool_name_and_its_exact_arguments() {
    let script = parse(r#"[[call read_file {"path":"a.txt","lines":[1,2]}]]"#).expect("a script");
    assert_eq!(
        script.steps,
        [call_step(&[(
            "read_file",
            Some(json!({ "path": "a.txt", "lines": [1, 2] }))
        )])]
    );
}

#[test]
fn a_call_without_arguments_leaves_them_to_the_schema() {
    let script = parse("[[call get_weather]]").expect("a script");
    assert_eq!(script.steps, [call_step(&[("get_weather", None)])]);
    let script = parse("[[call  std.read-file  ]]").expect("a script");
    assert_eq!(script.steps, [call_step(&[("std.read-file", None)])]);
}

#[test]
fn arguments_that_end_in_nested_arrays_are_read_whole() {
    let script = parse(r#"[[call plot {"matrix":[[1,2],[3,4]]}]] then stop"#).expect("a script");
    assert_eq!(
        script.steps,
        [call_step(&[(
            "plot",
            Some(json!({ "matrix": [[1, 2], [3, 4]] }))
        )])]
    );
    assert_eq!(script.rest, " then stop");
}

#[test]
fn one_block_can_hold_several_calls() {
    let script = parse(r#"[[call a {"x":1}; call b; call c {}]]"#).expect("a script");
    assert_eq!(
        script.steps,
        [call_step(&[
            ("a", Some(json!({ "x": 1 }))),
            ("b", None),
            ("c", Some(json!({})))
        ])]
    );
}

#[test]
fn each_block_is_one_step_and_the_text_between_them_stays() {
    let script = parse("First [[call a]] then [[call b {\"n\":2}]] and [[say Done.]] please")
        .expect("a script");
    assert_eq!(
        script.steps,
        [
            call_step(&[("a", None)]),
            call_step(&[("b", Some(json!({ "n": 2 })))]),
            Step::Say("Done.".to_owned())
        ]
    );
    assert_eq!(script.rest, "First  then  and  please");
}

#[test]
fn other_text_in_double_brackets_is_ordinary_text() {
    for text in [
        "the matrix [[1,2],[3,4]] is square",
        "see [[the wiki page]] for more",
        "[[callback now]] and [[calls x]] and [[sayonara]]",
        "[[say]] and [[call]]",
        "[[ say hi]]",
    ] {
        let script = parse(text).expect("a script");
        assert_eq!(script.steps, [], "{text}");
        assert_eq!(script.rest, text);
    }
}

#[test]
fn a_block_that_opens_and_cannot_be_read_is_an_error() {
    for (text, reason) in [
        ("[[say never closed", "no closing `]]`"),
        ("[[call ]]", "names no tool"),
        ("[[call {\"a\":1}]]", "names no tool"),
        ("[[call f {\"a\":}]]", "are not a JSON object"),
        ("[[call f {\"a\":1} extra]]", "`]]` or `; call`"),
        ("[[call f; g]]", "`]]` or `; call`"),
        ("[[call f [1,2]]]", "`]]` or `; call`"),
        ("[[call f", "`]]` or `; call`"),
    ] {
        let error = parse(text).expect_err("an error");
        assert!(error.contains(reason), "{text}: {error}");
    }
}

#[test]
fn stripping_takes_the_blocks_out_and_leaves_a_broken_message_whole() {
    assert_eq!(strip("Hello [[say Hi]] there"), "Hello  there");
    assert_eq!(
        strip("Hello [[say never closed"),
        "Hello [[say never closed"
    );
    assert_eq!(strip("no blocks"), "no blocks");
}
