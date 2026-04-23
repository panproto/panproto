use std::sync::Arc;

use crate::sort::SortExpr;

/// Implicit/explicit tag on an [`Operation`] input parameter.
///
/// An implicit parameter is one whose value is inferred at a call site by
/// unifying the declared sorts of the explicit inputs against the
/// inferred sorts of the actual arguments. Explicit parameters must be
/// written at the call site.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Implicit {
    /// Explicit parameter: caller must supply the argument at every call
    /// site.
    #[default]
    No,
    /// Implicit parameter: its value is recovered at the call site by
    /// first-order unification against the explicit arguments.
    Yes,
}

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
    /// Typed inputs as `(param_name, sort_expr, implicit)` triples.
    /// Parameter names are in scope in later input sorts and in the
    /// output sort. The third component marks the parameter as implicit
    /// (value recovered by unification at the call site) or explicit
    /// (caller-supplied).
    pub inputs: Vec<(Arc<str>, SortExpr, Implicit)>,
    /// The output sort expression. May reference any input parameter
    /// name as a `Term::Var`.
    pub output: SortExpr,
}

impl Operation {
    /// Create a new operation with every input marked explicit.
    ///
    /// The `inputs` vector carries `(param_name, sort_expr)` pairs; both
    /// positions accept anything that converts to [`Arc<str>`] /
    /// [`SortExpr`] respectively. The `output` accepts anything
    /// convertible to [`SortExpr`], including `&str`. Each input is
    /// tagged [`Implicit::No`]. For mixed implicit/explicit inputs use
    /// [`Self::with_implicit`].
    #[must_use]
    pub fn new(
        name: impl Into<Arc<str>>,
        inputs: Vec<(Arc<str>, SortExpr)>,
        output: impl Into<SortExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs: inputs
                .into_iter()
                .map(|(n, s)| (n, s, Implicit::No))
                .collect(),
            output: output.into(),
        }
    }

    /// Create a new operation from fully-specified `(name, sort, implicit)`
    /// triples.
    #[must_use]
    pub fn with_implicit(
        name: impl Into<Arc<str>>,
        inputs: Vec<(Arc<str>, SortExpr, Implicit)>,
        output: impl Into<SortExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs,
            output: output.into(),
        }
    }

    /// Create a unary operation (one explicit input, one output).
    #[must_use]
    pub fn unary(
        name: impl Into<Arc<str>>,
        input_name: impl Into<Arc<str>>,
        input_sort: impl Into<SortExpr>,
        output: impl Into<SortExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs: vec![(input_name.into(), input_sort.into(), Implicit::No)],
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

    /// Returns the total number of inputs (implicit + explicit).
    #[must_use]
    pub fn arity(&self) -> usize {
        self.inputs.len()
    }

    /// Returns the number of explicit (caller-supplied) inputs.
    #[must_use]
    pub fn explicit_arity(&self) -> usize {
        self.inputs
            .iter()
            .filter(|(_, _, imp)| matches!(imp, Implicit::No))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Term;

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
        assert_eq!(op.explicit_arity(), 2);
        assert!(
            op.inputs
                .iter()
                .all(|(_, _, imp)| matches!(imp, Implicit::No))
        );
    }

    #[test]
    fn with_implicit_inputs_preserves_tags() {
        let op = Operation::with_implicit(
            "app",
            vec![
                (Arc::from("a"), SortExpr::from("Ty"), Implicit::Yes),
                (Arc::from("b"), SortExpr::from("Ty"), Implicit::Yes),
                (
                    Arc::from("f"),
                    SortExpr::App {
                        name: Arc::from("arrow"),
                        args: vec![Term::var("a"), Term::var("b")],
                    },
                    Implicit::No,
                ),
            ],
            SortExpr::Name(Arc::from("Ty")),
        );
        assert_eq!(op.arity(), 3);
        assert_eq!(op.explicit_arity(), 1);
    }

    #[test]
    fn dependent_output() {
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
