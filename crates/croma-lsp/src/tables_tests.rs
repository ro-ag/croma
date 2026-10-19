
use super::*;

#[test]
fn field_keys_are_unique_and_complete() {
    // Every letter in the ABC 2.1 common-field set is present exactly once.
    let expected = "XTCOAMLQPZNGHKRBDFSIVWwmrs";
    for letter in expected.chars() {
        assert!(field_key(letter).is_some(), "missing field {letter}");
    }
    // No duplicate letters.
    let mut seen = std::collections::HashSet::new();
    for field in FIELD_KEYS {
        assert!(
            seen.insert(field.letter),
            "duplicate field {}",
            field.letter
        );
    }
    assert_eq!(seen.len(), expected.chars().count());
}

#[test]
fn field_insert_and_markdown_are_well_formed() {
    let k = field_key('K').expect("K is a field");
    assert_eq!(k.insert_text(), "K:");
    let md = k.markdown();
    assert!(md.contains("key"), "doc mentions key: {md}");
    assert!(md.contains("§3.1.14"), "doc has the section ref");
}

#[test]
fn decorations_cover_every_name_the_core_maps() {
    // The table is built from croma_core::musicxml::DECORATION_NAMES: every
    // name the writer maps gets hover and completion. A name added to the
    // core list without a row here fails this test.
    for name in croma_core::musicxml::DECORATION_NAMES {
        assert!(decoration(name).is_some(), "missing decoration {name}");
    }
    // And nothing is advertised that the core does not map.
    for d in DECORATIONS {
        assert!(
            croma_core::musicxml::DECORATION_NAMES.contains(&d.name),
            "{} is not a core decoration",
            d.name
        );
    }
    // No duplicate names.
    let mut seen = std::collections::HashSet::new();
    for d in DECORATIONS {
        assert!(seen.insert(d.name), "duplicate decoration {}", d.name);
    }
}

#[test]
fn shorthand_table_matches_core_canonical_names() {
    // Each shorthand we advertise must resolve to the SAME canonical name
    // croma_core's `parse::music::shorthand_canonical_name` produces (that fn
    // is crate-private, so the mapping is mirrored here; ABC 2.1 §4.14). If
    // the core map ever changes, this fixture is the tripwire.
    let core_map = [
        ('~', "roll"),
        ('H', "fermata"),
        ('L', "accent"),
        ('M', "lowermordent"),
        ('O', "coda"),
        ('P', "uppermordent"),
        ('S', "segno"),
        ('T', "trill"),
        ('u', "upbow"),
        ('v', "downbow"),
    ];
    for (symbol, canonical) in core_map {
        let ours = decoration_for_shorthand(symbol)
            .unwrap_or_else(|| panic!("table has shorthand {symbol}"));
        assert_eq!(ours.name, canonical, "shorthand {symbol} canonical name");
    }
}

#[test]
fn decoration_markdown_notes_the_shorthand() {
    let trill = decoration("trill").expect("trill");
    let md = trill.markdown();
    assert!(
        md.contains("shorthand `T`"),
        "trill md notes shorthand: {md}"
    );
    assert!(md.contains("§4.14"));
}
