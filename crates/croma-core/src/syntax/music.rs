//! Music-line surface AST: the `*Syntax` types produced by parsing an ABC
//! music line into notes, rests, chords, decorations, and barlines.

use crate::diagnostic::Span;
use crate::lower::default_tuplet_q;
use crate::model::{Accidental, BarlineKind, Fraction, RestVisibility};
use crate::syntax::field::{
    InlineFieldSyntax, MalformedSyntax, MusicFieldLine, PreservedDirectiveSyntax,
    ScoreDirectiveSyntax, UnsupportedSyntax,
};
use crate::syntax::lyric::{LyricLineSyntax, SymbolLineSyntax};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedMusicDocument {
    pub tunes: Vec<ParsedTuneMusic>,
}

impl ParsedMusicDocument {
    pub fn tune(&self, tune_index: usize) -> Option<&ParsedTuneMusic> {
        self.tunes.iter().find(|tune| tune.tune_index == tune_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTuneMusic {
    pub tune_index: usize,
    pub span: Span,
    pub lines: Vec<MusicLine>,
    pub body_fields: Vec<MusicFieldLine>,
    pub lyric_lines: Vec<LyricLineSyntax>,
    pub symbol_lines: Vec<SymbolLineSyntax>,
    pub score_directives: Vec<ScoreDirectiveSyntax>,
    pub preserved_directives: Vec<PreservedDirectiveSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicLine {
    pub line_index: usize,
    pub span: Span,
    pub code_span: Span,
    pub tokens: Vec<MusicToken>,
    pub items: Vec<MusicItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicToken {
    pub kind: MusicTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicTokenKind {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum MusicItem {
    Note(NoteSyntax),
    Rest(RestSyntax),
    MultiMeasureRest(MultiMeasureRestSyntax),
    Spacer(SpacerSyntax),
    Chord(ChordSyntax),
    GraceGroup(GraceGroupSyntax),
    ChordSymbol(QuotedTextSyntax),
    Annotation(QuotedTextSyntax),
    Decoration(DecorationSyntax),
    Tuplet(TupletSyntax),
    Slur(SlurSyntax),
    Tie(TieSyntax),
    BrokenRhythm(BrokenRhythmSyntax),
    Overlay(OverlaySyntax),
    VariantEnding(VariantEndingSyntax),
    Barline(BarlineSyntax),
    InlineField(InlineFieldSyntax),
    Unsupported(UnsupportedSyntax),
    Malformed(MalformedSyntax),
}

impl MusicItem {
    pub fn span(&self) -> Span {
        match self {
            Self::Note(item) => item.span,
            Self::Rest(item) => item.span,
            Self::MultiMeasureRest(item) => item.span,
            Self::Spacer(item) => item.span,
            Self::Chord(item) => item.span,
            Self::GraceGroup(item) => item.span,
            Self::ChordSymbol(item) | Self::Annotation(item) => item.span,
            Self::Decoration(item) => item.span,
            Self::Tuplet(item) => item.span,
            Self::Slur(item) => item.span,
            Self::Tie(item) => item.span,
            Self::BrokenRhythm(item) => item.span,
            Self::Overlay(item) => item.span,
            Self::VariantEnding(item) => item.span,
            Self::Barline(item) => item.span,
            Self::InlineField(item) => item.span,
            Self::Unsupported(item) => item.span,
            Self::Malformed(item) => item.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub accidental: Option<AccidentalSyntax>,
    pub pitch: PitchSyntax,
    pub octave_marks: Vec<OctaveMarkSyntax>,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentBundle {
    pub grace_groups: Vec<GraceGroupSyntax>,
    pub chord_symbols: Vec<QuotedTextSyntax>,
    pub annotations: Vec<QuotedTextSyntax>,
    pub decorations: Vec<DecorationSyntax>,
}

impl AttachmentBundle {
    pub fn is_empty(&self) -> bool {
        self.grace_groups.is_empty()
            && self.chord_symbols.is_empty()
            && self.annotations.is_empty()
            && self.decorations.is_empty()
    }

    pub(crate) fn span_start(&self) -> Option<usize> {
        self.grace_groups
            .iter()
            .map(|item| item.span.start)
            .chain(self.chord_symbols.iter().map(|item| item.span.start))
            .chain(self.annotations.iter().map(|item| item.span.start))
            .chain(self.decorations.iter().map(|item| item.span.start))
            .min()
    }

    pub(crate) fn push_quoted_text(&mut self, text: QuotedTextSyntax) {
        match text.kind {
            QuotedTextKind::ChordSymbol => self.chord_symbols.push(text),
            QuotedTextKind::Annotation(_) => self.annotations.push(text),
        }
    }

    /// Appends `other`'s attachments after this bundle's, per category. Used
    /// when a nested construct (grace group) restores the caller's pending
    /// bundle: the caller's items came first in the source, so they keep
    /// their position; `span_start()` stays correct because it is computed
    /// as the minimum over all member spans.
    pub(crate) fn extend(&mut self, other: AttachmentBundle) {
        self.grace_groups.extend(other.grace_groups);
        self.chord_symbols.extend(other.chord_symbols);
        self.annotations.extend(other.annotations);
        self.decorations.extend(other.decorations);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchSyntax {
    pub step: char,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccidentalSyntax {
    pub sign: Accidental,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OctaveMarkSyntax {
    pub mark: OctaveMark,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveMark {
    Lower,
    Raise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthSyntax {
    pub span: Span,
    pub raw: String,
    pub numerator: Option<SpannedNumber>,
    pub slash_count: u8,
    pub denominator: Option<SpannedNumber>,
    pub multiplier: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedNumber {
    pub value: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiMeasureRestSyntax {
    pub span: Span,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub count: Option<SpannedNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacerSyntax {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub open_span: Span,
    pub close_span: Option<Span>,
    pub members: Vec<ChordMemberSyntax>,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordMemberSyntax {
    pub span: Span,
    pub note: NoteSyntax,
    /// A tie marker (`-`) attached directly to this chord member, e.g. the `A`
    /// in `[DA-]`. ABC 2.1 §4.11 allows ties into, out of, and between chords.
    pub tie: Option<TieSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceGroupSyntax {
    pub span: Span,
    pub slash_span: Option<Span>,
    pub elements: Vec<GraceElementSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraceElementSyntax {
    Note(NoteSyntax),
    Chord(ChordSyntax),
    Rest(RestSyntax),
    Slur(SlurSyntax),
    Malformed(MalformedSyntax),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedTextSyntax {
    pub span: Span,
    pub content_span: Span,
    pub text: String,
    pub kind: QuotedTextKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotedTextKind {
    ChordSymbol,
    Annotation(AnnotationPlacement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationPlacement {
    Above,
    Below,
    Left,
    Right,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorationSyntax {
    pub span: Span,
    pub name_span: Span,
    pub name: String,
    pub kind: DecorationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Named,
    LegacyNamed,
    Shorthand,
    UserDefined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupletSyntax {
    pub span: Span,
    pub p: SpannedNumber,
    pub q: Option<SpannedNumber>,
    pub r: Option<SpannedNumber>,
}

impl TupletSyntax {
    pub(crate) fn q_value(&self) -> u32 {
        self.q
            .map(|q| q.value)
            .unwrap_or_else(|| default_tuplet_q(self.p.value))
    }

    pub(crate) fn r_value(&self) -> u32 {
        self.r.map(|r| r.value).unwrap_or(self.p.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlurSyntax {
    pub span: Span,
    pub dotted: bool,
    pub direction: SlurDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlurDirection {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TieSyntax {
    pub span: Span,
    pub dotted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenRhythmSyntax {
    pub span: Span,
    pub direction: BrokenRhythmDirection,
    pub count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlaySyntax {
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenRhythmDirection {
    LeftShorter,
    RightShorter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEndingSyntax {
    pub span: Span,
    pub shorthand: bool,
    pub endings: Vec<VariantEndingPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantEndingPart {
    Single(SpannedNumber),
    Range {
        start: SpannedNumber,
        end: SpannedNumber,
        span: Span,
    },
    Text {
        text: String,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarlineSyntax {
    pub span: Span,
    pub kind: BarlineKind,
    pub dotted: bool,
    pub raw: String,
}
