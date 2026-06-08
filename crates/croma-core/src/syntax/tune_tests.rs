    use super::*;

    #[test]
    fn records_basic_field_and_note_spans() {
        let surface = analyze("X:1\nK:C\nC|z\n");

        assert_eq!(surface.tokens_of_kind(SurfaceKind::Field).count(), 2);
        assert_eq!(surface.tokens_of_kind(SurfaceKind::Note).count(), 1);
        assert_eq!(surface.tokens_of_kind(SurfaceKind::Barline).count(), 1);
        assert_eq!(surface.tokens_of_kind(SurfaceKind::Rest).count(), 1);
    }

    #[test]
    fn records_spans_after_crlf_line_endings() {
        let source = SourceText::new("X:1\r\nK:C\r\nC|z");
        let surface = analyze_source(&source);

        let note = surface
            .tokens_of_kind(SurfaceKind::Note)
            .next()
            .expect("expected note token");

        assert_eq!(note.span, Span::new(10, 11));
        assert_eq!(source.slice(note.span), Some("C"));
    }

    #[test]
    fn classifies_file_header_followed_by_multiple_tunes() {
        let source = SourceText::new(
            "%abc-2.1\nM:4/4\nL:1/8\n\nX:1\nT:One\nK:C\nC|\n\nX:2\nT:Two\nK:G\nG|\n",
        );
        let surface = analyze_source(&source);
        let line_map = &surface.line_map;

        assert_eq!(line_map.lines[0].kind, LineKind::VersionLine);
        assert_eq!(line_map.lines[1].context, LineContext::FileHeader);
        assert_eq!(line_map.lines[2].context, LineContext::FileHeader);
        assert_eq!(
            line_map.blocks_of_kind(SourceBlockKind::FileHeader).count(),
            1
        );
        assert_eq!(line_map.tunes.len(), 2);
        assert_eq!(line_map.tunes[0].header_line_start, 4);
        assert_eq!(line_map.tunes[0].body_line_start, 7);
        assert_eq!(line_map.tunes[0].terminator_line, Some(8));
        assert_eq!(line_map.tunes[1].header_line_start, 9);
        assert_eq!(line_map.lines[7].kind, LineKind::MusicCode);
        assert_eq!(line_map.lines[12].kind, LineKind::MusicCode);
    }

    #[test]
    fn records_free_text_between_tunes() {
        let source =
            SourceText::new("X:1\nK:C\nC|\n\nnotes between tunes\nmore notes\n\nX:2\nK:G\nG|\n");
        let surface = analyze_source(&source);
        let line_map = &surface.line_map;

        assert_eq!(line_map.tunes.len(), 2);
        assert_eq!(line_map.lines[4].kind, LineKind::FreeText);
        assert_eq!(line_map.lines[5].kind, LineKind::FreeText);
        assert_eq!(line_map.lines[4].context, LineContext::FreeText);
        assert_eq!(
            line_map.blocks_of_kind(SourceBlockKind::FreeText).count(),
            1
        );
        assert_eq!(surface.tokens_of_kind(SurfaceKind::Note).count(), 2);
    }

    #[test]
    fn k_field_ends_tune_header() {
        let surface = analyze("X:1\nT:Title\nK:C\nCDEF\n");
        let tune = surface
            .line_map
            .tunes
            .first()
            .expect("expected one tune block");

        assert_eq!(surface.line_map.lines[2].field_code(), Some('K'));
        assert_eq!(
            surface.line_map.lines[2].context,
            LineContext::TuneHeader { tune_index: 0 }
        );
        assert_eq!(
            surface.line_map.lines[3].context,
            LineContext::TuneBody { tune_index: 0 }
        );
        assert_eq!(tune.header_line_end, 3);
        assert_eq!(tune.body_line_start, 3);
    }

    #[test]
    fn records_field_continuation_edges() {
        let source = SourceText::new("X:1\nT:Long title\n+:second line\nK:C\nC|\n");
        let surface = analyze_source(&source);
        let line_map = &surface.line_map;

        assert_eq!(line_map.lines[2].kind, LineKind::FieldContinuation);
        assert_eq!(
            line_map.lines[2].context,
            LineContext::TuneHeader { tune_index: 0 }
        );

        let edge = line_map
            .continuation_edges
            .iter()
            .find(|edge| edge.kind == ContinuationKind::FieldContinuation)
            .expect("expected field continuation edge");
        assert_eq!(edge.from_line, 1);
        assert_eq!(edge.to_line, 2);
        assert_eq!(source.slice(edge.marker_span), Some("+:"));
    }

    #[test]
    fn records_backslash_suppressed_score_line_breaks() {
        let source = SourceText::new("X:1\nK:C\nC D \\\n% comment in between\n%%score V1\nE F\n");
        let surface = analyze_source(&source);
        let line_map = &surface.line_map;

        assert_eq!(
            line_map.lines[2].score_line_break,
            ScoreLineBreak::Suppressed {
                marker_span: Span::new(12, 13)
            }
        );

        let edge = line_map
            .continuation_edges
            .iter()
            .find(|edge| edge.kind == ContinuationKind::MusicBackslash)
            .expect("expected music continuation edge");
        assert_eq!(edge.from_line, 2);
        assert_eq!(edge.to_line, 5);
        assert_eq!(source.slice(edge.marker_span), Some("\\"));
    }

    #[test]
    fn comments_before_and_after_music_do_not_terminate_tune() {
        let surface = analyze("X:1\nK:C\n% before\nC|\n% after\nD|\n");
        let line_map = &surface.line_map;

        assert_eq!(line_map.tunes.len(), 1);
        assert_eq!(line_map.lines[2].kind, LineKind::Comment);
        assert_eq!(
            line_map.lines[2].context,
            LineContext::TuneBody { tune_index: 0 }
        );
        assert_eq!(line_map.lines[4].kind, LineKind::Comment);
        assert_eq!(
            line_map.lines[4].context,
            LineContext::TuneBody { tune_index: 0 }
        );
        assert_eq!(line_map.lines[5].kind, LineKind::MusicCode);
        assert_eq!(line_map.tunes[0].terminator_line, None);
    }

    #[test]
    fn directives_free_text_and_field_continuations_do_not_enter_note_stream() {
        let surface = analyze(
            "X:1\nT:Title\n+:Letters ABC\nK:C\n%%text ABC\nC|\n\nfree ABC text\n\nX:2\nK:G\nG|\n",
        );

        assert_eq!(surface.line_map.lines[2].kind, LineKind::FieldContinuation);
        assert_eq!(
            surface.line_map.lines[4].kind,
            LineKind::TypesetTextDirective
        );
        assert_eq!(surface.line_map.lines[7].kind, LineKind::FreeText);
        assert_eq!(surface.tokens_of_kind(SurfaceKind::Note).count(), 2);
    }

    #[test]
    fn preserves_non_note_items_with_spans() {
        let source =
            SourceText::new("X:1\nT:Title\n+:More title\nK:C\nC % inline\n%%text printable\n");
        let surface = analyze_source(&source);
        let non_notes = &surface.line_map.non_note_items;

        let continuation = non_notes
            .iter()
            .find(|item| item.kind == NonNoteKind::FieldContinuation)
            .expect("expected field continuation item");
        assert_eq!(source.slice(continuation.span), Some("+:More title"));

        let inline_comment = non_notes
            .iter()
            .find(|item| item.kind == NonNoteKind::InlineComment)
            .expect("expected inline comment item");
        assert_eq!(source.slice(inline_comment.span), Some("% inline"));

        let typeset = non_notes
            .iter()
            .find(|item| item.kind == NonNoteKind::TypesetTextDirective)
            .expect("expected typeset text item");
        assert_eq!(source.slice(typeset.span), Some("%%text printable"));
    }

    #[test]
    fn classifies_stylesheet_and_typeset_text_directives() {
        let surface = analyze(
            "X:1\nK:C\n%%score V1\n%%text printable\n%%begintext\n%%inside\n%%endtext\nC|\n",
        );
        let line_map = &surface.line_map;

        assert_eq!(line_map.lines[2].kind, LineKind::StylesheetDirective);
        assert_eq!(line_map.lines[3].kind, LineKind::TypesetTextDirective);
        assert_eq!(line_map.lines[4].kind, LineKind::TypesetTextDirective);
        assert_eq!(line_map.lines[5].kind, LineKind::TypesetTextDirective);
        assert_eq!(line_map.lines[6].kind, LineKind::TypesetTextDirective);
        assert_eq!(
            line_map
                .blocks_of_kind(SourceBlockKind::TypesetText)
                .count(),
            1
        );
    }
