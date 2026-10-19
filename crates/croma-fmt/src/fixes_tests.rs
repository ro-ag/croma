use super::*;

fn opts() -> FormatOptions {
    FormatOptions::default()
}

#[test]
fn clean_input_yields_no_changes() {
    let result = auto_fix("X:1\nK:C\nCDE\n", opts());
    assert_eq!(result.output, crate::format("X:1\nK:C\nCDE\n", opts()));
    assert!(result.changes.is_empty());
    assert!(result.skipped.is_empty());
}

#[test]
fn joins_detached_length() {
    let result = auto_fix("X:1\nK:C\ng 2\n", opts());
    assert!(result.output.contains("g2"), "got: {:?}", result.output);
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].kind, FixKind::DetachedLength);
    // pitch sequence unchanged
    let base = pitch_seq_of("X:1\nK:C\ng 2\n", ParseOptions::default());
    let after = pitch_seq_of(&result.output, ParseOptions::default());
    assert_eq!(base, after);
}

#[test]
fn detached_accidental_is_never_touched() {
    // `^ g` would change G natural -> G#, a pitch change: must not be fixed.
    let result = auto_fix("X:1\nK:C\n^ g\n", opts());
    assert!(result.changes.is_empty(), "changes: {:?}", result.changes);
    assert!(result.output.contains("^ g"), "got: {:?}", result.output);
}

#[test]
fn unwraps_chord_symbol_in_brackets() {
    let result = auto_fix("X:1\nK:C\n[\"Cmaj\"abc]\n", opts());
    assert!(
        result.output.contains("\"Cmaj\"abc"),
        "got: {:?}",
        result.output
    );
    assert!(
        !result.output.contains("[\"Cmaj\""),
        "brackets remain: {:?}",
        result.output
    );
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].kind, FixKind::ChordSymbolInBrackets);
}

#[test]
fn collapses_doubled_tempo() {
    let result = auto_fix("X:1\nQ:1/4=1/4=160\nK:C\nC\n", opts());
    assert!(
        result.output.contains("Q:1/4=160"),
        "got: {:?}",
        result.output
    );
    assert!(
        !result.output.contains("1/4=1/4"),
        "still doubled: {:?}",
        result.output
    );
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].kind, FixKind::DoubledTempo);
}

#[test]
fn strips_bare_tempo_legacy_suffix() {
    let result = auto_fix("X:1\nQ:320s\nK:C\nC\n", opts());
    assert!(
        result.output.contains("Q:320\n"),
        "got: {:?}",
        result.output
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.kind == FixKind::BareTempoSuffix)
    );
}

#[test]
fn strips_bare_tempo_trailing_dot() {
    let result = auto_fix("X:1\nQ:400.\nK:C\nC\n", opts());
    assert!(
        result.output.contains("Q:400\n"),
        "got: {:?}",
        result.output
    );
}

#[test]
fn clean_bare_tempo_and_beat_spec_are_untouched() {
    for src in ["X:1\nQ:120\nK:C\nC\n", "X:1\nQ:1/4=120\nK:C\nC\n"] {
        let result = auto_fix(src, opts());
        assert!(
            !result
                .changes
                .iter()
                .any(|c| c.kind == FixKind::BareTempoSuffix),
            "should not fire on {src:?}"
        );
    }
}

#[test]
fn collapse_doubled_tempo_only_fires_on_exact_double() {
    assert_eq!(
        collapse_doubled_tempo("1/4=1/4=160"),
        Some("1/4=160".to_string())
    );
    assert_eq!(collapse_doubled_tempo("1/4=160"), None);
    assert_eq!(collapse_doubled_tempo("1/4=1/8=160"), None);
    assert_eq!(collapse_doubled_tempo("\"Allegro\""), None);
}

#[test]
fn output_is_idempotent_after_fixes() {
    let once = auto_fix("X:1\nK:C\ng 2\n", opts()).output;
    let twice = crate::format(&once, opts());
    assert_eq!(once, twice);
}

#[test]
fn collapses_spaced_double_barline() {
    let result = auto_fix("X:1\nL:1/4\nK:C\nCDE| |FGA\n", opts());
    assert!(
        result.output.contains("CDE|FGA"),
        "got: {:?}",
        result.output
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.kind == FixKind::RedundantBarline),
        "changes: {:?}",
        result.changes
    );
    // Structure gate: rendering identical.
    assert_eq!(
        musicxml_of("X:1\nL:1/4\nK:C\nCDE| |FGA\n", ParseOptions::default()),
        musicxml_of(&result.output, ParseOptions::default()),
    );
}

#[test]
fn collapses_thick_thin_repeat_run() {
    // `]||:` = a thick-thin final bar fused with a forward repeat (lowers to
    // [Final, RepeatStart]). The redundant `||` collapses away, but the `]`
    // light-heavy closer must survive: the canonical form is `|]:`, NOT a
    // bare `|:` (which would drop the final bar and fail the structure gate).
    let result = auto_fix("X:1\nL:1/4\nK:C\nab ]||: cd\n", opts());
    assert!(
        result.output.contains("ab |]: cd"),
        "got: {:?}",
        result.output
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.kind == FixKind::RedundantBarline)
    );
}

#[test]
fn keeps_real_double_bar_and_final_bar() {
    // `||` is a meaningful light-light double bar; `|]` a final bar. Neither
    // is redundant — the structure gate must leave them untouched.
    let double = auto_fix("X:1\nL:1/4\nK:C\nCDE||FGA\n", opts());
    assert!(
        double.output.contains("CDE||FGA"),
        "got: {:?}",
        double.output
    );
    assert!(
        !double
            .changes
            .iter()
            .any(|c| c.kind == FixKind::RedundantBarline)
    );

    let final_bar = auto_fix("X:1\nL:1/4\nK:C\nCDE|FGA|]\n", opts());
    assert!(
        final_bar.output.contains("|]"),
        "got: {:?}",
        final_bar.output
    );
}

#[test]
fn trims_space_after_field_colon() {
    let result = auto_fix("X:1\nT: My Tune\nK: C\nCDE\n", opts());
    assert!(
        result.output.contains("T:My Tune"),
        "got: {:?}",
        result.output
    );
    assert!(result.output.contains("K:C"), "got: {:?}", result.output);
    assert!(
        result
            .changes
            .iter()
            .filter(|c| c.kind == FixKind::FieldSpacing)
            .count()
            >= 2
    );
    // Structure gate: rendering identical.
    assert_eq!(
        musicxml_of("X:1\nT: My Tune\nK: C\nCDE\n", ParseOptions::default()),
        musicxml_of(&result.output, ParseOptions::default()),
    );
}

#[test]
fn field_spacing_does_not_overlap_doubled_tempo() {
    // `Q: 1/4=1/4=160` triggers both detectors; overlap resolution keeps the
    // whole-value tempo collapse, which already drops the leading space.
    let result = auto_fix("X:1\nQ: 1/4=1/4=160\nK:C\nC\n", opts());
    assert!(
        result.output.contains("Q:1/4=160"),
        "got: {:?}",
        result.output
    );
    assert!(
        !result.output.contains("1/4=1/4"),
        "got: {:?}",
        result.output
    );
}

#[test]
fn does_not_drop_a_real_boundary() {
    // `| |]` is a single bar then a final bar (two boundaries); collapsing to
    // `|]` would delete a boundary and change the rendering — gate reverts.
    let result = auto_fix("X:1\nL:1/4\nK:C\nCDE| |]\n", opts());
    assert_eq!(
        musicxml_of("X:1\nL:1/4\nK:C\nCDE| |]\n", ParseOptions::default()),
        musicxml_of(&result.output, ParseOptions::default()),
    );
}

#[test]
fn collapses_internal_midi_directive_whitespace() {
    // An active %%MIDI line with a multi-space argument run is canonicalized
    // to single spaces; the tokens are preserved.
    let result = auto_fix("X:1\n%%MIDI beat 97 87  77 4\nK:C\nCDE\n", opts());
    assert!(
        result.output.contains("%%MIDI beat 97 87 77 4"),
        "got: {:?}",
        result.output
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.kind == FixKind::MidiDirectiveSpacing),
        "changes: {:?}",
        result.changes
    );
}

#[test]
fn midi_spacing_preserves_comment_and_ignores_inert_tail() {
    // The comment tail is kept; the run before it collapses to one space.
    // An inert mid-line %%MIDI tail (not column 0) is never touched.
    let result = auto_fix(
        "X:1\n%%MIDI program 23  % Tango Accordion\nK:C %%MIDI gchordon\nCDE\n",
        opts(),
    );
    assert!(
        result
            .output
            .contains("%%MIDI program 23 % Tango Accordion"),
        "got: {:?}",
        result.output
    );
    assert!(
        result.output.contains("K:C %%MIDI gchordon"),
        "inert tail changed: {:?}",
        result.output
    );
}

#[test]
fn clean_midi_directive_is_untouched() {
    let result = auto_fix("X:1\n%%MIDI program 72\nK:C\nCDE\n", opts());
    assert!(
        !result
            .changes
            .iter()
            .any(|c| c.kind == FixKind::MidiDirectiveSpacing),
        "should not fire on a clean directive: {:?}",
        result.changes
    );
}

#[test]
fn directive_tokens_gate_allows_only_midi_arg_whitespace() {
    // Whitespace collapse inside an active %%MIDI argument region preserves
    // the token signature — the gate accepts it.
    assert!(directive_tokens_preserved(
        "X:1\n%%MIDI beat 97 87  77 4\nK:C\n",
        "X:1\n%%MIDI beat 97 87 77 4\nK:C\n",
    ));
    // A changed argument token is a content change — rejected.
    assert!(!directive_tokens_preserved(
        "X:1\n%%MIDI program 72\nK:C\n",
        "X:1\n%%MIDI program 73\nK:C\n",
    ));
    // A changed comment tail is a content change — rejected.
    assert!(!directive_tokens_preserved(
        "X:1\n%%MIDI program 72 % piano\nK:C\n",
        "X:1\n%%MIDI program 72 % organ\nK:C\n",
    ));
    // A change to a non-%%MIDI line is rejected (the fix only touches MIDI).
    assert!(!directive_tokens_preserved(
        "X:1\n%%MIDI program 72\nK:C\nCDE\n",
        "X:1\n%%MIDI program 72\nK:C\nCDF\n",
    ));
    // An inert mid-line `%%MIDI` tail (not column 0) is treated as a plain
    // line: collapsing whitespace around it is NOT permitted by this gate.
    assert!(!directive_tokens_preserved(
        "X:1\nK:C %%MIDI gchordon\n",
        "X:1\nK:C %%MIDI  gchordon\n",
    ));
}

// Carrier-registry tripwire: croma-fmt's compact codes must be ones croma-core
// reads, checked through croma-core's public API because its registry is
// private.

/// A minimal valid payload for each compact code. A code added to
/// `COMPACT_CARRIERS` without a payload here fails the test below, so the
/// tripwire cannot be skipped by accident.
fn sample_payload(code: &str) -> Option<&'static str> {
    Some(match code {
        "dp" => "dp=a",
        "ht" => "ht text=\"C\"",
        "le" => "le=1",
        "mr" => "mr",
        "kr" => "kr",
        "ec" => "ec t=s l=r n=\"1\"",
        "mf" => "mf",
        _ => return None,
    })
}

#[test]
fn every_compact_code_the_migration_writes_is_known_to_croma_core() {
    for (long, code) in COMPACT_CARRIERS {
        let payload = sample_payload(code)
            .unwrap_or_else(|| panic!("add a sample payload for `cr {code}` ({long})"));
        let source = format!("X:1\nL:1/4\nK:C\n[I:cr {payload}]C|\n");
        let export = croma_core::export_musicxml(&source).expect("exports");
        let unknown = export.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "abc.field.inline_ignored"
                && diagnostic.message.contains(&format!("cr {code}"))
        });
        assert!(!unknown, "`cr {code}` ({long}) is not a croma-core carrier");
    }
}

#[test]
fn an_unknown_code_is_reported_so_the_tripwire_can_fire() {
    let export = croma_core::export_musicxml("X:1\nL:1/4\nK:C\n[I:cr zz]C|\n").expect("exports");
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "abc.field.inline_ignored" && diagnostic.message.contains("cr zz")
    }));
}
