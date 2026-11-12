//! The JSON value a JSON-LD document holds, with the place of each value.

use std::rc::Rc;

/// A JSON value with its JSON Pointer, its first byte and its token.
#[derive(Clone, Debug)]
pub struct Json {
    /// The value.
    pub kind: Kind,
    /// Its JSON Pointer (RFC 6901) from the document's top value.
    pub pointer: Rc<str>,
    /// The byte offset of its first byte.
    pub offset: usize,
    /// The index of the token that writes it, for a scalar.
    pub token: usize,
}

/// A JSON value.
#[derive(Clone, Debug)]
pub enum Kind {
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
pub struct Member {
    /// The decoded key.
    pub key: String,
    /// The index of the key's token.
    pub key_token: usize,
    /// The value.
    pub value: Json,
}

/// Append the reference token `segment` to `pointer`, escaping `~` and `/`
/// as RFC 6901 Sec 3 requires.
pub fn child_pointer(pointer: &str, segment: &str) -> Rc<str> {
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
    pub const fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub const fn is_scalar(&self) -> bool {
        matches!(self.kind, Kind::Bool(_) | Kind::Number(_) | Kind::Str(_))
    }
}
