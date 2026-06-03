use crate::error::{CromaError, Result};
use crate::model::{Event, Fraction, Tune};
use crate::options::ExportOptions;
use crate::surface::SurfaceMap;

pub fn parse_tune(source: &str, _surface: &SurfaceMap, _options: ExportOptions) -> Result<Tune> {
    if source.trim().is_empty() {
        return Err(CromaError::EmptyInput);
    }

    let mut reference = String::new();
    let mut title = String::new();
    let mut meter = String::from("4/4");
    let mut unit = Fraction::new(1, 8);
    let mut key = String::new();
    let mut in_body = false;
    let mut events = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('%') {
            continue;
        }
        if !in_body {
            if let Some(value) = trimmed.strip_prefix("X:") {
                reference = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("T:") {
                title = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("M:") {
                meter = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("L:") {
                if let Some(parsed) = Fraction::parse(value.trim()) {
                    unit = parsed;
                }
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("K:") {
                key = value.trim().to_owned();
                in_body = true;
                continue;
            }
        }

        if in_body {
            parse_music_line(trimmed, unit, &mut events);
        }
    }

    if key.is_empty() {
        return Err(CromaError::MissingKey);
    }
    if events.iter().all(|event| matches!(event, Event::Bar)) {
        return Err(CromaError::NoMusic);
    }

    Ok(Tune {
        reference,
        title,
        meter,
        key,
        divisions: 8,
        events,
    })
}

fn parse_music_line(line: &str, unit: Fraction, events: &mut Vec<Event>) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '%' => break,
            '|' => {
                events.push(Event::Bar);
                index += 1;
            }
            'A'..='G' | 'a'..='g' => {
                let step = ch.to_ascii_uppercase();
                let octave = if ch.is_ascii_lowercase() { 5 } else { 4 };
                index += 1;
                let (length, next) = parse_length(&chars, index);
                events.push(Event::Note {
                    step,
                    octave,
                    duration: duration_divisions(unit, length),
                });
                index = next;
            }
            'z' | 'x' => {
                index += 1;
                let (length, next) = parse_length(&chars, index);
                events.push(Event::Rest {
                    duration: duration_divisions(unit, length),
                });
                index = next;
            }
            _ => {
                index += 1;
            }
        }
    }
}

fn parse_length(chars: &[char], start: usize) -> (Fraction, usize) {
    let mut index = start;
    let mut numerator = String::new();
    while index < chars.len() && chars[index].is_ascii_digit() {
        numerator.push(chars[index]);
        index += 1;
    }

    if index < chars.len() && chars[index] == '/' {
        index += 1;
        let mut denominator = String::new();
        while index < chars.len() && chars[index].is_ascii_digit() {
            denominator.push(chars[index]);
            index += 1;
        }
        let numerator = parse_u32_or_one(&numerator);
        let denominator = parse_u32_or_default(&denominator, 2);
        return (Fraction::new(numerator, denominator), index);
    }

    (Fraction::new(parse_u32_or_one(&numerator), 1), index)
}

fn parse_u32_or_one(value: &str) -> u32 {
    parse_u32_or_default(value, 1)
}

fn parse_u32_or_default(value: &str, default: u32) -> u32 {
    value.parse::<u32>().unwrap_or(default)
}

fn duration_divisions(unit: Fraction, length: Fraction) -> u32 {
    let numerator = 32 * unit.numerator * length.numerator;
    let denominator = unit.denominator * length.denominator;
    numerator
        .checked_div(denominator)
        .filter(|v| *v > 0)
        .unwrap_or(1)
}
