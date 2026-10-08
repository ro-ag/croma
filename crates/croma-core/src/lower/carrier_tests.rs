//! Compact carrier spelling (`[I:cr <code> …]`) expansion tests.

use super::{compact_code_for_long, expand_compact_carrier, unknown_directive_name};
use crate::options::is_croma_carrier_name;

#[test]
fn expands_direction_placement_values() {
    assert_eq!(
        expand_compact_carrier("cr dp=a").as_deref(),
        Some("croma-direction-placement placement=above")
    );
    assert_eq!(
        expand_compact_carrier("cr dp=b").as_deref(),
        Some("croma-direction-placement placement=below")
    );
}

#[test]
fn expands_flag_codes() {
    assert_eq!(
        expand_compact_carrier("cr mf").as_deref(),
        Some("croma-musicxml-forward")
    );
    assert_eq!(
        expand_compact_carrier("cr mr").as_deref(),
        Some("croma-meter-restatement")
    );
    assert_eq!(
        expand_compact_carrier("cr kr").as_deref(),
        Some("croma-key-restatement")
    );
    assert_eq!(
        expand_compact_carrier("cr htx").as_deref(),
        Some("croma-harmony-text textless=1")
    );
}

#[test]
fn passes_payload_fields_through_verbatim() {
    // Quoted free text and the `-hex=` variant are payload, not spelling: they
    // must survive expansion byte-for-byte.
    assert_eq!(
        expand_compact_carrier(r#"cr ht text="C7 alt""#).as_deref(),
        Some(r#"croma-harmony-text text="C7 alt""#)
    );
    assert_eq!(
        expand_compact_carrier("cr ht text-hex=4a6f").as_deref(),
        Some("croma-harmony-text text-hex=4a6f")
    );
    assert_eq!(
        expand_compact_carrier("cr le=3").as_deref(),
        Some("croma-lyric-extend verse=3")
    );
}

#[test]
fn expands_ending_close_value_tokens() {
    assert_eq!(
        expand_compact_carrier(r#"cr ec t=s l=r n="1""#).as_deref(),
        Some(r#"croma-ending-close type=stop location=right number="1""#)
    );
    assert_eq!(
        expand_compact_carrier(r#"cr ec t=d l=l n="1-2""#).as_deref(),
        Some(r#"croma-ending-close type=discontinue location=left number="1-2""#)
    );
}

#[test]
fn rejects_everything_that_is_not_a_registered_carrier() {
    // An unregistered code, a bare namespace, a namespace that is only a
    // prefix of another token, and a long-form carrier all yield None — the
    // caller then follows its existing paths.
    assert!(expand_compact_carrier("cr zz").is_none());
    assert!(expand_compact_carrier("cr").is_none());
    assert!(expand_compact_carrier("crx dp=a").is_none());
    assert!(expand_compact_carrier("croma-musicxml-forward").is_none());
    assert!(expand_compact_carrier("cr dp=x").is_none());
    // `n-hex=` has no ending-close arm on purpose (the writer's
    // `ending_number_value` only ever emits digits, `-`, and `,`, so a hex
    // ending label is unreachable by construction) — assert directly at the
    // expansion boundary that the arm stays absent, rather than through an
    // end-to-end `export_musicxml` check that would pass either way.
    assert!(expand_compact_carrier("cr ec t=s l=r n-hex=3132").is_none());
}

#[test]
fn compact_code_for_long_disambiguates_harmony_text() {
    // `croma-harmony-text` has two compact codes; the registry lists `htx`
    // first, so a naive first-match lookup would always report `htx`. The
    // textless value must report `htx`, and a value carrying real text/kind
    // fields must report `ht`.
    assert_eq!(
        compact_code_for_long("croma-harmony-text textless=1"),
        Some(("croma-harmony-text", "htx"))
    );
    assert_eq!(
        compact_code_for_long(r#"croma-harmony-text text="Cmaj7""#),
        Some(("croma-harmony-text", "ht"))
    );
}

#[test]
fn compact_code_for_long_covers_the_other_coded_carriers() {
    assert_eq!(
        compact_code_for_long("croma-direction-placement placement=above"),
        Some(("croma-direction-placement", "dp"))
    );
    assert_eq!(
        compact_code_for_long("croma-lyric-extend verse=1"),
        Some(("croma-lyric-extend", "le"))
    );
    assert_eq!(
        compact_code_for_long("croma-meter-restatement"),
        Some(("croma-meter-restatement", "mr"))
    );
    assert_eq!(
        compact_code_for_long("croma-key-restatement"),
        Some(("croma-key-restatement", "kr"))
    );
    assert_eq!(
        compact_code_for_long(r#"croma-ending-close type=stop location=right number="1""#),
        Some(("croma-ending-close", "ec"))
    );
    assert_eq!(
        compact_code_for_long("croma-musicxml-forward"),
        Some(("croma-musicxml-forward", "mf"))
    );
}

#[test]
fn compact_code_for_long_ignores_uncoded_and_unrelated_carriers() {
    // Uncoded long spellings (the other ~15 carriers) and the compact
    // spelling itself must never be reported as a deprecated long spelling.
    assert!(compact_code_for_long("croma-clef-cursor id=1").is_none());
    assert!(compact_code_for_long("croma-tempo bpm=120").is_none());
    assert!(compact_code_for_long("cr le=1").is_none());
    assert!(compact_code_for_long("croma-lyric-extend-suffix verse=1").is_none());
}

#[test]
fn unknown_directive_name_keeps_the_compact_code() {
    // The reported name for an unrecognised `[I:…]` is its directive token,
    // except in the compact namespace, where the code is kept so two unknown
    // carriers are not both reported as `cr`.
    assert_eq!(unknown_directive_name("cr zz"), "cr zz");
    assert_eq!(unknown_directive_name("cr zz=1"), "cr zz");
    assert_eq!(unknown_directive_name("cr dp=x more=1"), "cr dp");
    assert_eq!(unknown_directive_name("CR zz"), "CR zz");
    // No code to keep, and directives that merely share the letters: unchanged.
    assert_eq!(unknown_directive_name("cr"), "cr");
    assert_eq!(unknown_directive_name("credits x=1"), "credits");
    assert_eq!(unknown_directive_name("tuplets 3"), "tuplets");
    assert_eq!(unknown_directive_name("croma-future a=1"), "croma-future");
    assert_eq!(unknown_directive_name(""), "");
}

#[test]
fn reported_compact_names_are_recognised_as_croma_carriers() {
    // `unknown_directive_name` and `is_croma_carrier_name` must agree, or an
    // unknown compact carrier warns unsuppressibly. This is the join between
    // them.
    assert!(is_croma_carrier_name(&unknown_directive_name("cr zz=1")));
    assert!(is_croma_carrier_name(&unknown_directive_name(
        "croma-future"
    )));
    assert!(!is_croma_carrier_name(&unknown_directive_name(
        "credits x=1"
    )));
    assert!(!is_croma_carrier_name(&unknown_directive_name("cr")));
}

#[test]
fn legend_lists_exactly_the_registered_codes() {
    // `--legend` keeps its own table of display lines; it must name every
    // compact code the reader accepts and nothing else.
    use std::collections::BTreeSet;
    let registry: BTreeSet<&str> = super::COMPACT_CARRIERS
        .iter()
        .map(|(code, _)| *code)
        .collect();
    let legend: BTreeSet<&str> = crate::to_abc::LEGEND_LINES
        .iter()
        .map(|(code, _)| *code)
        .collect();
    assert_eq!(legend, registry);
}
