use super::*;
use crate::{ParseOptions, export_musicxml, parse_document};

#[test]
fn simple_score_emits_partwise_and_core_attributes() {
    let export =
        export_musicxml("X:1\nT:Scale\nM:6/8\nL:1/8\nK:G\nC2 z x|\n").expect("score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<score-partwise version=\"4.0\">"));
    assert!(export.musicxml.contains("<score-part id=\"P1\">"));
    assert!(export.musicxml.contains("<part id=\"P1\">"));
    assert!(export.musicxml.contains("<part-name>Scale</part-name>"));
    assert!(export.musicxml.contains("<divisions>8</divisions>"));
    assert!(export.musicxml.contains("<fifths>1</fifths>"));
    assert!(export.musicxml.contains("<beats>6</beats>"));
    assert!(export.musicxml.contains("<beat-type>8</beat-type>"));
    assert!(export.musicxml.contains("<sign>G</sign>"));
    assert!(export.musicxml.contains("<duration>8</duration>"));
    assert!(export.musicxml.contains("<type>quarter</type>"));
    assert!(export.musicxml.contains("<type>eighth</type>"));
    assert!(export.musicxml.contains("<note print-object=\"no\">"));
}

#[test]
fn text_output_is_escaped_for_metadata_lyrics_harmony_and_directions() {
    let source = concat!(
        "X:1\n",
        "T:A&B <T> \"Q\" 'R'\n",
        "C:Comp & < > \" '\n",
        "Q:Fast & < > \" '\n",
        "L:1/8\n",
        "K:C\n",
        "\"G7&<>'\"\"^Ann & < > '\"C D|\n",
        "w: lyr&<>' two\n",
        "%%foo dir & < > \" '\n",
    );
    let export = export_musicxml(source).expect("escaped text score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        export
            .musicxml
            .contains("A&amp;B &lt;T&gt; &quot;Q&quot; &apos;R&apos;")
    );
    assert!(
        export
            .musicxml
            .contains("Comp &amp; &lt; &gt; &quot; &apos;")
    );
    assert!(
        export
            .musicxml
            .contains("Fast &amp; &lt; &gt; &quot; &apos;")
    );
    // `G7&<>'` has junk after the quality token, so (like abc2xml) it is
    // not a recognised chord symbol and is emitted as escaped words.
    assert!(export.musicxml.contains("G7&amp;&lt;&gt;&apos;"));
    assert!(export.musicxml.contains("Ann &amp; &lt; &gt; &apos;"));
    assert!(export.musicxml.contains("lyr&amp;&lt;&gt;&apos;"));
    // Preserved `%%` stylesheet directives are playback/formatting only and
    // are not rendered as printed words, so `%%foo` must not leak out.
    assert!(!export.musicxml.contains("%%foo"));
}

#[test]
fn slash_chord_symbols_export_bass_step_and_alter() {
    // `Db/Ab` uses `b` as the root/bass flat (abc2xml's chord accidental);
    // `-` is reserved for the minor quality and is not a flat.
    let source = "X:1\nT:Slash Chords\nM:4/4\nL:1/4\nK:C\n\"C/E\"C \"Db/Ab\"D|\n";
    let export = export_musicxml(source).expect("slash chords should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<root-step>C</root-step>"));
    assert!(export.musicxml.contains("<bass-step>E</bass-step>"));
    assert!(export.musicxml.contains("<root-step>D</root-step>"));
    assert!(export.musicxml.contains("<root-alter>-1</root-alter>"));
    assert!(export.musicxml.contains("<bass-step>A</bass-step>"));
    assert!(export.musicxml.contains("<bass-alter>-1</bass-alter>"));
}

#[test]
fn malformed_quoted_chord_strings_export_as_words_not_fake_harmony() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"(A7)\"C \"C/\"D|\n";
    let export = export_musicxml(source).expect("malformed chord text should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 0);
    assert_eq!(count(&export.musicxml, "<root-step>"), 0);
    assert!(export.musicxml.contains("<words>(A7)</words>"));
    assert!(export.musicxml.contains("<words>C/</words>"));
}

#[test]
fn leading_whitespace_valid_chord_symbols_still_export_harmony() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"  G7\"C \" C/E\"D|\n";
    let export = export_musicxml(source).expect("valid spaced chords should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 2);
    assert!(export.musicxml.contains("<root-step>G</root-step>"));
    assert!(export.musicxml.contains("<bass-step>E</bass-step>"));
    assert!(!export.musicxml.contains("<words>G7</words>"));
    assert!(!export.musicxml.contains("<words>C/E</words>"));
}

#[test]
fn chord_symbol_before_barline_binds_to_next_note() {
    // ABC 2.1 §4.18: a chord symbol applies to the note it precedes. A barline
    // between the symbol and its note is a measure boundary, not a void:
    // `"F"| c` must keep the F harmony (bound to the c across the bar).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC4 \"F\"| c4 |]\n";
    let export = export_musicxml(source).expect("chord symbol at barline should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains("<root-step>F</root-step>"));
}

#[test]
fn chord_symbol_at_line_end_binds_to_note_on_next_line() {
    // A code line break is not a musical boundary (§6.1.1): `"Em7"` at the end
    // of one music line binds to the first note of the next line.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D \"Em7\"\nE F|\n";
    let export = export_musicxml(source).expect("line-end chord symbol should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains("<root-step>E</root-step>"));
    assert!(
        export
            .musicxml
            .contains("<kind text=\"Em7\">minor-seventh</kind>")
    );
}

#[test]
fn annotation_alone_on_line_binds_to_first_following_note() {
    // An annotation on a line of its own (tune_000377's `"Single Reel"`)
    // positions relative to the following note (§4.19) and must not be lost.
    let source = "X:1\nM:C|\nK:G\n\"Single Reel\"\nB2 e2 g2 f2|\n";
    let export = export_musicxml(source).expect("standalone annotation line should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<words>Single Reel</words>"));
}

#[test]
fn chord_symbol_before_multimeasure_rest_is_kept() {
    // `"Dm"Z2` — the symbol precedes a multi-measure rest; it binds to that
    // rest event, not to nothing.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Dm\"Z2 | C4 |]\n";
    let export = export_musicxml(source).expect("chord before multirest should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains("<root-step>D</root-step>"));
}

#[test]
fn multimeasure_rest_exports_real_full_measure_rests() {
    // ABC 2.1 §4.5: `Z2` means two measures of rest. MusicXML has no legal
    // one-measure overlong rest encoding; write real measures and use
    // `<multiple-rest>` only as display metadata.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|Z2|GABc|]\n";
    let export = export_musicxml(source).expect("multirest should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<measure number="), 4);
    assert_eq!(count(&export.musicxml, "<rest measure=\"yes\"/>"), 2);
    assert!(export.musicxml.contains("<multiple-rest>2</multiple-rest>"));
    assert!(!export.musicxml.contains("<time-modification>"));
}

#[test]
fn dangling_quoted_text_at_tune_end_warns_instead_of_silent_drop() {
    // Quoted text with no following timed event cannot bind to anything; it
    // must surface as a diagnostic, never vanish silently.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC4 \"X7\" |]\n";
    let export = export_musicxml(source).expect("dangling quoted text should still export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 0);
    assert!(
        export
            .diagnostics
            .iter()
            .any(|d| d.code == "abc.music.dangling_quoted_text"),
        "expected dangling-quoted-text diagnostic, got: {:?}",
        export
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn tuplet_member_written_type_factors_out_the_time_modification() {
    // ABC 2.1 §4.13: a (4 quadruplet in 6/8 plays 4 notes in the time of 3.
    // The WRITTEN type is the de-tupletted duration (eighth + 4:3), not a
    // re-spelling of the sounding duration (dotted 16th + 4:3, internally
    // inconsistent — tune_005074 family).
    let source = "X:1\nM:6/8\nL:1/8\nK:C\nCCC (4BABd|\n";
    let export = export_musicxml(source).expect("quadruplet should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<type>eighth</type>"), 7);
    assert!(!export.musicxml.contains("<type>16th</type>"));
    assert_eq!(count(&export.musicxml, "<actual-notes>4</actual-notes>"), 4);

    // Broken rhythm inside a triplet: `(3F>GG` — the lengthened F is a
    // dotted eighth under 3:2, not a dotless eighth.
    let source = "X:1\nM:4/4\nL:1/8\nK:C\n(3F>GG ABcd z2|\n";
    let export = export_musicxml(source).expect("broken triplet should export");
    assert_balanced_xml(&export.musicxml);
    let f_note = export
        .musicxml
        .split("<step>F</step>")
        .nth(1)
        .expect("F note present");
    let f_note = &f_note[..f_note.find("</note>").expect("note closes")];
    assert!(f_note.contains("<type>eighth</type>"));
    assert!(
        f_note.contains("<dot/>"),
        "lengthened F keeps its dot: {f_note}"
    );
}

#[test]
fn very_long_durations_spell_as_long_and_maxima() {
    // MusicXML note-type-value includes long (4 wholes) and maxima (8); croma
    // capped at breve, mis-typing early-music note values (tune_000386
    // family, 31 files).
    let source = "X:1\nM:none\nL:1/4\nK:C\nC16 C32|\n";
    let export = export_musicxml(source).expect("long values should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<type>long</type>"));
    assert!(export.musicxml.contains("<type>maxima</type>"));
}

#[test]
fn chord_member_slurs_attach_to_their_chords() {
    // ABC 2.1 §4.11: "Both ties and slurs may be used into, out of and
    // between chords". `[(C2(E2] [C2)E2)]` opens two slurs on the first
    // chord and closes both on the second (tune_011938/011866/004626
    // family) — croma dropped them with unknown-chord-token warnings.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n[(C2(E2] [C2)E2)]|\n";
    let export = export_musicxml(source).expect("chord-member slurs should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<slur"), 4);
    assert_eq!(count(&export.musicxml, "type=\"start\""), 2);
    assert_eq!(count(&export.musicxml, "type=\"stop\""), 2);
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|d| d.code == "abc.music.unknown_chord_token")
    );
}

#[test]
fn lowercase_root_quoted_text_is_words_not_harmony() {
    // ABC 2.1 §4.18: the chord root is A-G (uppercase); only the bass note
    // may be lowercase. "d" is annotation text, not a D chord (40 corpus
    // files). A lowercase BASS after / stays a chord.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"d\"C D \"D/f\"E F|\n";
    let export = export_musicxml(source).expect("lowercase root should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains("<words>d</words>"));
    assert!(export.musicxml.contains("<bass-step>F</bass-step>"));
}

#[test]
fn body_tempo_field_emits_metronome_direction() {
    // ABC 2.1 §3.1.8 and the field table allow Q: in the tune body
    // (tune_007548 family, ~120 files): a `Q:` line after K: must produce a
    // <metronome> direction, not vanish silently.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nQ:1/4=132\nCDEF|GABc|]\n";
    let export = export_musicxml(source).expect("body tempo should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<per-minute>132</per-minute>"));
    assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
}

#[test]
fn mid_tune_tempo_change_lands_at_its_measure() {
    // A tempo change between music lines positions at the point of change,
    // not at the start of the tune.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nQ:1/4=100\nCDEF|\nQ:1/4=160\nGABc|]\n";
    let export = export_musicxml(source).expect("mid-tune tempo should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<per-minute>100</per-minute>"));
    assert!(export.musicxml.contains("<per-minute>160</per-minute>"));
    let measure2 = export
        .musicxml
        .split("<measure number=\"2\">")
        .nth(1)
        .expect("measure 2 exists");
    assert!(measure2.contains("<per-minute>160</per-minute>"));
}

#[test]
fn liberal_barline_runs_keep_their_strongest_meaning() {
    // ABC 2.1 §4.8: "bar lines may have any shape, using a sequence of
    // |, [, ] and :". Croma recognized such runs as boundaries but erased
    // their meaning from the output (tune_012890/004928/003884 families).
    // A `]`-bearing run is a final (thin-thick) bar; leading repeat dots are
    // a backward repeat; a `|:`-leading run is a forward repeat.
    let final_run =
        export_musicxml("X:1\nM:4/4\nL:1/4\nK:C\nCDEF|GABc||]\n").expect("||] should export");
    assert_balanced_xml(&final_run.musicxml);
    assert!(
        final_run
            .musicxml
            .contains("<bar-style>light-heavy</bar-style>")
    );

    let leading_dots = export_musicxml("X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|GABc:[|]|]\n")
        .expect(":[|]|] should export");
    assert_balanced_xml(&leading_dots.musicxml);
    assert!(
        leading_dots
            .musicxml
            .contains("<repeat direction=\"backward\"/>")
    );

    let sandwich = export_musicxml("X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF:|\n|:|GABc:|\n")
        .expect("|:| should export");
    assert_balanced_xml(&sandwich.musicxml);
    assert_eq!(
        count(&sandwich.musicxml, "<repeat direction=\"forward\"/>"),
        2
    );
}

#[test]
fn trailing_colon_after_final_bar_is_a_repeat_start_not_a_phantom_measure() {
    // `|]:` before a new section (tune_012511 family): the colon is the
    // bar's trailing repeat dots — a forward repeat for what follows, never
    // a separate one-colon measure.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|]:GABc|]\n";
    let export = export_musicxml(source).expect("|]: should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<measure number="), 2);
    assert_eq!(
        count(&export.musicxml, "<repeat direction=\"forward\"/>"),
        1
    );
}

#[test]
fn lone_colon_between_notes_is_a_liberal_measure_boundary() {
    // `CDEF:GABc|`: §4.8's liberal-recognition guidance covers colon runs;
    // dropping the boundary mangled the measure structure (71 files). The
    // boundary is kept (3 measures), warned, and carries no repeat glyph.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF:GABc|cdef|]\n";
    let export = export_musicxml(source).expect("lone colon should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<measure number="), 3);
    assert!(!export.musicxml.contains("<repeat"));
    assert!(
        export
            .diagnostics
            .iter()
            .any(|d| d.code == "abc.music.barline.liberal")
    );
}

#[test]
fn multi_measure_volta_emits_ending_stop_at_closing_barline() {
    // ABC 2.1 §4.9: "The Nth ending starts with [N and ends with one of
    // ||, :| |] or [|" — endings legally span multiple measures. The
    // MusicXML bracket must close: an <ending type="start"> with no stop is
    // a dangling bracket (tune_008287 family, 905 affected files).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|[1 GABc|GGGG:|[2 cdef|]\n";
    let export = export_musicxml(source).expect("multi-measure volta should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        export
            .musicxml
            .contains("<ending number=\"1\" type=\"start\"/>")
    );
    assert!(
        export
            .musicxml
            .contains("<ending number=\"1\" type=\"stop\"/>")
    );
    assert!(
        export
            .musicxml
            .contains("<ending number=\"2\" type=\"start\"/>")
    );
    assert!(
        export
            .musicxml
            .contains("<ending number=\"2\" type=\"stop\"/>")
    );
}

#[test]
fn volta_unclosed_in_source_stays_open_at_part_end() {
    // ABC 2.1 §4.9 closes an ending at `||`, `:|`, `|]` or `[|`. When the
    // source never writes one (the tune just ends on a plain bar), no stop
    // is synthesized: fabricating a closing barline the source does not have
    // regressed 25 previously-matching corpus files. A closing bar later in
    // the source still stops the bracket (see
    // multi_measure_volta_emits_ending_stop_at_closing_barline).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|[1 GABc:|[2 cdef|\n";
    let export = export_musicxml(source).expect("volta to part end should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        export
            .musicxml
            .contains("<ending number=\"2\" type=\"start\"/>")
    );
    assert!(
        !export
            .musicxml
            .contains("<ending number=\"2\" type=\"stop\"/>")
    );
}

#[test]
fn ending_precedes_repeat_inside_barline_element() {
    // MusicXML's barline content model orders <ending> before <repeat>; the
    // reverse is schema-invalid.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|[1 GABc:|[2 cdef|]\n";
    let export = export_musicxml(source).expect("volta should export");

    assert_balanced_xml(&export.musicxml);
    let barline = export
        .musicxml
        .split("<barline location=\"right\">")
        .find(|chunk| chunk.contains("<ending number=\"1\" type=\"stop\"/>"))
        .expect("ending-1 stop barline exists");
    let barline = &barline[..barline.find("</barline>").expect("closed")];
    let ending_pos = barline.find("<ending").expect("ending present");
    let repeat_pos = barline.find("<repeat").expect("repeat present");
    assert!(
        ending_pos < repeat_pos,
        "ending must precede repeat, got: {barline}"
    );
}

#[test]
fn dynamic_decoration_before_barline_or_line_end_is_kept() {
    // `!f!` at the end of a line with its note across the barline/line break
    // (tune_004792 family): ABC 2.1 §4.14 binds a decoration to the following
    // decorated symbol; the boundary does not void it.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F !f!|\nG A B c|\n";
    let export = export_musicxml(source).expect("dynamic at barline should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<f/>"), 1);
}

#[test]
fn grace_group_before_barline_attaches_to_note_across_the_bar() {
    // `{e/}|d3`: ABC 2.1 §4.12 defines no void-at-barline rule for graces and
    // §4.20 orders a grace before the note it decorates — here that note is
    // across the bar. The transcriber's grace must not be silently lost
    // (tune_014161 family).
    let source = "X:1\nL:1/8\nM:6/8\nK:D\nGB2Af2{e/}|d3D2z|]\n";
    let export = export_musicxml(source).expect("grace at barline should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<grace"), 1);
    // The grace lands in measure 2, before the principal d.
    let measure2 = export
        .musicxml
        .split("<measure number=\"2\">")
        .nth(1)
        .expect("measure 2 exists");
    assert!(measure2.contains("<grace"));
}

#[test]
fn dangling_grace_group_at_tune_end_warns_instead_of_silent_drop() {
    // A grace group with no following note has nothing to decorate; it must
    // surface as a diagnostic, never vanish silently.
    let source = "X:1\nL:1/8\nK:C\nC4 {ab}|]\n";
    let export = export_musicxml(source).expect("dangling grace should still export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<grace"), 0);
    assert!(
        export
            .diagnostics
            .iter()
            .any(|d| d.code == "abc.music.dangling_grace_group"),
        "expected dangling-grace diagnostic, got: {:?}",
        export
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn chord_symbol_survives_legacy_plus_decoration() {
    // `"F"+>+a4`: the deprecated `+...+` decoration syntax (ABC 2.0 §10.2.2)
    // between the chord symbol and its note must not destroy the symbol —
    // same silent-data-loss family as the fixed grace/slur/tuplet flushes
    // (tune_006965 family).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"F\"+>+a4|\n";
    let export = export_musicxml(source).expect("chord before +decoration+ should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains("<root-step>F</root-step>"));
}

#[test]
fn crescendo_and_diminuendo_decorations_emit_wedges_not_words() {
    // ABC 2.1 (lines 1114-1121): !crescendo(!/!<(! open a hairpin,
    // !crescendo)!/!<)! close it (same for diminuendo). These are wedge
    // marks, not printable text — emitting the raw name as <words> mangles
    // the notation.
    let source = concat!(
        "X:1\n",
        "M:4/4\n",
        "L:1/4\n",
        "K:C\n",
        "!<(!C D !<)!E F|!diminuendo(!G A !diminuendo)!B c|\n",
    );
    let export = export_musicxml(source).expect("wedge decorations should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<wedge type=\"crescendo\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<wedge type=\"diminuendo\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<wedge type=\"stop\"/>"), 2);
    assert!(!export.musicxml.contains("<words><(</words>"));
    assert!(!export.musicxml.contains("<words>diminuendo(</words>"));
}

#[test]
fn plus_decoration_emits_stopped_technical_not_words() {
    // ABC 2.1 (line 1101): !+! is left-hand pizzicato — a note-attached
    // technical mark (MusicXML <stopped/>, the + glyph), not visible text.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n!+!C D E F|\n";
    let export = export_musicxml(source).expect("plus decoration should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<stopped/>"), 1);
    assert!(!export.musicxml.contains("<words>+</words>"));
}

#[test]
fn fingering_arpeggio_and_slide_decorations_emit_notations_not_words() {
    // ABC 2.1 §4.14 defines !0!-!5! as fingerings, !arpeggio! as a vertical
    // squiggle, and !slide! as a slide up to a note. These are note-attached
    // notations, not detached direction words.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n!3!C !arpeggio![CEG] !slide!D E|\n";
    let export = export_musicxml(source).expect("decoration notations should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<fingering>3</fingering>"), 1);
    assert_eq!(count(&export.musicxml, "<arpeggiate"), 1);
    assert_eq!(count(&export.musicxml, "<scoop/>"), 1);
    for text in ["3", "arpeggio", "slide"] {
        assert!(
            !export.musicxml.contains(&format!("<words>{text}</words>")),
            "{text} decoration should not be emitted as words"
        );
    }
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.musicxml.decoration.unsupported"),
        "supported decorations should not produce unsupported-decoration diagnostics"
    );
}

#[test]
fn chord_qualities_map_to_musicxml_kinds_matching_abc2xml() {
    // Each chord symbol must classify to the same <kind> abc2xml emits, so
    // music21 re-renders identical figures from <kind> (it ignores text=).
    let cases = [
        ("F#dim", "diminished"),
        ("Cdim7", "diminished-seventh"),
        ("Caug", "augmented"),
        ("C+", "augmented"),
        ("Co", "diminished"),
        ("C-", "minor"),
        ("Dsus4", "suspended-fourth"),
        ("Dsus2", "suspended-second"),
        ("Csus", "suspended-fourth"),
        ("Cmaj7", "major-seventh"),
        ("CM7", "major-seventh"),
        ("Cm6", "minor-sixth"),
        ("C6", "major-sixth"),
        ("Cm7", "minor-seventh"),
        ("C7", "dominant"),
        ("Cm", "minor"),
        ("C", "major"),
        // Ninth / eleventh / thirteenth kinds.
        ("C9", "dominant-ninth"),
        ("Cmaj9", "major-ninth"),
        ("Cm9", "minor-ninth"),
        ("C11", "dominant-11th"),
        ("Cm11", "minor-11th"),
        ("C13", "dominant-13th"),
        ("Cmaj13", "major-13th"),
        ("Cm13", "minor-13th"),
        // Half-diminished.
        ("Cm7b5", "half-diminished"),
        ("Cmin7b5", "minor-seventh"),
        // Suspended after a seventh keeps only the first kind token.
        ("C7sus4", "dominant"),
        ("Cmaj7sus4", "major-seventh"),
    ];
    for (symbol, expected_kind) in cases {
        let source = format!("X:1\nM:4/4\nL:1/4\nK:C\n\"{symbol}\"C4|\n");
        let export =
            export_musicxml(&source).unwrap_or_else(|_| panic!("chord {symbol} should export"));
        assert_balanced_xml(&export.musicxml);
        let expected = format!("<kind text=\"{symbol}\">{expected_kind}</kind>");
        assert!(
            export.musicxml.contains(&expected),
            "chord {symbol} should map to {expected_kind}; got:\n{}",
            export.musicxml
        );
    }
}

#[test]
fn power_chord_exports_major_kind_with_add_fifth_degree() {
    // abc2xml has no `power` quality: `A5` parses as a major triad with a
    // trailing `5` chord degree, emitted as an added fifth.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"A5\"C4|\n";
    let export = export_musicxml(source).expect("power chord should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<root-step>A</root-step>"));
    assert!(export.musicxml.contains(">major</kind>"));
    assert!(export.musicxml.contains("<degree-value>5</degree-value>"));
    assert!(export.musicxml.contains("<degree-alter>0</degree-alter>"));
    assert!(export.musicxml.contains("<degree-type>add</degree-type>"));
}

#[test]
fn altered_trailing_degrees_export_as_add_with_alter() {
    // Trailing chord degrees with a `#`/`b` accidental become added degrees
    // with the corresponding alter, matching abc2xml exactly.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"C7b9#5\"C4|\n";
    let export = export_musicxml(source).expect("altered chord should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains(">dominant</kind>"));
    assert!(export.musicxml.contains("<degree-value>9</degree-value>"));
    assert!(export.musicxml.contains("<degree-alter>-1</degree-alter>"));
    assert!(export.musicxml.contains("<degree-value>5</degree-value>"));
    assert!(export.musicxml.contains("<degree-alter>1</degree-alter>"));
    assert_eq!(count(&export.musicxml, "<degree>"), 2);
}

#[test]
fn add_and_omit_word_chords_export_as_words_not_harmony() {
    // abc2xml's chord grammar has no `add`/`no` tokens, so these symbols do
    // not parse as harmony at all and fall through to plain text.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cadd9\"C \"C9no3\"D \"Cadd11\"E|\n";
    let export = export_musicxml(source).expect("word chords should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 0);
    assert!(export.musicxml.contains("<words>Cadd9</words>"));
    assert!(export.musicxml.contains("<words>C9no3</words>"));
    assert!(export.musicxml.contains("<words>Cadd11</words>"));
}

#[test]
fn double_accidental_and_garbage_roots_are_not_harmony() {
    // abc2xml accepts only a single root accidental and rejects unparsable
    // tails, so these fall through to words rather than fake harmony.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cbb\"C \"C##\"D \"Cx\"E \"NC\"F|\n";
    let export = export_musicxml(source).expect("garbage chords should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 0);
    assert!(export.musicxml.contains("<words>Cbb</words>"));
    assert!(export.musicxml.contains("<words>C##</words>"));
    assert!(export.musicxml.contains("<words>Cx</words>"));
    assert!(export.musicxml.contains("<words>NC</words>"));
}

#[test]
fn parenthesized_chord_suffix_is_suppressed() {
    // A trailing parenthesized group is dropped: `C(no3)` is a plain major.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"C(no3)\"C4|\n";
    let export = export_musicxml(source).expect("parenthesized chord should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 1);
    assert!(export.musicxml.contains(">major</kind>"));
    assert_eq!(count(&export.musicxml, "<degree>"), 0);
}

#[test]
fn slash_chord_with_quality_keeps_kind_and_bass() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cm7/Bb\"C4|\n";
    let export = export_musicxml(source).expect("slash chord should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains(">minor-seventh</kind>"));
    assert!(export.musicxml.contains("<bass-step>B</bass-step>"));
    assert!(export.musicxml.contains("<bass-alter>-1</bass-alter>"));
}

#[test]
fn tempo_beat_equals_bpm_emits_metronome() {
    let source = "X:1\nM:4/4\nL:1/4\nQ:1/4=104\nK:C\nC4|\n";
    let export = export_musicxml(source).expect("tempo score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<metronome>"));
    assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
    assert!(export.musicxml.contains("<per-minute>104</per-minute>"));
    assert!(export.musicxml.contains("<sound tempo=\"104.00\""));
    assert!(!export.musicxml.contains("<words>1/4=104</words>"));
}

#[test]
fn tempo_dotted_beat_unit_emits_metronome_dot() {
    let source = "X:1\nM:4/4\nL:1/4\nQ:3/8=100\nK:C\nC4|\n";
    let export = export_musicxml(source).expect("tempo score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
    assert!(export.musicxml.contains("<beat-unit-dot"));
    assert!(export.musicxml.contains("<per-minute>100</per-minute>"));
    assert!(export.musicxml.contains("<sound tempo=\"150.00\""));
    assert!(!export.musicxml.contains("<words>3/8=100</words>"));
}

#[test]
fn tempo_bare_number_uses_unit_note_length() {
    let source = "X:1\nM:4/4\nL:1/8\nQ:120\nK:C\nC4|\n";
    let export = export_musicxml(source).expect("tempo score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<beat-unit>eighth</beat-unit>"));
    assert!(export.musicxml.contains("<per-minute>120</per-minute>"));
    assert!(export.musicxml.contains("<sound tempo=\"60.00\""));
    assert!(!export.musicxml.contains("<words>120</words>"));
}

#[test]
fn tempo_text_only_stays_words_with_default_sound() {
    let source = "X:1\nM:4/4\nL:1/4\nQ:\"Slow\"\nK:C\nC4|\n";
    let export = export_musicxml(source).expect("tempo score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<words>Slow</words>"));
    assert!(export.musicxml.contains("<sound tempo=\"120.00\""));
    assert_eq!(count(&export.musicxml, "<metronome>"), 0);
}

#[test]
fn tempo_text_plus_beat_emits_words_and_metronome() {
    let source = "X:1\nM:4/4\nL:1/4\nQ:\"allegretto\" 1/4=110\nK:C\nC4|\n";
    let export = export_musicxml(source).expect("tempo score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<words>allegretto</words>"));
    assert!(export.musicxml.contains("<metronome>"));
    assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
    assert!(export.musicxml.contains("<per-minute>110</per-minute>"));
    assert!(export.musicxml.contains("<sound tempo=\"110.00\""));
}

#[test]
fn midi_directives_are_not_emitted_as_words() {
    // `%%MIDI` (and other preserved `%%` stylesheet directives) control
    // playback/formatting, not printed musical text. abc2xml emits nothing for
    // them; Croma must not render them as visible <words> directions. A real
    // `Q:` tempo and the actual notes must still be present, proving we
    // suppressed only the directive, not genuine content.
    let source = concat!(
        "X:1\n",
        "T:Test\n",
        "M:C\n",
        "L:1/8\n",
        "Q:1/4=104\n",
        "K:G\n",
        "%%MIDI program 72\n",
        "%%MIDI channel 1\n",
        "CDEF GABc |\n",
    );
    let export = export_musicxml(source).expect("score with MIDI directives should export");

    assert_balanced_xml(&export.musicxml);
    // No preserved `%%`-derived directive may leak out as <words>.
    assert!(!export.musicxml.contains("%%MIDI"));
    assert!(!export.musicxml.contains("%%"));
    assert!(!export.musicxml.contains("<words>"));
    // Real content survives: the `Q:` tempo metronome and the notes.
    assert!(export.musicxml.contains("<metronome>"));
    assert!(export.musicxml.contains("<per-minute>104</per-minute>"));
    assert!(export.musicxml.contains("<step>C</step>"));
    assert!(export.musicxml.contains("<step>G</step>"));
}

#[test]
fn placement_prefixed_annotations_remain_words() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"^slow\"C \"_soft\"D|\n";
    let export = export_musicxml(source).expect("annotations should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony>"), 0);
    assert!(export.musicxml.contains("<direction placement=\"above\">"));
    assert!(export.musicxml.contains("<direction placement=\"below\">"));
    assert!(export.musicxml.contains("<words>slow</words>"));
    assert!(export.musicxml.contains("<words>soft</words>"));
}

#[test]
fn initial_barlines_do_not_emit_musicxml_heavy_light() {
    let source = "X:1\nT:Initial Barline\nM:4/4\nL:1/4\nK:C\nC |[| D |]\n";
    let export = export_musicxml(source).expect("initial barline should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        !export
            .musicxml
            .contains("<bar-style>heavy-light</bar-style>")
    );
    assert!(
        export
            .musicxml
            .contains("<bar-style>light-heavy</bar-style>")
    );
}

#[test]
fn leading_plain_barline_exports_notes_in_measure_one() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n| C D E F |]\n";
    let export = export_musicxml(source).expect("leading barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
    assert!(!measures[0].notes.iter().any(|note| note.rest));
}

#[test]
fn leading_repeat_start_exports_left_repeat_without_empty_measure() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|: C D E F :|\n";
    let export = export_musicxml(source).expect("leading repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
}

#[test]
fn leading_double_and_final_barlines_do_not_create_empty_measure() {
    for prefix in ["||", "|]"] {
        let source = format!("X:1\nM:4/4\nL:1/4\nK:C\n{prefix} C D E F |]\n");
        let export = export_musicxml(&source).expect("leading section barline should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"], "{prefix}");
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
        assert!(
            !measures[0]
                .barlines
                .iter()
                .any(|barline| barline.location == "left")
        );
        assert!(has_barline(
            &measures[0],
            "right",
            Some("light-heavy"),
            None
        ));
    }
}

#[test]
fn leading_liberal_barline_diagnoses_and_keeps_measure_timing() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n[::] C D E F |]\n";
    let export = export_musicxml(source).expect("liberal leading barline should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.music.barline.liberal",
        "[::]",
    );
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
    assert!(!measures[0].notes.iter().any(|note| note.rest));
}

#[test]
fn leading_double_repeat_start_exports_left_repeat_without_empty_measure() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n||: C D E F :||\n";
    let export = export_musicxml(source).expect("combined leading repeat should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
    );
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
}

#[test]
fn repeat_end_double_after_notes_exports_right_repeat_and_new_measure() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F :|| G A B c |]\n";
    let export = export_musicxml(source).expect("combined repeat end should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
}

#[test]
fn repeat_both_between_sections_exports_right_then_left_repeat() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F :||: G A B c |]\n";
    let export = export_musicxml(source).expect("repeat-both barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
}

#[test]
fn triple_repeat_extensions_export_repeat_edges() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:: C D E F ::|\n";
    let export = export_musicxml(source).expect("triple repeat barlines should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
    assert!(
        measures[0]
            .barlines
            .iter()
            .all(|barline| barline.repeat_times.is_none())
    );
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn excessive_repeat_dots_resolve_to_their_unambiguous_direction() {
    // `|:::` opens a repeat and `:::|` closes one — the dot COUNT is the
    // unclear part (a play-count hint croma does not model), not the
    // direction. Erasing the repeat entirely lost playback structure; the
    // liberal-run classifier now keeps the direction.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|::: C D E F :::|\n";
    let export = export_musicxml(source).expect("liberal repeat dots should recover");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(
        count(&export.musicxml, "<repeat direction=\"forward\"/>"),
        1
    );
    assert_eq!(
        count(&export.musicxml, "<repeat direction=\"backward\"/>"),
        1
    );
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn trailing_split_barline_does_not_emit_phantom_empty_measure() {
    let source = "X:1\nL:1/8\nK:C\nCDEF| |\n";
    let export = export_musicxml(source).expect("trailing split barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn trailing_thin_thick_then_thin_barline_does_not_emit_phantom_measure() {
    let source = "X:1\nL:1/8\nK:C\nCDEF|]|\n";
    let export = export_musicxml(source).expect("trailing |]| should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn trailing_split_barline_with_space_does_not_emit_phantom_measure() {
    let source = "X:1\nL:1/8\nK:C\nCDEF| | \n";
    let export = export_musicxml(source).expect("trailing split barline with space should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn split_barline_between_content_is_a_single_boundary() {
    let source = "X:1\nL:1/8\nK:C\nCDEF| | GABc|\n";
    let export = export_musicxml(source).expect("interior split barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
}

#[test]
fn continued_section_leading_double_barline_does_not_close_empty_measure() {
    // tune_006306 style: a pickup note uses a suppressed line break, then the
    // continued physical line starts a section with `||`; the next section
    // also starts with `||` after the previous one closed with `||`. That
    // second section marker keeps an empty section-leading measure, but must
    // not be reclassified as a right double barline on it.
    let source = "X:1\nM:3/4\nL:1/4\nK:Am\nE \\\n|| AA>c | e3 ||\n|| aa>a | a3 ||\n";
    let export = export_musicxml(source).expect("continued section double should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(
        measure_numbers(&measures),
        vec!["1", "2", "3", "4", "5", "6"]
    );
    assert_eq!(note_steps(&measures[0]), vec!['E']);
    assert_eq!(note_steps(&measures[1]), vec!['A', 'A', 'C']);
    assert_eq!(note_steps(&measures[2]), vec!['E']);
    assert!(measures[3].notes.is_empty());
    assert!(
        measures[3].barlines.is_empty(),
        "section-leading || empty measure must not carry barlines: {:?}",
        measures[3].barlines
    );
    assert_eq!(note_steps(&measures[4]), vec!['A', 'A', 'A']);
    assert_eq!(note_steps(&measures[5]), vec!['A']);
}

#[test]
fn thick_barline_then_repeat_start_run_is_a_single_boundary() {
    let source = "X:1\nL:1/8\nM:C\nK:C\nCDEF]||: GA |\n";
    let export = export_musicxml(source).expect("]||: barline run should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A']);
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
}

#[test]
fn thick_barline_run_after_second_ending_does_not_emit_phantom_measure() {
    let source = "X:1\nL:1/8\nM:C\nK:C\n[1 C2:|[2 C3]||: GA |\n";
    let export = export_musicxml(source).expect("second-ending ]||: run should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert_eq!(note_steps(&measures[0]), vec!['C']);
    assert_eq!(note_steps(&measures[1]), vec!['C']);
    assert_eq!(note_steps(&measures[2]), vec!['G', 'A']);
    assert!(has_barline(&measures[2], "left", None, Some("forward")));
}

#[test]
fn continued_thick_barline_run_after_second_ending_does_not_emit_phantom_measure() {
    let source = "X:1\nL:1/8\nM:C\nK:C\n[1 C2:|[2 C3]\\\n||: GA |\n";
    let export = export_musicxml(source).expect("continued second-ending ]||: run should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert_eq!(note_steps(&measures[0]), vec!['C']);
    assert_eq!(note_steps(&measures[1]), vec!['C']);
    assert_eq!(note_steps(&measures[2]), vec!['G', 'A']);
    assert!(has_barline(&measures[2], "left", None, Some("forward")));
}

#[test]
fn double_barline_repeat_start_between_content_is_a_single_boundary() {
    let source = "X:1\nL:1/8\nK:C\nCDEF||: GABc|\n";
    let export = export_musicxml(source).expect("||: repeat-start should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
}

#[test]
fn mid_tune_final_then_repeat_start_does_not_emit_phantom_measure() {
    let source = "X:1\nL:1/8\nK:C\nCDEF|]\n|: GABc :|\n";
    let export = export_musicxml(source).expect("final then repeat-start should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
    assert!(has_barline(&measures[1], "right", None, Some("backward")));
}

#[test]
fn contiguous_double_barline_stays_single_measure() {
    let source = "X:1\nL:1/8\nK:C\nCDEF||\n";
    let export = export_musicxml(source).expect("double barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn separate_content_lines_keep_two_measures() {
    let source = "X:1\nL:1/8\nK:C\nCDEF|\nGABc|\n";
    let export = export_musicxml(source).expect("two content lines should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
}

#[test]
fn leading_rest_measure_is_preserved_not_phantom() {
    let source = "X:1\nL:1/8\nK:C\nz4|CDEF|\n";
    let export = export_musicxml(source).expect("leading rest measure should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert!(measures[0].notes.iter().any(|note| note.rest));
    assert_eq!(note_steps(&measures[1]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn consecutive_leading_barlines_keep_single_empty_pickup_measure() {
    // abc2xml preserves exactly one leading empty (pickup) measure; the extra
    // bar lines coalesce into the same boundary rather than opening phantoms.
    let source = "X:1\nL:1/8\nK:C\n| | | |CDEF|\n";
    let export = export_musicxml(source).expect("leading bar-line run should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert!(!measures[0].notes.iter().any(|note| note.rest));
    assert!(measures[0].notes.is_empty());
    assert_eq!(note_steps(&measures[1]), vec!['C', 'D', 'E', 'F']);
}

#[test]
fn multi_voice_tacet_barline_only_measure_is_kept_aligned() {
    // In a multi-voice score, a voice line that is only a bar line is a
    // legitimate *tacet* bar: the voice rests through that measure but must keep
    // an empty measure so it stays measure-aligned with its siblings (this is
    // exactly the corpus pattern `[V:4]  |` / `[V:2] [|]`). The single-voice
    // phantom-collapse must NOT fire here: with the over-aggressive
    // (ungated) bar-line-only coalescing, V2's trailing `| |` tacet measure is
    // wrongly popped and V2 shrinks to 2 measures, breaking alignment. The
    // single-voice gate keeps it at 3.
    //
    // Each `[V:n] ...` line continues that voice. V2's final continuation line
    // is a bare bar-line run (`| |`) — a bar-line-only trailing measure that the
    // `finish()` trailing-pop would remove in single-voice music, but must be
    // preserved here.
    let source = concat!(
        "X:1\nM:2/4\nL:1/8\nK:C\n",
        "[V:1] CDEF |\n[V:2] EFGA |\n",
        "[V:1] GABc |\n[V:2] EDCB, |\n",
        "[V:1] cBAG |\n[V:2] | |\n",
    );
    let export = export_musicxml(source).expect("multi-voice tacet bar should export");
    assert_balanced_xml(&export.musicxml);

    let parts = part_bodies(&export.musicxml);
    assert_eq!(parts.len(), 2, "expected two voices/parts");

    let v1_measures = musicxml_measures(&parts[0]);
    let v2_measures = musicxml_measures(&parts[1]);
    assert_eq!(
        v1_measures.len(),
        3,
        "V1 should have three measures: {:?}",
        measure_numbers(&v1_measures)
    );
    assert_eq!(
        v2_measures.len(),
        v1_measures.len(),
        "V2 tacet bar must be kept so voices stay measure-aligned: {:?} vs {:?}",
        measure_numbers(&v2_measures),
        measure_numbers(&v1_measures)
    );
    // The trailing (third) tacet measure of V2 carries no sounding notes.
    assert!(
        v2_measures[2].notes.iter().all(|note| note.rest) || v2_measures[2].notes.is_empty(),
        "V2 trailing measure should be a tacet bar, not real notes"
    );
}

#[test]
fn multi_voice_empty_final_measure_keeps_final_barline() {
    // A final continuation line that is only `|]` still explicitly notates a
    // final bar for that voice. Keep the empty measure and its right barline.
    let source = concat!(
        "X:1\nM:2/4\nL:1/8\nK:C\n",
        "[V:1] CDEF |\n[V:2] EFGA |\n",
        "[V:1] GABc |\n[V:2] |]\n",
    );
    let export = export_musicxml(source).expect("multi-voice empty final bar should export");
    assert_balanced_xml(&export.musicxml);

    let parts = part_bodies(&export.musicxml);
    assert_eq!(parts.len(), 2, "expected two voices/parts");

    let v1_measures = musicxml_measures(&parts[0]);
    let v2_measures = musicxml_measures(&parts[1]);
    assert_eq!(v1_measures.len(), 2);
    assert_eq!(v2_measures.len(), 2);
    assert!(v2_measures[1].notes.is_empty());
    assert!(
        has_barline(&v2_measures[1], "right", Some("light-heavy"), None),
        "V2 empty final measure should retain its explicit |] right barline: {:?}",
        v2_measures[1].barlines
    );
}

#[test]
fn words_containing_double_colon_pipe_do_not_emit_repeat_barlines() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F |]\nW::| Cross over two couples\n";
    let export = export_musicxml(source).expect("words field should not affect barlines");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert!(has_barline(
        &measures[0],
        "right",
        Some("light-heavy"),
        None
    ));
    assert!(
        measures[0]
            .barlines
            .iter()
            .all(|barline| barline.repeat_direction.is_none())
    );
}

#[test]
fn tune_014868_style_leading_double_repeat_has_no_empty_measure() {
    let source = concat!(
        "X:260\n",
        "T:Bag o' Spuds -- Am\n",
        "M:4/4\n",
        "R:Reel\n",
        "K:Am\n",
        "||:\"Am\"A2eA BAeA|ABcd ecdB|\"G\"G2BG DGBG|GABc \"Em\"dBcB|\n",
        "\"Am\"A2eA BAeA|ABcd \"G\"ecdB|\"F\"ABcd efge|\"Em\"dBGB BAA2:|\n",
        "|:\"Am\"a2ea ageg|agbg agef|\"G\"gedc BGBd|g2ga bgeg|\n",
        "\"Am\"a2ea ageg|agbg ageg|\"G\"d3e g3e|\"Em\"dBGB BAA2:|\n",
    );
    let export = export_musicxml(source).expect("leading combined repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(
        measure_numbers(&measures),
        (1..=16)
            .map(|number| number.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        note_steps(&measures[0]),
        vec!['A', 'E', 'A', 'B', 'A', 'E', 'A']
    );
    assert_eq!(note_durations(&measures[0]), vec![8, 4, 4, 4, 4, 4, 4]);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[7], "right", None, Some("backward")));
    assert!(has_barline(&measures[8], "left", None, Some("forward")));
    assert!(has_barline(&measures[15], "right", None, Some("backward")));
    assert!(!measures[0].notes.iter().any(|note| note.rest));
}

#[test]
fn bracketed_repeat_start_and_final_repeat_end_export_repeat_edges() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n[|: C D E F :|]\n";
    let export = export_musicxml(source).expect("bracketed repeat barlines should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[0], "right", None, Some("backward")));
}

#[test]
fn pickup_repeat_start_places_forward_repeat_on_repeated_section() {
    // ABC 2.1 §6: `|:` after a pickup marks the START of the repeated
    // section, so the forward repeat belongs to the LEFT of measure 2
    // (`CDEF`), not the pickup measure 1 (`E`).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nE|:CDEF|GABc:|]\n";
    let export = export_musicxml(source).expect("pickup repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert_eq!(note_steps(&measures[0]), vec!['E']);
    assert_eq!(note_steps(&measures[1]), vec!['C', 'D', 'E', 'F']);
    assert!(
        !has_barline(&measures[0], "left", None, Some("forward")),
        "pickup measure must not carry the forward repeat"
    );
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
    assert!(has_barline(&measures[2], "right", None, Some("backward")));
}

#[test]
fn mid_tune_repeat_start_places_forward_repeat_on_repeated_section() {
    // `|:` after content mid-tune marks the start of the repeated section:
    // forward repeat belongs to the LEFT of measure 3 (`cBAG`).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|GABc|:cBAG|FEDC:|]\n";
    let export = export_musicxml(source).expect("mid-tune repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3", "4"]);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert_eq!(note_steps(&measures[2]), vec!['C', 'B', 'A', 'G']);
    assert!(
        !has_barline(&measures[1], "left", None, Some("forward")),
        "measure preceding the repeat must not carry the forward repeat"
    );
    assert!(has_barline(&measures[2], "left", None, Some("forward")));
    assert!(has_barline(&measures[3], "right", None, Some("backward")));
}

#[test]
fn leading_repeat_start_after_header_stays_on_its_own_measure() {
    // Non-regression: a `|:` with no preceding content in its measure is a
    // legitimate LEFT barline of measure 1 and must stay there.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|GABc:|]\n";
    let export = export_musicxml(source).expect("leading repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert!(has_barline(&measures[0], "left", None, Some("forward")));
    assert!(has_barline(&measures[1], "right", None, Some("backward")));
}

#[test]
fn double_then_repeat_start_after_content_defers_forward_repeat() {
    // `||:` after content (`Double` + `RepeatStart`) must not drop the
    // forward repeat and must place it on the measure beginning the body.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF||:GABc|cBAG:|]\n";
    let export = export_musicxml(source).expect("double-then-repeat should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert!(
        !has_barline(&measures[0], "left", None, Some("forward")),
        "first measure must not carry the forward repeat"
    );
    assert!(has_barline(&measures[1], "left", None, Some("forward")));
    assert!(has_barline(&measures[2], "right", None, Some("backward")));
}

#[test]
fn section_final_barline_followed_by_regular_is_preserved() {
    // Bug C: `|]` (light-heavy section barline) immediately followed by `|`
    // must be kept as the RIGHT barline of its measure (`GABc`).
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|GABc|]|cBAG|FEDC|]\n";
    let export = export_musicxml(source).expect("section final barline should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    assert!(
        has_barline(&measures[1], "right", Some("light-heavy"), None),
        "the section final barline after measure 2 must be preserved"
    );
}

#[test]
fn trailing_section_final_then_regular_spelling_keeps_final_barline() {
    let source = "X:1\nL:1/8\nK:C\nCDEF|]|\n";
    let export = export_musicxml(source).expect("trailing |]| should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert!(has_barline(
        &measures[0],
        "right",
        Some("light-heavy"),
        None
    ));
}

#[test]
fn adjacent_repeat_end_and_second_ending_starts_next_measure() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n|: C D E F |1 G A B c :|2 D E F G |]\n";
    let export = export_musicxml(source).expect("adjacent repeat ending should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert!(has_ending(&measures[1], "left", "1", "start"));
    assert!(has_ending(&measures[1], "right", "1", "stop"));
    assert!(has_ending(&measures[2], "left", "2", "start"));
    assert!(has_ending(&measures[2], "right", "2", "stop"));
    assert!(has_barline(&measures[1], "right", None, Some("backward")));
}

#[test]
fn internal_rest_measure_is_preserved_after_leading_barline_policy() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F | z4 | G A B c |]\n";
    let export = export_musicxml(source).expect("internal rest measure should export");

    assert_balanced_xml(&export.musicxml);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
    assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    assert_eq!(measures[1].notes.len(), 1);
    assert!(measures[1].notes[0].rest);
    assert_eq!(measures[1].notes[0].duration, Some(32));
    assert_eq!(note_steps(&measures[2]), vec!['G', 'A', 'B', 'C']);
}

#[test]
fn chords_grace_tuplets_ties_slurs_and_lyrics_export() {
    let source =
        "X:1\nT:Features\nM:4/4\nL:1/8\nK:C\n{g}[CEG] (3D-D F (G A)|\nw: chord trip let slur end\n";
    let export = export_musicxml(source).expect("feature score should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<chord/>"), 2);
    assert!(export.musicxml.contains("<grace/>"));
    assert!(export.musicxml.contains("<time-modification>"));
    assert!(export.musicxml.contains("<actual-notes>3</actual-notes>"));
    assert!(export.musicxml.contains("<normal-notes>2</normal-notes>"));
    assert!(export.musicxml.contains("<tuplet type=\"start\""));
    assert!(export.musicxml.contains("<tuplet type=\"stop\""));
    assert!(export.musicxml.contains("<tie type=\"start\"/>"));
    assert!(export.musicxml.contains("<tied type=\"start\""));
    assert!(export.musicxml.contains("<slur type=\"start\""));
    assert!(export.musicxml.contains("<slur type=\"stop\""));
    assert!(export.musicxml.contains("<text>chord</text>"));
}

#[test]
fn slur_opening_before_grace_group_starts_on_first_grace_note() {
    // ABC 2.1 §4.11 + §4.20: in `({grace}note)` the slur opens before the grace
    // group, so it must start on the FIRST grace note, not the following main
    // note. The main note carries only the matching stop.
    let source = "X:1\nT:Grace Slur\nM:4/4\nL:1/4\nK:C\n({g}c4)|\n";
    let export = export_musicxml(source).expect("grace-slur score should export");

    assert_balanced_xml(&export.musicxml);

    // The grace note carries the slur start inside its own notations.
    let grace_note_start = export
        .musicxml
        .find("<grace/>")
        .expect("grace note should be present");
    let grace_note_end = export.musicxml[grace_note_start..]
        .find("</note>")
        .map(|offset| grace_note_start + offset)
        .expect("grace note should be terminated");
    let grace_note = &export.musicxml[grace_note_start..grace_note_end];
    assert!(
        grace_note.contains("<notations>"),
        "grace note should carry notations: {grace_note}"
    );
    assert!(
        grace_note.contains("<slur type=\"start\" number=\"1\"/>"),
        "grace note should carry the slur start: {grace_note}"
    );

    // There is exactly one slur start and one slur stop overall (no degenerate
    // start+stop on the main note).
    assert_eq!(
        count(&export.musicxml, "<slur type=\"start\" number=\"1\"/>"),
        1
    );
    assert_eq!(
        count(&export.musicxml, "<slur type=\"stop\" number=\"1\"/>"),
        1
    );

    // The slur stop lands on the main note, which is NOT a grace note.
    let stop_index = export
        .musicxml
        .find("<slur type=\"stop\" number=\"1\"/>")
        .expect("slur stop should be present");
    let main_note_start = export.musicxml[..stop_index]
        .rfind("<note")
        .expect("slur stop should be inside a note");
    let main_note = &export.musicxml[main_note_start..stop_index];
    assert!(
        !main_note.contains("<grace/>"),
        "slur stop should land on the main note, not a grace note: {main_note}"
    );
    assert!(
        !main_note.contains("<slur type=\"start\""),
        "main note must not carry a degenerate slur start: {main_note}"
    );
}

#[test]
fn slur_opening_after_a_grace_starts_on_main_note_not_grace() {
    // Discrimination guard: `{g}c(de)` — the grace `{g}` leads note `c`, but the
    // slur opens AFTER it, before `d`. The slurred note `d` has NO leading grace
    // group, so the slur must start on `d` and the grace `G` must stay clean.
    // This proves the span check only re-targets a grace the slur actually
    // encloses (the `({grace}note)` form in
    // `slur_opening_before_grace_group_starts_on_first_grace_note`).
    let source = "X:1\nT:Grace Then Slur\nM:4/4\nL:1/4\nK:C\n{g}c(de)|\n";
    let export = export_musicxml(source).expect("grace-then-slur score should export");

    assert_balanced_xml(&export.musicxml);

    // The grace G carries no slur.
    let grace_note_start = export
        .musicxml
        .find("<grace/>")
        .expect("grace note should be present");
    let grace_note_end = export.musicxml[grace_note_start..]
        .find("</note>")
        .map(|offset| grace_note_start + offset)
        .expect("grace note should be terminated");
    let grace_note = &export.musicxml[grace_note_start..grace_note_end];
    assert!(
        !grace_note.contains("<slur"),
        "grace note must not carry a slur when the slur opens after it: {grace_note}"
    );

    // The slur start lands on a non-grace main note (D).
    let start_index = export
        .musicxml
        .find("<slur type=\"start\" number=\"1\"/>")
        .expect("slur start should be present");
    let main_note_start = export.musicxml[..start_index]
        .rfind("<note")
        .expect("slur start should be inside a note");
    let main_note = &export.musicxml[main_note_start..start_index];
    assert!(
        !main_note.contains("<grace/>"),
        "slur start should land on the main note: {main_note}"
    );
    assert!(
        export
            .musicxml
            .contains("<slur type=\"stop\" number=\"1\"/>")
    );
}

#[test]
fn slur_without_grace_still_spans_first_to_last_note() {
    // Guard: a plain `(DEF)` slur still spans D->F unchanged, and a plain grace
    // note with no slur emits no notations.
    let source = "X:1\nT:Plain Slur\nM:4/4\nL:1/4\nK:C\n(DEF) {g}A|\n";
    let export = export_musicxml(source).expect("plain slur score should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(
        count(&export.musicxml, "<slur type=\"start\" number=\"1\"/>"),
        1
    );
    assert_eq!(
        count(&export.musicxml, "<slur type=\"stop\" number=\"1\"/>"),
        1
    );

    // The lone grace note (the `{g}` before `A`) carries no notations/slur.
    let grace_note_start = export
        .musicxml
        .find("<grace/>")
        .expect("grace note should be present");
    let grace_note_end = export.musicxml[grace_note_start..]
        .find("</note>")
        .map(|offset| grace_note_start + offset)
        .expect("grace note should be terminated");
    let grace_note = &export.musicxml[grace_note_start..grace_note_end];
    assert!(
        !grace_note.contains("<notations>") && !grace_note.contains("<slur"),
        "plain grace note should carry no notations: {grace_note}"
    );
}

#[test]
fn grace_before_slur_open_attaches_to_main_note_and_slur_starts_on_main() {
    // ABC 2.1 §4.20: in `{g}(c4)` the grace group precedes the note `c`, so it
    // attaches to `c`. The slur `(` opens AFTER the grace, so the slur starts on
    // the MAIN note `c`, not on the grace. Before the fix the intervening slur
    // flushed the grace into a standalone item that lowering dropped, losing the
    // grace entirely.
    let source = "X:1\nT:Grace Before Slur\nM:4/4\nL:1/8\nK:C\n{g}(c4) d|\n";
    let export = export_musicxml(source).expect("grace-before-slur score should export");

    assert_balanced_xml(&export.musicxml);

    // The grace note is preserved.
    let grace_note_start = export
        .musicxml
        .find("<grace/>")
        .expect("grace note should be present");
    let grace_note_end = export.musicxml[grace_note_start..]
        .find("</note>")
        .map(|offset| grace_note_start + offset)
        .expect("grace note should be terminated");
    let grace_note = &export.musicxml[grace_note_start..grace_note_end];

    // The grace carries no slur — the slur opens after it.
    assert!(
        !grace_note.contains("<slur"),
        "grace note must not carry a slur when the slur opens after it: {grace_note}"
    );

    // Exactly one slur start/stop overall.
    assert_eq!(
        count(&export.musicxml, "<slur type=\"start\" number=\"1\"/>"),
        1
    );
    assert_eq!(
        count(&export.musicxml, "<slur type=\"stop\" number=\"1\"/>"),
        1
    );

    // The slur start lands on the main note `c`, which is NOT a grace note.
    let start_index = export
        .musicxml
        .find("<slur type=\"start\" number=\"1\"/>")
        .expect("slur start should be present");
    let main_note_start = export.musicxml[..start_index]
        .rfind("<note")
        .expect("slur start should be inside a note");
    let main_note = &export.musicxml[main_note_start..start_index];
    assert!(
        !main_note.contains("<grace/>"),
        "slur start should land on the main note, not the grace: {main_note}"
    );
}

#[test]
fn grace_before_slur_open_does_not_regress_slur_before_grace() {
    // Phase-18 guard: `({g}c4)` keeps the slur start on the FIRST grace note. The
    // grace-before-slur fix must not disturb this path.
    let source = "X:1\nT:Slur Before Grace\nM:4/4\nL:1/4\nK:C\n({g}c4) d|\n";
    let export = export_musicxml(source).expect("slur-before-grace score should export");

    assert_balanced_xml(&export.musicxml);

    let grace_note_start = export
        .musicxml
        .find("<grace/>")
        .expect("grace note should be present");
    let grace_note_end = export.musicxml[grace_note_start..]
        .find("</note>")
        .map(|offset| grace_note_start + offset)
        .expect("grace note should be terminated");
    let grace_note = &export.musicxml[grace_note_start..grace_note_end];
    assert!(
        grace_note.contains("<slur type=\"start\" number=\"1\"/>"),
        "slur opening before the grace should start on the grace note: {grace_note}"
    );
}

#[test]
fn grace_before_plain_note_still_attaches() {
    // Control: `{g}c4 d` (no slur) keeps the grace on `c` and emits no slur.
    let source = "X:1\nT:Plain Grace\nM:4/4\nL:1/8\nK:C\n{g}c4 d|\n";
    let export = export_musicxml(source).expect("plain grace score should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<grace/>"), 1);
    assert!(!export.musicxml.contains("<slur"));
}

#[test]
fn chord_symbol_before_grace_group_emits_harmony() {
    // `"F"{AB}c4`: the chord symbol written before the grace group binds to
    // the main note `c` and exports as a <harmony>; before the fix the first
    // grace note inside the braces stole the pending quoted text and lowering
    // silently dropped it.
    let source = "X:1\nT:Harmony Before Grace\nM:4/4\nL:1/8\nK:C\n\"F\"{AB}c4 d|\n";
    let export = export_musicxml(source).expect("harmony-before-grace score should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<harmony"), 1);
    assert_eq!(count(&export.musicxml, "<grace/>"), 2);
}

#[test]
fn grace_orphaned_before_barline_is_voided_without_panic() {
    // A grace flushed at a barline now carries across the bar to the next note
    // (`c {g}| d` decorates the d — see
    // grace_group_before_barline_attaches_to_note_across_the_bar). Only a
    // grace with NO following note at all (end of tune) is void: no panic, no
    // `<grace/>`, and a dangling-grace diagnostic instead of a silent drop.
    let carried = export_musicxml("X:1\nT:Orphan Grace Bar\nM:4/4\nL:1/4\nK:C\nc {g}| d|\n")
        .expect("carried-grace score should export");
    assert_balanced_xml(&carried.musicxml);
    assert_eq!(count(&carried.musicxml, "<grace/>"), 1);

    let orphan = export_musicxml("X:1\nT:Orphan Grace End\nM:4/4\nL:1/4\nK:C\nc {g}\n")
        .expect("orphan-grace score should export");
    assert_balanced_xml(&orphan.musicxml);
    assert_eq!(count(&orphan.musicxml, "<grace/>"), 0);
    assert!(
        orphan
            .diagnostics
            .iter()
            .any(|d| d.code == "abc.music.dangling_grace_group")
    );
}

#[test]
fn lyric_hyphen_controls_do_not_export_as_sung_text() {
    let source = "X:1\nT:Hyphen Lyrics\nM:4/4\nL:1/4\nK:C\nC D E F|\nw: A-des-te fi-del\n";
    let export = export_musicxml(source).expect("hyphen lyric score should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<lyric number=\"1\">"), 4);
    assert!(export.musicxml.contains("<text>A</text>"));
    assert!(export.musicxml.contains("<text>des</text>"));
    assert!(export.musicxml.contains("<text>te</text>"));
    assert!(export.musicxml.contains("<text>fi</text>"));
    assert!(!export.musicxml.contains("<text>del</text>"));
    assert!(!export.musicxml.contains("<text>-</text>"));
    assert_eq!(
        export
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "abc.lyric.syllable_count")
            .count(),
        1
    );
}

#[test]
fn escaped_literal_hyphen_in_lyrics_still_exports_as_text() {
    let source = "X:1\nT:Literal Hyphen Lyrics\nM:2/4\nL:1/4\nK:C\nC D|\nw: \\-dash end\n";
    let export = export_musicxml(source).expect("literal hyphen lyric score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<text>-dash</text>"));
    assert!(!export.musicxml.contains("<text>-</text>"));
    assert!(export.diagnostics.is_empty());
}

#[test]
fn lyric_underscore_exports_melisma_extender_without_sung_text() {
    let source = "X:1\nT:Melisma Lyrics\nM:3/4\nL:1/4\nK:C\nC D E|\nw: time_ day\n";
    let export = export_musicxml(source).expect("melisma lyric score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<text>time</text>"));
    assert!(export.musicxml.contains("<extend/>"));
    assert!(export.musicxml.contains("<text>day</text>"));
    assert!(!export.musicxml.contains("<text>_</text>"));
    assert!(export.diagnostics.is_empty());
}

#[test]
fn clef_octave_suffix_shifts_notes_and_marks_the_clef() {
    // `clef=treble-8` writes the notes one octave lower and adds a matching
    // clef-octave-change, like abc2xml. `C` (octave 4) becomes octave 3.
    let source = "X:1\nL:1/4\nK:C\nV:1 clef=treble-8\nC2 C2|\n";
    let export = export_musicxml(source).expect("treble-8 score should export");
    assert_balanced_xml(&export.musicxml);
    assert!(
        export
            .musicxml
            .contains("<clef-octave-change>-1</clef-octave-change>")
    );
    assert!(export.musicxml.contains("<octave>3</octave>"));
    assert!(!export.musicxml.contains("<octave>4</octave>"));
}

#[test]
fn voice_middle_modifier_shifts_octave() {
    // `clef=bass middle=d` declares d on the middle staff line, which shifts the
    // written->sounding octave down by two (abc2xml gtrans = -2). A lowercase
    // `e` (octave 5 written) therefore sounds in octave 3.
    let source = "X:1\nT:Bass test\nM:C\nL:1/4\nK:C\nV:1 clef=bass middle=d\ne e e e |\n";
    let export = export_musicxml(source).expect("bass middle=d score should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<octave>3</octave>"));
    assert!(!export.musicxml.contains("<octave>5</octave>"));
}

#[test]
fn bare_voice_switch_keeps_each_header_clef() {
    // Header voice definitions carry clefs; a later bare `V:n` switch in the
    // body must not wipe them. Each voice keeps its own clef and octave.
    let source = concat!(
        "X:1\nL:1/4\n",
        "V:1 clef=treble\nV:2 clef=bass\nK:C\n",
        "V:1\nc c|\n",
        "V:2\nC, C,|\n",
    );
    let export = export_musicxml(source).expect("multi-voice score should export");
    assert_balanced_xml(&export.musicxml);
    let p2 = export
        .musicxml
        .split("<part id=\"P2\">")
        .nth(1)
        .expect("part P2");
    assert!(
        p2.contains("<sign>F</sign>"),
        "V:2 should keep its bass clef"
    );
}

#[test]
fn key_field_octave_property_shifts_single_voice() {
    // `octave=1` on a header `K:` line (ABC 2.1 §4.6 allows clef/octave on K:)
    // shifts every note up one octave, exactly like abc2xml. The baseline (no
    // `octave=`) writes `C` at octave 4; with `octave=1` it must be octave 5.
    let baseline = export_musicxml("X:1\nL:1/4\nK:C\nC2 C2|\n").expect("baseline should export");
    assert!(baseline.musicxml.contains("<octave>4</octave>"));
    assert!(!baseline.musicxml.contains("<octave>5</octave>"));

    let shifted =
        export_musicxml("X:1\nL:1/4\nK:C octave=1\nC2 C2|\n").expect("octave=1 should export");
    assert_balanced_xml(&shifted.musicxml);
    assert!(
        shifted.musicxml.contains("<octave>5</octave>"),
        "octave=1 on K: should shift C from octave 4 to 5"
    );
    assert!(!shifted.musicxml.contains("<octave>4</octave>"));
}

#[test]
fn oversized_octave_and_clef_shifts_clamp_instead_of_panicking() {
    // `V:1 clef=treble+15 octave=125` used to overflow the per-note i8
    // base+shift addition (debug panic). The shift now clamps: the +15
    // suffix gives +2, `octave=125` clamps to +9 (abc2xml's effective
    // single-digit domain), total +11, so C (octave 4) lands at octave 15.
    let export = export_musicxml("X:1\nL:1/4\nK:C\nV:1 clef=treble+15 octave=125\nC2 C2|\n")
        .expect("oversized voice shifts should export");
    assert_balanced_xml(&export.musicxml);
    assert!(
        export.musicxml.contains("<octave>15</octave>"),
        "clef +15 (+2) and octave=125 (clamped +9) should shift C to octave 15"
    );

    // `octave=99999` (outside i8, previously silently ignored) clamps to +9,
    // also via the `K:` clef-property merge path: C octave 4 -> 13.
    let export = export_musicxml("X:1\nL:1/4\nK:C octave=99999\nC2 C2|\n")
        .expect("oversized K: octave should export");
    assert_balanced_xml(&export.musicxml);
    assert!(
        export.musicxml.contains("<octave>13</octave>"),
        "octave=99999 on K: should clamp to +9 and shift C to octave 13"
    );

    // A long octave-mark run no longer overflows the i8 mark sum.
    let marks = ",".repeat(200);
    let source = format!("X:1\nL:1/4\nK:C\nC{marks}DEF|\n");
    let export = export_musicxml(&source).expect("long octave-mark run should export");
    assert_balanced_xml(&export.musicxml);
}

#[test]
fn key_field_clef_shorthand_scopes_to_current_voice_only() {
    // `K:C treble+8` / `K:C treble-8` appearing after a `V:n` switch scope to the
    // voice currently in scope only (abc2xml semantics), and must NOT leak into
    // other voices. V:1 gets +8 (C octave 4 -> 5); V:2 gets -8 (C octave 4 -> 3).
    let source = concat!(
        "X:1\nL:1/4\n",
        "K:C\n",
        "V:1\n",
        "K:C treble+8\n",
        "C2 C2|\n",
        "V:2\n",
        "K:C treble-8\n",
        "C2 C2|\n",
    );
    let export = export_musicxml(source).expect("multi-voice K: clef score should export");
    assert_balanced_xml(&export.musicxml);

    let p1 = export
        .musicxml
        .split("<part id=\"P2\">")
        .next()
        .expect("part P1 segment");
    let p2 = export
        .musicxml
        .split("<part id=\"P2\">")
        .nth(1)
        .expect("part P2 segment");

    // V:1 shifted up: octave 5, never octave 4 or 3.
    assert!(
        p1.contains("<octave>5</octave>"),
        "V:1 K:C treble+8 should shift C to octave 5"
    );
    assert!(
        !p1.contains("<octave>3</octave>"),
        "V:1 shift must not be the V:2 (-8) shift"
    );
    // V:2 shifted down: octave 3, and crucially NOT octave 5 (no leak from V:1).
    assert!(
        p2.contains("<octave>3</octave>"),
        "V:2 K:C treble-8 should shift C to octave 3"
    );
    assert!(
        !p2.contains("<octave>5</octave>"),
        "V:1's +8 shift must not leak into V:2"
    );
}

#[test]
fn unclosed_decoration_does_not_swallow_following_notes() {
    // `!f2e2f2` is a stray `!` before notes (a deprecated line-break or
    // typo), not a decoration named "f2e2f2". The notes must survive.
    let source = "X:1\nL:1/8\nK:C\nA !f2e2f2 | g2|\n";
    let export = export_musicxml(source).expect("stray-bang score should export");
    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<step>F</step>"), 2);
    assert_eq!(count(&export.musicxml, "<step>E</step>"), 1);
    assert_eq!(count(&export.musicxml, "<step>A</step>"), 1);
    assert_eq!(count(&export.musicxml, "<step>G</step>"), 1);
}

#[test]
fn chords_adjacent_to_barlines_are_not_swallowed() {
    // `|[G2C,2]` and `][` must keep the chords intact: the `[` opens a chord,
    // it is not part of a liberal `|[` / `][` barline. Two 4/4 measures of
    // four quarter chords each, not many tiny measures.
    let source =
        "X:1\nL:1/8\nM:4/4\nK:C\n[G2C,2][c2C,2][A2F,2][e2C,2]|[F2D,2][D2D,2][A2D,2][d2D,2]|\n";
    let export = export_musicxml(source).expect("bass-chord score should export");
    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<measure "), 2);
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal"),
        "no chord bracket should be read as a liberal barline"
    );
}

#[test]
fn staccato_chord_keeps_its_length_suffix() {
    // `.[CE]2` is a staccato chord of length 2 (a quarter at L:1/8), not a
    // dotted barline. The leading `.` must not swallow the chord's length.
    let source = "X:1\nL:1/8\nK:C\n.[CE]2 [CE]|\n";
    let export = export_musicxml(source).expect("staccato chord should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<staccato/>"));
    // Two chords (each two notes) in one measure: the first is a quarter.
    assert_eq!(count(&export.musicxml, "<chord/>"), 2);
    assert_eq!(count(&export.musicxml, "<type>quarter</type>"), 2);
    assert_eq!(count(&export.musicxml, "<type>eighth</type>"), 2);
}

#[test]
fn decorations_map_to_notation_elements_not_words() {
    // ABC decorations map to MusicXML notation categories, not <words>:
    // fermata -> <fermata>, staccato/accent -> <articulations>,
    // up-bow -> <technical>, trill -> <ornaments>.
    let source = "X:1\nL:1/4\nK:C\n!fermata!.C !accent!D|!upbow!E !trill!F|\n";
    let export = export_musicxml(source).expect("decorated score should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<fermata type=\"upright\"/>"));
    assert!(export.musicxml.contains("<articulations>"));
    assert!(export.musicxml.contains("<staccato/>"));
    assert!(export.musicxml.contains("<accent/>"));
    assert!(export.musicxml.contains("<technical>"));
    assert!(export.musicxml.contains("<up-bow/>"));
    assert!(export.musicxml.contains("<ornaments>"));
    assert!(export.musicxml.contains("<trill-mark/>"));
    assert!(!export.musicxml.contains("<words>fermata</words>"));
    assert!(!export.musicxml.contains("<words>accent</words>"));
}

#[test]
fn shorthand_decorations_map_to_notation_elements_not_words() {
    // ABC 2.1 §4.14 single-char shorthand decorations are the canonical
    // equivalents of the long-form `!...!` names and must map to the same
    // MusicXML notation/symbol output, never to <words> directions.
    let source = "X:1\nL:1/4\nK:C\nHC TD|uE vF|MG Pa|\n";
    let export = export_musicxml(source).expect("shorthand decorations should export");
    assert_balanced_xml(&export.musicxml);
    // H -> fermata
    assert!(export.musicxml.contains("<fermata type=\"upright\"/>"));
    // T -> trill
    assert!(export.musicxml.contains("<trill-mark/>"));
    // u -> up-bow, v -> down-bow
    assert!(export.musicxml.contains("<up-bow/>"));
    assert!(export.musicxml.contains("<down-bow/>"));
    // M -> lowermordent (mordent), P -> uppermordent (inverted-mordent)
    assert!(export.musicxml.contains("<mordent/>"));
    assert!(export.musicxml.contains("<inverted-mordent/>"));
    // No raw shorthand chars leak out as <words>.
    for raw in ["H", "T", "u", "v", "M", "P"] {
        assert!(
            !export.musicxml.contains(&format!("<words>{raw}</words>")),
            "shorthand `{raw}` should not be emitted as <words>"
        );
    }
}

#[test]
fn shorthand_accent_maps_to_articulation_not_words() {
    let source = "X:1\nL:1/4\nK:C\nLC D|\n";
    let export = export_musicxml(source).expect("shorthand accent should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<articulations>"));
    assert!(export.musicxml.contains("<accent/>"));
    assert!(!export.musicxml.contains("<words>L</words>"));
}

#[test]
fn shorthand_segno_and_coda_map_to_direction_symbols_not_words() {
    let source = "X:1\nL:1/4\nK:C\nSC OD|\n";
    let export = export_musicxml(source).expect("shorthand segno/coda should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<segno/>"));
    assert!(export.musicxml.contains("<coda/>"));
    assert!(!export.musicxml.contains("<words>S</words>"));
    assert!(!export.musicxml.contains("<words>O</words>"));
}

#[test]
fn shorthand_roll_emits_neither_words_nor_diagnostic() {
    // `~` (Irish roll / general gracing) has no clean MusicXML equivalent;
    // abc2xml emits nothing. The hard requirement is that it must NOT become
    // a <words> direction, which would show up as an extra music21 direction.
    let source = "X:1\nL:1/4\nK:C\n~C D|\n";
    let export = export_musicxml(source).expect("shorthand roll should export");
    assert_balanced_xml(&export.musicxml);
    assert!(!export.musicxml.contains("<words>~</words>"));
    assert!(!export.musicxml.contains("<words>roll</words>"));
    // Suppressed cleanly: no unsupported-decoration diagnostic for `~`.
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.musicxml.decoration.unsupported"),
        "roll should be suppressed without an unsupported-decoration diagnostic"
    );
    // The notes and their timing survive.
    assert_eq!(count(&export.musicxml, "<note>"), 2);
}

#[test]
fn user_defined_symbol_expands_to_its_notation_not_words() {
    // `U:T=!trill!` redefines T; a `U:`-defined letter must expand to its
    // definition and map through the same notation path.
    let source = "X:1\nU:W=!trill!\nL:1/4\nK:C\nWC D|\n";
    let export = export_musicxml(source).expect("user symbol should export");
    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<trill-mark/>"));
    assert!(!export.musicxml.contains("<words>W</words>"));
}

#[test]
fn post_tune_words_export_as_credits_not_in_measure_directions() {
    // `W:` words are printed after the tune (ABC 2.1), so they belong in
    // score-header <credit> elements, not as in-measure <words> directions.
    let source = "X:1\nT:Song\nL:1/4\nK:C\nC D E F|\nW: Verse one here\nW: Verse two here\nW:\n";
    let export = export_musicxml(source).expect("post-tune words should export");
    assert_balanced_xml(&export.musicxml);
    assert!(
        export
            .musicxml
            .contains("<credit-words>Verse one here</credit-words>")
    );
    assert!(
        export
            .musicxml
            .contains("<credit-words>Verse two here</credit-words>")
    );
    // The empty `W:` line is skipped, and no verse leaks into a direction.
    assert!(!export.musicxml.contains("<words>Verse one here</words>"));
    assert_eq!(count(&export.musicxml, "<credit-words>"), 2);
}

#[test]
fn staves_parenthesis_group_merges_voices_into_one_part() {
    // `%%staves 1 (2 3) 4`: voices 2 and 3 share one part; 1 and 4 are their
    // own parts, giving three parts.
    let source = concat!(
        "X:1\nL:1/4\n%%staves 1 (2 3) 4\n",
        "V:1\nV:2\nV:3\nV:4\nK:C\n",
        "V:1\nC D|\nV:2\nE F|\nV:3\nG A|\nV:4\nc d|\n",
    );
    let export = export_musicxml(source).expect("grouped score should export");
    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<part id="), 3);
}

#[test]
fn staves_bracket_group_keeps_one_part_per_voice() {
    let source = concat!(
        "X:1\nL:1/4\n%%staves [1 2 3]\n",
        "V:1\nV:2\nV:3\nK:C\n",
        "V:1\nC D|\nV:2\nE F|\nV:3\nG A|\n",
    );
    let export = export_musicxml(source).expect("bracketed score should export");
    assert_eq!(count(&export.musicxml, "<part id="), 3);
}

#[test]
fn each_voice_becomes_its_own_part() {
    // A multi-voice tune exports as one score with one <part> per voice, in
    // voice order, all in a single document (matching abc2xml/music21).
    let source = "X:1\nL:1/4\nK:C\nV:1\nC D|E F|\nV:2\nG A|B c|\nV:3\nc B|A G|\n";
    let export = export_musicxml(source).expect("multi-voice score should export");
    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<score-partwise"), 1);
    assert_eq!(count(&export.musicxml, "<part id="), 3);
    for id in ["P1", "P2", "P3"] {
        assert!(
            export.musicxml.contains(&format!("<part id=\"{id}\"")),
            "missing part {id}"
        );
    }
}

#[test]
fn single_voice_tune_stays_one_part() {
    let source = "X:1\nL:1/4\nK:C\nC D E F|\n";
    let export = export_musicxml(source).expect("single-voice score should export");
    assert_eq!(count(&export.musicxml, "<part id="), 1);
}

#[test]
fn inline_key_change_applies_to_following_accidentals() {
    // `[K:D]` mid-tune must make the following notes use the D-major key
    // signature: the C in the second measure becomes C-sharp.
    let source = "X:1\nL:1/8\nK:C\nCEG c|[K:D]CEG c|\n";
    let export = export_musicxml(source).expect("inline key change should export");
    assert_balanced_xml(&export.musicxml);
    let second = export
        .musicxml
        .split("<measure number=\"2\">")
        .nth(1)
        .expect("second measure");
    assert!(
        second.contains("<step>C</step>\n          <alter>1</alter>"),
        "second measure C should be sharp under inline K:D"
    );
}

#[test]
fn inline_clef_only_key_field_does_not_reset_the_signature() {
    // `[K:clef=bass]` only changes the clef; it must not be misread as a key
    // change that wipes the D-major signature (the following F stays F#).
    let source = "X:1\nL:1/8\nK:D\nFAd f|[K:clef=bass]FAd f|\n";
    let export = export_musicxml(source).expect("inline clef change should export");
    assert_balanced_xml(&export.musicxml);
    let second = export
        .musicxml
        .split("<measure number=\"2\">")
        .nth(1)
        .expect("second measure");
    assert!(
        second.contains("<step>F</step>\n          <alter>1</alter>"),
        "F should stay sharp; clef-only inline key must not reset the signature"
    );
}

#[test]
fn escaped_literal_underscore_in_lyrics_still_exports_as_text() {
    let source = "X:1\nT:Literal Underscore Lyrics\nM:2/4\nL:1/4\nK:C\nC D|\nw: \\_hold end\n";
    let export = export_musicxml(source).expect("literal underscore lyric score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<text>_hold</text>"));
    assert!(!export.musicxml.contains("<text>_</text>"));
    assert!(export.diagnostics.is_empty());
}

#[test]
fn lyric_nbsp_inside_tune_000509_style_word_is_not_a_separator() {
    let source = "X:1\nT:NBSP Melisma Lyrics\nM:6/4\nL:1/4\nK:C\nC D E F G A|\nw: A-ten-toÃ\u{00a0}a-do_ra\n";
    let export = export_musicxml(source).expect("NBSP lyric score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<text>toÃ\u{00a0}a</text>"));
    assert!(export.musicxml.contains("<text>do</text>"));
    assert!(export.musicxml.contains("<extend/>"));
    assert!(!export.musicxml.contains("<text>toÃ</text>"));
    assert!(export.diagnostics.is_empty());
}

#[test]
fn ties_across_barlines_export_start_and_stop_without_diagnostic() {
    let source = "X:1\nM:2/4\nL:1/4\nK:C\nC- | C D |\n";
    let export = export_musicxml(source).expect("cross-bar tie should export");

    assert_balanced_xml(&export.musicxml);
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    assert_eq!(count(&export.musicxml, "<tie type=\"start\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tie type=\"stop\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tied type=\"start\""), 1);
    assert_eq!(count(&export.musicxml, "<tied type=\"stop\""), 1);
    let measures = musicxml_measures(&export.musicxml);
    assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
    assert_eq!(measures[0].notes.len(), 1);
    assert_eq!(measures[1].notes.len(), 2);
}

#[test]
fn whole_chord_tie_ties_every_matching_member() {
    // `[CE]-[CE]` ties both chord members across the two chords.
    let source = "X:1\nM:4/4\nL:1/2\nK:C\n[CE]-[CE]|\n";
    let export = export_musicxml(source).expect("whole-chord tie should export");
    assert_balanced_xml(&export.musicxml);
    assert!(
        !export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    // Both members tie: two starts + two stops for each of <tie> and <tied>.
    assert_eq!(count(&export.musicxml, "<tie type=\"start\"/>"), 2);
    assert_eq!(count(&export.musicxml, "<tie type=\"stop\"/>"), 2);
    assert_eq!(count(&export.musicxml, "<tied type=\"start\""), 2);
    assert_eq!(count(&export.musicxml, "<tied type=\"stop\""), 2);
}

#[test]
fn chord_internal_tie_ties_only_the_marked_member() {
    // `[DA-]2[FA]`: A continues into the next chord, D does not.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n[DA-]2[FA]|\n";
    let export = export_musicxml(source).expect("chord-internal tie should export");
    assert_balanced_xml(&export.musicxml);
    // Exactly one tied pair (the A member); D is not tied.
    assert_eq!(count(&export.musicxml, "<tie type=\"start\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tie type=\"stop\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tied type=\"start\""), 1);
    assert_eq!(count(&export.musicxml, "<tied type=\"stop\""), 1);
}

#[test]
fn standalone_tie_remains_a_single_pair_and_untied_chord_has_no_tied() {
    // Regression guard: plain single-note tie keeps exactly one pair.
    let single =
        export_musicxml("X:1\nM:4/4\nL:1/2\nK:C\nC2-C2|\n").expect("single tie should export");
    assert_eq!(count(&single.musicxml, "<tie type=\"start\"/>"), 1);
    assert_eq!(count(&single.musicxml, "<tie type=\"stop\"/>"), 1);
    assert_eq!(count(&single.musicxml, "<tied type=\"start\""), 1);
    assert_eq!(count(&single.musicxml, "<tied type=\"stop\""), 1);

    // A chord with no tie marker emits no <tied>.
    let chord =
        export_musicxml("X:1\nM:4/4\nL:1/1\nK:C\n[CE]|\n").expect("untied chord should export");
    assert!(!chord.musicxml.contains("<tied "));
    assert!(!chord.musicxml.contains("<tie "));
}

#[test]
fn grace_notes_export_reference_compatible_display_types_without_duration() {
    let source = "X:1\nT:Grace Display\nM:4/4\nL:1/4\nK:C\n{g}C {de}D|\n";
    let export = export_musicxml(source).expect("grace note should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<grace/>"), 3);
    assert_eq!(count(&export.musicxml, "<type>eighth</type>"), 1);
    assert_eq!(count(&export.musicxml, "<type>16th</type>"), 2);
    assert!(!export.musicxml.contains("<duration>0</duration>"));
}

#[test]
fn grace_note_length_modifiers_scale_display_type() {
    // The graphic `<type>` of a grace note must reflect both the count-based
    // base unit and the grace note's written length modifier, matching
    // abc2xml: base 1/8 for a single grace, 1/16 for a group, then multiplied
    // by the note's written length.
    //   {B}      single, no modifier -> 1/8        -> eighth
    //   {B/}     single, half        -> 1/8 * 1/2  -> 16th
    //   {AG}     two graces          -> 1/16 each  -> 16th
    //   {A/G/}   two graces, half    -> 1/16 * 1/2 -> 32nd
    let cases = [
        ("{B}C", "<type>eighth</type>", 1),
        ("{B/}C", "<type>16th</type>", 1),
        ("{AG}C", "<type>16th</type>", 2),
        ("{A/G/}C", "<type>32nd</type>", 2),
    ];
    for (body, expected_type, expected_count) in cases {
        let source = format!("X:1\nT:Grace Length\nM:4/4\nL:1/4\nK:C\n{body}|\n");
        let export = export_musicxml(&source).expect("grace note should export");
        assert_balanced_xml(&export.musicxml);
        assert_eq!(
            count(&export.musicxml, expected_type),
            expected_count,
            "grace body {body} should yield {expected_count}x {expected_type}",
        );
        // Grace notes carry no <duration> element regardless of modifier.
        assert!(
            !export.musicxml.contains("<duration>0</duration>"),
            "grace body {body} must not emit a zero <duration>",
        );
    }
}

#[test]
fn grace_notes_apply_implicit_key_signature_alter() {
    let source = "X:1\nT:Grace Key\nM:4/4\nL:1/4\nK:D\n{f}A {=f}A|\n";
    let export = export_musicxml(source).expect("grace key accidental should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<grace/>"), 2);
    assert_eq!(count(&export.musicxml, "<alter>1</alter>"), 1);
    assert!(export.musicxml.contains("<accidental>natural</accidental>"));
}

#[test]
fn sequential_tuplets_reuse_musicxml_number_levels() {
    let source = concat!(
        "X:1\n",
        "T:Many Tuplets\n",
        "M:4/4\n",
        "L:1/16\n",
        "K:C\n",
        "(3CDE (3DEF (3EFG (3FGA (3GAB (3ABc (3Bcd|\n",
    );
    let export = export_musicxml(source).expect("many sequential tuplets should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(
        count(&export.musicxml, "<tuplet type=\"start\" number=\"1\"/>"),
        7
    );
    assert_eq!(
        count(&export.musicxml, "<tuplet type=\"stop\" number=\"1\"/>"),
        7
    );
    assert!(!export.musicxml.contains("number=\"7\""));
}

#[test]
fn one_note_tuplet_emits_balanced_start_and_stop() {
    let source = "X:1\nT:One Note Tuplet\nL:1/8\nK:C\n(3:2:1G B|\n";
    let export = export_musicxml(source).expect("one-note tuplet should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(
        count(&export.musicxml, "<tuplet type=\"start\" number=\"1\"/>"),
        1
    );
    assert_eq!(
        count(&export.musicxml, "<tuplet type=\"stop\" number=\"1\"/>"),
        1
    );
    assert!(!export.musicxml.contains("number=\"2\""));
}

#[test]
fn reduced_duration_note_types_do_not_emit_spurious_tuplets() {
    let source = "X:1\nT:Long notes\nM:4/4\nL:1/4\nK:C\nC2 D4|\n";
    let export = export_musicxml(source).expect("long note types should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<type>half</type>"));
    assert!(export.musicxml.contains("<type>whole</type>"));
    assert!(!export.musicxml.contains("<time-modification>"));
}

#[test]
fn repeats_endings_multiple_voices_and_overlays_use_timeline_elements() {
    let source = concat!(
        "X:1\n",
        "M:2/4\n",
        "L:1/8\n",
        "K:C\n",
        "V:1\n",
        "|: C D & E F :| [1 G A | [2 B c |]\n",
        "V:2\n",
        "C2 D2|E2 F2|\n",
    );
    let export = export_musicxml(source).expect("timeline score should export");

    assert_balanced_xml(&export.musicxml);
    assert!(export.musicxml.contains("<repeat direction=\"forward\"/>"));
    assert!(export.musicxml.contains("<repeat direction=\"backward\"/>"));
    assert!(
        export
            .musicxml
            .contains("<ending number=\"1\" type=\"start\"/>")
    );
    assert!(
        export
            .musicxml
            .contains("<ending number=\"2\" type=\"start\"/>")
    );
    // V:1 (with its `&` overlay) and V:2 each become their own part. The
    // overlay still adds a second voice within V:1's part, so a backup and
    // `<voice>2</voice>` appear; V:2 is part P2, not a third voice.
    assert_eq!(count(&export.musicxml, "<part id="), 2);
    assert!(export.musicxml.contains("<part id=\"P2\""));
    assert!(export.musicxml.contains("<backup>"));
    assert!(export.musicxml.contains("<voice>2</voice>"));
    assert!(!export.musicxml.contains("<voice>3</voice>"));
}

#[test]
fn multi_pass_volta_merges_passes_into_single_ending_element() {
    let source = concat!(
        "X:1\n",
        "T:Volta\n",
        "M:2/4\n",
        "L:1/8\n",
        "K:G\n",
        "|: GABc |1,3 GA :|2,4 Bc :|\n",
    );
    let export = export_musicxml(source).expect("multi-pass volta score should export");

    assert_balanced_xml(&export.musicxml);
    // Each volta bracket lists its passes as a single comma-separated `number`.
    assert!(
        export
            .musicxml
            .contains("<ending number=\"1,3\" type=\"start\"")
    );
    assert!(
        export
            .musicxml
            .contains("<ending number=\"2,4\" type=\"start\"")
    );
    // The passes must NOT be split into separate `<ending>` elements.
    assert!(
        !export
            .musicxml
            .contains("<ending number=\"1\" type=\"start\"")
    );
    assert!(
        !export
            .musicxml
            .contains("<ending number=\"3\" type=\"start\"")
    );
    assert!(
        !export
            .musicxml
            .contains("<ending number=\"2\" type=\"start\"")
    );
    assert!(
        !export
            .musicxml
            .contains("<ending number=\"4\" type=\"start\"")
    );
}

#[test]
fn semantic_onset_gaps_emit_forward() {
    let source = "X:1\nL:1/8\nK:C\nC D|\n";
    let document = parse_document(source, ParseOptions::default());
    let tune = crate::parse::parse_tune_report_from_document(&document.value)
        .value
        .expect("expected tune");
    let mut score = tune.score;
    score.parts[0].voices[0].events[1].onset = Fraction::new(2, 8);
    let report = write_score_partwise(&score);

    assert_balanced_xml(&report.value);
    assert!(report.value.contains("<forward>"));
    assert!(report.value.contains("<duration>4</duration>"));
}

#[test]
fn unsupported_decoration_diagnoses_without_dropping_note_or_timing() {
    let source = "X:1\nL:1/8\nK:C\n!unknown!C D|\n";
    let export = export_musicxml(source).expect("unsupported decoration should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.musicxml.decoration.unsupported",
        "!unknown!",
    );
    assert_eq!(count(&export.musicxml, "<note>"), 2);
    assert_eq!(count(&export.musicxml, "<duration>4</duration>"), 2);
}

#[test]
fn variable_duration_chord_diagnoses_and_keeps_following_timing_valid() {
    let source = "X:1\nL:1/8\nK:C\n[E2G6] C|\n";
    let export = export_musicxml(source).expect("variable chord should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.music.chord.variable_duration",
        "[E2G6]",
    );
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.musicxml.chord.variable_duration",
        "E2G6",
    );
    assert_eq!(count(&export.musicxml, "<chord/>"), 1);
    assert_eq!(count(&export.musicxml, "<note>"), 3);
    assert!(!export.musicxml.contains("<backup>"));
}

#[test]
fn incomplete_overlay_diagnoses_and_later_measures_stay_stable() {
    let source = "X:1\nL:1/8\nK:C\nC D & E|F G|\n";
    let export = export_musicxml(source).expect("incomplete overlay should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.voice.overlay_incomplete_measure",
        "&",
    );
    assert!(export.musicxml.contains("<measure number=\"2\">"));
    assert!(export.musicxml.contains("<step>F</step>"));
    assert!(export.musicxml.contains("<step>G</step>"));
}

#[test]
fn rest_led_tuplet_emits_tuplet_start_on_the_rest() {
    // `(3zBA`: the leading rest carries the tuplet Start, so its <note> emits
    // <tuplet type="start"> — matching the abc2xml oracle.
    let source = "X:1\nL:1/8\nK:C\n(3zBA F|\n";
    let export = export_musicxml(source).expect("rest-led tuplet should export");

    assert_balanced_xml(&export.musicxml);
    let rest_note = export
        .musicxml
        .split("<note>")
        .skip(1)
        .find(|note| note.contains("<rest/>"))
        .expect("expected a rest note");
    assert!(rest_note.contains("<tuplet type=\"start\""));
    assert!(export.musicxml.contains("<tuplet type=\"stop\""));
}

#[test]
fn bad_tuplet_count_diagnoses_without_bogus_tuplet_notation_pairs() {
    let source = "X:1\nL:1/8\nK:C\n(3C|D E|\n";
    let export = export_musicxml(source).expect("short tuplet should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.music.tuplet.too_few_notes",
        "(3",
    );
    assert!(export.musicxml.contains("<time-modification>"));
    assert!(!export.musicxml.contains("<tuplet type=\"start\""));
    assert!(!export.musicxml.contains("<tuplet type=\"stop\""));
    assert!(export.musicxml.contains("<measure number=\"2\">"));
}

#[test]
fn unmatched_tie_and_slur_do_not_create_musicxml_pairs() {
    let source = "X:1\nL:1/8\nK:C\nC- D )E|\n";
    let export = export_musicxml(source).expect("unmatched tie and slur should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(source, &export.diagnostics, "abc.music.unmatched_tie", "-");
    assert_diagnostic_span(source, &export.diagnostics, "abc.music.unmatched_slur", ")");
    assert!(!export.musicxml.contains("<tie "));
    assert!(!export.musicxml.contains("<tied "));
    assert!(!export.musicxml.contains("<slur "));
    assert_eq!(count(&export.musicxml, "<note>"), 3);
}

#[test]
fn malformed_repeat_ending_keeps_measure_structure_valid() {
    let source = "X:1\nL:1/8\nK:C\nC|[1- D|E|\n";
    let export = export_musicxml(source).expect("malformed repeat ending should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.music.invalid_repeat_ending",
        "[1-",
    );
    assert!(!export.musicxml.contains("<ending number=\"1-\""));
    assert!(export.musicxml.contains("<measure number=\"1\">"));
    assert!(export.musicxml.contains("<measure number=\"2\">"));
    assert!(export.musicxml.contains("<measure number=\"3\">"));
    assert_eq!(count(&export.musicxml, "<note>"), 3);
}

#[test]
fn non_integral_duration_reports_precise_writer_diagnostic() {
    let source = "X:1\nL:1/8\nK:C\nC D|\n";
    let document = parse_document(source, ParseOptions::default());
    let tune = crate::parse::parse_tune_report_from_document(&document.value)
        .value
        .expect("expected tune");
    let mut score = tune.score;
    score.divisions = 1;
    let report = write_score_partwise(&score);

    assert_balanced_xml(&report.value);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.musicxml.duration.non_integral")
    );
    assert!(report.value.contains("<duration>1</duration>"));
}

#[test]
fn unsupported_note_type_duration_reports_precise_writer_diagnostic() {
    let source = "X:1\nL:1/8\nK:C\nC|\n";
    let document = parse_document(source, ParseOptions::default());
    let tune = crate::parse::parse_tune_report_from_document(&document.value)
        .value
        .expect("expected tune");
    let mut score = tune.score;
    score.divisions = 13;
    score.parts[0].voices[0].events[0].duration = Fraction::new(7, 13);
    let report = write_score_partwise(&score);

    assert_balanced_xml(&report.value);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.musicxml.duration.unsupported_note_type")
    );
    assert!(report.value.contains("<duration>28</duration>"));
}

#[test]
fn unknown_directive_is_direction_metadata_not_music() {
    let source = "X:1\nK:C\n%%foo bar & < >\nC|\n";
    let export = export_musicxml(source).expect("unknown directive should recover");

    assert_balanced_xml(&export.musicxml);
    assert_diagnostic_span(
        source,
        &export.diagnostics,
        "abc.directive.unsupported",
        "foo",
    );
    // The unsupported directive is reported as a diagnostic but, being a
    // preserved `%%` stylesheet directive, is not rendered as printed words.
    assert!(!export.musicxml.contains("%%foo"));
    assert_eq!(count(&export.musicxml, "<note>"), 1);
}

#[test]
fn unclosed_chord_bracket_before_barline_preserves_following_measures() {
    // An unclosed `[` (here followed by a space then a repeat-start) must not
    // swallow the real measures that follow it. The chord scan stops at the
    // barline so `CDEF` and `GABc` survive as two measures.
    let source = "X:1\nM:2/4\nL:1/8\nK:C\n[ |: CDEF | GABc |\n";
    let export = export_musicxml(source).expect("unclosed chord run should still export");

    assert!(
        count(&export.musicxml, "<measure ") >= 2,
        "expected at least two surviving measures, got {}",
        count(&export.musicxml, "<measure ")
    );
    for step in ['C', 'D', 'E', 'F', 'G', 'A', 'B'] {
        assert!(
            export.musicxml.contains(&format!("<step>{step}</step>")),
            "expected note {step} to survive the unclosed chord"
        );
    }
}

#[test]
fn unclosed_chord_with_quoted_text_before_barline_preserves_measures() {
    // A chord-symbol-like quoted text inside an unclosed bracket must also stop
    // at the barline rather than eating the following measures.
    let source = "X:1\nM:2/4\nL:1/8\nK:C\n[\"x\" CDEF | GABc |\n";
    let export = export_musicxml(source).expect("unclosed quoted chord run should still export");

    // The chord scan stops at the first bar line, so the measure that follows
    // the unclosed bracket (`GABc`) is no longer swallowed. Before the fix the
    // scan ran to end-of-line, eating both bar lines and discarding every
    // following measure; now the music after the unclosed run survives.
    assert!(
        count(&export.musicxml, "<measure ") >= 1,
        "expected at least one surviving measure, got {}",
        count(&export.musicxml, "<measure ")
    );
    for step in ['G', 'A', 'B'] {
        assert!(
            export.musicxml.contains(&format!("<step>{step}</step>")),
            "expected note {step} after the unclosed quoted chord to survive"
        );
    }
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[derive(Debug)]
struct XmlMeasure {
    number: String,
    notes: Vec<XmlNote>,
    barlines: Vec<XmlBarline>,
}

#[derive(Debug)]
struct XmlNote {
    rest: bool,
    step: Option<char>,
    duration: Option<u32>,
}

#[derive(Debug)]
struct XmlBarline {
    location: String,
    bar_style: Option<String>,
    repeat_direction: Option<String>,
    repeat_times: Option<String>,
    endings: Vec<XmlEnding>,
}

#[derive(Debug)]
struct XmlEnding {
    number: String,
    kind: String,
}

fn musicxml_measures(xml: &str) -> Vec<XmlMeasure> {
    let mut measures = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<measure ") {
        let start = index + offset;
        let open_end = xml[start..]
            .find('>')
            .map(|end| start + end)
            .expect("measure start tag should terminate");
        let end_tag = "</measure>";
        let end = xml[open_end..]
            .find(end_tag)
            .map(|end| open_end + end)
            .expect("measure should have closing tag");
        let open_tag = &xml[start..=open_end];
        let body = &xml[open_end + 1..end];
        measures.push(XmlMeasure {
            number: attr_value(open_tag, "number").expect("measure should have number"),
            notes: musicxml_notes(body),
            barlines: musicxml_barlines(body),
        });
        index = end + end_tag.len();
    }
    measures
}

fn musicxml_notes(xml: &str) -> Vec<XmlNote> {
    let mut notes = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<note") {
        let start = index + offset;
        let open_end = xml[start..]
            .find('>')
            .map(|end| start + end)
            .expect("note start tag should terminate");
        let end_tag = "</note>";
        let end = xml[open_end..]
            .find(end_tag)
            .map(|end| open_end + end)
            .expect("note should have closing tag");
        let body = &xml[open_end + 1..end];
        notes.push(XmlNote {
            rest: body.contains("<rest"),
            step: element_text(body, "step").and_then(|text| text.chars().next()),
            duration: element_text(body, "duration").and_then(|text| text.parse().ok()),
        });
        index = end + end_tag.len();
    }
    notes
}

fn musicxml_barlines(xml: &str) -> Vec<XmlBarline> {
    let mut barlines = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<barline ") {
        let start = index + offset;
        let open_end = xml[start..]
            .find('>')
            .map(|end| start + end)
            .expect("barline start tag should terminate");
        let end_tag = "</barline>";
        let end = xml[open_end..]
            .find(end_tag)
            .map(|end| open_end + end)
            .expect("barline should have closing tag");
        let open_tag = &xml[start..=open_end];
        let body = &xml[open_end + 1..end];
        let repeat_direction = body.find("<repeat ").and_then(|offset| {
            let repeat_start = offset;
            let repeat_end = body[repeat_start..]
                .find('>')
                .map(|end| repeat_start + end)?;
            attr_value(&body[repeat_start..=repeat_end], "direction")
        });
        let repeat_times = body.find("<repeat ").and_then(|offset| {
            let repeat_start = offset;
            let repeat_end = body[repeat_start..]
                .find('>')
                .map(|end| repeat_start + end)?;
            attr_value(&body[repeat_start..=repeat_end], "times")
        });
        barlines.push(XmlBarline {
            location: attr_value(open_tag, "location").expect("barline should have location"),
            bar_style: element_text(body, "bar-style"),
            repeat_direction,
            repeat_times,
            endings: musicxml_endings(body),
        });
        index = end + end_tag.len();
    }
    barlines
}

fn musicxml_endings(xml: &str) -> Vec<XmlEnding> {
    let mut endings = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<ending ") {
        let start = index + offset;
        let end = xml[start..]
            .find('>')
            .map(|end| start + end)
            .expect("ending tag should terminate");
        let tag = &xml[start..=end];
        endings.push(XmlEnding {
            number: attr_value(tag, "number").expect("ending should have number"),
            kind: attr_value(tag, "type").expect("ending should have type"),
        });
        index = end + 1;
    }
    endings
}

fn measure_numbers(measures: &[XmlMeasure]) -> Vec<&str> {
    measures
        .iter()
        .map(|measure| measure.number.as_str())
        .collect()
}

fn note_steps(measure: &XmlMeasure) -> Vec<char> {
    measure.notes.iter().filter_map(|note| note.step).collect()
}

fn note_durations(measure: &XmlMeasure) -> Vec<u32> {
    measure
        .notes
        .iter()
        .filter_map(|note| note.duration)
        .collect()
}

fn has_barline(
    measure: &XmlMeasure,
    location: &str,
    bar_style: Option<&str>,
    repeat_direction: Option<&str>,
) -> bool {
    measure.barlines.iter().any(|barline| {
        barline.location == location
            && barline.bar_style.as_deref() == bar_style
            && barline.repeat_direction.as_deref() == repeat_direction
    })
}

fn has_ending(measure: &XmlMeasure, location: &str, number: &str, kind: &str) -> bool {
    measure.barlines.iter().any(|barline| {
        barline.location == location
            && barline
                .endings
                .iter()
                .any(|ending| ending.number == number && ending.kind == kind)
    })
}

fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_owned())
}

fn element_text(block: &str, element: &str) -> Option<String> {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].to_owned())
}

fn assert_diagnostic_span(
    source: &str,
    diagnostics: &[Diagnostic],
    code: &'static str,
    snippet: &str,
) {
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("expected diagnostic {code}"));
    assert_eq!(&source[diagnostic.span.start..diagnostic.span.end], snippet);
}

fn assert_balanced_xml(xml: &str) {
    let mut stack: Vec<String> = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find('<') {
        let start = index + offset;
        let end = xml[start..]
            .find('>')
            .map(|end| start + end)
            .unwrap_or_else(|| panic!("unterminated XML tag at byte {start}"));
        let tag = &xml[start + 1..end];
        if tag.starts_with('?') || tag.starts_with('!') {
            index = end + 1;
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            let expected = stack.pop().expect("unexpected closing XML tag");
            assert_eq!(name.trim(), expected);
        } else if !tag.trim_end().ends_with('/') {
            let name = tag
                .split_whitespace()
                .next()
                .expect("XML tag should have a name")
                .trim_end_matches('/');
            stack.push(name.to_owned());
        }
        index = end + 1;
    }
    assert!(stack.is_empty(), "unclosed XML tags: {stack:?}");
}

/// Collect the `(step, alter)` of every pitched note in source order.
fn note_steps_and_alters(xml: &str) -> Vec<(char, i8)> {
    let mut out = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<step>") {
        let start = index + offset + "<step>".len();
        let end = start + xml[start..].find("</step>").expect("step end");
        let step = xml[start..end].chars().next().expect("step char");
        // The optional <alter> for this pitch lives between the step and the
        // closing </pitch>.
        let pitch_end = end + xml[end..].find("</pitch>").expect("pitch end");
        let alter = xml[end..pitch_end]
            .find("<alter>")
            .map(|alter_offset| {
                let alter_start = end + alter_offset + "<alter>".len();
                let alter_end =
                    alter_start + xml[alter_start..].find("</alter>").expect("alter end");
                xml[alter_start..alter_end]
                    .parse::<i8>()
                    .expect("alter int")
            })
            .unwrap_or(0);
        out.push((step, alter));
        index = pitch_end;
    }
    out
}

/// Split the partwise document into its `<part ...>...</part>` bodies.
fn part_bodies(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0;
    while let Some(offset) = xml[index..].find("<part ") {
        let start = index + offset;
        let body_start = start + xml[start..].find('>').expect("part open end") + 1;
        let body_end = body_start + xml[body_start..].find("</part>").expect("part end");
        out.push(xml[body_start..body_end].to_owned());
        index = body_end;
    }
    out
}

#[test]
fn inline_key_change_scopes_to_current_voice_only() {
    // V1 switches to C in its third measure; V3 must keep key G, so its F
    // notes stay F# throughout. abc2xml keeps the other voice in key G.
    let source = concat!(
        "X:1\n",
        "M:2/4\n",
        "L:1/8\n",
        "K:G\n",
        "V:1\n",
        "A A A A | A A A A | [K:C] A A A A |\n",
        "V:3\n",
        "F F F F | F F F F | G G G G |\n",
    );
    let export = export_musicxml(source).expect("multi-voice score should export");
    assert_balanced_xml(&export.musicxml);

    let parts = part_bodies(&export.musicxml);
    assert_eq!(parts.len(), 2, "expected two voices/parts");

    // V3 (second part): the eight F notes across the first two measures must
    // all sound F# (alter +1) from key G; the inline [K:C] in V1 must not
    // wipe V3's key signature.
    let v3 = note_steps_and_alters(&parts[1]);
    let f_notes: Vec<(char, i8)> = v3.into_iter().filter(|(step, _)| *step == 'F').collect();
    assert_eq!(f_notes.len(), 8, "V3 should have eight F notes");
    for (step, alter) in f_notes {
        assert_eq!((step, alter), ('F', 1), "V3 F must stay F# under key G");
    }
}

#[test]
fn tie_across_barline_keeps_natural_against_flat_key() {
    // `=B-` ties a natural B across the barline; the stop note must remain
    // natural (alter 0) and not pick up key F's B-flat.
    let source = "X:1\nM:4/4\nL:1/4\nK:F\nA G =B- | B A2 z |\n";
    let export = export_musicxml(source).expect("tied score should export");
    assert_balanced_xml(&export.musicxml);

    let notes = note_steps_and_alters(&export.musicxml);
    let b_notes: Vec<(char, i8)> = notes.into_iter().filter(|(step, _)| *step == 'B').collect();
    assert_eq!(
        b_notes.len(),
        2,
        "expected two B notes (tie start and stop)"
    );
    for (step, alter) in b_notes {
        assert_eq!(
            (step, alter),
            ('B', 0),
            "tied B must stay natural across bar"
        );
    }
}

#[test]
fn tie_across_barline_keeps_flat_against_neutral_key() {
    // `_B-` ties a flat B across the barline in key C; the stop note must
    // remain flat (alter -1) rather than reverting to natural.
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nA G _B- | B A2 z |\n";
    let export = export_musicxml(source).expect("tied score should export");
    assert_balanced_xml(&export.musicxml);

    let notes = note_steps_and_alters(&export.musicxml);
    let b_notes: Vec<(char, i8)> = notes.into_iter().filter(|(step, _)| *step == 'B').collect();
    assert_eq!(
        b_notes.len(),
        2,
        "expected two B notes (tie start and stop)"
    );
    for (step, alter) in b_notes {
        assert_eq!((step, alter), ('B', -1), "tied B must stay flat across bar");
    }
}

#[test]
fn mid_tune_key_and_meter_changes_emit_attributes() {
    let source = "X:1\nL:1/4\nK:C\nCDEF|[K:F]GAB_B|[M:3/4]ABc|\n";
    let export = export_musicxml(source).expect("score should export");
    let xml = export.musicxml;
    // measure 2 opens with a key change to one flat; measure 3 with 3/4 time
    let measures: Vec<&str> = xml.split("<measure ").collect();
    assert!(measures.len() >= 4, "three measures: {xml}");
    assert!(
        measures[2].contains("<fifths>-1</fifths>"),
        "measure 2 carries the key change: {}",
        measures[2]
    );
    assert!(
        measures[3].contains("<beats>3</beats>")
            && measures[3].contains("<beat-type>4</beat-type>"),
        "measure 3 carries the meter change: {}",
        measures[3]
    );
    // header attributes unchanged
    assert!(measures[1].contains("<fifths>0</fifths>"));
}

#[test]
fn mid_measure_key_change_emits_attributes_between_notes() {
    let source = "X:1\nL:1/4\nK:C\nCD[K:D]EF|\n";
    let export = export_musicxml(source).expect("score should export");
    let xml = export.musicxml;
    // the second <attributes> (fifths 2) appears after the 2nd note and
    // before the 3rd note within measure 1
    let m1_start = xml.find("<measure ").expect("measure 1");
    let m1 = &xml[m1_start..];
    let change = m1
        .find("<fifths>2</fifths>")
        .expect("mid-measure key change");
    let notes: Vec<usize> = m1.match_indices("<note>").map(|(i, _)| i).collect();
    assert!(notes.len() >= 4);
    assert!(
        notes[1] < change && change < notes[2],
        "key change sits between note 2 and note 3"
    );
}

#[test]
fn grace_implicit_alter_uses_position_active_key() {
    // After [K:D] (two sharps), an unmarked grace f must export F#.
    let source = "X:1\nL:1/4\nK:C\nCDEF|[K:D]{f}GABc|\n";
    let export = export_musicxml(source).expect("score should export");
    let xml = export.musicxml;
    let grace_at = xml.find("<grace/>").expect("grace note present");
    let after = &xml[grace_at..];
    assert!(
        after.contains("<step>F</step>")
            && after[..after.find("<octave>").expect("octave")].contains("<alter>1</alter>"),
        "grace F carries the new key's sharp: {}",
        &after[..300.min(after.len())]
    );
}

#[test]
fn tie_keeps_pitch_across_mid_measure_key_change() {
    // The tie-stop F must stay F# (the tied pitch), not re-resolve to F
    // natural under the new key — and the reverse direction likewise.
    for (source, want_alter) in [
        ("X:1\nM:4/4\nL:1/4\nK:D\nFGAF|F-[K:C]FGA|\n", "1"),
        ("X:1\nM:4/4\nL:1/4\nK:C\nFGAF|F-[K:D]FGA|\n", "0"),
    ] {
        let export = export_musicxml(source).expect("score should export");
        let xml = export.musicxml;
        let stop = xml
            .find("tied type=\"stop\"")
            .or_else(|| xml.find("<tie type=\"stop\""))
            .expect("tie stop present");
        let before = &xml[..stop];
        let note_start = before.rfind("<note>").expect("stop note");
        let note = &xml[note_start..stop];
        let alter = note
            .find("<alter>")
            .map(|i| &note[i + 7..i + 8])
            .unwrap_or("0");
        assert_eq!(alter, want_alter, "tie-stop alter for {source:?}: {note}");
    }
}

#[test]
fn free_meter_change_emits_no_empty_attributes() {
    let source = "X:1\nM:4/4\nL:1/8\nK:C\nCDEF|[M:none]GABcdefg|\n";
    let export = export_musicxml(source).expect("score should export");
    assert!(
        !export.musicxml.contains("<attributes></attributes>")
            && !export.musicxml.contains("<attributes/>"),
        "no empty attributes wrapper"
    );
}
