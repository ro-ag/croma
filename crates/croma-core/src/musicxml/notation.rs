use crate::model::{DecorationAttachment, EventAttachments, SlurRole, TieRole, TupletRole};

use super::{MusicXmlWriter, TimeModification, TupletNumbers, unsupported_duration_diagnostics};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_notations(
        &mut self,
        attachments: &EventAttachments,
        time_modification: Option<TimeModification>,
        tuplet_numbers: &TupletNumbers,
    ) {
        let has_tied = !attachments.ties.is_empty();
        let has_slurs = !attachments.slurs.is_empty();
        let has_tuplets = attachments
            .tuplets
            .iter()
            .any(|tuplet| matches!(tuplet.role, TupletRole::Start | TupletRole::Stop));
        let has_notation_decorations = attachments
            .decorations
            .iter()
            .any(|decoration| decoration_notation(decoration).is_some());
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
            let number = slur.pair_id.to_string();
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
        for tuplet in &attachments.tuplets {
            let Some(tuplet_type) = (match tuplet.role {
                TupletRole::Start => Some("start"),
                TupletRole::Stop => Some("stop"),
                TupletRole::Continue => None,
            }) else {
                continue;
            };
            let number = tuplet_numbers.number_for(tuplet.pair_id).to_string();
            self.xml.empty(
                "tuplet",
                &[("type", tuplet_type), ("number", number.as_str())],
            );
        }
        if has_notation_decorations {
            let kinds = |want: fn(NotationKind) -> Option<&'static str>| {
                attachments
                    .decorations
                    .iter()
                    .filter_map(|decoration| decoration_notation(decoration).and_then(want))
                    .collect::<Vec<_>>()
            };
            // MusicXML groups these per category, in schema order: ornaments,
            // technical, articulations, then fermata.
            let ornaments = kinds(|kind| match kind {
                NotationKind::Ornament(name) => Some(name),
                _ => None,
            });
            if !ornaments.is_empty() {
                self.xml.start("ornaments", &[]);
                for name in ornaments {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("ornaments");
            }
            let technical = kinds(|kind| match kind {
                NotationKind::Technical(name) => Some(name),
                _ => None,
            });
            if !technical.is_empty() {
                self.xml.start("technical", &[]);
                for name in technical {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("technical");
            }
            let articulations = kinds(|kind| match kind {
                NotationKind::Articulation(name) => Some(name),
                _ => None,
            });
            if !articulations.is_empty() {
                self.xml.start("articulations", &[]);
                for name in articulations {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("articulations");
            }
            for kind in attachments
                .decorations
                .iter()
                .filter_map(decoration_notation)
            {
                if let NotationKind::Fermata(fermata_type) = kind {
                    self.xml.empty("fermata", &[("type", fermata_type)]);
                }
            }
        }
        if time_modification.is_none() {
            self.diagnostics
                .extend(unsupported_duration_diagnostics(attachments));
        }
        self.xml.end("notations");
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotationKind {
    /// Inside `<ornaments>` (e.g. trill, mordent, turn).
    Ornament(&'static str),
    /// Inside `<articulations>` (e.g. staccato, accent, tenuto).
    Articulation(&'static str),
    /// Inside `<technical>` (e.g. up-bow, down-bow, open string).
    Technical(&'static str),
    /// A `<fermata>` element with the given type attribute.
    Fermata(&'static str),
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
        _ => return None,
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
