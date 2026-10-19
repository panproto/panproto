//! Byte-text forms preserve the bytes they denote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::Value;
use panproto_inst::value::FidelityReason;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_reference::bytes::{
    ByteTextError, decode_base64, encode_base64, latin1_decode, latin1_decode_encode,
    latin1_encode, latin1_encode_decode, optional, padded_bytes_decode_encode,
    padded_text_encode_decode,
};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

fn read(text: &str, form: BytesText) -> Result<Value, FidelityReason> {
    coerce(
        &Lexeme::new(text, LexClass::Bytes),
        &ValueSpec::new(ScalarKind::Bytes(form)),
    )
    .map_err(|error| error.reason)
}

/// The production reading of a base64 text in the reference's terms: the
/// decoded bytes, or the single refusal production reports for every
/// malformed text.
fn read_reference(result: Result<Vec<u8>, ByteTextError>) -> Result<Value, FidelityReason> {
    result
        .map(Value::Bytes)
        .map_err(|_| FidelityReason::InvalidEncoding)
}

fn url64(text: &str) -> String {
    text.replace('+', "-").replace('/', "_")
}

fn bytes_of(length: u32, number: u32) -> Vec<u8> {
    (0..length)
        .rev()
        .map(|place| {
            u8::try_from((number >> (place * 8)) & 0xff).expect("a masked byte fits in u8")
        })
        .collect()
}

/// Both Latin-1 directions on every byte string of length at most two.
fn check_latin1() -> u64 {
    let mut latin1_cases = 0_u64;
    for length in 0_u32..=2 {
        for number in 0..256_u32.pow(length) {
            let bytes = bytes_of(length, number);
            let text = latin1_decode(&bytes);
            assert_eq!(latin1_encode(&text).as_deref(), Ok(bytes.as_slice()));
            assert!(latin1_decode_encode(&bytes), "Latin-1 bytes {bytes:?}");
            assert!(latin1_encode_decode(&text), "Latin-1 text {text:?}");
            assert_eq!(
                read(&text, BytesText::Latin1),
                Ok(Value::Bytes(bytes.clone())),
                "Latin-1 length {length}, number {number}"
            );
            latin1_cases += 1;
        }
    }

    latin1_cases
}

/// The padded and optional base64 forms on every byte string of length
/// at most three: the number of byte strings and of padded encodings.
fn check_base64() -> (u64, u64) {
    let mut base64_cases = 0_u64;
    let mut padded_cases = 0_u64;
    for length in 0_u32..=3 {
        for number in 0..256_u32.pow(length) {
            let bytes = bytes_of(length, number);
            let standard = encode_base64(&bytes);
            let standard_unpadded = standard.trim_end_matches('=');
            let url = url64(&standard);
            let url_unpadded = url.trim_end_matches('=');
            assert!(padded_bytes_decode_encode(&bytes), "{bytes:?}");
            assert!(padded_text_encode_decode(&standard), "{standard}");
            assert_eq!(
                optional(standard_unpadded).as_deref(),
                Ok(standard.as_str())
            );
            assert_eq!(
                read(&standard, BytesText::Base64Padded),
                read_reference(decode_base64(&standard, true))
            );
            assert_eq!(
                read(standard_unpadded, BytesText::Base64),
                read_reference(decode_base64(standard_unpadded, false))
            );
            let expected = Ok(Value::Bytes(bytes));

            assert_eq!(
                read(&standard, BytesText::Base64Padded),
                expected,
                "padded standard length {length}, number {number}"
            );
            assert_eq!(
                read(&url, BytesText::Base64UrlPadded),
                expected,
                "padded URL length {length}, number {number}"
            );
            assert_eq!(read(&standard, BytesText::Base64), expected);
            assert_eq!(read(standard_unpadded, BytesText::Base64), expected);
            assert_eq!(read(&url, BytesText::Base64Url), expected);
            assert_eq!(read(url_unpadded, BytesText::Base64Url), expected);

            if standard_unpadded != standard {
                assert_eq!(
                    read(standard_unpadded, BytesText::Base64Padded),
                    Err(FidelityReason::InvalidEncoding)
                );
                assert_eq!(
                    decode_base64(standard_unpadded, true),
                    Err(ByteTextError::InvalidPadding)
                );
                assert_eq!(
                    read(url_unpadded, BytesText::Base64UrlPadded),
                    Err(FidelityReason::InvalidEncoding)
                );
            }
            base64_cases += 1;
            padded_cases += 2;
        }
    }
    (base64_cases, padded_cases)
}

/// Text outside Latin-1 and malformed base64, refused alike.
fn check_malformed_probes() -> u64 {
    let mut probes = 0_u64;
    for text in ["Ā", "€", "aĀ", "ÿĀ"] {
        assert_eq!(
            read(text, BytesText::Latin1),
            Err(FidelityReason::InvalidEncoding)
        );
        assert_eq!(latin1_encode(text), Err(ByteTextError::NonLatin1));
        assert!(!latin1_encode_decode(text), "{text}");
        probes += 1;
    }
    for text in [
        "/w=", "/x==", "@@@@", "A===", "AB=C", "AA==AA==", "=", "AAA", "A",
    ] {
        for (form, require_padding) in [(BytesText::Base64Padded, true), (BytesText::Base64, false)]
        {
            assert_eq!(
                read(text, form.clone()),
                read_reference(decode_base64(text, require_padding)),
                "{text} {form:?}"
            );
            probes += 1;
        }
    }

    probes
}

/// Latin-1 is a byte/code-point bijection, padded base64 requires its
/// canonical padding, and the optional forms accept both spellings on the
/// complete stated carriers. On every string of each carrier, production
/// reading agrees with the reference's decoding, and the reference's
/// round-trip laws hold.
#[test]
fn a_byte_text_form_keeps_its_bytes() {
    assert_eq!(read("ÿ", BytesText::Latin1), Ok(Value::Bytes(vec![0xff])));
    assert_eq!(
        read("Ā", BytesText::Latin1),
        Err(FidelityReason::InvalidEncoding)
    );
    assert_eq!(
        read("/w==", BytesText::Base64Padded),
        Ok(Value::Bytes(vec![0xff]))
    );
    assert_eq!(
        read("/w", BytesText::Base64Padded),
        Err(FidelityReason::InvalidEncoding)
    );
    assert_eq!(
        read("/w==", BytesText::Base64),
        Ok(Value::Bytes(vec![0xff]))
    );
    assert_eq!(read("/w", BytesText::Base64), Ok(Value::Bytes(vec![0xff])));

    let forms = [
        (BytesText::Base64Padded, "\"base64_padded\""),
        (BytesText::Base64UrlPadded, "\"base64_url_padded\""),
        (BytesText::Latin1, "\"latin1\""),
    ];
    for (form, json) in forms {
        assert_eq!(serde_json::to_string(&form).unwrap(), json);
        assert_eq!(serde_json::from_str::<BytesText>(json).unwrap(), form);
    }

    let latin1_cases = check_latin1();
    assert_eq!(latin1_cases, 1 + 256 + 256_u64.pow(2));
    let (base64_cases, padded_cases) = check_base64();
    let probes = check_malformed_probes();
    assert_eq!(probes, 4 + 9 * 2);

    let byte_strings = 1 + 256 + 256_u64.pow(2) + 256_u64.pow(3);
    assert_eq!(base64_cases, byte_strings);
    assert_eq!(padded_cases, 2 * byte_strings);
    println!(
        "byte text: {latin1_cases} Latin-1 strings through both directions; \
         {base64_cases} byte strings under two optional and two required-padding forms; \
         {padded_cases} padded encodings; {probes} malformed probes; all agree with the reference"
    );
}
