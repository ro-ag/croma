use crate::model::{DecorationAttachment, EventAttachments, SlurRole, TieRole, TupletRole};

use super::{MusicXmlWriter, TimeModification, TupletNumbers, unsupported_duration_diagnostics};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_notations(
        &mut self,
        attachments: &EventAttachments,
        time_modification: Option<TimeModification>,
        tuplet_numbers: &TupletNumbers,
        slur_voice_key: &str,
    ) {
        let has_tied = !attachments.ties.is_empty();
        let has_slurs = !attachments.slurs.is_empty();
        let notation_kinds = attachments
            .decorations
            .iter()
            .filter_map(decoration_notation)
            .collect::<Vec<_>>();
        let has_tuplets = attachments
            .tuplets
            .iter()
            .any(|tuplet| matches!(tuplet.role, TupletRole::Start | TupletRole::Stop));
        let has_notation_decorations = notation_kinds.iter().any(NotationKind::emits_own_notation);
        if !(has_tied || has_slurs || has_tuplets || has_notation_decorations) {
            return;
        }
        self.xml.start("notations", &[]);
        for tie in &attachments.ties {
            let number = tie.pair_id.to_string();
            let mut attrs = vec![
                (
                    "type",
                    match tie.role {
                        TieRole::Start => "start",
                        TieRole::Stop => "stop",
                    },
                ),
                ("number", number.as_str()),
            ];
            if tie.dotted {
                attrs.push(("line-type", "dotted"));
            }
            self.xml.empty("tied", &attrs);
        }
        for slur in &attachments.slurs {
            let number = self
                .slur_numbers
                .number_for(slur_voice_key, slur.pair_id, slur.role)
                .to_string();
            let mut attrs = vec![
                (
                    "type",
                    match slur.role {
                        SlurRole::Start => "start",
                        SlurRole::Stop => "stop",
                    },
                ),
                ("number", number.as_str()),
            ];
            if slur.dotted {
                attrs.push(("line-type", "dotted"));
            }
            self.xml.empty("slur", &attrs);
        }
        let mut tuplet_displays = notation_kinds.iter().filter_map(|kind| {
            if let NotationKind::TupletDisplay(display) = kind {
                Some(display)
            } else {
                None
            }
        });
        for tuplet in &attachments.tuplets {
            let Some(tuplet_type) = (match tuplet.role {
                TupletRole::Start => Some("start"),
                TupletRole::Stop => Some("stop"),
                TupletRole::Continue => None,
            }) else {
                continue;
            };
            let number = tuplet_numbers.number_for(tuplet.pair_id).to_string();
            let display = (tuplet_type == "start")
                .then(|| tuplet_displays.next())
                .flatten();
            self.write_tuplet_notation(tuplet_type, number.as_str(), display);
        }
        if has_notation_decorations {
            let notation_kinds = notation_kinds
                .iter()
                .filter(|kind| kind.emits_own_notation())
                .cloned()
                .collect::<Vec<_>>();
            let kinds = |want: fn(&NotationKind) -> Option<&'static str>| {
                notation_kinds.iter().filter_map(want).collect::<Vec<_>>()
            };
            // MusicXML groups these per category, in schema order: ornaments,
            // technical, articulations, fermata, then arpeggiate.
            for kind in &notation_kinds {
                if let NotationKind::Spanner {
                    element,
                    spanner_type,
                    line_type,
                    number,
                    text,
                } = kind
                {
                    let attrs = [
                        ("type", *spanner_type),
                        ("number", number.as_str()),
                        ("line-type", *line_type),
                    ];
                    if text.is_empty() {
                        self.xml.empty(element, &attrs);
                    } else {
                        self.xml.text_element_attrs(element, &attrs, text);
                    }
                }
            }
            let ornaments = notation_kinds
                .iter()
                .filter(|kind| {
                    matches!(
                        kind,
                        NotationKind::Ornament(_)
                            | NotationKind::Tremolo { .. }
                            | NotationKind::WavyLine { .. }
                    )
                })
                .collect::<Vec<_>>();
            if !ornaments.is_empty() {
                self.xml.start("ornaments", &[]);
                for kind in ornaments {
                    match kind {
                        NotationKind::Ornament(name) => self.xml.empty(name, &[]),
                        NotationKind::WavyLine { wavy_type, number } => self
                            .xml
                            .empty("wavy-line", &[("type", wavy_type), ("number", number)]),
                        NotationKind::Tremolo {
                            tremolo_type,
                            marks,
                        } => {
                            self.xml
                                .text_element_attrs("tremolo", &[("type", tremolo_type)], marks)
                        }
                        _ => unreachable!("ornaments list only contains ornament notations"),
                    }
                }
                self.xml.end("ornaments");
            }
            let articulations = kinds(|kind| match kind {
                NotationKind::Articulation(name) => Some(name),
                _ => None,
            });
            let technical = notation_kinds
                .iter()
                .filter(|kind| {
                    matches!(
                        kind,
                        NotationKind::Technical(_) | NotationKind::TechnicalText { .. }
                    )
                })
                .collect::<Vec<_>>();
            if !technical.is_empty() {
                self.xml.start("technical", &[]);
                for kind in technical {
                    match kind {
                        NotationKind::Technical(name) => self.xml.empty(name, &[]),
                        NotationKind::TechnicalText { name, text } => {
                            self.xml.text_element(name, text);
                        }
                        _ => unreachable!("technical list only contains technical notations"),
                    }
                }
                self.xml.end("technical");
            }
            if !articulations.is_empty() {
                self.xml.start("articulations", &[]);
                for name in articulations {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("articulations");
            }
            for kind in &notation_kinds {
                if let NotationKind::Fermata(fermata_type) = kind {
                    self.xml.empty("fermata", &[("type", fermata_type)]);
                }
            }
            for kind in notation_kinds {
                if matches!(kind, NotationKind::Arpeggiate) {
                    self.xml.empty("arpeggiate", &[]);
                }
            }
        }
        if time_modification.is_none() {
            self.diagnostics
                .extend(unsupported_duration_diagnostics(attachments));
        }
        self.xml.end("notations");
    }

    fn write_tuplet_notation(
        &mut self,
        tuplet_type: &'static str,
        number: &str,
        display: Option<&TupletDisplay>,
    ) {
        let mut attrs = vec![("type", tuplet_type)];
        if let Some(display) = display {
            if let Some(bracket) = display.bracket {
                attrs.push(("bracket", bracket));
            }
            if let (Some(actual), Some(normal)) = (&display.actual, &display.normal) {
                self.xml.start("tuplet", &attrs);
                self.write_tuplet_display_detail("tuplet-actual", actual);
                self.write_tuplet_display_detail("tuplet-normal", normal);
                self.xml.end("tuplet");
            } else {
                self.xml.empty("tuplet", &attrs);
            }
        } else {
            attrs.push(("number", number));
            self.xml.empty("tuplet", &attrs);
        }
    }

    fn write_tuplet_display_detail(&mut self, element: &'static str, detail: &TupletDisplayDetail) {
        self.xml.start(element, &[]);
        self.xml.text_element("tuplet-number", &detail.number);
        self.xml.text_element("tuplet-type", detail.note_type);
        for _ in 0..detail.dots {
            self.xml.empty("tuplet-dot", &[]);
        }
        self.xml.end(element);
    }

    pub(crate) fn write_time_modification(&mut self, time_modification: TimeModification) {
        self.xml.start("time-modification", &[]);
        self.xml
            .text_element("actual-notes", &time_modification.actual_notes.to_string());
        self.xml
            .text_element("normal-notes", &time_modification.normal_notes.to_string());
        self.xml.end("time-modification");
    }
}

/// MusicXML `<notations>` category and element for an ABC `!decoration!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NotationKind {
    /// Inside `<ornaments>` (e.g. trill, mordent, turn).
    Ornament(&'static str),
    /// Inside `<articulations>` (e.g. staccato, accent, tenuto).
    Articulation(&'static str),
    /// Inside `<technical>` (e.g. up-bow, down-bow, open string).
    Technical(&'static str),
    /// Text inside a `<technical>` child element (e.g. fingering).
    TechnicalText { name: &'static str, text: String },
    /// A `<fermata>` element with the given type attribute.
    Fermata(&'static str),
    /// A value-bearing MusicXML `<ornaments><tremolo type="...">N</tremolo>`.
    Tremolo {
        tremolo_type: &'static str,
        marks: &'static str,
    },
    /// Display metadata to write inside a generated `<tuplet type="start">`.
    TupletDisplay(TupletDisplay),
    /// A MusicXML `<glissando>` or `<slide>` notation.
    Spanner {
        element: &'static str,
        spanner_type: &'static str,
        line_type: &'static str,
        number: String,
        text: String,
    },
    /// A MusicXML `<ornaments><wavy-line .../>` notation.
    WavyLine {
        wavy_type: &'static str,
        number: String,
    },
    /// A note/chord arpeggiation mark.
    Arpeggiate,
}

impl NotationKind {
    fn emits_own_notation(&self) -> bool {
        !matches!(self, NotationKind::TupletDisplay(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TupletDisplay {
    bracket: Option<&'static str>,
    actual: Option<TupletDisplayDetail>,
    normal: Option<TupletDisplayDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TupletDisplayDetail {
    number: String,
    note_type: &'static str,
    dots: u8,
}

/// Map an ABC decoration to its MusicXML notation, per the ABC 2.1 decoration
/// list and the MusicXML notation categories. Decorations handled elsewhere as
/// dynamics or directions return `None`.
pub(crate) fn decoration_notation(decoration: &DecorationAttachment) -> Option<NotationKind> {
    Some(match decoration.name.as_str() {
        "." | "staccato" => NotationKind::Articulation("staccato"),
        ">" | "accent" | "emphasis" => NotationKind::Articulation("accent"),
        "tenuto" => NotationKind::Articulation("tenuto"),
        "wedge" => NotationKind::Articulation("staccatissimo"),
        "marcato" => NotationKind::Articulation("strong-accent"),
        "breath" => NotationKind::Articulation("breath-mark"),
        "caesura" => NotationKind::Articulation("caesura"),
        "detached-legato" => NotationKind::Articulation("detached-legato"),
        "falloff" => NotationKind::Articulation("falloff"),
        "doit" => NotationKind::Articulation("doit"),
        "fermata" => NotationKind::Fermata("upright"),
        "invertedfermata" => NotationKind::Fermata("inverted"),
        "trill" => NotationKind::Ornament("trill-mark"),
        "mordent" | "lowermordent" => NotationKind::Ornament("mordent"),
        "uppermordent" | "pralltriller" => NotationKind::Ornament("inverted-mordent"),
        "turn" => NotationKind::Ornament("turn"),
        "invertedturn" => NotationKind::Ornament("inverted-turn"),
        "upbow" => NotationKind::Technical("up-bow"),
        "downbow" => NotationKind::Technical("down-bow"),
        "open" => NotationKind::Technical("open-string"),
        "thumb" => NotationKind::Technical("thumb-position"),
        "snap" => NotationKind::Technical("snap-pizzicato"),
        // `!+!` is left-hand pizzicato (ABC 2.1 line 1101); MusicXML's
        // <stopped/> renders the same + glyph on the note.
        "+" | "plus" => NotationKind::Technical("stopped"),
        "0" => NotationKind::TechnicalText {
            name: "fingering",
            text: "0".to_owned(),
        },
        "1" => NotationKind::TechnicalText {
            name: "fingering",
            text: "1".to_owned(),
        },
        "2" => NotationKind::TechnicalText {
            name: "fingering",
            text: "2".to_owned(),
        },
        "3" => NotationKind::TechnicalText {
            name: "fingering",
            text: "3".to_owned(),
        },
        "4" => NotationKind::TechnicalText {
            name: "fingering",
            text: "4".to_owned(),
        },
        "5" => NotationKind::TechnicalText {
            name: "fingering",
            text: "5".to_owned(),
        },
        "arpeggio" => NotationKind::Arpeggiate,
        "slide" => NotationKind::Articulation("scoop"),
        _ => {
            return tremolo_notation(decoration.name.as_str())
                .or_else(|| technical_value_notation(decoration.name.as_str()))
                .or_else(|| spanner_notation(decoration.name.as_str()))
                .or_else(|| tuplet_display_notation(decoration.name.as_str()));
        }
    })
}

pub(crate) fn tuplet_display_decoration_name(
    bracket: Option<&str>,
    actual: Option<(&str, &str, usize)>,
    normal: Option<(&str, &str, usize)>,
) -> Option<String> {
    let bracket = match bracket {
        Some(value) => bracket_attr(value)?,
        None => "none",
    };
    let mut name = format!("musicxml-tuplet-detail-v1-b{bracket}");
    match (actual, normal) {
        (
            Some((actual_number, actual_type, actual_dots)),
            Some((normal_number, normal_type, normal_dots)),
        ) => {
            let actual_type = note_type_attr(actual_type)?;
            let normal_type = note_type_attr(normal_type)?;
            if positive_decimal_text(actual_number).is_none()
                || positive_decimal_text(normal_number).is_none()
            {
                return None;
            }
            name.push_str(&format!(
                "-a{actual_number}-{actual_type}-{actual_dots}-n{normal_number}-{normal_type}-{normal_dots}"
            ));
            Some(name)
        }
        (None, None) => (bracket != "none").then_some(name),
        _ => None,
    }
}

fn tuplet_display_notation(name: &str) -> Option<NotationKind> {
    let rest = name.strip_prefix("musicxml-tuplet-detail-v1-b")?;
    let (bracket, detail) = match rest.split_once("-a") {
        Some((bracket, rest)) => {
            let (actual, normal) = rest.split_once("-n")?;
            (
                bracket,
                Some((
                    tuplet_display_detail(actual)?,
                    tuplet_display_detail(normal)?,
                )),
            )
        }
        None => (rest, None),
    };
    if bracket.contains('-') {
        return None;
    }
    let bracket = match bracket {
        "none" => None,
        value => Some(bracket_attr(value)?),
    };
    let (actual, normal) = detail
        .map(|(actual, normal)| (Some(actual), Some(normal)))
        .unwrap_or((None, None));
    if bracket.is_none() && actual.is_none() {
        return None;
    }
    Some(NotationKind::TupletDisplay(TupletDisplay {
        bracket,
        actual,
        normal,
    }))
}

fn tuplet_display_detail(text: &str) -> Option<TupletDisplayDetail> {
    let mut parts = text.split('-');
    let number = positive_decimal_text(parts.next()?)?.to_owned();
    let note_type = note_type_attr(parts.next()?)?;
    let dots = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(TupletDisplayDetail {
        number,
        note_type,
        dots,
    })
}

fn bracket_attr(value: &str) -> Option<&'static str> {
    match value {
        "yes" => Some("yes"),
        "no" => Some("no"),
        _ => None,
    }
}

fn note_type_attr(value: &str) -> Option<&'static str> {
    match value {
        "1024th" => Some("1024th"),
        "512th" => Some("512th"),
        "256th" => Some("256th"),
        "128th" => Some("128th"),
        "64th" => Some("64th"),
        "32nd" => Some("32nd"),
        "16th" => Some("16th"),
        "eighth" => Some("eighth"),
        "quarter" => Some("quarter"),
        "half" => Some("half"),
        "whole" => Some("whole"),
        "breve" => Some("breve"),
        "long" => Some("long"),
        "maxima" => Some("maxima"),
        _ => None,
    }
}

fn spanner_notation(name: &str) -> Option<NotationKind> {
    if let Some(rest) = name.strip_prefix("musicxml-glissando-") {
        return direct_spanner_notation("glissando", rest);
    }
    if let Some(rest) = name.strip_prefix("musicxml-slide-") {
        return direct_spanner_notation("slide", rest);
    }
    let rest = name.strip_prefix("musicxml-wavy-line-")?;
    let (wavy_type, number) = rest.split_once('-')?;
    if number.contains('-') {
        return None;
    }
    Some(NotationKind::WavyLine {
        wavy_type: wavy_type_attr(wavy_type)?,
        number: decimal_text(number)?.to_owned(),
    })
}

fn direct_spanner_notation(element: &'static str, rest: &str) -> Option<NotationKind> {
    let (head, text) = match rest.split_once("-hex-") {
        Some((head, hex)) => (head, decode_hex_utf8(hex)?),
        None => (rest, String::new()),
    };
    let mut parts = head.split('-');
    let spanner_type = direct_spanner_type_attr(parts.next()?)?;
    let line_type = line_type_attr(parts.next()?)?;
    let number = decimal_text(parts.next()?)?.to_owned();
    if parts.next().is_some() {
        return None;
    }
    Some(NotationKind::Spanner {
        element,
        spanner_type,
        line_type,
        number,
        text,
    })
}

fn direct_spanner_type_attr(value: &str) -> Option<&'static str> {
    match value {
        "start" => Some("start"),
        "stop" => Some("stop"),
        _ => None,
    }
}

fn wavy_type_attr(value: &str) -> Option<&'static str> {
    match value {
        "start" => Some("start"),
        "stop" => Some("stop"),
        "continue" => Some("continue"),
        _ => None,
    }
}

fn line_type_attr(value: &str) -> Option<&'static str> {
    match value {
        "solid" => Some("solid"),
        "dashed" => Some("dashed"),
        "dotted" => Some("dotted"),
        "wavy" => Some("wavy"),
        _ => None,
    }
}

fn decimal_text(text: &str) -> Option<&str> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())).then_some(text)
}

fn positive_decimal_text(text: &str) -> Option<&str> {
    decimal_text(text).filter(|text| text.parse::<u32>().is_ok_and(|value| value > 0))
}

fn technical_value_notation(name: &str) -> Option<NotationKind> {
    if let Some(text) = name.strip_prefix("musicxml-tech-string-") {
        return decimal_technical_text("string", text);
    }
    if let Some(text) = name.strip_prefix("musicxml-tech-fret-") {
        return decimal_technical_text("fret", text);
    }
    let hex = name.strip_prefix("musicxml-tech-fingering-hex-")?;
    let text = decode_hex_utf8(hex)?;
    (!text.is_empty()).then_some(NotationKind::TechnicalText {
        name: "fingering",
        text,
    })
}

fn decimal_technical_text(name: &'static str, text: &str) -> Option<NotationKind> {
    decimal_text(text).map(|text| NotationKind::TechnicalText {
        name,
        text: text.to_owned(),
    })
}

fn decode_hex_utf8(hex: &str) -> Option<String> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks_exact(2) {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes.push((hi << 4) | lo);
    }
    let text = String::from_utf8(bytes).ok()?;
    text.chars().all(is_xml_char).then_some(text)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_xml_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

fn tremolo_notation(name: &str) -> Option<NotationKind> {
    let rest = name.strip_prefix("musicxml-tremolo-")?;
    let (tremolo_type, marks) = rest.rsplit_once('-')?;
    let tremolo_type = match tremolo_type {
        "single" => "single",
        "start" => "start",
        "stop" => "stop",
        _ => return None,
    };
    let marks = match marks {
        "0" => "0",
        "1" => "1",
        "2" => "2",
        "3" => "3",
        "4" => "4",
        "5" => "5",
        "6" => "6",
        "7" => "7",
        "8" => "8",
        _ => return None,
    };
    Some(NotationKind::Tremolo {
        tremolo_type,
        marks,
    })
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DirectionSymbol {
    Coda,
    Segno,
}

pub(crate) fn symbol_direction(name: &str) -> Option<DirectionSymbol> {
    match name.to_ascii_lowercase().as_str() {
        "coda" => Some(DirectionSymbol::Coda),
        "segno" => Some(DirectionSymbol::Segno),
        _ => None,
    }
}
