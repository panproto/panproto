//! The DID syntax of W3C Decentralized Identifiers (DIDs) v1.0.
//!
//! <https://www.w3.org/TR/did-1.0/>, section 3.1, "DID Syntax":
//!
//! ```text
//! did                = "did:" method-name ":" method-specific-id
//! method-name        = 1*method-char
//! method-char        = %x61-7A / DIGIT
//! method-specific-id = *( *idchar ":" ) 1*idchar
//! idchar             = ALPHA / DIGIT / "." / "-" / "_" / pct-encoded
//! pct-encoded        = "%" HEXDIG HEXDIG
//! ```
//!
//! The rules are RFC 5234 ABNF, whose quoted strings are case insensitive
//! (section 2.3), so the scheme `"did:"` matches in any case and `HEXDIG`
//! admits `a`-`f` as well as `A`-`F`; the method name is the code-point
//! range `%x61-7A`, lowercase letters only, with digits.

/// Whether `text` is a DID in the W3C DID syntax.
pub(super) fn is_did(text: &str) -> bool {
    let Some(rest) = text
        .get(..4)
        .filter(|scheme| scheme.eq_ignore_ascii_case("did:"))
        .map(|_| &text[4..])
    else {
        return false;
    };
    let Some((method, identifier)) = rest.split_once(':') else {
        return false;
    };
    !method.is_empty()
        && method
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && is_method_specific_id(identifier)
}

/// `*( *idchar ":" ) 1*idchar`: colon-separated runs of `idchar`, the last
/// one nonempty.
fn is_method_specific_id(identifier: &str) -> bool {
    let mut segments = identifier.split(':');
    let last = segments.next_back().unwrap_or_default();
    !last.is_empty() && is_idchars(last) && segments.all(is_idchars)
}

/// `*idchar`: ASCII letters, digits, `.`, `-`, `_` and `%` followed by two
/// hexadecimal digits.
fn is_idchars(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let encoded = bytes
                    .get(i + 1..i + 3)
                    .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit));
                if !encoded {
                    return false;
                }
                i += 3;
            }
            b if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') => i += 1,
            _ => return false,
        }
    }
    true
}
