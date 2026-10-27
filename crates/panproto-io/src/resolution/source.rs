//! Read access to a syntactic source for reference resolution.
//!
//! A tree is read as nodes and arcs, a table instance as rows and
//! foreign-key pairs, through one [`Source`], so that every reference form
//! evaluates its paths, keys and scopes by one rule in both shapes. The
//! rules are those of validation's instance reader: a path follows the
//! out-edges labelled by each hop; at the last hop it reads the own value of
//! every node reached, and when no node is reached there, the field named
//! like the last edge (a node's extra field, a row's cell) of each node
//! before it. Absent and null values are not values. A `Within(v)` scope is
//! the nearest element anchored at `v` reached backwards along in-edges, the
//! least of the equally near ones.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::key::{KeyingError, canonical_key_bytes};
use panproto_inst::{CanonicalSink, FInstance, FieldPresence, Value, ViewRef, WInstance};
use panproto_schema::{CanonicalWriter, Edge, Key, KeyPart, KeyScope};

use panproto_inst::resolution_complement::ResolutionValueSite;

/// One out- or in-edge of an element: the element at its other end and its
/// label.
#[derive(Clone, Debug)]
pub struct Step<'a> {
    /// The element at the other end.
    pub other: ViewRef,
    /// The edge's schema label.
    pub label: &'a Edge,
}

/// A tree with its arcs indexed by parent and by child.
#[derive(Debug)]
pub struct TreeSource<'a> {
    /// The tree.
    pub tree: &'a WInstance,
    out: HashMap<u32, Vec<usize>>,
    into: HashMap<u32, Vec<usize>>,
}

/// The scope an element's key values are unique in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeOf {
    /// The whole instance (a document or global key).
    Whole,
    /// The nearest element anchored at the `Within` vertex.
    Within(ViewRef),
    /// No element anchored at the `Within` vertex is an ancestor: the
    /// element is outside the key.
    Outside,
}

/// A syntactic source of either shape a resolution reads.
#[derive(Debug)]
pub enum Source<'a> {
    /// A tree.
    Tree(TreeSource<'a>),
    /// A table instance.
    Table(&'a FInstance),
}

/// The name a path's last edge reads as a field: its label, or its target
/// vertex when it is unlabelled.
pub fn field_name(edge: &Edge) -> String {
    edge.name
        .as_ref()
        .map_or_else(|| edge.tgt.to_string(), Name::to_string)
}

/// The canonical bytes of a key value: the part count, then each part's
/// value through the canonical value encoding.
pub fn key_bytes(values: &[Value]) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    writer.len(values.len());
    for value in values {
        writer.value(value);
    }
    writer.into_bytes()
}

const fn present(value: Option<&FieldPresence>) -> Option<&Value> {
    match value {
        Some(FieldPresence::Present(Value::Null) | FieldPresence::Null | FieldPresence::Absent)
        | None => None,
        Some(FieldPresence::Present(value)) => Some(value),
    }
}

const fn present_value(value: Option<&Value>) -> Option<&Value> {
    match value {
        Some(Value::Null) | None => None,
        Some(value) => Some(value),
    }
}

fn value_or_null(value: Option<&FieldPresence>) -> Option<Value> {
    match value {
        Some(FieldPresence::Present(value)) => Some(value.clone()),
        Some(FieldPresence::Null) => Some(Value::Null),
        Some(FieldPresence::Absent) | None => None,
    }
}

impl<'a> Source<'a> {
    /// Read a tree.
    #[must_use]
    pub fn tree(tree: &'a WInstance) -> Self {
        let mut out: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut into: HashMap<u32, Vec<usize>> = HashMap::new();
        for (index, (parent, child, _)) in tree.arcs.iter().enumerate() {
            out.entry(*parent).or_default().push(index);
            into.entry(*child).or_default().push(index);
        }
        Self::Tree(TreeSource { tree, out, into })
    }

    /// Read a table instance.
    #[must_use]
    pub const fn table(table: &'a FInstance) -> Self {
        Self::Table(table)
    }

    /// The elements anchored at `vertex`, ascending.
    #[must_use]
    pub fn elements(&self, vertex: &str) -> Vec<ViewRef> {
        match self {
            Self::Tree(source) => {
                let mut ids: Vec<u32> = source
                    .tree
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.anchor.as_ref() == vertex)
                    .map(|(&id, _)| id)
                    .collect();
                ids.sort_unstable();
                ids.into_iter().map(ViewRef::Node).collect()
            }
            Self::Table(table) => table
                .rows(vertex)
                .into_iter()
                .map(|(id, _)| ViewRef::Row(id))
                .collect(),
        }
    }

    /// The vertex `at` is anchored at, when it is an element.
    #[must_use]
    pub fn anchor(&self, at: &ViewRef) -> Option<&'a Name> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => {
                source.tree.nodes.get(id).map(|node| &node.anchor)
            }
            (Self::Table(table), ViewRef::Row(id)) => table.table_of(*id),
            _ => None,
        }
    }

    /// The out-edges of `at`: a tree node's arcs in arc order, a row's
    /// foreign-key pairs in ascending `(edge, target)` order.
    #[must_use]
    pub fn out_steps(&self, at: &ViewRef) -> Vec<Step<'a>> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .out
                .get(id)
                .map(|arcs| {
                    arcs.iter()
                        .map(|&index| {
                            let (_, child, label) = &source.tree.arcs[index];
                            Step {
                                other: ViewRef::Node(*child),
                                label,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            (Self::Table(table), ViewRef::Row(id)) => table
                .foreign_keys()
                .into_iter()
                .flat_map(|(label, _)| {
                    table.targets_of(label, *id).map(move |tgt| Step {
                        other: ViewRef::Row(tgt),
                        label,
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The in-edges of `at`: a tree node's parent arc, a row's incoming
    /// foreign-key pairs in ascending `(edge, source)` order.
    #[must_use]
    pub fn in_steps(&self, at: &ViewRef) -> Vec<Step<'a>> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .into
                .get(id)
                .map(|arcs| {
                    arcs.iter()
                        .map(|&index| {
                            let (parent, _, label) = &source.tree.arcs[index];
                            Step {
                                other: ViewRef::Node(*parent),
                                label,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            (Self::Table(table), ViewRef::Row(id)) => table
                .foreign_keys()
                .into_iter()
                .flat_map(|(label, _)| {
                    table.sources_of(label, *id).map(move |src| Step {
                        other: ViewRef::Row(src),
                        label,
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The own value of a tree node; a row has none.
    #[must_use]
    pub fn own_value(&self, at: &ViewRef) -> Option<&'a Value> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .tree
                .nodes
                .get(id)
                .and_then(|node| present(node.value.as_ref())),
            _ => None,
        }
    }

    /// A named field: a tree node's extra field, a row's cell.
    #[must_use]
    pub fn field(&self, at: &ViewRef, name: &str) -> Option<&'a Value> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .tree
                .nodes
                .get(id)
                .and_then(|node| present_value(node.extra_fields.get(name))),
            (Self::Table(table), ViewRef::Row(id)) => {
                table.row(*id).and_then(|row| present_value(row.get(name)))
            }
            _ => None,
        }
    }

    /// The terminal value sites reached from `start` along `path`, with
    /// their values, in element order.
    ///
    /// The empty path names `start`'s own value. Otherwise each hop follows
    /// the out-edges labelled with it; at the last hop every node reached
    /// contributes its own-value site, and when none is reached, every node
    /// before it contributes the field named like the last edge. A site
    /// without a value (absent or null) is not reached.
    #[must_use]
    pub fn path_sites(
        &self,
        start: &ViewRef,
        path: &[Edge],
    ) -> Vec<(ResolutionValueSite, &'a Value)> {
        let Some((last, hops)) = path.split_last() else {
            return self
                .own_value(start)
                .map(|value| (ResolutionValueSite::Own(start.clone()), value))
                .into_iter()
                .collect();
        };
        let mut frontier = vec![start.clone()];
        for hop in hops {
            frontier = frontier
                .iter()
                .flat_map(|at| self.out_steps(at))
                .filter(|step| step.label == hop)
                .map(|step| step.other)
                .collect();
        }
        let reached: Vec<ViewRef> = frontier
            .iter()
            .flat_map(|at| self.out_steps(at))
            .filter(|step| step.label == last)
            .map(|step| step.other)
            .collect();
        if reached.is_empty() {
            let field = field_name(last);
            frontier
                .into_iter()
                .filter_map(|holder| {
                    self.field(&holder, &field).map(|value| {
                        (
                            ResolutionValueSite::Field {
                                holder,
                                field: field.clone(),
                            },
                            value,
                        )
                    })
                })
                .collect()
        } else {
            reached
                .into_iter()
                .filter_map(|at| {
                    self.own_value(&at)
                        .map(|value| (ResolutionValueSite::Own(at), value))
                })
                .collect()
        }
    }

    /// The terminal values reached from `start` along `path`, preserving an
    /// explicit null while continuing to omit an absent field.
    ///
    /// Resolution normally treats null as no value. Foreign-key null matching
    /// needs the stricter distinction between an explicit null and absence, so
    /// this reader is kept separate from [`Self::path_sites`].
    #[must_use]
    pub fn path_values_or_null(&self, start: &ViewRef, path: &[Edge]) -> Vec<Value> {
        let own = |at: &ViewRef| match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .tree
                .nodes
                .get(id)
                .and_then(|node| value_or_null(node.value.as_ref())),
            _ => None,
        };
        let field = |at: &ViewRef, name: &str| match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .tree
                .nodes
                .get(id)
                .and_then(|node| node.extra_fields.get(name).cloned()),
            (Self::Table(table), ViewRef::Row(id)) => {
                table.row(*id).and_then(|row| row.get(name).cloned())
            }
            _ => None,
        };
        let Some((last, hops)) = path.split_last() else {
            return own(start).into_iter().collect();
        };
        let mut frontier = vec![start.clone()];
        for hop in hops {
            frontier = frontier
                .iter()
                .flat_map(|at| self.out_steps(at))
                .filter(|step| step.label == hop)
                .map(|step| step.other)
                .collect();
        }
        let reached: Vec<ViewRef> = frontier
            .iter()
            .flat_map(|at| self.out_steps(at))
            .filter(|step| step.label == last)
            .map(|step| step.other)
            .collect();
        if reached.is_empty() {
            let name = field_name(last);
            frontier
                .into_iter()
                .filter_map(|holder| field(&holder, &name))
                .collect()
        } else {
            reached.into_iter().filter_map(|at| own(&at)).collect()
        }
    }

    /// The index of `at` among the targets of the ordered edge `within`
    /// leaving its parent; a row has none.
    #[must_use]
    pub fn position_of(&self, at: &ViewRef, within: &Edge) -> Option<u32> {
        let Self::Tree(_) = self else {
            return None;
        };
        let into = self
            .in_steps(at)
            .into_iter()
            .find(|step| step.label == within)?;
        let index = self
            .out_steps(&into.other)
            .into_iter()
            .filter(|step| step.label == within)
            .position(|step| step.other == *at)?;
        u32::try_from(index).ok()
    }

    /// The value of every part of `key` for `at`, or `None` when some part
    /// has none: a path part takes the first value along its path, a
    /// position part the element's position.
    #[must_use]
    pub fn key_values(&self, at: &ViewRef, key: &Key) -> Option<Vec<Value>> {
        key.parts
            .iter()
            .map(|part| match part {
                KeyPart::Path(path) => self
                    .path_sites(at, path)
                    .into_iter()
                    .next()
                    .map(|(_, value)| value.clone()),
                KeyPart::Position { within } => self
                    .position_of(at, within)
                    .map(|position| Value::Int(i64::from(position))),
            })
            .collect()
    }

    /// The scope element of `at` under `scope`.
    ///
    /// For `Within(vertex)`, the nearest element anchored at `vertex`
    /// reached backwards along in-edges, the least of the equally near ones,
    /// or [`ScopeOf::Outside`] when there is none. Document and global
    /// scopes are the whole instance.
    #[must_use]
    pub fn scope_of(&self, at: &ViewRef, scope: &KeyScope) -> ScopeOf {
        let KeyScope::Within(vertex) = scope else {
            return ScopeOf::Whole;
        };
        let mut seen = BTreeSet::from([at.clone()]);
        let mut frontier = vec![at.clone()];
        while !frontier.is_empty() {
            let mut next: Vec<ViewRef> = frontier
                .iter()
                .flat_map(|element| self.in_steps(element))
                .map(|step| step.other)
                .filter(|element| seen.insert(element.clone()))
                .collect();
            next.sort();
            if let Some(found) = next
                .iter()
                .find(|element| self.anchor(element).is_some_and(|anchor| anchor == vertex))
            {
                return ScopeOf::Within(found.clone());
            }
            frontier = next;
        }
        ScopeOf::Outside
    }

    /// Every element in document order: a tree's preorder from its root,
    /// following each node's arcs in arc order, then any node the root does
    /// not reach in ascending id; a table's rows in ascending id.
    #[must_use]
    pub fn document_order(&self) -> Vec<ViewRef> {
        match self {
            Self::Tree(source) => {
                let tree = source.tree;
                let mut order = Vec::with_capacity(tree.nodes.len());
                let mut seen = BTreeSet::new();
                let mut stack = vec![tree.root];
                while let Some(node) = stack.pop() {
                    if !tree.nodes.contains_key(&node) || !seen.insert(node) {
                        continue;
                    }
                    order.push(ViewRef::Node(node));
                    if let Some(arcs) = source.out.get(&node) {
                        stack.extend(arcs.iter().rev().map(|&index| source.tree.arcs[index].1));
                    }
                }
                let mut rest: Vec<u32> = tree
                    .nodes
                    .keys()
                    .copied()
                    .filter(|id| !seen.contains(id))
                    .collect();
                rest.sort_unstable();
                order.extend(rest.into_iter().map(ViewRef::Node));
                order
            }
            Self::Table(table) => table.row_ids().into_iter().map(ViewRef::Row).collect(),
        }
    }

    /// The parent of a tree node and the position of its arc in the tree's
    /// arc list; `None` for a root or a row.
    #[must_use]
    pub fn parent_arc(&self, at: &ViewRef) -> Option<(u32, usize)> {
        match (self, at) {
            (Self::Tree(source), ViewRef::Node(id)) => source
                .into
                .get(id)
                .and_then(|arcs| arcs.first())
                .map(|&index| (source.tree.arcs[index].0, index)),
            _ => None,
        }
    }

    /// The node ids of the subtree rooted at `root`, ascending.
    #[must_use]
    pub fn subtree(&self, root: u32) -> BTreeSet<u32> {
        let mut nodes = BTreeSet::new();
        let Self::Tree(source) = self else {
            return nodes;
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if nodes.insert(node) {
                if let Some(arcs) = source.out.get(&node) {
                    stack.extend(arcs.iter().map(|&index| source.tree.arcs[index].1));
                }
            }
        }
        nodes
    }
}

/// One key's index over a source: the elements anchored at the key's vertex
/// bucketed by scope and key bytes. An element with a part that has no value
/// takes no part, and so does an element of a `Within` key with no ancestor
/// at the scope vertex.
#[derive(Debug, Default)]
pub struct KeyIndex {
    buckets: BTreeMap<(Option<ViewRef>, Vec<u8>), Vec<ViewRef>>,
}

impl KeyIndex {
    /// Index the elements of `vertex` by `key`.
    ///
    /// # Errors
    ///
    /// A declared equality cannot be evaluated exactly.
    pub fn build(source: &Source<'_>, vertex: &str, key: &Key) -> Result<Self, KeyingError> {
        let mut buckets: BTreeMap<(Option<ViewRef>, Vec<u8>), Vec<ViewRef>> = BTreeMap::new();
        for element in source.elements(vertex) {
            let Some(values) = source.key_values(&element, key) else {
                continue;
            };
            let scope = match source.scope_of(&element, &key.scope) {
                ScopeOf::Whole => None,
                ScopeOf::Within(scope) => Some(scope),
                ScopeOf::Outside => continue,
            };
            buckets
                .entry((scope, canonical_key_bytes(key, &values)?))
                .or_default()
                .push(element);
        }
        Ok(Self { buckets })
    }

    /// The elements of one bucket, ascending.
    #[must_use]
    pub fn lookup(&self, scope: Option<&ViewRef>, bytes: &[u8]) -> Vec<ViewRef> {
        self.buckets
            .get(&(scope.cloned(), bytes.to_vec()))
            .cloned()
            .unwrap_or_default()
    }
}

/// The tree node a `ViewRef` names.
#[must_use]
pub const fn node_of(at: &ViewRef) -> Option<u32> {
    match at {
        ViewRef::Node(id) => Some(*id),
        _ => None,
    }
}
