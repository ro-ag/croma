use crate::Span;

const UTF8_BOM: &str = "\u{feff}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    text: String,
    file_name: Option<String>,
    content_start: usize,
    line_starts: Vec<usize>,
    lines: Vec<SourceLine>,
}

impl SourceText {
    pub fn new(text: impl Into<String>) -> Self {
        Self::from_parts(text.into(), None)
    }

    pub fn with_file_name(text: impl Into<String>, file_name: impl Into<String>) -> Self {
        Self::from_parts(text.into(), Some(file_name.into()))
    }

    fn from_parts(text: String, file_name: Option<String>) -> Self {
        let content_start = text.strip_prefix(UTF8_BOM).map_or(0, |_| UTF8_BOM.len());
        let (line_starts, lines) = collect_lines(&text, content_start);

        Self {
            text,
            file_name,
            content_start,
            line_starts,
            lines,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    pub fn content(&self) -> &str {
        &self.text[self.content_start..]
    }

    pub fn content_start(&self) -> usize {
        self.content_start
    }

    pub fn content_span(&self) -> Span {
        Span::new(self.content_start, self.text.len())
    }

    pub fn has_leading_bom(&self) -> bool {
        self.content_start > 0
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content().is_empty()
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    pub fn line(&self, index: usize) -> Option<SourceLine> {
        self.lines.get(index).copied()
    }

    pub fn line_text(&self, index: usize) -> Option<&str> {
        self.line(index)
            .and_then(|line| self.slice(line.text_span()))
    }

    pub fn line_with_ending(&self, index: usize) -> Option<&str> {
        self.line(index)
            .and_then(|line| self.slice(line.full_span()))
    }

    pub fn line_ending(&self, index: usize) -> Option<LineEnding> {
        self.line(index).and_then(|line| line.ending())
    }

    pub fn slice(&self, span: Span) -> Option<&str> {
        if span.start > span.end || span.end > self.text.len() {
            return None;
        }
        if !self.text.is_char_boundary(span.start) || !self.text.is_char_boundary(span.end) {
            return None;
        }
        Some(&self.text[span.start..span.end])
    }

    pub fn line_column(&self, offset: usize) -> Option<LineColumn> {
        if offset < self.content_start || offset > self.text.len() {
            return None;
        }
        if !self.text.is_char_boundary(offset) {
            return None;
        }

        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(0) => return None,
            Err(index) => index - 1,
        };
        let line = self.lines[line_index];
        let column_offset = offset.min(line.text_span.end);
        let column = self.text[line.text_span.start..column_offset]
            .chars()
            .count()
            + 1;

        Some(LineColumn::new(line_index + 1, column))
    }

    pub fn line_column_span(&self, span: Span) -> Option<LineColumnSpan> {
        if span.start > span.end {
            return None;
        }

        Some(LineColumnSpan {
            start: self.line_column(span.start)?,
            end: self.line_column(span.end)?,
        })
    }

    pub fn byte_offset(&self, position: LineColumn) -> Option<usize> {
        if position.line == 0 || position.column == 0 {
            return None;
        }

        let line = self.lines.get(position.line - 1)?;
        if position.column == 1 {
            return Some(line.text_span.start);
        }

        let mut column = 1;
        for (relative_offset, ch) in
            self.text[line.text_span.start..line.text_span.end].char_indices()
        {
            column += 1;
            let next_offset = line.text_span.start + relative_offset + ch.len_utf8();
            if column == position.column {
                return Some(next_offset);
            }
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLine {
    text_span: Span,
    full_span: Span,
    ending: Option<LineEnding>,
}

impl SourceLine {
    pub fn text_span(self) -> Span {
        self.text_span
    }

    pub fn full_span(self) -> Span {
        self.full_span
    }

    pub fn start(self) -> usize {
        self.text_span.start
    }

    pub fn text_end(self) -> usize {
        self.text_span.end
    }

    pub fn end(self) -> usize {
        self.full_span.end
    }

    pub fn ending(self) -> Option<LineEnding> {
        self.ending
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumn {
    pub line: usize,
    pub column: usize,
}

impl LineColumn {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumnSpan {
    pub start: LineColumn,
    pub end: LineColumn,
}

fn collect_lines(text: &str, content_start: usize) -> (Vec<usize>, Vec<SourceLine>) {
    let mut line_starts = Vec::new();
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut line_start = content_start;
    let mut index = content_start;

    while index < bytes.len() {
        let (ending, ending_len) = match bytes[index] {
            b'\n' => (Some(LineEnding::Lf), 1),
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => (Some(LineEnding::Crlf), 2),
            b'\r' => (Some(LineEnding::Cr), 1),
            _ => {
                index += 1;
                continue;
            }
        };

        push_line(
            &mut line_starts,
            &mut lines,
            line_start,
            index,
            index + ending_len,
            ending,
        );
        index += ending_len;
        line_start = index;
    }

    push_line(
        &mut line_starts,
        &mut lines,
        line_start,
        text.len(),
        text.len(),
        None,
    );

    (line_starts, lines)
}

fn push_line(
    line_starts: &mut Vec<usize>,
    lines: &mut Vec<SourceLine>,
    start: usize,
    text_end: usize,
    end: usize,
    ending: Option<LineEnding>,
) {
    line_starts.push(start);
    lines.push(SourceLine {
        text_span: Span::new(start, text_end),
        full_span: Span::new(start, end),
        ending,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{assert_span_text, span_of};

    #[test]
    fn maps_utf8_byte_spans_to_line_columns() {
        let source = SourceText::new("T:Cafe\u{301}\nK:C\n");
        let accent = span_of(&source, "\u{301}");

        assert_eq!(accent, Span::new(6, 8));
        assert_eq!(source.slice(accent), Some("\u{301}"));
        assert_eq!(
            source.line_column(accent.start),
            Some(LineColumn::new(1, 7))
        );
        assert_eq!(
            source.byte_offset(LineColumn::new(1, 7)),
            Some(accent.start)
        );
        assert_span_text(&source, accent, "\u{301}");
    }

    #[test]
    fn maps_multiline_spans_to_line_column_ranges() {
        let source = SourceText::new("X:1\nT:Cafe\u{301}\nK:C");
        let span = span_of(&source, "T:Cafe\u{301}\nK");

        assert_eq!(
            source.line_column_span(span),
            Some(LineColumnSpan {
                start: LineColumn::new(2, 1),
                end: LineColumn::new(3, 2),
            })
        );
    }

    #[test]
    fn preserves_crlf_lf_and_cr_line_endings() {
        let source = SourceText::new("A\r\nB\nC\rD");

        assert_eq!(source.line_starts(), &[0, 3, 5, 7]);
        assert_eq!(source.line_ending(0), Some(LineEnding::Crlf));
        assert_eq!(source.line_ending(1), Some(LineEnding::Lf));
        assert_eq!(source.line_ending(2), Some(LineEnding::Cr));
        assert_eq!(source.line_ending(3), None);
        assert_eq!(source.line_with_ending(0), Some("A\r\n"));
        assert_eq!(source.line_with_ending(1), Some("B\n"));
        assert_eq!(source.line_with_ending(2), Some("C\r"));
        assert_eq!(source.line_text(3), Some("D"));
        assert_eq!(source.line_column(7), Some(LineColumn::new(4, 1)));
    }

    #[test]
    fn ignores_bom_only_at_file_start() {
        let source = SourceText::new("\u{feff}X:1\nT:\u{feff}Title");

        assert!(source.has_leading_bom());
        assert_eq!(source.content_start(), 3);
        assert_eq!(source.line_starts(), &[3, 7]);
        assert_eq!(source.line_column(3), Some(LineColumn::new(1, 1)));
        assert_eq!(source.line_column(0), None);
        assert_eq!(source.line_text(0), Some("X:1"));
        assert_eq!(source.line_text(1), Some("T:\u{feff}Title"));

        let mid_file_bom = span_of(&source, "\u{feff}Title");
        assert_eq!(
            source.line_column(mid_file_bom.start),
            Some(LineColumn::new(2, 3))
        );
        assert_span_text(&source, mid_file_bom, "\u{feff}Title");
    }

    #[test]
    fn can_carry_optional_file_name_for_diagnostics() {
        let source = SourceText::with_file_name("K:C\nC", "fixtures/minimal.abc");

        assert_eq!(source.file_name(), Some("fixtures/minimal.abc"));
    }
}
