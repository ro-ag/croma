//! Command-line surface for `croma`, defined with clap's derive API.
//!
//! This module only describes argument parsing. The actual pipeline logic lives
//! in `main.rs`; the structs here lower into the existing `CliOptions` plumbing.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use croma_core::{AbcSpecVersion, ParseMode, ParseOptions};

/// `croma` — an ABC notation toolkit.
#[derive(Debug, Parser)]
#[command(
    name = "croma",
    about = "ABC notation toolkit",
    disable_help_subcommand = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Export an ABC file to MusicXML.
    Xml(XmlArgs),
    /// Parse and validate an ABC file, reporting diagnostics only.
    Check(CheckArgs),
    /// Dump intermediate representations for debugging.
    Dump(DumpArgs),
    /// Format an ABC file (canonical formatting, in the style of rustfmt/gofmt).
    Fmt(FmtArgs),
    /// Read a MusicXML file back into a Score and project it (experimental).
    ///
    /// Inverts croma's own MusicXML writer. Gated behind the `musicxml-reader`
    /// feature; absent from the default build.
    #[cfg(feature = "musicxml-reader")]
    Read(ReadArgs),
    /// Convert a MusicXML file to ABC (read MusicXML -> Score -> write ABC).
    ///
    /// A discoverable alias for `read --format abc`. Gated behind the
    /// `musicxml-reader` feature; absent from the default build.
    #[cfg(feature = "musicxml-reader")]
    Musicxml2abc(Musicxml2abcArgs),
}

/// Options shared by every subcommand that feed the parser and diagnostics.
#[derive(Debug, Args, Clone)]
pub struct CommonArgs {
    /// Parse in strict mode (default).
    #[arg(long, group = "parse_mode_group", global = true)]
    pub strict: bool,
    /// Parse in loose mode.
    #[arg(long, group = "parse_mode_group", global = true)]
    pub loose: bool,
    /// Parse in recover mode.
    #[arg(long, group = "parse_mode_group", global = true)]
    pub recover: bool,
    /// Interpret the source against the ABC 2.2 draft spec.
    #[arg(long = "abc-2.2-draft", global = true)]
    pub abc_2_2_draft: bool,
    /// Diagnostics output format.
    #[arg(long, value_enum, default_value_t = DiagnosticsFormat::Text, global = true)]
    pub diagnostics: DiagnosticsFormat,
    /// Treat warnings as errors (non-zero exit when any warning is present).
    #[arg(long, global = true)]
    pub warnings_as_errors: bool,
}

impl CommonArgs {
    fn spec(&self) -> AbcSpecVersion {
        if self.abc_2_2_draft {
            AbcSpecVersion::V22Draft
        } else {
            AbcSpecVersion::V21
        }
    }

    fn parse_mode(&self) -> ParseMode {
        if self.loose {
            ParseMode::Loose
        } else if self.recover {
            ParseMode::Recover
        } else {
            ParseMode::Strict
        }
    }

    /// Lower the common flags into core `ParseOptions`.
    pub fn parse_options(&self) -> ParseOptions {
        ParseOptions::new(self.spec(), self.parse_mode())
    }

    /// The chosen diagnostics format.
    pub fn diagnostics_format(&self) -> DiagnosticsFormat {
        self.diagnostics
    }

    /// Whether warnings should be promoted to errors.
    pub fn warnings_as_errors(&self) -> bool {
        self.warnings_as_errors
    }
}

#[derive(Debug, Args)]
pub struct XmlArgs {
    /// The ABC file to export.
    pub file: PathBuf,
    /// Write MusicXML to this path instead of stdout.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// The ABC file to validate.
    pub file: PathBuf,
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Args)]
pub struct DumpArgs {
    /// What to dump.
    #[arg(value_enum)]
    pub kind: DumpKind,
    /// The ABC file to dump.
    pub file: PathBuf,
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Args)]
pub struct FmtArgs {
    /// The ABC file to format.
    pub file: PathBuf,
    /// Check whether the file is formatted; exit non-zero if it would change.
    #[arg(long)]
    pub check: bool,
    /// Write the formatted result back to the file in place.
    #[arg(short = 'w', long)]
    pub write: bool,
    /// Apply safe, pitch-preserving auto-fixes in addition to formatting.
    #[arg(long)]
    pub auto_fix: bool,
    #[command(flatten)]
    pub common: CommonArgs,
}

/// Arguments for `croma read`: read a MusicXML file and project the
/// reconstructed `Score` per `--format`.
#[cfg(feature = "musicxml-reader")]
#[derive(Debug, Args)]
pub struct ReadArgs {
    /// The MusicXML file to read.
    pub file: PathBuf,
    /// Write the projection to this path instead of stdout.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    /// How to project the reconstructed Score (default: MusicXML re-emission).
    #[arg(long, value_enum, default_value_t = ReadFormat::Xml)]
    pub format: ReadFormat,
}

/// Arguments for `croma musicxml2abc`: read a MusicXML file and write ABC.
#[cfg(feature = "musicxml-reader")]
#[derive(Debug, Args)]
pub struct Musicxml2abcArgs {
    /// The MusicXML file to convert.
    pub file: PathBuf,
    /// Write the ABC to this path instead of stdout.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
}

/// The projection of a Score reconstructed from MusicXML, selected by
/// `croma read --format`.
#[cfg(feature = "musicxml-reader")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReadFormat {
    /// Re-emit MusicXML from the reconstructed Score (the writer's inverse-image).
    Xml,
    /// Write ABC from the reconstructed Score.
    Abc,
    /// Pretty-print the reconstructed `Score` debug representation.
    Dump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DiagnosticsFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DumpKind {
    Tokens,
    Tree,
    Score,
    Abc,
}
