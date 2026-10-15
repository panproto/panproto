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

use std::path::PathBuf;

use panproto_gat::CheckStatus;
use panproto_inst::validation::ValueFormat;
use panproto_inst::{
    ExternalVerdicts, FieldPresence, GInstance, Node, ValidationBudget, ValidationOptions,
    ValidationPlan, Value,
};
use panproto_schema::{Constraint, ConstraintSemantics, Protocol, Schema, Vertex};

/// The status of validating one string value against `format`.
fn status(format: &str, value: &str) -> CheckStatus {
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
            ConstraintSemantics::Format("atproto".into()),
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
/// fails `format`.
fn check(format: &str, valid: &[&str], invalid: &[&str]) {
    let mut wrong = Vec::new();
    for value in valid {
        if status(format, value) != CheckStatus::Passed {
            wrong.push(format!("{format}: {value:?} should pass"));
        }
    }
    for value in invalid {
        if status(format, value) != CheckStatus::Failed {
            wrong.push(format!("{format}: {value:?} should fail"));
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

/// The data model's example link (<https://atproto.com/specs/data-model>,
/// "JSON Representation") is a DASL CID; a `CIDv0`, the same CID with an
/// upper-case prefix or alphabet, and other multibase encodings are not
/// the blessed string form.
#[test]
fn cid_examples_of_the_data_model_and_dasl_specifications() {
    let link = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    check(
        "cid",
        &[link],
        &[
            "QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR",
            &link.to_uppercase(),
            &format!("B{}", &link[1..]),
            "zdj7WWeQ43G6JJvLWQWZpyHuAMq6uYWRjkBXFad11vE2LHhQ7",
            "",
            "b",
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

/// The CID vectors are the string forms of the IPFS CID specification, of
/// which the data model blesses one: DASL CIDs (version 1, raw or DRISL
/// codec, SHA-256, `b` and lowercase base32; "only a specific 'blessed'
/// set of CID types are allowed", <https://atproto.com/specs/data-model>).
/// Every valid vector is outside that set: the two `bafybei` CIDs have the
/// `dag-pb` codec (`0x70`), and the rest use other multibase encodings or
/// do not decode to a CID.
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

/// Unpadded RFC 4648 base32 in the lowercase alphabet.
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

/// The string form of a CID whose binary form is `head` and then
/// `digest_len` bytes of digest.
fn cid_string(head: &[u8], digest_len: usize) -> String {
    let mut bytes = head.to_vec();
    bytes.extend(std::iter::repeat_n(0xab, digest_len));
    format!("b{}", base32(&bytes))
}

/// Each step of the DASL parse: version 1, codec raw or DRISL, SHA-256,
/// size 32, a 32-byte digest, and (per the CID specification) nothing
/// after it.
#[test]
fn cid_dasl_steps() {
    check(
        "cid",
        &[
            &cid_string(&[0x01, 0x71, 0x12, 0x20], 32),
            &cid_string(&[0x01, 0x55, 0x12, 0x20], 32),
        ],
        &[
            &cid_string(&[0x02, 0x71, 0x12, 0x20], 32),
            &cid_string(&[0x01, 0x70, 0x12, 0x20], 32),
            &cid_string(&[0x01, 0x71, 0x13, 0x20], 32),
            &cid_string(&[0x01, 0x71, 0x1e, 0x20], 32),
            &cid_string(&[0x01, 0x71, 0x12, 0x21], 33),
            &cid_string(&[0x01, 0x71, 0x12, 0x20], 31),
            &cid_string(&[0x01, 0x71, 0x12, 0x20], 33),
            &cid_string(&[0x01, 0x71, 0x12], 0),
        ],
    );
    // The encoder agrees with the data model's example link.
    let link = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    let head = cid_string(&[0x01, 0x71, 0x12, 0x20], 0);
    assert_eq!(&link[..7], &head[..7]);
}

/// A DASL CID decodes to exactly 36 bytes; a shorter or longer string,
/// nonzero pad bits or a character outside the lowercase alphabet fails.
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
}

/// The check reads the whole lexeme: surrounding whitespace is not
/// trimmed.
#[test]
fn a_format_reads_the_whole_lexeme() {
    assert_eq!(status("tid", "3jzfcijpj2z2a"), CheckStatus::Passed);
    assert_eq!(status("tid", " 3jzfcijpj2z2a"), CheckStatus::Failed);
}
