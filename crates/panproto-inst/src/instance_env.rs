//! Instance-aware expression evaluation.
//!
//! The graph traversal builtins (`edge`, `children`, `has_edge`, `edge_count`,
//! `anchor`) read an instance, which `panproto_expr::eval` does not hold. This
//! module supplies it as a [`panproto_expr::BuiltinResolver`], so those
//! builtins answer wherever they appear in an expression rather than only at
//! its root:
//!
//! - [`eval_with_instance`]: evaluates against a [`WInstance`] directly
//! - [`eval_with_element_ops`]: evaluates against any [`ElementOps`]
//!   implementor (polymorphic over all instance shapes)

use std::sync::Arc;

use panproto_expr::{BuiltinOp, BuiltinResolver, EvalConfig, Expr, ExprError, Literal};
use panproto_gat::Name;

use crate::element_ops::ElementOps;
use crate::value::Value;
use crate::wtype::WInstance;

/// Check if a builtin is a graph traversal operation.
const fn is_graph_builtin(op: BuiltinOp) -> bool {
    matches!(
        op,
        BuiltinOp::Edge
            | BuiltinOp::Children
            | BuiltinOp::HasEdge
            | BuiltinOp::EdgeCount
            | BuiltinOp::Anchor
    )
}

/// Reject a graph builtin applied to the wrong number of arguments, before any
/// argument is read.
///
/// # Errors
///
/// Returns [`ExprError::ArityMismatch`] when `args` is not the length
/// `op.arity()` declares.
pub fn check_graph_builtin_arity(op: BuiltinOp, args: &[Literal]) -> Result<(), ExprError> {
    let expected = op.arity();
    if args.len() == expected {
        return Ok(());
    }
    Err(ExprError::ArityMismatch {
        op: format!("{op:?}"),
        expected,
        got: args.len(),
    })
}

/// Answers the graph builtins against a [`WInstance`].
struct InstanceResolver<'a> {
    instance: &'a WInstance,
    context_node_id: Option<u32>,
}

impl BuiltinResolver for InstanceResolver<'_> {
    fn handles(&self, op: BuiltinOp) -> bool {
        is_graph_builtin(op)
    }

    fn apply(&self, op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
        apply_graph_builtin(op, args, self.instance, self.context_node_id)
    }
}

/// Answers the graph builtins against any [`ElementOps`] instance shape.
struct ElementOpsResolver<'a, T: ElementOps> {
    instance: &'a T,
    context: Option<u32>,
}

impl<T: ElementOps> BuiltinResolver for ElementOpsResolver<'_, T> {
    fn handles(&self, op: BuiltinOp) -> bool {
        is_graph_builtin(op)
    }

    fn apply(&self, op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
        check_graph_builtin_arity(op, args)?;
        self.instance.eval_graph_builtin(op, args, self.context)
    }
}

/// Evaluate an expression with access to an instance graph.
///
/// Graph traversal builtins (`edge`, `children`, `has_edge`, `edge_count`,
/// `anchor`) are resolved against the provided instance at every depth; all
/// other expressions evaluate exactly as `panproto_expr::eval` evaluates them.
///
/// The `context_node_id` determines which node is the "current" node
/// for graph traversal. Pass `None` to disable graph traversal.
///
/// # Errors
///
/// Returns `panproto_expr::ExprError` on evaluation failure.
pub fn eval_with_instance(
    expr: &Expr,
    env: &panproto_expr::Env,
    config: &EvalConfig,
    instance: &WInstance,
    context_node_id: Option<u32>,
) -> Result<Literal, ExprError> {
    let resolver = InstanceResolver {
        instance,
        context_node_id,
    };
    panproto_expr::eval_with_resolver(expr, env, config, &resolver)
}

/// Evaluate an expression with graph builtins resolved via [`ElementOps`].
///
/// This is the polymorphic version of [`eval_with_instance`]: it works
/// with any instance shape that implements [`ElementOps`]. Graph traversal
/// builtins are delegated to `T::eval_graph_builtin` at every depth; all other
/// expressions evaluate exactly as `panproto_expr::eval` evaluates them.
///
/// # Errors
///
/// Returns `panproto_expr::ExprError` on evaluation failure.
pub fn eval_with_element_ops<T: ElementOps>(
    expr: &Expr,
    env: &panproto_expr::Env,
    config: &EvalConfig,
    instance: &T,
    context: Option<u32>,
) -> Result<Literal, ExprError> {
    let resolver = ElementOpsResolver { instance, context };
    panproto_expr::eval_with_resolver(expr, env, config, &resolver)
}

/// Evaluate a graph traversal builtin against an instance.
fn apply_graph_builtin(
    op: BuiltinOp,
    args: &[Literal],
    instance: &WInstance,
    context_node_id: Option<u32>,
) -> Result<Literal, ExprError> {
    check_graph_builtin_arity(op, args)?;
    match op {
        BuiltinOp::Edge => {
            // edge(node_ref, edge_kind) yields the first matching child value
            let node_id = resolve_node_ref(&args[0], context_node_id)?;
            let edge_kind = args[1].as_str().ok_or_else(|| ExprError::TypeError {
                expected: "string".into(),
                got: args[1].type_name().into(),
            })?;
            let edge_name = Name::from(edge_kind);
            for &(src, tgt, ref edge) in &instance.arcs {
                if src == node_id && edge.kind == edge_name {
                    return Ok(node_to_literal(instance, tgt));
                }
            }
            Ok(Literal::Null)
        }
        BuiltinOp::Children => {
            // children(node_ref) yields every child value
            let node_id = resolve_node_ref(&args[0], context_node_id)?;
            let mut children = Vec::new();
            for &(src, tgt, _) in &instance.arcs {
                if src == node_id {
                    children.push(node_to_literal(instance, tgt));
                }
            }
            Ok(Literal::List(children))
        }
        BuiltinOp::HasEdge => {
            // has_edge(node_ref, edge_kind) yields whether such an arc leaves the node
            let node_id = resolve_node_ref(&args[0], context_node_id)?;
            let edge_kind = args[1].as_str().ok_or_else(|| ExprError::TypeError {
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
            // edge_count(node_ref) yields how many arcs leave the node
            let node_id = resolve_node_ref(&args[0], context_node_id)?;
            let count = instance
                .arcs
                .iter()
                .filter(|(src, _, _)| *src == node_id)
                .count();
            #[allow(clippy::cast_possible_wrap)]
            Ok(Literal::Int(count as i64))
        }
        BuiltinOp::Anchor => {
            // anchor(node_ref) yields the schema vertex the node sits over
            let node_id = resolve_node_ref(&args[0], context_node_id)?;
            instance
                .nodes
                .get(&node_id)
                .map_or(Ok(Literal::Null), |node| {
                    Ok(Literal::Str(node.anchor.as_ref().into()))
                })
        }
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Resolve a node reference from a literal value.
///
/// Accepts either an integer (direct node ID) or the string `"self"`
/// (resolved to `context_node_id`).
fn resolve_node_ref(lit: &Literal, context_node_id: Option<u32>) -> Result<u32, ExprError> {
    match lit {
        Literal::Int(id) => u32::try_from(*id).map_err(|_| ExprError::TypeError {
            expected: "non-negative int fitting u32".into(),
            got: format!("{id}"),
        }),
        Literal::Str(s) if s == "self" => context_node_id
            .ok_or_else(|| ExprError::UnboundVariable("self (no context node)".into())),
        _ => Err(ExprError::TypeError {
            expected: "int or \"self\"".into(),
            got: lit.type_name().into(),
        }),
    }
}

/// Convert a node's data to a Literal for expression evaluation.
///
/// Produces a Record with the node's `extra_fields`, anchor, and id.
fn node_to_literal(instance: &WInstance, node_id: u32) -> Literal {
    let Some(node) = instance.nodes.get(&node_id) else {
        return Literal::Null;
    };
    let mut fields: Vec<(Arc<str>, Literal)> = Vec::new();
    fields.push((Arc::from("_id"), Literal::Int(i64::from(node.id))));
    fields.push((
        Arc::from("_anchor"),
        Literal::Str(node.anchor.as_ref().into()),
    ));
    for (key, val) in &node.extra_fields {
        fields.push((Arc::from(key.as_str()), value_to_literal(val)));
    }
    Literal::Record(fields)
}

/// Convert an instance Value to a Literal.
fn value_to_literal(val: &Value) -> Literal {
    crate::wtype::value_to_expr_literal(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::metadata::Node;
    use crate::value::Value;
    use panproto_schema::Edge as SchemaEdge;

    fn make_instance() -> WInstance {
        let mut nodes = HashMap::new();
        let mut root = Node::new(0, "document");
        root.extra_fields
            .insert("title".into(), Value::Str("Test".into()));
        nodes.insert(0, root);

        let mut child = Node::new(1, "paragraph");
        child
            .extra_fields
            .insert("text".into(), Value::Str("Hello".into()));
        nodes.insert(1, child);

        let edge = SchemaEdge {
            src: Name::from("document"),
            tgt: Name::from("paragraph"),
            kind: Name::from("body"),
            name: None,
        };

        WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, Name::from("document"))
    }

    /// Helper to evaluate and assert success.
    fn eval_ok(expr: &Expr, inst: &WInstance, ctx: Option<u32>) -> Literal {
        let env = panproto_expr::Env::new();
        let config = EvalConfig::default();
        let result = eval_with_instance(expr, &env, &config, inst, ctx);
        assert!(result.is_ok(), "eval failed: {result:?}");
        result.unwrap_or(Literal::Null)
    }

    #[test]
    fn edge_follows_arc() {
        let inst = make_instance();
        let expr = Expr::Builtin(
            BuiltinOp::Edge,
            vec![
                Expr::Lit(Literal::Int(0)),
                Expr::Lit(Literal::Str("body".into())),
            ],
        );
        let result = eval_ok(&expr, &inst, Some(0));
        assert!(matches!(result, Literal::Record(_)));
    }

    #[test]
    fn children_returns_list() {
        let inst = make_instance();
        let expr = Expr::Builtin(BuiltinOp::Children, vec![Expr::Lit(Literal::Int(0))]);
        let result = eval_ok(&expr, &inst, Some(0));
        assert!(matches!(result, Literal::List(ref items) if items.len() == 1));
    }

    #[test]
    fn has_edge_true() {
        let inst = make_instance();
        let expr = Expr::Builtin(
            BuiltinOp::HasEdge,
            vec![
                Expr::Lit(Literal::Int(0)),
                Expr::Lit(Literal::Str("body".into())),
            ],
        );
        assert_eq!(eval_ok(&expr, &inst, Some(0)), Literal::Bool(true));
    }

    #[test]
    fn has_edge_false() {
        let inst = make_instance();
        let expr = Expr::Builtin(
            BuiltinOp::HasEdge,
            vec![
                Expr::Lit(Literal::Int(0)),
                Expr::Lit(Literal::Str("nonexistent".into())),
            ],
        );
        assert_eq!(eval_ok(&expr, &inst, Some(0)), Literal::Bool(false));
    }

    #[test]
    fn edge_count_works() {
        let inst = make_instance();
        let expr = Expr::Builtin(BuiltinOp::EdgeCount, vec![Expr::Lit(Literal::Int(0))]);
        assert_eq!(eval_ok(&expr, &inst, Some(0)), Literal::Int(1));
    }

    #[test]
    fn anchor_returns_kind() {
        let inst = make_instance();
        let expr = Expr::Builtin(BuiltinOp::Anchor, vec![Expr::Lit(Literal::Int(1))]);
        assert_eq!(
            eval_ok(&expr, &inst, Some(0)),
            Literal::Str("paragraph".into())
        );
    }
}
