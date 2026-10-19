
use super::*;

/// Decode a delta stream back to absolute (line, start, length, type) tuples.
fn decode(tokens: &SemanticTokens) -> Vec<(u32, u32, u32, u32)> {
    let mut out = Vec::new();
    let mut line = 0u32;
    let mut start = 0u32;
    for t in &tokens.data {
        if t.delta_line == 0 {
            start += t.delta_start;
        } else {
            line += t.delta_line;
            start = t.delta_start;
        }
        out.push((line, start, t.length, t.token_type));
    }
    out
}

#[test]
fn legend_indices_match_the_map() {
    // The map must only ever produce indices that exist in the legend.
    let len = legend().token_types.len() as u32;
    for kind in ALL_KINDS {
        if let Some(idx) = token_type_index(kind) {
            assert!(idx < len, "{kind:?} -> {idx} out of legend range {len}");
        }
    }
}

const ALL_KINDS: [MusicTokenKind; 25] = {
    use MusicTokenKind::*;
    [
        Whitespace,
        Accidental,
        Pitch,
        OctaveMark,
        Length,
        Rest,
        MultiMeasureRest,
        Spacer,
        Chord,
        GraceGroup,
        ChordSymbol,
        Annotation,
        Decoration,
        Tuplet,
        Slur,
        Tie,
        BrokenRhythm,
        Overlay,
        RepeatEnding,
        Barline,
        InlineField,
        Unsupported,
        Malformed,
        Comment,
        ScoreLineBreak,
    ]
};

#[test]
fn whitespace_is_the_only_unmapped_kind() {
    for kind in ALL_KINDS {
        let mapped = token_type_index(kind).is_some();
        if matches!(kind, MusicTokenKind::Whitespace) {
            assert!(!mapped, "Whitespace must be skipped");
        } else {
            assert!(mapped, "{kind:?} must map to a token type");
        }
    }
}

#[test]
fn known_layout_emits_expected_tokens() {
    // A minimal tune: "CDEF|" on the body line. Line layout (0-based):
    //   line 0: X:1
    //   line 1: K:C
    //   line 2: C D E F |   (no spaces: "CDEF|")
    // Each pitch is 1 wide; the barline is 1 wide. Whitespace (none here).
    let source = "X:1\nK:C\nCDEF|\n";
    let tokens = semantic_tokens(source, PositionEncoding::Utf8);
    let decoded = decode(&tokens);

    // 4 pitches + 1 barline, all on line 2.
    let pitch = 0u32;
    let barline = 3u32;
    assert_eq!(
        decoded,
        vec![
            (2, 0, 1, pitch),
            (2, 1, 1, pitch),
            (2, 2, 1, pitch),
            (2, 3, 1, pitch),
            (2, 4, 1, barline),
        ]
    );
}

#[test]
fn whitespace_tokens_are_skipped_but_positions_stay_correct() {
    // "C D E" — spaces between pitches are Whitespace and must not appear,
    // yet the pitch start columns must still be 0, 2, 4.
    let source = "X:1\nK:C\nC D E\n";
    let tokens = semantic_tokens(source, PositionEncoding::Utf8);
    let decoded = decode(&tokens);
    let starts: Vec<u32> = decoded.iter().map(|(_, s, _, _)| *s).collect();
    assert_eq!(
        starts,
        vec![0, 2, 4],
        "skipping whitespace keeps columns right"
    );
    assert!(
        decoded
            .iter()
            .all(|(l, _, len, ty)| *l == 2 && *len == 1 && *ty == 0)
    );
}

#[test]
fn multibyte_annotation_length_differs_by_encoding() {
    // An annotation "café" before a note. Under UTF-8 the quoted text is
    // wider (é is 2 bytes) than under UTF-16 (é is 1 unit).
    let source = "X:1\nK:C\n\"café\"C\n";
    let utf8 = decode(&semantic_tokens(source, PositionEncoding::Utf8));
    let utf16 = decode(&semantic_tokens(source, PositionEncoding::Utf16));

    // First token is the annotation/chord-symbol string on line 2 at col 0.
    let (_, _, len8, _) = utf8[0];
    let (_, _, len16, _) = utf16[0];
    // "café" with quotes = 6 chars; UTF-8 adds one byte for é -> 7 vs 6.
    assert_eq!(len8, 7, "utf8 byte length of \"café\"");
    assert_eq!(len16, 6, "utf16 unit length of \"café\"");
}

#[test]
fn delta_encoding_is_monotonic_and_in_bounds() {
    // A denser line exercises within-line deltas and a custom (rest) type.
    let source = "X:1\nK:C\nC2 z A,B' |[K:D]\n";
    let tokens = semantic_tokens(source, PositionEncoding::Utf8);
    let text = SourceText::new(source);
    let mut line = 0u32;
    let mut start = 0u32;
    for t in &tokens.data {
        assert!(t.delta_line < 1_000, "delta_line sane");
        if t.delta_line == 0 {
            start += t.delta_start;
        } else {
            line += t.delta_line;
            start = t.delta_start;
        }
        // Every decoded position must be in-bounds for the document: the
        // line exists and the start column + length stay within the line's
        // UTF-8 width (the encoding used here).
        assert!((line as usize) < text.line_count().max(1), "line in bounds");
        let width = text
            .line(line as usize)
            .and_then(|l| text.slice(croma_core::Span::new(l.start(), l.text_end())))
            .map(|s| s.len() as u32)
            .unwrap_or(0);
        assert!(start + t.length <= width, "token within line width");
    }
}

#[test]
fn empty_and_malformed_sources_never_panic() {
    for source in ["", "\n\n", "X:1\n", "[[[[\nK:C\n)))\n", "not abc é\n"] {
        for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            let _ = semantic_tokens(source, enc);
        }
    }
}

#[test]
fn container_tokens_are_kept_and_inner_tokens_dropped() {
    // A chord "[CEG]2" yields a Chord container token plus inner Pitch/Length
    // tokens; the emitter must keep the container and drop the nested ones,
    // so the stream stays non-overlapping. Same for a grace group "{ab}".
    let source = "X:1\nK:C\n[CEG]2 {ab}c |\n";
    let tokens = semantic_tokens(source, PositionEncoding::Utf8);
    let decoded = decode(&tokens);

    // Reconstruct absolute (start, end) columns and assert strictly ordered,
    // non-overlapping.
    let mut prev_end = 0u32;
    let mut prev_line = u32::MAX;
    for (line, start, length, _ty) in &decoded {
        if *line != prev_line {
            prev_end = 0;
            prev_line = *line;
        }
        assert!(
            *start >= prev_end,
            "token at {line}:{start} overlaps prev end {prev_end}"
        );
        prev_end = start + length;
    }

    // The chord container (MACRO=6) must be present covering "[CEG]2".
    let chord_macro = decoded
        .iter()
        .find(|(l, s, _, ty)| *l == 2 && *s == 0 && *ty == 6);
    assert!(
        chord_macro.is_some(),
        "chord container token present: {decoded:?}"
    );
    let (_, _, chord_len, _) = chord_macro.expect("chord token");
    assert_eq!(*chord_len, 6, "chord token covers \"[CEG]2\"");
}

#[test]
fn tokens_cover_exactly_the_non_whitespace_spans() {
    // Exhaustiveness in miniature (leg D (i)): the set of (start-byte,
    // end-byte) we emit equals the set of non-whitespace MusicToken spans.
    let source = "X:1\nK:C\n\"Am\"C2 D-D z2 |]\n";
    let report = parse_document(source, ParseOptions::default());
    let text = &report.value.source;

    // Expected spans straight from the parser (excluding Whitespace).
    let mut expected: Vec<(u32, u32)> = Vec::new();
    for tune in &report.value.music.tunes {
        for line in &tune.lines {
            for tok in &line.tokens {
                if matches!(tok.kind, MusicTokenKind::Whitespace) {
                    continue;
                }
                let s = byte_to_position(text, tok.span.start, PositionEncoding::Utf8);
                let e = byte_to_position(text, tok.span.end, PositionEncoding::Utf8);
                // Single-line tokens: encode as (line*10000 + col) pairs.
                expected.push((s.line * 10_000 + s.character, e.line * 10_000 + e.character));
            }
        }
    }
    expected.sort_unstable();

    // Emitted spans, reconstructed from the delta stream.
    let tokens = semantic_tokens(source, PositionEncoding::Utf8);
    let mut emitted: Vec<(u32, u32)> = Vec::new();
    let mut line = 0u32;
    let mut start = 0u32;
    for t in &tokens.data {
        if t.delta_line == 0 {
            start += t.delta_start;
        } else {
            line += t.delta_line;
            start = t.delta_start;
        }
        emitted.push((line * 10_000 + start, line * 10_000 + start + t.length));
    }
    emitted.sort_unstable();

    assert_eq!(
        emitted, expected,
        "emitted spans must equal non-whitespace token spans"
    );
}
