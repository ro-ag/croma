#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbcSpecVersion {
    #[default]
    V21,
    V22Draft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseMode {
    #[default]
    Strict,
    Loose,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiagnosticOptions {
    pub suppress_croma_carrier_warnings: bool,
}

impl DiagnosticOptions {
    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.suppress_croma_carrier_warnings = true;
        self
    }

    pub(crate) fn should_emit_croma_carrier_warning(self, name: &str) -> bool {
        !self.suppress_croma_carrier_warnings
            || !(is_croma_carrier_name(name) || is_croma_managed_midi_name(name))
    }
}

pub fn is_croma_carrier_name(name: &str) -> bool {
    name.trim_start()
        .get(.."croma-".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("croma-"))
}

fn is_croma_managed_midi_name(name: &str) -> bool {
    name.trim_start().eq_ignore_ascii_case("MIDI")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseOptions {
    pub spec: AbcSpecVersion,
    pub mode: ParseMode,
    pub diagnostics: DiagnosticOptions,
}

impl ParseOptions {
    pub fn new(spec: AbcSpecVersion, mode: ParseMode) -> Self {
        Self {
            spec,
            mode,
            diagnostics: DiagnosticOptions::default(),
        }
    }

    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.diagnostics = self.diagnostics.suppress_croma_carrier_warnings();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub spec: AbcSpecVersion,
    pub parse_mode: ParseMode,
    pub diagnostics: DiagnosticOptions,
    /// Render-oriented MusicXML hints (beams, stems, tuplet display, ...). Default
    /// leaves the writer's byte-for-byte round-trip output untouched.
    pub write: MusicXmlWriteOptions,
}

impl ExportOptions {
    pub fn parse_options(self) -> ParseOptions {
        ParseOptions {
            spec: self.spec,
            mode: self.parse_mode,
            diagnostics: self.diagnostics,
        }
    }

    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.diagnostics = self.diagnostics.suppress_croma_carrier_warnings();
        self
    }

    /// Turn on the full engraving profile (all render hints).
    pub fn engrave(mut self) -> Self {
        self.write = MusicXmlWriteOptions::engrave();
        self
    }
}

/// Opt-in engraving hints the MusicXML writer computes at write time from data the
/// score already holds (meter, durations, clef, voice). Every field defaults off, so
/// [`MusicXmlWriteOptions::default`] reproduces the writer's byte-for-byte round-trip
/// output; the reader self-loop and the abc2xml whitelist depend on that default.
///
/// Ported from MuseScore's engraving rules. See `docs/engraving-export.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MusicXmlWriteOptions {
    /// Emit `<beam>` grouping computed from meter + note durations.
    pub beams: bool,
    /// How to spell `<tuplet>` display detail when the source carried no directive.
    pub tuplet_display: TupletDisplay,
    /// Emit `<stem>up|down` computed from clef, pitch, voice, and beam membership.
    pub stems: bool,
    /// Emit multi-voice rest `<display-step>`/`<display-octave>` so voices do not collide.
    pub rest_placement: bool,
    /// Emit `<slur placement="above|below">` computed from stem direction / voice.
    pub slur_placement: bool,
    /// Emit `<tied orientation="over|under">` computed from stem direction / voice.
    pub tie_orientation: bool,
}

impl MusicXmlWriteOptions {
    /// The `--engrave` umbrella: every hint on, tuplet display = engraving default.
    pub fn engrave() -> Self {
        Self {
            beams: true,
            tuplet_display: TupletDisplay::EngravingDefault,
            stems: true,
            rest_placement: true,
            slur_placement: true,
            tie_orientation: true,
        }
    }

    /// Whether any hint is enabled (lets the writer skip plan construction entirely
    /// on the default path).
    pub fn any_enabled(self) -> bool {
        self.beams
            || self.stems
            || self.rest_placement
            || self.slur_placement
            || self.tie_orientation
            || self.tuplet_display != TupletDisplay::AsEncoded
    }
}

/// How the writer spells a `<tuplet>` whose ABC source carried no explicit
/// tuplet-display directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TupletDisplay {
    /// Emit only what the source encoded (current behaviour — a bare `<tuplet>`).
    #[default]
    AsEncoded,
    /// Emit the convention default (MuseScore `calcHasBracket`): number-only, no
    /// bracket, for a fully-beamed tuplet; bracketed otherwise.
    EngravingDefault,
}

impl From<ExportOptions> for ParseOptions {
    fn from(options: ExportOptions) -> Self {
        options.parse_options()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LowerOptions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_options_expose_parser_options() {
        let options = ExportOptions {
            spec: AbcSpecVersion::V22Draft,
            parse_mode: ParseMode::Loose,
            diagnostics: DiagnosticOptions::default(),
            write: MusicXmlWriteOptions::default(),
        };

        assert_eq!(
            options.parse_options(),
            ParseOptions::new(AbcSpecVersion::V22Draft, ParseMode::Loose)
        );
    }

    #[test]
    fn croma_carrier_warning_suppression_matches_private_namespace_and_midi() {
        let diagnostics = DiagnosticOptions::default().suppress_croma_carrier_warnings();

        assert!(!diagnostics.should_emit_croma_carrier_warning("croma-future"));
        assert!(!diagnostics.should_emit_croma_carrier_warning("CROMA-future"));
        assert!(!diagnostics.should_emit_croma_carrier_warning("MIDI"));
        assert!(!diagnostics.should_emit_croma_carrier_warning("midi"));
        assert!(diagnostics.should_emit_croma_carrier_warning("tuplets"));
        assert!(diagnostics.should_emit_croma_carrier_warning("not-croma-future"));
    }
}
