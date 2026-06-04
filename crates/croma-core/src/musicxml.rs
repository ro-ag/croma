use crate::model::{Accidental, BarlineKind, Event, RestVisibility, Tune};

pub fn write_score_partwise(tune: &Tune) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<score-partwise version=\"4.0\">\n");
    xml.push_str("  <part-list>\n");
    xml.push_str("    <score-part id=\"P1\"><part-name>");
    xml.push_str(&escape_xml(if tune.title.is_empty() {
        "Music"
    } else {
        &tune.title
    }));
    xml.push_str("</part-name></score-part>\n");
    xml.push_str("  </part-list>\n");
    xml.push_str("  <part id=\"P1\">\n");
    xml.push_str("    <measure number=\"1\">\n");
    xml.push_str("      <attributes>\n");
    xml.push_str(&format!(
        "        <divisions>{}</divisions>\n",
        tune.divisions
    ));
    xml.push_str("        <key><fifths>0</fifths></key>\n");
    let (beats, beat_type) = meter_parts(&tune.meter);
    xml.push_str(&format!(
        "        <time><beats>{beats}</beats><beat-type>{beat_type}</beat-type></time>\n"
    ));
    xml.push_str("        <clef><sign>G</sign><line>2</line></clef>\n");
    xml.push_str("      </attributes>\n");

    let mut measure = 1;
    let mut measure_has_time = false;
    for (index, event) in tune.events.iter().enumerate() {
        match event {
            Event::Note {
                step,
                octave,
                accidental,
                chord,
                duration,
                ..
            } => {
                xml.push_str("      <note>");
                if *chord {
                    xml.push_str("<chord/>");
                }
                xml.push_str("<pitch><step>");
                xml.push(*step);
                xml.push_str("</step>");
                if let Some(accidental) = accidental
                    && accidental.alter() != 0
                {
                    xml.push_str("<alter>");
                    xml.push_str(&accidental.alter().to_string());
                    xml.push_str("</alter>");
                }
                xml.push_str("<octave>");
                xml.push_str(&octave.to_string());
                xml.push_str("</octave></pitch>");
                xml.push_str(&format!("<duration>{duration}</duration>"));
                xml.push_str("<voice>1</voice>");
                write_note_type(&mut xml, *duration, tune.divisions);
                if let Some(accidental) = accidental {
                    write_accidental(&mut xml, *accidental);
                }
                xml.push_str("</note>\n");
                measure_has_time = true;
            }
            Event::Rest {
                visibility,
                duration,
                ..
            } => {
                xml.push_str("      <note><rest");
                if *visibility == RestVisibility::Invisible {
                    xml.push_str(" print-object=\"no\"");
                }
                xml.push_str("/>");
                xml.push_str(&format!("<duration>{duration}</duration>"));
                xml.push_str("<voice>1</voice>");
                write_note_type(&mut xml, *duration, tune.divisions);
                xml.push_str("</note>\n");
                measure_has_time = true;
            }
            Event::Spacer { .. } => {}
            Event::Barline { kind, .. } => {
                write_barline(&mut xml, *kind);
                if has_future_time(&tune.events[index + 1..]) {
                    if !measure_has_time && matches!(kind, BarlineKind::RepeatStart) {
                        continue;
                    }
                    measure += 1;
                    measure_has_time = false;
                    xml.push_str("    </measure>\n");
                    xml.push_str(&format!("    <measure number=\"{measure}\">\n"));
                    if matches!(kind, BarlineKind::RepeatBoth) {
                        write_barline(&mut xml, BarlineKind::RepeatStart);
                    }
                }
            }
        }
    }

    xml.push_str("    </measure>\n");
    xml.push_str("  </part>\n");
    xml.push_str("</score-partwise>\n");
    xml
}

fn write_accidental(xml: &mut String, accidental: Accidental) {
    xml.push_str("<accidental>");
    xml.push_str(accidental.musicxml_name());
    xml.push_str("</accidental>");
}

fn write_note_type(xml: &mut String, duration: u32, divisions: u32) {
    let (note_type, dots) = note_type_and_dots(duration, divisions);
    xml.push_str("<type>");
    xml.push_str(note_type);
    xml.push_str("</type>");
    for _ in 0..dots {
        xml.push_str("<dot/>");
    }
}

fn write_barline(xml: &mut String, kind: BarlineKind) {
    match kind {
        BarlineKind::Regular | BarlineKind::Liberal => {}
        BarlineKind::Double => {
            xml.push_str(
                "      <barline location=\"right\"><bar-style>light-light</bar-style></barline>\n",
            );
        }
        BarlineKind::Final => {
            xml.push_str(
                "      <barline location=\"right\"><bar-style>light-heavy</bar-style></barline>\n",
            );
        }
        BarlineKind::Initial => {
            xml.push_str(
                "      <barline location=\"left\"><bar-style>heavy-light</bar-style></barline>\n",
            );
        }
        BarlineKind::RepeatStart => {
            xml.push_str(
                "      <barline location=\"left\"><repeat direction=\"forward\"/></barline>\n",
            );
        }
        BarlineKind::RepeatEnd => {
            xml.push_str(
                "      <barline location=\"right\"><repeat direction=\"backward\"/></barline>\n",
            );
        }
        BarlineKind::RepeatBoth => {
            xml.push_str(
                "      <barline location=\"right\"><repeat direction=\"backward\"/></barline>\n",
            );
        }
        BarlineKind::Dotted => {
            xml.push_str(
                "      <barline location=\"right\"><bar-style>dotted</bar-style></barline>\n",
            );
        }
        BarlineKind::Invisible => {
            xml.push_str(
                "      <barline location=\"right\"><bar-style>none</bar-style></barline>\n",
            );
        }
    }
}

fn has_future_time(events: &[Event]) -> bool {
    events.iter().any(Event::is_time_bearing)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn meter_parts(meter: &str) -> (&str, &str) {
    meter.split_once('/').unwrap_or(("4", "4"))
}

fn note_type_and_dots(duration: u32, divisions: u32) -> (&'static str, u8) {
    for (name, base) in note_type_candidates(divisions) {
        for dots in 0..=3 {
            if dotted_duration(base, dots) == duration {
                return (name, dots);
            }
        }
    }
    ("quarter", 0)
}

fn note_type_candidates(divisions: u32) -> Vec<(&'static str, u32)> {
    let mut candidates = vec![
        ("whole", divisions.saturating_mul(4)),
        ("half", divisions.saturating_mul(2)),
        ("quarter", divisions),
    ];

    for (name, divisor) in [("eighth", 2), ("16th", 4), ("32nd", 8), ("64th", 16)] {
        if divisions.is_multiple_of(divisor) {
            candidates.push((name, divisions / divisor));
        }
    }

    candidates
}

fn dotted_duration(base: u32, dots: u8) -> u32 {
    let mut duration = base;
    let mut dot_value = base;
    for _ in 0..dots {
        dot_value /= 2;
        duration += dot_value;
    }
    duration
}
