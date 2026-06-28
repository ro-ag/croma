use crate::model::{ClefChangeModel, Part, StaffId};

use super::{MusicXmlWriter, unsupported_transpose_warning};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_attributes(&mut self, part: &Part) {
        self.xml.start("attributes", &[]);
        self.xml
            .text_element("divisions", &self.score.divisions.max(1).to_string());
        if let Some(key) = initial_key_for_part(part).or(self.score.metadata.key.as_ref()) {
            self.write_key_element(key);
        }
        if let Some(meter) = initial_meter_for_part(part).or(self.score.metadata.meter.as_ref()) {
            self.write_time_element(meter);
        }
        if part.staves.len() > 1 {
            self.xml
                .text_element("staves", &part.staves.len().to_string());
        }
        self.write_clefs(part);
        self.write_transpose_if_available(part);
        self.xml.end("attributes");
    }

    /// The `<key>` element body shared by the header attributes and mid-tune
    /// key-change emission.
    pub(crate) fn write_key_element(&mut self, key: &crate::model::KeySignatureModel) {
        self.xml.start("key", &[]);
        self.xml.text_element("fifths", &key.fifths.to_string());
        for accidental in &key.explicit_accidentals {
            self.xml
                .text_element("key-step", &accidental.step.to_string());
            self.xml
                .text_element("key-alter", &accidental.accidental.alter().to_string());
            self.xml
                .text_element("key-accidental", accidental.accidental.musicxml_name());
        }
        self.xml.end("key");
    }

    /// The `<time>` element body shared by the header attributes and mid-tune
    /// meter-change emission. Free meters emit nothing.
    pub(crate) fn write_time_element(&mut self, meter: &crate::model::MeterModel) {
        if meter.free_meter {
            return;
        }
        let Some(parts) = meter_parts(&meter.display) else {
            return;
        };
        let symbol = meter
            .time_symbol
            .as_deref()
            .filter(|symbol| matches!(*symbol, "common" | "cut"))
            .or(parts.symbol);
        let attrs = symbol.map(|symbol| [("symbol", symbol)]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("time", attrs_slice);
        for part in parts.parts {
            self.xml.text_element("beats", &part.beats);
            self.xml.text_element("beat-type", &part.beat_type);
        }
        self.xml.end("time");
    }

    /// A mid-tune key change: a minimal `<attributes>` at the current cursor.
    pub(crate) fn write_mid_tune_key(&mut self, key: &crate::model::KeySignatureModel) {
        self.xml.start("attributes", &[]);
        self.write_key_element(key);
        self.xml.end("attributes");
    }

    /// A mid-tune meter change: a minimal `<attributes>` at the current cursor.
    /// Free meters (`M:none`) render no <time>, so no wrapper either.
    pub(crate) fn write_mid_tune_meter(&mut self, meter: &crate::model::MeterModel) {
        if meter.free_meter || meter_parts(&meter.display).is_none() {
            return;
        }
        self.xml.start("attributes", &[]);
        self.write_time_element(meter);
        self.xml.end("attributes");
    }

    pub(crate) fn write_mid_tune_clef(
        &mut self,
        clef: &ClefChangeModel,
        staff: StaffId,
        part: &Part,
    ) {
        self.xml.start("attributes", &[]);
        self.write_clef_element(Some(clef.clef.text.as_str()), staff, part.staves.len() > 1);
        self.xml.end("attributes");
    }

    pub(crate) fn write_multiple_rest_measure_style(&mut self, count: u32) {
        self.xml.start("attributes", &[]);
        self.xml.start("measure-style", &[]);
        self.xml.text_element("multiple-rest", &count.to_string());
        self.xml.end("measure-style");
        self.xml.end("attributes");
    }

    fn write_clefs(&mut self, part: &Part) {
        let staves = if part.staves.is_empty() {
            vec![StaffId {
                value: 1,
                span: part.source_span,
            }]
        } else {
            part.staves.iter().map(|staff| staff.id).collect()
        };
        for staff in staves {
            let clef_text = part
                .voices
                .iter()
                .find(|voice| voice.staff.value == staff.value)
                .and_then(|voice| voice.initial_properties.clef.as_ref())
                .map(|clef| clef.text.as_str());
            self.write_clef_element(clef_text, staff, part.staves.len() > 1);
        }
    }

    fn write_clef_element(&mut self, clef_text: Option<&str>, staff: StaffId, numbered: bool) {
        let clef = clef_model(clef_text);
        let number = staff.value.to_string();
        let attrs = numbered.then_some([("number", number.as_str())]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("clef", attrs_slice);
        self.xml.text_element("sign", clef.sign);
        self.xml.text_element("line", clef.line);
        if clef.octave_change != 0 {
            self.xml
                .text_element("clef-octave-change", &clef.octave_change.to_string());
        }
        self.xml.end("clef");
    }

    fn write_transpose_if_available(&mut self, part: &Part) {
        for voice in &part.voices {
            // The ABC `transpose=` voice property (ABC 2.1) takes precedence; a
            // `%%MIDI transpose` projection (abc2midi convention) is the fallback
            // when no native property is present.
            let chromatic = if let Some(transpose) = voice.properties.transpose.as_ref() {
                match transpose.text.trim().parse::<i32>() {
                    Ok(value) => value,
                    Err(_) => {
                        self.diagnostics
                            .push(unsupported_transpose_warning(transpose.span));
                        continue;
                    }
                }
            } else if let Some(transpose) = voice.midi_transpose {
                i32::from(transpose)
            } else {
                continue;
            };
            self.xml.start("transpose", &[]);
            self.xml.text_element("chromatic", &chromatic.to_string());
            self.xml.end("transpose");
            return;
        }
    }
}

pub(crate) fn initial_key_for_part(part: &Part) -> Option<&crate::model::KeySignatureModel> {
    part.voices
        .iter()
        .find_map(|voice| voice.initial_key.as_ref())
}

pub(crate) fn initial_meter_for_part(part: &Part) -> Option<&crate::model::MeterModel> {
    part.voices
        .iter()
        .find_map(|voice| voice.initial_meter.as_ref())
}

struct MeterParts {
    parts: Vec<MeterPart>,
    symbol: Option<&'static str>,
}

struct MeterPart {
    beats: String,
    beat_type: String,
}

fn meter_parts(display: &str) -> Option<MeterParts> {
    match display.trim() {
        "C" => Some(meter_parts_with_symbol("4", "4", Some("common"))),
        "C|" => Some(meter_parts_with_symbol("2", "2", Some("cut"))),
        "none" | "M:none" => None,
        value => {
            let parts = if value.contains('+') && !value.trim_start().starts_with('(') {
                value
                    .split('+')
                    .map(|part| {
                        let (beats, beat_type) = part.trim().split_once('/')?;
                        Some(MeterPart {
                            beats: beats.trim().to_owned(),
                            beat_type: beat_type.trim().to_owned(),
                        })
                    })
                    .collect::<Option<Vec<_>>>()?
            } else {
                let (beats, beat_type) = value.split_once('/')?;
                vec![MeterPart {
                    beats: strip_grouping_parentheses(beats).to_owned(),
                    beat_type: beat_type.trim().to_owned(),
                }]
            };
            (!parts.is_empty()).then_some(MeterParts {
                parts,
                symbol: None,
            })
        }
    }
}

fn meter_parts_with_symbol(
    beats: &str,
    beat_type: &str,
    symbol: Option<&'static str>,
) -> MeterParts {
    MeterParts {
        parts: vec![MeterPart {
            beats: beats.to_owned(),
            beat_type: beat_type.to_owned(),
        }],
        symbol,
    }
}

fn strip_grouping_parentheses(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(trimmed)
        .trim()
}

struct ClefModel {
    sign: &'static str,
    line: &'static str,
    octave_change: i8,
}

fn clef_model(clef: Option<&str>) -> ClefModel {
    let clef = clef.unwrap_or("treble").to_ascii_lowercase();
    let octave_change = if clef.contains("-15") {
        -2
    } else if clef.contains("+15") {
        2
    } else if clef.contains("-8") {
        -1
    } else if clef.contains("+8") {
        1
    } else {
        0
    };
    let (sign, line) = if clef.contains("bass") {
        ("F", "4")
    } else if clef.contains("alto") {
        ("C", "3")
    } else if clef.contains("tenor") {
        ("C", "4")
    } else if clef.contains("perc") {
        ("percussion", "2")
    } else {
        ("G", "2")
    };
    ClefModel {
        sign,
        line,
        octave_change,
    }
}
