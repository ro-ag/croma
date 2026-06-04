use crate::Span;
use crate::source::SourceText;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SurfaceMap {
    pub tokens: Vec<SurfaceToken>,
}

impl SurfaceMap {
    pub fn tokens_of_kind(&self, kind: SurfaceKind) -> impl Iterator<Item = &SurfaceToken> {
        self.tokens.iter().filter(move |token| token.kind == kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceToken {
    pub kind: SurfaceKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Field,
    Comment,
    Barline,
    Note,
    Rest,
    Other,
}

pub fn analyze(source: &str) -> SurfaceMap {
    analyze_source(&SourceText::new(source))
}

pub fn analyze_source(source: &SourceText) -> SurfaceMap {
    let mut tokens = Vec::new();

    for index in 0..source.line_count() {
        let Some(line) = source.line(index) else {
            continue;
        };
        let Some(line_text) = source.line_text(index) else {
            continue;
        };
        analyze_line(line_text, line.start(), &mut tokens);
    }

    SurfaceMap { tokens }
}

fn analyze_line(line: &str, line_offset: usize, tokens: &mut Vec<SurfaceToken>) {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    if trimmed.is_empty() {
        return;
    }

    if trimmed.starts_with('%') {
        tokens.push(SurfaceToken {
            kind: SurfaceKind::Comment,
            span: Span::new(line_offset + leading, line_offset + line.len()),
        });
        return;
    }

    if is_field(trimmed) {
        tokens.push(SurfaceToken {
            kind: SurfaceKind::Field,
            span: Span::new(line_offset + leading, line_offset + leading + 2),
        });
        return;
    }

    for (column, ch) in line.char_indices() {
        let kind = match ch {
            '%' => {
                tokens.push(SurfaceToken {
                    kind: SurfaceKind::Comment,
                    span: Span::new(line_offset + column, line_offset + line.len()),
                });
                break;
            }
            '|' => SurfaceKind::Barline,
            'A'..='G' | 'a'..='g' => SurfaceKind::Note,
            'z' | 'x' => SurfaceKind::Rest,
            _ => SurfaceKind::Other,
        };
        if kind != SurfaceKind::Other {
            tokens.push(SurfaceToken {
                kind,
                span: Span::new(line_offset + column, line_offset + column + ch.len_utf8()),
            });
        }
    }
}

fn is_field(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
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
}
