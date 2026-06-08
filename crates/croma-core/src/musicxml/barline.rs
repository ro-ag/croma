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
        match kind {
            BarlineKind::Double => self.xml.text_element("bar-style", "light-light"),
            BarlineKind::Final => self.xml.text_element("bar-style", "light-heavy"),
            BarlineKind::Initial => self.xml.text_element("bar-style", "heavy-light"),
            BarlineKind::RepeatStart => {
                self.xml.empty("repeat", &[("direction", "forward")]);
            }
            BarlineKind::RepeatEnd => {
                self.xml.empty("repeat", &[("direction", "backward")]);
            }
            BarlineKind::RepeatBoth => {
                self.xml.empty("repeat", &[("direction", "backward")]);
            }
            BarlineKind::Dotted => self.xml.text_element("bar-style", "dotted"),
            BarlineKind::Invisible => self.xml.text_element("bar-style", "none"),
            BarlineKind::Regular | BarlineKind::Liberal => {}
        }
        for child in ending_children {
            self.xml.empty(
                "ending",
                &[
                    ("number", child.number),
                    (
                        "type",
                        match child.kind {
                            EndingType::Start => "start",
                            EndingType::Stop => "stop",
                        },
                    ),
                ],
            );
        }
        self.xml.end("barline");
    }

    pub(crate) fn write_ending_barline(
        &mut self,
        location: BarlineLocation,
        endings: &[String],
        ending_type: EndingType,
        repeat_kind: Option<BarlineKind>,
    ) {
        let children = endings
            .iter()
            .map(|number| EndingChild {
                number: number.as_str(),
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
    kind: EndingType,
}
