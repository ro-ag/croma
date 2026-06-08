//! Surface AST for ABC source.
//!
//! This module holds the surface syntax tree produced by line classification
//! and music-line parsing: the tune/line surface structure (`tune`) and the
//! music-line `*Syntax` types.

pub mod tune;

pub use tune::{
    ClassifiedLine, ContinuationEdge, ContinuationKind, FieldHeader, LineContext, LineKind,
    LineMap, NonNoteItem, NonNoteKind, ScoreLineBreak, SourceBlock, SourceBlockKind, SurfaceKind,
    SurfaceMap, SurfaceToken, TuneBlock, analyze, analyze_source,
};
