use std::sync::Arc;

use crate::sort::SortExpr;

/// An operation (term constructor) in a GAT.
///
/// Operations are the functions / constructors of a GAT. Each operation
/// has typed inputs and a typed output, where types are sort expressions.
/// The input parameter names are in scope in the sort expressions of
/// later inputs and in the output sort, enabling dependent signatures.
///
/// # Examples
///
/// - `src: (e: Edge) → Vertex` (graph source map)
/// - `add: (a: Int, b: Int) → Int` (integer addition)
/// - `id: (x: Ob) → Hom(x, x)` (identity morphism with a dependent output)
///
/// Based on the formal definition of GAT operations from Cartmell (1986).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Operation {
    /// The operation name (e.g., "src", "tgt", "compose").
    pub name: Arc<str>,
    /// Typed inputs as `(param_name, sort_expr)` pairs. Parameter names
    /// are in scope in later input sorts and in the output sort.
    pub inputs: Vec<(Arc<str>, SortExpr)>,
    /// The output sort expression. May reference any input parameter
    /// name as a `Term::Var`.
    pub output: SortExpr,
}

impl Operation {
    /// Create a new operation.
    ///
    /// The `inputs` vector carries `(param_name, sort_expr)` pairs; both
    /// positions accept anything that converts to [`Arc<str>`] /
    /// [`SortExpr`] respectively. The `output` accepts anything
    /// convertible to [`SortExpr`], including `&str`.
    #[must_use]
    pub fn new(
        name: impl Into<Arc<str>>,
        inputs: Vec<(Arc<str>, SortExpr)>,
        output: impl Into<SortExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs,
            output: output.into(),
        }
    }

    /// Create a unary operation (one input, one output).
    #[must_use]
    pub fn unary(
        name: impl Into<Arc<str>>,
        input_name: impl Into<Arc<str>>,
        input_sort: impl Into<SortExpr>,
        output: impl Into<SortExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs: vec![(input_name.into(), input_sort.into())],
            output: output.into(),
        }
    }

    /// Create a nullary operation (constant / zero-input constructor).
    #[must_use]
    pub fn nullary(name: impl Into<Arc<str>>, output: impl Into<SortExpr>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            output: output.into(),
        }
    }

    /// Returns the number of inputs.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.inputs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unary_op() {
        let op = Operation::unary("src", "e", "Edge", "Vertex");
        assert_eq!(op.arity(), 1);
        assert_eq!(&**op.output.head(), "Vertex");
    }

    #[test]
    fn nullary_op() {
        let op = Operation::nullary("zero", "Int");
        assert_eq!(op.arity(), 0);
        assert_eq!(&**op.output.head(), "Int");
    }

    #[test]
    fn binary_op() {
        let op = Operation::new(
            "add",
            vec![
                (Arc::from("a"), SortExpr::from("Int")),
                (Arc::from("b"), SortExpr::from("Int")),
            ],
            "Int",
        );
        assert_eq!(op.arity(), 2);
    }

    #[test]
    fn dependent_output() {
        use crate::eq::Term;
        let op = Operation::unary(
            "id",
            "x",
            "Ob",
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("x"), Term::var("x")],
            },
        );
        assert_eq!(&**op.output.head(), "Hom");
        assert_eq!(op.output.args().len(), 2);
    }
}
