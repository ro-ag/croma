//! Field and directive surface AST: inline/body music fields, score and
//! preserved stylesheet directives, and the unsupported/malformed markers used
//! when a music token cannot be represented.

use crate::diagnostic::Span;
use crate::fields::{KeySignature, Meter, ScoreDirective, Spanned, UnitNoteLength, VoiceDefinition};
use crate::syntax::lyric::{LyricLineSyntax, SymbolLineSyntax};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFieldSyntax {
    pub span: Span,
    pub marker_span: Span,
    pub code: char,
    pub value: Spanned<String>,
    pub voice: Option<Spanned<VoiceDefinition>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicFieldLine {
    pub line_index: usize,
    pub code: char,
    pub line_span: Span,
    pub marker_span: Span,
    pub value: Spanned<String>,
    pub kind: MusicFieldLineKind,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicFieldLineKind {
    Meter(Spanned<Meter>),
    UnitNoteLength(Spanned<UnitNoteLength>),
    Key(Spanned<KeySignature>),
    Voice(Spanned<VoiceDefinition>),
    Lyric(LyricLineSyntax),
    Symbol(SymbolLineSyntax),
    PostTuneText(Spanned<String>),
    Score(ScoreDirective),
    Unknown(Spanned<String>),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirectiveSyntax {
    pub line_index: usize,
    pub span: Span,
    pub marker_span: Span,
    pub name_span: Span,
    pub value: Spanned<String>,
    pub directive: ScoreDirective,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedDirectiveSyntax {
    pub line_index: usize,
    pub span: Span,
    pub marker_span: Span,
    pub name: Spanned<String>,
    pub value: Spanned<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedSyntax {
    pub span: Span,
    pub kind: UnsupportedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedSyntaxKind {
    Chord,
    GraceGroup,
    Tuplet,
    Slur,
    Tie,
    Decoration,
    BrokenRhythm,
    RepeatEnding,
    QuotedText,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedSyntax {
    pub span: Span,
    pub kind: MalformedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedSyntaxKind {
    DanglingAccidental,
    StandaloneOctave,
    StandaloneLength,
    InvalidLength,
    UnclosedInlineField,
    UnclosedChord,
    UnclosedGraceGroup,
    UnclosedQuotedText,
    UnclosedSlur,
    UnclosedDecoration,
    InvalidBarline,
    InvalidTuplet,
    InvalidRepeatEnding,
    InvalidDecoration,
    UnknownToken,
}
