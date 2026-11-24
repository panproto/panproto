//! Paths into a declarative document and their location in JSON text.
//!
//! A path names one value of a document by the member names and array
//! indices that lead to it from the root. Both DSL references locate a
//! refusal by such a path, and [`locate`] finds the byte range of the value
//! a path names in the JSON text the document was read from.

use crate::json::Value;

/// One step of a document path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Segment {
    /// The member of an object with this name.
    Key(String),
    /// The element of an array at this index.
    Index(usize),
}

/// The path from a document's root to one of its values.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(pub Vec<Segment>);

impl Path {
    /// The root.
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// This path extended by the member `key`.
    #[must_use]
    pub fn key(&self, key: &str) -> Self {
        let mut steps = self.0.clone();
        steps.push(Segment::Key(key.to_owned()));
        Self(steps)
    }

    /// This path extended by the element at `index`.
    #[must_use]
    pub fn index(&self, index: usize) -> Self {
        let mut steps = self.0.clone();
        steps.push(Segment::Index(index));
        Self(steps)
    }

    /// This path followed by `suffix`.
    #[must_use]
    pub fn join(&self, suffix: &Self) -> Self {
        let mut steps = self.0.clone();
        steps.extend(suffix.0.iter().cloned());
        Self(steps)
    }
}

/// A JSON value with the byte range it occupies.
struct Located {
    start: usize,
    end: usize,
    children: Children,
}

enum Children {
    None,
    Object(Vec<(String, Located)>),
    Array(Vec<Located>),
}

struct Cursor<'a> {
    text: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn blank(&mut self) {
        while matches!(self.text.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        let found = self.text.get(self.at) == Some(&byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn string(&mut self) -> Option<String> {
        let start = self.at;
        if !self.take(b'"') {
            return None;
        }
        loop {
            match self.text.get(self.at)? {
                b'"' => {
                    self.at += 1;
                    break;
                }
                b'\\' => self.at += 2,
                _ => self.at += 1,
            }
        }
        let raw = std::str::from_utf8(self.text.get(start..self.at)?).ok()?;
        match crate::json::parse(raw)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    fn value(&mut self) -> Option<Located> {
        self.blank();
        let start = self.at;
        let children = match self.text.get(self.at)? {
            b'{' => {
                self.at += 1;
                let mut entries = Vec::new();
                self.blank();
                if !self.take(b'}') {
                    loop {
                        self.blank();
                        let key = self.string()?;
                        self.blank();
                        if !self.take(b':') {
                            return None;
                        }
                        let value = self.value()?;
                        entries.push((key, value));
                        self.blank();
                        if self.take(b'}') {
                            break;
                        }
                        if !self.take(b',') {
                            return None;
                        }
                    }
                }
                Children::Object(entries)
            }
            b'[' => {
                self.at += 1;
                let mut elements = Vec::new();
                self.blank();
                if !self.take(b']') {
                    loop {
                        elements.push(self.value()?);
                        self.blank();
                        if self.take(b']') {
                            break;
                        }
                        if !self.take(b',') {
                            return None;
                        }
                    }
                }
                Children::Array(elements)
            }
            b'"' => {
                self.string()?;
                Children::None
            }
            _ => {
                while self.text.get(self.at).is_some_and(|b| {
                    !matches!(b, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
                }) {
                    self.at += 1;
                }
                let raw = std::str::from_utf8(self.text.get(start..self.at)?).ok()?;
                match crate::json::parse(raw)? {
                    Value::Null | Value::Bool(_) | Value::Number(_) => Children::None,
                    _ => return None,
                }
            }
        };
        Some(Located {
            start,
            end: self.at,
            children,
        })
    }
}

/// Definition: the location of a path in JSON text.
///
/// The byte range `(offset, length)` of the value at `path`, from its first
/// byte to its last; when an object names a member more than once the last
/// occurrence is located. `None` when `text` is not one JSON value or holds
/// no value at `path`.
///
/// Book anchors: `L-theory-dsl-4`, `L-lens-dsl-6`.
#[must_use]
pub fn locate(text: &str, path: &Path) -> Option<(usize, usize)> {
    crate::json::parse(text)?;
    let mut cursor = Cursor {
        text: text.as_bytes(),
        at: 0,
    };
    let mut node = cursor.value()?;
    for step in &path.0 {
        node = match (step, node.children) {
            (Segment::Key(key), Children::Object(entries)) => {
                entries.into_iter().rev().find(|(k, _)| k == key)?.1
            }
            (Segment::Index(index), Children::Array(elements)) => {
                elements.into_iter().nth(*index)?
            }
            _ => return None,
        };
    }
    Some((node.start, node.end - node.start))
}
