//! The JSON value a JSON-LD document holds, with the place of each value.

use std::rc::Rc;

/// A JSON value with its JSON Pointer, its first byte and its token.
#[derive(Clone, Debug)]
pub(crate) struct Json {
    /// The value.
    pub(crate) kind: Kind,
    /// Its JSON Pointer (RFC 6901) from the document's top value.
    pub(crate) pointer: Rc<str>,
    /// The byte offset of its first byte.
    pub(crate) offset: usize,
    /// The index of the token that writes it, for a scalar.
    pub(crate) token: usize,
}

/// A JSON value.
#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Null,
    Bool(bool),
    /// A number, by its token.
    Number(String),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<Member>),
}

/// An object member.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    /// The decoded key.
    pub(crate) key: String,
    /// The index of the key's token.
    pub(crate) key_token: usize,
    /// The value.
    pub(crate) value: Json,
}

/// Append the reference token `segment` to `pointer`, escaping `~` and `/`
/// as RFC 6901 Sec 3 requires.
pub(crate) fn child_pointer(pointer: &str, segment: &str) -> Rc<str> {
    let mut out = String::with_capacity(pointer.len() + segment.len() + 1);
    out.push_str(pointer);
    out.push('/');
    for c in segment.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            _ => out.push(c),
        }
    }
    Rc::from(out)
}

impl Json {
    pub(crate) const fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub(crate) const fn is_scalar(&self) -> bool {
        matches!(self.kind, Kind::Bool(_) | Kind::Number(_) | Kind::Str(_))
    }
}
