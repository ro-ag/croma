//! Totality regressions: every input here once panicked, hung or aborted, or is
//! an extreme value the audit confirmed safe. Each goes through every core
//! entry point (parse, lower, MusicXML with and without the engraving profile,
//! and the ABC writer); the test passes when none of them panics. `Z4294967295` is
//! covered by `multi_measure_rest_expansion_is_capped_with_a_warning`: exporting
//! the capped 10,000 measures here would cost seconds per run.

use crate::{
    AbcWriteOptions, ExportOptions, LowerOptions, ParseOptions, export_musicxml,
    export_musicxml_with_options, lower_score, parse_document, write_abc,
};

/// Bodies placed after `X:1` and before `K:C`/the music, keyed by what they hit.
const HEADER_CASES: &[&str] = &[
    // Extreme unit lengths and meters.
    "L:1/0",
    "L:1/99999999999",
    "L:1/499999999",
    "M:4/0",
    "M:0/0",
    "M:268435457/4",
    "M:4294967295/4",
    // Tempo values and beat sums.
    "Q:1/4=99999999999",
    "Q:1/65536 1/65536=120",
    "Q:1/4294967291 1/4294967279=120",
];

const MUSIC_CASES: &[&str] = &[
    // Key mode words with multi-byte letters, header and inline.
    "[K:Cmé]C|",
    "[K:Gdór]C|",
    // Lengths and broken rhythm past u32 ratios.
    "A99999999999",
    "A/0",
    "A4294967295B/4294967295",
    "A>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>B",
    "C2D2&E2",
    // Tuplets: zero, huge, zero q, and deep nesting.
    "(0ABC",
    "(99999999999ABC",
    "(3:0:3ABC",
    "(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1A|",
    // Huge clef and transposition properties.
    "[K:C octave=2147483647]C|",
    "[K:C transpose=-2147483648]C|",
    "[K:C middle=c'''''''''''''''''''''''''''''''']C|",
];

fn exercise(source: &str) {
    let document = parse_document(source, ParseOptions::default()).value;
    if let Some(score) = lower_score(&document, LowerOptions).value {
        let _ = write_abc(&score, AbcWriteOptions::default());
    }
    let _ = export_musicxml(source);
    let _ = export_musicxml_with_options(source, ExportOptions::default().engrave());
}

#[test]
fn extreme_inputs_never_panic_in_any_core_entry_point() {
    exercise("X:1\nK:Cmé\nC|\n");
    exercise("X:1\nK:Cmaé\nC|\n");
    for header in HEADER_CASES {
        exercise(&format!("X:1\n{header}\nK:C\nCDEF GABc|\n"));
    }
    for music in MUSIC_CASES {
        exercise(&format!("X:1\nM:4/4\nL:1/8\nK:C\n{music}\n"));
        exercise(&format!("X:1\nM:none\nL:1/8\nK:C\n{music}\n"));
    }
}
