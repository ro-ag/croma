use super::{MeasureSequence, MusicXmlWriter};
use crate::model::TextAttachment;

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_chord_symbol(
        &mut self,
        symbol: &TextAttachment,
        sequence: &MeasureSequence<'score>,
    ) {
        self.write_chord_symbol_text(
            &symbol.text,
            symbol.musicxml_harmony_text.as_deref(),
            sequence,
        );
    }

    pub(crate) fn write_plain_chord_symbol(
        &mut self,
        text: &str,
        sequence: &MeasureSequence<'score>,
    ) {
        self.write_chord_symbol_text(text, None, sequence);
    }

    fn write_chord_symbol_text(
        &mut self,
        text: &str,
        musicxml_harmony_text: Option<&str>,
        sequence: &MeasureSequence<'score>,
    ) {
        if self.write_harmony(text, musicxml_harmony_text) {
            return;
        }
        let words = text.trim();
        if !words.is_empty() {
            self.write_direction_words(
                words,
                None,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
    }

    pub(crate) fn write_harmony(
        &mut self,
        text: &str,
        musicxml_harmony_text: Option<&str>,
    ) -> bool {
        let Some(chord) = parse_chord_symbol(text) else {
            return false;
        };
        self.xml.start("harmony", &[]);
        self.xml.start("root", &[]);
        self.xml
            .text_element("root-step", &chord.root_step.to_string());
        if chord.root_alter != 0 {
            self.xml
                .text_element("root-alter", &chord.root_alter.to_string());
        }
        self.xml.end("root");
        if let Some("") = musicxml_harmony_text {
            self.xml.text_element("kind", chord.kind);
        } else {
            self.xml.text_element_attrs(
                "kind",
                &[("text", musicxml_harmony_text.unwrap_or(text))],
                chord.kind,
            );
        }
        if let Some(bass_step) = chord.bass_step {
            self.xml.start("bass", &[]);
            self.xml.text_element("bass-step", &bass_step.to_string());
            if chord.bass_alter != 0 {
                self.xml
                    .text_element("bass-alter", &chord.bass_alter.to_string());
            }
            self.xml.end("bass");
        }
        // Trailing chord degrees are emitted as added degrees, mirroring
        // abc2xml (which only ever produces `degree-type = add`).
        for degree in &chord.degrees {
            self.xml.start("degree", &[]);
            self.xml
                .text_element("degree-value", &degree.value.to_string());
            self.xml
                .text_element("degree-alter", &degree.alter.to_string());
            self.xml.text_element("degree-type", "add");
            self.xml.end("degree");
        }
        self.xml.end("harmony");
        true
    }
}

#[derive(Debug, Clone)]
struct ParsedChordSymbol {
    root_step: char,
    root_alter: i8,
    bass_step: Option<char>,
    bass_alter: i8,
    kind: &'static str,
    degrees: Vec<ChordDegree>,
}

#[derive(Debug, Clone, Copy)]
struct ChordDegree {
    value: u8,
    alter: i8,
}

/// Chord-quality token table, mirroring abc2xml's `compChordTab`
/// (`docs/.../abc2xml.py`). Ordered longest-token-first so a greedy prefix
/// match consumes the maximal quality (e.g. `maj7` before `m`/`ma`). The first
/// matched token determines `<kind>`; this is the closed MusicXML kind enum
/// that ABC commonly uses. Anything not listed falls back to `major`, matching
/// abc2xml's `chordTab.get(token, 'major')`.
const CHORD_QUALITY_TABLE: &[(&str, &str)] = &[
    // Seventh chords (longest first).
    ("maj7", "major-seventh"),
    ("Maj7", "major-seventh"),
    ("min7", "minor-seventh"),
    ("dim7", "diminished-seventh"),
    ("aug7", "augmented-seventh"),
    ("mi7b5", "half-diminished"),
    ("m7b5", "half-diminished"),
    ("ma7", "major-seventh"),
    ("M7", "major-seventh"),
    ("mi7", "minor-seventh"),
    ("m7", "minor-seventh"),
    ("o7", "diminished-seventh"),
    ("-7", "minor-seventh"),
    ("+7", "augmented-seventh"),
    ("7", "dominant"),
    // Sixth chords.
    ("min6", "minor-sixth"),
    ("ma6", "major-sixth"),
    ("M6", "major-sixth"),
    ("mi6", "minor-sixth"),
    ("m6", "minor-sixth"),
    ("6", "major-sixth"),
    // Ninth chords.
    ("maj9", "major-ninth"),
    ("Maj9", "major-ninth"),
    ("min9", "minor-ninth"),
    ("ma9", "major-ninth"),
    ("M9", "major-ninth"),
    ("mi9", "minor-ninth"),
    ("m9", "minor-ninth"),
    ("9", "dominant-ninth"),
    // Eleventh chords.
    ("maj11", "major-11th"),
    ("Maj11", "major-11th"),
    ("min11", "minor-11th"),
    ("ma11", "major-11th"),
    ("M11", "major-11th"),
    ("mi11", "minor-11th"),
    ("m11", "minor-11th"),
    ("11", "dominant-11th"),
    // Thirteenth chords.
    ("maj13", "major-13th"),
    ("Maj13", "major-13th"),
    ("min13", "minor-13th"),
    ("ma13", "major-13th"),
    ("M13", "major-13th"),
    ("mi13", "minor-13th"),
    ("m13", "minor-13th"),
    ("13", "dominant-13th"),
    // Triads (must come after the extended qualities above).
    ("maj", "major"),
    ("Maj", "major"),
    ("aug", "augmented"),
    ("dim", "diminished"),
    ("min", "minor"),
    ("ma", "major"),
    ("mi", "minor"),
    ("M", "major"),
    ("m", "minor"),
    ("o", "diminished"),
    ("+", "augmented"),
    ("-", "minor"),
];

/// Suspended-quality tokens. abc2xml parses an optional suspended token *after*
/// the main quality but keeps only the first kind token for `<kind>`; when a
/// suspended token stands alone it determines the kind. Ordered longest-first.
const SUSPENDED_TABLE: &[(&str, &str)] = &[
    ("sus4", "suspended-fourth"),
    ("sus2", "suspended-second"),
    ("sus", "suspended-fourth"),
];

/// Structural chord-symbol parser following the ABC 2.1 §4.18 grammar as
/// implemented by abc2xml (the comparison baseline):
///
/// ```text
/// chordsym = root accidental? quality? suspended? degree* ("/" bass)?
/// ```
///
/// Returns `None` (so the symbol is emitted as plain `<words>`) when any part
/// fails to parse or unconsumed text remains, exactly mirroring abc2xml's
/// pyparsing behaviour (e.g. `Cadd9`, `Cbb`, `NC` are not harmony). A trailing
/// parenthesised group is suppressed.
fn parse_chord_symbol(text: &str) -> Option<ParsedChordSymbol> {
    let trimmed = text.trim();
    // A trailing parenthesised group is dropped by abc2xml (`C(no3)` -> major).
    let core = match trimmed.find('(') {
        Some(open) if trimmed.ends_with(')') => trimmed[..open].trim_end(),
        _ => trimmed,
    };

    // ABC 2.1 §4.18: the chord ROOT is "one of the letters A-G"; only the
    // bass note is granted case-insensitivity ("any letter (A-G or a-g)").
    // A lowercase first letter ("d", "play") is not a chord symbol — it
    // falls through to a <words> direction, matching the spec (and the
    // baseline, which demotes such strings to text).
    if !core.starts_with(|ch: char| ch.is_ascii_uppercase()) {
        return None;
    }
    let (root, rest) = parse_chord_tone(core)?;
    // The bass `/X` is split off the tail first; the rest before it is the
    // quality + degrees.
    let (quality_part, bass) = match rest.split_once('/') {
        Some((head, bass_text)) => {
            let (bass_tone, bass_rest) = parse_chord_tone(bass_text)?;
            if !bass_rest.is_empty() {
                return None;
            }
            (head, Some(bass_tone))
        }
        None => (rest, None),
    };

    let quality = quality_part.trim();
    // Optional quality token (greedy longest prefix), then optional suspended
    // token. The kind comes from the first matched token; abc2xml drops the
    // suspended token from <kind> when a quality precedes it.
    let mut remaining = quality;
    let mut kind = "major";
    let mut matched_any = false;
    if let Some((token, mapped)) = match_prefix(remaining, CHORD_QUALITY_TABLE) {
        kind = mapped;
        matched_any = true;
        remaining = &remaining[token.len()..];
    }
    if let Some((token, mapped)) = match_prefix(remaining, SUSPENDED_TABLE) {
        if !matched_any {
            kind = mapped;
        }
        remaining = &remaining[token.len()..];
    }

    // Zero or more trailing chord degrees: `[#=b]?(2|4|5|6|7|9|11|13)`.
    let mut degrees = Vec::new();
    loop {
        let trimmed_remaining = remaining.trim_start();
        match parse_chord_degree(trimmed_remaining) {
            Some((degree, after)) => {
                degrees.push(degree);
                remaining = after;
            }
            None => {
                remaining = trimmed_remaining;
                break;
            }
        }
    }

    if !remaining.is_empty() {
        // Unconsumed text means this is not a recognised chord symbol.
        return None;
    }

    Some(ParsedChordSymbol {
        root_step: root.step,
        root_alter: root.alter,
        bass_step: bass.map(|tone| tone.step),
        bass_alter: bass.map(|tone| tone.alter).unwrap_or(0),
        kind,
        degrees,
    })
}

/// Returns the matching `(token, mapped_value)` for the longest table entry
/// that is a prefix of `text`, or `None`.
fn match_prefix(
    text: &str,
    table: &[(&'static str, &'static str)],
) -> Option<(&'static str, &'static str)> {
    table
        .iter()
        .find(|(token, _)| text.starts_with(token))
        .map(|(token, mapped)| (*token, *mapped))
}

/// Parses one chord degree `[#=b]?(2|4|5|6|7|9|11|13)` from the start of `text`,
/// returning the degree and the unconsumed tail.
fn parse_chord_degree(text: &str) -> Option<(ChordDegree, &str)> {
    let (alter, after_accidental) = match text.as_bytes().first() {
        Some(b'#') => (1, &text[1..]),
        Some(b'b') => (-1, &text[1..]),
        Some(b'=') => (0, &text[1..]),
        _ => (0, text),
    };
    // Match the two-digit degrees before the single-digit ones.
    for value in [13u8, 11, 9, 7, 6, 5, 4, 2] {
        let token = value.to_string();
        if after_accidental.starts_with(&token) {
            return Some((
                ChordDegree { value, alter },
                &after_accidental[token.len()..],
            ));
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
struct ChordTone {
    step: char,
    alter: i8,
}

/// Parses a chord root/bass tone `[A-G][#b]?` from the start of `text`,
/// returning the tone and the unconsumed tail. Only a single accidental is
/// accepted, matching abc2xml (which rejects `Cbb`/`C##`).
fn parse_chord_tone(text: &str) -> Option<(ChordTone, &str)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    let step = first.to_ascii_uppercase();
    if !matches!(step, 'A'..='G') {
        return None;
    }
    let mut consumed = first.len_utf8();
    // Only a single `#`/`b`/`=` accidental, like abc2xml's `chord_accidental`.
    // A `-` is the minor-quality token, never a root/bass flat.
    let alter = match text[consumed..].chars().next() {
        Some('#') => {
            consumed += 1;
            1
        }
        Some('b') => {
            consumed += 1;
            -1
        }
        Some('=') => {
            consumed += 1;
            0
        }
        _ => 0,
    };
    Some((ChordTone { step, alter }, &text[consumed..]))
}
