use crate::Span;
use crate::source::SourceText;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SurfaceMap {
    pub line_map: LineMap,
    pub tokens: Vec<SurfaceToken>,
}

impl SurfaceMap {
    pub fn tokens_of_kind(&self, kind: SurfaceKind) -> impl Iterator<Item = &SurfaceToken> {
        self.tokens.iter().filter(move |token| token.kind == kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineMap {
    pub lines: Vec<ClassifiedLine>,
    pub blocks: Vec<SourceBlock>,
    pub tunes: Vec<TuneBlock>,
    pub continuation_edges: Vec<ContinuationEdge>,
    pub non_note_items: Vec<NonNoteItem>,
}

impl LineMap {
    pub fn lines_of_kind(&self, kind: LineKind) -> impl Iterator<Item = &ClassifiedLine> {
        self.lines.iter().filter(move |line| line.kind == kind)
    }

    pub fn blocks_of_kind(&self, kind: SourceBlockKind) -> impl Iterator<Item = &SourceBlock> {
        self.blocks.iter().filter(move |block| block.kind == kind)
    }

    pub fn music_lines(&self) -> impl Iterator<Item = &ClassifiedLine> {
        self.lines_of_kind(LineKind::MusicCode)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedLine {
    pub index: usize,
    pub kind: LineKind,
    pub context: LineContext,
    pub span: Span,
    pub text_span: Span,
    pub content_span: Span,
    pub marker_span: Option<Span>,
    pub field: Option<FieldHeader>,
    pub trailing_comment: Option<Span>,
    pub score_line_break: ScoreLineBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    VersionLine,
    EmptyLine,
    Comment,
    StylesheetDirective,
    InformationField,
    FieldContinuation,
    MusicCode,
    FreeText,
    TypesetTextDirective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineContext {
    Preamble,
    FileHeader,
    BetweenBlocks,
    FreeText,
    TypesetText,
    TuneHeader { tune_index: usize },
    TuneBody { tune_index: usize },
    TuneTerminator { tune_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldHeader {
    pub code: char,
    pub marker_span: Span,
    pub value_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreLineBreak {
    NotApplicable,
    Physical,
    Suppressed { marker_span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBlock {
    pub kind: SourceBlockKind,
    pub span: Span,
    pub line_start: usize,
    pub line_end: usize,
    pub tune_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceBlockKind {
    FileHeader,
    Tune,
    FreeText,
    TypesetText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuneBlock {
    pub index: usize,
    pub span: Span,
    pub header_span: Span,
    pub body_span: Span,
    pub line_start: usize,
    pub line_end: usize,
    pub header_line_start: usize,
    pub header_line_end: usize,
    pub body_line_start: usize,
    pub body_line_end: usize,
    pub terminator_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinuationEdge {
    pub kind: ContinuationKind,
    pub from_line: usize,
    pub to_line: usize,
    pub span: Span,
    pub marker_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationKind {
    FieldContinuation,
    MusicBackslash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonNoteItem {
    pub kind: NonNoteKind,
    pub line_index: usize,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonNoteKind {
    VersionLine,
    EmptyLine,
    Comment,
    InlineComment,
    StylesheetDirective,
    InformationField,
    FieldContinuation,
    FreeText,
    TypesetTextDirective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceToken {
    pub kind: SurfaceKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Field,
    FieldContinuation,
    Comment,
    Directive,
    FreeText,
    Barline,
    Note,
    Rest,
    Other,
}

pub fn analyze(source: &str) -> SurfaceMap {
    analyze_source(&SourceText::new(source))
}

pub fn analyze_source(source: &SourceText) -> SurfaceMap {
    let line_map = classify_lines(source);
    let tokens = tokenize_surface(source, &line_map);

    SurfaceMap { line_map, tokens }
}

fn classify_lines(source: &SourceText) -> LineMap {
    let mut builder = LineMapBuilder::new(source);
    builder.classify();
    builder.finish()
}

struct LineMapBuilder<'source> {
    source: &'source SourceText,
    lines: Vec<ClassifiedLine>,
    blocks: Vec<SourceBlock>,
    tunes: Vec<TuneBlock>,
    continuation_edges: Vec<ContinuationEdge>,
    non_note_items: Vec<NonNoteItem>,
    state: DocumentState,
    next_tune_index: usize,
    previous_field_line: Option<usize>,
    pending_music_continuation: Option<PendingMusicContinuation>,
    typeset_text_block_start: Option<usize>,
    in_begintext_block: bool,
}

impl<'source> LineMapBuilder<'source> {
    fn new(source: &'source SourceText) -> Self {
        Self {
            source,
            lines: Vec::new(),
            blocks: Vec::new(),
            tunes: Vec::new(),
            continuation_edges: Vec::new(),
            non_note_items: Vec::new(),
            state: DocumentState::Start,
            next_tune_index: 0,
            previous_field_line: None,
            pending_music_continuation: None,
            typeset_text_block_start: None,
            in_begintext_block: false,
        }
    }

    fn classify(&mut self) {
        for index in 0..self.source.line_count() {
            let Some(source_line) = self.source.line(index) else {
                continue;
            };
            let Some(text) = self.source.line_text(index) else {
                continue;
            };

            let raw_line = classify_raw_line(
                index,
                source_line.full_span(),
                source_line.text_span(),
                text,
                self.in_begintext_block,
            );

            let line = self.apply_structure(raw_line);
            self.track_typeset_block(&line);
            self.track_non_note_items(&line);
            self.track_field_continuation(&line);
            self.track_music_continuation(&line);
            self.update_begintext_state(&line, text);
            self.lines.push(line);
        }
    }

    fn apply_structure(&mut self, mut line: ClassifiedLine) -> ClassifiedLine {
        line.context = match self.state.clone() {
            DocumentState::Start => self.apply_start_structure(&mut line),
            DocumentState::FileHeader { start_line } => {
                self.apply_file_header_structure(&mut line, start_line)
            }
            DocumentState::BetweenBlocks => self.apply_between_blocks_structure(&mut line),
            DocumentState::FreeText { start_line } => {
                self.apply_free_text_structure(&mut line, start_line)
            }
            DocumentState::TuneHeader { tune } => self.apply_tune_header_structure(&mut line, tune),
            DocumentState::TuneBody { tune } => self.apply_tune_body_structure(&mut line, tune),
        };

        if line.kind != LineKind::MusicCode {
            line.score_line_break = ScoreLineBreak::NotApplicable;
        }

        line
    }

    fn apply_start_structure(&mut self, line: &mut ClassifiedLine) -> LineContext {
        match line.kind {
            LineKind::VersionLine | LineKind::EmptyLine | LineKind::Comment => {
                LineContext::Preamble
            }
            LineKind::InformationField if line.field_code() == Some('X') => self.start_tune(line),
            LineKind::InformationField
            | LineKind::FieldContinuation
            | LineKind::StylesheetDirective => {
                self.state = DocumentState::FileHeader {
                    start_line: line.index,
                };
                LineContext::FileHeader
            }
            LineKind::TypesetTextDirective => {
                self.state = DocumentState::BetweenBlocks;
                LineContext::TypesetText
            }
            LineKind::FreeText | LineKind::MusicCode => {
                line.kind = LineKind::FreeText;
                self.state = DocumentState::FreeText {
                    start_line: line.index,
                };
                LineContext::FreeText
            }
        }
    }

    fn apply_file_header_structure(
        &mut self,
        line: &mut ClassifiedLine,
        start_line: usize,
    ) -> LineContext {
        if line.kind == LineKind::EmptyLine && self.is_synthetic_final_empty_line(line) {
            self.close_file_header(start_line, line.index);
            self.state = DocumentState::BetweenBlocks;
            return LineContext::BetweenBlocks;
        }

        match line.kind {
            LineKind::EmptyLine => {
                self.close_file_header(start_line, line.index);
                self.state = DocumentState::BetweenBlocks;
                LineContext::BetweenBlocks
            }
            LineKind::InformationField if line.field_code() == Some('X') => {
                self.close_file_header(start_line, line.index);
                self.start_tune(line)
            }
            LineKind::InformationField
            | LineKind::FieldContinuation
            | LineKind::StylesheetDirective
            | LineKind::Comment => LineContext::FileHeader,
            LineKind::TypesetTextDirective => {
                self.close_file_header(start_line, line.index);
                self.state = DocumentState::BetweenBlocks;
                LineContext::TypesetText
            }
            LineKind::VersionLine => LineContext::FileHeader,
            LineKind::FreeText | LineKind::MusicCode => {
                self.close_file_header(start_line, line.index);
                line.kind = LineKind::FreeText;
                self.state = DocumentState::FreeText {
                    start_line: line.index,
                };
                LineContext::FreeText
            }
        }
    }

    fn apply_between_blocks_structure(&mut self, line: &mut ClassifiedLine) -> LineContext {
        match line.kind {
            LineKind::InformationField if line.field_code() == Some('X') => self.start_tune(line),
            LineKind::TypesetTextDirective => LineContext::TypesetText,
            LineKind::FreeText | LineKind::MusicCode => {
                line.kind = LineKind::FreeText;
                self.state = DocumentState::FreeText {
                    start_line: line.index,
                };
                LineContext::FreeText
            }
            LineKind::VersionLine
            | LineKind::EmptyLine
            | LineKind::Comment
            | LineKind::StylesheetDirective
            | LineKind::InformationField
            | LineKind::FieldContinuation => LineContext::BetweenBlocks,
        }
    }

    fn apply_free_text_structure(
        &mut self,
        line: &mut ClassifiedLine,
        start_line: usize,
    ) -> LineContext {
        if line.kind == LineKind::EmptyLine && self.is_synthetic_final_empty_line(line) {
            self.close_free_text_block(start_line, line.index);
            self.state = DocumentState::BetweenBlocks;
            return LineContext::BetweenBlocks;
        }

        match line.kind {
            LineKind::EmptyLine => {
                self.close_free_text_block(start_line, line.index);
                self.state = DocumentState::BetweenBlocks;
                LineContext::FreeText
            }
            LineKind::InformationField if line.field_code() == Some('X') => {
                self.close_free_text_block(start_line, line.index);
                self.start_tune(line)
            }
            LineKind::TypesetTextDirective => {
                self.close_free_text_block(start_line, line.index);
                self.state = DocumentState::BetweenBlocks;
                LineContext::TypesetText
            }
            LineKind::FreeText | LineKind::MusicCode => {
                line.kind = LineKind::FreeText;
                LineContext::FreeText
            }
            LineKind::VersionLine
            | LineKind::Comment
            | LineKind::StylesheetDirective
            | LineKind::InformationField
            | LineKind::FieldContinuation => LineContext::FreeText,
        }
    }

    fn apply_tune_header_structure(
        &mut self,
        line: &mut ClassifiedLine,
        mut tune: OpenTune,
    ) -> LineContext {
        if line.kind == LineKind::EmptyLine && self.is_synthetic_final_empty_line(line) {
            self.close_tune(tune, line.index, None);
            self.state = DocumentState::BetweenBlocks;
            return LineContext::BetweenBlocks;
        }

        match line.kind {
            LineKind::EmptyLine => {
                let tune_index = tune.index;
                self.close_tune(tune, line.index, Some(line.index));
                self.state = DocumentState::BetweenBlocks;
                LineContext::TuneTerminator { tune_index }
            }
            LineKind::InformationField
                if line.field_code() == Some('X') && line.index != tune.start_line =>
            {
                self.close_tune(tune, line.index, None);
                self.start_tune(line)
            }
            LineKind::InformationField if line.field_code() == Some('K') => {
                let tune_index = tune.index;
                tune.body_start_line = Some(line.index + 1);
                self.state = DocumentState::TuneBody { tune };
                LineContext::TuneHeader { tune_index }
            }
            LineKind::FreeText | LineKind::MusicCode => {
                line.kind = LineKind::FreeText;
                let tune_index = tune.index;
                self.state = DocumentState::TuneHeader { tune };
                LineContext::TuneHeader { tune_index }
            }
            LineKind::VersionLine
            | LineKind::Comment
            | LineKind::StylesheetDirective
            | LineKind::InformationField
            | LineKind::FieldContinuation
            | LineKind::TypesetTextDirective => {
                let tune_index = tune.index;
                self.state = DocumentState::TuneHeader { tune };
                LineContext::TuneHeader { tune_index }
            }
        }
    }

    fn apply_tune_body_structure(
        &mut self,
        line: &mut ClassifiedLine,
        tune: OpenTune,
    ) -> LineContext {
        if line.kind == LineKind::EmptyLine && self.is_synthetic_final_empty_line(line) {
            self.close_tune(tune, line.index, None);
            self.pending_music_continuation = None;
            self.state = DocumentState::BetweenBlocks;
            return LineContext::BetweenBlocks;
        }

        match line.kind {
            LineKind::EmptyLine => {
                let tune_index = tune.index;
                self.close_tune(tune, line.index, Some(line.index));
                self.pending_music_continuation = None;
                self.state = DocumentState::BetweenBlocks;
                LineContext::TuneTerminator { tune_index }
            }
            LineKind::FreeText | LineKind::MusicCode => {
                line.kind = LineKind::MusicCode;
                let tune_index = tune.index;
                self.state = DocumentState::TuneBody { tune };
                LineContext::TuneBody { tune_index }
            }
            LineKind::VersionLine
            | LineKind::Comment
            | LineKind::StylesheetDirective
            | LineKind::InformationField
            | LineKind::FieldContinuation
            | LineKind::TypesetTextDirective => {
                let tune_index = tune.index;
                self.state = DocumentState::TuneBody { tune };
                LineContext::TuneBody { tune_index }
            }
        }
    }

    fn start_tune(&mut self, line: &ClassifiedLine) -> LineContext {
        let tune_index = self.next_tune_index;
        self.next_tune_index += 1;
        self.pending_music_continuation = None;
        self.state = DocumentState::TuneHeader {
            tune: OpenTune {
                index: tune_index,
                start_line: line.index,
                body_start_line: None,
            },
        };
        LineContext::TuneHeader { tune_index }
    }

    fn close_file_header(&mut self, start_line: usize, end_line: usize) {
        if start_line >= end_line {
            return;
        }

        self.blocks.push(SourceBlock {
            kind: SourceBlockKind::FileHeader,
            span: self.line_range_span(start_line, end_line),
            line_start: start_line,
            line_end: end_line,
            tune_index: None,
        });
    }

    fn close_free_text_block(&mut self, start_line: usize, end_line: usize) {
        if start_line >= end_line {
            return;
        }

        self.blocks.push(SourceBlock {
            kind: SourceBlockKind::FreeText,
            span: self.line_range_span(start_line, end_line),
            line_start: start_line,
            line_end: end_line,
            tune_index: None,
        });
    }

    fn close_tune(&mut self, tune: OpenTune, end_line: usize, terminator_line: Option<usize>) {
        if tune.start_line >= end_line {
            return;
        }

        let body_start_line = tune.body_start_line.unwrap_or(end_line);
        let header_line_end = body_start_line.min(end_line);
        let body_line_start = body_start_line.min(end_line);
        let span = self.line_range_span(tune.start_line, end_line);
        let header_span = self.line_range_span(tune.start_line, header_line_end);
        let body_span = self.line_range_span(body_line_start, end_line);

        self.blocks.push(SourceBlock {
            kind: SourceBlockKind::Tune,
            span,
            line_start: tune.start_line,
            line_end: end_line,
            tune_index: Some(tune.index),
        });
        self.tunes.push(TuneBlock {
            index: tune.index,
            span,
            header_span,
            body_span,
            line_start: tune.start_line,
            line_end: end_line,
            header_line_start: tune.start_line,
            header_line_end,
            body_line_start,
            body_line_end: end_line,
            terminator_line,
        });
    }

    fn track_typeset_block(&mut self, line: &ClassifiedLine) {
        if line.kind == LineKind::TypesetTextDirective {
            if self.typeset_text_block_start.is_none() {
                self.typeset_text_block_start = Some(line.index);
            }
        } else if let Some(start_line) = self.typeset_text_block_start.take() {
            self.close_typeset_text_block(start_line, line.index);
        }
    }

    fn close_typeset_text_block(&mut self, start_line: usize, end_line: usize) {
        if start_line >= end_line {
            return;
        }

        self.blocks.push(SourceBlock {
            kind: SourceBlockKind::TypesetText,
            span: self.line_range_span(start_line, end_line),
            line_start: start_line,
            line_end: end_line,
            tune_index: tune_index_for_context(self.lines[start_line].context),
        });
    }

    fn track_non_note_items(&mut self, line: &ClassifiedLine) {
        if let Some(kind) = non_note_kind_for_line(line.kind) {
            self.non_note_items.push(NonNoteItem {
                kind,
                line_index: line.index,
                span: non_note_span(line),
            });
        }

        if let Some(span) = line.trailing_comment {
            self.non_note_items.push(NonNoteItem {
                kind: NonNoteKind::InlineComment,
                line_index: line.index,
                span,
            });
        }
    }

    fn track_field_continuation(&mut self, line: &ClassifiedLine) {
        if line.kind == LineKind::FieldContinuation {
            if let (Some(from_line), Some(marker_span)) =
                (self.previous_field_line, line.marker_span)
            {
                let from_end = self
                    .lines
                    .get(from_line)
                    .map(|line| line.text_span.end)
                    .unwrap_or(marker_span.start);
                self.continuation_edges.push(ContinuationEdge {
                    kind: ContinuationKind::FieldContinuation,
                    from_line,
                    to_line: line.index,
                    span: Span::new(from_end, marker_span.end),
                    marker_span,
                });
            }
            self.previous_field_line = Some(line.index);
            return;
        }

        if matches!(line.kind, LineKind::InformationField) {
            self.previous_field_line = Some(line.index);
        } else if !matches!(
            line.kind,
            LineKind::Comment | LineKind::StylesheetDirective | LineKind::TypesetTextDirective
        ) {
            self.previous_field_line = None;
        }
    }

    fn track_music_continuation(&mut self, line: &ClassifiedLine) {
        if line.kind == LineKind::MusicCode {
            if let Some(pending) = self.pending_music_continuation.take() {
                self.continuation_edges.push(ContinuationEdge {
                    kind: ContinuationKind::MusicBackslash,
                    from_line: pending.from_line,
                    to_line: line.index,
                    span: Span::new(pending.marker_span.start, line.content_span.start),
                    marker_span: pending.marker_span,
                });
            }

            self.pending_music_continuation = match line.score_line_break {
                ScoreLineBreak::Suppressed { marker_span } => Some(PendingMusicContinuation {
                    from_line: line.index,
                    marker_span,
                }),
                ScoreLineBreak::NotApplicable | ScoreLineBreak::Physical => None,
            };
        }
    }

    fn update_begintext_state(&mut self, line: &ClassifiedLine, text: &str) {
        if line.kind != LineKind::TypesetTextDirective {
            return;
        }

        let Some(name) = directive_name(text) else {
            return;
        };

        if name.eq_ignore_ascii_case("begintext") {
            self.in_begintext_block = true;
        } else if name.eq_ignore_ascii_case("endtext") {
            self.in_begintext_block = false;
        }
    }

    fn finish(mut self) -> LineMap {
        match self.state.clone() {
            DocumentState::FileHeader { start_line } => {
                self.close_file_header(start_line, self.source.line_count());
            }
            DocumentState::FreeText { start_line } => {
                self.close_free_text_block(start_line, self.source.line_count());
            }
            DocumentState::TuneHeader { tune } | DocumentState::TuneBody { tune } => {
                self.close_tune(tune, self.source.line_count(), None);
            }
            DocumentState::Start | DocumentState::BetweenBlocks => {}
        }

        if let Some(start_line) = self.typeset_text_block_start.take() {
            self.close_typeset_text_block(start_line, self.source.line_count());
        }

        LineMap {
            lines: self.lines,
            blocks: self.blocks,
            tunes: self.tunes,
            continuation_edges: self.continuation_edges,
            non_note_items: self.non_note_items,
        }
    }

    fn line_range_span(&self, start_line: usize, end_line: usize) -> Span {
        if start_line >= end_line {
            let offset = self
                .source
                .line(start_line)
                .map(|line| line.start())
                .unwrap_or_else(|| self.source.len());
            return Span::new(offset, offset);
        }

        let start = self
            .source
            .line(start_line)
            .map(|line| line.start())
            .unwrap_or_else(|| self.source.len());
        let end = self
            .source
            .line(end_line - 1)
            .map(|line| line.end())
            .unwrap_or(start);
        Span::new(start, end)
    }

    fn is_synthetic_final_empty_line(&self, line: &ClassifiedLine) -> bool {
        line.index + 1 == self.source.line_count()
            && line.text_span.is_empty()
            && line.span.is_empty()
            && line.text_span.start == self.source.len()
            && self.source.len() > self.source.content_start()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DocumentState {
    Start,
    FileHeader { start_line: usize },
    BetweenBlocks,
    FreeText { start_line: usize },
    TuneHeader { tune: OpenTune },
    TuneBody { tune: OpenTune },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenTune {
    index: usize,
    start_line: usize,
    body_start_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingMusicContinuation {
    from_line: usize,
    marker_span: Span,
}

impl ClassifiedLine {
    pub fn field_code(&self) -> Option<char> {
        self.field.map(|field| field.code)
    }
}

fn classify_raw_line(
    index: usize,
    span: Span,
    text_span: Span,
    text: &str,
    in_begintext_block: bool,
) -> ClassifiedLine {
    let content_start = trimmed_start_offset(text);
    let content = &text[content_start..];
    let content_span = Span::new(text_span.start + content_start, text_span.end);
    let trailing_comment = trailing_comment_span(text, text_span.start);

    let mut line = ClassifiedLine {
        index,
        kind: LineKind::FreeText,
        context: LineContext::BetweenBlocks,
        span,
        text_span,
        content_span,
        marker_span: None,
        field: None,
        trailing_comment,
        score_line_break: ScoreLineBreak::NotApplicable,
    };

    if content.trim_end().is_empty() {
        line.kind = LineKind::EmptyLine;
        line.content_span = Span::new(text_span.end, text_span.end);
        line.trailing_comment = None;
        return line;
    }

    if in_begintext_block {
        line.kind = LineKind::TypesetTextDirective;
        line.marker_span = content
            .strip_prefix("%%")
            .map(|_| Span::new(line.content_span.start, line.content_span.start + 2));
        return line;
    }

    if content.starts_with("%abc-") {
        line.kind = LineKind::VersionLine;
        line.marker_span = Some(Span::new(
            line.content_span.start,
            line.content_span.start + 5,
        ));
        line.trailing_comment = None;
        return line;
    }

    if content.starts_with("%%") {
        line.marker_span = Some(Span::new(
            line.content_span.start,
            line.content_span.start + 2,
        ));
        line.kind = if is_typeset_text_directive(content) {
            LineKind::TypesetTextDirective
        } else {
            LineKind::StylesheetDirective
        };
        line.trailing_comment = None;
        return line;
    }

    if content.starts_with('%') {
        line.kind = LineKind::Comment;
        line.marker_span = Some(Span::new(
            line.content_span.start,
            line.content_span.start + 1,
        ));
        line.trailing_comment = None;
        return line;
    }

    if content.starts_with("+:") {
        line.kind = LineKind::FieldContinuation;
        line.marker_span = Some(Span::new(
            line.content_span.start,
            line.content_span.start + 2,
        ));
        return line;
    }

    if let Some(field) = parse_field_header(content, line.content_span.start, text_span.end) {
        line.kind = LineKind::InformationField;
        line.marker_span = Some(field.marker_span);
        line.field = Some(field);
        return line;
    }

    line.score_line_break = music_score_line_break(text, text_span.start);
    line
}

fn parse_field_header(
    content: &str,
    content_offset: usize,
    text_end: usize,
) -> Option<FieldHeader> {
    let mut chars = content.chars();
    let code = chars.next()?;
    if !code.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }

    let marker_end = content_offset + code.len_utf8() + 1;
    Some(FieldHeader {
        code,
        marker_span: Span::new(content_offset, marker_end),
        value_span: Span::new(marker_end, text_end),
    })
}

fn trimmed_start_offset(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

fn trailing_comment_span(text: &str, line_offset: usize) -> Option<Span> {
    let comment_start = text.char_indices().find_map(|(offset, ch)| {
        if ch == '%' && !is_escaped(text, offset) {
            Some(offset)
        } else {
            None
        }
    })?;

    Some(Span::new(
        line_offset + comment_start,
        line_offset + text.len(),
    ))
}

fn music_score_line_break(text: &str, line_offset: usize) -> ScoreLineBreak {
    let code_end = trailing_comment_span(text, 0)
        .map(|span| span.start)
        .unwrap_or(text.len());
    let code = text[..code_end].trim_end();
    let Some(last) = code.strip_suffix('\\') else {
        return ScoreLineBreak::Physical;
    };
    let marker_start = line_offset + last.len();
    ScoreLineBreak::Suppressed {
        marker_span: Span::new(marker_start, marker_start + 1),
    }
}

fn is_escaped(text: &str, offset: usize) -> bool {
    let mut slash_count = 0;
    for byte in text[..offset].bytes().rev() {
        if byte == b'\\' {
            slash_count += 1;
        } else {
            break;
        }
    }
    slash_count % 2 == 1
}

fn directive_name(text: &str) -> Option<&str> {
    let content = text.trim_start();
    let rest = content.strip_prefix("%%")?;
    let name_end = rest
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(rest.len());
    Some(&rest[..name_end])
}

fn is_typeset_text_directive(content: &str) -> bool {
    let Some(name) = directive_name(content) else {
        return false;
    };

    matches!(
        name.to_ascii_lowercase().as_str(),
        "text" | "center" | "begintext" | "endtext"
    )
}

fn non_note_kind_for_line(kind: LineKind) -> Option<NonNoteKind> {
    match kind {
        LineKind::VersionLine => Some(NonNoteKind::VersionLine),
        LineKind::EmptyLine => Some(NonNoteKind::EmptyLine),
        LineKind::Comment => Some(NonNoteKind::Comment),
        LineKind::StylesheetDirective => Some(NonNoteKind::StylesheetDirective),
        LineKind::InformationField => Some(NonNoteKind::InformationField),
        LineKind::FieldContinuation => Some(NonNoteKind::FieldContinuation),
        LineKind::FreeText => Some(NonNoteKind::FreeText),
        LineKind::TypesetTextDirective => Some(NonNoteKind::TypesetTextDirective),
        LineKind::MusicCode => None,
    }
}

fn non_note_span(line: &ClassifiedLine) -> Span {
    if line.kind == LineKind::EmptyLine {
        line.text_span
    } else {
        line.content_span
    }
}

fn tune_index_for_context(context: LineContext) -> Option<usize> {
    match context {
        LineContext::TuneHeader { tune_index }
        | LineContext::TuneBody { tune_index }
        | LineContext::TuneTerminator { tune_index } => Some(tune_index),
        LineContext::Preamble
        | LineContext::FileHeader
        | LineContext::BetweenBlocks
        | LineContext::FreeText
        | LineContext::TypesetText => None,
    }
}

fn tokenize_surface(source: &SourceText, line_map: &LineMap) -> Vec<SurfaceToken> {
    let mut tokens = Vec::new();

    for line in &line_map.lines {
        match line.kind {
            LineKind::InformationField => {
                if let Some(field) = line.field {
                    tokens.push(SurfaceToken {
                        kind: SurfaceKind::Field,
                        span: field.marker_span,
                    });
                }
            }
            LineKind::FieldContinuation => {
                if let Some(marker_span) = line.marker_span {
                    tokens.push(SurfaceToken {
                        kind: SurfaceKind::FieldContinuation,
                        span: marker_span,
                    });
                }
            }
            LineKind::Comment => tokens.push(SurfaceToken {
                kind: SurfaceKind::Comment,
                span: line.content_span,
            }),
            LineKind::StylesheetDirective | LineKind::TypesetTextDirective => {
                tokens.push(SurfaceToken {
                    kind: SurfaceKind::Directive,
                    span: line.content_span,
                });
            }
            LineKind::FreeText => tokens.push(SurfaceToken {
                kind: SurfaceKind::FreeText,
                span: line.content_span,
            }),
            LineKind::MusicCode => {
                if let Some(text) = source.slice(line.text_span) {
                    tokenize_music_line(text, line.text_span.start, &mut tokens);
                }
            }
            LineKind::VersionLine | LineKind::EmptyLine => {}
        }

        if line.kind != LineKind::MusicCode
            && let Some(span) = line.trailing_comment
        {
            tokens.push(SurfaceToken {
                kind: SurfaceKind::Comment,
                span,
            });
        }
    }

    tokens
}

fn tokenize_music_line(line: &str, line_offset: usize, tokens: &mut Vec<SurfaceToken>) {
    for (column, ch) in line.char_indices() {
        let kind = match ch {
            '%' if !is_escaped(line, column) => {
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
}
