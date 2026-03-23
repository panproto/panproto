//! Content-addressed storage for expressions.
//!
//! Expressions are serialized via `MessagePack` and stored as blob objects
//! in the VCS, following the same pattern used for schemas and protocols.

use panproto_expr::Expr;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::Object;
use crate::store::Store;

/// Serialize an expression and store it as a content-addressed object.
///
/// Returns the `ObjectId` (blake3 hash) of the stored expression. If an
/// identical expression already exists in the store, this is a no-op that
/// returns the existing ID.
///
/// # Errors
///
/// Returns an error if serialization or storage fails.
pub fn store_expr(store: &mut dyn Store, expr: &Expr) -> Result<ObjectId, VcsError> {
    let object = Object::Expr(Box::new(expr.clone()));
    store.put(&object)
}

/// Load an expression from the store by its content-addressed ID.
///
/// # Errors
///
/// Returns [`VcsError::ObjectNotFound`] if no object exists with the
/// given ID, or [`VcsError::WrongObjectType`] if the object is not an
/// expression.
pub fn load_expr(store: &dyn Store, id: &ObjectId) -> Result<Expr, VcsError> {
    match store.get(id)? {
        Object::Expr(expr) => Ok(*expr),
        other => Err(VcsError::WrongObjectType {
            expected: "expr",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_expr::{BuiltinOp, Literal};

    use crate::MemStore;

    #[test]
    fn store_load_round_trip() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let expr = Expr::let_in(
            "x",
            Expr::Lit(Literal::Int(42)),
            Expr::builtin(
                BuiltinOp::Add,
                vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
            ),
        );

        let id = store_expr(&mut store, &expr)?;
        let loaded = load_expr(&store, &id)?;
        assert_eq!(loaded, expr);
        Ok(())
    }

    #[test]
    fn store_idempotent() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let expr = Expr::Lit(Literal::Str("hello".into()));

        let id1 = store_expr(&mut store, &expr)?;
        let id2 = store_expr(&mut store, &expr)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn load_wrong_type_returns_error() -> Result<(), VcsError> {
        use std::collections::HashMap;

        let mut store = MemStore::new();
        let schema = panproto_schema::Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };
        let id = store.put(&Object::Schema(Box::new(schema)))?;
        let result = load_expr(&store, &id);
        assert!(matches!(
            result,
            Err(VcsError::WrongObjectType {
                expected: "expr",
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let store = MemStore::new();
        let result = load_expr(&store, &ObjectId::ZERO);
        assert!(matches!(result, Err(VcsError::ObjectNotFound { .. })));
    }

    #[test]
    fn different_exprs_get_different_ids() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let e1 = Expr::Lit(Literal::Int(1));
        let e2 = Expr::Lit(Literal::Int(2));

        let id1 = store_expr(&mut store, &e1)?;
        let id2 = store_expr(&mut store, &e2)?;
        assert_ne!(id1, id2);
        Ok(())
    }

    #[test]
    fn complex_expr_round_trip() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let expr = Expr::lam(
            "record",
            Expr::builtin(
                BuiltinOp::Concat,
                vec![
                    Expr::field(Expr::var("record"), "first_name"),
                    Expr::Lit(Literal::Str(" ".into())),
                ],
            ),
        );

        let id = store_expr(&mut store, &expr)?;
        let loaded = load_expr(&store, &id)?;
        assert_eq!(loaded, expr);
        Ok(())
    }
}
