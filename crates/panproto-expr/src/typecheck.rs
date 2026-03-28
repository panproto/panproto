//! Lightweight type inference for expressions.
//!
//! Provides best-effort type inference over the expression language without
//! requiring a full dependent type system. Useful for validating coercion
//! expressions and catching obvious type mismatches early.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use crate::expr::{Expr, ExprType};
use crate::literal::Literal;

/// Errors from type inference.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TypeError {
    /// A variable was not found in the type environment.
    #[error("unbound variable: {0}")]
    UnboundVariable(String),

    /// An expression produced a type that does not match the expected type.
    #[error("type mismatch: expected {expected:?}, got {got:?}")]
    TypeMismatch {
        /// The type that was expected.
        expected: ExprType,
        /// The type that was inferred.
        got: ExprType,
    },

    /// The type of an expression could not be determined.
    #[error("cannot infer type of expression")]
    CannotInfer,
}

/// Infer the output type of an expression given input variable types.
///
/// This performs a single pass over the expression tree, using builtin
/// signatures where available and falling back to `ExprType::Any` for
/// polymorphic or opaque constructs.
///
/// # Errors
///
/// Returns [`TypeError::UnboundVariable`] if a variable is not in the
/// environment.
pub fn infer_type<S: BuildHasher>(
    expr: &Expr,
    env: &HashMap<Arc<str>, ExprType, S>,
) -> Result<ExprType, TypeError> {
    match expr {
        Expr::Var(name) => env
            .get(name.as_ref())
            .copied()
            .ok_or_else(|| TypeError::UnboundVariable(name.to_string())),
        Expr::Lit(lit) => Ok(literal_type(lit)),
        Expr::Builtin(op, _) => {
            if let Some((_, out)) = op.signature() {
                Ok(out)
            } else {
                Ok(ExprType::Any)
            }
        }
        Expr::Lam(..) | Expr::App(..) | Expr::Field(..) | Expr::Index(..) => Ok(ExprType::Any),
        Expr::Record(..) => Ok(ExprType::Record),
        Expr::List(..) => Ok(ExprType::List),
        Expr::Match { arms, .. } => {
            if let Some((_, body)) = arms.first() {
                infer_type(body, env)
            } else {
                Ok(ExprType::Any)
            }
        }
        Expr::Let { name, value, body } => {
            let val_type = infer_type(value, env)?;
            let mut inner_env: HashMap<Arc<str>, ExprType> =
                env.iter().map(|(k, v)| (Arc::clone(k), *v)).collect();
            inner_env.insert(Arc::clone(name), val_type);
            infer_type(body, &inner_env)
        }
    }
}

/// Map a literal value to its expression type.
const fn literal_type(lit: &Literal) -> ExprType {
    match lit {
        Literal::Int(_) => ExprType::Int,
        Literal::Float(_) => ExprType::Float,
        Literal::Str(_) => ExprType::Str,
        Literal::Bool(_) => ExprType::Bool,
        Literal::Null | Literal::Bytes(_) | Literal::Closure { .. } => ExprType::Any,
        Literal::List(_) => ExprType::List,
        Literal::Record(_) => ExprType::Record,
    }
}

/// Validate that a coercion expression produces the expected target type
/// when given a single input variable `"v"` of the specified source type.
///
/// # Errors
///
/// Returns [`TypeError::TypeMismatch`] if the inferred type does not match
/// `target` (and the inferred type is not `Any`), or [`TypeError::UnboundVariable`]
/// if the expression references variables other than `"v"`.
pub fn validate_coercion(expr: &Expr, source: ExprType, target: ExprType) -> Result<(), TypeError> {
    let env = HashMap::from([(Arc::from("v"), source)]);
    let inferred = infer_type(expr, &env)?;
    if inferred != ExprType::Any && inferred != target {
        return Err(TypeError::TypeMismatch {
            expected: target,
            got: inferred,
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::expr::{BuiltinOp, Expr};
    use crate::literal::Literal;

    #[test]
    fn infer_literal_types() {
        let env = HashMap::new();
        assert_eq!(
            infer_type(&Expr::Lit(Literal::Int(42)), &env).unwrap(),
            ExprType::Int
        );
        assert_eq!(
            infer_type(&Expr::Lit(Literal::Float(1.0)), &env).unwrap(),
            ExprType::Float
        );
        assert_eq!(
            infer_type(&Expr::Lit(Literal::Str("hi".into())), &env).unwrap(),
            ExprType::Str
        );
        assert_eq!(
            infer_type(&Expr::Lit(Literal::Bool(true)), &env).unwrap(),
            ExprType::Bool
        );
    }

    #[test]
    fn infer_var_from_env() {
        let env = HashMap::from([(Arc::from("x"), ExprType::Int)]);
        assert_eq!(infer_type(&Expr::var("x"), &env).unwrap(), ExprType::Int);
    }

    #[test]
    fn unbound_var_errors() {
        let env = HashMap::new();
        let result = infer_type(&Expr::var("missing"), &env);
        assert!(matches!(result, Err(TypeError::UnboundVariable(_))));
    }

    #[test]
    fn infer_builtin_with_signature() {
        let env = HashMap::new();
        let expr = Expr::int_to_float(Expr::Lit(Literal::Int(1)));
        assert_eq!(infer_type(&expr, &env).unwrap(), ExprType::Float);
    }

    #[test]
    fn infer_let_propagates_type() {
        let env = HashMap::new();
        let expr = Expr::let_in(
            "x",
            Expr::Lit(Literal::Int(42)),
            Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::var("x")]),
        );
        assert_eq!(infer_type(&expr, &env).unwrap(), ExprType::Float);
    }

    #[test]
    fn validate_coercion_ok() {
        let expr = Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::var("v")]);
        validate_coercion(&expr, ExprType::Int, ExprType::Float).unwrap();
    }

    #[test]
    fn validate_coercion_mismatch() {
        let expr = Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::var("v")]);
        let result = validate_coercion(&expr, ExprType::Int, ExprType::Str);
        assert!(matches!(result, Err(TypeError::TypeMismatch { .. })));
    }
}
