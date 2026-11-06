//! Sample-based verification of declared coercion classes.
//!
//! A [`DirectedEquation`] carries a
//! [`CoercionClass`] that declares the
//! round-trip fidelity of its forward (`impl_term`) and backward
//! (`inverse`) expressions. The functions in this module run a
//! declared class's round-trip laws against a supplied set of sample
//! inputs and report any violations.
//!
//! The checked elementary constructors
//! [`sort_coerce_checked`](crate::protolens::elementary::sort_coerce_checked)
//! and
//! [`directed_eq_checked`](crate::protolens::elementary::directed_eq_checked)
//! call [`check_coercion_honesty`] at construction time, so a dishonest
//! declaration is rejected where it is built rather than silently
//! accepted (the older infallible constructors remain as the
//! `*_unchecked`-style escape hatch). The check is *evidence, not
//! proof*: it exercises the declared laws on the supplied samples only.
//!
//! # Laws by class
//!
//! | Class | Forward law | Backward law |
//! |---|---|---|
//! | `Iso` | `forward(inverse(v)) == v` for every sample `v` | `inverse(forward(s)) == s` for every sample `s` |
//! | `Retraction` | not required | `inverse(forward(s)) == s` (forward is a section) |
//! | `Projection` | `forward(forward(s))` stable (deterministic) | not applicable |
//! | `Opaque` | not required | not applicable |
//!
//! `Iso` and `Retraction` additionally require `inverse` to be
//! `Some(_)`; a missing inverse on either is a
//! [`CoercionLawViolation::MissingInverse`]. `Projection` and `Opaque`
//! with an inverse present is not a violation; the inverse is simply
//! not consulted.
//!
//! # Sample generation
//!
//! Samples are supplied by the caller as a slice of
//! [`panproto_expr::Literal`] values. This module does not generate
//! them; callers can use any source (a fixed fixture, a random
//! generator, values extracted from a schema's vertex carriers, etc.).
//! See [`default_samples_for_string_value`] for the simplest case.
//!
//! # Limitations
//!
//! The check is sound but incomplete: passing on all supplied samples
//! proves nothing about inputs that were not tried. Treat the result
//! as evidence, not as a proof. For `Retraction` in particular, only
//! the backward law (`inverse ∘ forward = id`) is checked; the
//! forward-injectivity side is a property of the underlying function
//! and is not sample-testable here.

use std::fmt;
use std::sync::Arc;

use panproto_expr::{Env, EvalConfig, Expr, Literal, eval};
use panproto_gat::{CoercionClass, DirectedEquation, Theory, ValueKind};
use panproto_inst::value::FieldPresence;
use panproto_inst::{FInstance, GInstance, RowId, value_to_expr_literal};
use rustc_hash::FxHashMap;

/// A graph carrier from which a coercion-law sample was drawn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphCoercionCarrier {
    /// A declaration-level failure independent of any instance sample.
    Declaration,
    /// The primary value carried by a node.
    NodeValue {
        /// Numeric graph node id.
        node: u32,
    },
    /// An extra field retained on a node.
    Field {
        /// Numeric graph node id.
        node: u32,
        /// Extra-field key.
        field: String,
    },
}

/// A table carrier from which a coercion-law sample was drawn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TableCoercionCarrier {
    /// A declaration-level failure independent of any instance sample.
    Declaration,
    /// One cell in a named table row.
    Cell {
        /// Table vertex name.
        table: String,
        /// Globally unique row id.
        row: RowId,
        /// Column name.
        column: String,
    },
}

/// A graph violation together with the carrier that supplied its sample.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphCoercionLawViolation {
    /// Carrier whose value failed the declared law.
    pub carrier: GraphCoercionCarrier,
    /// Violation reported by the shared coercion algebra.
    pub violation: CoercionLawViolation,
}

/// A table violation together with the carrier that supplied its sample.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TableCoercionLawViolation {
    /// Carrier whose value failed the declared law.
    pub carrier: TableCoercionCarrier,
    /// Violation reported by the shared coercion algebra.
    pub violation: CoercionLawViolation,
}

/// A carrier value that cannot be represented in the expression language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CoercionCarrierError {
    /// An unsupported graph carrier value.
    #[error("graph carrier {carrier:?} contains an inexpressible {value_type}")]
    Graph {
        /// Carrier containing the unsupported value.
        carrier: GraphCoercionCarrier,
        /// Stable instance-value type name.
        value_type: &'static str,
    },
    /// An unsupported table carrier value.
    #[error("table carrier {carrier:?} contains an inexpressible {value_type}")]
    Table {
        /// Carrier containing the unsupported value.
        carrier: TableCoercionCarrier,
        /// Stable instance-value type name.
        value_type: &'static str,
    },
}

/// A violation of a declared coercion class's round-trip law on a
/// single sample input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum CoercionLawViolation {
    /// The forward-then-backward law `inverse(forward(s)) == s` failed
    /// on this sample. Applies to `Iso` and `Retraction` classes.
    Backward {
        /// Input value `s`.
        input: Literal,
        /// Value of `forward(s)`.
        forward_result: Literal,
        /// Value of `inverse(forward(s))`.
        round_tripped: Literal,
    },
    /// The backward-then-forward law `forward(inverse(v)) == v` failed
    /// on this sample. Applies to `Iso` class only.
    Forward {
        /// Input value `v`.
        input: Literal,
        /// Value of `inverse(v)`.
        inverse_result: Literal,
        /// Value of `forward(inverse(v))`.
        round_tripped: Literal,
    },
    /// The forward function is not deterministic on this sample: two
    /// evaluations produced different outputs. Applies to `Projection`
    /// class.
    NonDeterministic {
        /// Input value.
        input: Literal,
        /// First evaluation's result.
        first: Literal,
        /// Second evaluation's result.
        second: Literal,
    },
    /// The declared class requires an inverse (`Iso` or `Retraction`)
    /// but no inverse expression is available.
    MissingInverse {
        /// The declared class.
        class: CoercionClass,
    },
    /// Evaluating the forward expression returned an error on this
    /// sample. Typically a type mismatch or an unbound variable inside
    /// `forward`.
    ForwardEvalError {
        /// Input value.
        input: Literal,
        /// Stringified error.
        error: String,
    },
    /// Evaluating the inverse expression returned an error on this
    /// sample. Typically a type mismatch between `forward`'s output
    /// and `inverse`'s expected input.
    InverseEvalError {
        /// Input value fed into `forward` (for backward law) or the
        /// sample itself (for forward law).
        input: Literal,
        /// Stringified error.
        error: String,
    },
    /// A coercion class variant the checker does not recognise (added
    /// to the upstream `CoercionClass` enum after this module was
    /// last updated). Returned once per invocation rather than per
    /// sample to keep the output concise.
    ///
    /// Stores the `Debug` rendering of the unrecognised class rather
    /// than the [`CoercionClass`] value itself so the identifying
    /// information survives future `Debug` impl drift on the enum.
    UnknownClass {
        /// Best-effort `Debug` rendering of the unrecognised class.
        debug_repr: String,
    },
}

/// Check a forward / inverse expression pair against the round-trip
/// laws of a declared [`CoercionClass`].
///
/// The `var_name` parameter is the name under which each sample is
/// bound in the evaluation environment. For `ApplyExpr`-style
/// transforms this is the field key; for standalone coercions, any
/// name that appears as a free variable in `forward` and `inverse`.
///
/// Returns one [`CoercionLawViolation`] per failing sample. An empty
/// vector means all supplied samples satisfy the declared laws; it
/// does not prove the declaration holds in general.
#[must_use]
pub fn check_coercion_laws(
    forward: &Expr,
    inverse: Option<&Expr>,
    class: CoercionClass,
    samples: &[Literal],
    var_name: &str,
) -> Vec<CoercionLawViolation> {
    let var: Arc<str> = Arc::from(var_name);
    let config = EvalConfig::default();
    let mut violations = Vec::new();

    match class {
        CoercionClass::Iso => {
            let Some(inv) = inverse else {
                violations.push(CoercionLawViolation::MissingInverse { class });
                return violations;
            };
            for sample in samples {
                check_backward(forward, inv, sample, &var, &config, &mut violations);
                check_forward(forward, inv, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Retraction => {
            let Some(inv) = inverse else {
                violations.push(CoercionLawViolation::MissingInverse { class });
                return violations;
            };
            for sample in samples {
                check_backward(forward, inv, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Projection => {
            for sample in samples {
                check_deterministic(forward, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Opaque => {
            // Opaque makes no round-trip claim; nothing to verify.
        }
        other => {
            // A class variant the checker does not recognise.
            // Surface it once rather than silently passing so a
            // future class addition cannot masquerade as Opaque.
            // Capture the `Debug` rendering here so downstream
            // consumers see identifying info even if the enum's
            // `Debug` impl drifts across releases.
            violations.push(CoercionLawViolation::UnknownClass {
                debug_repr: format!("{other:?}"),
            });
        }
    }

    violations
}

/// Check the shared coercion algebra using every expressible graph node value
/// and extra field as a carrier-labelled sample.
///
/// Nodes are visited by ascending id, with the primary value before
/// lexicographically ordered extra fields. Absent values are not samples;
/// explicit nulls are.
///
/// # Errors
///
/// Returns [`CoercionCarrierError`] at the first carrier whose value has no
/// faithful expression literal.
pub fn check_graph_coercion_laws(
    forward: &Expr,
    inverse: Option<&Expr>,
    class: CoercionClass,
    instance: &GInstance,
    var_name: &str,
) -> Result<Vec<GraphCoercionLawViolation>, CoercionCarrierError> {
    let mut node_ids: Vec<u32> = instance.nodes().keys().copied().collect();
    node_ids.sort_unstable();
    let mut out = check_coercion_laws(forward, inverse, class, &[], var_name)
        .into_iter()
        .map(|violation| GraphCoercionLawViolation {
            carrier: GraphCoercionCarrier::Declaration,
            violation,
        })
        .collect::<Vec<_>>();
    if !out.is_empty() {
        return Ok(out);
    }
    for node_id in node_ids {
        let node = &instance.nodes()[&node_id];
        if let Some(presence) = &node.value {
            let carrier = GraphCoercionCarrier::NodeValue { node: node_id };
            let literal = match presence {
                FieldPresence::Present(value) => {
                    value_to_expr_literal(value).map_err(|error| CoercionCarrierError::Graph {
                        carrier: carrier.clone(),
                        value_type: error.value_type,
                    })?
                }
                FieldPresence::Null => Literal::Null,
                FieldPresence::Absent => continue,
            };
            out.extend(
                check_coercion_laws(forward, inverse, class, &[literal], var_name)
                    .into_iter()
                    .map(|violation| GraphCoercionLawViolation {
                        carrier: carrier.clone(),
                        violation,
                    }),
            );
        }
        let mut fields: Vec<_> = node.extra_fields.iter().collect();
        fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
        for (field, value) in fields {
            let carrier = GraphCoercionCarrier::Field {
                node: node_id,
                field: field.clone(),
            };
            let literal =
                value_to_expr_literal(value).map_err(|error| CoercionCarrierError::Graph {
                    carrier: carrier.clone(),
                    value_type: error.value_type,
                })?;
            out.extend(
                check_coercion_laws(forward, inverse, class, &[literal], var_name)
                    .into_iter()
                    .map(|violation| GraphCoercionLawViolation {
                        carrier: carrier.clone(),
                        violation,
                    }),
            );
        }
    }
    Ok(out)
}

/// Check the shared coercion algebra using every table cell as a
/// carrier-labelled sample.
///
/// Tables, rows, and columns are visited in ascending order.
///
/// # Errors
///
/// Returns [`CoercionCarrierError`] at the first cell whose value has no
/// faithful expression literal.
pub fn check_table_coercion_laws(
    forward: &Expr,
    inverse: Option<&Expr>,
    class: CoercionClass,
    instance: &FInstance,
    var_name: &str,
) -> Result<Vec<TableCoercionLawViolation>, CoercionCarrierError> {
    let mut out = check_coercion_laws(forward, inverse, class, &[], var_name)
        .into_iter()
        .map(|violation| TableCoercionLawViolation {
            carrier: TableCoercionCarrier::Declaration,
            violation,
        })
        .collect::<Vec<_>>();
    if !out.is_empty() {
        return Ok(out);
    }
    for (table, rows) in instance.tables() {
        for (row_id, row) in rows.rows() {
            let mut cells: Vec<_> = row.iter().collect();
            cells.sort_unstable_by(|a, b| a.0.cmp(b.0));
            for (column, value) in cells {
                let carrier = TableCoercionCarrier::Cell {
                    table: table.to_string(),
                    row: row_id,
                    column: column.clone(),
                };
                let literal =
                    value_to_expr_literal(value).map_err(|error| CoercionCarrierError::Table {
                        carrier: carrier.clone(),
                        value_type: error.value_type,
                    })?;
                out.extend(
                    check_coercion_laws(forward, inverse, class, &[literal], var_name)
                        .into_iter()
                        .map(|violation| TableCoercionLawViolation {
                            carrier: carrier.clone(),
                            violation,
                        }),
                );
            }
        }
    }
    Ok(out)
}

/// Check the round-trip laws of `deq`'s declared coercion class using
/// samples that bind the supplied key as the input variable.
///
/// If `deq.inverse` is `None`, the class is expected to be `Opaque`
/// or `Projection` (which do not consult the inverse). If the class
/// is `Iso` or `Retraction` without an inverse, a
/// [`CoercionLawViolation::MissingInverse`] is reported.
#[must_use]
pub fn check_directed_equation_coercion_law(
    deq: &DirectedEquation,
    samples: &[Literal],
    var_name: &str,
) -> Vec<CoercionLawViolation> {
    check_coercion_laws(
        &deq.impl_term,
        deq.inverse.as_ref(),
        deq.coercion_class,
        samples,
        var_name,
    )
}

/// Default sample set for string-valued coercions.
///
/// Covers the empty string, a lowercase ASCII identifier, a
/// mixed-case name, an all-uppercase string, a string containing
/// whitespace, and a short Unicode string. Useful as a sanity-check
/// when no domain-specific samples are available.
#[must_use]
pub fn default_samples_for_string_value() -> Vec<Literal> {
    vec![
        Literal::Str(String::new()),
        Literal::Str("name".to_owned()),
        Literal::Str("Alice".to_owned()),
        Literal::Str("ALICE".to_owned()),
        Literal::Str("hello world".to_owned()),
        Literal::Str("schön".to_owned()),
    ]
}

fn check_backward(
    forward: &Expr,
    inverse: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let forward_result = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let inverse_env = Env::new().extend(Arc::clone(var), forward_result.clone());
    match eval(inverse, &inverse_env, config) {
        Ok(round_tripped) => {
            if round_tripped != *sample {
                violations.push(CoercionLawViolation::Backward {
                    input: sample.clone(),
                    forward_result,
                    round_tripped,
                });
            }
        }
        Err(e) => {
            violations.push(CoercionLawViolation::InverseEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
        }
    }
}

fn check_forward(
    forward: &Expr,
    inverse: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let inverse_result = match eval(inverse, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::InverseEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let forward_env = Env::new().extend(Arc::clone(var), inverse_result.clone());
    match eval(forward, &forward_env, config) {
        Ok(round_tripped) => {
            if round_tripped != *sample {
                violations.push(CoercionLawViolation::Forward {
                    input: sample.clone(),
                    inverse_result,
                    round_tripped,
                });
            }
        }
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
        }
    }
}

fn check_deterministic(
    forward: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let first = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let second = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    if first != second {
        violations.push(CoercionLawViolation::NonDeterministic {
            input: sample.clone(),
            first,
            second,
        });
    }
}

/// A per-value-kind registry of sample inputs for coercion law checks.
///
/// Callers register a set of sample `Literal` values for each
/// [`ValueKind`] they care about;
/// [`check_directed_equation_with_registry`] uses the registry entry
/// matching `deq.source_kind` to supply the sample input.
#[derive(Debug, Clone)]
pub struct CoercionSampleRegistry {
    samples: FxHashMap<ValueKind, Vec<Literal>>,
}

impl CoercionSampleRegistry {
    /// Construct an empty registry. Lookups on every value kind return
    /// the empty slice until [`Self::register`] is called.
    #[must_use]
    pub fn new() -> Self {
        Self {
            samples: FxHashMap::default(),
        }
    }

    /// Construct a registry pre-populated with defaults for every
    /// primitive [`ValueKind`].
    ///
    /// The default samples cover common edge cases:
    /// `Bool` (both), `Int` (small, large, zero, negative),
    /// `Float` (zero, small, negative, non-integer),
    /// `Str` (the six values from
    /// [`default_samples_for_string_value`]),
    /// `Bytes` (empty + small), `Null`, and `Token` (short
    /// identifier-like strings). `Any` receives the union of every
    /// other kind's samples.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(
            ValueKind::Bool,
            vec![Literal::Bool(false), Literal::Bool(true)],
        );
        reg.register(
            ValueKind::Int,
            vec![
                Literal::Int(0),
                Literal::Int(1),
                Literal::Int(-1),
                Literal::Int(42),
                Literal::Int(i64::MAX),
                Literal::Int(i64::MIN),
            ],
        );
        reg.register(
            ValueKind::Float,
            vec![
                Literal::Float(0.0),
                Literal::Float(1.0),
                Literal::Float(-1.0),
                Literal::Float(3.5),
                Literal::Float(-2.25),
            ],
        );
        reg.register(ValueKind::Str, default_samples_for_string_value());
        reg.register(
            ValueKind::Bytes,
            vec![
                Literal::Bytes(Vec::new()),
                Literal::Bytes(b"abc".to_vec()),
                Literal::Bytes(vec![0, 255, 7]),
            ],
        );
        reg.register(ValueKind::Null, vec![Literal::Null]);
        reg.register(
            ValueKind::Token,
            vec![
                Literal::Str("token".to_owned()),
                Literal::Str("id_42".to_owned()),
                Literal::Str("ns:name".to_owned()),
            ],
        );
        // Refined scalars sample as their canonical string / decimal forms.
        reg.register(
            ValueKind::DateTime,
            vec![
                Literal::Str("2026-07-14T09:47:12Z".to_owned()),
                Literal::Str("2000-01-01T00:00:00Z".to_owned()),
            ],
        );
        reg.register(
            ValueKind::Date,
            vec![
                Literal::Str("2026-07-14".to_owned()),
                Literal::Str("2000-01-01".to_owned()),
            ],
        );
        reg.register(
            ValueKind::Time,
            vec![
                Literal::Str("09:47:12".to_owned()),
                Literal::Str("00:00:00".to_owned()),
            ],
        );
        reg.register(
            ValueKind::Decimal,
            vec![
                Literal::Str("0".to_owned()),
                Literal::Str("3.14".to_owned()),
                Literal::Str("-100.00".to_owned()),
            ],
        );
        reg.register(
            ValueKind::Uuid,
            vec![
                Literal::Str("00000000-0000-0000-0000-000000000000".to_owned()),
                Literal::Str("550e8400-e29b-41d4-a716-446655440000".to_owned()),
            ],
        );

        // `Any` is the union across every other registered kind.
        // Iterate `ValueKind::all()` (which excludes nothing but the
        // `Any` slot itself, handled below) rather than the backing
        // `FxHashMap` so the `Any` bucket is reproducible; the
        // hashmap's iteration order is not stable and would leak
        // into downstream law-check output.
        let mut union: Vec<Literal> = Vec::new();
        for kind in ValueKind::all() {
            if matches!(kind, ValueKind::Any) {
                continue;
            }
            if let Some(vs) = reg.samples.get(kind) {
                union.extend(vs.iter().cloned());
            }
        }
        reg.register(ValueKind::Any, union);
        reg
    }

    /// Register `samples` as the sample set for `kind`. Replaces any
    /// previously registered samples for that kind.
    pub fn register(&mut self, kind: ValueKind, samples: Vec<Literal>) {
        self.samples.insert(kind, samples);
    }

    /// Look up the registered samples for `kind`. Returns an empty
    /// slice if no samples were registered.
    #[must_use]
    pub fn samples_for(&self, kind: ValueKind) -> &[Literal] {
        self.samples.get(&kind).map_or(&[], Vec::as_slice)
    }

    /// Indicates whether any samples have been registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.values().all(Vec::is_empty)
    }
}

impl Default for CoercionSampleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Check the round-trip laws of `deq`'s declared coercion class using
/// samples drawn from a [`CoercionSampleRegistry`].
///
/// The registry entry for `deq.source_kind` supplies the samples. If
/// `deq.source_kind` is `None` (untyped equation), the lookup falls
/// back to the `Any` entry. When neither entry has samples, the
/// function returns an empty violation list (there is nothing to
/// test).
#[must_use]
pub fn check_directed_equation_with_registry(
    deq: &DirectedEquation,
    registry: &CoercionSampleRegistry,
    var_name: &str,
) -> Vec<CoercionLawViolation> {
    let primary = deq
        .source_kind
        .map_or(&[] as &[Literal], |k| registry.samples_for(k));
    let samples: &[Literal] = if primary.is_empty() {
        registry.samples_for(ValueKind::Any)
    } else {
        primary
    };
    if samples.is_empty() {
        return Vec::new();
    }
    check_directed_equation_coercion_law(deq, samples, var_name)
}

/// A construction-time rejection: a declared [`CoercionClass`] failed
/// its sample-based round-trip laws.
///
/// Returned by [`check_coercion_honesty`] and by the checked elementary
/// constructors. The [`Display`](fmt::Display) rendering carries the
/// *evidence, not proof* caveat: the samples that were tried do not
/// round-trip under the declared class, so the declaration cannot be
/// honest — but passing would not have proved honesty in general.
#[derive(Debug, Clone)]
pub struct CoercionHonestyError {
    /// The declared class that failed verification.
    pub class: CoercionClass,
    /// The per-sample violations that were found (non-empty).
    pub violations: Vec<CoercionLawViolation>,
}

impl fmt::Display for CoercionHonestyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "declared coercion class {:?} is not honest: {} sample(s) failed its \
             round-trip laws. This is evidence, not proof — the samples that were \
             tried do not round-trip, so the declaration cannot hold in general. \
             Violations: {:?}",
            self.class,
            self.violations.len(),
            self.violations,
        )
    }
}

impl std::error::Error for CoercionHonestyError {}

/// Sample-check a forward / inverse pair declared as `class` at
/// construction time, drawing samples for `source_kind` from `registry`.
///
/// When `registry` has no samples for `source_kind`, the `Any` bucket is
/// used as a fallback. When neither has samples, the check passes
/// vacuously (there is nothing to test).
///
/// This is the honesty gate wired into the checked elementary
/// constructors. It is *evidence, not proof*: an empty violation list
/// means every supplied sample satisfied the declared laws, not that the
/// declaration holds for all inputs.
///
/// # Errors
///
/// Returns [`CoercionHonestyError`] carrying every sample-level violation
/// when the declared class cannot be verified on the drawn samples.
pub fn check_coercion_honesty(
    forward: &Expr,
    inverse: Option<&Expr>,
    class: CoercionClass,
    source_kind: ValueKind,
    var_name: &str,
    registry: &CoercionSampleRegistry,
) -> Result<(), CoercionHonestyError> {
    let primary = registry.samples_for(source_kind);
    let samples: &[Literal] = if primary.is_empty() {
        registry.samples_for(ValueKind::Any)
    } else {
        primary
    };
    let violations = check_coercion_laws(forward, inverse, class, samples, var_name);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(CoercionHonestyError { class, violations })
    }
}

/// Report produced by [`check_theory`].
///
/// Each entry pairs a directed equation's name with the violations
/// (if any) surfaced by sample-based law checking.
#[derive(Debug, Clone, Default)]
pub struct TheoryCoercionReport {
    /// One entry per directed equation checked, in theory declaration
    /// order. An entry with an empty violation vector means the
    /// equation passed every sample.
    pub per_equation: Vec<(Arc<str>, Vec<CoercionLawViolation>)>,
}

impl TheoryCoercionReport {
    /// Indicates whether every directed equation passed. Equivalent to
    /// `self.violation_count() == 0`.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.per_equation.iter().all(|(_, vs)| vs.is_empty())
    }

    /// Total number of violations across every directed equation.
    #[must_use]
    pub fn violation_count(&self) -> usize {
        self.per_equation.iter().map(|(_, vs)| vs.len()).sum()
    }
}

/// Validate every directed equation in `theory` against `registry`.
///
/// Each equation is checked with the default variable binding name
/// `"x"`. Callers whose equations use a different free variable name
/// should call [`check_theory_with_var`] (or
/// [`check_directed_equation_with_registry`] directly).
#[must_use]
pub fn check_theory(theory: &Theory, registry: &CoercionSampleRegistry) -> TheoryCoercionReport {
    check_theory_with_var(theory, registry, "x")
}

/// Variable-name-parameterized [`check_theory`].
///
/// Binds each sample under `var_name` instead of the default `"x"`.
/// Use this when the theory's directed equations share a different
/// free variable name (for example, `"v"` or a field key).
#[must_use]
pub fn check_theory_with_var(
    theory: &Theory,
    registry: &CoercionSampleRegistry,
    var_name: &str,
) -> TheoryCoercionReport {
    let mut per_equation = Vec::with_capacity(theory.directed_eqs.len());
    for deq in &theory.directed_eqs {
        let violations = check_directed_equation_with_registry(deq, registry, var_name);
        per_equation.push((Arc::clone(&deq.name), violations));
    }
    TheoryCoercionReport { per_equation }
}

/// Extension trait adding coercion-law validation to
/// [`DirectedEquation`].
///
/// Implemented only in `panproto-lens` to keep `panproto-gat` free of
/// lens-specific dependencies. Callers bring the method into scope
/// with `use panproto_lens::coercion_laws::CoercionLawValidation;`.
pub trait CoercionLawValidation {
    /// Validate the declared coercion class against samples drawn from
    /// `registry`.
    ///
    /// Returns `Ok(())` when every sample satisfies the declared
    /// laws. Returns `Err(violations)` otherwise. The `Result` is the
    /// sole contract: debug and release builds behave identically, so
    /// callers can rely on the `Err` path being exercised by their
    /// test suite.
    ///
    /// # Errors
    ///
    /// Returns the full list of sample-level violations when the
    /// declared coercion class cannot be verified on the supplied
    /// samples.
    fn validate_coercion_law(
        &self,
        registry: &CoercionSampleRegistry,
        var_name: &str,
    ) -> Result<(), Vec<CoercionLawViolation>>;
}

impl CoercionLawValidation for DirectedEquation {
    fn validate_coercion_law(
        &self,
        registry: &CoercionSampleRegistry,
        var_name: &str,
    ) -> Result<(), Vec<CoercionLawViolation>> {
        let violations = check_directed_equation_with_registry(self, registry, var_name);
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_expr::{BuiltinOp, Expr};
    use panproto_inst::{Node, Value};
    use std::collections::HashMap;

    /// `upper x` coerces a string to uppercase.
    fn upper_expr(var: &str) -> Expr {
        Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from(var))])
    }

    /// Identity expression `x` (a pure pass-through).
    fn identity_expr(var: &str) -> Expr {
        Expr::Var(Arc::from(var))
    }

    #[test]
    fn graph_carriers_use_shared_algebra_and_retain_location() {
        let graph = GInstance::new().with_node(
            Node::new(7, "text")
                .with_value(FieldPresence::Present(Value::Str("Alice".into())))
                .with_extra_field("alias", Value::Str("Bob".into())),
        );
        let violations = check_graph_coercion_laws(
            &upper_expr("x"),
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &graph,
            "x",
        )
        .unwrap();
        assert_eq!(violations.len(), 4);
        assert!(matches!(
            (&violations[0].carrier, &violations[0].violation),
            (
                GraphCoercionCarrier::NodeValue { node: 7 },
                CoercionLawViolation::Backward { .. }
            )
        ));
        assert!(matches!(
            (&violations[1].carrier, &violations[1].violation),
            (
                GraphCoercionCarrier::NodeValue { node: 7 },
                CoercionLawViolation::Forward { .. }
            )
        ));
        assert!(matches!(
            (&violations[2].carrier, &violations[2].violation),
            (
                GraphCoercionCarrier::Field { node: 7, field },
                CoercionLawViolation::Backward { .. }
            ) if field == "alias"
        ));
        assert!(matches!(
            (&violations[3].carrier, &violations[3].violation),
            (
                GraphCoercionCarrier::Field { node: 7, field },
                CoercionLawViolation::Forward { .. }
            ) if field == "alias"
        ));
    }

    #[test]
    fn declaration_failures_are_reported_once_without_a_carrier_sample() {
        let graph = check_graph_coercion_laws(
            &identity_expr("x"),
            None,
            CoercionClass::Iso,
            &GInstance::new(),
            "x",
        )
        .unwrap();
        assert_eq!(graph.len(), 1);
        assert!(matches!(
            (&graph[0].carrier, &graph[0].violation),
            (
                GraphCoercionCarrier::Declaration,
                CoercionLawViolation::MissingInverse { .. }
            )
        ));

        let table = check_table_coercion_laws(
            &identity_expr("x"),
            None,
            CoercionClass::Iso,
            &FInstance::default(),
            "x",
        )
        .unwrap();
        assert_eq!(table.len(), 1);
        assert!(matches!(
            (&table[0].carrier, &table[0].violation),
            (
                TableCoercionCarrier::Declaration,
                CoercionLawViolation::MissingInverse { .. }
            )
        ));
    }

    #[test]
    fn honest_identity_passes_on_graph_and_table_carriers() {
        let graph = GInstance::new().with_node(
            Node::new(1, "text")
                .with_value(FieldPresence::Present(Value::Str("a".into())))
                .with_extra_field("field", Value::Int(1)),
        );
        assert!(
            check_graph_coercion_laws(
                &identity_expr("x"),
                Some(&identity_expr("x")),
                CoercionClass::Iso,
                &graph,
                "x",
            )
            .unwrap()
            .is_empty()
        );

        let table = FInstance::from_parts(
            [(
                "t".into(),
                vec![(RowId(1), HashMap::from([("c".into(), Value::Int(1))]))],
            )],
            [],
        )
        .unwrap();
        assert!(
            check_table_coercion_laws(
                &identity_expr("x"),
                Some(&identity_expr("x")),
                CoercionClass::Iso,
                &table,
                "x",
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn table_cells_use_shared_algebra_and_retain_location() {
        let row = HashMap::from([("name".to_owned(), Value::Str("Alice".into()))]);
        let table = FInstance::from_parts([("person".into(), vec![(RowId(3), row)])], []).unwrap();
        let violations = check_table_coercion_laws(
            &upper_expr("x"),
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &table,
            "x",
        )
        .unwrap();
        assert_eq!(violations.len(), 2);
        assert!(matches!(
            (&violations[0].carrier, &violations[0].violation),
            (
                TableCoercionCarrier::Cell { table, row: RowId(3), column },
                CoercionLawViolation::Backward { .. }
            ) if table == "person" && column == "name"
        ));
        assert!(matches!(
            (&violations[1].carrier, &violations[1].violation),
            (
                TableCoercionCarrier::Cell { table, row: RowId(3), column },
                CoercionLawViolation::Forward { .. }
            ) if table == "person" && column == "name"
        ));
    }

    #[test]
    fn graph_carrier_rejects_inexpressible_value_with_location() {
        let graph = GInstance::new().with_node(
            Node::new(2, "token").with_value(FieldPresence::Present(Value::Token("red".into()))),
        );
        let error = check_graph_coercion_laws(
            &identity_expr("x"),
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &graph,
            "x",
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CoercionCarrierError::Graph {
                carrier: GraphCoercionCarrier::NodeValue { node: 2 },
                value_type: "token"
            }
        ));
    }

    #[test]
    fn iso_with_honest_identity_passes() {
        // identity forward, identity inverse, declared Iso: both
        // round trips hold for every sample.
        let violations = check_coercion_laws(
            &identity_expr("x"),
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(
            violations.is_empty(),
            "honest identity iso must have no violations, got {violations:?}"
        );
    }

    #[test]
    fn iso_with_lying_identity_inverse_is_flagged() {
        // upper forward, identity inverse, declared Iso: backward law
        // holds only when the input was already uppercase; forward
        // law never holds because inverse(v) = v and forward(v) =
        // upper(v) differ whenever v has lowercase content.
        let forward = upper_expr("x");
        let inverse = identity_expr("x");
        let violations = check_coercion_laws(
            &forward,
            Some(&inverse),
            CoercionClass::Iso,
            &[Literal::Str("Alice".to_owned())],
            "x",
        );
        assert!(
            !violations.is_empty(),
            "lying iso declaration must be flagged",
        );
        // Both directions should surface: upper("Alice") = "ALICE",
        // identity("ALICE") = "ALICE" != "Alice" for backward;
        // identity("Alice") = "Alice", upper("Alice") = "ALICE" !=
        // "Alice" for forward.
        let has_backward = violations
            .iter()
            .any(|v| matches!(v, CoercionLawViolation::Backward { .. }));
        let has_forward = violations
            .iter()
            .any(|v| matches!(v, CoercionLawViolation::Forward { .. }));
        assert!(
            has_backward,
            "expected Backward violation in {violations:?}"
        );
        assert!(has_forward, "expected Forward violation in {violations:?}");
    }

    #[test]
    fn retraction_checks_only_backward_direction() {
        // upper + lower: upper(lower(s)) = upper(s), not always s;
        // but lower(upper(s)) = lower(s), not always s either.
        // Retraction requires only inverse(forward(s)) = s; here we
        // use `upper` as forward, `lower` as inverse, and note that
        // `lower(upper("Alice")) = "alice" != "Alice"`, so the
        // backward law fails.
        let forward = upper_expr("x");
        let inverse = Expr::Builtin(BuiltinOp::Lower, vec![Expr::Var(Arc::from("x"))]);
        let violations = check_coercion_laws(
            &forward,
            Some(&inverse),
            CoercionClass::Retraction,
            &[Literal::Str("Alice".to_owned())],
            "x",
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, CoercionLawViolation::Backward { .. })),
            "retraction backward violation expected, got {violations:?}"
        );
    }

    #[test]
    fn projection_checks_determinism() {
        // A pure upper expression is deterministic: repeated
        // evaluations produce the same result, so Projection
        // validates.
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Projection,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(
            violations.is_empty(),
            "deterministic projection must pass, got {violations:?}"
        );
    }

    #[test]
    fn opaque_declares_no_law_so_always_passes() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Opaque,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(violations.is_empty(), "opaque has no laws to violate");
    }

    #[test]
    fn iso_without_inverse_reports_missing_inverse() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Iso,
            &default_samples_for_string_value(),
            "x",
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            violations[0],
            CoercionLawViolation::MissingInverse {
                class: CoercionClass::Iso,
            }
        ));
    }

    #[test]
    fn retraction_without_inverse_reports_missing_inverse() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Retraction,
            &default_samples_for_string_value(),
            "x",
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            violations[0],
            CoercionLawViolation::MissingInverse {
                class: CoercionClass::Retraction,
            }
        ));
    }

    #[test]
    fn check_directed_equation_matches_explicit_call() {
        // Shortcut delegates to check_coercion_laws with the same
        // arguments; verify parity.
        let forward = upper_expr("x");
        let deq = DirectedEquation {
            name: Arc::from("upper_iso_lying"),
            lhs: panproto_gat::Term::var("x"),
            rhs: panproto_gat::Term::app("upper", vec![panproto_gat::Term::var("x")]),
            impl_term: forward.clone(),
            inverse: Some(identity_expr("x")),
            source_kind: Some(panproto_gat::ValueKind::Str),
            target_kind: Some(panproto_gat::ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        };
        let samples = vec![Literal::Str("Alice".to_owned())];
        let direct = check_coercion_laws(
            &forward,
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &samples,
            "x",
        );
        let via_deq = check_directed_equation_coercion_law(&deq, &samples, "x");
        assert_eq!(direct.len(), via_deq.len());
    }

    fn honest_iso_deq(name: &str) -> DirectedEquation {
        DirectedEquation {
            name: Arc::from(name),
            lhs: panproto_gat::Term::var("x"),
            rhs: panproto_gat::Term::var("x"),
            impl_term: identity_expr("x"),
            inverse: Some(identity_expr("x")),
            source_kind: Some(ValueKind::Str),
            target_kind: Some(ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        }
    }

    fn lying_iso_deq(name: &str) -> DirectedEquation {
        DirectedEquation {
            name: Arc::from(name),
            lhs: panproto_gat::Term::var("x"),
            rhs: panproto_gat::Term::app("upper", vec![panproto_gat::Term::var("x")]),
            impl_term: upper_expr("x"),
            inverse: Some(identity_expr("x")),
            source_kind: Some(ValueKind::Str),
            target_kind: Some(ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        }
    }

    #[test]
    fn registry_defaults_any_union_is_deterministic() {
        // The `Any` bucket must be byte-identical across repeated
        // constructions; without the explicit kind-order iteration it
        // would inherit `FxHashMap`'s nondeterministic iteration.
        let first = CoercionSampleRegistry::with_defaults();
        let first_any: Vec<Literal> = first.samples_for(ValueKind::Any).to_vec();
        for _ in 0..8 {
            let next = CoercionSampleRegistry::with_defaults();
            let next_any: Vec<Literal> = next.samples_for(ValueKind::Any).to_vec();
            assert_eq!(
                first_any, next_any,
                "Any-union must be stable across with_defaults invocations",
            );
        }
        // Leading samples must come from the declared ordering
        // (Bool first): sanity check that the fixed iteration is
        // actually in effect.
        assert!(matches!(first_any.first(), Some(Literal::Bool(false))));
    }

    #[test]
    fn registry_defaults_cover_every_primitive_kind() {
        // `ValueKind::all()` is exhaustiveness-guarded upstream;
        // iterating it here guarantees a new variant is surfaced as
        // a test failure rather than silently passing.
        let reg = CoercionSampleRegistry::with_defaults();
        for kind in ValueKind::all() {
            assert!(
                !reg.samples_for(*kind).is_empty(),
                "kind {kind:?} must have default samples"
            );
        }
        assert!(!reg.is_empty());
    }

    #[test]
    fn registry_check_passes_on_honest_iso() {
        let reg = CoercionSampleRegistry::with_defaults();
        let deq = honest_iso_deq("honest");
        let violations = check_directed_equation_with_registry(&deq, &reg, "x");
        assert!(
            violations.is_empty(),
            "honest iso must pass: {violations:?}"
        );
    }

    #[test]
    fn registry_check_flags_lying_iso() {
        let reg = CoercionSampleRegistry::with_defaults();
        let deq = lying_iso_deq("lying");
        let violations = check_directed_equation_with_registry(&deq, &reg, "x");
        assert!(!violations.is_empty(), "lying iso must be flagged");
    }

    #[test]
    fn registry_check_falls_back_to_any_when_kind_missing() {
        let mut reg = CoercionSampleRegistry::new();
        reg.register(ValueKind::Any, vec![Literal::Str("Alice".to_owned())]);
        let mut deq = lying_iso_deq("lying_untyped");
        deq.source_kind = None;
        let violations = check_directed_equation_with_registry(&deq, &reg, "x");
        assert!(!violations.is_empty());
    }

    #[test]
    fn check_theory_reports_mixed_honesty() {
        let theory = Theory::full(
            "TestCoercionTheory",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![honest_iso_deq("honest"), lying_iso_deq("lying")],
            Vec::new(),
        );
        let reg = CoercionSampleRegistry::with_defaults();
        let report = check_theory(&theory, &reg);
        assert!(!report.is_clean());
        assert_eq!(report.per_equation.len(), 2);
        let (honest_name, honest_violations) = &report.per_equation[0];
        assert_eq!(honest_name.as_ref(), "honest");
        assert!(honest_violations.is_empty());
        let (lying_name, lying_violations) = &report.per_equation[1];
        assert_eq!(lying_name.as_ref(), "lying");
        assert!(!lying_violations.is_empty());
        assert!(report.violation_count() >= 1);
    }

    #[test]
    fn coercion_law_validation_trait_succeeds_on_honest() {
        let reg = CoercionSampleRegistry::with_defaults();
        let deq = honest_iso_deq("honest");
        assert!(deq.validate_coercion_law(&reg, "x").is_ok());
    }

    #[test]
    fn coercion_law_validation_trait_flags_lying() {
        // The trait method's sole contract is the `Result`; both
        // debug and release builds return `Err` on a lying declaration.
        let reg = CoercionSampleRegistry::with_defaults();
        let deq = lying_iso_deq("lying");
        let violations = check_directed_equation_with_registry(&deq, &reg, "x");
        assert!(!violations.is_empty());
        let Err(err) = deq.validate_coercion_law(&reg, "x") else {
            panic!("lying iso must yield Err in every build config");
        };
        assert!(!err.is_empty(), "Err payload must carry the violations");
    }

    #[test]
    fn exhaustive_check_coercion_class() {
        // Exhaustiveness guard for `check_coercion_laws`.
        //
        // `CoercionClass` is `#[non_exhaustive]`, so the checker needs
        // a wildcard arm (currently producing `UnknownClass`). This
        // test enumerates every *known* variant and asserts the
        // checker handles each explicitly, i.e. never returns
        // `UnknownClass` for a variant it ought to recognise.
        //
        // When a new variant is added upstream, this test begins
        // emitting `UnknownClass` and forces an update before merge.
        let forward = identity_expr("x");
        let inverse = Some(identity_expr("x"));
        let samples = [Literal::Str("probe".to_owned())];

        for class in CoercionClass::all() {
            let violations = check_coercion_laws(&forward, inverse.as_ref(), *class, &samples, "x");
            for v in &violations {
                assert!(
                    !matches!(v, CoercionLawViolation::UnknownClass { .. }),
                    "check_coercion_laws must handle {class:?} explicitly; \
                     got UnknownClass in {violations:?}",
                );
            }
        }
    }

    #[test]
    fn eval_error_on_wrong_type_is_reported() {
        // upper on an integer sample: evaluator surfaces a type
        // error; the check reports it rather than silently passing.
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Projection,
            &[Literal::Int(42)],
            "x",
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, CoercionLawViolation::ForwardEvalError { .. })),
            "expected ForwardEvalError, got {violations:?}"
        );
    }
}
