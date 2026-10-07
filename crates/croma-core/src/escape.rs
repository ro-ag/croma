//! Backslash escapes inside double-quoted ABC values (field values such as
//! `name="…"`, `Q:"…"` tempo text, and croma carrier `key="…"` values).

/// Undo the escaping croma's ABC writer applies to quoted text: `\\` is a
/// backslash and `\"` a double quote. Any other backslash sequence, and a
/// trailing backslash, is kept as written, so ABC 2.1 §8.2 text-string
/// mnemonics such as `\'e` reach the output intact rather than losing their
/// backslash.
pub(crate) fn unescape_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some(escaped @ ('"' | '\\')) => out.push(escaped),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
#[path = "escape_tests.rs"]
mod tests;
