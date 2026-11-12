//! Complete finite scopes for the `bson` page.

use crate::pages::bson::{Bson, Decimal, Document, canonical_text, encode, relaxed_text};

/// The values of the scope: one or more of every BSON type.
///
/// Among them are the values the Extended JSON writers refuse (a double NaN with a payload, a
/// decimal128 NaN with a sign, a signal and a payload, an embedded document
/// holding a wrapper key).
#[must_use]
pub fn values() -> Vec<Bson> {
    vec![
        Bson::Double(1.5_f64.to_bits()),
        Bson::Double((-0.0_f64).to_bits()),
        Bson::Double(1.234_567_892_123_2e18_f64.to_bits()),
        Bson::Double(0x7FF8_0000_0000_0000),
        Bson::Double(0x7FF8_0000_0000_0012),
        Bson::Double(f64::NEG_INFINITY.to_bits()),
        Bson::String("é\u{0}\"".to_owned()),
        Bson::Document(vec![("x".to_owned(), Bson::Int32(1))]),
        Bson::Document(vec![("$oid".to_owned(), Bson::Null)]),
        Bson::Array(vec![Bson::Null, Bson::Int64(-2)]),
        Bson::Binary(0x80, vec![1, 2]),
        Bson::Binary(2, vec![0xFF]),
        Bson::Undefined,
        Bson::ObjectId([0xA0; 12]),
        Bson::Boolean(true),
        Bson::DateTime(1_356_351_330_501),
        Bson::DateTime(-1),
        Bson::Null,
        Bson::Regex("a/b".to_owned(), "im".to_owned()),
        Bson::DbPointer("n".to_owned(), [0x0B; 12]),
        Bson::Code("f".to_owned()),
        Bson::Symbol("s".to_owned()),
        Bson::CodeWithScope("g".to_owned(), vec![("y".to_owned(), Bson::Null)]),
        Bson::Int32(-7),
        Bson::Timestamp(4_000_000_000, 1),
        Bson::Int64(1 << 40),
        Bson::Decimal128(Decimal::Finite {
            negative: true,
            coefficient: 150,
            exponent: -5,
        }),
        Bson::Decimal128(Decimal::NaN(true, true, 5)),
        Bson::Decimal128(Decimal::Infinity(false)),
        Bson::MinKey,
        Bson::MaxKey,
    ]
}

/// The documents of the scope: the empty document, one element `a` of
/// each value, two elements `a` and `b` of each ordered pair of values,
/// and two elements both named `a`.
#[must_use]
pub fn documents() -> Vec<Document> {
    let values = values();
    let mut out = vec![Vec::new()];
    for value in &values {
        out.push(vec![("a".to_owned(), value.clone())]);
    }
    for first in &values {
        for second in &values {
            out.push(vec![
                ("a".to_owned(), first.clone()),
                ("b".to_owned(), second.clone()),
            ]);
        }
    }
    out.push(vec![
        ("a".to_owned(), Bson::Null),
        ("a".to_owned(), Bson::Null),
    ]);
    out
}

/// The number of [`documents`]: `1 + v + v^2 + 1` for `v` values.
#[must_use]
pub const fn documents_count(values: usize) -> usize {
    1 + values + values * values + 1
}

/// The bytes that replace each byte of a mutated document.
pub const REPLACEMENTS: &[u8] = &[0x00, 0x01, 0x7F, 0xFF];

/// The byte strings of the scope.
///
/// They are the encoding of every document, and for the empty and every one-element document, each byte replaced by each of
/// [`REPLACEMENTS`] that differs from it, each proper prefix, and the
/// encoding followed by one `0x00`.
#[must_use]
pub fn byte_strings() -> Vec<Vec<u8>> {
    let documents = documents();
    let mut out: Vec<Vec<u8>> = documents.iter().map(encode).collect();
    for document in documents.iter().filter(|d| d.len() <= 1) {
        let bytes = encode(document);
        for i in 0..bytes.len() {
            for &replacement in REPLACEMENTS {
                if bytes[i] != replacement {
                    let mut mutated = bytes.clone();
                    mutated[i] = replacement;
                    out.push(mutated);
                }
            }
        }
        for length in 0..bytes.len() {
            out.push(bytes[..length].to_vec());
        }
        let mut longer = bytes;
        longer.push(0);
        out.push(longer);
    }
    out
}

/// Extended JSON texts that exercise each refusal and alternative form of
/// the reader.
pub const TEXTS: &[&str] = &[
    "[1]",
    r#"{"a":1,"a":2}"#,
    r#"{"a\u0000":1}"#,
    r#"{"$oid":"x","$numberInt":1}"#,
    r#"{"a":12345678901234567890}"#,
    r#"{"a":-9223372036854775808}"#,
    r#"{"a":2147483648}"#,
    r#"{"a":-0}"#,
    r#"{"a":0.10000000000000000001}"#,
    r#"{"a":1.50}"#,
    r#"{"a":1E+2}"#,
    r#"{"a":1e400}"#,
    r#"{"a":{"$numberInt":42}}"#,
    r#"{"a":{"$numberInt":"042"}}"#,
    r#"{"a":{"$numberInt":"2147483648"}}"#,
    r#"{"a":{"$numberLong":"-0"}}"#,
    r#"{"a":{"$numberLong":"1","x":1}}"#,
    r#"{"a":{"$numberDouble":"1e400"}}"#,
    r#"{"a":{"$numberDouble":"inf"}}"#,
    r#"{"a":{"$numberDouble":".5"}}"#,
    r#"{"a":{"$numberDouble":"0.1000000000000000000001"}}"#,
    r#"{"a":{"$numberDecimal":"1E+6112"}}"#,
    r#"{"a":{"$numberDecimal":"1.000000000000000000000000000000000000"}}"#,
    r#"{"a":{"$numberDecimal":"sNaN"}}"#,
    r#"{"a":{"$numberDecimal":"-iNF"}}"#,
    r#"{"a":{"$numberDecimal":"017."}}"#,
    r#"{"a":{"$numberDecimal":"1e"}}"#,
    r#"{"a":{"$oid":"0123456789ABCDEFabcdef01"}}"#,
    r#"{"a":{"$oid":"0123456789abcdef0123456"}}"#,
    r#"{"a":{"$binary":{"base64":"//8=","subType":"0"}}}"#,
    r#"{"a":{"$binary":{"subType":"80","base64":""}}}"#,
    r#"{"a":{"$binary":{"base64":"//8","subType":"00"}}}"#,
    r#"{"a":{"$binary":{"base64":"//9=","subType":"00"}}}"#,
    r#"{"a":{"$binary":{"base64":"//8=","subType":"100"}}}"#,
    r#"{"a":{"$binary":"AQID","$type":"00"}}"#,
    r#"{"a":{"$uuid":"73ffd264-44b3-4c69-90e8-e7d1dfc035d4"}}"#,
    r#"{"a":{"$uuid":"73ffd26444b34c6990e8e7d1dfc035d4"}}"#,
    r#"{"a":{"$code":"x","$scope":{"$numberInt":"1"}}}"#,
    r#"{"a":{"$scope":{},"$code":1}}"#,
    r#"{"a":{"$code":"x","$scope":1}}"#,
    r#"{"a":{"$scope":{}}}"#,
    r#"{"a":{"$timestamp":{"i":1,"t":2}}}"#,
    r#"{"a":{"$timestamp":{"t":1}}}"#,
    r#"{"a":{"$timestamp":{"t":4294967296,"i":0}}}"#,
    r#"{"a":{"$regularExpression":{"options":"i","pattern":"p"}}}"#,
    r#"{"a":{"$regularExpression":{"pattern":"p","options":"mi"}}}"#,
    r#"{"a":{"$regularExpression":{"pattern":"p\u0000","options":""}}}"#,
    r#"{"a":{"$regularExpression":{"pattern":1,"options":""}}}"#,
    r#"{"a":{"$dbPointer":{"$id":{"$oid":"0123456789abcdef01234567"},"$ref":"n"}}}"#,
    r#"{"a":{"$dbPointer":{"$ref":"n","$id":"x"}}}"#,
    r#"{"a":{"$date":42}}"#,
    r#"{"a":{"$date":"2012-12-24T13:15:30.501+01:00"}}"#,
    r#"{"a":{"$date":"2012-12-24t12:15:30z"}}"#,
    r#"{"a":{"$date":"2012-12-24T12:15:30.5012Z"}}"#,
    r#"{"a":{"$date":"2012-02-30T12:15:30Z"}}"#,
    r#"{"a":{"$date":"2012-12-24T12:15:30"}}"#,
    r#"{"a":{"$date":{"$numberLong":"-1"}}}"#,
    r#"{"a":{"$minKey":1.0}}"#,
    r#"{"a":{"$maxKey":1}}"#,
    r#"{"a":{"$undefined":false}}"#,
    r#"{"a":{"$regex":"x","$options":"i"},"t":{"$type":"string"}}"#,
    r#"{"a":{"$ref":"c","$id":{"$oid":"0123456789abcdef01234567"}}}"#,
];

/// The texts of the scope: the canonical and the relaxed text of every
/// document that has them, then [`TEXTS`].
#[must_use]
pub fn texts() -> Vec<String> {
    let mut out = Vec::new();
    for document in documents() {
        out.extend(
            [canonical_text(&document), relaxed_text(&document)]
                .into_iter()
                .flatten(),
        );
    }
    out.extend(TEXTS.iter().map(|t| (*t).to_owned()));
    out
}
