//! Static documentation tables for hover and completion, sourced from the ABC
//! 2.1 standard and **grounded in what `croma-core` actually recognises** — not
//! invented. Per the promotion spec (decision 4, "hover/completion are static
//! tables"), these are pure presentation over the core's existing taxonomy: no
//! new spec, no core change.
//!
//! Two tables:
//!
//! - [`FIELD_KEYS`] — the ABC 2.1 §3.1 information-field set (letter → name +
//!   one-line doc + spec reference). Cross-checked against the field kinds
//!   `croma_core::parse::field` recognises (`X T C O A M L Q P Z N G H K R B D F
//!   S I U V W w m r s`).
//! - [`DECORATIONS`] — the decoration names croma maps to MusicXML, drawn from
//!   `croma_core`'s `decoration_notation` (notations: staccato, accent, tenuto,
//!   marcato, fermata, trill, mordent, …), `direction` (dynamics p/f/mf/…, coda,
//!   segno, crescendo/diminuendo hairpins), and the single-char shorthands in
//!   `parse::music::shorthand_canonical_name` (`~ H L M O P S T u v`). Each entry
//!   names the meaning and any shorthand. Cited to ABC 2.1 §4.14.
//!
//! The doc strings are short Markdown so they render in an LSP `MarkupContent`
//! hover and a `CompletionItem.documentation`.

/// The ABC 2.1 spec page every entry references.
pub const ABC_SPEC_URL: &str = "https://abcnotation.com/wiki/abc:standard:v2.1";

/// One ABC information field: its letter, human name, a one-line description, and
/// the ABC 2.1 section it is defined in.
#[derive(Debug, Clone, Copy)]
pub struct FieldKey {
    /// The single-letter field code (e.g. `X`, `T`, `K`).
    pub letter: char,
    /// The field's name (e.g. "reference number", "tune title").
    pub name: &'static str,
    /// A one-line description of the field's purpose.
    pub doc: &'static str,
    /// The ABC 2.1 section reference (e.g. "§3.1.1").
    pub section: &'static str,
}

impl FieldKey {
    /// The completion insert/label text: the field letter followed by its colon
    /// (`X:`), matching how a header field is written.
    pub fn insert_text(&self) -> String {
        format!("{}:", self.letter)
    }

    /// The Markdown documentation body shown in hover / completion detail.
    pub fn markdown(&self) -> String {
        format!(
            "**`{letter}:`** — {name} (ABC 2.1 {section})\n\n{doc}\n\n[ABC 2.1 standard]({url})",
            letter = self.letter,
            name = self.name,
            section = self.section,
            doc = self.doc,
            url = ABC_SPEC_URL,
        )
    }
}

/// The ABC 2.1 §3.1 information fields, in spec letter order. The set matches the
/// field codes `croma_core` recognises (see `parse::field`): the tune-body /
/// common-field letters `X T C O A M L Q P Z N G H K R B D F S I U V W w m r s`.
///
/// `K` (key) is the field that ends a tune header; `X` (reference number) starts
/// a tune. Both are the high-value completions at a header line start.
pub const FIELD_KEYS: &[FieldKey] = &[
    FieldKey {
        letter: 'X',
        name: "reference number",
        doc: "Starts a tune and gives it a unique number within the file.",
        section: "§3.1.1",
    },
    FieldKey {
        letter: 'T',
        name: "tune title",
        doc: "The title of the tune; may appear more than once for subtitles.",
        section: "§3.1.2",
    },
    FieldKey {
        letter: 'C',
        name: "composer",
        doc: "The composer of the tune.",
        section: "§3.1.3",
    },
    FieldKey {
        letter: 'O',
        name: "origin",
        doc: "The geographic or cultural origin of the tune.",
        section: "§3.1.4",
    },
    FieldKey {
        letter: 'A',
        name: "area",
        doc: "A more specific area within the origin (deprecated in favour of O:).",
        section: "§3.1.5",
    },
    FieldKey {
        letter: 'M',
        name: "meter",
        doc: "The time signature, e.g. `M:4/4`, `M:6/8`, `M:C`, or `M:none`.",
        section: "§3.1.6",
    },
    FieldKey {
        letter: 'L',
        name: "unit note length",
        doc: "The default note length, e.g. `L:1/8`. Defaults from the meter when absent.",
        section: "§3.1.7",
    },
    FieldKey {
        letter: 'Q',
        name: "tempo",
        doc: "The tempo, e.g. `Q:1/4=120` or `Q:\"Allegro\"`.",
        section: "§3.1.8",
    },
    FieldKey {
        letter: 'P',
        name: "parts",
        doc: "In the header, the play order of parts (`P:ABAB`); in the body, a part/section label.",
        section: "§3.1.9",
    },
    FieldKey {
        letter: 'Z',
        name: "transcription",
        doc: "Transcription notes — who digitised the tune and any related remarks.",
        section: "§3.1.10",
    },
    FieldKey {
        letter: 'N',
        name: "notes",
        doc: "Free-text annotations about the tune.",
        section: "§3.1.11",
    },
    FieldKey {
        letter: 'G',
        name: "group",
        doc: "Grouping information used by some indexing software.",
        section: "§3.1.12",
    },
    FieldKey {
        letter: 'H',
        name: "history",
        doc: "The history or background of the tune; may span several lines.",
        section: "§3.1.13",
    },
    FieldKey {
        letter: 'K',
        name: "key",
        doc: "The key signature, e.g. `K:C`, `K:Gmaj`, `K:Ddor`. The `K:` line ends the tune header.",
        section: "§3.1.14",
    },
    FieldKey {
        letter: 'R',
        name: "rhythm",
        doc: "The rhythm or dance type, e.g. `R:reel`, `R:jig`.",
        section: "§3.1.15",
    },
    FieldKey {
        letter: 'B',
        name: "book",
        doc: "The book or collection the tune was published in.",
        section: "§3.1.16",
    },
    FieldKey {
        letter: 'D',
        name: "discography",
        doc: "A recording on which the tune appears.",
        section: "§3.1.17",
    },
    FieldKey {
        letter: 'F',
        name: "file URL",
        doc: "A URL or file reference for the source of the tune.",
        section: "§3.1.18",
    },
    FieldKey {
        letter: 'S',
        name: "source",
        doc: "Where the tune was collected from (a person, recording, or manuscript).",
        section: "§3.1.19",
    },
    FieldKey {
        letter: 'I',
        name: "instruction",
        doc: "A processing instruction / stylesheet directive, e.g. `I:linebreak $`.",
        section: "§3.1.20",
    },
    FieldKey {
        letter: 'V',
        name: "voice",
        doc: "Defines or selects a voice, e.g. `V:1 clef=treble name=\"Flute\"`.",
        section: "§4.1",
    },
    FieldKey {
        letter: 'W',
        name: "words (after tune)",
        doc: "Lyrics printed after the tune (capital W), as opposed to aligned `w:` lines.",
        section: "§3.1.21",
    },
    FieldKey {
        letter: 'w',
        name: "words (aligned)",
        doc: "Lyrics aligned under the preceding music line, syllable by syllable.",
        section: "§4.16",
    },
    FieldKey {
        letter: 'm',
        name: "macro",
        doc: "Defines a macro that expands within the tune body.",
        section: "§3.1.22",
    },
    FieldKey {
        letter: 'r',
        name: "remark",
        doc: "An inline remark/comment field, ignored by typesetters.",
        section: "§3.1.23",
    },
    FieldKey {
        letter: 's',
        name: "symbol line",
        doc: "A line of decoration symbols aligned under the preceding music line.",
        section: "§4.17",
    },
];

/// Look up the field entry for a letter, if it is a recognised ABC field.
pub fn field_key(letter: char) -> Option<&'static FieldKey> {
    FIELD_KEYS.iter().find(|field| field.letter == letter)
}

/// One ABC decoration: its canonical name, the meaning shown on hover, and an
/// optional single-character shorthand (ABC 2.1 §4.14) that also produces it.
#[derive(Debug, Clone, Copy)]
pub struct Decoration {
    /// The canonical decoration name as written between `!`/`+` delimiters
    /// (e.g. `trill`, `staccato`, `crescendo(`).
    pub name: &'static str,
    /// A one-line description of what the decoration means.
    pub doc: &'static str,
    /// The single-char shorthand that is equivalent, if any (e.g. `T` for trill).
    pub shorthand: Option<char>,
}

impl Decoration {
    /// The Markdown documentation body shown in hover / completion detail.
    pub fn markdown(&self) -> String {
        let shorthand = match self.shorthand {
            Some(symbol) => format!(" (shorthand `{symbol}`)"),
            None => String::new(),
        };
        format!(
            "**`!{name}!`**{shorthand} — {doc}\n\nABC 2.1 §4.14 decoration\n\n[ABC 2.1 standard]({url})",
            name = self.name,
            shorthand = shorthand,
            doc = self.doc,
            url = ABC_SPEC_URL,
        )
    }
}

/// The decorations croma recognises and maps to MusicXML, grouped by their
/// `croma_core` source: articulations/ornaments/technical/fermata/arpeggio from
/// `musicxml::notation::decoration_notation`, dynamics + coda/segno + hairpins
/// from `musicxml::direction`, and the single-char shorthands from
/// `parse::music::shorthand_canonical_name`. Names are the canonical spelling the
/// parser stores; the `shorthand` column records the §4.14 single-char form where
/// one exists. (Kept in a single flat list so completion can offer them all.)
pub const DECORATIONS: &[Decoration] = &[
    // Articulations.
    Decoration {
        name: "staccato",
        doc: "Staccato dot — play the note short and detached. Also written `.`",
        shorthand: None,
    },
    Decoration {
        name: "accent",
        doc: "Accent (`>`) — emphasise the note.",
        shorthand: Some('L'),
    },
    Decoration {
        name: "tenuto",
        doc: "Tenuto — hold the note for its full value.",
        shorthand: None,
    },
    Decoration {
        name: "marcato",
        doc: "Marcato (strong accent) — a marked, forceful attack.",
        shorthand: None,
    },
    Decoration {
        name: "wedge",
        doc: "Staccatissimo — very short and detached.",
        shorthand: None,
    },
    Decoration {
        name: "breath",
        doc: "Breath mark — a short break between notes.",
        shorthand: None,
    },
    Decoration {
        name: "slide",
        doc: "Slide (scoop) into the note.",
        shorthand: None,
    },
    // Fermata.
    Decoration {
        name: "fermata",
        doc: "Fermata — hold the note longer than its written value.",
        shorthand: Some('H'),
    },
    Decoration {
        name: "invertedfermata",
        doc: "Inverted fermata — a fermata printed below the note.",
        shorthand: None,
    },
    // Ornaments.
    Decoration {
        name: "trill",
        doc: "Trill — rapidly alternate the note with the one above.",
        shorthand: Some('T'),
    },
    Decoration {
        name: "mordent",
        doc: "Mordent (lower mordent) — a quick alternation with the note below.",
        shorthand: None,
    },
    Decoration {
        name: "lowermordent",
        doc: "Lower mordent — a quick alternation with the note below.",
        shorthand: Some('M'),
    },
    Decoration {
        name: "uppermordent",
        doc: "Upper mordent (pralltriller) — a quick alternation with the note above.",
        shorthand: Some('P'),
    },
    Decoration {
        name: "pralltriller",
        doc: "Pralltriller (upper mordent) — a quick alternation with the note above.",
        shorthand: None,
    },
    Decoration {
        name: "turn",
        doc: "Turn — a four-note ornament around the note (above, note, below, note).",
        shorthand: None,
    },
    Decoration {
        name: "invertedturn",
        doc: "Inverted turn — a turn that begins with the note below.",
        shorthand: None,
    },
    // Technical.
    Decoration {
        name: "upbow",
        doc: "Up-bow — bow the string upward.",
        shorthand: Some('u'),
    },
    Decoration {
        name: "downbow",
        doc: "Down-bow — bow the string downward.",
        shorthand: Some('v'),
    },
    Decoration {
        name: "open",
        doc: "Open string — play on the open string (no finger).",
        shorthand: None,
    },
    Decoration {
        name: "thumb",
        doc: "Thumb position — use the thumb (cello/string technique).",
        shorthand: None,
    },
    Decoration {
        name: "snap",
        doc: "Snap pizzicato — pluck the string so it snaps against the fingerboard.",
        shorthand: None,
    },
    Decoration {
        name: "plus",
        doc: "Left-hand pizzicato / stopped (the `+` glyph).",
        shorthand: None,
    },
    Decoration {
        name: "arpeggio",
        doc: "Arpeggio — roll the notes of a chord.",
        shorthand: None,
    },
    // Fingerings 0–5.
    Decoration {
        name: "0",
        doc: "Fingering 0 (open / thumb).",
        shorthand: None,
    },
    Decoration {
        name: "1",
        doc: "Fingering 1.",
        shorthand: None,
    },
    Decoration {
        name: "2",
        doc: "Fingering 2.",
        shorthand: None,
    },
    Decoration {
        name: "3",
        doc: "Fingering 3.",
        shorthand: None,
    },
    Decoration {
        name: "4",
        doc: "Fingering 4.",
        shorthand: None,
    },
    Decoration {
        name: "5",
        doc: "Fingering 5.",
        shorthand: None,
    },
    // Dynamics.
    Decoration {
        name: "ppp",
        doc: "Dynamic — pianississimo (very, very soft).",
        shorthand: None,
    },
    Decoration {
        name: "pp",
        doc: "Dynamic — pianissimo (very soft).",
        shorthand: None,
    },
    Decoration {
        name: "p",
        doc: "Dynamic — piano (soft).",
        shorthand: None,
    },
    Decoration {
        name: "mp",
        doc: "Dynamic — mezzo-piano (moderately soft).",
        shorthand: None,
    },
    Decoration {
        name: "mf",
        doc: "Dynamic — mezzo-forte (moderately loud).",
        shorthand: None,
    },
    Decoration {
        name: "f",
        doc: "Dynamic — forte (loud).",
        shorthand: None,
    },
    Decoration {
        name: "ff",
        doc: "Dynamic — fortissimo (very loud).",
        shorthand: None,
    },
    Decoration {
        name: "fff",
        doc: "Dynamic — fortississimo (very, very loud).",
        shorthand: None,
    },
    Decoration {
        name: "sfz",
        doc: "Dynamic — sforzando (a sudden strong accent).",
        shorthand: None,
    },
    // Directions.
    Decoration {
        name: "coda",
        doc: "Coda sign — marks a coda section.",
        shorthand: Some('O'),
    },
    Decoration {
        name: "segno",
        doc: "Segno sign — the point a D.S. returns to.",
        shorthand: Some('S'),
    },
    // Hairpins (crescendo / diminuendo wedges).
    Decoration {
        name: "crescendo(",
        doc: "Start of a crescendo hairpin (grow louder). Close with `!crescendo)!`.",
        shorthand: None,
    },
    Decoration {
        name: "crescendo)",
        doc: "End of a crescendo hairpin.",
        shorthand: None,
    },
    Decoration {
        name: "diminuendo(",
        doc: "Start of a diminuendo hairpin (grow softer). Close with `!diminuendo)!`.",
        shorthand: None,
    },
    Decoration {
        name: "diminuendo)",
        doc: "End of a diminuendo hairpin.",
        shorthand: None,
    },
    // The Irish roll: recognised shorthand `~`, normalised to `roll`.
    Decoration {
        name: "roll",
        doc: "Roll (Irish ornament, `~`) — a gracing turn around the note.",
        shorthand: Some('~'),
    },
];

/// Look up a decoration by its canonical name (the text between `!`/`+`).
pub fn decoration(name: &str) -> Option<&'static Decoration> {
    DECORATIONS
        .iter()
        .find(|decoration| decoration.name == name)
}

/// Look up the decoration a recognised single-char shorthand expands to (ABC 2.1
/// §4.14), e.g. `T` → trill, `~` → roll. Mirrors
/// `croma_core::parse::music::shorthand_canonical_name`.
pub fn decoration_for_shorthand(symbol: char) -> Option<&'static Decoration> {
    DECORATIONS
        .iter()
        .find(|decoration| decoration.shorthand == Some(symbol))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_keys_are_unique_and_complete() {
        // Every letter in the ABC 2.1 common-field set is present exactly once.
        let expected = "XTCOAMLQPZNGHKRBDFSIVWwmrs";
        for letter in expected.chars() {
            assert!(field_key(letter).is_some(), "missing field {letter}");
        }
        // No duplicate letters.
        let mut seen = std::collections::HashSet::new();
        for field in FIELD_KEYS {
            assert!(
                seen.insert(field.letter),
                "duplicate field {}",
                field.letter
            );
        }
        assert_eq!(seen.len(), expected.chars().count());
    }

    #[test]
    fn field_insert_and_markdown_are_well_formed() {
        let k = field_key('K').expect("K is a field");
        assert_eq!(k.insert_text(), "K:");
        let md = k.markdown();
        assert!(md.contains("key"), "doc mentions key: {md}");
        assert!(md.contains("§3.1.14"), "doc has the section ref");
    }

    #[test]
    fn decorations_are_grounded_in_core_recognised_names() {
        // Spot-check the high-value decorations the spec calls out.
        for name in [
            "staccato",
            "accent",
            "tenuto",
            "marcato",
            "fermata",
            "trill",
            "mordent",
            "lowermordent",
            "uppermordent",
            "turn",
            "upbow",
            "downbow",
            "open",
            "thumb",
            "snap",
            "arpeggio",
            "slide",
            "roll",
            "coda",
            "segno",
            "crescendo(",
        ] {
            assert!(decoration(name).is_some(), "missing decoration {name}");
        }
        // Dynamics + fingerings.
        for name in ["p", "f", "mf", "ppp", "fff", "sfz", "0", "5"] {
            assert!(decoration(name).is_some(), "missing decoration {name}");
        }
        // No duplicate names.
        let mut seen = std::collections::HashSet::new();
        for d in DECORATIONS {
            assert!(seen.insert(d.name), "duplicate decoration {}", d.name);
        }
    }

    #[test]
    fn shorthand_table_matches_core_canonical_names() {
        // Each shorthand we advertise must resolve to the SAME canonical name
        // croma_core's `parse::music::shorthand_canonical_name` produces (that fn
        // is crate-private, so the mapping is mirrored here; ABC 2.1 §4.14). If
        // the core map ever changes, this fixture is the tripwire.
        let core_map = [
            ('~', "roll"),
            ('H', "fermata"),
            ('L', "accent"),
            ('M', "lowermordent"),
            ('O', "coda"),
            ('P', "uppermordent"),
            ('S', "segno"),
            ('T', "trill"),
            ('u', "upbow"),
            ('v', "downbow"),
        ];
        for (symbol, canonical) in core_map {
            let ours = decoration_for_shorthand(symbol)
                .unwrap_or_else(|| panic!("table has shorthand {symbol}"));
            assert_eq!(ours.name, canonical, "shorthand {symbol} canonical name");
        }
    }

    #[test]
    fn decoration_markdown_notes_the_shorthand() {
        let trill = decoration("trill").expect("trill");
        let md = trill.markdown();
        assert!(
            md.contains("shorthand `T`"),
            "trill md notes shorthand: {md}"
        );
        assert!(md.contains("§4.14"));
    }
}
