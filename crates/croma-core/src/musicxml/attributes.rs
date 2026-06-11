use crate::model::{Part, StaffId};

use super::{MusicXmlWriter, unsupported_transpose_warning};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_attributes(&mut self, part: &Part) {
        self.xml.start("attributes", &[]);
        self.xml
            .text_element("divisions", &self.score.divisions.max(1).to_string());
        if let Some(key) = &self.score.metadata.key.clone() {
            self.write_key_element(key);
        }
        if let Some(meter) = &self.score.metadata.meter.clone() {
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
        let Some((beats, beat_type, symbol)) = meter_parts(&meter.display) else {
            return;
        };
        let attrs = symbol.map(|symbol| [("symbol", symbol)]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("time", attrs_slice);
        self.xml.text_element("beats", beats);
        self.xml.text_element("beat-type", beat_type);
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
                .and_then(|voice| voice.properties.clef.as_ref())
                .map(|clef| clef.text.as_str());
            let clef = clef_model(clef_text);
            let number = staff.value.to_string();
            let attrs = (part.staves.len() > 1).then_some([("number", number.as_str())]);
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
    }

    fn write_transpose_if_available(&mut self, part: &Part) {
        for voice in &part.voices {
            let Some(transpose) = voice.properties.transpose.as_ref() else {
                continue;
            };
            let Ok(chromatic) = transpose.text.trim().parse::<i32>() else {
                self.diagnostics
                    .push(unsupported_transpose_warning(transpose.span));
                continue;
            };
            self.xml.start("transpose", &[]);
            self.xml.text_element("chromatic", &chromatic.to_string());
            self.xml.end("transpose");
            return;
        }
    }
}

fn meter_parts(display: &str) -> Option<(&str, &str, Option<&'static str>)> {
    match display.trim() {
        "C" => Some(("4", "4", Some("common"))),
        "C|" => Some(("2", "2", Some("cut"))),
        "none" | "M:none" => None,
        value => {
            let (beats, beat_type) = value.split_once('/')?;
            Some((beats.trim(), beat_type.trim(), None))
        }
    }
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
