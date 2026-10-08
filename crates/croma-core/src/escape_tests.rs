//! Quoted-value unescaping tests.

use super::unescape_quoted;

#[test]
fn undoes_the_writer_escapes() {
    assert_eq!(unescape_quoted(r#"say \"hi\""#), r#"say "hi""#);
    assert_eq!(unescape_quoted(r"a\\b"), r"a\b");
}

#[test]
fn keeps_other_backslash_sequences_and_a_trailing_backslash() {
    // ABC 2.1 §8.2 mnemonics stay intact for whoever renders them.
    assert_eq!(unescape_quoted(r"Caf\'e"), r"Caf\'e");
    assert_eq!(unescape_quoted(r"\ss"), r"\ss");
    assert_eq!(unescape_quoted(r"end\"), r"end\");
}

#[test]
fn a_character_is_escaped_by_an_odd_run_of_backslashes() {
    assert!(super::is_escaped(r#"a\""#, 2));
    assert!(!super::is_escaped(r#"a\\""#, 3));
    assert!(super::is_escaped(r#"a\\\""#, 4));
    assert!(!super::is_escaped(r#"""#, 0));
}
