//! ABC 2.1 §8.2 text-string decoding and encoding tests.

use super::{decode_entity_at, decode_escape_at, decode_text_string, encode_text_string};

#[test]
fn decodes_the_three_encodings_of_e_acute() {
    // §8.2: mnemonic, named entity, 16-bit and 32-bit fixed-width unicode.
    for written in [
        "Caf\\'e",
        "Caf&eacute;",
        "Caf\\u00e9",
        "Caf\\U000000e9",
        "Caf&#233;",
        "Caf&#xe9;",
    ] {
        assert_eq!(decode_text_string(written), "Café", "{written}");
    }
}

#[test]
fn decodes_every_accent_family_and_ligature() {
    // One example per row family of §14.1.
    let cases = [
        ("\\`a", "à"),
        ("\\'Y", "Ý"),
        ("\\^o", "ô"),
        ("\\~n", "ñ"),
        ("\\\"u", "ü"),
        ("\\cC", "Ç"),
        ("\\AA", "Å"),
        ("\\/o", "ø"),
        ("\\ua", "ă"),
        ("\\uE", "Ĕ"),
        ("\\vs", "š"),
        ("\\Ho", "ő"),
        ("\\ss", "ß"),
        ("\\AE", "Æ"),
        ("\\oe", "œ"),
        ("\\DH", "Ð"),
        ("\\th", "þ"),
        ("&Scaron;", "Š"),
        ("&szlig;", "ß"),
        ("&copy;", "©"),
        ("&quot;", "\""),
    ];
    for (written, expected) in cases {
        assert_eq!(decode_text_string(written), expected, "{written}");
    }
}

#[test]
fn accepts_the_abc_2_0_cedilla_and_ring_spellings() {
    // §14.2 errata: `\,c` and `\oa` were replaced but are worth still reading.
    assert_eq!(decode_text_string("Gar\\,con \\oA"), "Garçon Å");
}

#[test]
fn special_character_escapes() {
    // §8.2 "Special characters": \\ \% \&.
    assert_eq!(decode_text_string("a\\\\b 50\\% G\\&T"), "a\\b 50% G&T");
}

#[test]
fn u_followed_by_non_hex_is_a_breve() {
    // §8.2: "if any of the 4 characters after \u is not hexadecimal, then it is
    // interpreted as a breve".
    assert_eq!(decode_text_string("\\uAtx"), "Ătx");
    assert_eq!(decode_text_string("\\uABCD"), "\u{abcd}");
}

#[test]
fn backslash_quote_is_an_umlaut_only_before_a_vowel_or_y() {
    assert_eq!(decode_text_string("M\\\"uller"), "Müller");
    assert_eq!(decode_text_string("say \\\"hi\\\""), "say \"hi\"");
}

#[test]
fn unknown_sequences_and_lone_ampersands_stay_as_written() {
    assert_eq!(
        decode_text_string("\\q \\ x & y &unknown; &;"),
        "\\q \\ x & y &unknown; &;"
    );
    assert_eq!(decode_text_string("trailing\\"), "trailing\\");
    assert_eq!(
        decode_text_string("&#xD800;"),
        "&#xD800;",
        "a lone surrogate is not a char"
    );
    assert_eq!(decode_text_string("plain"), "plain");
}

#[test]
fn position_decoders_report_bytes_consumed() {
    assert_eq!(decode_escape_at("\\'e rest"), Some(('é', 3)));
    assert_eq!(decode_escape_at("\\u00e9 rest"), Some(('é', 6)));
    assert_eq!(decode_escape_at("\\- rest"), None);
    assert_eq!(decode_entity_at("&eacute; rest"), Some(('é', 8)));
    assert_eq!(decode_entity_at("& rest"), None);
}

#[test]
fn encoding_round_trips_through_decoding() {
    let samples = [
        "plain",
        "Café",
        "back\\slash \\'e",
        "50% off",
        "G&T and & and &eacute;",
        "say \"hi\"",
        "&quot; literally",
    ];
    for text in samples {
        for quoted in [false, true] {
            let encoded = encode_text_string(text, quoted);
            assert_eq!(
                decode_text_string(&encoded),
                text,
                "{text:?} quoted={quoted}: {encoded}"
            );
            if quoted {
                assert!(!encoded.contains('"'), "{encoded}");
            }
            assert!(
                !encoded.contains('%') || encoded.contains("\\%"),
                "{encoded}"
            );
        }
    }
}
