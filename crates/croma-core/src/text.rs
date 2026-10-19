//! ABC 2.1 §8.2 text strings: decoding the accent mnemonics (`\'e`), named
//! HTML entities (`&eacute;`), fixed-width unicode (`é`, `\U000000e9`)
//! and the special-character escapes (`\\`, `\%`, `\&`) into plain text, and
//! encoding plain text back so a re-parse decodes to the same string.
//!
//! The parser keeps text exactly as written (the formatter and the LSP are
//! lossless); lowering decodes it when it becomes score text (titles, words,
//! lyrics, annotations, part names, tempo text), and the ABC writer encodes it.

/// One row of ABC 2.1 §14.1 "Supported accents & ligatures": the two-character
/// mnemonic after the backslash, the named HTML entity (empty when the spec
/// lists none), and the character.
const ACCENTS: &[(&str, &str, char)] = &[
    ("`A", "Agrave", 'À'),
    ("`a", "agrave", 'à'),
    ("`E", "Egrave", 'È'),
    ("`e", "egrave", 'è'),
    ("`I", "Igrave", 'Ì'),
    ("`i", "igrave", 'ì'),
    ("`O", "Ograve", 'Ò'),
    ("`o", "ograve", 'ò'),
    ("`U", "Ugrave", 'Ù'),
    ("`u", "ugrave", 'ù'),
    ("'A", "Aacute", 'Á'),
    ("'a", "aacute", 'á'),
    ("'E", "Eacute", 'É'),
    ("'e", "eacute", 'é'),
    ("'I", "Iacute", 'Í'),
    ("'i", "iacute", 'í'),
    ("'O", "Oacute", 'Ó'),
    ("'o", "oacute", 'ó'),
    ("'U", "Uacute", 'Ú'),
    ("'u", "uacute", 'ú'),
    ("'Y", "Yacute", 'Ý'),
    ("'y", "yacute", 'ý'),
    ("^A", "Acirc", 'Â'),
    ("^a", "acirc", 'â'),
    ("^E", "Ecirc", 'Ê'),
    ("^e", "ecirc", 'ê'),
    ("^I", "Icirc", 'Î'),
    ("^i", "icirc", 'î'),
    ("^O", "Ocirc", 'Ô'),
    ("^o", "ocirc", 'ô'),
    ("^U", "Ucirc", 'Û'),
    ("^u", "ucirc", 'û'),
    ("^Y", "Ycirc", 'Ŷ'),
    ("^y", "ycirc", 'ŷ'),
    ("~A", "Atilde", 'Ã'),
    ("~a", "atilde", 'ã'),
    ("~N", "Ntilde", 'Ñ'),
    ("~n", "ntilde", 'ñ'),
    ("~O", "Otilde", 'Õ'),
    ("~o", "otilde", 'õ'),
    ("\"A", "Auml", 'Ä'),
    ("\"a", "auml", 'ä'),
    ("\"E", "Euml", 'Ë'),
    ("\"e", "euml", 'ë'),
    ("\"I", "Iuml", 'Ï'),
    ("\"i", "iuml", 'ï'),
    ("\"O", "Ouml", 'Ö'),
    ("\"o", "ouml", 'ö'),
    ("\"U", "Uuml", 'Ü'),
    ("\"u", "uuml", 'ü'),
    ("\"Y", "Yuml", 'Ÿ'),
    ("\"y", "yuml", 'ÿ'),
    ("cC", "Ccedil", 'Ç'),
    ("cc", "ccedil", 'ç'),
    ("AA", "Aring", 'Å'),
    ("aa", "aring", 'å'),
    ("/O", "Oslash", 'Ø'),
    ("/o", "oslash", 'ø'),
    ("uA", "Abreve", 'Ă'),
    ("ua", "abreve", 'ă'),
    ("uE", "", 'Ĕ'),
    ("ue", "", 'ĕ'),
    ("vS", "Scaron", 'Š'),
    ("vs", "scaron", 'š'),
    ("vZ", "Zcaron", 'Ž'),
    ("vz", "zcaron", 'ž'),
    ("HO", "", 'Ő'),
    ("Ho", "", 'ő'),
    ("HU", "", 'Ű'),
    ("Hu", "", 'ű'),
    // Ligatures, etc.
    ("AE", "AElig", 'Æ'),
    ("ae", "aelig", 'æ'),
    ("OE", "OElig", 'Œ'),
    ("oe", "oelig", 'œ'),
    ("ss", "szlig", 'ß'),
    ("DH", "ETH", 'Ð'),
    ("dh", "eth", 'ð'),
    ("TH", "THORN", 'Þ'),
    ("th", "thorn", 'þ'),
    // ABC 2.0's cedilla and ring spellings, which the §14.2 errata replaced but
    // suggest still accepting.
    (",C", "", 'Ç'),
    (",c", "", 'ç'),
    ("oA", "", 'Å'),
    ("oa", "", 'å'),
];

/// Entities §8.2 names beyond the accent table, plus the XML basics.
const OTHER_ENTITIES: &[(&str, char)] = &[
    ("quot", '"'),
    ("copy", '©'),
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
];

/// Decode the escape that starts at the `\` beginning `text`, returning the
/// decoded text and how many bytes it consumed, or `None` when `text` does not
/// start with a §8.2 escape (the caller keeps the backslash as written).
///
/// `\"` is the umlaut mnemonic when a vowel or `y` follows (`\"o` is ö); any
/// other `\"` is an escaped double quote, as croma and abc2xml have always read
/// it inside quoted text.
pub(crate) fn decode_escape_at(text: &str) -> Option<(char, usize)> {
    let rest = text.strip_prefix('\\')?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    match first {
        '\\' | '%' | '&' => return Some((first, 2)),
        'u' => {
            if let Some(ch) = hex_char(rest.get(1..5)) {
                return Some((ch, 6));
            }
        }
        'U' => {
            if let Some(ch) = hex_char(rest.get(1..9)) {
                return Some((ch, 10));
            }
        }
        _ => {}
    }
    // Two-character mnemonic. `first` is ASCII for every table row, so the
    // pair is the next two bytes.
    if let Some(pair) = rest.get(..2)
        && let Some((_, _, ch)) = ACCENTS.iter().find(|(mnemonic, _, _)| *mnemonic == pair)
    {
        return Some((*ch, 3));
    }
    (first == '"').then_some(('"', 2))
}

/// Decode the entity that starts at the `&` beginning `text` (`&eacute;`,
/// `&#233;`, `&#xe9;`), returning the character and the bytes consumed, or
/// `None` when it is not a known entity (an ampersand followed by anything else
/// is literal, ABC 2.1 §8.2).
pub(crate) fn decode_entity_at(text: &str) -> Option<(char, usize)> {
    let rest = text.strip_prefix('&')?;
    let end = rest.find(';')?;
    let name = rest.get(..end)?;
    let consumed = end + 2;
    if let Some(number) = name.strip_prefix('#') {
        let value = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse::<u32>().ok()?,
        };
        return char::from_u32(value).map(|ch| (ch, consumed));
    }
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    ACCENTS
        .iter()
        .find(|(_, entity, _)| !entity.is_empty() && *entity == name)
        .map(|(_, _, ch)| *ch)
        .or_else(|| {
            OTHER_ENTITIES
                .iter()
                .find(|(entity, _)| *entity == name)
                .map(|(_, ch)| *ch)
        })
        .map(|ch| (ch, consumed))
}

/// `Some(char)` when `digits` is all hex and names a Unicode scalar value.
fn hex_char(digits: Option<&str>) -> Option<char> {
    let digits = digits?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    char::from_u32(u32::from_str_radix(digits, 16).ok()?)
}

/// Decode every §8.2 escape and entity in `text`. Anything that is not a
/// recognised escape or entity is kept exactly as written.
pub(crate) fn decode_text_string(text: &str) -> String {
    if !text.contains(['\\', '&']) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while let Some(rest) = text.get(index..) {
        let Some(ch) = rest.chars().next() else {
            break;
        };
        let decoded = match ch {
            '\\' => decode_escape_at(rest),
            '&' => decode_entity_at(rest),
            _ => None,
        };
        match decoded {
            Some((decoded, consumed)) => {
                out.push(decoded);
                index += consumed;
            }
            None => {
                out.push(ch);
                index += ch.len_utf8();
            }
        }
    }
    out
}

/// Encode `text` for an ABC text string so that [`decode_text_string`] gives it
/// back unchanged: `\` → `\\`, `%` → `\%` (it would start a comment), `&` → `\&`
/// when a letter or `#` follows (it could start an entity). Inside a quoted
/// string (`quoted`), `"` → `&quot;`, the spelling §8.2 gives for a quote.
pub(crate) fn encode_text_string(text: &str, quoted: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '&' if chars
                .peek()
                .is_some_and(|next| next.is_ascii_alphabetic() || *next == '#') =>
            {
                out.push_str("\\&")
            }
            '"' if quoted => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
