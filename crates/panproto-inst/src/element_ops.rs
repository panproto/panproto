//! Operations on the category of elements of an instance presheaf.
//!
//! An instance is an attributed C-set: a functor `F: Schema → Rel` from
//! the schema category to the category of sets and relations. The
//! **category of elements** `∫F` (Grothendieck construction) has objects
//! `(v, x)` where `v` is a schema vertex and `x ∈ F(v)`.
//!
//! [`ElementOps`] abstracts the three operations the query engine needs
//! to navigate and inspect elements of `∫F`:
//!
//! 1. **Fiber selection**: `F(v)` — all elements over vertex `v`
//! 2. **Relational pushforward**: `F(e)(S)` — direct image along edge `e`
//! 3. **Stalk projection**: observable data at an element, projected from
//!    the fiber's dependent sum to a flat evaluation environment
//!
//! This trait follows the [`AcsetOps`](crate::AcsetOps) naming convention
//! and is implemented for all three instance shapes.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Literal};
use panproto_gat::Name;

use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::wtype::{
    WInstance, build_env_with_children, collect_scalar_child_values, value_to_expr_literal,
};

/// Operations on the category of elements `∫F` of an instance presheaf.
///
/// Elements are uniformly addressed by `u32` IDs. For [`WInstance`] and
/// [`GInstance`], these are node IDs. For [`FInstance`], these encode
/// `(table_ordinal, row_index)` pairs via a faithful bijection (see
/// [`encode_finstance_id`] / [`decode_finstance_id`]).
pub trait ElementOps {
    /// Fiber selection: all elements over schema vertex `v`.
    ///
    /// Returns `F(v) = {x | sort(x) = v}` as a vector of element IDs.
    fn fiber(&self, vertex: &Name) -> Vec<u32>;

    /// Relational pushforward along a schema edge.
    ///
    /// Given `S ⊆ F(v)` and edge kind `e: v → w`, computes the direct
    /// image `F(e)(S) = {y ∈ F(w) | ∃x ∈ S, (x,y) ∈ F(e)}`.
    ///
    /// The result has set semantics (deduplicated). For tree-shaped
    /// instances this is automatic; for graph-shaped instances with
    /// cycles, explicit dedup is required.
    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32>;

    /// Stalk projection: observable data at element `x`.
    ///
    /// Constructs the evaluation environment from the fiber projection
    /// `π_stalk: Fiber(sort(x)) → Env`, which includes:
    /// - Local attributes (`extra_fields` for W/G-instances, columns for F-instances)
    /// - Scalar child values via the dependent-sum projection
    ///   `Σ_{e: v→w} π_scalar(Fiber(target(e, x)))`
    /// - Metadata: `_anchor`, `_id`, `_value`, `_children_count`/`_edge_count`
    ///
    /// `extra_fields` take precedence over child scalars on key collision
    /// (left-biased coproduct injection: transform-local state overrides
    /// structural data).
    fn stalk(&self, element_id: u32) -> panproto_expr::Env;

    /// Evaluate a graph traversal builtin at an element.
    ///
    /// Interprets `Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor`
    /// in the internal logic of the category of elements.
    ///
    /// # Errors
    ///
    /// Returns `panproto_expr::ExprError` if argument types are wrong
    /// or the context node cannot be resolved.
    fn eval_graph_builtin(
        &self,
        op: BuiltinOp,
        args: &[Literal],
        context: Option<u32>,
    ) -> Result<Literal, panproto_expr::ExprError>;

    /// Attribute map: all observable fields at element `x`.
    ///
    /// Returns the same data as [`stalk`](Self::stalk) but as raw
    /// [`Value`]s for query result projection, without converting to
    /// expression literals.
    fn attributes(&self, element_id: u32) -> HashMap<String, Value>;

    /// Sort of element `x`: the schema vertex `v` such that `x ∈ F(v)`.
    fn sort(&self, element_id: u32) -> Option<Name>;

    /// Leaf value at element `x`, if present.
    fn element_value(&self, element_id: u32) -> Option<FieldPresence>;
}

// ---------------------------------------------------------------------------
// WInstance implementation
// ---------------------------------------------------------------------------

impl ElementOps for WInstance {
    fn fiber(&self, vertex: &Name) -> Vec<u32> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.anchor == *vertex)
            .map(|(id, _)| *id)
            .collect()
    }

    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32> {
        let mut result = Vec::new();
        for &node_id in elements {
            for &(src, tgt, ref edge) in &self.arcs {
                if src == node_id && edge.kind == *edge_kind {
                    result.push(tgt);
                }
            }
        }
        result
    }

    fn stalk(&self, element_id: u32) -> panproto_expr::Env {
        let Some(node) = self.nodes.get(&element_id) else {
            return panproto_expr::Env::new();
        };
        crate::query::build_node_env(node, self)
    }

    fn eval_graph_builtin(
        &self,
        op: BuiltinOp,
        args: &[Literal],
        context: Option<u32>,
    ) -> Result<Literal, panproto_expr::ExprError> {
        winstance_graph_builtin(self, op, args, context)
    }

    fn attributes(&self, element_id: u32) -> HashMap<String, Value> {
        let Some(node) = self.nodes.get(&element_id) else {
            return HashMap::new();
        };
        let scalars = collect_scalar_child_values(self, element_id);
        let mut combined = scalars;
        for (key, val) in &node.extra_fields {
            combined.insert(key.clone(), val.clone());
        }
        combined
    }

    fn sort(&self, element_id: u32) -> Option<Name> {
        self.nodes.get(&element_id).map(|n| n.anchor.clone())
    }

    fn element_value(&self, element_id: u32) -> Option<FieldPresence> {
        self.nodes.get(&element_id).and_then(|n| n.value.clone())
    }
}

// ---------------------------------------------------------------------------
// GInstance implementation
// ---------------------------------------------------------------------------

impl ElementOps for GInstance {
    fn fiber(&self, vertex: &Name) -> Vec<u32> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.anchor == *vertex)
            .map(|(id, _)| *id)
            .collect()
    }

    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32> {
        let mut result = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for &node_id in elements {
            for &(src, tgt, ref edge) in &self.edges {
                if src == node_id && edge.kind == *edge_kind && seen.insert(tgt) {
                    result.push(tgt);
                }
            }
        }
        result
    }

    fn stalk(&self, element_id: u32) -> panproto_expr::Env {
        let Some(node) = self.nodes.get(&element_id) else {
            return panproto_expr::Env::new();
        };

        // Collect scalar values from outgoing edge targets (analogous to
        // child scalar collection in WInstance). This is the dependent-sum
        // projection for graph-shaped instances.
        let child_scalars = collect_graph_scalar_children(self, element_id);
        let mut env = build_env_with_children(&node.extra_fields, &child_scalars);

        // Metadata
        env = env.extend(
            Arc::from("_anchor"),
            Literal::Str(node.anchor.as_ref().into()),
        );
        env = env.extend(Arc::from("_id"), Literal::Int(i64::from(node.id)));

        if let Some(val) = self.values.get(&element_id) {
            env = env.extend(Arc::from("_value"), value_to_expr_literal(val));
        }

        let edge_count = self
            .edges
            .iter()
            .filter(|(src, _, _)| *src == element_id)
            .count();
        #[allow(clippy::cast_possible_wrap)]
        {
            env = env.extend(Arc::from("_edge_count"), Literal::Int(edge_count as i64));
        }

        env
    }

    fn eval_graph_builtin(
        &self,
        op: BuiltinOp,
        args: &[Literal],
        context: Option<u32>,
    ) -> Result<Literal, panproto_expr::ExprError> {
        ginstance_graph_builtin(self, op, args, context)
    }

    fn attributes(&self, element_id: u32) -> HashMap<String, Value> {
        let Some(node) = self.nodes.get(&element_id) else {
            return HashMap::new();
        };
        let child_scalars = collect_graph_scalar_children(self, element_id);
        let mut combined = child_scalars;
        for (key, val) in &node.extra_fields {
            combined.insert(key.clone(), val.clone());
        }
        combined
    }

    fn sort(&self, element_id: u32) -> Option<Name> {
        self.nodes.get(&element_id).map(|n| n.anchor.clone())
    }

    fn element_value(&self, element_id: u32) -> Option<FieldPresence> {
        self.values
            .get(&element_id)
            .map(|v| FieldPresence::Present(v.clone()))
    }
}

/// Collect scalar values from a graph node's outgoing edge targets.
///
/// Analogous to [`collect_scalar_child_values`] for W-type instances:
/// this is the dependent-sum projection `Σ_{e: v→w} π_scalar(Fiber(w))`
/// for graph-shaped instances. Uses `edge.name` (or `edge.tgt`) as the
/// field name, matching the W-type convention.
fn collect_graph_scalar_children(instance: &GInstance, node_id: u32) -> HashMap<String, Value> {
    let mut result = HashMap::new();
    for &(src, tgt, ref edge) in &instance.edges {
        if src != node_id {
            continue;
        }
        // Check node values first (GInstance stores values separately)
        if let Some(val) = instance.values.get(&tgt) {
            let field_name = edge.name.as_deref().unwrap_or(&*edge.tgt);
            result.insert(field_name.to_string(), val.clone());
        } else if let Some(tgt_node) = instance.nodes.get(&tgt) {
            // Fall back to node.value if present
            if let Some(FieldPresence::Present(val)) = &tgt_node.value {
                let field_name = edge.name.as_deref().unwrap_or(&*edge.tgt);
                result.insert(field_name.to_string(), val.clone());
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// FInstance implementation
// ---------------------------------------------------------------------------

/// Encode an [`FInstance`] element as a `u32` ID.
///
/// [`FInstance`] elements are `(table, row)` pairs. We encode them as
/// `(table_ordinal << 16) | row_index`, where `table_ordinal` is the
/// index of the table name in lexicographic order. This bijection is
/// faithful when each table has fewer than 2^16 rows.
#[must_use]
pub const fn encode_finstance_id(table_ordinal: u16, row_index: u16) -> u32 {
    (table_ordinal as u32) << 16 | row_index as u32
}

/// Decode an [`FInstance`] element ID to `(table_ordinal, row_index)`.
#[must_use]
pub const fn decode_finstance_id(id: u32) -> (u16, u16) {
    #[allow(clippy::cast_possible_truncation)]
    ((id >> 16) as u16, id as u16)
}

impl FInstance {
    /// Sorted table names for deterministic ordinal assignment.
    fn sorted_table_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tables.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Find the ordinal for a table name.
    fn table_ordinal(&self, table_name: &str) -> Option<u16> {
        let names = self.sorted_table_names();
        names
            .binary_search(&table_name)
            .ok()
            .and_then(|i| u16::try_from(i).ok())
    }

    /// Build the ordinal-to-table-name mapping (sorted).
    ///
    /// Returns `&str` references into `self.tables` keys, ordered by
    /// lexicographic sort. This avoids repeated sorting in hot paths.
    fn ordinal_map(&self) -> Vec<&str> {
        self.sorted_table_names()
    }

    /// Resolve a synthetic element ID to `(table_name, row_index)`.
    ///
    /// The returned `&str` borrows from `self.tables` keys.
    fn resolve_element(&self, element_id: u32) -> Option<(&str, usize)> {
        let (table_ord, row_idx) = decode_finstance_id(element_id);
        let names = self.sorted_table_names();
        let table_name = *names.get(usize::from(table_ord))?;
        let rows = self.tables.get(table_name)?;
        let row = usize::from(row_idx);
        if row < rows.len() {
            Some((table_name, row))
        } else {
            None
        }
    }
}

impl ElementOps for FInstance {
    fn fiber(&self, vertex: &Name) -> Vec<u32> {
        let vertex_str: &str = vertex.as_ref();
        let Some(rows) = self.tables.get(vertex_str) else {
            return Vec::new();
        };
        let Some(table_ord) = self.table_ordinal(vertex_str) else {
            return Vec::new();
        };
        (0..rows.len())
            .filter_map(|i| {
                u16::try_from(i)
                    .ok()
                    .map(|ri| encode_finstance_id(table_ord, ri))
            })
            .collect()
    }

    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32> {
        let element_set: rustc_hash::FxHashSet<u32> = elements.iter().copied().collect();
        let names = self.ordinal_map();
        let mut result = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        for (edge, pairs) in &self.foreign_keys {
            if edge.kind != *edge_kind {
                continue;
            }
            let Some(src_ord) = names
                .binary_search(&&*edge.src)
                .ok()
                .and_then(|i| u16::try_from(i).ok())
            else {
                continue;
            };
            let Some(tgt_ord) = names
                .binary_search(&&*edge.tgt)
                .ok()
                .and_then(|i| u16::try_from(i).ok())
            else {
                continue;
            };

            for &(src_row, tgt_row) in pairs {
                let Ok(src_row_u16) = u16::try_from(src_row) else {
                    continue;
                };
                let Ok(tgt_row_u16) = u16::try_from(tgt_row) else {
                    continue;
                };
                let src_id = encode_finstance_id(src_ord, src_row_u16);
                if element_set.contains(&src_id) {
                    let tgt_id = encode_finstance_id(tgt_ord, tgt_row_u16);
                    if seen.insert(tgt_id) {
                        result.push(tgt_id);
                    }
                }
            }
        }

        result
    }

    fn stalk(&self, element_id: u32) -> panproto_expr::Env {
        let Some((table_name, row_idx)) = self.resolve_element(element_id) else {
            return panproto_expr::Env::new();
        };
        let Some(rows) = self.tables.get(table_name) else {
            return panproto_expr::Env::new();
        };
        let Some(row) = rows.get(row_idx) else {
            return panproto_expr::Env::new();
        };

        // FInstance has no structural children; the stalk is just
        // the column values (the fiber F(v) is a flat set of records).
        let mut env = crate::wtype::build_env_from_extra_fields(row);

        // Metadata
        env = env.extend(Arc::from("_anchor"), Literal::Str(table_name.into()));
        env = env.extend(Arc::from("_id"), Literal::Int(i64::from(element_id)));

        // FK out-degree
        let fk_count = self
            .foreign_keys
            .iter()
            .filter(|(edge, _)| edge.src.as_ref() == table_name)
            .flat_map(|(_, pairs)| pairs.iter())
            .filter(|(src_row, _)| *src_row == row_idx)
            .count();
        #[allow(clippy::cast_possible_wrap)]
        {
            env = env.extend(Arc::from("_edge_count"), Literal::Int(fk_count as i64));
        }

        env
    }

    fn eval_graph_builtin(
        &self,
        op: BuiltinOp,
        args: &[Literal],
        context: Option<u32>,
    ) -> Result<Literal, panproto_expr::ExprError> {
        finstance_graph_builtin(self, op, args, context)
    }

    fn attributes(&self, element_id: u32) -> HashMap<String, Value> {
        let Some((table_name, row_idx)) = self.resolve_element(element_id) else {
            return HashMap::new();
        };
        self.tables
            .get(table_name)
            .and_then(|rows| rows.get(row_idx))
            .cloned()
            .unwrap_or_default()
    }

    fn sort(&self, element_id: u32) -> Option<Name> {
        self.resolve_element(element_id)
            .map(|(table_name, _)| Name::from(table_name))
    }

    fn element_value(&self, _element_id: u32) -> Option<FieldPresence> {
        // FInstance rows have no distinguished leaf value.
        None
    }
}

// ---------------------------------------------------------------------------
// Graph builtin implementations
// ---------------------------------------------------------------------------

/// Resolve a node reference from a literal (int or "self").
fn resolve_node_ref(lit: &Literal, context: Option<u32>) -> Result<u32, panproto_expr::ExprError> {
    match lit {
        Literal::Int(id) => u32::try_from(*id).map_err(|_| panproto_expr::ExprError::TypeError {
            expected: "non-negative int fitting u32".into(),
            got: format!("{id}"),
        }),
        Literal::Str(s) if s == "self" => context.ok_or_else(|| {
            panproto_expr::ExprError::UnboundVariable("self (no context node)".into())
        }),
        _ => Err(panproto_expr::ExprError::TypeError {
            expected: "int or \"self\"".into(),
            got: lit.type_name().into(),
        }),
    }
}

/// Convert a [`Node`]'s data to a [`Literal::Record`] for expression evaluation.
fn node_to_literal(node: &Node) -> Literal {
    let mut fields: Vec<(Arc<str>, Literal)> = Vec::new();
    fields.push((Arc::from("_id"), Literal::Int(i64::from(node.id))));
    fields.push((
        Arc::from("_anchor"),
        Literal::Str(node.anchor.as_ref().into()),
    ));
    for (key, val) in &node.extra_fields {
        fields.push((Arc::from(key.as_str()), value_to_expr_literal(val)));
    }
    Literal::Record(fields)
}

/// Graph builtins for [`WInstance`].
fn winstance_graph_builtin(
    instance: &WInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    match op {
        BuiltinOp::Edge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let edge_name = Name::from(edge_kind);
            for &(src, tgt, ref edge) in &instance.arcs {
                if src == node_id && edge.kind == edge_name {
                    if let Some(node) = instance.nodes.get(&tgt) {
                        return Ok(node_to_literal(node));
                    }
                    return Ok(Literal::Null);
                }
            }
            Ok(Literal::Null)
        }
        BuiltinOp::Children => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let mut children = Vec::new();
            for &(src, tgt, _) in &instance.arcs {
                if src == node_id {
                    if let Some(node) = instance.nodes.get(&tgt) {
                        children.push(node_to_literal(node));
                    }
                }
            }
            Ok(Literal::List(children))
        }
        BuiltinOp::HasEdge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let edge_name = Name::from(edge_kind);
            let found = instance
                .arcs
                .iter()
                .any(|(src, _, edge)| *src == node_id && edge.kind == edge_name);
            Ok(Literal::Bool(found))
        }
        BuiltinOp::EdgeCount => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let count = instance
                .arcs
                .iter()
                .filter(|(src, _, _)| *src == node_id)
                .count();
            #[allow(clippy::cast_possible_wrap)]
            Ok(Literal::Int(count as i64))
        }
        BuiltinOp::Anchor => {
            let node_id = resolve_node_ref(&args[0], context)?;
            instance
                .nodes
                .get(&node_id)
                .map_or(Ok(Literal::Null), |node| {
                    Ok(Literal::Str(node.anchor.as_ref().into()))
                })
        }
        _ => Ok(Literal::Null),
    }
}

/// Graph builtins for [`GInstance`].
fn ginstance_graph_builtin(
    instance: &GInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    match op {
        BuiltinOp::Edge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let edge_name = Name::from(edge_kind);
            for &(src, tgt, ref edge) in &instance.edges {
                if src == node_id && edge.kind == edge_name {
                    if let Some(node) = instance.nodes.get(&tgt) {
                        return Ok(node_to_literal(node));
                    }
                    return Ok(Literal::Null);
                }
            }
            Ok(Literal::Null)
        }
        BuiltinOp::Children => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let mut children = Vec::new();
            for &(src, tgt, _) in &instance.edges {
                if src == node_id {
                    if let Some(node) = instance.nodes.get(&tgt) {
                        children.push(node_to_literal(node));
                    }
                }
            }
            Ok(Literal::List(children))
        }
        BuiltinOp::HasEdge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let edge_name = Name::from(edge_kind);
            let found = instance
                .edges
                .iter()
                .any(|(src, _, edge)| *src == node_id && edge.kind == edge_name);
            Ok(Literal::Bool(found))
        }
        BuiltinOp::EdgeCount => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let count = instance
                .edges
                .iter()
                .filter(|(src, _, _)| *src == node_id)
                .count();
            #[allow(clippy::cast_possible_wrap)]
            Ok(Literal::Int(count as i64))
        }
        BuiltinOp::Anchor => {
            let node_id = resolve_node_ref(&args[0], context)?;
            instance
                .nodes
                .get(&node_id)
                .map_or(Ok(Literal::Null), |node| {
                    Ok(Literal::Str(node.anchor.as_ref().into()))
                })
        }
        _ => Ok(Literal::Null),
    }
}

/// Convert an [`FInstance`] row to a [`Literal::Record`].
fn finstance_row_to_literal(row: &HashMap<String, Value>) -> Literal {
    let fields: Vec<(Arc<str>, Literal)> = row
        .iter()
        .map(|(k, v)| (Arc::from(k.as_str()), value_to_expr_literal(v)))
        .collect();
    Literal::Record(fields)
}

/// Graph builtins for [`FInstance`]: maps graph traversal to FK semantics.
fn finstance_graph_builtin(
    instance: &FInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    match op {
        BuiltinOp::Edge => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let Some((src_table, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::Null);
            };
            let edge_name = Name::from(edge_kind);
            for (edge, pairs) in &instance.foreign_keys {
                if edge.kind == edge_name && edge.src.as_ref() == src_table {
                    for &(s, t) in pairs {
                        if s == src_row {
                            if let Some(row) =
                                instance.tables.get(&*edge.tgt).and_then(|r| r.get(t))
                            {
                                return Ok(finstance_row_to_literal(row));
                            }
                        }
                    }
                }
            }
            Ok(Literal::Null)
        }
        BuiltinOp::Children => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let Some((src_table, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::List(Vec::new()));
            };
            let mut children = Vec::new();
            for (edge, pairs) in &instance.foreign_keys {
                if edge.src.as_ref() != src_table {
                    continue;
                }
                for &(s, t) in pairs {
                    if s == src_row {
                        if let Some(row) = instance.tables.get(&*edge.tgt).and_then(|r| r.get(t)) {
                            children.push(finstance_row_to_literal(row));
                        }
                    }
                }
            }
            Ok(Literal::List(children))
        }
        BuiltinOp::HasEdge => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let edge_kind =
                args[1]
                    .as_str()
                    .ok_or_else(|| panproto_expr::ExprError::TypeError {
                        expected: "string".into(),
                        got: args[1].type_name().into(),
                    })?;
            let edge_name = Name::from(edge_kind);

            let Some((src_table, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::Bool(false));
            };

            let found = instance.foreign_keys.iter().any(|(edge, pairs)| {
                edge.kind == edge_name
                    && edge.src.as_ref() == src_table
                    && pairs.iter().any(|(s, _)| *s == src_row)
            });
            Ok(Literal::Bool(found))
        }
        BuiltinOp::EdgeCount => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let Some((src_table, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::Int(0));
            };

            let count: usize = instance
                .foreign_keys
                .iter()
                .filter(|(edge, _)| edge.src.as_ref() == src_table)
                .flat_map(|(_, pairs)| pairs.iter())
                .filter(|(s, _)| *s == src_row)
                .count();
            #[allow(clippy::cast_possible_wrap)]
            Ok(Literal::Int(count as i64))
        }
        BuiltinOp::Anchor => {
            let element_id = resolve_node_ref(&args[0], context)?;
            instance
                .resolve_element(element_id)
                .map_or(Ok(Literal::Null), |(table_name, _)| {
                    Ok(Literal::Str(table_name.into()))
                })
        }
        _ => Ok(Literal::Null),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::Edge as SchemaEdge;

    // -- WInstance tests --

    fn make_winstance_with_children() -> WInstance {
        let mut nodes = HashMap::new();

        // Parent "binding" node with no extra_fields
        nodes.insert(0, Node::new(0, "binding"));

        // Child "binding.var" with leaf value "x0"
        nodes.insert(
            1,
            Node::new(1, "binding.var").with_value(FieldPresence::Present(Value::Str("x0".into()))),
        );

        // Child "binding.type" with leaf value "noun"
        nodes.insert(
            2,
            Node::new(2, "binding.type")
                .with_value(FieldPresence::Present(Value::Str("noun".into()))),
        );

        let arcs = vec![
            (
                0,
                1,
                SchemaEdge {
                    src: "binding".into(),
                    tgt: "binding.var".into(),
                    kind: "prop".into(),
                    name: Some("var".into()),
                },
            ),
            (
                0,
                2,
                SchemaEdge {
                    src: "binding".into(),
                    tgt: "binding.type".into(),
                    kind: "prop".into(),
                    name: Some("type".into()),
                },
            ),
        ];

        WInstance::new(nodes, arcs, vec![], 0, Name::from("binding"))
    }

    #[test]
    fn winstance_fiber_selects_by_anchor() {
        let inst = make_winstance_with_children();
        let fibers = inst.fiber(&Name::from("binding"));
        assert_eq!(fibers.len(), 1);
        assert_eq!(fibers[0], 0);
    }

    #[test]
    fn winstance_pushforward_follows_arcs() {
        let inst = make_winstance_with_children();
        let children = inst.pushforward(&[0], &Name::from("prop"));
        assert_eq!(children.len(), 2);
        assert!(children.contains(&1));
        assert!(children.contains(&2));
    }

    #[test]
    fn winstance_stalk_includes_child_scalars() {
        let inst = make_winstance_with_children();
        let env = inst.stalk(0);
        // "var" should be bound from the child node's value
        let config = panproto_expr::EvalConfig::default();
        let var_expr = panproto_expr::Expr::Var("var".into());
        let result = panproto_expr::eval(&var_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("x0".into()));

        let type_expr = panproto_expr::Expr::Var("type".into());
        let result = panproto_expr::eval(&type_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("noun".into()));
    }

    #[test]
    fn winstance_attributes_includes_child_scalars() {
        let inst = make_winstance_with_children();
        let attrs = inst.attributes(0);
        assert_eq!(attrs.get("var"), Some(&Value::Str("x0".into())));
        assert_eq!(attrs.get("type"), Some(&Value::Str("noun".into())));
    }

    #[test]
    fn winstance_extra_fields_override_child_scalars() {
        let mut nodes = HashMap::new();
        let mut parent = Node::new(0, "thing");
        // extra_fields has "name" = "override"
        parent
            .extra_fields
            .insert("name".into(), Value::Str("override".into()));
        nodes.insert(0, parent);

        // Child also provides "name" = "original" via edge.name
        nodes.insert(
            1,
            Node::new(1, "thing.name")
                .with_value(FieldPresence::Present(Value::Str("original".into()))),
        );

        let arcs = vec![(
            0,
            1,
            SchemaEdge {
                src: "thing".into(),
                tgt: "thing.name".into(),
                kind: "prop".into(),
                name: Some("name".into()),
            },
        )];

        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("thing"));
        let attrs = inst.attributes(0);
        // extra_fields take precedence (left-biased coproduct)
        assert_eq!(attrs.get("name"), Some(&Value::Str("override".into())));
    }

    // -- GInstance tests --

    fn make_ginstance_with_values() -> GInstance {
        let mut person_a = Node::new(0, "person");
        person_a
            .extra_fields
            .insert("role".into(), Value::Str("manager".into()));

        GInstance::new()
            .with_node(person_a)
            .with_node(Node::new(1, "person"))
            .with_node(Node::new(2, "department"))
            .with_edge(
                0,
                1,
                SchemaEdge {
                    src: "person".into(),
                    tgt: "person".into(),
                    kind: "knows".into(),
                    name: None,
                },
            )
            .with_edge(
                0,
                2,
                SchemaEdge {
                    src: "person".into(),
                    tgt: "department".into(),
                    kind: "works_in".into(),
                    name: Some("department".into()),
                },
            )
            .with_value(0, Value::Str("Alice".into()))
            .with_value(1, Value::Str("Bob".into()))
            .with_value(2, Value::Str("Engineering".into()))
    }

    #[test]
    fn ginstance_fiber_selects_by_anchor() {
        let g = make_ginstance_with_values();
        let persons = g.fiber(&Name::from("person"));
        assert_eq!(persons.len(), 2);
        let depts = g.fiber(&Name::from("department"));
        assert_eq!(depts.len(), 1);
    }

    #[test]
    fn ginstance_pushforward_with_dedup() {
        // Create a graph with two paths to the same target (cycle-like)
        let g = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "a"))
            .with_node(Node::new(2, "b"))
            .with_edge(
                0,
                2,
                SchemaEdge {
                    src: "a".into(),
                    tgt: "b".into(),
                    kind: "link".into(),
                    name: None,
                },
            )
            .with_edge(
                1,
                2,
                SchemaEdge {
                    src: "a".into(),
                    tgt: "b".into(),
                    kind: "link".into(),
                    name: None,
                },
            );

        // Both source nodes reach the same target; result should be deduplicated
        let result = g.pushforward(&[0, 1], &Name::from("link"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 2);
    }

    #[test]
    fn ginstance_stalk_includes_edge_target_values() {
        let g = make_ginstance_with_values();
        let env = g.stalk(0);
        let config = panproto_expr::EvalConfig::default();

        // "department" should be bound from edge target value via edge.name
        let dept_expr = panproto_expr::Expr::Var("department".into());
        let result = panproto_expr::eval(&dept_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("Engineering".into()));

        // "role" should come from extra_fields
        let role_expr = panproto_expr::Expr::Var("role".into());
        let result = panproto_expr::eval(&role_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("manager".into()));

        // "_value" should be Alice
        let val_expr = panproto_expr::Expr::Var("_value".into());
        let result = panproto_expr::eval(&val_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("Alice".into()));
    }

    #[test]
    fn ginstance_graph_builtins() {
        let g = make_ginstance_with_values();

        // EdgeCount for node 0 (has 2 outgoing edges)
        let count = g
            .eval_graph_builtin(BuiltinOp::EdgeCount, &[Literal::Int(0)], Some(0))
            .unwrap();
        assert_eq!(count, Literal::Int(2));

        // HasEdge for "knows" from node 0
        let has = g
            .eval_graph_builtin(
                BuiltinOp::HasEdge,
                &[Literal::Int(0), Literal::Str("knows".into())],
                Some(0),
            )
            .unwrap();
        assert_eq!(has, Literal::Bool(true));

        // HasEdge for "nonexistent" from node 0
        let has_not = g
            .eval_graph_builtin(
                BuiltinOp::HasEdge,
                &[Literal::Int(0), Literal::Str("nonexistent".into())],
                Some(0),
            )
            .unwrap();
        assert_eq!(has_not, Literal::Bool(false));

        // Anchor of node 2
        let anchor = g
            .eval_graph_builtin(BuiltinOp::Anchor, &[Literal::Int(2)], Some(0))
            .unwrap();
        assert_eq!(anchor, Literal::Str("department".into()));
    }

    // -- FInstance tests --

    fn make_finstance_with_fk() -> FInstance {
        let mut alice = HashMap::new();
        alice.insert("name".to_string(), Value::Str("Alice".into()));
        alice.insert("age".to_string(), Value::Int(30));

        let mut bob = HashMap::new();
        bob.insert("name".to_string(), Value::Str("Bob".into()));
        bob.insert("age".to_string(), Value::Int(25));

        let mut post1 = HashMap::new();
        post1.insert("title".to_string(), Value::Str("Hello".into()));

        let fk = SchemaEdge {
            src: "posts".into(),
            tgt: "users".into(),
            kind: "author".into(),
            name: Some("author".into()),
        };

        FInstance::new()
            .with_table("users", vec![alice, bob])
            .with_table("posts", vec![post1])
            .with_foreign_key(fk, vec![(0, 0)]) // post 0 authored by user 0
    }

    #[test]
    fn finstance_fiber_returns_synthetic_ids() {
        let f = make_finstance_with_fk();
        let users = f.fiber(&Name::from("users"));
        assert_eq!(users.len(), 2);

        // Verify round-trip: decode each ID and check sort
        for &id in &users {
            assert_eq!(f.sort(id), Some(Name::from("users")));
        }

        let posts = f.fiber(&Name::from("posts"));
        assert_eq!(posts.len(), 1);
        for &id in &posts {
            assert_eq!(f.sort(id), Some(Name::from("posts")));
        }
    }

    #[test]
    fn finstance_pushforward_follows_fk() {
        let f = make_finstance_with_fk();
        let posts = f.fiber(&Name::from("posts"));
        let authors = f.pushforward(&posts, &Name::from("author"));
        assert_eq!(authors.len(), 1);
        // The author should be Alice (user row 0)
        let attrs = f.attributes(authors[0]);
        assert_eq!(attrs.get("name"), Some(&Value::Str("Alice".into())));
    }

    #[test]
    fn finstance_stalk_binds_columns() {
        let f = make_finstance_with_fk();
        let users = f.fiber(&Name::from("users"));
        // Find Alice's element (name == "Alice")
        let alice_id = users
            .iter()
            .find(|&&id| {
                let attrs = f.attributes(id);
                attrs.get("name") == Some(&Value::Str("Alice".into()))
            })
            .copied()
            .unwrap();

        let env = f.stalk(alice_id);
        let config = panproto_expr::EvalConfig::default();
        let name_expr = panproto_expr::Expr::Var("name".into());
        let result = panproto_expr::eval(&name_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Str("Alice".into()));

        let age_expr = panproto_expr::Expr::Var("age".into());
        let result = panproto_expr::eval(&age_expr, &env, &config);
        assert_eq!(result.unwrap(), Literal::Int(30));
    }

    #[test]
    fn finstance_round_trip_encoding() {
        let id = encode_finstance_id(3, 42);
        let (table_ord, row_idx) = decode_finstance_id(id);
        assert_eq!(table_ord, 3);
        assert_eq!(row_idx, 42);
    }

    #[test]
    fn finstance_graph_builtins() {
        let f = make_finstance_with_fk();
        let posts = f.fiber(&Name::from("posts"));
        let post_id = posts[0];

        // EdgeCount: post has 1 FK (author)
        let count = f
            .eval_graph_builtin(
                BuiltinOp::EdgeCount,
                &[Literal::Int(i64::from(post_id))],
                Some(post_id),
            )
            .unwrap();
        assert_eq!(count, Literal::Int(1));

        // HasEdge for "author"
        let has = f
            .eval_graph_builtin(
                BuiltinOp::HasEdge,
                &[
                    Literal::Int(i64::from(post_id)),
                    Literal::Str("author".into()),
                ],
                Some(post_id),
            )
            .unwrap();
        assert_eq!(has, Literal::Bool(true));

        // Anchor
        let anchor = f
            .eval_graph_builtin(
                BuiltinOp::Anchor,
                &[Literal::Int(i64::from(post_id))],
                Some(post_id),
            )
            .unwrap();
        assert_eq!(anchor, Literal::Str("posts".into()));
    }
}
