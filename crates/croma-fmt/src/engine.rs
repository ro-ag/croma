//! The canonical, token-preserving formatting engine.
//!
//! Musical tokens are copied verbatim by their source byte span; only the
//! whitespace *between* tokens, blank-line runs, and the final newline are
//! normalized. Because no musical byte is ever reconstructed, the formatting is
//! lossless by construction, and collapsing a run of spaces to a single space
//! preserves beaming (a break stays a break; adjacency stays adjacency).

use std::collections::HashMap;

use croma_core::{MusicLine, MusicTokenKind, ParseOptions, parse_document};

/// Produce the canonical formatting of `source`.
pub(crate) fn format(source: &str, options: ParseOptions) -> String {
    let report = parse_document(source, options);
    let document = report.value;

    let mut music_lines: HashMap<usize, &MusicLine> = HashMap::new();
    for tune in &document.music.tunes {
        for line in &tune.lines {
            music_lines.insert(line.line_index, line);
        }
    }

    let mut out = String::with_capacity(source.len());
    let mut pending_blank = false;

    for (index, raw) in source.lines().enumerate() {
        let line = match music_lines.get(&index) {
            Some(music_line) if !music_line.tokens.is_empty() => {
                format_music_line(source, music_line)
            }
            _ => raw.trim_end().to_string(),
        };

        if line.is_empty() {
            // Blank line: collapse runs, never emit a leading blank.
            pending_blank = !out.is_empty();
            continue;
        }

        if pending_blank {
            out.push('\n');
            pending_blank = false;
        }
        out.push_str(&line);
        out.push('\n');
    }

    out
}

/// Reconstruct one music line from its classified tokens: copy each non-space
/// token verbatim, collapse each whitespace run to a single space, and drop
/// leading/trailing whitespace.
fn format_music_line(source: &str, line: &MusicLine) -> String {
    let mut out = String::new();
    let mut pending_space = false;

    for token in &line.tokens {
        match token.kind {
            MusicTokenKind::Whitespace => pending_space = true,
            _ => {
                let slice = source.get(token.span.start..token.span.end).unwrap_or("");
                if !out.is_empty() && pending_space {
                    out.push(' ');
                }
                out.push_str(slice);
                pending_space = false;
            }
        }
    }

    out.trim_end().to_string()
}
