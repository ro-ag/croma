//! Hex-encoded UTF-8 text in croma carriers (`text-hex=`, `clef-hex=`,
//! `n-hex=`, `…-hex-<digits>` decoration names).

/// Decode `hex` (two digits per byte, either case) into text. `None` when the
/// digits are malformed, the bytes are not UTF-8, or the text holds a character
/// XML 1.0 cannot carry, so a hand-written carrier can never put one into the
/// MusicXML output. Empty input decodes to empty text.
pub(crate) fn decode_hex_utf8(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut digits = hex.bytes();
    while let (Some(hi), Some(lo)) = (digits.next(), digits.next()) {
        bytes.push((hex_digit(hi)? << 4) | hex_digit(lo)?);
    }
    let text = String::from_utf8(bytes).ok()?;
    text.chars().all(is_xml_char).then_some(text)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The XML 1.0 `Char` production.
fn is_xml_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

#[cfg(test)]
#[path = "hex_tests.rs"]
mod tests;
