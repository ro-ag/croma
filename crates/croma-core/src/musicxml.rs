use crate::model::{Event, Tune};

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
    for (index, event) in tune.events.iter().enumerate() {
        match event {
            Event::Note {
                step,
                octave,
                duration,
            } => {
                xml.push_str("      <note>");
                xml.push_str("<pitch><step>");
                xml.push(*step);
                xml.push_str("</step><octave>");
                xml.push_str(&octave.to_string());
                xml.push_str("</octave></pitch>");
                xml.push_str(&format!("<duration>{duration}</duration>"));
                xml.push_str("<voice>1</voice><type>");
                xml.push_str(note_type(*duration, tune.divisions));
                xml.push_str("</type>");
                xml.push_str("</note>\n");
            }
            Event::Rest { duration } => {
                xml.push_str("      <note><rest/>");
                xml.push_str(&format!("<duration>{duration}</duration>"));
                xml.push_str("<voice>1</voice><type>");
                xml.push_str(note_type(*duration, tune.divisions));
                xml.push_str("</type></note>\n");
            }
            Event::Bar => {
                if !tune.events[index + 1..]
                    .iter()
                    .any(|event| !matches!(event, Event::Bar))
                {
                    continue;
                }
                measure += 1;
                xml.push_str("    </measure>\n");
                xml.push_str(&format!("    <measure number=\"{measure}\">\n"));
            }
        }
    }

    xml.push_str("    </measure>\n");
    xml.push_str("  </part>\n");
    xml.push_str("</score-partwise>\n");
    xml
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

fn note_type(duration: u32, divisions: u32) -> &'static str {
    match duration {
        value if value == divisions * 4 => "whole",
        value if value == divisions * 2 => "half",
        value if value == divisions => "quarter",
        value if value * 2 == divisions => "eighth",
        value if value * 4 == divisions => "16th",
        value if value * 8 == divisions => "32nd",
        _ => "quarter",
    }
}
