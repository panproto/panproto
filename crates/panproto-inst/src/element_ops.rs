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
use crate::ginstance::{EdgeId, GInstance};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::view_ref::RowId;
use crate::wtype::{
    WInstance, build_env_with_children, collect_scalar_child_values, value_to_expr_literal,
};

/// Operations on the category of elements `∫F` of an instance presheaf.
///
/// Elements are uniformly addressed by `u32` IDs. For [`WInstance`] and
/// [`GInstance`], these are node IDs. For [`FInstance`], they are row ids
/// (the number inside a [`RowId`]), which are unique across the whole
/// instance, so a table of any size is addressed without an encoding.
pub trait ElementOps {
    /// Fiber selection: all elements over schema vertex `v`.
    ///
    /// Returns `F(v) = {x | sort(x) = v}` as a vector of element IDs in
    /// ascending order, so that anything taken from the front of it (a
    /// query's `limit`, say) is the same in every process.
    fn fiber(&self, vertex: &Name) -> Vec<u32>;

    /// Relational pushforward along a schema edge.
    ///
    /// Given `S ⊆ F(v)` and edge kind `e: v → w`, computes the direct
    /// image `F(e)(S) = {y ∈ F(w) | ∃x ∈ S, (x,y) ∈ F(e)}`.
    ///
    /// Tree- and graph-shaped instances count paths: a source listed `k`
    /// times in `elements` contributes each of its targets `k` times, so a
    /// target reached along two paths appears twice and each copy is followed
    /// again by a further step. Tree-shaped instances yield targets element by
    /// element, each element's in arc order. Graph-shaped instances yield
    /// them in ascending [`EdgeId`] order, one per matching edge and
    /// occurrence of its source, so a target reached by two parallel edges
    /// appears twice. Table-shaped instances deduplicate targets and yield
    /// them by foreign key in ascending schema-edge order, each key's pairs
    /// in stored order.
    ///
    /// Because paths are counted, composing pushforwards over a graph yields
    /// one entry per walk, and the number of walks can grow exponentially in
    /// the number of steps: `2k` steps through a chain of `k` diamonds list
    /// its last node `2^k` times. A tree has one path to each node, so its
    /// composed pushforwards never exceed its node count. A query's path
    /// navigation ([`execute_elements`](crate::execute_elements)) keeps each
    /// target once per step, so its cost stays linear in the path's length.
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
    /// Interprets `Edge`, `Edges`, `Children`, `HasEdge`, `EdgeCount`,
    /// `Anchor` in the internal logic of the category of elements.
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
        let mut ids: Vec<u32> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.anchor == *vertex)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
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
        let mut ids: Vec<u32> = self
            .nodes()
            .iter()
            .filter(|(_, n)| n.anchor == *vertex)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32> {
        // How often each source is listed: every listing is a path that
        // continues along each matching out-edge.
        let mut sources: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        for &node in elements {
            *sources.entry(node).or_default() += 1;
        }
        let mut ids: Vec<EdgeId> = sources
            .keys()
            .flat_map(|&node| self.out_edges(node).iter().copied())
            .filter(|id| self.edge(*id).is_some_and(|e| e.edge.kind == *edge_kind))
            .collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| self.edge(id))
            .flat_map(|e| std::iter::repeat_n(e.tgt, sources.get(&e.src).copied().unwrap_or(0)))
            .collect()
    }

    fn stalk(&self, element_id: u32) -> panproto_expr::Env {
        let Some(node) = self.node(element_id) else {
            return panproto_expr::Env::new();
        };

        // Scalar values of out-edge targets: the dependent-sum projection
        // for graph-shaped instances, as child scalars are for trees.
        let child_scalars = collect_graph_scalar_children(self, element_id);
        let mut env = build_env_with_children(&node.extra_fields, &child_scalars);

        // Metadata
        env = env.extend(
            Arc::from("_anchor"),
            Literal::Str(node.anchor.as_ref().into()),
        );
        env = env.extend(Arc::from("_id"), Literal::Int(i64::from(node.id)));

        if let Some(FieldPresence::Present(val)) = &node.value {
            env = env.extend(Arc::from("_value"), value_to_expr_literal(val));
        }

        env.extend(
            Arc::from("_edge_count"),
            Literal::Int(count_literal(self.out_edges(element_id).len())),
        )
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
        let Some(node) = self.node(element_id) else {
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
        self.node(element_id).map(|n| n.anchor.clone())
    }

    fn element_value(&self, element_id: u32) -> Option<FieldPresence> {
        self.node(element_id).and_then(|n| n.value.clone())
    }
}

/// A count as an expression integer, saturating at `i64::MAX`.
fn count_literal(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// Collect scalar values from a graph node's outgoing edge targets.
///
/// Analogous to [`collect_scalar_child_values`] for W-type instances:
/// this is the dependent-sum projection `Σ_{e: v→w} π_scalar(Fiber(w))`
/// for graph-shaped instances. Out-edges are read in
/// [`GInstance::ordered_out_edges`] order; each whose target holds a
/// present value binds it under `edge.name` (or `edge.tgt`), a later edge
/// overriding an earlier one of the same name, matching the W-type
/// convention.
fn collect_graph_scalar_children(instance: &GInstance, node_id: u32) -> HashMap<String, Value> {
    let mut result = HashMap::new();
    for id in instance.ordered_out_edges(node_id) {
        let Some(edge) = instance.edge(id) else {
            continue;
        };
        if let Some(FieldPresence::Present(val)) =
            instance.node(edge.tgt).and_then(|n| n.value.as_ref())
        {
            let field_name = edge.edge.name.as_deref().unwrap_or(&*edge.edge.tgt);
            result.insert(field_name.to_string(), val.clone());
        }
    }
    result
}

// ---------------------------------------------------------------------------
// FInstance implementation
// ---------------------------------------------------------------------------

impl FInstance {
    /// Resolve an element id to its row's table and id.
    fn resolve_element(&self, element_id: u32) -> Option<(&Name, RowId)> {
        let id = RowId(element_id);
        self.table_of(id).map(|table| (table, id))
    }

    /// The number of foreign-key pairs whose source is this row.
    fn out_pair_count(&self, row: RowId) -> usize {
        self.foreign_keys()
            .into_iter()
            .flat_map(|(_, pairs)| pairs.iter())
            .filter(|(src, _)| *src == row)
            .count()
    }
}

impl ElementOps for FInstance {
    fn fiber(&self, vertex: &Name) -> Vec<u32> {
        self.table(vertex.as_ref())
            .map_or_else(Vec::new, |t| t.ids().into_iter().map(|id| id.0).collect())
    }

    fn pushforward(&self, elements: &[u32], edge_kind: &Name) -> Vec<u32> {
        let element_set: rustc_hash::FxHashSet<u32> = elements.iter().copied().collect();
        let mut result = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for (edge, pairs) in self.foreign_keys() {
            if edge.kind != *edge_kind {
                continue;
            }
            for &(src, tgt) in pairs {
                if element_set.contains(&src.0) && seen.insert(tgt.0) {
                    result.push(tgt.0);
                }
            }
        }
        result
    }

    fn stalk(&self, element_id: u32) -> panproto_expr::Env {
        let Some((table_name, row_id)) = self.resolve_element(element_id) else {
            return panproto_expr::Env::new();
        };
        let Some(row) = self.row(row_id) else {
            return panproto_expr::Env::new();
        };

        // FInstance has no structural children; the stalk is just
        // the column values (the fiber F(v) is a flat set of records).
        let mut env = crate::wtype::build_env_from_extra_fields(row);

        // Metadata
        env = env.extend(
            Arc::from("_anchor"),
            Literal::Str(table_name.as_ref().into()),
        );
        env = env.extend(Arc::from("_id"), Literal::Int(i64::from(element_id)));

        // FK out-degree
        let fk_count = i64::try_from(self.out_pair_count(row_id)).unwrap_or(i64::MAX);
        env = env.extend(Arc::from("_edge_count"), Literal::Int(fk_count));

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
        self.row(RowId(element_id)).cloned().unwrap_or_default()
    }

    fn sort(&self, element_id: u32) -> Option<Name> {
        self.table_of(RowId(element_id)).cloned()
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
    let mut extra: Vec<(&String, &Value)> = node.extra_fields.iter().collect();
    extra.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (key, val) in extra {
        fields.push((Arc::from(key.as_str()), value_to_expr_literal(val)));
    }
    Literal::Record(fields)
}

/// Read the edge-kind argument of `edge`, `edges` and `has_edge`.
fn edge_kind_arg(lit: &Literal) -> Result<Name, panproto_expr::ExprError> {
    lit.as_str()
        .map(Name::from)
        .ok_or_else(|| panproto_expr::ExprError::TypeError {
            expected: "string".into(),
            got: lit.type_name().into(),
        })
}

/// Graph builtins for [`WInstance`].
fn winstance_graph_builtin(
    instance: &WInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    crate::instance_env::check_graph_builtin_arity(op, args)?;
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
        BuiltinOp::Edges => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_name = edge_kind_arg(&args[1])?;
            Ok(Literal::List(
                instance
                    .arcs
                    .iter()
                    .filter(|(src, _, edge)| *src == node_id && edge.kind == edge_name)
                    .filter_map(|(_, tgt, _)| instance.nodes.get(tgt).map(node_to_literal))
                    .collect(),
            ))
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
///
/// Out-edges are read in ascending [`EdgeId`] order: `edge` follows the
/// least matching edge, `edges` and `children` list targets one per edge.
fn ginstance_graph_builtin(
    instance: &GInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    crate::instance_env::check_graph_builtin_arity(op, args)?;
    let target = |id: &EdgeId| instance.edge(*id).and_then(|e| instance.node(e.tgt));
    let has_kind =
        |id: &EdgeId, kind: &Name| instance.edge(*id).is_some_and(|e| e.edge.kind == *kind);
    match op {
        BuiltinOp::Edge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_name = edge_kind_arg(&args[1])?;
            Ok(instance
                .out_edges(node_id)
                .iter()
                .find(|id| has_kind(id, &edge_name))
                .and_then(target)
                .map_or(Literal::Null, node_to_literal))
        }
        BuiltinOp::Edges => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_name = edge_kind_arg(&args[1])?;
            Ok(Literal::List(
                instance
                    .out_edges(node_id)
                    .iter()
                    .filter(|id| has_kind(id, &edge_name))
                    .filter_map(target)
                    .map(node_to_literal)
                    .collect(),
            ))
        }
        BuiltinOp::Children => {
            let node_id = resolve_node_ref(&args[0], context)?;
            Ok(Literal::List(
                instance
                    .out_edges(node_id)
                    .iter()
                    .filter_map(target)
                    .map(node_to_literal)
                    .collect(),
            ))
        }
        BuiltinOp::HasEdge => {
            let node_id = resolve_node_ref(&args[0], context)?;
            let edge_name = edge_kind_arg(&args[1])?;
            Ok(Literal::Bool(
                instance
                    .out_edges(node_id)
                    .iter()
                    .any(|id| has_kind(id, &edge_name)),
            ))
        }
        BuiltinOp::EdgeCount => {
            let node_id = resolve_node_ref(&args[0], context)?;
            Ok(Literal::Int(count_literal(
                instance.out_edges(node_id).len(),
            )))
        }
        BuiltinOp::Anchor => {
            let node_id = resolve_node_ref(&args[0], context)?;
            Ok(instance.node(node_id).map_or(Literal::Null, |node| {
                Literal::Str(node.anchor.as_ref().into())
            }))
        }
        _ => Ok(Literal::Null),
    }
}

/// Convert an [`FInstance`] row to a [`Literal::Record`].
///
/// Fields are listed in ascending column name.
fn finstance_row_to_literal(row: &HashMap<String, Value>) -> Literal {
    let mut columns: Vec<(&String, &Value)> = row.iter().collect();
    columns.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let fields: Vec<(Arc<str>, Literal)> = columns
        .into_iter()
        .map(|(k, v)| (Arc::from(k.as_str()), value_to_expr_literal(v)))
        .collect();
    Literal::Record(fields)
}

/// The rows a table-instance element reaches along foreign keys of `kind`
/// (every kind when `None`): keys in ascending schema-edge order, pairs in
/// ascending order.
fn finstance_targets<'a>(
    instance: &'a FInstance,
    src_row: RowId,
    kind: Option<&Name>,
) -> Vec<&'a HashMap<String, Value>> {
    instance
        .foreign_keys()
        .into_iter()
        .filter(|(edge, _)| kind.is_none_or(|k| edge.kind == *k))
        .flat_map(|(_, pairs)| {
            pairs
                .iter()
                .filter(move |(s, _)| *s == src_row)
                .filter_map(move |&(_, t)| instance.row(t))
        })
        .collect()
}

/// Graph builtins for [`FInstance`]: maps graph traversal to FK semantics.
fn finstance_graph_builtin(
    instance: &FInstance,
    op: BuiltinOp,
    args: &[Literal],
    context: Option<u32>,
) -> Result<Literal, panproto_expr::ExprError> {
    crate::instance_env::check_graph_builtin_arity(op, args)?;
    match op {
        BuiltinOp::Edge | BuiltinOp::Edges => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let edge_name = edge_kind_arg(&args[1])?;
            let rows = instance
                .resolve_element(element_id)
                .map(|(_, src_row)| finstance_targets(instance, src_row, Some(&edge_name)))
                .unwrap_or_default();
            Ok(if op == BuiltinOp::Edge {
                rows.first()
                    .map_or(Literal::Null, |row| finstance_row_to_literal(row))
            } else {
                Literal::List(rows.into_iter().map(finstance_row_to_literal).collect())
            })
        }
        BuiltinOp::Children => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let Some((_, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::List(Vec::new()));
            };
            Ok(Literal::List(
                finstance_targets(instance, src_row, None)
                    .into_iter()
                    .map(finstance_row_to_literal)
                    .collect(),
            ))
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

            let Some((_, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::Bool(false));
            };

            let found = instance.foreign_keys().into_iter().any(|(edge, pairs)| {
                edge.kind == edge_name && pairs.iter().any(|(s, _)| *s == src_row)
            });
            Ok(Literal::Bool(found))
        }
        BuiltinOp::EdgeCount => {
            let element_id = resolve_node_ref(&args[0], context)?;
            let Some((_, src_row)) = instance.resolve_element(element_id) else {
                return Ok(Literal::Int(0));
            };
            Ok(Literal::Int(
                i64::try_from(instance.out_pair_count(src_row)).unwrap_or(i64::MAX),
            ))
        }
        BuiltinOp::Anchor => {
            let element_id = resolve_node_ref(&args[0], context)?;
            Ok(instance
                .resolve_element(element_id)
                .map_or(Literal::Null, |(table_name, _)| {
                    Literal::Str(table_name.as_ref().into())
                }))
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
    fn ginstance_pushforward_yields_one_target_per_edge() {
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

        // Both source nodes reach the same target, once per edge.
        let result = g.pushforward(&[1, 0], &Name::from("link"));
        assert_eq!(result, vec![2, 2]);
    }

    #[test]
    fn ginstance_edges_builtin_lists_parallel_edges() {
        let link = SchemaEdge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "link".into(),
            name: None,
        };
        let g = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_node(Node::new(2, "b"))
            .with_edge(0, 2, link.clone())
            .with_edge(0, 1, link.clone())
            .with_edge(0, 2, link);
        let ids = |lit: Literal| match lit {
            Literal::List(items) => items
                .into_iter()
                .map(|item| match item {
                    Literal::Record(fields) => fields[0].1.clone(),
                    other => other,
                })
                .collect::<Vec<_>>(),
            other => vec![other],
        };
        let all = g
            .eval_graph_builtin(
                BuiltinOp::Edges,
                &[Literal::Int(0), Literal::Str("link".into())],
                Some(0),
            )
            .unwrap();
        assert_eq!(
            ids(all),
            vec![Literal::Int(2), Literal::Int(1), Literal::Int(2)]
        );
        let none = g
            .eval_graph_builtin(
                BuiltinOp::Edges,
                &[Literal::Int(0), Literal::Str("other".into())],
                Some(0),
            )
            .unwrap();
        assert_eq!(none, Literal::List(Vec::new()));
    }

    #[test]
    fn winstance_and_finstance_edges_builtin_list_every_match() {
        let w = make_winstance_with_children();
        let all = w
            .eval_graph_builtin(
                BuiltinOp::Edges,
                &[Literal::Int(0), Literal::Str("prop".into())],
                Some(0),
            )
            .unwrap();
        assert!(matches!(all, Literal::List(ref items) if items.len() == 2));

        let f = make_finstance_with_fk();
        let post = f.fiber(&Name::from("posts"))[0];
        let authors = f
            .eval_graph_builtin(
                BuiltinOp::Edges,
                &[Literal::Int(i64::from(post)), Literal::Str("author".into())],
                Some(post),
            )
            .unwrap();
        let Literal::List(authors) = authors else {
            panic!("edges yields a list");
        };
        assert_eq!(authors.len(), 1);
        assert_eq!(
            authors[0],
            Literal::Record(vec![
                ("age".into(), Literal::Int(30)),
                ("name".into(), Literal::Str("Alice".into())),
            ]),
            "row fields are listed in ascending column name"
        );
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

        // Users are rows 0 and 1, the post row 2, authored by user 0.
        FInstance::new()
            .with_table("users", vec![alice, bob])
            .and_then(|f| f.with_table("posts", vec![post1]))
            .and_then(|f| f.with_foreign_key(fk, [(RowId(2), RowId(0))]))
            .unwrap()
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
    fn table_element_ids_are_row_ids() {
        let f = FInstance::new()
            .with_rows(
                "users",
                [(RowId(70_000), HashMap::new()), (RowId(3), HashMap::new())],
            )
            .and_then(|f| f.with_rows("posts", [(RowId(9), HashMap::new())]))
            .unwrap();
        assert_eq!(f.fiber(&Name::from("users")), vec![3, 70_000]);
        assert_eq!(f.fiber(&Name::from("posts")), vec![9]);
        assert_eq!(f.sort(70_000), Some(Name::from("users")));
        assert_eq!(f.sort(4), None);
        let env = f.stalk(70_000);
        assert_eq!(
            panproto_expr::eval(
                &panproto_expr::Expr::Var("_id".into()),
                &env,
                &panproto_expr::EvalConfig::default()
            )
            .unwrap(),
            Literal::Int(70_000)
        );
    }

    #[test]
    fn a_table_with_more_than_65536_rows_queries_correctly() {
        const ROWS: u32 = 65_600;
        let fk = SchemaEdge {
            src: "item".into(),
            tgt: "item".into(),
            kind: "next".into(),
            name: None,
        };
        let mut f = FInstance::new();
        for i in 0..ROWS {
            f.insert_row(
                "item",
                RowId(i),
                HashMap::from([("n".to_owned(), Value::Int(i64::from(i)))]),
            )
            .unwrap();
        }
        f.insert_foreign_key(fk, RowId(65_590), RowId(65_599))
            .unwrap();
        let items = f.fiber(&Name::from("item"));
        assert_eq!(items.len(), usize::try_from(ROWS).unwrap());
        assert_eq!(items.last(), Some(&(ROWS - 1)));
        // Rows past the 16-bit boundary keep their own cells and edges.
        assert_eq!(f.attributes(65_599).get("n"), Some(&Value::Int(65_599)));
        assert_eq!(f.pushforward(&[65_590], &Name::from("next")), vec![65_599]);
        assert_eq!(
            f.pushforward(&[65_590 - 65_536], &Name::from("next")),
            Vec::<u32>::new()
        );

        let query = crate::query::InstanceQuery {
            anchor: Name::from("item"),
            predicate: Some(panproto_expr::Expr::Builtin(
                BuiltinOp::Gte,
                vec![
                    panproto_expr::Expr::Var("n".into()),
                    panproto_expr::Expr::Lit(Literal::Int(65_598)),
                ],
            )),
            group_by: None,
            project: None,
            limit: None,
            path: Vec::new(),
        };
        let schema = panproto_schema::Schema::default();
        let found: Vec<u32> = crate::query::execute_functor(&query, &f, &schema)
            .into_iter()
            .map(|m| m.node_id)
            .collect();
        assert_eq!(found, vec![65_598, 65_599]);
    }

    #[test]
    fn the_table_query_square_against_from_finstance_is_identical() {
        let f = make_finstance_with_fk();
        let (g, _) = GInstance::from_finstance(&f).unwrap();
        let schema = panproto_schema::Schema::default();
        let queries = [
            crate::query::InstanceQuery {
                anchor: Name::from("users"),
                predicate: Some(panproto_expr::Expr::Builtin(
                    BuiltinOp::Gt,
                    vec![
                        panproto_expr::Expr::Var("age".into()),
                        panproto_expr::Expr::Lit(Literal::Int(20)),
                    ],
                )),
                group_by: None,
                project: Some(vec!["name".into()]),
                limit: None,
                path: Vec::new(),
            },
            crate::query::InstanceQuery {
                anchor: Name::from("posts"),
                predicate: None,
                group_by: None,
                project: None,
                limit: Some(1),
                path: vec![Name::from("author")],
            },
        ];
        for query in &queries {
            let on_table = crate::query::execute_functor(query, &f, &schema);
            let on_graph = crate::query::execute_graph(query, &g, &schema);
            let key = |m: &crate::query::QueryMatch| {
                let mut fields: Vec<(String, Value)> = m
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                fields.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                (m.node_id, m.anchor.clone(), m.value.clone(), fields)
            };
            let on_table: Vec<_> = on_table.iter().map(key).collect();
            let on_graph: Vec<_> = on_graph.iter().map(key).collect();
            assert!(!on_table.is_empty());
            assert_eq!(on_table, on_graph, "query {query:?}");
        }
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
