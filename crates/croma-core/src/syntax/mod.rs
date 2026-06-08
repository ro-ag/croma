//! Surface AST for ABC source.
//!
//! This module holds the surface syntax tree produced by line classification
//! and music-line parsing: the tune/line surface structure (`tune`) and the
//! music-line `*Syntax` types.

pub mod field;
pub mod lyric;
pub mod music;
pub mod tune;
mod tune_classify;

pub use field::{
    InlineFieldSyntax, MalformedSyntax, MalformedSyntaxKind, MusicFieldLine, MusicFieldLineKind,
    PreservedDirectiveSyntax, ScoreDirectiveSyntax, UnsupportedSyntax, UnsupportedSyntaxKind,
};
pub use lyric::{
    LyricLineSyntax, LyricTokenKind, LyricTokenSyntax, SymbolLineSyntax, SymbolTokenKind,
    SymbolTokenSyntax,
};
pub use music::{
    AccidentalSyntax, AnnotationPlacement, AttachmentBundle, BarlineSyntax, BrokenRhythmDirection,
    BrokenRhythmSyntax, ChordMemberSyntax, ChordSyntax, DecorationKind, DecorationSyntax,
    GraceElementSyntax, GraceGroupSyntax, LengthSyntax, MultiMeasureRestSyntax, MusicItem,
    MusicLine, MusicToken, MusicTokenKind, NoteSyntax, OctaveMark, OctaveMarkSyntax,
    OverlaySyntax, ParsedMusicDocument, ParsedTuneMusic, PitchSyntax, QuotedTextKind,
    QuotedTextSyntax, RestSyntax, SlurDirection, SlurSyntax, SpacerSyntax, SpannedNumber,
    TieSyntax, TupletSyntax, VariantEndingPart, VariantEndingSyntax,
};
pub use tune::{
    ClassifiedLine, ContinuationEdge, ContinuationKind, FieldHeader, LineContext, LineKind,
    LineMap, NonNoteItem, NonNoteKind, ScoreLineBreak, SourceBlock, SourceBlockKind, SurfaceKind,
    SurfaceMap, SurfaceToken, TuneBlock, analyze, analyze_source,
};
