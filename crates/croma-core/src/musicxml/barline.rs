use crate::model::BarlineKind;

use super::{BarlineLocation, EndingType, MusicXmlWriter};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_barline(
        &mut self,
        location: BarlineLocation,
        kind: BarlineKind,
        ending_children: &[EndingChild<'_>],
    ) {
        match kind {
            BarlineKind::Regular | BarlineKind::Liberal if ending_children.is_empty() => {
                return;
            }
            _ => {}
        }
        let location = location.as_str();
        self.xml.start("barline", &[("location", location)]);
        // MusicXML's barline content model orders bar-style, then <ending>,
        // then <repeat>.
        match kind {
            BarlineKind::Double => self.xml.text_element("bar-style", "light-light"),
            BarlineKind::Final => self.xml.text_element("bar-style", "light-heavy"),
            BarlineKind::Initial => self.xml.text_element("bar-style", "heavy-light"),
            BarlineKind::Dotted => self.xml.text_element("bar-style", "dotted"),
            BarlineKind::Dashed => self.xml.text_element("bar-style", "dashed"),
            BarlineKind::Invisible => self.xml.text_element("bar-style", "none"),
            BarlineKind::RepeatStart => self.xml.text_element("bar-style", "heavy-light"),
            BarlineKind::RepeatEnd | BarlineKind::RepeatBoth => {
                self.xml.text_element("bar-style", "light-heavy");
            }
            BarlineKind::Regular | BarlineKind::Liberal => {}
        }
        for child in ending_children {
            let attrs = [
                ("number", child.number),
                (
                    "type",
                    match child.kind {
                        EndingType::Start => "start",
                        EndingType::Stop => "stop",
                        EndingType::Discontinue => "discontinue",
                    },
                ),
            ];
            if let Some(text) = child.text {
                self.xml.text_element_attrs("ending", &attrs, text);
            } else {
                self.xml.empty("ending", &attrs);
            }
        }
        match kind {
            BarlineKind::RepeatStart => {
                self.xml.empty("repeat", &[("direction", "forward")]);
            }
            BarlineKind::RepeatEnd | BarlineKind::RepeatBoth => {
                self.xml.empty("repeat", &[("direction", "backward")]);
            }
            _ => {}
        }
        self.xml.end("barline");
    }

    pub(crate) fn write_ending_barline(
        &mut self,
        location: BarlineLocation,
        endings: &[EndingDisplay],
        ending_type: EndingType,
        repeat_kind: Option<BarlineKind>,
    ) {
        let children = endings
            .iter()
            .map(|ending| EndingChild {
                number: ending.number.as_str(),
                text: match ending_type {
                    EndingType::Start => ending.text.as_deref(),
                    EndingType::Stop | EndingType::Discontinue => None,
                },
                kind: ending_type,
            })
            .collect::<Vec<_>>();
        self.write_barline(
            location,
            repeat_kind.unwrap_or(BarlineKind::Regular),
            &children,
        );
    }
}

pub(crate) struct EndingChild<'a> {
    number: &'a str,
    text: Option<&'a str>,
    kind: EndingType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EndingDisplay {
    pub(crate) number: String,
    pub(crate) text: Option<String>,
}
