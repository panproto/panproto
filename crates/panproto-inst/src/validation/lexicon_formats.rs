//! The string formats of the AT Protocol Lexicon.
//!
//! Each function decides whether a string is a lexeme of one Lexicon string
//! format, following the normative text the Lexicon specification
//! (<https://atproto.com/specs/lexicon>, section "String Formats") points to
//! for that format:
//!
//! - `handle`: <https://atproto.com/specs/handle>, "Handle Identifier Syntax";
//! - `nsid`: <https://atproto.com/specs/nsid>, "NSID Syntax";
//! - `tid`: <https://atproto.com/specs/tid>, "TID Syntax";
//! - `record-key`: <https://atproto.com/specs/record-key>, "Record Key Syntax";
//! - `at-uri`: <https://atproto.com/specs/at-uri-scheme>, "Restricted AT URI
//!   Syntax", the subset the Lexicon `at-uri` type uses;
//! - `did`: <https://atproto.com/specs/did>, "DID Identifier Syntax", on
//!   top of the W3C DID syntax every family checks;
//! - `at-identifier`: the Lexicon specification itself, a handle or a DID
//!   in the Lexicon's `did` syntax;
//! - `cid`: <https://atproto.com/specs/data-model>, "Link and CID Formats",
//!   which makes the lowercase-base32 DASL CID form the string format and
//!   admits only the blessed codecs and SHA-256 multihash
//!   ([`super::multiformats`]);
//! - `language`: a well-formed language tag of RFC 5646 (BCP 47), section
//!   2.1, as the Lexicon specification requires ("well-formed", not
//!   "valid", section 2.2.9).
//!
//! Every check is syntactic. The Lexicon specification asks that a value
//! passing basic syntax be accepted by Lexicon validation and that other
//! problems (an unsupported DID method, a reserved top-level domain, a tag
//! that is well-formed but not valid) be raised by the application, so
//! none of those is refused here.

/// The most characters a handle may have.
const HANDLE_MAX: usize = 253;
/// The most characters one label of a handle or of an NSID's domain
/// authority may have.
const LABEL_MAX: usize = 63;
/// The most characters an NSID may have.
const NSID_MAX: usize = 317;
/// The most characters an NSID's domain authority may have.
const NSID_AUTHORITY_MAX: usize = 253;
/// The most characters an NSID's name segment may have.
const NSID_NAME_MAX: usize = 63;
/// The number of characters of a TID.
const TID_LEN: usize = 13;
/// The base32-sortable alphabet of a TID.
const TID_ALPHABET: &[u8] = b"234567abcdefghijklmnopqrstuvwxyz";
/// The characters a TID may start with: the top bit of its integer is 0.
const TID_FIRST: &[u8] = b"234567abcdefghij";
/// The most characters a record key may have.
const RECORD_KEY_MAX: usize = 512;
/// The most characters a DID may have in the context of atproto.
const DID_MAX: usize = 2048;
/// The most bytes an AT URI may have ("8 kilobytes").
const AT_URI_MAX: usize = 8 * 1024;

/// A label of a hostname: 1 to 63 ASCII letters, digits and hyphens, not
/// starting or ending with a hyphen.
fn is_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= LABEL_MAX
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
}

/// A handle: an ASCII hostname of at most 253 characters with at least two
/// labels, whose last label (the top-level domain) does not start with a
/// digit.
pub(super) fn is_handle(text: &str) -> bool {
    if !text.is_ascii() || text.len() > HANDLE_MAX {
        return false;
    }
    let labels: Vec<&str> = text.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| is_label(label))
        && labels
            .last()
            .and_then(|tld| tld.as_bytes().first())
            .is_some_and(|b| !b.is_ascii_digit())
}

/// An NSID: an ASCII domain authority in reverse order (at most 253
/// characters, at least two labels, the first not starting with a digit)
/// followed by a name of 1 to 63 ASCII letters and digits not starting
/// with a digit, at least three segments and at most 317 characters in
/// all.
pub(super) fn is_nsid(text: &str) -> bool {
    if !text.is_ascii() || text.len() > NSID_MAX {
        return false;
    }
    let Some((authority, name)) = text.rsplit_once('.') else {
        return false;
    };
    let labels: Vec<&str> = authority.split('.').collect();
    let name_ok = !name.is_empty()
        && name.len() <= NSID_NAME_MAX
        && name.bytes().all(|b| b.is_ascii_alphanumeric())
        && name.as_bytes().first().is_some_and(|b| !b.is_ascii_digit());
    name_ok
        && authority.len() <= NSID_AUTHORITY_MAX
        && labels.len() >= 2
        && labels.iter().all(|label| is_label(label))
        && labels
            .first()
            .and_then(|tld| tld.as_bytes().first())
            .is_some_and(|b| !b.is_ascii_digit())
}

/// A TID: 13 characters of the base32-sortable alphabet, the first one of
/// `234567abcdefghij`.
pub(super) fn is_tid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == TID_LEN
        && bytes.iter().all(|b| TID_ALPHABET.contains(b))
        && bytes.first().is_some_and(|b| TID_FIRST.contains(b))
}

/// A record key: 1 to 512 characters, each an ASCII letter or digit or one
/// of `.`, `-`, `_`, `:` and `~`, other than `.` and `..`.
pub(super) fn is_record_key(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= RECORD_KEY_MAX
        && text != "."
        && text != ".."
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':' | b'~'))
}

/// A DID as the Lexicon's `did` format reads it: a W3C DID
/// ([`super::did::is_did`]) that also meets the atproto rules, so it starts
/// with lowercase `did:`, its method is one or more lowercase ASCII letters,
/// and it has at most 2048 characters. With the W3C syntax this is the
/// specification's reference expression
/// `^did:[a-z]+:[a-zA-Z0-9._:%-]*[a-zA-Z0-9._-]$` with its length limit,
/// a percent sign always starting a percent-encoded octet.
pub(super) fn is_lexicon_did(text: &str) -> bool {
    text.len() <= DID_MAX
        && super::did::is_did(text)
        && text
            .strip_prefix("did:")
            .and_then(|rest| rest.split_once(':'))
            .is_some_and(|(method, _)| method.bytes().all(|b| b.is_ascii_lowercase()))
}

/// An AT identifier: a DID or a handle. The two cannot be confused, since a
/// handle has no `:`.
pub(super) fn is_at_identifier(text: &str) -> bool {
    is_lexicon_did(text) || is_handle(text)
}

/// An AT URI in the restricted syntax Lexicons use: at most 8 kilobytes,
/// `at://`, an authority that is a handle or a DID, then optionally `/` and
/// an NSID, then optionally `/` and a record key, and nothing else (no
/// query, no fragment, no trailing slash).
pub(super) fn is_at_uri(text: &str) -> bool {
    if text.len() > AT_URI_MAX {
        return false;
    }
    let Some(rest) = text.strip_prefix("at://") else {
        return false;
    };
    let mut parts = rest.split('/');
    let authority_ok = parts.next().is_some_and(is_at_identifier);
    let collection_ok = parts.next().is_none_or(is_nsid);
    let record_key_ok = parts.next().is_none_or(is_record_key);
    authority_ok && collection_ok && record_key_ok && parts.next().is_none()
}

/// The grandfathered tags of RFC 5646 section 2.1 (the `irregular` and
/// `regular` productions), compared without regard to case.
const GRANDFATHERED: &[&str] = &[
    "en-GB-oed",
    "i-ami",
    "i-bnn",
    "i-default",
    "i-enochian",
    "i-hak",
    "i-klingon",
    "i-lux",
    "i-mingo",
    "i-navajo",
    "i-pwn",
    "i-tao",
    "i-tay",
    "i-tsu",
    "sgn-BE-FR",
    "sgn-BE-NL",
    "sgn-CH-DE",
    "art-lojban",
    "cel-gaulish",
    "no-bok",
    "no-nyn",
    "zh-guoyu",
    "zh-hakka",
    "zh-min",
    "zh-min-nan",
    "zh-xiang",
];

fn all_alpha(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphabetic())
}

fn all_digit(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

fn all_alphanum(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The `privateuse` production after its `x`: one or more subtags of 1 to
/// 8 alphanumerics, to the end.
fn is_private_use_tail(subtags: &[&str]) -> bool {
    !subtags.is_empty()
        && subtags
            .iter()
            .all(|s| (1..=8).contains(&s.len()) && all_alphanum(s))
}

/// The `langtag` production of RFC 5646 section 2.1 over the tag's subtags.
///
/// Every subtag type is told apart by its length, position and characters,
/// as the section says, so one pass in production order decides it: the
/// language, up to three extended language subtags after a two- or
/// three-letter language, a script, a region, variants, extensions, and a
/// private use part.
fn is_langtag(subtags: &[&str]) -> bool {
    let mut rest = subtags;
    // language = 2*3ALPHA ["-" extlang] / 4ALPHA / 5*8ALPHA
    let Some((language, tail)) = rest.split_first() else {
        return false;
    };
    if !(2..=8).contains(&language.len()) || !all_alpha(language) {
        return false;
    }
    rest = tail;
    if language.len() <= 3 {
        // extlang = 3ALPHA *2("-" 3ALPHA)
        let mut extlangs = 0;
        while extlangs < 3 {
            match rest.split_first() {
                Some((s, tail)) if s.len() == 3 && all_alpha(s) => {
                    rest = tail;
                    extlangs += 1;
                }
                _ => break,
            }
        }
    }
    // ["-" script], script = 4ALPHA
    if let Some((s, tail)) = rest.split_first()
        && s.len() == 4
        && all_alpha(s)
    {
        rest = tail;
    }
    // ["-" region], region = 2ALPHA / 3DIGIT
    if let Some((s, tail)) = rest.split_first()
        && ((s.len() == 2 && all_alpha(s)) || (s.len() == 3 && all_digit(s)))
    {
        rest = tail;
    }
    // *("-" variant), variant = 5*8alphanum / (DIGIT 3alphanum)
    while let Some((s, tail)) = rest.split_first() {
        let variant = ((5..=8).contains(&s.len()) && all_alphanum(s))
            || (s.len() == 4 && s.as_bytes()[0].is_ascii_digit() && all_alphanum(s));
        if !variant {
            break;
        }
        rest = tail;
    }
    // *("-" extension), extension = singleton 1*("-" (2*8alphanum))
    while let Some((s, tail)) = rest.split_first() {
        let singleton = s.len() == 1 && all_alphanum(s) && !s.eq_ignore_ascii_case("x");
        if !singleton {
            break;
        }
        let parts = tail
            .iter()
            .take_while(|p| (2..=8).contains(&p.len()) && all_alphanum(p))
            .count();
        if parts == 0 {
            return false;
        }
        rest = &tail[parts..];
    }
    // ["-" privateuse]
    match rest.split_first() {
        None => true,
        Some((s, tail)) => s.eq_ignore_ascii_case("x") && is_private_use_tail(tail),
    }
}

/// A well-formed language tag of RFC 5646 (BCP 47): a `langtag`, a
/// `privateuse` tag or a grandfathered tag, without regard to case
/// (section 2.1.1).
pub(super) fn is_language(text: &str) -> bool {
    if GRANDFATHERED.iter().any(|g| g.eq_ignore_ascii_case(text)) {
        return true;
    }
    let subtags: Vec<&str> = text.split('-').collect();
    match subtags.split_first() {
        Some((first, tail)) if first.eq_ignore_ascii_case("x") => is_private_use_tail(tail),
        _ => is_langtag(&subtags),
    }
}
