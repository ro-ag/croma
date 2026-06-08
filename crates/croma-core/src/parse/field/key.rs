//! Key-signature (`K:`) field parsing.

use super::misc::tokens_with_spans;
use super::voice::parse_voice_properties;
use super::*;
use crate::diagnostic::Span;

pub(crate) fn parse_key(value: &str, value_span: Span) -> KeySignature {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return KeySignature {
            raw: trimmed.to_owned(),
            tonic: None,
            mode: KeyMode::None,
            accidentals: Vec::new(),
            explicit: false,
            properties: VoiceProperties::default(),
        };
    }

    if trimmed == "HP" || trimmed == "Hp" {
        return KeySignature {
            raw: trimmed.to_owned(),
            tonic: None,
            mode: if trimmed == "HP" {
                KeyMode::HighlandPipes
            } else {
                KeyMode::HighlandPipesMarked
            },
            accidentals: Vec::new(),
            explicit: false,
            properties: VoiceProperties::default(),
        };
    }

    let tokens = tokens_with_spans(trimmed, value_span.start);
    let mut tonic = None;
    let mut mode = KeyMode::Major;
    let mut explicit = false;
    let mut token_start = 0;

    if let Some(first) = tokens.first()
        && let Some((parsed_tonic, inline_mode)) = parse_tonic_token(&first.value)
    {
        tonic = Some(parsed_tonic);
        if let Some(inline_mode) = inline_mode {
            mode = inline_mode;
            explicit = mode == KeyMode::Explicit;
        }
        token_start = 1;
    }

    if let Some(token) = tokens.get(token_start)
        && let Some(parsed_mode) = parse_key_mode(&token.value)
    {
        explicit = parsed_mode == KeyMode::Explicit;
        mode = parsed_mode;
        token_start += 1;
    }

    let accidentals = tokens[token_start..]
        .iter()
        .filter_map(parse_key_accidental)
        .collect();

    // ABC 2.1 §4.6: a K: field may also carry clef/octave/middle/transpose
    // modifiers (`K:C treble+8`, `K: Dm octave=1`). The tonic/mode tokens are
    // already consumed, so parse the remainder with the voice-property parser.
    // Accidental tokens (`^f`, `_B`) carry no `=` and match no clef shorthand,
    // so they fall harmlessly into `other`.
    let properties = if let Some(first_remaining) = tokens.get(token_start) {
        let remainder_start = first_remaining.span.start;
        let remainder_offset = remainder_start.saturating_sub(value_span.start);
        let remainder = trimmed.get(remainder_offset..).unwrap_or("");
        parse_voice_properties(&Spanned::new(remainder.to_owned(), {
            Span::new(remainder_start, value_span.end)
        }))
    } else {
        VoiceProperties::default()
    };

    KeySignature {
        raw: trimmed.to_owned(),
        tonic,
        mode,
        accidentals,
        explicit,
        properties,
    }
}

fn parse_tonic_token(token: &str) -> Option<(KeyTonic, Option<KeyMode>)> {
    let mut chars = token.char_indices();
    let (_, first) = chars.next()?;
    if !matches!(first.to_ascii_uppercase(), 'A'..='G') {
        return None;
    }

    let mut accidental = None;
    let mut mode_start = first.len_utf8();
    if let Some((offset, ch)) = chars.next() {
        match ch {
            '#' => {
                accidental = Some(KeyTonicAccidental::Sharp);
                mode_start = offset + ch.len_utf8();
            }
            'b' => {
                accidental = Some(KeyTonicAccidental::Flat);
                mode_start = offset + ch.len_utf8();
            }
            _ => {
                mode_start = offset;
            }
        }
    }

    let mode = if mode_start < token.len() {
        // Any text after the tonic letter (and optional accidental) must be a
        // recognised mode suffix, e.g. `Cmaj`, `Ador`. Otherwise the token is
        // not a key tonic at all — for instance a clef shorthand (`bass`,
        // `alto`) or a property token (`clef=bass`) that merely happens to start
        // with a note letter must not be misread as a key change.
        match parse_key_mode(&token[mode_start..]) {
            Some(mode) => Some(mode),
            None => return None,
        }
    } else {
        None
    };

    Some((
        KeyTonic {
            step: first.to_ascii_uppercase(),
            accidental,
        },
        mode,
    ))
}

fn parse_key_mode(value: &str) -> Option<KeyMode> {
    let lower = value.to_ascii_lowercase();
    if lower == "m" {
        return Some(KeyMode::Minor);
    }

    let prefix = &lower[..lower.len().min(3)];
    match prefix {
        "maj" => Some(KeyMode::Major),
        "ion" => Some(KeyMode::Ionian),
        "min" => Some(KeyMode::Minor),
        "aeo" => Some(KeyMode::Aeolian),
        "mix" => Some(KeyMode::Mixolydian),
        "dor" => Some(KeyMode::Dorian),
        "phr" => Some(KeyMode::Phrygian),
        "lyd" => Some(KeyMode::Lydian),
        "loc" => Some(KeyMode::Locrian),
        "exp" => Some(KeyMode::Explicit),
        _ => None,
    }
}

fn parse_key_accidental(token: &Spanned<String>) -> Option<KeyAccidental> {
    let value = token.value.as_str();
    let (sign, sign_len) = if value.starts_with("__") {
        (AccidentalSign::DoubleFlat, 2)
    } else if value.starts_with("^^") {
        (AccidentalSign::DoubleSharp, 2)
    } else if value.starts_with('_') {
        (AccidentalSign::Flat, 1)
    } else if value.starts_with('^') {
        (AccidentalSign::Sharp, 1)
    } else if value.starts_with('=') {
        (AccidentalSign::Natural, 1)
    } else {
        return None;
    };
    let note = value[sign_len..].chars().next()?;
    if !matches!(note.to_ascii_uppercase(), 'A'..='G') {
        return None;
    }
    let note_span = Span::new(
        token.span.start + sign_len,
        token.span.start + sign_len + note.len_utf8(),
    );
    Some(KeyAccidental {
        sign,
        note: Spanned::new(note, note_span),
        span: Span::new(token.span.start, note_span.end),
    })
}
