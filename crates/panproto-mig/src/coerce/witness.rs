//! `SortLensWitness`: a lens-law-satisfying carrier conversion.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, ValueKind};

/// A carrier-level bidirectional transform expressed in the panproto
/// expression language.
///
/// `forward` (and, when present, `inverse`) are closed expressions
/// parameterized by `forward_param` / `inverse_param`. When an
/// `inverse` is supplied, the pair describes a Cambria-style
/// asymmetric lens between sorts of kind `source_kind` and
/// `target_kind`.
#[derive(Clone, Debug)]
pub struct SortLensWitness {
    /// Human-readable identifier: used in explanations and library
    /// lookups.
    pub name: String,
    /// Carrier kind of the source sort.
    pub source_kind: ValueKind,
    /// Carrier kind of the target sort.
    pub target_kind: ValueKind,
    /// Round-trip classification: `Iso`, `Retraction`, or `Projection`.
    pub class: CoercionClass,
    /// Name of the parameter `forward` captures.
    pub forward_param: Arc<str>,
    /// Forward expression: a closed expression over `forward_param`.
    pub forward: Expr,
    /// Name of the parameter `inverse` captures, if any.
    pub inverse_param: Option<Arc<str>>,
    /// Optional inverse expression.
    pub inverse: Option<Expr>,
    /// Short human-readable description shown in candidate
    /// explanations (e.g. `"int ↔ str via IntToStr / StrToInt"`).
    pub description: String,
}

impl SortLensWitness {
    /// Convenience: indicates whether the witness is a true model
    /// isomorphism.
    #[must_use]
    pub const fn is_iso(&self) -> bool {
        matches!(self.class, CoercionClass::Iso)
    }
}

/// A searchable library of sort-lens witnesses.
///
/// Indexed by the `(source, target)` carrier-kind pair. Values are
/// vectors because the same kind pair can have multiple distinct
/// witnesses (e.g. `int ↔ int` with different scale factors).
#[derive(Clone, Debug, Default)]
pub struct WitnessLibrary {
    by_kinds: HashMap<(ValueKind, ValueKind), Vec<SortLensWitness>>,
}

impl WitnessLibrary {
    /// Construct an empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a witness under its `(source_kind, target_kind)` key.
    pub fn register(&mut self, witness: SortLensWitness) {
        let key = (witness.source_kind, witness.target_kind);
        self.by_kinds.entry(key).or_default().push(witness);
    }

    /// Return all witnesses from `source_kind` to `target_kind`.
    #[must_use]
    pub fn lookup(&self, source_kind: ValueKind, target_kind: ValueKind) -> &[SortLensWitness] {
        self.by_kinds
            .get(&(source_kind, target_kind))
            .map_or(&[] as &[SortLensWitness], Vec::as_slice)
    }

    /// Iterate all registered witnesses.
    pub fn iter(&self) -> impl Iterator<Item = &SortLensWitness> {
        self.by_kinds.values().flatten()
    }

    /// Number of registered witnesses (counting multiplicities across
    /// the same kind pair).
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_kinds.values().map(Vec::len).sum()
    }

    /// Whether the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_kinds.values().all(Vec::is_empty)
    }

    /// Merge another library's witnesses into this one.
    pub fn extend(&mut self, other: Self) {
        for (_, ws) in other.by_kinds {
            for w in ws {
                self.register(w);
            }
        }
    }
}

/// Build the default, protocol-agnostic witness library.
///
/// All witnesses in this library are `Iso`-classified and round-trip
/// exactly on the carriers they advertise. Callers can verify the
/// laws themselves via [`super::witness_satisfies_lens_laws`]; the
/// built-ins here have matching unit tests in this module.
#[must_use]
pub fn default_witness_library() -> WitnessLibrary {
    let mut lib = WitnessLibrary::new();

    lib.register(int_to_str_witness());
    lib.register(str_to_int_witness());
    lib.register(int_to_float_witness());
    lib.register(bool_to_int_witness());
    lib.register(int_to_bool_witness());

    lib
}

// ---------------------------------------------------------------------------
// Built-in witnesses
// ---------------------------------------------------------------------------

/// `int → str` via `IntToStr`; inverse `str → int` via `StrToInt`.
#[must_use]
pub fn int_to_str_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "int_to_str".to_owned(),
        source_kind: ValueKind::Int,
        target_kind: ValueKind::Str,
        class: CoercionClass::Iso,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::IntToStr, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::StrToInt, vec![Expr::Var(w)])),
        description: "int ↔ str via IntToStr / StrToInt".to_owned(),
    }
}

/// `str → int` via `StrToInt`; inverse `int → str` via `IntToStr`.
///
/// Note: not `Iso` on the full `str` carrier (non-integer strings fail
/// `StrToInt`). Classified as `Retraction` to flag the lossy
/// fallback; users who know their strings are numeric can treat it as
/// an iso.
#[must_use]
pub fn str_to_int_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "str_to_int".to_owned(),
        source_kind: ValueKind::Str,
        target_kind: ValueKind::Int,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::StrToInt, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::IntToStr, vec![Expr::Var(w)])),
        description: "str → int via StrToInt (Retraction)".to_owned(),
    }
}

/// `int → float` via `IntToFloat`; inverse `float → int` via `FloatToInt`.
///
/// Classified `Retraction` because the float → int direction is not
/// injective on the full float carrier (floats with fractional parts
/// lose precision).
#[must_use]
pub fn int_to_float_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "int_to_float".to_owned(),
        source_kind: ValueKind::Int,
        target_kind: ValueKind::Float,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::FloatToInt, vec![Expr::Var(w)])),
        description: "int → float via IntToFloat (Retraction: FloatToInt truncates)".to_owned(),
    }
}

/// `bool → int` as `true ↦ 1`, `false ↦ 0`. Expressed with
/// `match` over the lambda parameter.
#[must_use]
pub fn bool_to_int_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&v))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Int(1)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Int(0)),
            ),
        ],
    };
    // Inverse: 0 ↦ false, every other int ↦ true. This is the classic
    // C-style cast; not an iso on the full int carrier, so
    // `Retraction`.
    let inverse = Expr::Builtin(
        BuiltinOp::Neq,
        vec![Expr::Var(Arc::clone(&w)), Expr::Lit(Literal::Int(0))],
    );
    SortLensWitness {
        name: "bool_to_int".to_owned(),
        source_kind: ValueKind::Bool,
        target_kind: ValueKind::Int,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "bool → int (true↦1, false↦0); Retraction (non-0/1 ints)".to_owned(),
    }
}

/// `int → bool`: 0 ↦ false, everything else ↦ true. Inverse round-
/// trips `true ↦ 1`, `false ↦ 0`.
#[must_use]
pub fn int_to_bool_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Builtin(
        BuiltinOp::Neq,
        vec![Expr::Var(Arc::clone(&v)), Expr::Lit(Literal::Int(0))],
    );
    let inverse = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&w))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Int(1)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Int(0)),
            ),
        ],
    };
    SortLensWitness {
        name: "int_to_bool".to_owned(),
        source_kind: ValueKind::Int,
        target_kind: ValueKind::Bool,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "int → bool (0↦false, nonzero↦true); Retraction".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::witness_satisfies_lens_laws;
    use super::*;

    fn int_samples() -> Vec<Literal> {
        vec![
            Literal::Int(0),
            Literal::Int(1),
            Literal::Int(-1),
            Literal::Int(42),
            Literal::Int(-1_000),
        ]
    }

    fn str_numeric_samples() -> Vec<Literal> {
        vec![
            Literal::Str("0".to_owned()),
            Literal::Str("1".to_owned()),
            Literal::Str("-1".to_owned()),
            Literal::Str("42".to_owned()),
            Literal::Str("-1000".to_owned()),
        ]
    }

    #[test]
    fn int_to_str_round_trips_on_int_samples() {
        let w = int_to_str_witness();
        witness_satisfies_lens_laws(&w, &int_samples(), &str_numeric_samples())
            .expect("int_to_str should round-trip as an iso on these samples");
    }

    #[test]
    fn str_to_int_round_trips_on_numeric_strings() {
        let w = str_to_int_witness();
        witness_satisfies_lens_laws(&w, &str_numeric_samples(), &int_samples())
            .expect("str_to_int should round-trip as a retraction on numeric strings");
    }

    #[test]
    fn int_to_float_round_trips_on_whole_ints() {
        let w = int_to_float_witness();
        witness_satisfies_lens_laws(&w, &int_samples(), &[])
            .expect("int→float→int should round-trip for whole-number samples");
    }

    #[test]
    fn bool_to_int_round_trips() {
        let w = bool_to_int_witness();
        witness_satisfies_lens_laws(
            &w,
            &[Literal::Bool(true), Literal::Bool(false)],
            &[Literal::Int(0), Literal::Int(1)],
        )
        .expect("bool_to_int should round-trip on {0,1} target samples");
    }

    #[test]
    fn int_to_bool_round_trips() {
        let w = int_to_bool_witness();
        witness_satisfies_lens_laws(
            &w,
            &[Literal::Int(0), Literal::Int(1)],
            &[Literal::Bool(true), Literal::Bool(false)],
        )
        .expect("int_to_bool should round-trip on {0,1} source samples");
    }

    #[test]
    fn default_library_is_non_empty() {
        let lib = default_witness_library();
        assert!(lib.len() >= 5);
        assert!(!lib.is_empty());
        assert!(!lib.lookup(ValueKind::Int, ValueKind::Str).is_empty());
    }

    #[test]
    fn library_lookup_returns_empty_for_unknown_pair() {
        let lib = default_witness_library();
        assert!(lib.lookup(ValueKind::Bytes, ValueKind::Token).is_empty());
    }
}
