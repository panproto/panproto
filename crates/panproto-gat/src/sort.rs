use std::sync::Arc;

/// Classification of a coercion's round-trip properties.
///
/// Forms a three-element lattice under information loss: Iso ≤ Retraction ≤ Opaque.
/// Categorically, this classifies the adjunction witness of a fiber morphism in
/// the Grothendieck fibration over the schema category:
///
/// - `Iso`: both unit and counit are identities (isomorphism in the fiber)
/// - `Retraction`: unit is identity, counit is not (section/retraction pair)
/// - `Opaque`: no adjoint exists (the complement stores the entire original value)
///
/// Composition follows the lattice product: Iso ∘ Iso = Iso, anything ∘ Opaque = Opaque,
/// otherwise Retraction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum CoercionClass {
    /// Isomorphism: both round-trip laws hold. Complement stores nothing for this coercion.
    #[default]
    Iso,
    /// Retraction: `inverse(forward(v)) = v` but `forward(inverse(w)) ≠ w` in general.
    /// Complement stores the residual between the original and the round-tripped value.
    Retraction,
    /// No inverse exists. Complement stores the entire original value.
    Opaque,
}

impl CoercionClass {
    /// Compose two coercion classes (lattice product in the information loss ordering).
    #[must_use]
    pub const fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::Iso, x) | (x, Self::Iso) => x,
            (Self::Opaque, _) | (_, Self::Opaque) => Self::Opaque,
            (Self::Retraction, Self::Retraction) => Self::Retraction,
        }
    }

    /// Returns `true` if this coercion is lossless (isomorphism).
    #[must_use]
    pub const fn is_lossless(self) -> bool {
        matches!(self, Self::Iso)
    }
}

impl PartialOrd for CoercionClass {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CoercionClass {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        const fn rank(c: CoercionClass) -> u8 {
            match c {
                CoercionClass::Iso => 0,
                CoercionClass::Retraction => 1,
                CoercionClass::Opaque => 2,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

/// Classify a builtin coercion operation by its source/target value kinds
/// and round-trip class.
///
/// Returns `None` for non-coercion builtins.
#[must_use]
pub const fn classify_builtin_coercion(
    op: panproto_expr::BuiltinOp,
) -> Option<(ValueKind, ValueKind, CoercionClass)> {
    use panproto_expr::BuiltinOp;
    match op {
        // Int → Float: every i64 maps to a distinct f64 (within 2^53),
        // but not every f64 maps back. Retraction: float_to_int(int_to_float(n)) = n.
        BuiltinOp::IntToFloat => {
            Some((ValueKind::Int, ValueKind::Float, CoercionClass::Retraction))
        }
        // Float → Int: truncation loses fractional part. No guaranteed inverse.
        BuiltinOp::FloatToInt => Some((ValueKind::Float, ValueKind::Int, CoercionClass::Opaque)),
        // Int → Str: every int has a canonical string form and str_to_int(int_to_str(n)) = n,
        // but not every string is a valid int, so int_to_str(str_to_int(s)) ≠ s in general.
        // This is a retraction (section of str_to_int), not an iso.
        BuiltinOp::IntToStr => Some((ValueKind::Int, ValueKind::Str, CoercionClass::Retraction)),
        // Float → Str: formatting may lose precision (e.g., "1.0" round-trips to "1").
        // Neither direction is guaranteed to round-trip. Opaque.
        BuiltinOp::FloatToStr => Some((ValueKind::Float, ValueKind::Str, CoercionClass::Opaque)),
        // Str → Int: fails on non-numeric strings. Opaque.
        BuiltinOp::StrToInt => Some((ValueKind::Str, ValueKind::Int, CoercionClass::Opaque)),
        // Str → Float: fails on non-numeric strings. Opaque.
        BuiltinOp::StrToFloat => Some((ValueKind::Str, ValueKind::Float, CoercionClass::Opaque)),
        _ => None,
    }
}

/// The primitive value kind that a value sort ranges over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValueKind {
    /// Boolean values.
    Bool,
    /// Integer values.
    Int,
    /// Floating-point values.
    Float,
    /// String values.
    Str,
    /// Byte-sequence values.
    Bytes,
    /// Opaque token values.
    Token,
    /// Null / unit values.
    Null,
    /// Any value kind (polymorphic).
    Any,
}

/// The kind of a sort, distinguishing structural sorts from value/coercion sorts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SortKind {
    /// Standard structural sort (vertices, edges, constraints).
    #[default]
    Structural,
    /// Value sort: carries data of a specific kind.
    Val(ValueKind),
    /// Coercion sort: a directed morphism between value kinds.
    /// Categorically, this is a morphism in the fiber category over the schema.
    Coercion {
        /// The source value kind.
        from: ValueKind,
        /// The target value kind.
        to: ValueKind,
        /// Round-trip classification of this coercion.
        class: CoercionClass,
    },
    /// Merger sort: combines values of a specific kind.
    Merger(ValueKind),
}

/// A parameter of a dependent sort.
///
/// Sort parameters allow sorts to depend on terms of other sorts,
/// which is the key feature distinguishing GATs from ordinary algebraic theories.
///
/// # Example
///
/// In the theory of categories, `Hom(a: Ob, b: Ob)` has two parameters
/// of sort `Ob`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SortParam {
    /// The parameter name (e.g., "a", "b").
    pub name: Arc<str>,
    /// The sort this parameter ranges over (e.g., "Ob").
    pub sort: Arc<str>,
}

/// A sort declaration in a GAT.
///
/// Sorts are the types of a GAT. They may be simple (no parameters)
/// or dependent (parameterized by terms of other sorts).
///
/// # Examples
///
/// - Simple sort: `Vertex` (no params)
/// - Dependent sort: `Hom(a: Ob, b: Ob)` (two params of sort `Ob`)
/// - Dependent sort: `Constraint(v: Vertex)` (one param of sort `Vertex`)
///
/// Based on the formal definition of GAT sorts from Cartmell (1986).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Sort {
    /// The sort name (e.g., "Vertex", "Edge", "Hom").
    pub name: Arc<str>,
    /// Parameters this sort depends on. Empty for simple sorts.
    pub params: Vec<SortParam>,
    /// The kind of this sort (structural, value, coercion, or merger).
    #[serde(default)]
    pub kind: SortKind,
}

impl Sort {
    /// Create a simple (non-dependent) sort with structural kind.
    #[must_use]
    pub fn simple(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            kind: SortKind::default(),
        }
    }

    /// Create a dependent sort with the given parameters and structural kind.
    #[must_use]
    pub fn dependent(name: impl Into<Arc<str>>, params: Vec<SortParam>) -> Self {
        Self {
            name: name.into(),
            params,
            kind: SortKind::default(),
        }
    }

    /// Create a simple sort with a specific kind.
    #[must_use]
    pub fn with_kind(name: impl Into<Arc<str>>, kind: SortKind) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            kind,
        }
    }

    /// Returns `true` if this sort has no parameters.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.params.is_empty()
    }

    /// Returns the arity (number of parameters) of this sort.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.params.len()
    }
}

impl SortParam {
    /// Create a new sort parameter.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>, sort: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            sort: sort.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_sort() {
        let s = Sort::simple("Vertex");
        assert!(s.is_simple());
        assert_eq!(s.arity(), 0);
        assert_eq!(&*s.name, "Vertex");
    }

    #[test]
    fn dependent_sort() {
        let s = Sort::dependent(
            "Hom",
            vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
        );
        assert!(!s.is_simple());
        assert_eq!(s.arity(), 2);
    }

    // --- CoercionClass algebraic law tests ---

    const ALL_CLASSES: [CoercionClass; 3] = [
        CoercionClass::Iso,
        CoercionClass::Retraction,
        CoercionClass::Opaque,
    ];

    #[test]
    fn coercion_class_identity() {
        for &x in &ALL_CLASSES {
            assert_eq!(CoercionClass::Iso.compose(x), x, "Iso is left identity");
            assert_eq!(x.compose(CoercionClass::Iso), x, "Iso is right identity");
        }
    }

    #[test]
    fn coercion_class_absorption() {
        for &x in &ALL_CLASSES {
            assert_eq!(
                CoercionClass::Opaque.compose(x),
                CoercionClass::Opaque,
                "Opaque absorbs on left"
            );
            assert_eq!(
                x.compose(CoercionClass::Opaque),
                CoercionClass::Opaque,
                "Opaque absorbs on right"
            );
        }
    }

    #[test]
    fn coercion_class_associativity() {
        for &a in &ALL_CLASSES {
            for &b in &ALL_CLASSES {
                for &c in &ALL_CLASSES {
                    assert_eq!(
                        a.compose(b).compose(c),
                        a.compose(b.compose(c)),
                        "associativity: ({a:?} . {b:?}) . {c:?} == {a:?} . ({b:?} . {c:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn coercion_class_commutativity() {
        for &a in &ALL_CLASSES {
            for &b in &ALL_CLASSES {
                assert_eq!(
                    a.compose(b),
                    b.compose(a),
                    "commutativity: {a:?} . {b:?} == {b:?} . {a:?}"
                );
            }
        }
    }

    #[test]
    fn coercion_class_ordering_consistent_with_compose() {
        for &a in &ALL_CLASSES {
            for &b in &ALL_CLASSES {
                let composed = a.compose(b);
                assert!(
                    composed >= a,
                    "compose({a:?}, {b:?}) = {composed:?} should be >= {a:?}"
                );
                assert!(
                    composed >= b,
                    "compose({a:?}, {b:?}) = {composed:?} should be >= {b:?}"
                );
            }
        }
    }

    #[test]
    fn classify_builtin_coercion_coverage() {
        use panproto_expr::BuiltinOp;

        // Every coercion builtin is classified.
        let coercion_ops = [
            BuiltinOp::IntToFloat,
            BuiltinOp::FloatToInt,
            BuiltinOp::IntToStr,
            BuiltinOp::FloatToStr,
            BuiltinOp::StrToInt,
            BuiltinOp::StrToFloat,
        ];
        for op in coercion_ops {
            assert!(
                classify_builtin_coercion(op).is_some(),
                "{op:?} should be classified"
            );
        }

        // Non-coercion builtins are not classified.
        assert!(classify_builtin_coercion(BuiltinOp::Add).is_none());
        assert!(classify_builtin_coercion(BuiltinOp::Concat).is_none());
    }

    #[test]
    fn no_builtin_classified_as_iso() {
        use panproto_expr::BuiltinOp;

        // No builtin coercion should be Iso (all have failure modes or precision loss).
        let coercion_ops = [
            BuiltinOp::IntToFloat,
            BuiltinOp::FloatToInt,
            BuiltinOp::IntToStr,
            BuiltinOp::FloatToStr,
            BuiltinOp::StrToInt,
            BuiltinOp::StrToFloat,
        ];
        for op in coercion_ops {
            if let Some((_, _, class)) = classify_builtin_coercion(op) {
                assert_ne!(
                    class,
                    CoercionClass::Iso,
                    "{op:?} should not be classified as Iso"
                );
            }
        }
    }
}
