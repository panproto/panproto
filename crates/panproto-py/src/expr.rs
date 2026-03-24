//! Python bindings for panproto expression language.
//!
//! Wraps `panproto-expr` (pure functional expression AST and evaluator)
//! and `panproto-expr-parser` (Haskell-style surface syntax).

use pyo3::prelude::*;

use panproto_expr::{self, Env, EvalConfig, Expr};

use crate::convert;

/// An expression in panproto's pure functional language.
///
/// The expression language is a lambda calculus with ~50 built-in
/// operations, pattern matching, records, and lists. Evaluation is
/// deterministic and bounded by configurable step/depth limits.
#[pyclass(name = "Expr", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyExpr {
    pub(crate) inner: Expr,
}

#[pymethods]
impl PyExpr {
    /// Evaluate the expression with an empty environment.
    ///
    /// Returns
    /// -------
    /// dict
    ///     The evaluation result as a ``Literal`` (serialized to dict).
    fn eval(&self, py: Python<'_>) -> PyResult<PyObject> {
        let env = Env::new();
        let config = EvalConfig::default();
        let value = panproto_expr::eval(&self.inner, &env, &config)
            .map_err(|e| crate::error::ExprError::new_err(format!("eval failed: {e}")))?;
        convert::to_python(py, &value)
    }

    /// Serialize the expression AST to a Python dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    /// Pretty-print the expression in Haskell-style surface syntax.
    fn pretty(&self) -> String {
        panproto_expr_parser::pretty_print(&self.inner)
    }

    fn __repr__(&self) -> String {
        let pp = panproto_expr_parser::pretty_print(&self.inner);
        if pp.len() > 80 {
            format!("Expr({}...)", &pp[..77])
        } else {
            format!("Expr({pp})")
        }
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Parse an expression from Haskell-style surface syntax.
///
/// Parameters
/// ----------
/// source : str
///     Expression source code. Supports lambda expressions, let/where
///     bindings, case/of, list comprehensions, do-notation, operator
///     sections, record syntax, and ``->`` graph traversal.
///
/// Returns
/// -------
/// Expr
///     The parsed expression AST.
///
/// Raises
/// ------
/// `ExprError`
///     If tokenization or parsing fails.
///
/// Examples
/// --------
/// >>> expr = parse_expr(r"\\x -> x + 1")
/// >>> expr.pretty()
/// '\\x -> x + 1'
#[pyfunction]
pub fn parse_expr(source: &str) -> PyResult<PyExpr> {
    let tokens = panproto_expr_parser::tokenize(source)
        .map_err(|e| crate::error::ExprError::new_err(format!("tokenize failed: {e:?}")))?;
    let expr = panproto_expr_parser::parse(&tokens)
        .map_err(|e| crate::error::ExprError::new_err(format!("parse failed: {e:?}")))?;
    Ok(PyExpr { inner: expr })
}

/// Pretty-print an expression to Haskell-style surface syntax.
#[pyfunction]
pub fn pretty_print_expr(expr: &PyExpr) -> String {
    panproto_expr_parser::pretty_print(&expr.inner)
}

/// Register expression types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyExpr>()?;
    parent.add_function(wrap_pyfunction!(parse_expr, parent)?)?;
    parent.add_function(wrap_pyfunction!(pretty_print_expr, parent)?)?;
    Ok(())
}
