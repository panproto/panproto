//! Paths to the elements of a document, and their source spans.
//!
//! A [`DocumentPath`] names one value of a document by the member names
//! and array indices that lead to it from the document root, as a JSON
//! Pointer (RFC 6901) does. Every refusal of a document carries the path of
//! the value it refuses, whatever the surface syntax; [`locate_json`] turns
//! a path into the byte range of that value in JSON source text.

use std::fmt;

/// One step of a [`DocumentPath`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PathSegment {
    /// The member of an object with this name.
    Key(String),
    /// The element of an array at this index.
    Index(usize),
}

/// The path from a document's root to one of its values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DocumentPath(pub Vec<PathSegment>);

impl DocumentPath {
    /// The path of the document root.
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// This path extended by the member `key`.
    #[must_use]
    pub fn key(&self, key: &str) -> Self {
        let mut segments = self.0.clone();
        segments.push(PathSegment::Key(key.to_owned()));
        Self(segments)
    }

    /// This path extended by the array element at `index`.
    #[must_use]
    pub fn index(&self, index: usize) -> Self {
        let mut segments = self.0.clone();
        segments.push(PathSegment::Index(index));
        Self(segments)
    }

    /// This path followed by every segment of `suffix`.
    #[must_use]
    pub fn join(&self, suffix: &Self) -> Self {
        let mut segments = self.0.clone();
        segments.extend(suffix.0.iter().cloned());
        Self(segments)
    }
}

/// Written as a JSON Pointer: `/` before each segment, with `~` escaped
/// as `~0` and `/` as `~1` inside a member name. The root is the empty
/// string.
impl fmt::Display for DocumentPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.0 {
            match segment {
                PathSegment::Key(key) => {
                    f.write_str("/")?;
                    f.write_str(&key.replace('~', "~0").replace('/', "~1"))?;
                }
                PathSegment::Index(index) => write!(f, "/{index}")?,
            }
        }
        Ok(())
    }
}

/// The byte range `(offset, length)` of the value at `path` in the JSON
/// text `source`.
///
/// The range covers the value as written, from its first byte to its last
/// (a string with its quotes, a container with its brackets). When an
/// object names a member more than once, the last occurrence is the one
/// located, which is the occurrence a JSON reader keeps. `None` when
/// `source` is not JSON text or holds no value at `path`.
#[must_use]
pub fn locate_json(source: &str, path: &DocumentPath) -> Option<(usize, usize)> {
    let mut scanner = Scanner {
        bytes: source.as_bytes(),
        at: 0,
    };
    scanner.whitespace();
    let found = scanner.find(&path.0)?;
    scanner.whitespace();
    (scanner.at == scanner.bytes.len()).then_some(found)
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// A document the path reader cannot read.
struct Unreadable;

impl Scanner<'_> {
    fn whitespace(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Option<()> {
        (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }

    /// Read the value at the cursor and return the range of the value that
    /// `path` names inside it (the whole value for an empty path), leaving
    /// the cursor after the whole value.
    fn find(&mut self, path: &[PathSegment]) -> Option<(usize, usize)> {
        let start = self.at;
        let found = match (self.bytes.get(self.at)?, path.split_first()) {
            (b'{', first) => self.object(first).ok()?,
            (b'[', first) => self.array(first).ok()?,
            (_, Some(_)) => {
                self.scalar()?;
                None
            }
            (_, None) => {
                self.scalar()?;
                Some((start, self.at - start))
            }
        };
        match path {
            [] => Some((start, self.at - start)),
            _ => found,
        }
    }

    fn object(
        &mut self,
        first: Option<(&PathSegment, &[PathSegment])>,
    ) -> Result<Option<(usize, usize)>, Unreadable> {
        self.eat(b'{').ok_or(Unreadable)?;
        self.whitespace();
        let mut found = None;
        if self.eat(b'}').is_some() {
            return Ok(found);
        }
        loop {
            self.whitespace();
            let key = self.string().ok_or(Unreadable)?;
            self.whitespace();
            self.eat(b':').ok_or(Unreadable)?;
            self.whitespace();
            match first {
                Some((PathSegment::Key(wanted), rest)) if *wanted == key => {
                    found = self.find(rest);
                }
                _ => {
                    self.find(&[]).ok_or(Unreadable)?;
                }
            }
            self.whitespace();
            if self.eat(b'}').is_some() {
                return Ok(found);
            }
            self.eat(b',').ok_or(Unreadable)?;
        }
    }

    fn array(
        &mut self,
        first: Option<(&PathSegment, &[PathSegment])>,
    ) -> Result<Option<(usize, usize)>, Unreadable> {
        self.eat(b'[').ok_or(Unreadable)?;
        self.whitespace();
        let mut found = None;
        if self.eat(b']').is_some() {
            return Ok(found);
        }
        let mut index = 0;
        loop {
            self.whitespace();
            match first {
                Some((PathSegment::Index(wanted), rest)) if *wanted == index => {
                    found = self.find(rest);
                }
                _ => {
                    self.find(&[]).ok_or(Unreadable)?;
                }
            }
            index += 1;
            self.whitespace();
            if self.eat(b']').is_some() {
                return Ok(found);
            }
            self.eat(b',').ok_or(Unreadable)?;
        }
    }

    /// Read a string and return its decoded contents.
    fn string(&mut self) -> Option<String> {
        let start = self.at;
        self.eat(b'"')?;
        loop {
            match *self.bytes.get(self.at)? {
                b'"' => {
                    self.at += 1;
                    break;
                }
                b'\\' => self.at += 2,
                _ => self.at += 1,
            }
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.at)?).ok()?;
        serde_json::from_str(text).ok()
    }

    /// Read a string, number, `true`, `false` or `null`.
    fn scalar(&mut self) -> Option<()> {
        if self.bytes.get(self.at) == Some(&b'"') {
            return self.string().map(|_| ());
        }
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.'))
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.at)?).ok()?;
        serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .filter(|value| !value.is_array() && !value.is_object() && !value.is_string())
            .map(|_| ())
    }
}
