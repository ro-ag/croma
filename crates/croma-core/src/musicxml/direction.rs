use crate::model::{
    AlignedSymbolKind, AnnotationPlacementModel, EventAttachments, Part, TempoBeat, TempoModel,
    TextAttachment,
};

use super::notation::{DirectionSymbol, decoration_notation, symbol_direction};
use super::{MeasureSequence, MusicXmlWriter, unsupported_decoration_warning};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_initial_directions(&mut self, is_first_part: bool) {
        // Score-level directions (tempo and preserved `%%` directives) belong to
        // the score once, not to every part. With one part per voice, emitting
        // them in each part duplicated them N times. `W:` post-tune verses are
        // emitted separately as score-header credits (see `write_credits`).
        if !is_first_part {
            return;
        }
        if let Some(tempo_model) = &self.score.metadata.tempo_model {
            self.write_tempo_direction(tempo_model);
        } else if let Some(tempo) = &self.score.metadata.tempo {
            self.write_direction_words(&tempo.text, None, Some("1"), Some(1));
        }
        // Preserved `%%`/`%%MIDI` stylesheet directives are kept on the model
        // for round-trip/formatter use, but they control playback/formatting,
        // not printed musical text. abc2xml emits nothing for them, so the
        // MusicXML writer must not render them as visible <words> directions.
    }

    /// Emit a `Q:` tempo as a MusicXML `<metronome>` direction (matching the
    /// abc2xml reference), falling back to plain `<words>` when the field has no
    /// numeric tempo. A `<sound tempo=...>` is always emitted: quarter-notes per
    /// minute for a numeric tempo, or a default of 120 for text-only tempos.
    pub(crate) fn write_tempo_direction(&mut self, tempo: &TempoModel) {
        let beat_unit = tempo.beat.and_then(beat_unit_model);
        // A numeric tempo we cannot map to a beat unit falls back to words using
        // the raw field text, preserving prior behavior for exotic forms.
        if tempo.beat.is_some() && beat_unit.is_none() {
            if let Some(raw) = &self.score.metadata.tempo {
                self.write_direction_words(&raw.text, None, Some("1"), Some(1));
            }
            return;
        }

        self.xml.start("direction", &[("placement", "above")]);
        if let Some(text) = &tempo.text {
            self.xml.start("direction-type", &[]);
            self.xml.text_element("words", text);
            self.xml.end("direction-type");
        }
        if let Some(unit) = &beat_unit {
            self.xml.start("direction-type", &[]);
            self.xml.start("metronome", &[]);
            self.xml.text_element("beat-unit", unit.name);
            if unit.dotted {
                self.xml.empty("beat-unit-dot", &[]);
            }
            self.xml.text_element(
                "per-minute",
                &tempo.beat.expect("beat present").bpm.to_string(),
            );
            self.xml.end("metronome");
            self.xml.end("direction-type");
        }
        let sound_tempo = match tempo.beat {
            Some(beat) => sound_tempo_qpm(beat),
            None => 120.0,
        };
        self.xml
            .empty("sound", &[("tempo", &format!("{sound_tempo:.2}"))]);
        self.xml.end("direction");
    }

    pub(crate) fn write_harmony_and_directions(
        &mut self,
        attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        for symbol in &attachments.chord_symbols {
            self.write_chord_symbol(&symbol.text, sequence);
        }
        for symbol in attachments
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == AlignedSymbolKind::ChordSymbol)
        {
            self.write_chord_symbol(&symbol.text, sequence);
        }
        for annotation in &attachments.annotations {
            let text = annotation_text(annotation);
            self.write_direction_words(
                text,
                annotation.placement,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
        for symbol in attachments.symbols.iter().filter(|symbol| {
            matches!(
                symbol.kind,
                AlignedSymbolKind::Annotation
                    | AlignedSymbolKind::Raw
                    | AlignedSymbolKind::Decoration
            )
        }) {
            self.write_direction_words(
                &symbol.text,
                None,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
        for decoration in &attachments.decorations {
            if let Some(dynamic) = dynamic_decoration(decoration.name.as_str()) {
                self.write_dynamic(dynamic, sequence, part);
            } else if let Some(direction) = symbol_direction(decoration.name.as_str()) {
                self.write_direction_type(direction, sequence, part);
            } else if let Some(wedge) = wedge_decoration(decoration.name.as_str()) {
                self.write_wedge(wedge, sequence, part);
            } else if is_suppressed_decoration(decoration.name.as_str()) {
                // No clean MusicXML equivalent (e.g. the Irish roll `~`).
                // abc2xml emits nothing; suppress without a words direction or
                // an unsupported-decoration diagnostic.
            } else if decoration_notation(decoration).is_none() {
                self.diagnostics
                    .push(unsupported_decoration_warning(decoration));
                self.write_direction_words(
                    &decoration.name,
                    None,
                    Some(sequence.voice_number.as_str()),
                    Some(sequence.staff.value),
                );
            }
        }
    }

    fn write_dynamic(
        &mut self,
        dynamic: &'static str,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        self.xml.start("direction", &[("placement", "below")]);
        self.xml.start("direction-type", &[]);
        self.xml.start("dynamics", &[]);
        self.xml.empty(dynamic, &[]);
        self.xml.end("dynamics");
        self.xml.end("direction-type");
        self.xml.text_element("voice", &sequence.voice_number);
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.xml.end("direction");
    }

    fn write_direction_type(
        &mut self,
        direction: DirectionSymbol,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        self.xml.start("direction", &[("placement", "above")]);
        self.xml.start("direction-type", &[]);
        match direction {
            DirectionSymbol::Coda => self.xml.empty("coda", &[]),
            DirectionSymbol::Segno => self.xml.empty("segno", &[]),
        }
        self.xml.end("direction-type");
        self.xml.text_element("voice", &sequence.voice_number);
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.xml.end("direction");
    }

    fn write_wedge(
        &mut self,
        wedge: &'static str,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        self.xml.start("direction", &[("placement", "below")]);
        self.xml.start("direction-type", &[]);
        self.xml.empty("wedge", &[("type", wedge)]);
        self.xml.end("direction-type");
        self.xml.text_element("voice", &sequence.voice_number);
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.xml.end("direction");
    }

    pub(crate) fn write_direction_words(
        &mut self,
        text: &str,
        placement: Option<AnnotationPlacementModel>,
        voice: Option<&str>,
        staff: Option<u32>,
    ) {
        let placement_attr = placement.map(placement_name);
        let attrs = placement_attr.map(|placement| [("placement", placement)]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("direction", attrs_slice);
        self.xml.start("direction-type", &[]);
        self.xml.text_element("words", text);
        self.xml.end("direction-type");
        if let Some(voice) = voice {
            self.xml.text_element("voice", voice);
        }
        if let Some(staff) = staff
            && staff > 1
        {
            self.xml.text_element("staff", &staff.to_string());
        }
        self.xml.end("direction");
    }
}

/// A MusicXML beat unit: a note-type name plus whether it carries a single dot.
struct BeatUnit {
    name: &'static str,
    dotted: bool,
}

/// Map a tempo beat fraction to a MusicXML `<beat-unit>` (with optional dot).
///
/// Plain powers of two map directly (1/4 -> quarter). A numerator of 3 over a
/// power of two is a dotted unit one power larger (3/8 -> dotted quarter). Forms
/// that do not map cleanly return `None` so the writer falls back to words.
fn beat_unit_model(beat: TempoBeat) -> Option<BeatUnit> {
    let name_for = |denominator: u32| -> Option<&'static str> {
        match denominator {
            1 => Some("whole"),
            2 => Some("half"),
            4 => Some("quarter"),
            8 => Some("eighth"),
            16 => Some("16th"),
            32 => Some("32nd"),
            64 => Some("64th"),
            _ => None,
        }
    };
    match beat.beat_numerator {
        1 => name_for(beat.beat_denominator).map(|name| BeatUnit {
            name,
            dotted: false,
        }),
        // 3/(2^k) is a dotted note one power larger, e.g. 3/8 -> dotted quarter.
        3 if beat.beat_denominator.is_multiple_of(2) => {
            name_for(beat.beat_denominator / 2).map(|name| BeatUnit { name, dotted: true })
        }
        _ => None,
    }
}

/// Quarter-notes-per-minute for a `<sound tempo>` value: `beat * 4 * bpm`.
fn sound_tempo_qpm(beat: TempoBeat) -> f64 {
    (f64::from(beat.beat_numerator) / f64::from(beat.beat_denominator)) * 4.0 * f64::from(beat.bpm)
}

fn annotation_text(annotation: &TextAttachment) -> &str {
    match annotation.placement {
        Some(_) => annotation
            .text
            .strip_prefix(['^', '_', '<', '>', '@'])
            .unwrap_or(&annotation.text),
        None => &annotation.text,
    }
}

fn placement_name(placement: AnnotationPlacementModel) -> &'static str {
    match placement {
        AnnotationPlacementModel::Above => "above",
        AnnotationPlacementModel::Below => "below",
        AnnotationPlacementModel::Left
        | AnnotationPlacementModel::Right
        | AnnotationPlacementModel::Free => "above",
    }
}

/// Hairpin decorations (ABC 2.1 lines 1114-1121): `!crescendo(!`/`!<(!` open a
/// crescendo wedge, `!diminuendo(!`/`!>(!` a diminuendo, and the `)` forms
/// close the open wedge (MusicXML wedge type `stop`).
fn wedge_decoration(name: &str) -> Option<&'static str> {
    match name {
        "crescendo(" | "<(" => Some("crescendo"),
        "diminuendo(" | ">(" => Some("diminuendo"),
        "crescendo)" | "<)" | "diminuendo)" | ">)" => Some("stop"),
        _ => None,
    }
}

fn dynamic_decoration(name: &str) -> Option<&'static str> {
    match name {
        "p" => Some("p"),
        "pp" => Some("pp"),
        "ppp" => Some("ppp"),
        "f" => Some("f"),
        "ff" => Some("ff"),
        "fff" => Some("fff"),
        "mp" => Some("mp"),
        "mf" => Some("mf"),
        "sfz" => Some("sfz"),
        _ => None,
    }
}

/// Decorations that have no clean MusicXML equivalent and are intentionally not
/// emitted (matching abc2xml, which emits nothing). They must not fall through
/// to a `<words>` direction.
fn is_suppressed_decoration(name: &str) -> bool {
    // `~` (Irish roll / general gracing) normalizes to the canonical `roll`.
    matches!(name, "roll")
}
