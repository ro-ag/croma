//! Lyric and symbol line surface AST: the token streams produced from `w:`
//! lyric lines and `s:` symbol lines.

use crate::diagnostic::Span;
use crate::fields::Spanned;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLineSyntax {
    pub line_index: usize,
    pub span: Span,
    pub value: Spanned<String>,
    pub tokens: Vec<LyricTokenSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricTokenSyntax {
    pub span: Span,
    pub text: String,
    pub kind: LyricTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricTokenKind {
    Syllable,
    Hyphen,
    Extender,
    Skip,
    Bar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLineSyntax {
    pub line_index: usize,
    pub span: Span,
    pub value: Spanned<String>,
    pub tokens: Vec<SymbolTokenSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTokenSyntax {
    pub span: Span,
    pub text: String,
    pub kind: SymbolTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolTokenKind {
    Decoration,
    ChordSymbol,
    Annotation,
    Raw,
    Skip,
    Bar,
}
