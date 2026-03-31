use std::sync::Arc;

/// Classification of a coercion's round-trip properties.
///
/// Forms a four-element lattice under information loss, shaped as a
/// diamond:
///
/// ```text
///          Iso
///         /   \
///  Retraction   Projection
///         \   /
///         Opaque
/// ```
///
/// Categorically, this classifies the adjunction witness of a fiber
/// morphism in the Grothendieck fibration over the schema category:
///
/// - `Iso`: both unit and counit are identities (isomorphism in the
///   fiber). Complement stores nothing.
/// - `Retraction`: the forward map has a left inverse
///   (`inverse(forward(v)) = v`). The forward map is injective
///   (information-preserving). Complement stores the residual between
///   the original and the round-tripped value.
/// - `Projection`: the forward map is a dependent projection from the
///   total fiber. The result is deterministically re-derivable from
///   the source data, but no inverse exists that recovers the source
///   from the result alone. Complement stores nothing for the derived
///   field because `get` re-derives it; this is the dual of
///   `Retraction` in the information-loss lattice.
/// - `Opaque`: no structural relationship between forward and backward
///   maps. Complement stores the entire original value.
///
/// Composition follows the lattice meet (in the quality ordering):
/// `Iso` is the identity, `Opaque` is the absorber, same-kind composes
/// idempotently, and cross-kind (`Retraction` ∘ `Projection`) collapses
/// to `Opaque`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum CoercionClass {
    /// Isomorphism: both round-trip laws hold. Complement stores nothing
    /// for this coercion.
    #[default]
    Iso,
    /// Retraction: `inverse(forward(v)) = v` but
    /// `forward(inverse(w)) ≠ w` in general. The forward map is
    /// injective (information-preserving). Complement stores the
    /// residual between the original and the round-tripped value.
    Retraction,
    /// Projection: the result is a deterministic function of the source
    /// fiber, but no inverse recovers the source from the result alone.
    /// This is the classification for `ComputeField` transforms that
    /// derive data from child scalar values (the dependent-sum
    /// projection). Complement stores nothing for the derived field
    /// because `get` re-derives it deterministically; modifications to
    /// the derived field in the view are not independently
    /// round-trippable (analogous to SQL computed columns).
    ///
    /// Dual to `Retraction` in the information-loss lattice:
    /// `Retraction` preserves information forward (left inverse exists),
    /// `Projection` is re-derivable (no inverse, but deterministic).
    Projection,
    /// No inverse exists and no structural re-derivation property holds.
    /// Complement stores the entire original value.
    Opaque,
}

impl CoercionClass {
    /// Compose two coercion classes (lattice meet in the quality
    /// ordering, equivalently lattice join in the information-loss
    /// ordering).
    ///
    /// Laws:
    /// - `Iso` is identity: `Iso.compose(x) = x`
    /// - `Opaque` absorbs: `Opaque.compose(x) = Opaque`
    /// - Same-kind is idempotent: `Retraction.compose(Retraction) = Retraction`,
    ///   `Projection.compose(Projection) = Projection`
    /// - Cross-kind collapses: `Retraction.compose(Projection) = Opaque`
    ///   (a retraction followed by a projection, or vice versa, has
    ///   neither a left inverse nor a re-derivation property)
    #[must_use]
    pub const fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::Iso, x) | (x, Self::Iso) => x,
            (Self::Opaque, _) | (_, Self::Opaque) => Self::Opaque,
            (Self::Retraction, Self::Retraction) => Self::Retraction,
            (Self::Projection, Self::Projection) => Self::Projection,
            // Cross-kind: retraction composed with projection (or vice
            // versa) has neither a left inverse nor re-derivability,
            // so it collapses to Opaque.
            (Self::Retraction, Self::Projection) | (Self::Projection, Self::Retraction) => {
                Self::Opaque
            }
        }
    }

    /// Returns `true` if this coercion is lossless (isomorphism).
    ///
    /// Only `Iso` is lossless: both round-trip laws hold. `Projection`
    /// is NOT lossless (the derivation discards source information),
    /// even though its complement happens to be empty (the derived value
    /// is re-computable, not stored).
    #[must_use]
    pub const fn is_lossless(self) -> bool {
        matches!(self, Self::Iso)
    }

    /// Returns `true` if the complement must store data for this
    /// coercion.
    ///
    /// `Iso` and `Projection` both have empty complements:
    /// - `Iso`: no information lost, nothing to store.
    /// - `Projection`: the derived value is re-computed by `get`
    ///   deterministically from the source fiber, so the complement
    ///   need not store it. (The source data itself survives through
    ///   the tree structure, not through this coercion's complement.)
    ///
    /// `Retraction` and `Opaque` require complement storage:
    /// - `Retraction`: stores the residual (the counit's failure).
    /// - `Opaque`: stores the entire original value.
    #[must_use]
    pub const fn needs_complement_storage(self) -> bool {
        matches!(self, Self::Retraction | Self::Opaque)
    }
}

impl PartialOrd for CoercionClass {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CoercionClass {
    /// Linear extension of the diamond partial order, reflecting
    /// increasing lossiness: `Iso < Retraction < Projection < Opaque`.
    ///
    /// In the diamond lattice, `Retraction` and `Projection` are
    /// incomparable (neither implies the other). This total order
    /// linearizes them by ranking `Retraction` below `Projection`
    /// because a retraction has a left inverse (the forward map
    /// preserves all information), while a projection has no inverse
    /// (though the result is re-derivable). The linearization is
    /// consistent with `compose`: `compose(a, b) >= max(a, b)` holds
    /// for all pairs.
    ///
    /// This ordering is used by the breaking-change detector: changing
    /// a coercion from `Retraction` to `Projection` is a downgrade
    /// (more structural information lost).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        const fn rank(c: CoercionClass) -> u8 {
            match c {
                CoercionClass::Iso => 0,
                CoercionClass::Retraction => 1,
                CoercionClass::Projection => 2,
                CoercionClass::Opaque => 3,
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

    const ALL_CLASSES: [CoercionClass; 4] = [
        CoercionClass::Iso,
        CoercionClass::Retraction,
        CoercionClass::Projection,
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

    #[test]
    fn needs_complement_storage_consistent_with_lattice() {
        // Iso and Projection have empty complement; Retraction and Opaque store data.
        // This matches the diamond lattice: the "upper" pair (Retraction, Opaque)
        // requires storage; the "lower" pair (Iso) and "re-derivable" (Projection)
        // do not.
        assert!(
            !CoercionClass::Iso.needs_complement_storage(),
            "Iso: lossless, no storage"
        );
        assert!(
            CoercionClass::Retraction.needs_complement_storage(),
            "Retraction: stores residual"
        );
        assert!(
            !CoercionClass::Projection.needs_complement_storage(),
            "Projection: derived value re-computed, no storage"
        );
        assert!(
            CoercionClass::Opaque.needs_complement_storage(),
            "Opaque: stores entire original"
        );
    }

    #[test]
    fn projection_compose_laws() {
        // Projection-specific composition laws that verify the diamond
        // lattice structure.

        // Projection is idempotent.
        assert_eq!(
            CoercionClass::Projection.compose(CoercionClass::Projection),
            CoercionClass::Projection,
            "Projection . Projection = Projection (projections compose)"
        );

        // Cross-kind composition collapses to Opaque: a retraction
        // (left inverse) composed with a projection (no inverse) has
        // neither property.
        assert_eq!(
            CoercionClass::Retraction.compose(CoercionClass::Projection),
            CoercionClass::Opaque,
            "Retraction . Projection = Opaque (diamond lattice meet)"
        );
        assert_eq!(
            CoercionClass::Projection.compose(CoercionClass::Retraction),
            CoercionClass::Opaque,
            "Projection . Retraction = Opaque (commutativity of meet)"
        );

        // Iso is identity for Projection.
        assert_eq!(
            CoercionClass::Iso.compose(CoercionClass::Projection),
            CoercionClass::Projection,
        );

        // Opaque absorbs Projection.
        assert_eq!(
            CoercionClass::Opaque.compose(CoercionClass::Projection),
            CoercionClass::Opaque,
        );
    }
}
