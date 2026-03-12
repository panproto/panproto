/// An operation (term constructor) in a GAT.
///
/// Operations are the functions/constructors of a GAT. Each operation
/// has typed inputs and a typed output, where types are sort names.
///
/// # Examples
///
/// - `src: Edge → Vertex` (graph source map)
/// - `add: (a: Int, b: Int) → Int` (integer addition)
/// - `id: (x: Ob) → Hom(x, x)` (identity morphism)
///
/// Based on the formal definition of GAT operations from Cartmell (1986).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Operation {
    /// The operation name (e.g., "src", "tgt", "compose").
    pub name: String,
    /// Typed inputs as `(param_name, sort_name)` pairs.
    pub inputs: Vec<(String, String)>,
    /// The output sort name.
    pub output: String,
}

impl Operation {
    /// Create a new operation.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        inputs: Vec<(String, String)>,
        output: impl Into<String>,
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
        name: impl Into<String>,
        input_name: impl Into<String>,
        input_sort: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs: vec![(input_name.into(), input_sort.into())],
            output: output.into(),
        }
    }

    /// Create a nullary operation (constant / zero-input constructor).
    #[must_use]
    pub fn nullary(name: impl Into<String>, output: impl Into<String>) -> Self {
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
        assert_eq!(op.output, "Vertex");
    }

    #[test]
    fn nullary_op() {
        let op = Operation::nullary("zero", "Int");
        assert_eq!(op.arity(), 0);
        assert_eq!(op.output, "Int");
    }

    #[test]
    fn binary_op() {
        let op = Operation::new(
            "add",
            vec![("a".into(), "Int".into()), ("b".into(), "Int".into())],
            "Int",
        );
        assert_eq!(op.arity(), 2);
    }
}
