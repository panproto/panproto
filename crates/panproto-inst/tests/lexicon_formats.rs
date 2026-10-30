#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The AT Protocol Lexicon string formats of `ValueFormat`.
//!
//! Each format is checked end to end: a schema whose string vertex carries
//! `format = <name>` under `ConstraintSemantics::Format("atproto")`
//! validates a graph holding one value, and the report passes or fails.
//!
//! The cases come from three sources, each fetched on 2026-10-14:
//!
//! - the examples the specifications give, quoted literally: the Lexicon
//!   specification (<https://atproto.com/specs/lexicon>), the handle, NSID,
//!   TID, record key, AT URI and DID specifications under
//!   <https://atproto.com/specs/>, the data model's example link
//!   (<https://atproto.com/specs/data-model>) and RFC 5646 Appendix A;
//! - the interop test vectors those specifications link to, copied
//!   unchanged into `tests/fixtures/atproto-interop/syntax/` from
//!   <https://github.com/bluesky-social/atproto-interop-tests> at commit
//!   `056e5741bb330757205d6b16db5266fffcae937b` (CC0 1.0; the repository's
//!   README says the specifications are the authority where the two
//!   disagree, and each disagreement is listed below with the text that
//!   decides it);
//! - edge cases at each limit the specifications state.
//!
//! The last section runs production and the executable reference
//! (`panproto_reference`'s validation page) over the vectors and generated
//! corpora around every rule, under the `atproto` family and another, and
//! requires the same verdict for every value.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use panproto_gat::CheckStatus;
use panproto_inst::validation::ValueFormat;
use panproto_inst::{
    CheckKind, ExternalVerdicts, FieldPresence, GInstance, Node, ValidationBudget,
    ValidationOptions, ValidationPlan, Value, ViewRef,
};
use panproto_schema::{Constraint, ConstraintSemantics, Protocol, Schema, Vertex};

/// The status of validating one string value against `format` declared by
/// the AT Protocol Lexicon (the format family `atproto`).
fn status(format: &str, value: &str) -> CheckStatus {
    status_in("atproto", format, value)
}

/// The status of validating one string value against `format` declared
/// under the format family `family`.
fn status_in(family: &str, format: &str, value: &str) -> CheckStatus {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "v".into(),
        Vertex {
            id: "v".into(),
            kind: "string".into(),
            nsid: None,
        },
    );
    schema.constraints.insert(
        "v".into(),
        vec![Constraint {
            sort: "format".into(),
            value: format.into(),
        }],
    );
    let protocol = Protocol {
        name: "test".into(),
        constraint_sorts: vec!["format".into()],
        constraint_semantics: std::iter::once((
            "format".to_owned(),
            ConstraintSemantics::Format(family.into()),
        ))
        .collect(),
        ..Protocol::default()
    };
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    assert_eq!(
        plan.static_findings(),
        &[],
        "{format} compiles with no finding"
    );
    let instance = GInstance::new()
        .with_node(Node::new(0, "v").with_value(FieldPresence::Present(Value::Str(value.into()))));
    plan.validate_graph(
        &instance,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )
    .status
}

/// Asserts that every value of `valid` passes and every value of `invalid`
/// fails `format` declared by the AT Protocol Lexicon.
fn check(format: &str, valid: &[&str], invalid: &[&str]) {
    check_in("atproto", format, valid, invalid);
}

/// Asserts that every value of `valid` passes and every value of `invalid`
/// fails `format` declared under the format family `family`.
fn check_in(family: &str, format: &str, valid: &[&str], invalid: &[&str]) {
    let mut wrong = Vec::new();
    for value in valid {
        if status_in(family, format, value) != CheckStatus::Passed {
            wrong.push(format!("{family} {format}: {value:?} should pass"));
        }
    }
    for value in invalid {
        if status_in(family, format, value) != CheckStatus::Failed {
            wrong.push(format!("{family} {format}: {value:?} should fail"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_lexicon_formats_are_listed_by_their_lexicon_names() {
    for (name, format) in [
        ("at-identifier", ValueFormat::AtIdentifier),
        ("at-uri", ValueFormat::AtUri),
        ("cid", ValueFormat::Cid),
        ("handle", ValueFormat::Handle),
        ("nsid", ValueFormat::Nsid),
        ("tid", ValueFormat::Tid),
        ("record-key", ValueFormat::RecordKey),
        ("language", ValueFormat::Language),
    ] {
        assert_eq!(ValueFormat::named(name), Some(format), "{name}");
        assert_eq!(format.name(), name);
        assert!(ValueFormat::ALL.contains(&format), "{name}");
    }
}

// ---------------------------------------------------------------------------
// The specifications' own examples.
// ---------------------------------------------------------------------------

/// <https://atproto.com/specs/handle>, "Identifier Examples".
#[test]
fn handle_examples_of_the_handle_specification() {
    check(
        "handle",
        &[
            "jay.bsky.social",
            "8.cn",
            "name.t--t",
            "XX.LCS.MIT.EDU",
            "a.co",
            "xn--notarealidn.com",
            "xn--fiqa61au8b7zsevnm8ak20mc4a87e.xn--fiqs8s",
            "xn--ls8h.test",
            "example.t",
            // "Valid syntax, but must always fail resolution due to other
            // restrictions": Lexicon validation accepts them.
            "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion",
            "laptop.local",
            "blah.arpa",
            // "handle.invalid ... is syntactically valid in the Lexicon
            // schema language".
            "handle.invalid",
        ],
        &[
            "jo@hn.test",
            "\u{1F4A9}.test",
            "john..test",
            "xn--bcher-.tld",
            "john.0",
            "cn.8",
            "www.mase\u{0142}kowski.pl.com",
            "org",
            "name.org.",
        ],
    );
}

/// <https://atproto.com/specs/nsid>, "Examples".
#[test]
fn nsid_examples_of_the_nsid_specification() {
    check(
        "nsid",
        &[
            "com.example.fooBar",
            "net.users.bob.ping",
            "a-0.b-1.c",
            "a.b.c",
            "com.example.fooBarV2",
            "cn.8.lex.stuff",
        ],
        &["com.exa\u{1F4A9}ple.thing", "com.example", "com.example.3"],
    );
}

/// <https://atproto.com/specs/tid>, "Examples".
#[test]
fn tid_examples_of_the_tid_specification() {
    check(
        "tid",
        &[
            "3jzfcijpj2z2a",
            "7777777777777",
            "3zzzzzzzzzzzz",
            "2222222222222",
        ],
        &[
            "3jzfcijpj2z21",
            "0000000000000",
            "3JZFCIJPJ2Z2A",
            "3jzfcijpj2z2aa",
            "3jzfcijpj2z2",
            "222",
            "3jzf-cij-pj2z-2a",
            "zzzzzzzzzzzzz",
            "kjzfcijpj2z2a",
        ],
    );
}

/// <https://atproto.com/specs/record-key>, "Examples".
#[test]
fn record_key_examples_of_the_record_key_specification() {
    check(
        "record-key",
        &[
            "3jui7kd54zh2y",
            "self",
            "example.com",
            "~1.2-3_",
            "dHJ1ZQ",
            "pre:fix",
            "_",
        ],
        &[
            "alpha/beta",
            ".",
            "..",
            "#extra",
            "@handle",
            "any space",
            "any+space",
            "number[3]",
            "number(3)",
            "\"quote\"",
            "dHJ1ZQ==",
        ],
    );
}

/// <https://atproto.com/specs/at-uri-scheme>, "Examples": the URIs valid in
/// Lexicon syntax pass, and those valid only in the general syntax or in
/// neither fail.
#[test]
fn at_uri_examples_of_the_at_uri_specification() {
    check(
        "at-uri",
        &[
            "at://foo.com/com.example.foo/123",
            "at://did:plc:ewvi7nxzyoun6zhxrhs64oiz/app.bsky.actor.profile/self",
            "at://did:plc:6msi3pj7krzih5qxqtryxlzw/com.atproto.lexicon.schema/com.atproto.lexicon.schema",
            // The two URIs the specification opens with.
            "at://did:plc:vwzwgnygau7ed7b7wt5ux7y2/app.bsky.feed.post/3k5nobkf2w72g",
            "at://retr0.id/app.bsky.feed.post/3k5nobkf2w72g",
        ],
        &[
            // Valid general syntax, invalid in current Lexicon.
            "at://foo.com/example/123",
            "at://computer",
            "at://example.com:3000",
            // Invalid in both.
            "at://foo.com/",
            "at://user:pass@foo.com",
        ],
    );
}

/// <https://atproto.com/specs/lexicon>, "at-identifier": a handle or a DID,
/// with the DID examples of <https://atproto.com/specs/did> ("Valid DID
/// syntax (would pass Lexicon syntax validation), but unsupported DID
/// method" included) and its invalid DIDs.
#[test]
fn at_identifier_examples_of_the_lexicon_and_did_specifications() {
    check(
        "at-identifier",
        &[
            "jay.bsky.social",
            "did:plc:ewvi7nxzyoun6zhxrhs64oiz",
            "did:web:user.example.com",
            "did:method:val:two",
            "did:m:v",
            "did:method::::val",
            "did:method:-:_:.",
            "did:key:zQ3shZc2QzApp2oymGvQbzP8eKheVshBHbU4ZYjeXqwSKEn6N",
        ],
        &[
            "did:METHOD:val",
            "did:m123:val",
            "DID:method:val",
            "did:method:",
            "did:method:val/two",
            "did:method:val?two",
            "did:method:val#two",
        ],
    );
}

/// The Lexicon's `cid` format delegates to the data model's blessed CID
/// set (<https://atproto.com/specs/data-model>, "Link and CID Formats"):
/// `CIDv1`, raw or DRISL/dag-cbor, SHA-256 with a 32-byte digest, in canonical
/// lowercase unpadded base32 with the `b` prefix.
#[test]
fn cid_examples_of_the_atproto_data_model() {
    let link = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    check(
        "cid",
        &[
            link,
            // The raw-codec CIDv1 for the bytes `hello`.
            "bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq",
        ],
        &[
            &link.to_uppercase(),
            &format!("B{}", &link[1..]),
            "zb2rhe5P4gXftAwvA4eXQ5HJwsER2owDyS9sKaQRRVQPn93bA",
            "QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR",
            "",
            "b",
            "B",
        ],
    );
}

/// <https://atproto.com/specs/lexicon>, "language", and RFC 5646.
#[test]
fn language_examples_of_the_lexicon_specification_and_rfc_5646() {
    check(
        "language",
        &[
            // The Lexicon specification's examples.
            "ja",
            "ban",
            "pt-BR",
            "hy-Latn-IT-arevela",
            // RFC 5646 Appendix A.
            "de",
            "fr",
            "i-enochian",
            "zh-Hant",
            "zh-Hans",
            "sr-Cyrl",
            "sr-Latn",
            "zh-cmn-Hans-CN",
            "cmn-Hans-CN",
            "zh-yue-HK",
            "yue-HK",
            "zh-Hans-CN",
            "sr-Latn-RS",
            "sl-rozaj",
            "sl-rozaj-biske",
            "sl-nedis",
            "de-CH-1901",
            "sl-IT-nedis",
            "hy-Latn-IT-arevela",
            "de-DE",
            "en-US",
            "es-419",
            "de-CH-x-phonebk",
            "az-Arab-x-AZE-derbend",
            "x-whatever",
            "qaa-Qaaa-QM-x-southern",
            "de-Qaaa",
            "sr-Latn-QM",
            "sr-Qaaa-RS",
            "en-US-u-islamcal",
            "zh-CN-a-myext-x-private",
            "en-a-myext-b-another",
            // Appendix A, "Some Invalid Tags": two extensions with the same
            // singleton is well-formed and not valid, and the Lexicon asks
            // only for well-formed tags.
            "ar-a-aaa-b-bbb-a-ccc",
            // Section 2.1.1: case carries no meaning.
            "MN-cYRL-mn",
        ],
        &[
            // Appendix A, "Some Invalid Tags": two region subtags, which
            // no production allows, and a singleton in primary position.
            "de-419-DE",
            "a-DE",
            "",
            "-",
            "en-",
            "en--US",
            "e",
            "en_US",
            "toolonglang",
            "en-x",
            "en-a",
            "en-a-x-foo",
            "x",
            "en-US-",
            " en",
            "en ",
            "\u{0130}t",
        ],
    );
}

// ---------------------------------------------------------------------------
// The interop test vectors.
// ---------------------------------------------------------------------------

fn vectors(file: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/atproto-interop/syntax")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Runs the valid and invalid vectors of `stem` against `format`, apart from
/// the listed exceptions, which are asserted to take the opposite verdict.
fn interop(format: &str, stem: &str, valid_but_refused: &[&str], invalid_but_accepted: &[&str]) {
    let valid = vectors(&format!("{stem}_syntax_valid.txt"));
    let invalid = vectors(&format!("{stem}_syntax_invalid.txt"));
    for exception in valid_but_refused {
        assert!(
            valid.iter().any(|v| v == exception),
            "{exception:?} is a valid vector"
        );
    }
    for exception in invalid_but_accepted {
        assert!(
            invalid.iter().any(|v| v == exception),
            "{exception:?} is an invalid vector"
        );
    }
    let pass: Vec<&str> = valid
        .iter()
        .map(String::as_str)
        .filter(|v| !valid_but_refused.contains(v))
        .chain(invalid_but_accepted.iter().copied())
        .collect();
    let fail: Vec<&str> = invalid
        .iter()
        .map(String::as_str)
        .filter(|v| !invalid_but_accepted.contains(v))
        .chain(valid_but_refused.iter().copied())
        .collect();
    check(format, &pass, &fail);
}

#[test]
fn handle_interop_vectors() {
    interop("handle", "handle", &[], &[]);
}

/// One valid vector has a domain authority of 283 characters, over the
/// NSID specification's limit ("Domain authority: ... at most 253
/// characters (including periods)"), though under the overall limit of 317.
#[test]
fn nsid_interop_vectors() {
    let long: Vec<String> = vectors("nsid_syntax_valid.txt")
        .into_iter()
        .filter(|v| v.len() == 287)
        .collect();
    assert_eq!(long.len(), 1, "one valid vector has 287 characters");
    let authority: Vec<&str> = long
        .iter()
        .filter_map(|v| v.rsplit_once('.'))
        .map(|(a, _)| a)
        .collect();
    assert_eq!(authority.iter().map(|a| a.len()).collect::<Vec<_>>(), [283]);
    let long: Vec<&str> = long.iter().map(String::as_str).collect();
    interop("nsid", "nsid", &long, &[]);
}

#[test]
fn tid_interop_vectors() {
    interop("tid", "tid", &[], &[]);
}

#[test]
fn record_key_interop_vectors() {
    interop("record-key", "recordkey", &[], &[]);
}

#[test]
fn at_uri_interop_vectors() {
    interop("at-uri", "aturi", &[], &[]);
}

#[test]
fn at_identifier_interop_vectors() {
    interop("at-identifier", "atidentifier", &[], &[]);
}

/// Two invalid vectors are well-formed under RFC 5646's ABNF, which the
/// Lexicon specification makes the test ("must validate as a well-formed
/// language tag, as defined in the RFC"): `jaja` is a `language` subtag of
/// `4ALPHA` ("reserved for future use"), and `JA` is `ja` in another case
/// (section 2.1.1: "language tags and their subtags ... are to be treated
/// as case insensitive").
#[test]
fn language_interop_vectors() {
    interop("language", "language", &[], &["jaja", "JA"]);
}

/// The parse-invalid vectors repeat a variant or a singleton: well-formed,
/// not valid (RFC 5646 section 2.2.9), so Lexicon validation accepts them
/// and leaves them to the application, as the Lexicon specification says
/// ("Clients should ignore language strings which are 'well-formed' but
/// not 'valid'").
#[test]
fn language_tags_that_are_well_formed_but_not_valid_pass() {
    let tags = vectors("language_parse_invalid.txt");
    assert_eq!(tags.len(), 4);
    let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
    check("language", &tags, &[]);
}

/// The upstream interop corpus predates the data model's blessed CID
/// contract. Its five decodable CIDs use the unblessed dag-pb codec, and
/// three other entries place the text `<multihash>` where a multihash must
/// occur, so all eight are refused:
///
/// - `mBcDxtdWx0aWhhc2g+` has 17 characters after its base 64 prefix,
///   a length no RFC 4648 base 64 encoding has (section 4: the final
///   quantum is 8, 16 or 24 bits);
/// - `z7x3CtScH765HvShXT` decodes to `01 70 3c 6d ...`: version 1, codec
///   `dag-pb`, then a multihash of code `0x3c` (`<`) whose digest size
///   `0x6d` (`m`, 109) is not the 9 bytes left ("The `<digest-length>` MUST
///   consume the remaining bytes exactly");
/// - `7134036155352661643226414134664076` is `7` and the octal numeral of
///   those bytes, not their multibase base 8 encoding (3-bit groups from
///   the most significant bit of the first byte), which decodes to a
///   leading `0x2e`, not version 1.
#[test]
fn cid_interop_vectors() {
    interop(
        "cid",
        "cid",
        &[
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            "zdj7WWeQ43G6JJvLWQWZpyHuAMq6uYWRjkBXFad11vE2LHhQ7",
            "bafybeie5gq4jxvzmsym6hjlwxej4rwdoxt7wadqvmmwbqi7r27fclha2va",
            "mBcDxtdWx0aWhhc2g+",
            "z7x3CtScH765HvShXT",
            "zdj7WhuEjrB52m1BisYCtmjH1hSKa7yZ3jEZ9JcXaFRD51wVz",
            "7134036155352661643226414134664076",
            "f017012202c5f688262e0ece8569aa6f94d60aad55ca8d9d83734e4a7430d0cff6588ec2b",
        ],
        &[],
    );
}

/// The three valid vectors refused above are refused for the reasons given:
/// each is the multibase encoding of a `CIDv1` header and the ASCII text
/// `<multihash>`, under some reading of its base.
#[test]
fn the_refused_cid_vectors_encode_a_placeholder_for_the_multihash() {
    let mut placeholder = vec![0x01, 0x70];
    placeholder.extend_from_slice(b"<multihash>");
    assert_eq!(
        format!("z{}", base_n(&placeholder, BASE58BTC)),
        "z7x3CtScH765HvShXT"
    );
    // Octal digits of the bytes read as one number, with the version byte's
    // leading zero bits dropped.
    let number = placeholder
        .iter()
        .fold(String::new(), |bits, byte| format!("{bits}{byte:08b}"));
    let number = u128::from_str_radix(&number, 2).expect("104 bits");
    assert_eq!(format!("7{number:o}"), "7134036155352661643226414134664076");
    // Base 64 of the bytes after the version byte, after an extra `B`.
    assert_eq!(
        format!("mB{}", rfc4648(&placeholder[1..], BASE64, 6, None)),
        "mBcDxtdWx0aWhhc2g+"
    );
}

// ---------------------------------------------------------------------------
// Edge cases at the stated limits.
// ---------------------------------------------------------------------------

/// A handle has at most 253 characters, a label at most 63.
#[test]
fn handle_limits() {
    let label63 = "a".repeat(63);
    let at_limit = format!("{}.{}.{}.{}", label63, label63, label63, "b".repeat(61));
    assert_eq!(at_limit.len(), 253);
    let over = format!("{}.{}.{}.{}", label63, label63, label63, "b".repeat(62));
    check(
        "handle",
        &[&at_limit, &format!("{label63}.test"), "a.b"],
        &[
            &over,
            &format!("{}.test", "a".repeat(64)),
            "a.b-",
            "-a.b",
            "a.1b",
            "a",
            "",
            ".",
            "a.b.",
        ],
    );
}

/// An NSID has at most 317 characters, an authority at most 253 and a name
/// at most 63.
#[test]
fn nsid_limits() {
    let label63 = "a".repeat(63);
    let authority = format!("{}.{}.{}.{}", label63, label63, label63, "b".repeat(61));
    assert_eq!(authority.len(), 253);
    let at_limit = format!("{authority}.{}", "c".repeat(63));
    assert_eq!(at_limit.len(), 317);
    check(
        "nsid",
        &[&at_limit, "a.b.c0"],
        &[
            &format!("{authority}.{}", "c".repeat(64)),
            &format!("{authority}b.c"),
            "a.b.c-d",
            "a.b.c_d",
            "1a.b.c",
            "a.b.",
            "a..c",
            "",
        ],
    );
}

/// The subtag limits of the RFC 5646 section 2.1 ABNF: a language of 2 to
/// 8 letters, at most three extended language subtags, a region of two
/// letters or three digits, a variant of 5 to 8 alphanumerics or a digit
/// and three alphanumerics, extension subtags of 2 to 8 alphanumerics and
/// private use subtags of 1 to 8.
#[test]
fn language_limits() {
    check(
        "language",
        &[
            "abcdefgh",
            "zh-abc-def-ghi",
            "de-199",
            "de-1996",
            "de-abcdefgh",
            "en-a-bc-defghijk",
            "en-x-a-abcdefgh",
            "I-AMI",
            "zh-min-nan",
            "en-GB-oed",
        ],
        &[
            "abcdefghi",
            "zh-abc-def-ghi-jkl",
            "de-19a",
            "de-abcdefghi",
            "en-a-b",
            "en-a-bc-defghijkl",
            "en-x-abcdefghi",
            "en-GB-oed-x",
            "123",
        ],
    );
}

/// A record key has 1 to 512 characters.
#[test]
fn record_key_limits() {
    check(
        "record-key",
        &[&"a".repeat(512), "a", "...", ".a"],
        &[&"a".repeat(513), "", "a%20", "\u{00E9}"],
    );
}

/// An AT URI has at most 8 KiB; a DID authority at most 2048 characters.
#[test]
fn at_uri_limits() {
    let did_at_limit = format!("did:plc:{}", "a".repeat(2048 - 8));
    assert_eq!(did_at_limit.len(), 2048);
    check(
        "at-identifier",
        &[&did_at_limit],
        &[&format!("{did_at_limit}a")],
    );
    check(
        "at-uri",
        &[
            &format!("at://{did_at_limit}/a.b.c/{}", "k".repeat(512)),
            "at://a.b/c.d.e/self",
            "at://did:web:localhost%3A1234",
        ],
        &[
            &format!("at://{did_at_limit}a"),
            "at://a.b/c.d.e/self?x=1",
            "at://a.b/c.d.e/self#frag",
            "at://a.b/c.d.e/self/",
            "at://a.b//c.d.e",
            "at://",
            "at:///c.d.e",
            "AT://a.b",
        ],
    );
}

// ---------------------------------------------------------------------------
// The CID string form, encoded independently of the validator.
// ---------------------------------------------------------------------------

const BASE16_LOWER: &str = "0123456789abcdef";
const BASE32: &str = "abcdefghijklmnopqrstuvwxyz234567";
const BASE32HEX: &str = "0123456789abcdefghijklmnopqrstuv";
const BASE32Z: &str = "ybndrfg8ejkmcpqxot1uwisza345h769";
const BASE64: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64URL: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const BASE36: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
const BASE58BTC: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE58FLICKR: &str = "123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
const BASE45: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// The base256emoji alphabet, byte value 0 first, as the multibase
/// `Base256Emoji` specification tabulates it.
const BASE256EMOJI: [char; 256] = [
    '\u{1F680}',
    '\u{1FA90}',
    '\u{2604}',
    '\u{1F6F0}',
    '\u{1F30C}',
    '\u{1F311}',
    '\u{1F312}',
    '\u{1F313}',
    '\u{1F314}',
    '\u{1F315}',
    '\u{1F316}',
    '\u{1F317}',
    '\u{1F318}',
    '\u{1F30D}',
    '\u{1F30F}',
    '\u{1F30E}',
    '\u{1F409}',
    '\u{2600}',
    '\u{1F4BB}',
    '\u{1F5A5}',
    '\u{1F4BE}',
    '\u{1F4BF}',
    '\u{1F602}',
    '\u{2764}',
    '\u{1F60D}',
    '\u{1F923}',
    '\u{1F60A}',
    '\u{1F64F}',
    '\u{1F495}',
    '\u{1F62D}',
    '\u{1F618}',
    '\u{1F44D}',
    '\u{1F605}',
    '\u{1F44F}',
    '\u{1F601}',
    '\u{1F525}',
    '\u{1F970}',
    '\u{1F494}',
    '\u{1F496}',
    '\u{1F499}',
    '\u{1F622}',
    '\u{1F914}',
    '\u{1F606}',
    '\u{1F644}',
    '\u{1F4AA}',
    '\u{1F609}',
    '\u{263A}',
    '\u{1F44C}',
    '\u{1F917}',
    '\u{1F49C}',
    '\u{1F614}',
    '\u{1F60E}',
    '\u{1F607}',
    '\u{1F339}',
    '\u{1F926}',
    '\u{1F389}',
    '\u{1F49E}',
    '\u{270C}',
    '\u{2728}',
    '\u{1F937}',
    '\u{1F631}',
    '\u{1F60C}',
    '\u{1F338}',
    '\u{1F64C}',
    '\u{1F60B}',
    '\u{1F497}',
    '\u{1F49A}',
    '\u{1F60F}',
    '\u{1F49B}',
    '\u{1F642}',
    '\u{1F493}',
    '\u{1F929}',
    '\u{1F604}',
    '\u{1F600}',
    '\u{1F5A4}',
    '\u{1F603}',
    '\u{1F4AF}',
    '\u{1F648}',
    '\u{1F447}',
    '\u{1F3B6}',
    '\u{1F612}',
    '\u{1F92D}',
    '\u{2763}',
    '\u{1F61C}',
    '\u{1F48B}',
    '\u{1F440}',
    '\u{1F62A}',
    '\u{1F611}',
    '\u{1F4A5}',
    '\u{1F64B}',
    '\u{1F61E}',
    '\u{1F629}',
    '\u{1F621}',
    '\u{1F92A}',
    '\u{1F44A}',
    '\u{1F973}',
    '\u{1F625}',
    '\u{1F924}',
    '\u{1F449}',
    '\u{1F483}',
    '\u{1F633}',
    '\u{270B}',
    '\u{1F61A}',
    '\u{1F61D}',
    '\u{1F634}',
    '\u{1F31F}',
    '\u{1F62C}',
    '\u{1F643}',
    '\u{1F340}',
    '\u{1F337}',
    '\u{1F63B}',
    '\u{1F613}',
    '\u{2B50}',
    '\u{2705}',
    '\u{1F97A}',
    '\u{1F308}',
    '\u{1F608}',
    '\u{1F918}',
    '\u{1F4A6}',
    '\u{2714}',
    '\u{1F623}',
    '\u{1F3C3}',
    '\u{1F490}',
    '\u{2639}',
    '\u{1F38A}',
    '\u{1F498}',
    '\u{1F620}',
    '\u{261D}',
    '\u{1F615}',
    '\u{1F33A}',
    '\u{1F382}',
    '\u{1F33B}',
    '\u{1F610}',
    '\u{1F595}',
    '\u{1F49D}',
    '\u{1F64A}',
    '\u{1F639}',
    '\u{1F5E3}',
    '\u{1F4AB}',
    '\u{1F480}',
    '\u{1F451}',
    '\u{1F3B5}',
    '\u{1F91E}',
    '\u{1F61B}',
    '\u{1F534}',
    '\u{1F624}',
    '\u{1F33C}',
    '\u{1F62B}',
    '\u{26BD}',
    '\u{1F919}',
    '\u{2615}',
    '\u{1F3C6}',
    '\u{1F92B}',
    '\u{1F448}',
    '\u{1F62E}',
    '\u{1F646}',
    '\u{1F37B}',
    '\u{1F343}',
    '\u{1F436}',
    '\u{1F481}',
    '\u{1F632}',
    '\u{1F33F}',
    '\u{1F9E1}',
    '\u{1F381}',
    '\u{26A1}',
    '\u{1F31E}',
    '\u{1F388}',
    '\u{274C}',
    '\u{270A}',
    '\u{1F44B}',
    '\u{1F630}',
    '\u{1F928}',
    '\u{1F636}',
    '\u{1F91D}',
    '\u{1F6B6}',
    '\u{1F4B0}',
    '\u{1F353}',
    '\u{1F4A2}',
    '\u{1F91F}',
    '\u{1F641}',
    '\u{1F6A8}',
    '\u{1F4A8}',
    '\u{1F92C}',
    '\u{2708}',
    '\u{1F380}',
    '\u{1F37A}',
    '\u{1F913}',
    '\u{1F619}',
    '\u{1F49F}',
    '\u{1F331}',
    '\u{1F616}',
    '\u{1F476}',
    '\u{1F974}',
    '\u{25B6}',
    '\u{27A1}',
    '\u{2753}',
    '\u{1F48E}',
    '\u{1F4B8}',
    '\u{2B07}',
    '\u{1F628}',
    '\u{1F31A}',
    '\u{1F98B}',
    '\u{1F637}',
    '\u{1F57A}',
    '\u{26A0}',
    '\u{1F645}',
    '\u{1F61F}',
    '\u{1F635}',
    '\u{1F44E}',
    '\u{1F932}',
    '\u{1F920}',
    '\u{1F927}',
    '\u{1F4CC}',
    '\u{1F535}',
    '\u{1F485}',
    '\u{1F9D0}',
    '\u{1F43E}',
    '\u{1F352}',
    '\u{1F617}',
    '\u{1F911}',
    '\u{1F30A}',
    '\u{1F92F}',
    '\u{1F437}',
    '\u{260E}',
    '\u{1F4A7}',
    '\u{1F62F}',
    '\u{1F486}',
    '\u{1F446}',
    '\u{1F3A4}',
    '\u{1F647}',
    '\u{1F351}',
    '\u{2744}',
    '\u{1F334}',
    '\u{1F4A3}',
    '\u{1F438}',
    '\u{1F48C}',
    '\u{1F4CD}',
    '\u{1F940}',
    '\u{1F922}',
    '\u{1F445}',
    '\u{1F4A1}',
    '\u{1F4A9}',
    '\u{1F450}',
    '\u{1F4F8}',
    '\u{1F47B}',
    '\u{1F910}',
    '\u{1F92E}',
    '\u{1F3BC}',
    '\u{1F975}',
    '\u{1F6A9}',
    '\u{1F34E}',
    '\u{1F34A}',
    '\u{1F47C}',
    '\u{1F48D}',
    '\u{1F4E3}',
    '\u{1F942}',
];

/// RFC 4648 bit packing: `bits` bits per symbol of `alphabet`, most
/// significant first, the last symbol filled with zero bits, then `=` up to
/// a multiple of `pad` symbols when `pad` is given.
fn rfc4648(bytes: &[u8], alphabet: &str, bits: u32, pad: Option<usize>) -> String {
    let symbols: Vec<char> = alphabet.chars().collect();
    let mask = (1u32 << bits) - 1;
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut held = 0;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        held += 8;
        while held >= bits {
            held -= bits;
            out.push(symbols[((buffer >> held) & mask) as usize]);
        }
        buffer &= (1 << held) - 1;
    }
    if held > 0 {
        out.push(symbols[((buffer << (bits - held)) & mask) as usize]);
    }
    if let Some(block) = pad {
        while out.chars().count() % block != 0 {
            out.push('=');
        }
    }
    out
}

/// The bits of `bytes`, eight binary digits per byte, most significant
/// first.
fn binary_digits(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut digits, byte| {
        // Writing to a `String` cannot fail.
        let _ = write!(digits, "{byte:08b}");
        digits
    })
}

/// A big-endian base conversion over `alphabet` that writes each leading
/// zero byte as the alphabet's first symbol (the base 10, base 36 and
/// base 58 rule).
fn base_n(bytes: &[u8], alphabet: &str) -> String {
    let symbols: Vec<char> = alphabet.chars().collect();
    let base = u32::try_from(symbols.len()).expect("small alphabet");
    let zeros = bytes.iter().take_while(|b| **b == 0).count();
    let mut digits: Vec<u32> = Vec::new();
    for byte in &bytes[zeros..] {
        let mut carry = u32::from(*byte);
        for digit in &mut digits {
            carry += *digit * 256;
            *digit = carry % base;
            carry /= base;
        }
        while carry > 0 {
            digits.push(carry % base);
            carry /= base;
        }
    }
    std::iter::repeat_n(symbols[0], zeros)
        .chain(digits.iter().rev().map(|d| symbols[*d as usize]))
        .collect()
}

/// RFC 9285 base 45: two bytes as three symbols, least significant first;
/// a last odd byte as two.
fn base45(bytes: &[u8]) -> String {
    let symbols: Vec<char> = BASE45.chars().collect();
    let mut out = String::new();
    for chunk in bytes.chunks(2) {
        let (mut n, width) = match chunk {
            [a, b] => (u32::from(*a) * 256 + u32::from(*b), 3),
            [a] => (u32::from(*a), 2),
            _ => unreachable!("chunks of two"),
        };
        for _ in 0..width {
            out.push(symbols[(n % 45) as usize]);
            n /= 45;
        }
    }
    out
}

/// The multibase proquint body: `ro-` and one five-letter proquint per
/// 16-bit word, joined by `-`.
fn proquint(bytes: &[u8]) -> String {
    const CONSONANTS: &[u8] = b"bdfghjklmnprstvz";
    const VOWELS: &[u8] = b"aiou";
    let words: Vec<String> = bytes
        .chunks(2)
        .map(|w| {
            let n = usize::from(u16::from_be_bytes([w[0], w[1]]));
            [
                CONSONANTS[(n >> 12) & 15],
                VOWELS[(n >> 10) & 3],
                CONSONANTS[(n >> 6) & 15],
                VOWELS[(n >> 4) & 3],
                CONSONANTS[n & 15],
            ]
            .iter()
            .map(|b| char::from(*b))
            .collect()
        })
        .collect();
    format!("ro-{}", words.join("-"))
}

/// The string forms of `bytes` in every encoding of the multibase table
/// that is not reserved, each with its prefix. Proquint encodes 16-bit
/// words and defines no form for an odd number of bytes, so it is left out
/// for those.
fn multibase_forms(bytes: &[u8]) -> Vec<(&'static str, String)> {
    let base2 = binary_digits(bytes);
    let base10 = base_n(bytes, "0123456789");
    let mut forms = vec![
        ("base2", format!("0{base2}")),
        ("base8", format!("7{}", rfc4648(bytes, "01234567", 3, None))),
        ("base10", format!("9{base10}")),
        (
            "base16",
            format!("f{}", rfc4648(bytes, BASE16_LOWER, 4, None)),
        ),
        (
            "base16upper",
            format!("F{}", rfc4648(bytes, BASE16_LOWER, 4, None).to_uppercase()),
        ),
        (
            "base32hex",
            format!("v{}", rfc4648(bytes, BASE32HEX, 5, None)),
        ),
        (
            "base32hexupper",
            format!("V{}", rfc4648(bytes, BASE32HEX, 5, None).to_uppercase()),
        ),
        (
            "base32hexpad",
            format!("t{}", rfc4648(bytes, BASE32HEX, 5, Some(8))),
        ),
        (
            "base32hexpadupper",
            format!("T{}", rfc4648(bytes, BASE32HEX, 5, Some(8)).to_uppercase()),
        ),
        ("base32", format!("b{}", rfc4648(bytes, BASE32, 5, None))),
        (
            "base32upper",
            format!("B{}", rfc4648(bytes, BASE32, 5, None).to_uppercase()),
        ),
        (
            "base32pad",
            format!("c{}", rfc4648(bytes, BASE32, 5, Some(8))),
        ),
        (
            "base32padupper",
            format!("C{}", rfc4648(bytes, BASE32, 5, Some(8)).to_uppercase()),
        ),
        ("base32z", format!("h{}", rfc4648(bytes, BASE32Z, 5, None))),
        ("base36", format!("k{}", base_n(bytes, BASE36))),
        (
            "base36upper",
            format!("K{}", base_n(bytes, BASE36).to_uppercase()),
        ),
        ("base45", format!("R{}", base45(bytes))),
        ("base58btc", format!("z{}", base_n(bytes, BASE58BTC))),
        ("base58flickr", format!("Z{}", base_n(bytes, BASE58FLICKR))),
        ("base64", format!("m{}", rfc4648(bytes, BASE64, 6, None))),
        (
            "base64pad",
            format!("M{}", rfc4648(bytes, BASE64, 6, Some(4))),
        ),
        (
            "base64url",
            format!("u{}", rfc4648(bytes, BASE64URL, 6, None)),
        ),
        (
            "base64urlpad",
            format!("U{}", rfc4648(bytes, BASE64URL, 6, Some(4))),
        ),
        (
            "base256emoji",
            std::iter::once('\u{1F680}')
                .chain(bytes.iter().map(|b| BASE256EMOJI[usize::from(*b)]))
                .collect(),
        ),
    ];
    if bytes.len() % 2 == 0 {
        forms.push(("proquint", format!("p{}", proquint(bytes))));
    }
    forms
}

/// The binary form of a CID: `head` and then `digest_len` bytes of
/// digest.
fn cid_bytes(head: &[u8], digest_len: usize) -> Vec<u8> {
    let mut bytes = head.to_vec();
    bytes.extend(std::iter::repeat_n(0xab, digest_len));
    bytes
}

/// The string form of a binary CID in lowercase base 32 with `b`.
fn cid_string(head: &[u8], digest_len: usize) -> String {
    format!(
        "b{}",
        rfc4648(&cid_bytes(head, digest_len), BASE32, 5, None)
    )
}

/// The test encoders agree with the examples of the specifications they
/// follow.
#[test]
fn the_test_encoders_reproduce_the_specification_examples() {
    // The multibase README, "Multibase By Example".
    let text = b"Multibase is awesome! \\o/";
    assert_eq!(
        rfc4648(text, BASE32, 5, None).to_uppercase(),
        "JV2WY5DJMJQXGZJANFZSAYLXMVZW63LFEEQFY3ZP"
    );
    assert_eq!(
        base_n(text, BASE36).to_uppercase(),
        "3IY8QKL64VUGCX009XWUHKF6GBBTS3TVRXFRA5R"
    );
    assert_eq!(base45(text), "TZ9:VDNEDHECDZC+ED944A4FVQEF$DK84%UB21");
    assert_eq!(
        base_n(text, BASE58BTC),
        "YAjKoNbau5KiqmHPmSxYCvn66dA1vLmwbt"
    );
    assert_eq!(
        rfc4648(text, BASE64, 6, Some(4)),
        "TXVsdGliYXNlIGlzIGF3ZXNvbWUhIFxvLw=="
    );
    // Base10 and Base36 examples.
    assert_eq!(base_n(&[0x00, 0x00, 0xff], "0123456789"), "00255");
    assert_eq!(base_n(&[0x00, 0x01, 0x00], BASE36), "074");
    // RFC 9285 section 4.3.
    assert_eq!(base45(b"AB"), "BB8");
    assert_eq!(base45(b"Hello!!"), "%69 VD92EX0");
    // The proquint paper and the multibase Proquint specification.
    assert_eq!(proquint(&[127, 0, 0, 1]), "ro-lusab-babad");
    // The CID specification's example: the CIDv1 of the raw bytes hello.
    let hello = [
        0x01, 0x55, 0x12, 0x20, 0x2c, 0xf2, 0x4d, 0xba, 0x5f, 0xb0, 0xa3, 0x0e, 0x26, 0xe8, 0x3b,
        0x2a, 0xc5, 0xb9, 0xe2, 0x9e, 0x1b, 0x16, 0x1e, 0x5c, 0x1f, 0xa7, 0x42, 0x5e, 0x73, 0x04,
        0x33, 0x62, 0x93, 0x8b, 0x98, 0x24,
    ];
    assert_eq!(
        format!("b{}", rfc4648(&hello, BASE32, 5, None)),
        "bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq"
    );
}

/// The two binary CID forms blessed by the AT Protocol data model.
fn blessed_cid_binaries() -> Vec<Vec<u8>> {
    vec![
        cid_bytes(&[0x01, 0x71, 0x12, 0x20], 32),
        cid_bytes(&[0x01, 0x55, 0x12, 0x20], 32),
    ]
}

/// Binary CIDs outside the blessed contract: wrong version, codec, hash,
/// digest size or total length, including `CIDv0` and trailing bytes.
fn non_blessed_cid_binaries() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("version 0", cid_bytes(&[0x00, 0x71, 0x12, 0x20], 32)),
        ("version 2", cid_bytes(&[0x02, 0x71, 0x12, 0x20], 32)),
        ("a CIDv0 multihash", cid_bytes(&[0x12, 0x20], 32)),
        ("the dag-pb codec", cid_bytes(&[0x01, 0x70, 0x12, 0x20], 32)),
        (
            "the dag-json codec",
            cid_bytes(&[0x01, 0xa9, 0x02, 0x12, 0x20], 32),
        ),
        ("SHA-512", cid_bytes(&[0x01, 0x55, 0x13, 0x20], 32)),
        ("an identity hash", cid_bytes(&[0x01, 0x55, 0x00, 0x20], 32)),
        (
            "a declared 31-byte digest",
            cid_bytes(&[0x01, 0x71, 0x12, 0x1f], 32),
        ),
        ("a short digest", cid_bytes(&[0x01, 0x71, 0x12, 0x20], 31)),
        ("trailing bytes", cid_bytes(&[0x01, 0x71, 0x12, 0x20], 33)),
    ]
}

/// Only blessed bytes in canonical lowercase base32 pass. Every other
/// multibase encoding and every non-blessed binary CID fails.
#[test]
fn only_blessed_cids_in_canonical_base32_pass() {
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for bytes in blessed_cid_binaries() {
        for (base, form) in multibase_forms(&bytes) {
            if base == "base32" {
                valid.push(form);
            } else {
                invalid.push(form);
            }
        }
    }
    for (_, bytes) in non_blessed_cid_binaries() {
        for (_, form) in multibase_forms(&bytes) {
            invalid.push(form);
        }
    }
    let valid: Vec<&str> = valid.iter().map(String::as_str).collect();
    let invalid: Vec<&str> = invalid.iter().map(String::as_str).collect();
    check("cid", &valid, &invalid);
}

/// Noncanonical spellings and non-base32 multibase encodings are refused,
/// even where the underlying multibase specification would decode them.
#[test]
fn noncanonical_and_other_multibase_forms_fail() {
    let link = cid_bytes(&[0x01, 0x71, 0x12, 0x20], 32);
    let base32 = rfc4648(&link, BASE32, 5, None);
    let mixed: String = base32
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect();
    let hex = rfc4648(&link, BASE16_LOWER, 4, None);
    let binary = binary_digits(&link);
    let base8 = rfc4648(&link, "01234567", 3, None);
    check(
        "cid",
        &[],
        &[
            &format!("b{mixed}"),
            &format!("B{mixed}"),
            &format!("B{base32}"),
            &format!("b{}", base32.to_uppercase()),
            &format!("F{hex}"),
            &format!("f{}", hex.to_uppercase()),
            &format!("v{}", rfc4648(&link, BASE32HEX, 5, None).to_uppercase()),
            &format!("C{}", rfc4648(&link, BASE32, 5, Some(8))),
            &format!("K{}", base_n(&link, BASE36)),
            &format!("k{}", base_n(&link, BASE36).to_uppercase()),
            // The version byte 0x01 written without its seven leading
            // zeros.
            &format!("0{}", binary.trim_start_matches('0')),
            &format!("7{base8}1"),
            &format!("7{base8}77"),
        ],
    );
}

/// Strings that are no multibase encoding of a `CIDv1`, each refused by the
/// rule named with it.
#[test]
fn cid_strings_that_no_base_decodes_fail() {
    let link = cid_bytes(&[0x01, 0x71, 0x12, 0x20], 32);
    // 37 bytes, so that base 64 has pad bits and padding.
    let odd = cid_bytes(&[0x01, 0xa9, 0x02, 0x1e, 0x20], 32);
    let base32 = rfc4648(&link, BASE32, 5, None);
    let base64 = rfc4648(&odd, BASE64, 6, None);
    let base64pad = rfc4648(&odd, BASE64, 6, Some(4));
    assert!(base64pad.ends_with("=="));
    let hex = rfc4648(&link, BASE16_LOWER, 4, None);
    let base58 = base_n(&link, BASE58BTC);
    let base45 = base45(&link);
    let proquint = proquint(&link);
    let binary = binary_digits(&link);
    let cid_v0 = cid_bytes(&[0x12, 0x20], 32);
    check(
        "cid",
        &[],
        &[
            // The reserved prefixes and a code point no encoding has.
            &format!("1{base58}"),
            &format!("Q{base58}"),
            &format!("/{base32}"),
            &format!("\u{0}{base32}"),
            &format!("x{base32}"),
            // A CIDv0, bare and multibase-encoded.
            "QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR",
            &format!("z{}", base_n(&cid_v0, BASE58BTC)),
            &format!("b{}", rfc4648(&cid_v0, BASE32, 5, None)),
            // RFC 4648: a symbol outside the alphabet, padding where the
            // base has none, padding missing or wrong where it has some, a
            // length no encoding has, and nonzero pad bits (section 3.5).
            &format!("b{}1", &base32[..base32.len() - 1]),
            &format!("b{base32}="),
            &format!("m{base64}="),
            &format!("M{base64}"),
            &format!("M{base64pad}="),
            &format!("M{}", base64pad.replacen('=', "", 1)),
            &format!("c{base32}"),
            &format!("b{base32}a"),
            &format!("f{hex}0"),
            &format!("f{}g", &hex[..hex.len() - 1]),
            &format!("b{}b", &base32[..base32.len() - 1]),
            &format!("m{}B", &base64[..base64.len() - 1]),
            // URL-safe symbols are not base 64 symbols.
            &format!("m{}-", &base64[..base64.len() - 1]),
            // z-base-32's alphabet is lower case; base 58 has no 0, O, I, l.
            &format!("h{}", rfc4648(&link, BASE32Z, 5, None).to_uppercase()),
            &format!("z0{base58}"),
            &format!("zl{base58}"),
            // Base 2 and base 10 symbols.
            &format!("0{}2", &binary[1..]),
            &format!("9{}a", base_n(&link, "0123456789")),
            // RFC 9285: a triplet over 65535, a pair over 255, a length of
            // 3k + 1, and a lower-case letter.
            &format!("R{base45}GGW"),
            &format!("R{base45}:6"),
            &format!("R{base45}0"),
            &format!("R{}", base45.to_lowercase()),
            // Proquint: the  prefix, five letters per word, consonant
            // and vowel positions, the separator.
            &format!("p{}", &proquint[3..]),
            &format!("p{proquint}-"),
            &format!("pro-{}", proquint[3..].replacen('-', "", 1)),
            &format!("p{}a", &proquint[..proquint.len() - 1]),
            &format!("p{}", proquint.to_uppercase()),
            // A symbol outside the base256emoji alphabet.
            &format!(
                "\u{1F680}{}x",
                link.iter()
                    .map(|b| BASE256EMOJI[usize::from(*b)])
                    .collect::<String>()
            ),
            // Whitespace is not trimmed.
            &format!(" b{base32}"),
            &format!("b{base32} "),
        ],
    );
}

/// A DASL CID decodes to exactly 36 bytes; a shorter or longer string,
/// nonzero pad bits or a character outside the alphabet fails.
#[test]
fn cid_structure() {
    let link = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    // A blob reference: the raw codec.
    let raw = "bafkreichzyovokfzmymz36p5jibbjrhsur6n7hjnzxrpbt5jaydp2szvna";
    check(
        "cid",
        &[link, raw],
        &[
            // One character short and one long.
            &link[..link.len() - 1],
            &format!("{link}a"),
            // The last character changed so that its pad bits are nonzero.
            &format!("{}b", &link[..link.len() - 1]),
            // A character outside the alphabet.
            &format!("{}1{}", &link[..10], &link[11..]),
            &format!("{link}="),
        ],
    );
    // The encoder agrees with the data model's example link.
    let head = cid_string(&[0x01, 0x71, 0x12, 0x20], 0);
    assert_eq!(&link[..7], &head[..7]);
}

// ---------------------------------------------------------------------------
// DIDs: the shared W3C syntax and the AT Protocol rules.
// ---------------------------------------------------------------------------

/// The shared `did` format is the DID syntax of W3C DID Core 1.0
/// (<https://www.w3.org/TR/did-1.0/>, section 3.1, "DID Syntax"):
/// `"did:" method-name ":" method-specific-id`, a method of lowercase
/// letters and digits, and an identifier of colon-separated segments of
/// `ALPHA / DIGIT / "." / "-" / "_" / pct-encoded`, the last nonempty. The
/// literal `"did:"` is an ABNF string, which RFC 5234 section 2.3 makes
/// case insensitive. A format family other than the AT Protocol Lexicon
/// adds nothing, so the atproto DID specification's invalid
/// `did:m123:val` passes here.
#[test]
fn the_shared_did_format_is_the_w3c_did_syntax() {
    for family in ["json-schema", "openapi", "k8s-crd", "rss-atom"] {
        check_in(
            family,
            "did",
            &[
                "did:example:123456789abcdefghi",
                "did:m123:val",
                "did:123:val",
                "did:plc:ewvi7nxzyoun6zhxrhs64oiz",
                "did:web:user.example.com",
                "did:method::::val",
                "did:method:-:_:.",
                "did:web:localhost%3A1234",
                "did:web:localhost%3a1234",
                "did:m:%41%42",
                "DID:example:1",
                "Did:example:1",
                "did:method:VAL",
                &format!("did:plc:{}", "a".repeat(4096)),
            ],
            &[
                "did:METHOD:val",
                "did:Example:1",
                "did:method:",
                "did:method:val:",
                "did:method:val/two",
                "did:method:val?two",
                "did:method:val#two",
                "did:method:val%",
                "did:method:val%4",
                "did:method:val%zz",
                "did:method:val%%41",
                "did::val",
                "did:method",
                "did:",
                "did",
                "did:m:a b",
                "did:m:a+b",
                "did:m:a@b",
                "did:m:caf\u{e9}",
                "did:m\u{e9}:a",
                " did:m:a",
                "did:m:a ",
                "di:m:a",
                "urn:m:a",
            ],
        );
    }
}

/// A `did` declared by an AT Protocol Lexicon is a W3C DID that also meets
/// the atproto DID rules (<https://atproto.com/specs/did>, "DID Identifier
/// Syntax"): it starts with lowercase `did:`, its method is lowercase
/// letters only, and it has at most 2048 characters. The specification's
/// examples take their stated verdicts.
#[test]
fn an_atproto_did_also_meets_the_atproto_did_rules() {
    let at_limit = format!("did:plc:{}", "a".repeat(2048 - 8));
    assert_eq!(at_limit.len(), 2048);
    check(
        "did",
        &[
            "did:plc:ewvi7nxzyoun6zhxrhs64oiz",
            "did:web:user.example.com",
            "did:method:val:two",
            "did:m:v",
            "did:method::::val",
            "did:method:-:_:.",
            "did:key:zQ3shZc2QzApp2oymGvQbzP8eKheVshBHbU4ZYjeXqwSKEn6N",
            "did:web:localhost%3A1234",
            &at_limit,
        ],
        &[
            "did:METHOD:val",
            "did:m123:val",
            "DID:method:val",
            "did:method:",
            "did:method:val/two",
            "did:method:val?two",
            "did:method:val#two",
            "did:123:val",
            "Did:method:val",
            "did:method:val%",
            "did:method:val%zz",
            &format!("{at_limit}a"),
        ],
    );
}

/// The family decides only `did`: under `atproto` and under any other
/// family, every other format takes the same verdict on the DIDs above.
#[test]
fn only_the_did_format_depends_on_the_atproto_family() {
    let values = [
        "did:m123:val",
        "DID:method:val",
        "did:plc:ewvi7nxzyoun6zhxrhs64oiz",
        "at://did:m123:val/a.b.c/self",
        "at://did:plc:ewvi7nxzyoun6zhxrhs64oiz/a.b.c/self",
        "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a",
        "jay.bsky.social",
    ];
    for format in ValueFormat::ALL {
        if format == ValueFormat::Did {
            continue;
        }
        for value in values {
            assert_eq!(
                status_in("atproto", format.name(), value),
                status_in("json-schema", format.name(), value),
                "{} {value:?}",
                format.name()
            );
        }
    }
    assert_eq!(
        status_in("atproto", "did", "did:m123:val"),
        CheckStatus::Failed
    );
    assert_eq!(
        status_in("json-schema", "did", "did:m123:val"),
        CheckStatus::Passed
    );
}

/// The DID inside `at-identifier` and `at-uri` is the Lexicon's `did`:
/// a W3C DID meeting the atproto rules, so a percent sign must start a
/// percent-encoded octet there too.
#[test]
fn the_did_of_an_at_identifier_is_the_lexicon_did() {
    check(
        "at-identifier",
        &["did:web:localhost%3A1234", "did:m:a%4f"],
        &["did:m:a%zz", "did:m:a%4", "did:m:%", "did:m123:val"],
    );
    check(
        "at-uri",
        &["at://did:web:localhost%3A1234/a.b.c/self"],
        &["at://did:m:a%zz/a.b.c/self", "at://did:m:a%4/a.b.c"],
    );
}

/// The check reads the whole lexeme: surrounding whitespace is not
/// trimmed.
#[test]
fn a_format_reads_the_whole_lexeme() {
    assert_eq!(status("tid", "3jzfcijpj2z2a"), CheckStatus::Passed);
    assert_eq!(status("tid", " 3jzfcijpj2z2a"), CheckStatus::Failed);
}

// ---------------------------------------------------------------------------
// Agreement with the executable reference.
// ---------------------------------------------------------------------------

/// The nodes whose value fails `format` under `family` in production, with
/// a check that no node is left undecided.
fn production_failures(family: &str, format: &str, values: &[String]) -> BTreeSet<u32> {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "v".into(),
        Vertex {
            id: "v".into(),
            kind: "string".into(),
            nsid: None,
        },
    );
    schema.constraints.insert(
        "v".into(),
        vec![Constraint {
            sort: "format".into(),
            value: format.into(),
        }],
    );
    let protocol = Protocol {
        name: "test".into(),
        constraint_sorts: vec!["format".into()],
        constraint_semantics: std::iter::once((
            "format".to_owned(),
            ConstraintSemantics::Format(family.into()),
        ))
        .collect(),
        ..Protocol::default()
    };
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let mut instance = GInstance::new();
    for (id, value) in (0u32..).zip(values) {
        instance = instance.with_node(
            Node::new(id, "v").with_value(FieldPresence::Present(Value::Str(value.clone()))),
        );
    }
    let report = plan.validate_graph(
        &instance,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(report.incomplete.is_empty(), "{format}: undecided");
    report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == CheckKind::ValueConstraint)
        .map(|d| match d.subject {
            Some(ViewRef::Node(id)) => id,
            ref other => panic!("{format}: a value finding about {other:?}"),
        })
        .collect()
}

/// The nodes whose value fails `format` under `family` in the reference
/// validation page.
fn reference_failures(family: &str, format: &str, values: &[String]) -> BTreeSet<u32> {
    use panproto_reference::lens::ViewRef as RViewRef;
    use panproto_reference::model::{Graph as RGraph, Node as RNode};
    use panproto_reference::schema::Schema as RSchema;
    use panproto_reference::validation::{self as reference, CheckKind as RKind, Semantics};
    use panproto_reference::value::Value as RValue;

    let mut schema = RSchema::default().vertex("v", "scalar");
    schema.constraints.insert(
        "v".to_owned(),
        vec![("format".to_owned(), format.to_owned())],
    );
    let semantics = BTreeMap::from([("format".to_owned(), Semantics::Format(family.to_owned()))]);
    let graph = RGraph {
        nodes: (0u32..)
            .zip(values)
            .map(|(id, value)| {
                (
                    id,
                    RNode::new(id, "v").with_value(RValue::Str(value.clone())),
                )
            })
            .collect(),
        ..RGraph::default()
    };
    let report = reference::validate(&schema, &semantics, &graph);
    assert!(
        report.incomplete.is_empty(),
        "{format}: undecided in the reference"
    );
    report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == RKind::ValueConstraint)
        .map(|d| match d.subject {
            Some(RViewRef::Node(id)) => id,
            ref other => panic!("{format}: a reference value finding about {other:?}"),
        })
        .collect()
}

/// Asserts that production and the reference give every value of `values`
/// the same verdict for `format` under each family, in batches.
fn agree(format: &str, values: &[String]) {
    let mut values: Vec<String> = values.to_vec();
    values.sort();
    values.dedup();
    let mut disagreements = Vec::new();
    let mut passed = 0usize;
    for family in ["atproto", "json-schema"] {
        for batch in values.chunks(512) {
            let production = production_failures(family, format, batch);
            let reference = reference_failures(family, format, batch);
            passed += batch.len() - production.len();
            for id in production.symmetric_difference(&reference) {
                disagreements.push(format!(
                    "{family} {format} {:?}: production {}, reference {}",
                    batch[*id as usize],
                    if production.contains(id) {
                        "fails"
                    } else {
                        "passes"
                    },
                    if reference.contains(id) {
                        "fails"
                    } else {
                        "passes"
                    },
                ));
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Both verdicts occur, so the agreement is not vacuous.
    assert!(
        passed > 0 && passed < 2 * values.len(),
        "{format}: one verdict only"
    );
}

/// Every line of every interop vector file.
fn all_vectors() -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/atproto-interop/syntax");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    files.sort();
    files
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .filter_map(|path| path.file_name()?.to_str())
        .flat_map(vectors)
        .collect()
}

/// Every string of `alphabet` of length at most `max`, each after every
/// prefix of `prefixes`.
fn strings(prefixes: &[&str], alphabet: &[&str], max: usize) -> Vec<String> {
    let mut layer: Vec<String> = prefixes.iter().map(|p| (*p).to_owned()).collect();
    let mut all = layer.clone();
    for _ in 0..max {
        layer = layer
            .iter()
            .flat_map(|s| alphabet.iter().map(move |a| format!("{s}{a}")))
            .collect();
        all.extend(layer.iter().cloned());
    }
    all
}

/// `seed` and every string one edit from it: a character deleted, replaced
/// by one of `alphabet`, or one of `alphabet` inserted.
fn edits(seed: &str, alphabet: &[char]) -> Vec<String> {
    let chars: Vec<char> = seed.chars().collect();
    let mut out = vec![seed.to_owned()];
    for i in 0..=chars.len() {
        for a in alphabet {
            let mut inserted = chars.clone();
            inserted.insert(i, *a);
            out.push(inserted.into_iter().collect());
            if i < chars.len() {
                let mut replaced = chars.clone();
                replaced[i] = *a;
                out.push(replaced.into_iter().collect());
            }
        }
        if i < chars.len() {
            let mut deleted = chars.clone();
            deleted.remove(i);
            out.push(deleted.into_iter().collect());
        }
    }
    out
}

/// DIDs around every rule: the scheme in each case, methods and
/// identifiers over letters of both cases, digits, the separators, percent
/// signs with and without hexadecimal digits, and characters outside the
/// grammar.
fn did_corpus() -> Vec<String> {
    let mut corpus = strings(
        &[
            "did:", "DID:", "Did:", "did:m:", "did:M:", "did:m1:", "did:1:",
        ],
        &["a", "Z", "7", ":", "%", "F", "g", ".", "-", "_", "/", " "],
        4,
    );
    corpus.push(format!("did:plc:{}", "a".repeat(2040)));
    corpus.push(format!("did:plc:{}", "a".repeat(2041)));
    corpus.extend(all_vectors());
    corpus
}

#[test]
fn the_did_format_agrees_with_the_reference() {
    agree("did", &did_corpus());
}

#[test]
fn the_at_identifier_format_agrees_with_the_reference() {
    let mut corpus = did_corpus();
    corpus.extend(strings(&[""], &["a", "1", "-", ".", "A"], 6));
    agree("at-identifier", &corpus);
}

#[test]
fn the_handle_and_nsid_formats_agree_with_the_reference() {
    let mut corpus = strings(&[""], &["a", "1", "-", ".", "A"], 7);
    corpus.push(format!("{}.{}", "a".repeat(63), "b".repeat(63)));
    corpus.push(format!("{}.{}", "a".repeat(64), "b".repeat(63)));
    corpus.push(format!("{}.c", "a.".repeat(126)));
    corpus.push(format!("{}.c", "a.".repeat(127)));
    corpus.extend(all_vectors());
    agree("handle", &corpus);
    agree("nsid", &corpus);
}

#[test]
fn the_tid_and_record_key_formats_agree_with_the_reference() {
    let mut corpus: Vec<String> = [
        "3jzfcijpj2z2a",
        "2222222222222",
        "7777777777777",
        "jzzzzzzzzzzzz",
    ]
    .iter()
    .flat_map(|seed| {
        edits(
            seed,
            &['2', '7', 'a', 'j', 'k', 'z', 'A', '1', '0', '-', '.'],
        )
    })
    .collect();
    corpus.extend(strings(
        &[""],
        &["a", ".", "~", ":", "%", "_", "-", " ", "\u{e9}"],
        4,
    ));
    corpus.push("a".repeat(512));
    corpus.push("a".repeat(513));
    corpus.extend(all_vectors());
    agree("tid", &corpus);
    agree("record-key", &corpus);
}

#[test]
fn the_at_uri_format_agrees_with_the_reference() {
    let authorities = [
        "a.b",
        "did:plc:x",
        "did:m123:x",
        "did:m:a%zz",
        "did:m:a%4f",
        "DID:m:x",
        "",
        "a",
        "a.b:3000",
    ];
    let collections = ["", "/a.b.c", "/a.b", "/a.b.3", "/", "/a.b.c/"];
    let keys = ["", "/self", "/.", "/..", "/a%20", "/~", "/a/b", "?q", "#f"];
    let mut corpus = Vec::new();
    for authority in authorities {
        for collection in collections {
            for key in keys {
                corpus.push(format!("at://{authority}{collection}{key}"));
                corpus.push(format!("AT://{authority}{collection}{key}"));
            }
        }
    }
    corpus.extend(all_vectors());
    agree("at-uri", &corpus);
}

#[test]
fn the_language_format_agrees_with_the_reference() {
    let subtags = [
        "a",
        "x",
        "i",
        "ab",
        "abc",
        "abcd",
        "abcde",
        "abcdefgh",
        "abcdefghi",
        "1",
        "123",
        "1234",
        "1abc",
        "a1",
        "Latn",
        "oed",
    ];
    let mut corpus = Vec::new();
    let mut layer: Vec<String> = vec![String::new()];
    for _ in 0..4 {
        layer = layer
            .iter()
            .flat_map(|tag| {
                subtags.iter().map(move |s| {
                    if tag.is_empty() {
                        (*s).to_owned()
                    } else {
                        format!("{tag}-{s}")
                    }
                })
            })
            .collect();
        corpus.extend(layer.iter().cloned());
    }
    corpus.extend(
        [
            "",
            "-",
            "en-",
            "en--US",
            "en_US",
            "i-klingon",
            "I-KLINGON",
            "zh-min-nan",
        ]
        .map(str::to_owned),
    );
    corpus.extend(all_vectors());
    agree("language", &corpus);
}

#[test]
fn the_cid_format_agrees_with_the_reference() {
    let mutations = [
        '0',
        '1',
        '7',
        '9',
        'a',
        'b',
        'f',
        'z',
        'A',
        'B',
        'F',
        'Z',
        '=',
        '-',
        '_',
        '+',
        '/',
        ':',
        'Q',
        'm',
        'p',
        'R',
        ' ',
        '\u{1F680}',
        '\u{1F942}',
    ];
    let mut corpus = Vec::new();
    for bytes in blessed_cid_binaries() {
        for (_, form) in multibase_forms(&bytes) {
            corpus.push(form);
        }
    }
    for (_, bytes) in non_blessed_cid_binaries() {
        for (_, form) in multibase_forms(&bytes) {
            corpus.push(form);
        }
    }
    for bytes in [
        cid_bytes(&[0x01, 0x71, 0x12, 0x20], 32),
        cid_bytes(&[0x01, 0x70, 0x12, 0x20], 32),
    ] {
        for (_, form) in multibase_forms(&bytes) {
            corpus.extend(edits(&form, &mutations));
        }
    }
    corpus.extend(all_vectors());
    agree("cid", &corpus);
}
