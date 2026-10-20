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

/// The number of workers an exhaustive sweep is spread over.
const WORKERS: usize = 4;

/// Run `work` once per worker index on its own thread and collect the
/// results in worker order.
fn spread<T: Send>(work: impl Fn(usize) -> T + Sync) -> Vec<T> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let work = &work;
                scope.spawn(move || work(worker))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a sweep worker finishes"))
            .collect()
    })
}

/// Both Latin-1 directions on every byte string of length at most two.
fn check_latin1() -> u64 {
    let mut latin1_cases = 0_u64;
    for bytes in panproto_reference::enumerate::values::byte_strings()
        .expect("the length-two byte-string scope is finite")
    {
        let text = latin1_decode(&bytes);
        assert_eq!(latin1_encode(&text).as_deref(), Ok(bytes.as_slice()));
        assert!(latin1_decode_encode(&bytes), "Latin-1 bytes {bytes:?}");
        assert!(latin1_encode_decode(&text), "Latin-1 text {text:?}");
        assert_eq!(
            read(&text, BytesText::Latin1),
            Ok(Value::Bytes(bytes.clone())),
            "Latin-1 bytes {bytes:?}"
        );
        latin1_cases += 1;
    }

    latin1_cases
}

/// The padded and optional base64 forms on one byte string.
fn check_base64_bytes(bytes: &[u8]) {
    let standard = encode_base64(bytes);
    let standard_unpadded = standard.trim_end_matches('=');
    let url = url64(&standard);
    let url_unpadded = url.trim_end_matches('=');
    assert!(padded_bytes_decode_encode(bytes), "{bytes:?}");
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
    let expected = Ok(Value::Bytes(bytes.to_vec()));

    assert_eq!(
        read(&standard, BytesText::Base64Padded),
        expected,
        "padded standard {bytes:?}"
    );
    assert_eq!(
        read(&url, BytesText::Base64UrlPadded),
        expected,
        "padded URL {bytes:?}"
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
}

/// The padded and optional base64 forms on every byte string of length
/// at most three, spread over the workers: the number of byte strings and
/// of padded encodings.
fn check_base64() -> (u64, u64) {
    let counts = spread(|worker| {
        let mut base64_cases = 0_u64;
        let mut padded_cases = 0_u64;
        for bytes in panproto_reference::enumerate::values::byte_strings_up_to_three()
            .expect("the length-three byte-string scope is finite")
            .skip(worker)
            .step_by(WORKERS)
        {
            check_base64_bytes(&bytes);
            base64_cases += 1;
            padded_cases += 2;
        }
        (base64_cases, padded_cases)
    });
    counts
        .into_iter()
        .fold((0, 0), |(cases, padded), (more_cases, more_padded)| {
            (cases + more_cases, padded + more_padded)
        })
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

/// Counts from one worker's share of the base64 text sweep: texts seen,
/// texts the padded form accepts, texts the optional form accepts.
#[derive(Clone, Copy, Default)]
struct TextCounts {
    texts: u64,
    padded: u64,
    optional: u64,
}

/// One text of the base64 text scope: production reading under each
/// standard form agrees with the reference decoding in that mode, and an
/// accepted padded spelling satisfies the padded encode-then-decode law.
fn check_base64_text(text: &str, counts: &mut TextCounts) {
    counts.texts += 1;
    if let Ok(bytes) = decode_base64(text, true) {
        assert!(padded_text_encode_decode(text), "padded {text:?}");
        assert_eq!(
            read(text, BytesText::Base64Padded),
            Ok(Value::Bytes(bytes)),
            "padded {text:?}"
        );
        counts.padded += 1;
    } else {
        assert!(!padded_text_encode_decode(text), "padded {text:?}");
        assert_eq!(
            read(text, BytesText::Base64Padded),
            Err(FidelityReason::InvalidEncoding),
            "padded {text:?}"
        );
    }
    match decode_base64(text, false) {
        Ok(bytes) => {
            assert_eq!(
                read(text, BytesText::Base64),
                Ok(Value::Bytes(bytes)),
                "optional {text:?}"
            );
            counts.optional += 1;
        }
        Err(_) => assert_eq!(
            read(text, BytesText::Base64),
            Err(FidelityReason::InvalidEncoding),
            "optional {text:?}"
        ),
    }
}

/// Padded canonicity and optional-padding decoding over every text of
/// length at most four over the base64 characters, `=` and `@`: production
/// accepts exactly the texts the reference decodes, with the same bytes, and
/// refuses the rest as an invalid encoding.
#[test]
fn every_short_base64_text_reads_as_the_reference_decodes() {
    let counts = spread(|worker| {
        let mut counts = TextCounts::default();
        for text in panproto_reference::enumerate::values::base64_texts()
            .skip(worker)
            .step_by(WORKERS)
        {
            check_base64_text(&text, &mut counts);
        }
        counts
    });
    let total = counts
        .iter()
        .fold(TextCounts::default(), |sum, part| TextCounts {
            texts: sum.texts + part.texts,
            padded: sum.padded + part.padded,
            optional: sum.optional + part.optional,
        });

    let symbols = 64 + 2_u64;
    let expected_texts: u64 = (0..=4).map(|length| symbols.pow(length)).sum();
    assert_eq!(total.texts, expected_texts);
    // The padded form accepts exactly one spelling of each byte string of
    // length at most three: the empty text and the four-character quanta.
    let byte_strings: u64 = (0..=3).map(|length| 256_u64.pow(length)).sum();
    assert_eq!(total.padded, byte_strings);
    // The optional form also accepts the two- and three-character unpadded
    // spellings of one and two bytes.
    assert_eq!(total.optional, byte_strings + 256 + 256_u64.pow(2));
    println!(
        "base64 texts of length at most 4 over 66 symbols: {} texts (expected {expected_texts}); \
         {} accepted padded (expected {byte_strings}); {} accepted optional; \
         production agrees with the reference on all",
        total.texts, total.padded, total.optional
    );
}

/// Optional-padding equivalence over every unpadded text a padding can
/// complete: appending the padding that completes the quantum changes
/// neither the normal form, nor the reference decoding, nor production's
/// reading.
#[test]
fn optional_padding_is_equivalent_on_every_short_unpadded_text() {
    let texts = panproto_reference::enumerate::values::unpadded_texts();
    let mut checked = 0_u64;
    let mut normalized = 0_u64;
    let mut decoded = 0_u64;
    for unpadded in &texts {
        let length = unpadded.chars().count();
        let padding = (4 - length % 4) % 4;
        let padded = format!("{unpadded}{}", "=".repeat(padding));

        let normal = optional(unpadded);
        assert_eq!(
            normal,
            optional(&padded),
            "optional {unpadded:?} {padded:?}"
        );
        let decoding = decode_base64(unpadded, false);
        assert_eq!(
            decoding,
            decode_base64(&padded, false),
            "decode {unpadded:?} {padded:?}"
        );
        let production = read(unpadded, BytesText::Base64);
        assert_eq!(
            production,
            read(&padded, BytesText::Base64),
            "production {unpadded:?} {padded:?}"
        );
        assert_eq!(production, read_reference(decoding.clone()), "{unpadded:?}");

        checked += 1;
        normalized += u64::from(normal.is_ok());
        decoded += u64::from(decoding.is_ok());
    }

    let symbols = 64 + 1_u64;
    let expected = 1 + symbols.pow(2) + symbols.pow(3);
    assert_eq!(checked, expected);
    println!(
        "unpadded texts of length 0, 2, 3 over 65 symbols: {checked} pairs (expected {expected}); \
         {normalized} normalize; {decoded} decode; both spellings agree on all"
    );
}

/// Latin-1 over every string of length at most two over the Latin-1 code
/// points and three beyond it: the encode-then-decode law holds exactly on
/// the strings within U+00FF, encoding refuses the rest, and production
/// reads each string as the reference encodes it.
#[test]
fn latin1_reads_exactly_the_strings_within_u_ff() {
    let mut checked = 0_u64;
    let mut within = 0_u64;
    for text in panproto_reference::enumerate::values::latin1_strings() {
        let is_latin1 = text.chars().all(|c| u32::from(c) <= 0xff);
        assert_eq!(latin1_encode_decode(&text), is_latin1, "{text:?}");
        let encoded = latin1_encode(&text);
        if is_latin1 {
            let bytes = encoded.expect("a Latin-1 string encodes");
            assert_eq!(
                read(&text, BytesText::Latin1),
                Ok(Value::Bytes(bytes)),
                "{text:?}"
            );
            within += 1;
        } else {
            assert_eq!(encoded, Err(ByteTextError::NonLatin1), "{text:?}");
            assert_eq!(
                read(&text, BytesText::Latin1),
                Err(FidelityReason::InvalidEncoding),
                "{text:?}"
            );
        }
        checked += 1;
    }

    let symbols = 256 + 3_u64;
    let expected = 1 + symbols + symbols.pow(2);
    let expected_within = 1 + 256 + 256_u64.pow(2);
    assert_eq!(checked, expected);
    assert_eq!(within, expected_within);
    println!(
        "Latin-1 strings of length at most 2 over 259 code points: {checked} strings \
         (expected {expected}); {within} within U+00FF (expected {expected_within}); \
         production agrees with the reference on all"
    );
}

/// The characters of the generated base64 texts: the alphabet, the padding
/// character, one ASCII character outside the alphabet and one non-ASCII
/// character.
const TEXT_SYMBOLS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=@é";

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    })]

    /// Generated byte strings through every reference byte-text function and
    /// every production form, and generated texts through both standard
    /// base64 readings, agree with the reference.
    #[test]
    fn randomized_byte_text_differential(
        bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..=16),
        symbols in proptest::collection::vec(
            proptest::sample::select(TEXT_SYMBOLS.chars().collect::<Vec<_>>()),
            0..=12,
        ),
    ) {
        let text = latin1_decode(&bytes);
        proptest::prop_assert!(
            text.chars().map(u32::from).eq(bytes.iter().copied().map(u32::from))
        );
        proptest::prop_assert!(latin1_decode_encode(&bytes));
        proptest::prop_assert!(latin1_encode_decode(&text));
        proptest::prop_assert_eq!(
            read(&text, BytesText::Latin1),
            Ok(Value::Bytes(bytes.clone()))
        );

        let standard = encode_base64(&bytes);
        let unpadded = standard.trim_end_matches('=');
        let url = url64(&standard);
        let url_unpadded = url.trim_end_matches('=');
        let expected = Ok(Value::Bytes(bytes.clone()));
        proptest::prop_assert!(padded_bytes_decode_encode(&bytes));
        proptest::prop_assert!(padded_text_encode_decode(&standard));
        proptest::prop_assert_eq!(optional(unpadded), Ok(standard.clone()));
        proptest::prop_assert_eq!(
            read(&standard, BytesText::Base64Padded),
            read_reference(decode_base64(&standard, true))
        );
        proptest::prop_assert_eq!(&read(&standard, BytesText::Base64Padded), &expected);
        proptest::prop_assert_eq!(&read(&standard, BytesText::Base64), &expected);
        proptest::prop_assert_eq!(
            read(unpadded, BytesText::Base64),
            read_reference(decode_base64(unpadded, false))
        );
        proptest::prop_assert_eq!(&read(unpadded, BytesText::Base64), &expected);
        proptest::prop_assert_eq!(&read(&url, BytesText::Base64UrlPadded), &expected);
        proptest::prop_assert_eq!(&read(&url, BytesText::Base64Url), &expected);
        proptest::prop_assert_eq!(&read(url_unpadded, BytesText::Base64Url), &expected);
        if unpadded != standard {
            proptest::prop_assert_eq!(
                read(unpadded, BytesText::Base64Padded),
                Err(FidelityReason::InvalidEncoding)
            );
            proptest::prop_assert_eq!(
                read(url_unpadded, BytesText::Base64UrlPadded),
                Err(FidelityReason::InvalidEncoding)
            );
        }

        let generated: String = symbols.into_iter().collect();
        proptest::prop_assert_eq!(
            read(&generated, BytesText::Base64Padded),
            read_reference(decode_base64(&generated, true)),
            "padded {:?}", generated
        );
        proptest::prop_assert_eq!(
            read(&generated, BytesText::Base64),
            read_reference(decode_base64(&generated, false)),
            "optional {:?}", generated
        );
    }
}
