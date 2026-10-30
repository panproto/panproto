//! Complete finite scopes for the value-I/O page.

use crate::value_io::{Presence, Route};

/// Every route and presence-state pair in the VF8 default-route table.
#[must_use]
pub const fn presences() -> [(Route, Presence); 12] {
    [
        (Route::Json, Presence::Null),
        (Route::Json, Presence::Absent),
        (Route::Toml, Presence::Null),
        (Route::Toml, Presence::Absent),
        (Route::Yaml, Presence::Null),
        (Route::Yaml, Presence::Absent),
        (Route::Xml, Presence::Null),
        (Route::Xml, Presence::Absent),
        (Route::Csv, Presence::Null),
        (Route::Csv, Presence::Absent),
        (Route::Tsv, Presence::Null),
        (Route::Tsv, Presence::Absent),
    ]
}

/// One bounded set of blob fields covering JSON escaping and size extremes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobFields {
    /// The CID text carried by the nested link object.
    pub ref_: &'static str,
    /// The media type text.
    pub mime: &'static str,
    /// The declared blob size.
    pub size: u64,
}

/// Blob fields for JSON escape, Unicode and size boundaries.
#[must_use]
pub const fn blob_fields() -> [BlobFields; 4] {
    [
        BlobFields {
            ref_: "",
            mime: "",
            size: 0,
        },
        BlobFields {
            ref_: "quote\"slash\\backspace\u{8}form-feed\u{c}",
            mime: "line\nreturn\rtab\t",
            size: 1,
        },
        BlobFields {
            ref_: "\0\u{1f}\u{7f}",
            mime: "\u{80}\u{9f}",
            size: 2,
        },
        BlobFields {
            ref_: "bafy-λ-雪-😀",
            mime: "image/雪-😀",
            size: u64::MAX,
        },
    ]
}

/// One accepted XML source-encoding case and its decoded element text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlEncodingCase {
    /// Complete XML source bytes.
    pub source: Vec<u8>,
    /// Expected Unicode text after parsing.
    pub expected: &'static str,
}

fn utf16_xml(text: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xff, 0xfe]
    } else {
        vec![0xfe, 0xff]
    };
    for unit in text.encode_utf16() {
        let encoded = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

/// Accepted UTF-8, UTF-16 and ISO-8859-1 XML source encodings.
#[must_use]
pub fn canonical_xml_encoding_cases() -> Vec<XmlEncodingCase> {
    let utf16 = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><root><value>x</value></root>";
    let mut latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root><value>".to_vec();
    latin1.push(0x80);
    latin1.extend_from_slice(b"</value></root>");
    vec![
        XmlEncodingCase {
            source:
                b"\xef\xbb\xbf<?xml version=\"1.0\" encoding=\"UTF-8\"?><root><value>x</value></root>"
                    .to_vec(),
            expected: "x",
        },
        XmlEncodingCase {
            source: utf16_xml(utf16, true),
            expected: "x",
        },
        XmlEncodingCase {
            source: utf16_xml(utf16, false),
            expected: "x",
        },
        XmlEncodingCase {
            source: latin1,
            expected: "\u{80}",
        },
    ]
}

/// Every Unicode scalar value, in code-point order: U+0000 to U+10FFFF less
/// the 2,048 surrogates, 1,112,064 scalars.
pub fn single_scalars() -> impl Iterator<Item = char> {
    (0..=0x0010_ffff_u32).filter_map(char::from_u32)
}

/// Integer numerals at and beyond the `i64` boundary: the bounds themselves,
/// small values of each sign, the first integers outside `i64`, and
/// `10^20` and `2^64` of each sign.
#[must_use]
pub const fn exact_integer_lexemes() -> [&'static str; 13] {
    [
        "0",
        "1",
        "-1",
        "9223372036854775807",
        "-9223372036854775808",
        "9223372036854775808",
        "-9223372036854775809",
        "100000000000000000000",
        "-100000000000000000000",
        "18446744073709551616",
        "-18446744073709551616",
        "123456789012345678901234567890",
        "-123456789012345678901234567890",
    ]
}

/// The special decimal values: both infinities, and the quiet and signalling
/// NaNs of each sign with payloads 0 and 1.
#[must_use]
pub const fn special_decimal_lexemes() -> [&'static str; 10] {
    [
        "Infinity",
        "-Infinity",
        "NaN",
        "-NaN",
        "NaN1",
        "-NaN1",
        "sNaN",
        "-sNaN",
        "sNaN1",
        "-sNaN1",
    ]
}

/// The non-finite binary64 bit patterns: both infinities, the two canonical
/// NaNs, and a quiet NaN of each sign with payload 1.
#[must_use]
pub const fn nonfinite_float_bits() -> [u64; 6] {
    [
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0001,
    ]
}

/// Every byte string of length at most two.
#[must_use]
pub fn byte_strings() -> Option<super::ByteStrings> {
    super::byte_strings(2)
}

/// Numerals with at most two digits and exponents from -1 to 1.
#[must_use]
pub fn numerals() -> Vec<String> {
    super::numerals(2, -1, 1)
}

/// All words of length at most two over a supplied labelled alphabet.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 2)
}

/// The fixed temporal lexical product used by the page.
#[must_use]
pub fn temporal_lexemes() -> Vec<String> {
    super::temporal_lexemes()
}
