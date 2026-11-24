//! Source formats, and the location of a refused element in source text.

use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::path::{DocumentPath, PathSegment};

/// The surface syntax of a document's source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    /// Nickel, evaluated with the DSL's bundled contract library.
    Nickel,
    /// JSON.
    Json,
    /// YAML.
    Yaml,
}

/// The byte range `(offset, length)` of the element at `path` in `source`.
///
/// JSON text is located by [`locate_json`](crate::path::locate_json), the
/// whole value; YAML text by [`locate_yaml`], the value's first character.
/// Nickel text gives `None`, as does text in which `path` names no value; a
/// refusal of such text keeps its path and carries no span.
#[must_use]
pub fn locate(source: &str, format: SourceFormat, path: &DocumentPath) -> Option<(usize, usize)> {
    match format {
        SourceFormat::Json => crate::path::locate_json(source, path),
        SourceFormat::Yaml => locate_yaml(source, path),
        SourceFormat::Nickel => None,
    }
}

/// The byte range `(offset, length)` of the first character of the YAML
/// node at `path` in `source`, the last occurrence when a member is named
/// twice, as a JSON object keeps it; `None` when `path` names no node.
///
/// The YAML reader reports positions only as the start of the node it was
/// reading when an error was raised, and has no end position for a node,
/// so a YAML node is located by its first character and not by its extent:
/// the node at `path` is read by a visitor that refuses it, and the
/// position of that refusal is the node's start.
#[must_use]
pub fn locate_yaml(source: &str, path: &DocumentPath) -> Option<(usize, usize)> {
    let occurrences = Find { path: &path.0 }
        .deserialize(yaml_serde::Deserializer::from_str(source))
        .ok()??;
    let error = Seek {
        path: &path.0,
        occurrences: &occurrences,
    }
    .deserialize(yaml_serde::Deserializer::from_str(source))
    .err()?;
    let offset = error.location()?.index();
    Some((offset, char_length(source, offset)))
}

/// The byte range `(offset, length)` of a YAML syntax error in `source`:
/// the character at which the YAML reader stopped, or the empty range at
/// the end of the text; `None` when the reader reports no position.
#[must_use]
pub fn yaml_syntax_span(source: &str) -> Option<(usize, usize)> {
    let error = serde_json::Value::deserialize(yaml_serde::Deserializer::from_str(source)).err()?;
    let offset = error.location()?.index().min(source.len());
    Some((offset, char_length(source, offset)))
}

/// The byte length of the character at `offset` in `source`, zero at the
/// end of the text or inside a character.
fn char_length(source: &str, offset: usize) -> usize {
    source
        .get(offset..)
        .and_then(|rest| rest.chars().next())
        .map_or(0, char::len_utf8)
}

/// Follows a path through a YAML document without refusing anything, and
/// returns, for each segment, the occurrence of the member it follows (the
/// last one of its name); `None` when the path names no node.
struct Find<'p> {
    path: &'p [PathSegment],
}

impl<'de> DeserializeSeed<'de> for Find<'_> {
    type Value = Option<Vec<usize>>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        if self.path.is_empty() {
            IgnoredAny::deserialize(deserializer)?;
            return Ok(Some(Vec::new()));
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Find<'_> {
    type Value = Option<Vec<usize>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any YAML node")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (first, rest) = (&self.path[0], &self.path[1..]);
        let mut found = None;
        let mut occurrence = 0;
        while let Some(key) = map.next_key::<String>()? {
            if matches!(first, PathSegment::Key(name) if *name == key) {
                found = map.next_value_seed(Find { path: rest })?.map(|mut inner| {
                    inner.insert(0, occurrence);
                    inner
                });
                occurrence += 1;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(found)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let (first, rest) = (&self.path[0], &self.path[1..]);
        let mut found = None;
        let mut index = 0;
        loop {
            if matches!(first, PathSegment::Index(wanted) if *wanted == index) {
                match seq.next_element_seed(Find { path: rest })? {
                    Some(inner) => {
                        found = inner.map(|mut inner| {
                            inner.insert(0, 0);
                            inner
                        });
                    }
                    None => break,
                }
            } else if seq.next_element::<IgnoredAny>()?.is_none() {
                break;
            }
            index += 1;
        }
        Ok(found)
    }

    fn visit_bool<E: Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

/// Follows a path through a YAML document, taking at each member segment
/// the occurrence [`Find`] chose, and refuses the node it reaches, so that
/// the reader's error carries that node's position.
struct Seek<'p> {
    path: &'p [PathSegment],
    occurrences: &'p [usize],
}

impl<'de> DeserializeSeed<'de> for Seek<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if self.path.is_empty() {
            return deserializer.deserialize_any(Refuse);
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seek<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a node on the path")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let inner = Self {
            path: &self.path[1..],
            occurrences: &self.occurrences[1..],
        };
        let mut occurrence = 0;
        while let Some(key) = map.next_key::<String>()? {
            if matches!(&self.path[0], PathSegment::Key(name) if *name == key) {
                if occurrence == self.occurrences[0] {
                    return map.next_value_seed(inner);
                }
                occurrence += 1;
            }
            map.next_value::<IgnoredAny>()?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let PathSegment::Index(wanted) = self.path[0] else {
            return Ok(());
        };
        for _ in 0..wanted {
            if seq.next_element::<IgnoredAny>()?.is_none() {
                return Ok(());
            }
        }
        seq.next_element_seed(Self {
            path: &self.path[1..],
            occurrences: &self.occurrences[1..],
        })
        .map(|_| ())
    }
}

/// A visitor that refuses every node.
struct Refuse;

impl Visitor<'_> for Refuse {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("no node: the refusal locates the node")
    }
}

/// The byte range `(offset, length)` of a JSON syntax error in `source`:
/// the one byte at which the JSON reader stopped, or the empty range at
/// the end of the text.
#[must_use]
pub fn json_syntax_span(source: &str, error: &serde_json::Error) -> (usize, usize) {
    let line_start: usize = source
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum();
    // `serde_json` counts the bytes read on the line, so the byte it names
    // is the one before its column.
    let offset = (line_start + error.column().saturating_sub(1)).min(source.len());
    (offset, usize::from(offset < source.len()))
}
