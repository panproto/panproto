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

/// Stable ordinal for [`ValueKind`], used to sort `(src, tgt)` pairs in
/// the witness library deterministically without relying on `Debug`
/// output. Adding a new [`ValueKind`] without an entry here is a
/// compile-time error thanks to the exhaustive match.
const fn value_kind_ordinal(kind: ValueKind) -> u8 {
    match kind {
        ValueKind::Null => 0,
        ValueKind::Bool => 1,
        ValueKind::Int => 2,
        ValueKind::Float => 3,
        ValueKind::Str => 4,
        ValueKind::Bytes => 5,
        ValueKind::Token => 6,
        ValueKind::Any => 7,
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
    ///
    /// Witnesses are appended in insertion order. [`Self::lookup`]
    /// returns this order verbatim, so callers that prefer Iso over
    /// Retraction (for example) should register Iso witnesses first.
    pub fn register(&mut self, witness: SortLensWitness) {
        let key = (witness.source_kind, witness.target_kind);
        self.by_kinds.entry(key).or_default().push(witness);
    }

    /// Return all witnesses from `source_kind` to `target_kind`.
    ///
    /// Order is insertion order: the first registered witness appears
    /// first. Callers that want "best" witnesses should rank the slice
    /// themselves (e.g. by [`SortLensWitness::class`]).
    #[must_use]
    pub fn lookup(&self, source_kind: ValueKind, target_kind: ValueKind) -> &[SortLensWitness] {
        self.by_kinds
            .get(&(source_kind, target_kind))
            .map_or(&[] as &[SortLensWitness], Vec::as_slice)
    }

    /// Iterate all registered witnesses in a deterministic order.
    ///
    /// Order is: by `(source_kind, target_kind)` ascending using an
    /// explicit ordinal for [`ValueKind`], then by insertion order
    /// within each `(source, target)` bucket. Making this deterministic
    /// lets explanations and diagnostics reproduce across runs even
    /// though the backing storage is a `HashMap`.
    pub fn iter(&self) -> impl Iterator<Item = &SortLensWitness> {
        let mut keys: Vec<&(ValueKind, ValueKind)> = self.by_kinds.keys().collect();
        // A table-driven ordinal is stable against `Debug` impl edits
        // upstream in `panproto-gat`, avoids an allocation per comparison,
        // and makes the ordering explicit.
        keys.sort_by_key(|k| (value_kind_ordinal(k.0), value_kind_ordinal(k.1)));
        keys.into_iter().flat_map(move |k| self.by_kinds[k].iter())
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
    ///
    /// Witnesses from `other` are drained in the same
    /// deterministic order as [`Self::iter`] (sorted by
    /// `(source_kind, target_kind)` ordinal, then insertion order
    /// within each bucket), so the merged library is a function of the
    /// operands' contents rather than the backing `HashMap`'s
    /// iteration order.
    pub fn extend(&mut self, other: Self) {
        // Collect keys in the canonical order before draining to avoid
        // reading and mutating `other.by_kinds` in the same iteration.
        let mut keys: Vec<(ValueKind, ValueKind)> = other.by_kinds.keys().copied().collect();
        keys.sort_by_key(|k| (value_kind_ordinal(k.0), value_kind_ordinal(k.1)));
        let mut by_kinds = other.by_kinds;
        for k in keys {
            if let Some(ws) = by_kinds.remove(&k) {
                for w in ws {
                    self.register(w);
                }
            }
        }
    }

    /// Look up a registered witness by its `name`.
    ///
    /// Returns the first registered witness whose `name` matches
    /// exactly. Witness names are expected to be unique within a
    /// library; callers that rely on this invariant should verify it
    /// via [`Self::witness_names_are_unique`].
    #[must_use]
    pub fn witness_by_name(&self, name: &str) -> Option<&SortLensWitness> {
        self.iter().find(|w| w.name == name)
    }

    /// Check that every registered witness has a distinct `name`.
    ///
    /// Returns `Ok(())` when all names are unique; otherwise returns
    /// the offending duplicate name. Useful as an assertion after
    /// building a library from multiple cartridges.
    ///
    /// # Errors
    ///
    /// Returns the first duplicate name encountered.
    pub fn witness_names_are_unique(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for w in self.iter() {
            if !seen.insert(w.name.as_str()) {
                return Err(w.name.clone());
            }
        }
        Ok(())
    }
}

/// Build the default, protocol-agnostic witness library.
///
/// Every built-in witness satisfies `GetPut` on its full source carrier.
/// All of them are classified [`CoercionClass::Retraction`] because
/// the reverse direction either rejects off-domain targets
/// (`StrToInt` on non-numeric strings, `FloatToInt` on fractional
/// floats) or collapses multiple target values onto the same source
/// (`Neq 0` on ints with `|v| > 1`). Callers can verify the laws via
/// [`super::witness_satisfies_lens_laws`] for `GetPut` and
/// [`super::witness_forward_fails_on`] to positively exhibit an
/// off-domain target.
#[must_use]
pub fn default_witness_library() -> WitnessLibrary {
    let mut lib = WitnessLibrary::new();

    // Int ↔ Str
    lib.register(int_to_str_witness());
    lib.register(str_to_int_witness());

    // Int ↔ Float
    lib.register(int_to_float_witness());
    lib.register(float_to_int_witness());

    // Float ↔ Str
    lib.register(float_to_str_witness());
    lib.register(str_to_float_witness());

    // Bool ↔ Int
    lib.register(bool_to_int_witness());
    lib.register(int_to_bool_witness());

    // Bool ↔ Str  (canonical "true"/"false")
    lib.register(bool_to_str_witness());
    lib.register(str_to_bool_witness());

    // Bool ↔ Float
    lib.register(bool_to_float_witness());
    lib.register(float_to_bool_witness());

    lib
}

// ---------------------------------------------------------------------------
// Built-in witnesses
// ---------------------------------------------------------------------------

/// `int → str` via `IntToStr`; inverse `str → int` via `StrToInt`.
///
/// Classified as [`CoercionClass::Retraction`]: `inverse(forward(v)) = v`
/// holds for every integer (the `GetPut` direction is exact), but the
/// `PutGet` direction fails on the full `str` carrier because
/// non-numeric strings (`""`, `"abc"`, `"1.5"`, `" 3"`) cause
/// `StrToInt` to error. The witness is an iso only on the sub-carrier
/// of canonical decimal integer strings.
#[must_use]
pub fn int_to_str_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "int_to_str".to_owned(),
        source_kind: ValueKind::Int,
        target_kind: ValueKind::Str,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::IntToStr, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::StrToInt, vec![Expr::Var(w)])),
        description: "int → str via IntToStr (Retraction: StrToInt fails on non-numeric strings)"
            .to_owned(),
    }
}

/// `str → int` via `StrToInt`; inverse `int → str` via `IntToStr`.
///
/// Classified [`CoercionClass::Retraction`].
///
/// **Domain restriction.** The forward direction is partial: it only
/// accepts canonical decimal integer strings (optional leading `-`,
/// then digits). `""`, `"abc"`, `"1.5"`, `" 3"`, and `"0x10"` all
/// cause `StrToInt` to return an error. Callers feeding the witness
/// must pre-validate their strings, or accept that migration will
/// fail on malformed input.
///
/// **Lens-law scope.** [`super::witness_satisfies_lens_laws`] verifies
/// `GetPut` on the samples you provide. Pass only in-domain strings
/// (e.g. `"0"`, `"-1"`, `"42"`) to assert the law on the stated
/// domain. The checker will report the forward-direction error if you
/// pass an off-domain sample, which is the correct behaviour for a
/// `Retraction`: the law holds on-domain, and off-domain evidence is
/// surfaced as an error rather than silently passed.
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
/// Classified [`CoercionClass::Retraction`] because `FloatToInt`
/// truncates on the full `f64` carrier: floats with fractional parts
/// (e.g. `1.5`) and floats whose magnitude exceeds `i64::MAX` do not
/// round-trip.
///
/// **When it is actually an iso.** For integers `v` with `|v| < 2^53`,
/// `IntToFloat(v)` is exact and `FloatToInt(IntToFloat(v)) = v`. The
/// witness is therefore an iso on the sub-carrier `{v : |v| < 2^53}`.
/// We intentionally classify the witness as `Retraction` rather than
/// providing a separate `int_to_float_iso` variant: the CSP-level
/// decision to admit a lossy migration should be based on the
/// `Retraction` confidence floor, not on a domain-restricted iso that
/// the caller has no reliable way to verify at schema time. Documented
/// here so callers who know their integer magnitudes fit can treat the
/// witness as invertible.
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

// ---------------------------------------------------------------------------
// Numeric ↔ numeric additional witnesses
// ---------------------------------------------------------------------------

/// `float → int` via `FloatToInt` (truncation); inverse `int → float`
/// via `IntToFloat`.
///
/// Classified [`CoercionClass::Retraction`] because `FloatToInt` is
/// lossy on floats with fractional parts (`1.5 ↦ 1`, `IntToFloat(1) =
/// 1.0 ≠ 1.5`). The witness is an iso on the sub-carrier of
/// whole-number floats `{v : v == v.trunc() ∧ |v| < 2^53}`.
#[must_use]
pub fn float_to_int_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "float_to_int".to_owned(),
        source_kind: ValueKind::Float,
        target_kind: ValueKind::Int,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::FloatToInt, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::Var(w)])),
        description: "float → int via FloatToInt (Retraction: fractional parts truncate)"
            .to_owned(),
    }
}

/// `float → str` via `FloatToStr`; inverse `str → float` via `StrToFloat`.
///
/// Classified [`CoercionClass::Retraction`]: non-numeric strings (and
/// non-canonical numeric strings that `StrToFloat` rejects) fail the
/// reverse direction. The witness round-trips on every float the
/// default formatter can re-parse (the shortest-canonical printer used
/// by `FloatToStr` is designed to be `StrToFloat`-reversible).
#[must_use]
pub fn float_to_str_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "float_to_str".to_owned(),
        source_kind: ValueKind::Float,
        target_kind: ValueKind::Str,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::FloatToStr, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::StrToFloat, vec![Expr::Var(w)])),
        description: "float → str via FloatToStr (Retraction: non-numeric strings fail)".to_owned(),
    }
}

/// `str → float` via `StrToFloat`; inverse `float → str` via `FloatToStr`.
///
/// Classified [`CoercionClass::Retraction`]. Forward direction is
/// partial (rejects non-numeric strings). Reverse direction formats
/// the float with the default printer, which may not match the
/// original string bit-for-bit (e.g. `"1.00"` → `1.0` → `"1"`). An iso
/// only on the sub-carrier of canonical float printings.
#[must_use]
pub fn str_to_float_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    SortLensWitness {
        name: "str_to_float".to_owned(),
        source_kind: ValueKind::Str,
        target_kind: ValueKind::Float,
        class: CoercionClass::Retraction,
        forward_param: Arc::clone(&v),
        forward: Expr::Builtin(BuiltinOp::StrToFloat, vec![Expr::Var(Arc::clone(&v))]),
        inverse_param: Some(Arc::clone(&w)),
        inverse: Some(Expr::Builtin(BuiltinOp::FloatToStr, vec![Expr::Var(w)])),
        description: "str → float via StrToFloat (Retraction: not all strings are canonical)"
            .to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Bool ↔ Str / Float witnesses
// ---------------------------------------------------------------------------

/// `bool → str` as `true ↦ "true"`, `false ↦ "false"`.
///
/// Classified [`CoercionClass::Retraction`]. Forward is total on the
/// `bool` carrier. Inverse recognizes exactly the canonical
/// `"true"`/`"false"` strings; any other string fails to evaluate.
/// Iso on the `{"true", "false"}` sub-carrier.
#[must_use]
pub fn bool_to_str_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&v))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Str("true".to_owned())),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Str("false".to_owned())),
            ),
        ],
    };
    let inverse = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&w))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Str("true".to_owned())),
                Expr::Lit(Literal::Bool(true)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Str("false".to_owned())),
                Expr::Lit(Literal::Bool(false)),
            ),
        ],
    };
    SortLensWitness {
        name: "bool_to_str".to_owned(),
        source_kind: ValueKind::Bool,
        target_kind: ValueKind::Str,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "bool → str (true↦\"true\", false↦\"false\"); Retraction on arbitrary strings"
            .to_owned(),
    }
}

/// `str → bool` accepts only canonical `"true"` / `"false"`.
///
/// Classified [`CoercionClass::Retraction`]. Forward is partial: any
/// string other than `"true"` or `"false"` causes the match to fail to
/// evaluate. Inverse is the canonical printer from [`bool_to_str_witness`].
/// Iso on the `{"true", "false"}` sub-carrier.
#[must_use]
pub fn str_to_bool_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&v))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Str("true".to_owned())),
                Expr::Lit(Literal::Bool(true)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Str("false".to_owned())),
                Expr::Lit(Literal::Bool(false)),
            ),
        ],
    };
    let inverse = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&w))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Str("true".to_owned())),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Str("false".to_owned())),
            ),
        ],
    };
    SortLensWitness {
        name: "str_to_bool".to_owned(),
        source_kind: ValueKind::Str,
        target_kind: ValueKind::Bool,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "str → bool (accepts \"true\"/\"false\" only); Retraction".to_owned(),
    }
}

/// `bool → float` as `true ↦ 1.0`, `false ↦ 0.0`.
///
/// Classified [`CoercionClass::Retraction`]. Inverse tests `≠ 0.0`, so
/// every float collapses to one of two booleans; the round-trip is
/// exact only on `{0.0, 1.0}`.
#[must_use]
pub fn bool_to_float_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&v))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Float(1.0)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Float(0.0)),
            ),
        ],
    };
    let inverse = Expr::Builtin(
        BuiltinOp::Neq,
        vec![Expr::Var(Arc::clone(&w)), Expr::Lit(Literal::Float(0.0))],
    );
    SortLensWitness {
        name: "bool_to_float".to_owned(),
        source_kind: ValueKind::Bool,
        target_kind: ValueKind::Float,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "bool → float (true↦1.0, false↦0.0); Retraction (non-0/1 floats)".to_owned(),
    }
}

/// `float → bool`: `0.0 ↦ false`, everything else ↦ `true`.
///
/// Classified [`CoercionClass::Retraction`]. Inverse round-trips
/// `true ↦ 1.0`, `false ↦ 0.0`, so the round-trip is exact on
/// `{0.0, 1.0}` only.
///
/// **NaN handling.** `Neq(NaN, 0.0)` evaluates to `true` under
/// IEEE-754 semantics (NaN is not equal to anything, including
/// itself), so `forward(NaN) = true` and `inverse(true) = 1.0`. NaN
/// inputs therefore also do not round-trip.
#[must_use]
pub fn float_to_bool_witness() -> SortLensWitness {
    let v: Arc<str> = Arc::from("v");
    let w: Arc<str> = Arc::from("w");
    let forward = Expr::Builtin(
        BuiltinOp::Neq,
        vec![Expr::Var(Arc::clone(&v)), Expr::Lit(Literal::Float(0.0))],
    );
    let inverse = Expr::Match {
        scrutinee: Box::new(Expr::Var(Arc::clone(&w))),
        arms: vec![
            (
                panproto_expr::Pattern::Lit(Literal::Bool(true)),
                Expr::Lit(Literal::Float(1.0)),
            ),
            (
                panproto_expr::Pattern::Lit(Literal::Bool(false)),
                Expr::Lit(Literal::Float(0.0)),
            ),
        ],
    };
    SortLensWitness {
        name: "float_to_bool".to_owned(),
        source_kind: ValueKind::Float,
        target_kind: ValueKind::Bool,
        class: CoercionClass::Retraction,
        forward_param: v,
        forward,
        inverse_param: Some(w),
        inverse: Some(inverse),
        description: "float → bool (0.0↦false, nonzero↦true); Retraction".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::{witness_forward_fails_on, witness_satisfies_lens_laws};
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
    fn int_to_str_get_put_on_int_samples() {
        // int_to_str is now classified Retraction, so `GetPut` holds on
        // all ints; target_samples are ignored by the checker for
        // non-Iso classes.
        let w = int_to_str_witness();
        witness_satisfies_lens_laws(&w, &int_samples(), &[])
            .expect("int_to_str `GetPut` should hold on every int");
    }

    #[test]
    fn int_to_str_fails_on_off_domain_string() {
        // A non-numeric string (off the canonical decimal sub-carrier)
        // must not round-trip: StrToInt errors, so forward(inverse(t))
        // fails to evaluate — which witness_forward_fails_on treats as
        // confirmation of the Retraction classification.
        let w = int_to_str_witness();
        witness_forward_fails_on(&w, &Literal::Str("abc".to_owned()))
            .expect("non-numeric strings should not round-trip through int_to_str");
        witness_forward_fails_on(&w, &Literal::Str(String::new()))
            .expect("empty string should not round-trip through int_to_str");
    }

    #[test]
    fn str_to_int_round_trips_on_numeric_strings() {
        let w = str_to_int_witness();
        witness_satisfies_lens_laws(&w, &str_numeric_samples(), &[])
            .expect("str_to_int should satisfy `GetPut` on numeric strings");
    }

    #[test]
    fn str_to_int_off_domain_target_does_not_round_trip() {
        // Any int maps back to a canonical decimal string; a
        // non-canonical str ("0x10") is off-domain for the reverse
        // direction (its inverse, IntToStr, would yield the canonical
        // form, which is different).
        let w = str_to_int_witness();
        // Forward errors on "0x10" → the helper treats this as
        // acceptable evidence of non-iso.
        witness_forward_fails_on(&w, &Literal::Int(10))
            .or_else(|_| witness_forward_fails_on(&w, &Literal::Str("0x10".to_owned())))
            .expect("str_to_int should not be a full iso");
    }

    #[test]
    fn int_to_float_round_trips_on_whole_ints() {
        let w = int_to_float_witness();
        witness_satisfies_lens_laws(&w, &int_samples(), &[])
            .expect("int→float→int should round-trip for whole-number samples");
    }

    #[test]
    fn int_to_float_precision_boundary_at_2_pow_53() {
        // Pins the sharpness of the Retraction classification at the
        // `|v| < 2^53` boundary. 2^53 itself still round-trips exactly
        // through f64 (the mantissa is 53 bits, so 2^53 is representable
        // with a trailing zero). 2^53 + 1 is NOT representable and
        // rounds to 2^53, so the round-trip collapses to 2^53.
        let w = int_to_float_witness();
        let boundary: i64 = 1_i64 << 53;
        // At the boundary: GetPut holds.
        witness_satisfies_lens_laws(&w, &[Literal::Int(boundary)], &[])
            .expect("int→float→int should round-trip at exactly 2^53");
        // One past the boundary: GetPut fails because 2^53 + 1 rounds
        // to 2^53 during IntToFloat, then FloatToInt recovers 2^53 ≠
        // 2^53 + 1.
        let err = witness_satisfies_lens_laws(&w, &[Literal::Int(boundary + 1)], &[])
            .expect_err("int→float→int must NOT round-trip at 2^53 + 1");
        assert!(
            err.contains("GetPut"),
            "precision-loss violation must be reported as GetPut: {err}"
        );
    }

    #[test]
    fn int_to_float_fails_on_fractional_target() {
        let w = int_to_float_witness();
        witness_forward_fails_on(&w, &Literal::Float(1.5))
            .expect("fractional floats must not round-trip through int_to_float");
    }

    #[test]
    fn bool_to_int_round_trips_on_zero_one() {
        // Retraction on the full int carrier; iso on the {0,1} sub-
        // carrier. `GetPut` holds on {true,false}.
        let w = bool_to_int_witness();
        witness_satisfies_lens_laws(&w, &[Literal::Bool(true), Literal::Bool(false)], &[])
            .expect("bool_to_int `GetPut` should hold on {true,false}");
    }

    #[test]
    fn bool_to_int_fails_on_off_domain_int() {
        // Target carrier contains ints outside {0,1}. Round-trip on
        // Int(2): inverse maps 2 ↦ true (Neq 0), forward maps true ↦ 1,
        // which is not 2.
        let w = bool_to_int_witness();
        witness_forward_fails_on(&w, &Literal::Int(2))
            .expect("Int(2) should not round-trip through bool_to_int");
        witness_forward_fails_on(&w, &Literal::Int(-1))
            .expect("Int(-1) should not round-trip through bool_to_int");
    }

    #[test]
    fn int_to_bool_round_trips_on_zero_one() {
        let w = int_to_bool_witness();
        witness_satisfies_lens_laws(&w, &[Literal::Int(0), Literal::Int(1)], &[])
            .expect("int_to_bool `GetPut` should hold on {0,1}");
    }

    #[test]
    fn int_to_bool_fails_get_put_on_off_domain_source() {
        // `GetPut` on Int(5): forward → true, inverse → 1, which is not
        // 5. The checker should report a `GetPut` violation.
        let w = int_to_bool_witness();
        let err = witness_satisfies_lens_laws(&w, &[Literal::Int(5)], &[]).unwrap_err();
        assert!(
            err.contains("GetPut violation"),
            "expected GetPut violation for off-domain Int(5); got: {err}"
        );
    }

    #[test]
    fn default_library_is_non_empty() {
        let lib = default_witness_library();
        assert!(lib.len() >= 12);
        assert!(!lib.is_empty());
        assert!(!lib.lookup(ValueKind::Int, ValueKind::Str).is_empty());
    }

    fn float_whole_samples() -> Vec<Literal> {
        vec![
            Literal::Float(0.0),
            Literal::Float(1.0),
            Literal::Float(-1.0),
            Literal::Float(42.0),
            Literal::Float(-1000.0),
        ]
    }

    #[test]
    fn float_to_int_round_trips_on_whole_floats() {
        let w = float_to_int_witness();
        witness_satisfies_lens_laws(&w, &float_whole_samples(), &[])
            .expect("float_to_int GetPut should hold on whole-number floats");
    }

    #[test]
    fn float_to_int_fails_on_fractional_source() {
        let w = float_to_int_witness();
        // Forward truncates 1.5 → 1, then inverse(1) = 1.0 ≠ 1.5, so
        // `witness_satisfies_lens_laws` must report the GetPut violation.
        let err = witness_satisfies_lens_laws(&w, &[Literal::Float(1.5)], &[])
            .expect_err("fractional floats must not round-trip through float_to_int");
        assert!(
            err.contains("GetPut"),
            "error should identify GetPut: {err}"
        );
    }

    #[test]
    fn float_to_str_round_trips_on_whole_floats() {
        let w = float_to_str_witness();
        witness_satisfies_lens_laws(&w, &float_whole_samples(), &[]).expect(
            "float_to_str GetPut should hold on whole floats (shortest-canonical printing)",
        );
    }

    #[test]
    fn float_to_str_fails_on_off_domain_string() {
        let w = float_to_str_witness();
        witness_forward_fails_on(&w, &Literal::Str("not-a-number".to_owned()))
            .expect("non-numeric strings should not round-trip through float_to_str");
    }

    #[test]
    fn str_to_float_round_trips_on_canonical_numeric_strings() {
        let w = str_to_float_witness();
        // Canonical shortest printings only — "1.00" would not round-trip
        // through FloatToStr because the printer emits "1".
        let samples = vec![
            Literal::Str("0".to_owned()),
            Literal::Str("1".to_owned()),
            Literal::Str("-1".to_owned()),
            Literal::Str("1.5".to_owned()),
            Literal::Str("-3.25".to_owned()),
        ];
        witness_satisfies_lens_laws(&w, &samples, &[])
            .expect("str_to_float GetPut should hold on canonical-printed strings");
    }

    #[test]
    fn str_to_float_fails_on_non_canonical_string() {
        let w = str_to_float_witness();
        // "1.00" parses to 1.0, and FloatToStr formats 1.0 as "1" (the
        // shortest-canonical form), so `"1.00"` does not round-trip.
        let err = witness_satisfies_lens_laws(&w, &[Literal::Str("1.00".to_owned())], &[])
            .expect_err("non-canonical numeric strings must not round-trip through str_to_float");
        assert!(
            err.contains("GetPut"),
            "error should identify GetPut: {err}"
        );
    }

    #[test]
    fn bool_to_str_round_trips() {
        let w = bool_to_str_witness();
        witness_satisfies_lens_laws(
            &w,
            &[Literal::Bool(true), Literal::Bool(false)],
            &[
                Literal::Str("true".to_owned()),
                Literal::Str("false".to_owned()),
            ],
        )
        .expect("bool_to_str GetPut should hold on {true, false}");
    }

    #[test]
    fn bool_to_str_fails_on_off_domain_string() {
        let w = bool_to_str_witness();
        witness_forward_fails_on(&w, &Literal::Str("yes".to_owned()))
            .expect("non-canonical strings should not round-trip through bool_to_str");
        witness_forward_fails_on(&w, &Literal::Str("TRUE".to_owned()))
            .expect("TRUE (uppercase) should not round-trip — only canonical lowercase");
    }

    #[test]
    fn str_to_bool_round_trips_on_canonical_strings() {
        let w = str_to_bool_witness();
        witness_satisfies_lens_laws(
            &w,
            &[
                Literal::Str("true".to_owned()),
                Literal::Str("false".to_owned()),
            ],
            &[Literal::Bool(true), Literal::Bool(false)],
        )
        .expect("str_to_bool GetPut should hold on canonical samples");
    }

    #[test]
    fn str_to_bool_fails_on_off_domain_string() {
        let w = str_to_bool_witness();
        let err = witness_satisfies_lens_laws(&w, &[Literal::Str("yes".to_owned())], &[])
            .expect_err("non-canonical source strings must not round-trip through str_to_bool");
        assert!(
            err.contains("GetPut") || err.contains("forward"),
            "error should identify a law violation: {err}"
        );
    }

    #[test]
    fn bool_to_float_round_trips() {
        let w = bool_to_float_witness();
        witness_satisfies_lens_laws(
            &w,
            &[Literal::Bool(true), Literal::Bool(false)],
            &[Literal::Float(0.0), Literal::Float(1.0)],
        )
        .expect("bool_to_float GetPut should hold on {true, false}");
    }

    #[test]
    fn bool_to_float_fails_on_off_domain_float() {
        let w = bool_to_float_witness();
        witness_forward_fails_on(&w, &Literal::Float(2.5))
            .expect("non-0/1 floats should not round-trip through bool_to_float");
    }

    #[test]
    fn float_to_bool_round_trips_on_zero_one() {
        let w = float_to_bool_witness();
        witness_satisfies_lens_laws(
            &w,
            &[Literal::Float(0.0), Literal::Float(1.0)],
            &[Literal::Bool(true), Literal::Bool(false)],
        )
        .expect("float_to_bool GetPut should hold on {0.0, 1.0}");
    }

    #[test]
    fn float_to_bool_fails_on_fractional_float() {
        let w = float_to_bool_witness();
        let err = witness_satisfies_lens_laws(&w, &[Literal::Float(2.5)], &[])
            .expect_err("non-0/1 floats must not round-trip through float_to_bool");
        assert!(
            err.contains("GetPut"),
            "error should identify GetPut: {err}"
        );
    }

    #[test]
    fn default_library_covers_all_new_witness_pairs() {
        let lib = default_witness_library();
        // Every (source, target) kind pair for the four primitive
        // carriers (Bool, Int, Float, Str) now has a witness.
        for src in [
            ValueKind::Bool,
            ValueKind::Int,
            ValueKind::Float,
            ValueKind::Str,
        ] {
            for tgt in [
                ValueKind::Bool,
                ValueKind::Int,
                ValueKind::Float,
                ValueKind::Str,
            ] {
                if src == tgt {
                    continue;
                }
                assert!(
                    !lib.lookup(src, tgt).is_empty(),
                    "missing witness from {src:?} to {tgt:?}"
                );
            }
        }
        // Bytes / Token / Null intentionally do not ship witnesses in
        // the default library; confirm those remain empty so a future
        // addition is deliberate.
        assert!(lib.lookup(ValueKind::Bytes, ValueKind::Str).is_empty());
        assert!(lib.lookup(ValueKind::Token, ValueKind::Int).is_empty());
    }

    #[test]
    fn library_lookup_returns_empty_for_unknown_pair() {
        let lib = default_witness_library();
        assert!(lib.lookup(ValueKind::Bytes, ValueKind::Token).is_empty());
    }

    #[test]
    fn library_iter_is_deterministic() {
        // Two identical libraries built independently should iterate
        // in the same order, regardless of HashMap internal ordering.
        let a: Vec<_> = default_witness_library()
            .iter()
            .map(|w| w.name.clone())
            .collect();
        let b: Vec<_> = default_witness_library()
            .iter()
            .map(|w| w.name.clone())
            .collect();
        assert_eq!(a, b, "iter() order must be deterministic across builds");
    }

    #[test]
    fn default_library_has_unique_witness_names() {
        let lib = default_witness_library();
        lib.witness_names_are_unique()
            .expect("default library must have unique witness names");
    }

    #[test]
    fn witness_by_name_finds_registered_entry() {
        let lib = default_witness_library();
        let w = lib
            .witness_by_name("int_to_str")
            .expect("default library should expose int_to_str");
        assert_eq!(w.source_kind, ValueKind::Int);
        assert_eq!(w.target_kind, ValueKind::Str);
        assert!(lib.witness_by_name("does_not_exist").is_none());
    }

    #[test]
    fn witness_names_duplicate_detected() {
        let mut lib = WitnessLibrary::new();
        lib.register(int_to_str_witness());
        // Register a second witness with the SAME name but different
        // kinds so `lookup` cannot distinguish them by key alone. The
        // invariant check should flag this.
        let mut dup = bool_to_int_witness();
        dup.name = "int_to_str".to_owned();
        lib.register(dup);
        let err = lib.witness_names_are_unique().unwrap_err();
        assert_eq!(err, "int_to_str");
    }

    #[test]
    fn extend_with_colliding_witness_name_does_not_deduplicate() {
        // Audit concern 8: the uniqueness invariant on witness names is
        // opt-in via `witness_names_are_unique`. `extend` itself does
        // NOT enforce uniqueness - it blindly registers every witness
        // from `other`. A user who extends the default library with
        // their own "int_to_str" will observe a post-extend library
        // where `witness_by_name("int_to_str")` still returns the
        // FIRST registered witness (deterministic iter order); the
        // duplicate is reachable only via `lookup`.
        //
        // This test pins that behaviour so a future uniqueness-on-
        // extend change is a deliberate breaking choice rather than a
        // silent semantic shift.
        let mut lib = default_witness_library();
        let mut user = WitnessLibrary::new();
        let mut custom = int_to_str_witness();
        custom.description = "USER-OVERRIDE int_to_str".to_owned();
        user.register(custom);
        lib.extend(user);
        // Uniqueness check must now FAIL, exhibiting the duplicate.
        let err = lib
            .witness_names_are_unique()
            .expect_err("extend must not silently deduplicate colliding names");
        assert_eq!(err, "int_to_str");
        // `lookup` returns both, in registration order: default first,
        // user second.
        let candidates = lib.lookup(ValueKind::Int, ValueKind::Str);
        assert_eq!(candidates.len(), 2);
        assert!(
            candidates[0]
                .description
                .starts_with("int → str via IntToStr")
        );
        assert!(candidates[1].description.starts_with("USER-OVERRIDE"));
    }

    #[test]
    fn extend_merges_in_deterministic_order() {
        // Build two libraries A and B with disjoint kind pairs whose
        // HashMap insertion order can vary run-to-run, then merge
        // `extend` each into a fresh base and confirm both merges
        // produce the same post-extend iteration order.
        let base_a = default_witness_library();
        let base_b = default_witness_library();
        let mut target1 = WitnessLibrary::new();
        target1.extend(base_a);
        let mut target2 = WitnessLibrary::new();
        target2.extend(base_b);
        let order1: Vec<_> = target1.iter().map(|w| w.name.clone()).collect();
        let order2: Vec<_> = target2.iter().map(|w| w.name.clone()).collect();
        assert_eq!(order1, order2, "extend must preserve canonical ordering");
    }

    #[test]
    fn float_to_str_round_trips_pathological_ieee_values() {
        // Classic IEEE-754 examples whose printed form depends on
        // shortest-canonical formatting. `0.1 + 0.2 = 0.30000000000000004`
        // and `1.0 / 3.0` both stress the FloatToStr/StrToFloat
        // round-trip. `literal_equal`'s 1e-12 relative tolerance should
        // absorb any sub-ULP drift while surfacing real precision loss.
        let w = float_to_str_witness();
        let samples = vec![
            Literal::Float(0.1_f64 + 0.2_f64),
            Literal::Float(1.0_f64 / 3.0_f64),
            Literal::Float(std::f64::consts::PI),
            Literal::Float(f64::MIN_POSITIVE),
        ];
        witness_satisfies_lens_laws(&w, &samples, &[])
            .expect("FloatToStr/StrToFloat must round-trip pathological IEEE-754 values");
    }

    #[test]
    fn is_iso_is_false_for_non_iso_classes() {
        // Pin: `is_iso` must return true only for `Iso`, false for
        // every other round-trip class (including the `Opaque` floor).
        let mut w = int_to_str_witness();
        assert!(!w.is_iso(), "Retraction is not Iso");
        w.class = CoercionClass::Projection;
        assert!(!w.is_iso(), "Projection is not Iso");
        w.class = CoercionClass::Opaque;
        assert!(!w.is_iso(), "Opaque is not Iso");
        w.class = CoercionClass::Iso;
        assert!(w.is_iso(), "Iso must be recognized as Iso");
    }

    #[test]
    fn default_witness_library_is_structurally_identical_across_builds() {
        // Iter order + name set must be identical between two fresh
        // constructions. This guards against future edits that might
        // introduce order-dependent state (e.g. randomization in a
        // shared registry).
        let a = default_witness_library();
        let b = default_witness_library();
        let names_a: Vec<String> = a.iter().map(|w| w.name.clone()).collect();
        let names_b: Vec<String> = b.iter().map(|w| w.name.clone()).collect();
        assert_eq!(names_a, names_b);
        // Classes and kind pairs must match too.
        let sig_a: Vec<_> = a
            .iter()
            .map(|w| (w.name.clone(), w.source_kind, w.target_kind, w.class))
            .collect();
        let sig_b: Vec<_> = b
            .iter()
            .map(|w| (w.name.clone(), w.source_kind, w.target_kind, w.class))
            .collect();
        assert_eq!(sig_a, sig_b);
    }

    #[test]
    fn library_lookup_preserves_insertion_order() {
        // Two witnesses for the same kind pair must surface in the
        // order they were registered. This is load-bearing for
        // confidence ranking in align::coerce.
        let mut lib = WitnessLibrary::new();
        lib.register(int_to_str_witness());
        // Register a second int→str witness (dummy: same expression,
        // different name) and confirm ordering.
        let mut dup = int_to_str_witness();
        dup.name = "int_to_str_alt".to_owned();
        lib.register(dup);
        let got: Vec<&str> = lib
            .lookup(ValueKind::Int, ValueKind::Str)
            .iter()
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(got, vec!["int_to_str", "int_to_str_alt"]);
    }
}
