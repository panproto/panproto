//! The tree-sitter tree of an RDF document, read out of its CST schema.

use panproto_schema::Schema;

use super::{RdfReadError, RdfReadErrorKind};

/// A tree-sitter tree read out of the CST schema: each node's kind, byte
/// span and children in source order.
pub(super) struct Tree<'a> {
    pub(super) cst: &'a Schema,
}

/// One node of the tree.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cursor<'a> {
    /// The CST vertex.
    pub(super) name: &'a str,
    /// The grammar's name for the node.
    pub(super) kind: &'a str,
    /// The byte offset of its first byte.
    pub(super) start: usize,
    /// The byte offset after its last byte.
    pub(super) end: usize,
}

impl<'a> Tree<'a> {
    fn byte(&self, name: &str, sort: &str) -> usize {
        self.cst
            .constraints
            .get(name)
            .and_then(|cs| cs.iter().find(|c| c.sort.as_ref() == sort))
            .and_then(|c| c.value.parse().ok())
            .unwrap_or(0)
    }

    pub(super) fn cursor(&self, name: &'a str) -> Cursor<'a> {
        Cursor {
            name,
            kind: self.cst.vertices.get(name).map_or("", |v| v.kind.as_ref()),
            start: self.byte(name, "start-byte"),
            end: self.byte(name, "end-byte"),
        }
    }

    /// The children of `parent` in source order, comments included.
    pub(super) fn children(&self, parent: &Cursor<'a>) -> Vec<Cursor<'a>> {
        let mut children: Vec<Cursor<'a>> = self
            .cst
            .outgoing_edges(parent.name)
            .iter()
            .map(|edge| self.cursor(edge.tgt.as_ref()))
            .collect();
        children.sort_by_key(|child| (child.start, child.end));
        children
    }

    /// The document node, when the parser produced one.
    pub(super) fn root(&self) -> Option<Cursor<'a>> {
        self.cst
            .vertices
            .iter()
            .find(|(_, v)| v.kind.as_ref() == "document")
            .map(|(name, _)| self.cursor(name.as_ref()))
    }

    /// Every node of kind `kind`, in source order.
    pub(super) fn all_of_kind(&self, kind: &str) -> Vec<Cursor<'a>> {
        let mut found: Vec<Cursor<'a>> = self
            .cst
            .vertices
            .iter()
            .filter(|(_, v)| v.kind.as_ref() == kind)
            .map(|(name, _)| self.cursor(name.as_ref()))
            .collect();
        found.sort_by_key(|node| (node.start, node.end));
        found
    }

    /// The first byte of the earliest region the parser marked as
    /// recovered: the smallest start of an `ERROR` node, a missing node or
    /// an empty node other than the document.
    pub(super) fn first_marker(&self) -> Option<usize> {
        self.markers().into_iter().map(|(start, _)| start).min()
    }

    fn markers(&self) -> Vec<(usize, usize)> {
        self.cst
            .vertices
            .iter()
            .filter(|(name, vertex)| {
                let missing = self
                    .cst
                    .constraints
                    .get(*name)
                    .is_some_and(|cs| cs.iter().any(|c| c.sort.as_ref() == "missing"));
                let cursor = self.cursor(name.as_ref());
                vertex.kind.as_ref() == "ERROR"
                    || missing
                    || (cursor.start == cursor.end && vertex.kind.as_ref() != "document")
            })
            .map(|(name, _)| {
                let cursor = self.cursor(name.as_ref());
                (cursor.start, cursor.end)
            })
            .collect()
    }

    /// The first parser recovery marker, by offset: an `ERROR` node, a node
    /// the parser inserted as missing, or an empty node other than the
    /// document.
    pub(super) fn first_recovery(&self) -> Option<usize> {
        let markers = self.markers();
        // The outermost marker spans the whole region the parser could not
        // read; the innermost one nested in it is where reading failed.
        let mut current = *markers
            .iter()
            .min_by_key(|(start, end)| (*start, std::cmp::Reverse(*end)))?;
        while let Some(inner) = markers
            .iter()
            .filter(|&&m| m != current && m.0 >= current.0 && m.1 <= current.1)
            .min_by_key(|(start, end)| (*start, std::cmp::Reverse(*end)))
        {
            current = *inner;
        }
        Some(current.0)
    }
}

/// Decode `input` as UTF-8 and refuse a raw U+0000 or U+007F, which the
/// tree-sitter Turtle grammar cannot read inside a string.
pub(super) fn checked_text(input: &[u8]) -> Result<&str, RdfReadError> {
    let text = std::str::from_utf8(input)
        .map_err(|e| RdfReadError::new(e.valid_up_to(), RdfReadErrorKind::NotUtf8))?;
    if let Some((offset, character)) = text
        .char_indices()
        .find(|&(_, c)| matches!(c, '\u{0}' | '\u{7F}'))
    {
        return Err(RdfReadError::new(
            offset,
            RdfReadErrorKind::UnsupportedCharacter { character },
        ));
    }
    Ok(text)
}
