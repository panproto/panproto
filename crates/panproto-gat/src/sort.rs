use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::Term;

/// A sort expression: a plain sort name or a dependent sort applied to
/// argument terms.
///
/// Appears in [`Operation::output`](crate::op::Operation::output),
/// [`Operation::inputs`](crate::op::Operation::inputs) entries, and
/// [`SortParam::sort`], wherever a sort occurs.
///
/// Normalization invariant: every [`SortExpr`] produced by constructors
/// and public operations is normalized. A normalized value uses the
/// `Name` spelling whenever the argument list is empty; `App { name, args:
/// [] }` never appears in normalized values. This invariant is enforced
/// by the smart constructor [`SortExpr::app`], by [`Self::subst`], by
/// [`Self::rename_head`], by [`Self::apply_maps`], and by the custom
/// `serde::Deserialize` impl. The derived [`PartialEq`] and [`Hash`] are
/// replaced with manual impls that treat `Name(n)` and `App { name: n,
/// args: [] }` as equal (in case a caller constructs a non-normalized
/// value directly through the `App` variant).
///
/// The serde representation uses `#[serde(untagged)]`, so `Name(n)`
/// serializes as the bare string `"n"` and `App` serializes as a struct
/// with `name` and `args` fields.
#[derive(Debug, Clone, Eq, serde::Serialize)]
#[serde(untagged)]
pub enum SortExpr {
    /// A plain sort name with no parameters applied.
    Name(Arc<str>),
    /// A dependent sort applied to argument terms, e.g. `Tm(Γ, A)`.
    App {
        /// The sort's declared name.
        name: Arc<str>,
        /// Argument terms, one per declared sort parameter.
        args: Vec<Term>,
    },
}

impl SortExpr {
    /// Smart constructor: produces [`Self::Name`] when `args` is empty,
    /// otherwise [`Self::App`]. This is the only way to construct a
    /// normalized `SortExpr::App`; direct use of the `App` variant is
    /// allowed for backwards compatibility, but normalization should be
    /// restored via [`Self::normalize`] before the value escapes its
    /// local context.
    #[must_use]
    pub fn app(name: impl Into<Arc<str>>, args: Vec<Term>) -> Self {
        if args.is_empty() {
            Self::Name(name.into())
        } else {
            Self::App {
                name: name.into(),
                args,
            }
        }
    }

    /// Normalize this expression: collapse `App { name, args: [] }` into
    /// `Name(name)`. Idempotent; leaves already-normalized values
    /// unchanged. The argument list is not recursed into because the
    /// term AST inside arguments does not share this two-spelling
    /// ambiguity.
    #[must_use]
    pub fn normalize(self) -> Self {
        if let Self::App { name, args } = &self {
            if args.is_empty() {
                return Self::Name(Arc::clone(name));
            }
        }
        self
    }

    /// Extract the bare sort name, ignoring any applied arguments.
    #[must_use]
    pub const fn head(&self) -> &Arc<str> {
        match self {
            Self::Name(n) | Self::App { name: n, .. } => n,
        }
    }

    /// Argument terms, if any; empty slice for `Name`.
    #[must_use]
    pub fn args(&self) -> &[Term] {
        match self {
            Self::Name(_) => &[],
            Self::App { args, .. } => args,
        }
    }

    /// Substitute `mapping` (parameter name to term) throughout this sort
    /// expression's argument terms.
    #[must_use]
    pub fn subst(&self, mapping: &FxHashMap<Arc<str>, Term>) -> Self {
        match self {
            Self::Name(n) => Self::Name(Arc::clone(n)),
            Self::App { name, args } => Self::app(
                Arc::clone(name),
                args.iter().map(|t| t.substitute(mapping)).collect(),
            ),
        }
    }

    /// Structural equality modulo `Name(n) == App { name: n, args: [] }`.
    ///
    /// Equivalent to [`PartialEq::eq`] after the normalization invariant;
    /// retained as a named method for documentation and for code that
    /// wants to make the two-spelling quotient explicit at call sites.
    #[must_use]
    pub fn alpha_eq(&self, other: &Self) -> bool {
        self.head() == other.head() && self.args() == other.args()
    }

    /// Definitional equality modulo a directed-rewrite system.
    ///
    /// Normalizes both sides by rewriting every argument term with
    /// [`crate::eq::normalize`] under `rules` and a bounded step budget,
    /// then compares the normalized sort expressions with
    /// [`Self::alpha_eq`]. Heads must agree; if they do not, no amount
    /// of rewriting closes the gap and the function returns `false`
    /// immediately. Callers that want the strict structural equality
    /// should continue to use [`Self::alpha_eq`].
    #[must_use]
    pub fn alpha_eq_modulo_rewrites(
        &self,
        other: &Self,
        rules: &[crate::eq::DirectedEquation],
        step_limit: usize,
    ) -> bool {
        self.alpha_eq_modulo_rewrites_status(other, rules, step_limit)
            .0
    }

    /// As [`alpha_eq_modulo_rewrites`](Self::alpha_eq_modulo_rewrites), but also
    /// reports whether any argument normalization exhausted its step budget.
    ///
    /// The second component is `true` when equality could not be decided
    /// because a normalization ran out of steps. Callers surface a budget
    /// error rather than a spurious inequality in that case.
    #[must_use]
    pub fn alpha_eq_modulo_rewrites_status(
        &self,
        other: &Self,
        rules: &[crate::eq::DirectedEquation],
        step_limit: usize,
    ) -> (bool, bool) {
        if self.head() != other.head() || self.args().len() != other.args().len() {
            return (false, false);
        }
        let mut exhausted = false;
        let mut normalize_all = |args: &[Term]| -> Vec<Term> {
            args.iter()
                .map(|t| {
                    let (nf, ex) = crate::eq::normalize_with_status(t, rules, step_limit);
                    exhausted |= ex;
                    nf
                })
                .collect()
        };
        let left = normalize_all(self.args());
        let right = normalize_all(other.args());
        (left == right, exhausted)
    }

    /// Rename the head (sort name) via `sort_map`, leaving arguments
    /// unchanged.
    #[must_use]
    pub fn rename_head(&self, sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>) -> Self {
        match self {
            Self::Name(n) => Self::Name(sort_map.get(n).cloned().unwrap_or_else(|| Arc::clone(n))),
            Self::App { name, args } => Self::app(
                sort_map
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| Arc::clone(name)),
                args.clone(),
            ),
        }
    }

    /// Apply a sort-name rename to the head and an operation rename to
    /// every term in the argument list.
    #[must_use]
    pub fn apply_maps(
        &self,
        sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
        op_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
    ) -> Self {
        match self {
            Self::Name(n) => Self::Name(sort_map.get(n).cloned().unwrap_or_else(|| Arc::clone(n))),
            Self::App { name, args } => Self::app(
                sort_map
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| Arc::clone(name)),
                args.iter().map(|t| t.rename_ops(op_map)).collect(),
            ),
        }
    }
}

/// Build a variable-rename substitution that sends each domain parameter
/// name to the codomain parameter name at the same position.
///
/// Returned substitution is suitable for feeding to
/// [`SortExpr::subst`](crate::sort::SortExpr::subst) or
/// [`Term::substitute`](crate::eq::Term::substitute). Positions past the
/// shorter of the two sequences are not bound; callers that need an
/// arity equality check should verify it before calling this helper.
///
/// Identity pairs (where the two names agree) are omitted, so the
/// returned substitution is the identity when both sides already use the
/// same names and `subst` becomes a no-op on short-circuit paths.
#[must_use]
pub fn positional_param_rename<I, J>(
    domain_params: I,
    target_params: J,
) -> FxHashMap<Arc<str>, Term>
where
    I: IntoIterator<Item = Arc<str>>,
    J: IntoIterator<Item = Arc<str>>,
{
    let mut rename = FxHashMap::default();
    for (d, t) in domain_params.into_iter().zip(target_params) {
        if d != t {
            rename.insert(d, Term::Var(t));
        }
    }
    rename
}

/// Compare two operation or sort signatures modulo positional alpha-rename
/// of the declared parameter names.
///
/// Each signature is a list of `(param_name, param_sort)` pairs plus an
/// output sort expression. Parameter names are local binders and are in
/// scope in every later parameter sort and in the output. Two signatures
/// are considered equivalent when the substitution that sends the left
/// side's parameter names to the right side's (positionally) makes the
/// sort expressions pairwise equal under [`SortExpr::alpha_eq`].
///
/// Arity mismatches fail. If either side has fewer parameters than the
/// other this function returns `false`.
#[must_use]
pub fn signatures_equivalent_modulo_param_rename(
    lhs_inputs: &[(Arc<str>, SortExpr, crate::op::Implicit)],
    lhs_output: &SortExpr,
    rhs_inputs: &[(Arc<str>, SortExpr, crate::op::Implicit)],
    rhs_output: &SortExpr,
) -> bool {
    if lhs_inputs.len() != rhs_inputs.len() {
        return false;
    }
    let rename = positional_param_rename(
        lhs_inputs.iter().map(|(n, _, _)| Arc::clone(n)),
        rhs_inputs.iter().map(|(n, _, _)| Arc::clone(n)),
    );
    for ((_, lhs_sort, l_imp), (_, rhs_sort, r_imp)) in lhs_inputs.iter().zip(rhs_inputs.iter()) {
        if l_imp != r_imp {
            return false;
        }
        if !lhs_sort.subst(&rename).alpha_eq(rhs_sort) {
            return false;
        }
    }
    lhs_output.subst(&rename).alpha_eq(rhs_output)
}

/// Compare two sort declarations' parameter blocks modulo positional
/// alpha-rename of the declared parameter names.
///
/// Sort parameter sorts may themselves be dependent (e.g.
/// `(Γ : Ctx, A : Ty(Γ))`), so the rename is accumulated positionally
/// and applied to every later parameter sort.
#[must_use]
pub fn sort_params_equivalent_modulo_rename(lhs: &[SortParam], rhs: &[SortParam]) -> bool {
    if lhs.len() != rhs.len() {
        return false;
    }
    let rename = positional_param_rename(
        lhs.iter().map(|p| Arc::clone(&p.name)),
        rhs.iter().map(|p| Arc::clone(&p.name)),
    );
    lhs.iter()
        .zip(rhs.iter())
        .all(|(l, r)| l.sort.subst(&rename).alpha_eq(&r.sort))
}

impl PartialEq for SortExpr {
    /// Two sort expressions are equal when their heads and argument lists
    /// agree. This reduces `Name(n)` to `App { name: n, args: [] }` under
    /// equality, matching [`SortExpr::alpha_eq`] and ensuring that the
    /// `Eq`/`Hash` contract holds across both spellings.
    fn eq(&self, other: &Self) -> bool {
        self.head() == other.head() && self.args() == other.args()
    }
}

impl std::hash::Hash for SortExpr {
    /// Hash by head and arguments. This agrees with [`PartialEq::eq`]
    /// across the `Name` / empty-args `App` quotient so that both
    /// spellings occupy the same hash bucket when used as a map key.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.head().hash(state);
        self.args().hash(state);
    }
}

impl<'de> serde::Deserialize<'de> for SortExpr {
    /// Normalize on load: any `App { name, args: [] }` incoming from JSON
    /// collapses to `Name(name)` so that every downstream consumer sees
    /// the canonical spelling.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(Arc<str>),
            App { name: Arc<str>, args: Vec<Term> },
        }
        match Raw::deserialize(deserializer)? {
            Raw::Name(n) => Ok(Self::Name(n)),
            Raw::App { name, args } => Ok(Self::app(name, args)),
        }
    }
}

impl std::fmt::Display for SortExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(n) => f.write_str(n),
            Self::App { name, args } => {
                f.write_str(name)?;
                f.write_str("(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Var(n) => f.write_str(n),
            Self::App { op, args } if args.is_empty() => write!(f, "{op}()"),
            Self::App { op, args } => {
                write!(f, "{op}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
            }
            Self::Hole { name } => match name {
                Some(n) => write!(f, "?{n}"),
                None => f.write_str("?"),
            },
            Self::Let { name, bound, body } => {
                write!(f, "let {name} = {bound} in {body}")
            }
            Self::Case {
                scrutinee,
                branches,
            } => {
                write!(f, "case {scrutinee} of ")?;
                for (i, b) in branches.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{}(", b.constructor)?;
                    for (j, binder) in b.binders.iter().enumerate() {
                        if j > 0 {
                            f.write_str(", ")?;
                        }
                        f.write_str(binder)?;
                    }
                    write!(f, ") => {}", b.body)?;
                }
                f.write_str(" end")
            }
        }
    }
}

impl From<&str> for SortExpr {
    fn from(s: &str) -> Self {
        Self::Name(Arc::from(s))
    }
}

impl From<String> for SortExpr {
    fn from(s: String) -> Self {
        Self::Name(Arc::from(s))
    }
}

impl From<Arc<str>> for SortExpr {
    fn from(s: Arc<str>) -> Self {
        Self::Name(s)
    }
}

impl From<&Arc<str>> for SortExpr {
    fn from(s: &Arc<str>) -> Self {
        Self::Name(Arc::clone(s))
    }
}

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

    /// Every currently-known variant of [`CoercionClass`].
    ///
    /// The returned slice is the authoritative iteration order for
    /// exhaustiveness-sensitive consumers (sample-based law checkers,
    /// regression tests that must run once per class, breaking-change
    /// migration tables). A dummy `match` below forces a compile
    /// error when a new variant is added upstream: extend both the
    /// match arms and the returned slice together.
    ///
    /// `CoercionClass` is `#[non_exhaustive]`; callers outside this
    /// crate cannot write a `match` that panics on unknown variants
    /// without a wildcard arm. This method lets them stay in lockstep
    /// with the enum anyway.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        // Exhaustiveness guard: adding a new variant to
        // `CoercionClass` breaks this match and forces an update to
        // `ALL` below.
        const fn _exhaustiveness_witness(c: CoercionClass) {
            match c {
                CoercionClass::Iso
                | CoercionClass::Retraction
                | CoercionClass::Projection
                | CoercionClass::Opaque => {}
            }
        }
        const ALL: &[CoercionClass] = &[
            CoercionClass::Iso,
            CoercionClass::Retraction,
            CoercionClass::Projection,
            CoercionClass::Opaque,
        ];
        ALL
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
    /// Refined string: an ISO 8601 date-time (`2026-07-14T09:47:12Z`).
    ///
    /// The refined scalar kinds are declared after `Any` so the discriminants
    /// of the original kinds are preserved when new refinements are appended.
    DateTime,
    /// Refined string: an ISO 8601 calendar date (`2026-07-14`).
    Date,
    /// Refined string: an ISO 8601 wall-clock time (`09:47:12`).
    Time,
    /// Refined numeric: an exact base-10 decimal (no binary float rounding).
    Decimal,
    /// Refined string: an RFC 4122 UUID.
    Uuid,
}

impl ValueKind {
    /// Returns the canonical string representation of this value kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Bool => "boolean",
            Self::Int => "integer",
            Self::Float => "number",
            Self::Str => "string",
            Self::Bytes => "bytes",
            Self::Token => "token",
            Self::Null => "null",
            Self::DateTime => "date-time",
            Self::Date => "date",
            Self::Time => "time",
            Self::Decimal => "decimal",
            Self::Uuid => "uuid",
            Self::Any => "any",
        }
    }

    /// Every variant of [`ValueKind`], in a fixed canonical order.
    ///
    /// Consumers that need to iterate every value kind (sample
    /// registries, coverage regression tests, kind-indexed tables)
    /// should use this rather than hand-written slice literals; the
    /// dummy `match` below turns a silent omission into a compile
    /// error when a new variant is added.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        // Exhaustiveness guard: adding a new variant to `ValueKind`
        // breaks this match and forces an update to `ALL` below.
        const fn _exhaustiveness_witness(k: ValueKind) {
            match k {
                ValueKind::Bool
                | ValueKind::Int
                | ValueKind::Float
                | ValueKind::Str
                | ValueKind::Bytes
                | ValueKind::Token
                | ValueKind::Null
                | ValueKind::DateTime
                | ValueKind::Date
                | ValueKind::Time
                | ValueKind::Decimal
                | ValueKind::Uuid
                | ValueKind::Any => {}
            }
        }
        const ALL: &[ValueKind] = &[
            ValueKind::Bool,
            ValueKind::Int,
            ValueKind::Float,
            ValueKind::Str,
            ValueKind::Bytes,
            ValueKind::Token,
            ValueKind::Null,
            ValueKind::DateTime,
            ValueKind::Date,
            ValueKind::Time,
            ValueKind::Decimal,
            ValueKind::Uuid,
            ValueKind::Any,
        ];
        ALL
    }
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
    /// The sort expression this parameter ranges over (e.g., `Ob` or
    /// a dependent sort like `Ty(Γ)`).
    pub sort: SortExpr,
}

/// Closure attribute on a [`Sort`].
///
/// An open sort may be inhabited by any operation whose output head
/// names this sort. A closed sort enumerates the exhaustive set of
/// constructor operations, enabling pattern matching via `Term::Case`
/// and exhaustiveness checking at theory-declaration time.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SortClosure {
    /// Open sort: any operation with this output head may produce an
    /// inhabitant. No exhaustiveness check applies.
    #[default]
    Open,
    /// Closed sort: the listed op names form the complete set of
    /// introduction forms. Pattern matches over this sort must cover
    /// every listed constructor exactly once.
    Closed(Vec<Arc<str>>),
}

/// A sort declaration in a GAT.
///
/// Sorts are the types of a GAT. They may be simple (no parameters)
/// or dependent (parameterized by terms of other sorts). A sort may
/// additionally be declared closed against an enumerated set of
/// constructors, enabling exhaustive pattern matching.
///
/// # Examples
///
/// - Simple sort: `Vertex` (no params)
/// - Dependent sort: `Hom(a: Ob, b: Ob)` (two params of sort `Ob`)
/// - Dependent sort: `Constraint(v: Vertex)` (one param of sort `Vertex`)
/// - Closed inductive sort: `Nat` closed against `[zero, succ]`
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
    /// Closure attribute. Defaults to [`SortClosure::Open`].
    #[serde(default)]
    pub closure: SortClosure,
}

impl Sort {
    /// Create a simple (non-dependent) sort with structural kind and
    /// open closure.
    #[must_use]
    pub fn simple(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            kind: SortKind::default(),
            closure: SortClosure::Open,
        }
    }

    /// Create a dependent sort with the given parameters, structural
    /// kind, and open closure.
    #[must_use]
    pub fn dependent(name: impl Into<Arc<str>>, params: Vec<SortParam>) -> Self {
        Self {
            name: name.into(),
            params,
            kind: SortKind::default(),
            closure: SortClosure::Open,
        }
    }

    /// Create a simple sort with a specific kind and open closure.
    #[must_use]
    pub fn with_kind(name: impl Into<Arc<str>>, kind: SortKind) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            kind,
            closure: SortClosure::Open,
        }
    }

    /// Create a closed simple sort with the given constructor op names.
    ///
    /// The closure declares these ops as the exhaustive set of
    /// constructors for this sort. A `Term::Case` over the sort must
    /// cover every listed constructor exactly once; adding a new
    /// constructor elsewhere in the theory without updating the
    /// closure fails `typecheck_theory`.
    #[must_use]
    pub fn closed<I, S>(name: impl Into<Arc<str>>, params: Vec<SortParam>, constructors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        Self {
            name: name.into(),
            params,
            kind: SortKind::default(),
            closure: SortClosure::Closed(constructors.into_iter().map(Into::into).collect()),
        }
    }

    /// Returns the canonical schema-level vertex kind for this sort.
    ///
    /// The vertex kind is derived from the `SortKind` via the Grothendieck
    /// fibration: `Val(vk)` maps to the canonical value kind name,
    /// `Structural` maps to the sort name itself.
    #[must_use]
    pub fn default_vertex_kind(&self) -> Arc<str> {
        match &self.kind {
            SortKind::Val(vk) => Arc::from(vk.as_str()),
            SortKind::Structural | SortKind::Coercion { .. } | SortKind::Merger(_) => {
                Arc::clone(&self.name)
            }
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
    ///
    /// The `sort` argument may be any value that converts to a
    /// [`SortExpr`], including `&str`, `String`, `Arc<str>`, or a
    /// fully-specified `SortExpr::App`.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>, sort: impl Into<SortExpr>) -> Self {
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

    // --- SortExpr tests ---

    #[test]
    fn sort_expr_from_str() {
        let e: SortExpr = "Ob".into();
        assert_eq!(e, SortExpr::Name(Arc::from("Ob")));
        assert_eq!(&**e.head(), "Ob");
    }

    #[test]
    fn sort_expr_app_head() {
        let e = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        assert_eq!(&**e.head(), "Hom");
        assert_eq!(e.args().len(), 2);
    }

    #[test]
    fn sort_expr_alpha_eq_name_vs_empty_app() {
        let a = SortExpr::Name(Arc::from("Ob"));
        let b = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        assert!(a.alpha_eq(&b));
        assert!(b.alpha_eq(&a));
    }

    #[test]
    fn sort_expr_alpha_eq_structural() {
        let a = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let b = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let c = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("y"), Term::var("x")],
        };
        assert!(a.alpha_eq(&b));
        assert!(!a.alpha_eq(&c));
    }

    #[test]
    fn sort_expr_subst() {
        let e = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let mut mapping: FxHashMap<Arc<str>, Term> = FxHashMap::default();
        mapping.insert(Arc::from("x"), Term::constant("a"));
        mapping.insert(Arc::from("y"), Term::constant("b"));
        let result = e.subst(&mapping);
        assert_eq!(
            result,
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::constant("a"), Term::constant("b")],
            }
        );
    }

    #[test]
    fn sort_expr_subst_name_unchanged() {
        let e = SortExpr::Name(Arc::from("Ob"));
        let mut mapping: FxHashMap<Arc<str>, Term> = FxHashMap::default();
        mapping.insert(Arc::from("x"), Term::constant("a"));
        assert_eq!(e.subst(&mapping), e);
    }

    #[test]
    fn sort_expr_serde_name_is_bare_string() -> Result<(), Box<dyn std::error::Error>> {
        let e = SortExpr::Name(Arc::from("Ob"));
        let s = serde_json::to_string(&e)?;
        assert_eq!(s, "\"Ob\"");
        let back: SortExpr = serde_json::from_str(&s)?;
        assert_eq!(back, e);
        Ok(())
    }

    #[test]
    fn sort_expr_serde_app_is_struct() -> Result<(), Box<dyn std::error::Error>> {
        let e = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let s = serde_json::to_string(&e)?;
        let back: SortExpr = serde_json::from_str(&s)?;
        assert_eq!(back, e);
        Ok(())
    }

    #[test]
    fn sort_expr_display() {
        let e = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        assert_eq!(format!("{e}"), "Hom(x, y)");
        let n = SortExpr::Name(Arc::from("Ob"));
        assert_eq!(format!("{n}"), "Ob");
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

    // --- A1 normalization / Eq-Hash consistency tests ---

    #[test]
    fn empty_args_app_normalizes_to_name() {
        let raw = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        let n = raw.normalize();
        assert!(matches!(n, SortExpr::Name(ref s) if &**s == "Ob"));
        // Normalization is idempotent.
        assert_eq!(n.clone().normalize(), n);
    }

    #[test]
    fn smart_constructor_collapses_empty_args() {
        let v = SortExpr::app("Ob", Vec::new());
        assert!(matches!(v, SortExpr::Name(_)));
    }

    #[test]
    fn smart_constructor_preserves_nonempty() {
        let v = SortExpr::app("Hom", vec![Term::var("x"), Term::var("y")]);
        assert!(matches!(v, SortExpr::App { .. }));
    }

    #[test]
    fn eq_treats_name_and_empty_app_equal() {
        let a = SortExpr::Name(Arc::from("Ob"));
        let b = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn hash_agrees_with_eq_across_spellings() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let a = SortExpr::Name(Arc::from("Ob"));
        let b = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        let hash = |v: &SortExpr| {
            let mut h = DefaultHasher::new();
            v.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn hashmap_lookup_crosses_spellings() {
        let mut m: FxHashMap<SortExpr, usize> = FxHashMap::default();
        m.insert(SortExpr::Name(Arc::from("Ob")), 1);
        let key = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        assert_eq!(m.get(&key).copied(), Some(1));
    }

    #[test]
    fn subst_produces_normalized_output() {
        let e = SortExpr::App {
            name: Arc::from("S"),
            args: vec![Term::var("x")],
        };
        let mut mapping: FxHashMap<Arc<str>, Term> = FxHashMap::default();
        // Substitute x with a term that reduces args count? No, subst
        // replaces var-by-term; args count stays. Normalization matters
        // most at *construction* time and for the empty-args App case.
        mapping.insert(Arc::from("x"), Term::constant("c"));
        let r = e.subst(&mapping);
        assert!(matches!(r, SortExpr::App { .. }));
    }

    #[test]
    fn rename_head_normalizes_empty_app() {
        let e = SortExpr::App {
            name: Arc::from("Ob"),
            args: Vec::new(),
        };
        let mut sm: std::collections::HashMap<Arc<str>, Arc<str>> =
            std::collections::HashMap::new();
        sm.insert(Arc::from("Ob"), Arc::from("Obj"));
        let r = e.rename_head(&sm);
        assert!(matches!(r, SortExpr::Name(ref n) if &**n == "Obj"));
    }

    #[test]
    fn deserialize_empty_args_app_normalizes() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"name":"Ob","args":[]}"#;
        let v: SortExpr = serde_json::from_str(json)?;
        assert!(matches!(v, SortExpr::Name(ref n) if &**n == "Ob"));
        Ok(())
    }

    // --- positional_param_rename / signatures_equivalent_modulo_param_rename ---

    #[test]
    fn positional_rename_identity_is_empty() {
        let r = positional_param_rename(
            [Arc::from("a"), Arc::from("b")],
            [Arc::from("a"), Arc::from("b")],
        );
        assert!(r.is_empty(), "identity rename should be empty");
    }

    #[test]
    fn positional_rename_maps_differing_names_only() {
        let r = positional_param_rename(
            [Arc::from("a"), Arc::from("y"), Arc::from("c")],
            [Arc::from("x"), Arc::from("y"), Arc::from("z")],
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r.get(&Arc::from("a")), Some(&Term::var("x")));
        assert_eq!(r.get(&Arc::from("c")), Some(&Term::var("z")));
        assert!(!r.contains_key(&Arc::from("y")));
    }

    #[test]
    fn signature_equivalence_accepts_alpha_variant() {
        use crate::op::Implicit;
        // (a : Ob) -> Hom(a, a) vs (x : Ob) -> Hom(x, x)
        let lhs_inputs = vec![(Arc::from("a"), SortExpr::from("Ob"), Implicit::No)];
        let lhs_output = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("a"), Term::var("a")],
        };
        let rhs_inputs = vec![(Arc::from("x"), SortExpr::from("Ob"), Implicit::No)];
        let rhs_output = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        };
        assert!(signatures_equivalent_modulo_param_rename(
            &lhs_inputs,
            &lhs_output,
            &rhs_inputs,
            &rhs_output,
        ));
    }

    #[test]
    fn signature_equivalence_rejects_swap() {
        use crate::op::Implicit;
        // (x, y : Ob) -> Hom(x, y) vs (x, y : Ob) -> Hom(y, x)
        let hom = |a: &str, b: &str| SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var(a), Term::var(b)],
        };
        let lhs_inputs = vec![
            (Arc::from("x"), SortExpr::from("Ob"), Implicit::No),
            (Arc::from("y"), SortExpr::from("Ob"), Implicit::No),
        ];
        let rhs_inputs = lhs_inputs.clone();
        assert!(!signatures_equivalent_modulo_param_rename(
            &lhs_inputs,
            &hom("x", "y"),
            &rhs_inputs,
            &hom("y", "x"),
        ));
    }

    #[test]
    fn signature_equivalence_rejects_arity_mismatch() {
        use crate::op::Implicit;
        let lhs_inputs = vec![(Arc::from("x"), SortExpr::from("Ob"), Implicit::No)];
        let rhs_inputs: Vec<(Arc<str>, SortExpr, Implicit)> = Vec::new();
        assert!(!signatures_equivalent_modulo_param_rename(
            &lhs_inputs,
            &SortExpr::from("Ob"),
            &rhs_inputs,
            &SortExpr::from("Ob"),
        ));
    }

    #[test]
    fn sort_params_rename_alpha_equivalent() {
        // (a : Ob, b : Ob) vs (p : Ob, q : Ob)
        let lhs = vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")];
        let rhs = vec![SortParam::new("p", "Ob"), SortParam::new("q", "Ob")];
        assert!(sort_params_equivalent_modulo_rename(&lhs, &rhs));
    }

    #[test]
    fn refined_value_kinds_are_distinct_and_named() {
        // A record-to-theory encoder must be able to recover a
        // `datetime` / `date` / `time` / `decimal` / `uuid` field as such
        // rather than as a plain string: the refined scalar kinds carry a
        // distinct identity and a canonical name, and appear in `all()` so
        // exhaustive consumers (sample registries, coverage checks) see them.
        for (kind, name) in [
            (ValueKind::DateTime, "date-time"),
            (ValueKind::Date, "date"),
            (ValueKind::Time, "time"),
            (ValueKind::Decimal, "decimal"),
            (ValueKind::Uuid, "uuid"),
        ] {
            assert_ne!(kind, ValueKind::Str, "{kind:?} must not collapse to Str");
            assert_eq!(kind.as_str(), name);
            assert!(
                ValueKind::all().contains(&kind),
                "{kind:?} missing from all()"
            );
        }
    }

    #[test]
    fn sort_params_rename_detects_dependent_difference() {
        // (Γ : Ctx, A : Ty(Γ)) vs (G : Ctx, A : Ty(G)) -- rename should succeed.
        let lhs = vec![
            SortParam::new("Gamma", "Ctx"),
            SortParam::new(
                "A",
                SortExpr::App {
                    name: Arc::from("Ty"),
                    args: vec![Term::var("Gamma")],
                },
            ),
        ];
        let rhs = vec![
            SortParam::new("G", "Ctx"),
            SortParam::new(
                "A",
                SortExpr::App {
                    name: Arc::from("Ty"),
                    args: vec![Term::var("G")],
                },
            ),
        ];
        assert!(sort_params_equivalent_modulo_rename(&lhs, &rhs));
    }

    #[test]
    fn sort_params_rename_rejects_genuine_difference() {
        // (Γ : Ctx, A : Ty(Γ)) vs (G : Ctx, A : Ty(A)) -- second param's
        // inner var refers to itself, not to the first param. Not an alpha
        // variant.
        let lhs = vec![
            SortParam::new("Gamma", "Ctx"),
            SortParam::new(
                "A",
                SortExpr::App {
                    name: Arc::from("Ty"),
                    args: vec![Term::var("Gamma")],
                },
            ),
        ];
        let rhs = vec![
            SortParam::new("G", "Ctx"),
            SortParam::new(
                "A",
                SortExpr::App {
                    name: Arc::from("Ty"),
                    args: vec![Term::var("A")],
                },
            ),
        ];
        assert!(!sort_params_equivalent_modulo_rename(&lhs, &rhs));
    }

    mod property {
        use super::*;
        use proptest::prelude::*;

        fn arb_name() -> impl Strategy<Value = Arc<str>> {
            prop::sample::select(&["S", "T", "Hom", "Tm", "Ob"][..]).prop_map(Arc::from)
        }

        fn arb_term(depth: usize) -> BoxedStrategy<Term> {
            if depth == 0 {
                prop::sample::select(&["x", "y", "z"][..])
                    .prop_map(|s| Term::var(Arc::from(s)))
                    .boxed()
            } else {
                let leaf = prop::sample::select(&["x", "y", "z"][..])
                    .prop_map(|s| Term::var(Arc::from(s)));
                let app = (
                    prop::sample::select(&["f", "g"][..]).prop_map(Arc::from),
                    prop::collection::vec(arb_term(depth - 1), 0..=2),
                )
                    .prop_map(|(op, args)| Term::App { op, args });
                prop_oneof![leaf, app].boxed()
            }
        }

        fn arb_sort_expr() -> BoxedStrategy<SortExpr> {
            prop_oneof![
                arb_name().prop_map(SortExpr::Name),
                (arb_name(), prop::collection::vec(arb_term(1), 0..=3))
                    .prop_map(|(name, args)| SortExpr::app(name, args))
            ]
            .boxed()
        }

        fn arb_subst() -> BoxedStrategy<FxHashMap<Arc<str>, Term>> {
            prop::collection::vec(
                (
                    prop::sample::select(&["x", "y", "z"][..]).prop_map(Arc::from),
                    arb_term(1),
                ),
                0..=3,
            )
            .prop_map(|pairs| {
                let mut m = FxHashMap::default();
                for (k, v) in pairs {
                    m.insert(k, v);
                }
                m
            })
            .boxed()
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn subst_empty_is_identity(e in arb_sort_expr()) {
                let empty = FxHashMap::default();
                prop_assert_eq!(e.subst(&empty), e);
            }

            #[test]
            fn subst_preserves_head(e in arb_sort_expr(), sigma in arb_subst()) {
                let after = e.subst(&sigma);
                prop_assert_eq!(e.head(), after.head());
            }

            #[test]
            fn normalization_is_idempotent(e in arb_sort_expr()) {
                let n1 = e.normalize();
                let n2 = n1.clone().normalize();
                prop_assert_eq!(n1, n2);
            }

            #[test]
            fn sig_equivalence_is_reflexive(
                raw_inputs in prop::collection::vec(
                    (prop::sample::select(&["x", "y", "z"][..]).prop_map(Arc::from), arb_sort_expr()),
                    0..=3,
                ),
                output in arb_sort_expr(),
            ) {
                let inputs: Vec<(Arc<str>, SortExpr, crate::op::Implicit)> = raw_inputs
                    .into_iter()
                    .map(|(n, s)| (n, s, crate::op::Implicit::No))
                    .collect();
                prop_assert!(signatures_equivalent_modulo_param_rename(
                    &inputs, &output, &inputs, &output,
                ));
            }

            #[test]
            fn sig_equivalence_under_alpha_rename(
                sort_name in arb_name(),
                first in prop::sample::select(&["x", "y", "z"][..]).prop_map(Arc::from),
                replacement in prop::sample::select(&["p", "q", "r"][..]).prop_map(Arc::from),
            ) {
                // Build signature `(first : sort_name) -> App { sort_name, [first] }`
                // and its alpha variant `(replacement : sort_name) -> App { sort_name, [replacement] }`.
                // Both should be signature-equivalent under the positional rename.
                let lhs_inputs: Vec<(Arc<str>, SortExpr, crate::op::Implicit)> = vec![(
                    Arc::clone(&first),
                    SortExpr::Name(Arc::clone(&sort_name)),
                    crate::op::Implicit::No,
                )];
                let lhs_output = SortExpr::App {
                    name: Arc::clone(&sort_name),
                    args: vec![Term::Var(Arc::clone(&first))],
                };
                let rhs_inputs: Vec<(Arc<str>, SortExpr, crate::op::Implicit)> = vec![(
                    Arc::clone(&replacement),
                    SortExpr::Name(Arc::clone(&sort_name)),
                    crate::op::Implicit::No,
                )];
                let rhs_output = SortExpr::App {
                    name: sort_name,
                    args: vec![Term::Var(replacement)],
                };
                prop_assert!(signatures_equivalent_modulo_param_rename(
                    &lhs_inputs, &lhs_output, &rhs_inputs, &rhs_output,
                ));
            }

            #[test]
            fn name_and_empty_app_hash_equal(name in arb_name()) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let a = SortExpr::Name(Arc::clone(&name));
                let b = SortExpr::App { name, args: Vec::new() };
                let mut ha = DefaultHasher::new();
                a.hash(&mut ha);
                let mut hb = DefaultHasher::new();
                b.hash(&mut hb);
                prop_assert_eq!(ha.finish(), hb.finish());
                prop_assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn coercion_class_serde_wire_format_is_pascal_case() {
        // `CoercionClass` uses the default serde representation (no
        // `rename_all` attribute). The TypeScript `CoercionClass` and
        // Python string decoders MUST match this exactly; if anyone ever
        // adds `#[serde(rename_all = ...)]` to the enum, every SDK
        // consumer would silently start seeing a different string and
        // drop the field. This test locks the wire format.
        let cases = [
            (CoercionClass::Iso, "\"Iso\""),
            (CoercionClass::Retraction, "\"Retraction\""),
            (CoercionClass::Projection, "\"Projection\""),
            (CoercionClass::Opaque, "\"Opaque\""),
        ];
        for (value, expected) in cases {
            match serde_json::to_string(&value) {
                Ok(s) => assert_eq!(s, expected, "unexpected wire format"),
                Err(e) => panic!("serde failed to serialize a plain enum: {e}"),
            }
        }
    }

    #[test]
    fn alpha_eq_modulo_rewrites_status_reports_exhaustion() {
        // Non-terminating rule f(x) -> f(f(x)).
        let rule = crate::eq::DirectedEquation::new(
            "expand",
            Term::app("f", vec![Term::var("x")]),
            Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
            panproto_expr::Expr::Var("_".into()),
        );
        // Two dependent sorts Tm(f(a)) and Tm(b): same head and arity, but the
        // first argument loops under the rule, so equality cannot be decided.
        let s1 = SortExpr::App {
            name: "Tm".into(),
            args: vec![Term::app("f", vec![Term::constant("a")])],
        };
        let s2 = SortExpr::App {
            name: "Tm".into(),
            args: vec![Term::constant("b")],
        };
        let (equal, exhausted) = s1.alpha_eq_modulo_rewrites_status(&s2, &[rule], 5);
        assert!(!equal);
        assert!(exhausted, "looping argument normalization must be flagged");
    }

    #[test]
    fn alpha_eq_modulo_rewrites_status_terminating_not_exhausted() {
        let rule = crate::eq::DirectedEquation::new(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
            panproto_expr::Expr::Var("_".into()),
        );
        let s1 = SortExpr::App {
            name: "Tm".into(),
            args: vec![Term::app(
                "add",
                vec![Term::constant("zero"), Term::constant("a")],
            )],
        };
        let s2 = SortExpr::App {
            name: "Tm".into(),
            args: vec![Term::constant("a")],
        };
        let (equal, exhausted) = s1.alpha_eq_modulo_rewrites_status(&s2, &[rule], 100);
        assert!(equal, "add(zero, a) normalizes to a");
        assert!(!exhausted);
    }
}
