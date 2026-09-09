//! Compact carrier spelling.
//!
//! croma's round-trip carriers (`docs/carriers.md`) have two spellings: the
//! original `[I:croma-<long-name> …]` and the compact `[I:cr <code> …]` the
//! writer emits today. Compact is a SPELLING, not a second semantics: this
//! module rewrites a compact carrier into the exact long-form string the
//! existing `parse_*_instruction` functions already understand, so there is one
//! parser per carrier and the two spellings cannot drift apart.

/// The closed registry: (compact code, long-form name). Only the frequent
/// carriers are coded; the remaining 15 keep their long spelling in both
/// directions because recoding them saves ~639 B across a 60-file corpus
/// sample.
pub(crate) const COMPACT_CARRIERS: [(&str, &str); 8] = [
    ("dp", "croma-direction-placement"),
    ("htx", "croma-harmony-text"),
    ("ht", "croma-harmony-text"),
    ("le", "croma-lyric-extend"),
    ("mr", "croma-meter-restatement"),
    ("kr", "croma-key-restatement"),
    ("ec", "croma-ending-close"),
    ("mf", "croma-musicxml-forward"),
];

/// Rewrite `cr <code> [fields]` into its long-form equivalent, or `None` when
/// the value is not a registered compact carrier (an unknown code, a bare `cr`,
/// or any other `[I:…]` content). `None` means "not mine" — the caller falls
/// through to the long-form parsers and then to the unknown-instruction path,
/// which is what keeps a foreign `[I:cr …]` from being read as croma state.
pub(crate) fn expand_compact_carrier(value: &str) -> Option<String> {
    let rest = value.trim().strip_prefix("cr")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let (code, fields) = match rest.find(char::is_whitespace) {
        Some(index) => (&rest[..index], rest[index..].trim_start()),
        None => (rest, ""),
    };
    // `code` may itself carry the value (`dp=a`, `le=3`); split it off first.
    let (code, inline_value) = match code.split_once('=') {
        Some((code, value)) => (code, Some(value)),
        None => (code, None),
    };
    match (code, inline_value) {
        ("dp", Some(value)) => {
            let placement = match value {
                "a" => "above",
                "b" => "below",
                _ => return None,
            };
            Some(format!("croma-direction-placement placement={placement}"))
        }
        ("le", Some(verse)) => Some(format!("croma-lyric-extend verse={verse}")),
        ("htx", None) => Some("croma-harmony-text textless=1".to_owned()),
        ("ht", None) if !fields.is_empty() => Some(format!("croma-harmony-text {fields}")),
        ("mr", None) => Some("croma-meter-restatement".to_owned()),
        ("kr", None) => Some("croma-key-restatement".to_owned()),
        ("mf", None) => Some("croma-musicxml-forward".to_owned()),
        ("ec", None) => Some(format!(
            "croma-ending-close {}",
            expand_ending_close(fields)?
        )),
        _ => None,
    }
}

/// The registry entry whose LONG name this `[I:…]` value uses, as
/// `(long name, compact code)`. Used to warn that a coded carrier was written
/// in its deprecated long spelling.
///
/// `croma-harmony-text` has two codes (`ht` and `htx`) for the same long
/// name; a naive first match always reports `htx` because it is listed
/// first. Report `htx` only when the value carries `textless`, and `ht`
/// otherwise, so the suggested compact spelling matches what the value
/// actually expands to.
pub(crate) fn compact_code_for_long(value: &str) -> Option<(&'static str, &'static str)> {
    let value = value.trim();
    let (matched_code, long) = COMPACT_CARRIERS
        .iter()
        .copied()
        .find(|(_, long)| matches_long_carrier(value, long))?;
    let code = if long == "croma-harmony-text" {
        if value.contains("textless") {
            "htx"
        } else {
            "ht"
        }
    } else {
        matched_code
    };
    Some((long, code))
}

/// Whether `value` (the full `[I:…]` payload) is an occurrence of the given
/// long-form carrier name: the name followed by either end of string or
/// whitespace, so `croma-harmony-text` does not falsely match some longer
/// unrelated name that happens to share the prefix.
fn matches_long_carrier(value: &str, long: &str) -> bool {
    value
        .strip_prefix(long)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// `t=s l=r n="1"` -> `type=stop location=right number="1"`. The `n=` label is
/// payload (it can hold `1-2` ranges or comma lists), so it is passed through
/// with its key renamed and its value untouched. Unlike other carriers'
/// free-text fields, `number` has no `-hex=` variant: the writer
/// (`ending_number_value` in `to_abc.rs`) only ever builds it from digits,
/// `-`, and `,`, which `needs_hex_inline_carrier` never flags, so a `n-hex=`
/// field cannot come from croma's own writer. A hand-written one falls
/// through to `None` here and is handled by the caller's unknown-instruction
/// path, same as any other unrecognised field.
fn expand_ending_close(fields: &str) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for field in split_fields(fields) {
        let (key, value) = field.split_once('=')?;
        let expanded = match (key, value) {
            ("t", "s") => "type=stop".to_owned(),
            ("t", "d") => "type=discontinue".to_owned(),
            ("l", "l") => "location=left".to_owned(),
            ("l", "r") => "location=right".to_owned(),
            ("n", value) => format!("number={value}"),
            _ => return None,
        };
        out.push(expanded);
    }
    (!out.is_empty()).then(|| out.join(" "))
}

/// Split a field list on whitespace that is OUTSIDE double quotes, so a quoted
/// value keeps its spaces (`n="1, 2"` is one field).
fn split_fields(fields: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in fields.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => {
                if let Some(begin) = start.take() {
                    out.push(&fields[begin..index]);
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(index);
                }
            }
        }
    }
    if let Some(begin) = start {
        out.push(&fields[begin..]);
    }
    out
}

#[cfg(test)]
#[path = "carrier_tests.rs"]
mod tests;
