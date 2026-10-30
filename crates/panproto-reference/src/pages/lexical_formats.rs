//! Independent recognizers for the closed lexical formats used by validation.
//!
//! These recognizers are intentionally defined in the reference crate rather
//! than shared with production.  The simple formats follow their grammars
//! directly; the CID recognizer decodes the canonical lowercase-base32 body
//! and then checks the `ATProto` data model's blessed CID envelope.

const HANDLE_LIMIT: usize = 253;
const LABEL_LIMIT: usize = 63;
const NSID_LIMIT: usize = 317;
const DID_LIMIT: usize = 2048;
const AT_URI_LIMIT: usize = 8192;

/// Decides one of the formats in the closed reference validation domain.
pub(super) fn matches(family: &str, name: &str, text: &str) -> Option<bool> {
    Some(match name {
        "did" => did(family, text),
        "at-identifier" => at_identifier(text),
        "at-uri" => at_uri(text),
        "cid" => cid(text),
        "handle" => handle(text),
        "nsid" => nsid(text),
        "tid" => tid(text),
        "record-key" => record_key(text),
        "language" => language(text),
        _ => return None,
    })
}

fn did(family: &str, text: &str) -> bool {
    w3c_did(text)
        && (family != "atproto"
            || (text.len() <= DID_LIMIT
                && text.starts_with("did:")
                && text[4..].split_once(':').is_some_and(|(method, _)| {
                    method.bytes().all(|byte| byte.is_ascii_lowercase())
                })))
}

fn w3c_did(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 6 || !bytes[..4].eq_ignore_ascii_case(b"did:") {
        return false;
    }
    let Some(separator) = bytes[4..].iter().position(|byte| *byte == b':') else {
        return false;
    };
    let method_end = separator + 4;
    if method_end == 4
        || !bytes[4..method_end]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return false;
    }
    did_identifier(&bytes[method_end + 1..])
}

fn did_identifier(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.last() == Some(&b':') {
        return false;
    }
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'%' => {
                if !bytes
                    .get(offset + 1..offset + 3)
                    .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
                {
                    return false;
                }
                offset += 3;
            }
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':') => {
                offset += 1;
            }
            _ => return false,
        }
    }
    true
}

fn lexicon_did(text: &str) -> bool {
    did("atproto", text)
}

fn label(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=LABEL_LIMIT).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
}

fn handle(text: &str) -> bool {
    if !text.is_ascii() || text.len() > HANDLE_LIMIT {
        return false;
    }
    let labels: Vec<&str> = text.split('.').collect();
    labels.len() >= 2
        && labels.iter().copied().all(label)
        && labels
            .last()
            .and_then(|part| part.as_bytes().first())
            .is_some_and(|byte| !byte.is_ascii_digit())
}

fn nsid(text: &str) -> bool {
    if !text.is_ascii() || text.len() > NSID_LIMIT {
        return false;
    }
    let segments: Vec<&str> = text.split('.').collect();
    if segments.len() < 3 {
        return false;
    }
    let authority_len = segments[..segments.len() - 1]
        .iter()
        .map(|part| part.len())
        .sum::<usize>()
        + segments.len()
        - 2;
    let name = segments[segments.len() - 1];
    authority_len <= HANDLE_LIMIT
        && segments[..segments.len() - 1].iter().copied().all(label)
        && segments[0]
            .as_bytes()
            .first()
            .is_some_and(|byte| !byte.is_ascii_digit())
        && (1..=LABEL_LIMIT).contains(&name.len())
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && name
            .as_bytes()
            .first()
            .is_some_and(|byte| !byte.is_ascii_digit())
}

fn tid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 13
        && bytes
            .iter()
            .all(|byte| b"234567abcdefghijklmnopqrstuvwxyz".contains(byte))
        && bytes
            .first()
            .is_some_and(|byte| b"234567abcdefghij".contains(byte))
}

fn record_key(text: &str) -> bool {
    (1..=512).contains(&text.len())
        && !matches!(text, "." | "..")
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'~')
        })
}

fn at_identifier(text: &str) -> bool {
    handle(text) || lexicon_did(text)
}

fn at_uri(text: &str) -> bool {
    if text.len() > AT_URI_LIMIT {
        return false;
    }
    let Some(body) = text.strip_prefix("at://") else {
        return false;
    };
    let parts: Vec<&str> = body.split('/').collect();
    (1..=3).contains(&parts.len())
        && at_identifier(parts[0])
        && parts.get(1).is_none_or(|collection| nsid(collection))
        && parts.get(2).is_none_or(|key| record_key(key))
}

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

fn alphabetic(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn numeric(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
}

fn alphanumeric(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn private_tail(parts: &[&str]) -> bool {
    !parts.is_empty()
        && parts
            .iter()
            .all(|part| part.len() <= 8 && alphanumeric(part))
}

fn language(text: &str) -> bool {
    if GRANDFATHERED
        .iter()
        .any(|special| special.eq_ignore_ascii_case(text))
    {
        return true;
    }
    let parts: Vec<&str> = text.split('-').collect();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("x"))
    {
        return private_tail(&parts[1..]);
    }
    let Some(primary) = parts.first() else {
        return false;
    };
    if !(2..=8).contains(&primary.len()) || !alphabetic(primary) {
        return false;
    }
    let mut at = 1;
    if primary.len() <= 3 {
        for _ in 0..3 {
            if parts
                .get(at)
                .is_some_and(|part| part.len() == 3 && alphabetic(part))
            {
                at += 1;
            } else {
                break;
            }
        }
    }
    if parts
        .get(at)
        .is_some_and(|part| part.len() == 4 && alphabetic(part))
    {
        at += 1;
    }
    if parts.get(at).is_some_and(|part| {
        (part.len() == 2 && alphabetic(part)) || (part.len() == 3 && numeric(part))
    }) {
        at += 1;
    }
    while parts.get(at).is_some_and(|part| {
        ((5..=8).contains(&part.len()) && alphanumeric(part))
            || (part.len() == 4 && part.as_bytes()[0].is_ascii_digit() && alphanumeric(part))
    }) {
        at += 1;
    }
    while parts.get(at).is_some_and(|part| {
        part.len() == 1 && alphanumeric(part) && !part.eq_ignore_ascii_case("x")
    }) {
        at += 1;
        let start = at;
        while parts
            .get(at)
            .is_some_and(|part| (2..=8).contains(&part.len()) && alphanumeric(part))
        {
            at += 1;
        }
        if at == start {
            return false;
        }
    }
    if parts
        .get(at)
        .is_some_and(|part| part.eq_ignore_ascii_case("x"))
    {
        return private_tail(&parts[at + 1..]);
    }
    at == parts.len()
}

const CID_BASE32: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

fn cid(text: &str) -> bool {
    let Some(body) = text.strip_prefix('b') else {
        return false;
    };
    decode_canonical_base32(body).is_some_and(|bytes| blessed_cid(&bytes))
}

fn blessed_cid(bytes: &[u8]) -> bool {
    let mut at = 0;
    if read_varint(bytes, &mut at) != Some(1) {
        return false;
    }
    let Some(codec) = read_varint(bytes, &mut at) else {
        return false;
    };
    if !matches!(codec, 0x55 | 0x71) || read_varint(bytes, &mut at) != Some(0x12) {
        return false;
    }
    read_varint(bytes, &mut at) == Some(32) && bytes.len().checked_sub(at) == Some(32)
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let start = *at;
    let mut value = 0u64;
    for shift in (0..63).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            if *at - start > 1 && byte == 0 {
                return None;
            }
            return Some(value);
        }
    }
    None
}

fn decode_canonical_base32(body: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(body.len() * 5 / 8);
    let mut buffer = 0u32;
    let mut held = 0u32;
    for symbol in body.bytes() {
        let value = CID_BASE32
            .iter()
            .position(|candidate| *candidate == symbol)
            .and_then(|position| u32::try_from(position).ok())?;
        buffer = (buffer << 5) | value;
        held += 5;
        if held >= 8 {
            held -= 8;
            output.push(u8::try_from(buffer >> held).ok()?);
            buffer &= (1 << held) - 1;
        }
    }
    (held < 5 && buffer == 0).then_some(output)
}
