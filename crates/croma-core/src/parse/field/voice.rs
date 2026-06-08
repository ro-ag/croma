//! Voice (`V:`) and score (`%%score`/`I:score`) directive field parsing.

use super::misc::{is_escaped, split_first_word, trim_quoted_value_span, trim_value_span};
use super::*;
use crate::diagnostic::Span;

pub(super) fn parse_voice(value: Spanned<String>) -> VoiceDefinition {
    let (id, properties) = split_first_word(value);
    let parsed_properties = parse_voice_properties(&properties);
    VoiceDefinition {
        id,
        properties,
        parsed_properties,
    }
}

pub(crate) fn parse_voice_for_music(value: Spanned<String>) -> VoiceDefinition {
    parse_voice(value)
}

pub(super) fn upsert_voice_definition(
    voices: &mut Vec<Spanned<VoiceDefinition>>,
    voice: Spanned<VoiceDefinition>,
) {
    if let Some(existing) = voices
        .iter_mut()
        .find(|existing| existing.value.id.value == voice.value.id.value)
    {
        *existing = voice;
    } else {
        voices.push(voice);
    }
}

pub(super) fn parse_voice_properties(properties: &Spanned<String>) -> VoiceProperties {
    let mut parsed = VoiceProperties::default();
    for property in voice_property_tokens(&properties.value, properties.span.start) {
        let key_lower = property.key.value.to_ascii_lowercase();
        // A bare clef shorthand carries no `=` — its value is empty and the whole
        // token sits in `key`. `treble+8`, `bass`, `alto`, etc. (optionally with a
        // `±8`/`±15` octave suffix) name a clef just like `clef=treble+8` does, so
        // record it as the clef. ABC 2.1 §4.6 permits the bare form on V: and K:
        // fields alike, and abc2xml accepts it.
        if property.value.value.is_empty()
            && parsed.clef.is_none()
            && is_bare_clef_shorthand(&key_lower)
        {
            parsed.clef = Some(property.key.clone());
            continue;
        }
        match key_lower.as_str() {
            "name" => parsed.name = Some(property.value.clone()),
            "nm" => parsed.nm = Some(property.value.clone()),
            "subname" => parsed.subname = Some(property.value.clone()),
            "snm" | "sname" => parsed.snm = Some(property.value.clone()),
            "clef" => parsed.clef = Some(property.value.clone()),
            "stem" => {
                parsed.stem = match property.value.value.to_ascii_lowercase().as_str() {
                    "up" => Some(Spanned::new(StemDirection::Up, property.value.span)),
                    "down" => Some(Spanned::new(StemDirection::Down, property.value.span)),
                    _ => None,
                };
                if parsed.stem.is_none() {
                    parsed.other.push(property);
                }
            }
            "octave" | "oct" => parsed.octave = Some(property.value.clone()),
            "middle" | "m" => parsed.middle = Some(property.value.clone()),
            "transpose" | "transposition" | "score" | "sound" | "shift" => {
                parsed.transpose = Some(property.value.clone());
            }
            _ => parsed.other.push(property),
        }
    }
    parsed
}

/// Whether a bare (no `=`) token names a clef shorthand, e.g. `treble`, `bass`,
/// `alto`, `tenor`, `perc`, or `none`, optionally with a `±8`/`±15` octave
/// suffix (`treble+8`, `bass-15`). The input is already lowercased.
fn is_bare_clef_shorthand(token: &str) -> bool {
    const CLEF_NAMES: [&str; 6] = ["treble", "alto", "tenor", "bass", "perc", "none"];
    CLEF_NAMES.iter().any(|name| token.starts_with(name))
}

fn voice_property_tokens(value: &str, offset: usize) -> Vec<VoiceProperty> {
    let mut properties = Vec::new();
    let mut index = 0;
    while index < value.len() {
        while value[index..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            let Some(ch) = value[index..].chars().next() else {
                break;
            };
            index += ch.len_utf8();
            if index >= value.len() {
                break;
            }
        }
        if index >= value.len() {
            break;
        }

        let start = index;
        let mut in_quote = false;
        while index < value.len() {
            let Some(ch) = value[index..].chars().next() else {
                break;
            };
            if ch == '"' && !is_escaped(value, index) {
                in_quote = !in_quote;
            } else if ch.is_whitespace() && !in_quote {
                break;
            }
            index += ch.len_utf8();
        }

        let token = &value[start..index];
        let span = Span::new(offset + start, offset + index);
        if let Some(eq_offset) = token.find('=') {
            let key = trim_value_span(&token[..eq_offset], offset + start);
            let value_start = start + eq_offset + 1;
            let raw_value = &value[value_start..index];
            let parsed_value = trim_quoted_value_span(raw_value, offset + value_start);
            properties.push(VoiceProperty {
                key,
                value: parsed_value,
                span,
            });
        } else {
            let key = trim_value_span(token, offset + start);
            properties.push(VoiceProperty {
                key,
                value: Spanned::new(String::new(), Span::new(span.end, span.end)),
                span,
            });
        }
    }
    properties
}

pub(crate) fn parse_score_directive(value: Spanned<String>) -> ScoreDirective {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < value.value.len() {
        let Some(ch) = value.value[index..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }

        let start = index;
        let kind = match ch {
            '(' | '[' | '{' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::GroupStart(ch)
            }
            ')' | ']' | '}' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::GroupEnd(ch)
            }
            '|' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::StaffSeparator
            }
            ',' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::MeasureSeparator
            }
            '*' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::FloatingVoiceMarker
            }
            _ => {
                while index < value.value.len() {
                    let Some(ch) = value.value[index..].chars().next() else {
                        break;
                    };
                    if ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | '|') {
                        break;
                    }
                    index += ch.len_utf8();
                }
                ScoreDirectiveTokenKind::Voice(value.value[start..index].to_owned())
            }
        };
        tokens.push(ScoreDirectiveToken {
            span: Span::new(value.span.start + start, value.span.start + index),
            kind,
        });
    }
    ScoreDirective { value, tokens }
}
