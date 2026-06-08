use crate::Span;
use crate::source::SourceText;
use crate::syntax::tune_classify::{classify_lines, tokenize_surface};

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

#[cfg(test)]
#[path = "tune_tests.rs"]
mod tests;
