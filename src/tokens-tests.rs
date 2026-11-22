use super::*;

#[test]
fn hello_world_is_six_tokens() {
    assert_eq!(
        split("Hello, world!"),
        ["Hell", "o", ",", " worl", "d", "!"]
    );
    assert_eq!(count("Hello, world!"), 6);
}

#[test]
fn a_run_of_letters_and_digits_is_cut_every_four_characters() {
    assert_eq!(split("abcdefghi"), ["abcd", "efgh", "i"]);
    assert_eq!(split("ab12cd3"), ["ab12", "cd3"]);
    assert_eq!(split("abcd"), ["abcd"]);
}

#[test]
fn every_other_visible_character_is_one_token() {
    assert_eq!(split("a+b"), ["a", "+", "b"]);
    assert_eq!(split("..."), [".", ".", "."]);
    assert_eq!(split(r#"{"a":1}"#), ["{", "\"", "a", "\"", ":", "1", "}"]);
}

#[test]
fn whitespace_belongs_to_the_token_that_follows_it() {
    assert_eq!(split("to be"), ["to", " be"]);
    assert_eq!(split("  hi"), ["  hi"]);
    assert_eq!(split("a\n\n- b"), ["a", "\n\n-", " b"]);
}

#[test]
fn whitespace_at_the_end_belongs_to_the_last_token() {
    assert_eq!(split("hi  "), ["hi  "]);
    assert_eq!(split("yes. \n"), ["yes", ". \n"]);
    assert_eq!(split(" , "), [" , "]);
}

#[test]
fn text_that_is_only_whitespace_is_one_token_and_empty_text_is_none() {
    assert_eq!(split(" \n\t"), [" \n\t"]);
    assert_eq!(count(" "), 1);
    assert_eq!(split(""), Vec::<&str>::new());
    assert_eq!(count(""), 0);
}

#[test]
fn letters_of_any_script_count_four_to_a_token() {
    assert_eq!(
        split("h\u{e9}llo w\u{f6}rld"),
        ["h\u{e9}ll", "o", " w\u{f6}rl", "d"]
    );
    assert_eq!(
        split("\u{4f60}\u{597d}\u{4e16}\u{754c}\u{518d}\u{89c1}"),
        ["\u{4f60}\u{597d}\u{4e16}\u{754c}", "\u{518d}\u{89c1}"]
    );
}

#[test]
fn the_tokens_joined_together_are_the_text() {
    for text in [
        "Nice to meet you, Sam. What can I help with?",
        "  leading and trailing  ",
        "tabs\tand\r\nline ends\n",
        r#"{"tool":"read_file","lines":[1,2,3]}"#,
        "\u{1f600} emoji \u{1f600}\u{1f600}",
        "",
    ] {
        let tokens = split(text);
        assert_eq!(tokens.concat(), text);
        assert_eq!(count(text), tokens.len() as u64);
        assert!(tokens.iter().all(|token| !token.is_empty()), "{tokens:?}");
    }
}

#[test]
fn a_cut_keeps_the_first_tokens_and_reports_text_that_fits() {
    assert_eq!(cut("Nice to meet you", 2), Some("Nice to"));
    assert_eq!(cut("Hello", 1), Some("Hell"));
    assert_eq!(cut("Nice to", 2), None);
    assert_eq!(cut("Nice", 5), None);
    assert_eq!(cut("", 1), None);
    assert_eq!(cut("anything", u64::MAX), None);
}
