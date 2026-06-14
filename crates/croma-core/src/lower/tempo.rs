//! Tempo (`Q:`) field parsing into the semantic tempo model.

use crate::diagnostic::Span;
use crate::model::{Fraction, TempoBeat, TempoModel};

/// Parse an ABC `Q:` tempo field (ABC 2.1 §3.1.8) into a structured model.
///
/// Recognised forms:
/// - `beat=bpm` (e.g. `1/4=120`) — explicit beat unit and bpm.
/// - `bpm` (a bare number, e.g. `120`) — the unit note length `L:` is the beat.
/// - `"text"` with any of the above, in any order — the quoted text is kept.
/// - `"text"` alone — no numeric tempo (writer falls back to words).
///
/// Multiple summed beat fractions (`1/4 3/8=40`) are summed into a single beat
/// fraction, matching abc2xml. Returns `None` when the field carries no numeric
/// tempo and is not a clean single quoted string, so the writer keeps emitting
/// the raw field text verbatim as words (preserving prior behavior for free
/// text such as `Q:Fast`).
pub(crate) fn parse_tempo_model(
    raw: &str,
    span: Span,
    unit_note_length: Fraction,
) -> Option<TempoModel> {
    // Extract the first quoted string as the tempo text; tempo numerics live in
    // the unquoted remainder (in any position relative to the text).
    let (text, remainder) = extract_quoted_text(raw);
    let beat = parse_tempo_beat(&remainder, unit_note_length);
    if beat.is_none() {
        // No numeric tempo: only treat the field as a structured tempo when it
        // is exactly one quoted string (`Q:"Andante"`). Anything else stays raw
        // words via the writer's fallback.
        let is_clean_quoted = matches!(raw.trim().as_bytes(), [b'"', .., b'"'])
            && raw.trim().len() >= 2
            && text.is_some();
        if !is_clean_quoted {
            return None;
        }
    }
    Some(TempoModel {
        text,
        beat,
        source_span: span,
    })
}

/// Split out the first double-quoted run as tempo text, returning the text and
/// the source with that quoted run removed.
fn extract_quoted_text(raw: &str) -> (Option<String>, String) {
    let Some(open) = raw.find('"') else {
        return (None, raw.to_owned());
    };
    let rest = &raw[open + 1..];
    let Some(close_rel) = rest.find('"') else {
        // Unterminated quote: treat the remainder as text, leave nothing numeric.
        return (Some(rest.trim().to_owned()), raw[..open].to_owned());
    };
    let text = rest[..close_rel].trim().to_owned();
    let mut remainder = raw[..open].to_owned();
    remainder.push(' ');
    remainder.push_str(&rest[close_rel + 1..]);
    let text = (!text.is_empty()).then_some(text);
    (text, remainder)
}

/// Parse the numeric portion of a `Q:` field into a [`TempoBeat`].
fn parse_tempo_beat(remainder: &str, unit_note_length: Fraction) -> Option<TempoBeat> {
    let trimmed = remainder.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((beats, bpm_part)) = trimmed.split_once('=') {
        // `beat[ beat...]=bpm`. `C`/`C=bpm` means the unit note length.
        let bpm = parse_u32(bpm_part)?;
        let (mut num, mut den) = (0u32, 1u32);
        let mut saw_fraction = false;
        for token in beats.split_whitespace() {
            if token.eq_ignore_ascii_case("C") {
                let (a, b) = add_fractions(
                    num,
                    den,
                    unit_note_length.numerator,
                    unit_note_length.denominator,
                );
                num = a;
                den = b;
                saw_fraction = true;
                continue;
            }
            let (fn_num, fn_den) = parse_fraction(token)?;
            let (a, b) = add_fractions(num, den, fn_num, fn_den);
            num = a;
            den = b;
            saw_fraction = true;
        }
        if !saw_fraction {
            return None;
        }
        let (num, den) = reduce_fraction(num, den);
        Some(TempoBeat {
            beat_numerator: num,
            beat_denominator: den,
            bpm,
        })
    } else {
        // Bare number: the unit note length is the beat unit.
        let bpm = parse_bare_tempo_bpm(trimmed)?;
        Some(TempoBeat {
            beat_numerator: unit_note_length.numerator,
            beat_denominator: unit_note_length.denominator,
            bpm,
        })
    }
}

fn parse_u32(text: &str) -> Option<u32> {
    let value = text.trim().parse::<u32>().ok()?;
    (value > 0).then_some(value)
}

/// Parse the bare-number tempo form (ABC 2.1 §10.1, deprecated `Q:120`): the
/// leading integer is the bpm. Tolerates a trailing decimal tail (`400.`,
/// `400.0`, `400.5`) and legacy abc2mtex suffix letters (`320s`), matching
/// abc2xml's lenient acceptance. Rejects fields with internal whitespace or no
/// leading digit (free text such as `Q:Fast` or `Q:3 dancers`) so they keep
/// falling through to verbatim words.
fn parse_bare_tempo_bpm(trimmed: &str) -> Option<u32> {
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return None;
    }
    let digit_len = trimmed
        .chars()
        .take_while(char::is_ascii_digit)
        .map(char::len_utf8)
        .sum::<usize>();
    if digit_len == 0 {
        return None;
    }
    // The remainder after the leading digits must be a benign suffix: a decimal
    // tail (`.`/`.123`) or purely-alphabetic legacy chars. Anything else (an
    // operator or a disjoint number) is not a bare tempo.
    let rest = &trimmed[digit_len..];
    let benign = rest.is_empty()
        || rest.chars().all(|c| c == '.' || c.is_ascii_digit())
        || rest.chars().all(|c| c.is_ascii_alphabetic());
    if !benign {
        return None;
    }
    let value = trimmed[..digit_len].parse::<u32>().ok()?;
    (value > 0).then_some(value)
}

fn parse_fraction(token: &str) -> Option<(u32, u32)> {
    match token.split_once('/') {
        Some((num, den)) => {
            let num = num.trim().parse::<u32>().ok()?;
            let den = den.trim().parse::<u32>().ok()?;
            (num > 0 && den > 0).then_some((num, den))
        }
        None => {
            let num = token.trim().parse::<u32>().ok()?;
            (num > 0).then_some((num, 1))
        }
    }
}

fn add_fractions(a_num: u32, a_den: u32, b_num: u32, b_den: u32) -> (u32, u32) {
    if a_den == 0 {
        return (b_num, b_den);
    }
    let num = a_num * b_den + b_num * a_den;
    let den = a_den * b_den;
    reduce_fraction(num, den)
}

fn reduce_fraction(num: u32, den: u32) -> (u32, u32) {
    if num == 0 || den == 0 {
        return (num, den.max(1));
    }
    let divisor = gcd(num, den);
    (num / divisor, den / divisor)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}
