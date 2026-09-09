//! Compact carrier spelling (`[I:cr <code> …]`) expansion tests.

use super::expand_compact_carrier;

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
}
