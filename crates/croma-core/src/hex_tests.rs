//! Carrier hex decoding tests.

use super::decode_hex_utf8;

#[test]
fn decodes_utf8_in_either_case() {
    assert_eq!(decode_hex_utf8("4142").as_deref(), Some("AB"));
    assert_eq!(decode_hex_utf8("c3A9").as_deref(), Some("é"));
    assert_eq!(decode_hex_utf8("").as_deref(), Some(""));
}

#[test]
fn rejects_malformed_digits_and_non_utf8() {
    assert_eq!(decode_hex_utf8("414"), None);
    assert_eq!(decode_hex_utf8("4g"), None);
    assert_eq!(decode_hex_utf8("ff"), None);
}

#[test]
fn rejects_characters_xml_cannot_carry() {
    // U+0001 and U+0000 are not XML 1.0 characters; tab/newline are.
    assert_eq!(decode_hex_utf8("41014200"), None);
    assert_eq!(decode_hex_utf8("410a09").as_deref(), Some("A\n\t"));
}
