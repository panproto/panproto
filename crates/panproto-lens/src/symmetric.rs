//! Symmetric lenses via span composition.
//!
//! A symmetric lens between schemas S and T is a pair of asymmetric lenses
//! that share a common complement. This module provides the span-based
//! construction where the "middle" schema M serves as the shared state.

use panproto_inst::{SynthIndex, WInstance};
use panproto_mig::hom_search::find_span;
use panproto_schema::{Protocol, Schema, canonical_bytes};

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::auto_lens::AutoLensConfig;
use crate::error::LensError;
use crate::lens_shape::{LensShape, TreeShape};
use crate::protolens::ProtolensChain;

/// A violation of complement coherence in a symmetric lens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoherenceViolation {
    /// Which direction's round-trip caused the violation.
    pub direction: &'static str,
    /// Details about the mismatch.
    pub detail: String,
}

/// A symmetric lens between two schemas, built from a shared middle schema.
///
/// The left leg is a lens from M to S, and the right leg is a lens from M
/// to T. Together they synchronize S and T via the common state M.
pub struct SymmetricLens {
    /// Lens from the middle schema to the left schema.
    pub left: Lens,
    /// Lens from the middle schema to the right schema.
    pub right: Lens,
    /// The shared middle schema.
    pub middle: Schema,
}

/// Describe the first way two candidate middles differ, or `None` when
/// they are the same schema.
///
/// Equality is equality of [`canonical_bytes`], so every schema coordinate
/// represented by the canonical encoding participates in the decision.
fn middle_disagreement(left: &Schema, right: &Schema) -> Option<String> {
    (canonical_bytes(left) != canonical_bytes(right)).then(|| {
        "canonical schema encodings differ; the two legs do not have the same middle".to_owned()
    })
}

pub(crate) fn sync_left_to_right_shape<S: LensShape>(
    lens: &SymmetricLens,
    view: &S::Instance,
    complement: &S::Complement,
    left_context: &S::Context,
    right_context: &S::Context,
) -> Result<(S::Instance, S::Complement), LensError> {
    let middle = S::put(&lens.left, view, complement, left_context)?;
    S::get(&lens.right, &middle, right_context)
}

pub(crate) fn sync_right_to_left_shape<S: LensShape>(
    lens: &SymmetricLens,
    view: &S::Instance,
    complement: &S::Complement,
    left_context: &S::Context,
    right_context: &S::Context,
) -> Result<(S::Instance, S::Complement), LensError> {
    let middle = S::put(&lens.right, view, complement, right_context)?;
    S::get(&lens.left, &middle, left_context)
}

fn observation_divergence<S: LensShape>(
    expected: &(S::Instance, S::Complement),
    actual: &(S::Instance, S::Complement),
) -> Option<String> {
    let expected_index = S::synth_index(&expected.0, &expected.1);
    let actual_index = S::synth_index(&actual.0, &actual.1);
    if !S::equivalent(&expected.0, &expected_index, &actual.0, &actual_index) {
        return Some(
            S::divergence(&expected.0, &expected_index, &actual.0, &actual_index)
                .unwrap_or_else(|| "views are not observationally equivalent".to_owned()),
        );
    }
    S::complement_divergence(&expected.1, &actual.1)
        .map(|detail| format!("complements differ: {detail}"))
}

/// Boundary occupied by the first observation in an alternating sync trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceSide {
    /// The supplied observation is a left-boundary view.
    Left,
    /// The supplied observation is a right-boundary view.
    Right,
}

/// One implementation and its initial observation in a finite sync trace.
pub struct TraceObservation<'a, S: LensShape> {
    /// Symmetric lens being observed.
    pub lens: &'a SymmetricLens,
    /// Shape context for the left leg.
    pub left_context: &'a S::Context,
    /// Shape context for the right leg.
    pub right_context: &'a S::Context,
    /// Initial boundary view, after any stable-keyed in-domain mutation.
    pub view: &'a S::Instance,
    /// Complement paired with `view`.
    pub complement: &'a S::Complement,
}

fn errors_observationally_equal(left: &LensError, right: &LensError) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
        && left.detail() == right.detail()
        && left.to_string() == right.to_string()
}

/// Compare two implementations on a finite alternating synchronization trace.
///
/// Callers apply the same stable-keyed, in-domain mutation to both supplied
/// initial views. At every successful step, both the indexed view and the
/// complete complement are compared. Equal typed refusals at the same step
/// terminate the trace successfully; any other error pair is a mismatch.
///
/// # Errors
///
/// Returns a law violation at the first distinguishable trace observation.
pub fn check_observational_trace<S: LensShape>(
    first: &TraceObservation<'_, S>,
    second: &TraceObservation<'_, S>,
    starting_side: TraceSide,
    steps: usize,
) -> Result<(), crate::error::LawViolation> {
    use crate::error::LawViolation;

    let mut first_state = (first.view.clone(), first.complement.clone());
    let mut second_state = (second.view.clone(), second.complement.clone());
    if let Some(detail) = observation_divergence::<S>(&first_state, &second_state) {
        return Err(LawViolation::PutGet {
            detail: format!("initial trace observations differ: {detail}"),
        });
    }

    let mut side = starting_side;
    for step in 0..steps {
        let first_next = match side {
            TraceSide::Left => sync_left_to_right_shape::<S>(
                first.lens,
                &first_state.0,
                &first_state.1,
                first.left_context,
                first.right_context,
            ),
            TraceSide::Right => sync_right_to_left_shape::<S>(
                first.lens,
                &first_state.0,
                &first_state.1,
                first.left_context,
                first.right_context,
            ),
        };
        let second_next = match side {
            TraceSide::Left => sync_left_to_right_shape::<S>(
                second.lens,
                &second_state.0,
                &second_state.1,
                second.left_context,
                second.right_context,
            ),
            TraceSide::Right => sync_right_to_left_shape::<S>(
                second.lens,
                &second_state.0,
                &second_state.1,
                second.left_context,
                second.right_context,
            ),
        };
        match (first_next, second_next) {
            (Ok(left), Ok(right)) => {
                if let Some(detail) = observation_divergence::<S>(&left, &right) {
                    return Err(LawViolation::PutGet {
                        detail: format!("trace step {step} differs: {detail}"),
                    });
                }
                first_state = left;
                second_state = right;
            }
            (Err(left), Err(right)) if errors_observationally_equal(&left, &right) => return Ok(()),
            (Err(left), Err(right)) => {
                return Err(LawViolation::PutGet {
                    detail: format!("trace step {step} has unequal refusals: {left}; {right}"),
                });
            }
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => {
                return Err(LawViolation::PutGet {
                    detail: format!(
                        "trace step {step} refuses in only one implementation: {error}"
                    ),
                });
            }
        }
        side = match side {
            TraceSide::Left => TraceSide::Right,
            TraceSide::Right => TraceSide::Left,
        };
    }
    Ok(())
}

pub(crate) fn verify_complement_coherence_shape<S: LensShape>(
    lens: &SymmetricLens,
    middle: &S::Instance,
    left_context: &S::Context,
    right_context: &S::Context,
) -> Vec<CoherenceViolation> {
    let mut violations = Vec::new();
    let left = match S::get(&lens.left, middle, left_context) {
        Ok(value) => Some(value),
        Err(error) => {
            violations.push(CoherenceViolation {
                direction: "middle to left",
                detail: format!("get failed: {error}"),
            });
            None
        }
    };
    let right = match S::get(&lens.right, middle, right_context) {
        Ok(value) => Some(value),
        Err(error) => {
            violations.push(CoherenceViolation {
                direction: "middle to right",
                detail: format!("get failed: {error}"),
            });
            None
        }
    };

    if let (Some(left), Some(right)) = (&left, &right) {
        match sync_left_to_right_shape::<S>(lens, &left.0, &left.1, left_context, right_context) {
            Ok(actual) => {
                if let Some(detail) = observation_divergence::<S>(right, &actual) {
                    violations.push(CoherenceViolation {
                        direction: "left to right",
                        detail,
                    });
                }
            }
            Err(error) => violations.push(CoherenceViolation {
                direction: "left to right",
                detail: format!("sync failed: {error}"),
            }),
        }
        match sync_right_to_left_shape::<S>(lens, &right.0, &right.1, left_context, right_context) {
            Ok(actual) => {
                if let Some(detail) = observation_divergence::<S>(left, &actual) {
                    violations.push(CoherenceViolation {
                        direction: "right to left",
                        detail,
                    });
                }
            }
            Err(error) => violations.push(CoherenceViolation {
                direction: "right to left",
                detail: format!("sync failed: {error}"),
            }),
        }
    }
    violations
}

pub(crate) fn check_symmetric_laws_shape<S: LensShape>(
    lens: &SymmetricLens,
    middle: &S::Instance,
    left_context: &S::Context,
    right_context: &S::Context,
) -> Result<(), crate::error::LawViolation> {
    use crate::error::LawViolation;

    let left = S::get(&lens.left, middle, left_context).map_err(LawViolation::Error)?;
    let right = S::get(&lens.right, middle, right_context).map_err(LawViolation::Error)?;

    for (name, leg, observation, context) in [
        ("left", &lens.left, &left, left_context),
        ("right", &lens.right, &right, right_context),
    ] {
        let restored =
            S::put(leg, &observation.0, &observation.1, context).map_err(LawViolation::Error)?;
        let empty_index = SynthIndex::default();
        if !S::equivalent(middle, &empty_index, &restored, &empty_index) {
            return Err(LawViolation::GetPut {
                detail: format!("{name} leg changed the middle instance"),
            });
        }
        let observed_again = S::get(leg, &restored, context).map_err(LawViolation::Error)?;
        if let Some(detail) = observation_divergence::<S>(observation, &observed_again) {
            return Err(LawViolation::GetPut {
                detail: format!("{name} leg observation changed: {detail}"),
            });
        }
    }

    let right_after =
        sync_left_to_right_shape::<S>(lens, &left.0, &left.1, left_context, right_context)
            .map_err(LawViolation::Error)?;
    if let Some(detail) = observation_divergence::<S>(&right, &right_after) {
        return Err(LawViolation::PutGet {
            detail: format!("left-to-right sync equation failed: {detail}"),
        });
    }
    let left_after =
        sync_right_to_left_shape::<S>(lens, &right.0, &right.1, left_context, right_context)
            .map_err(LawViolation::Error)?;
    if let Some(detail) = observation_divergence::<S>(&left, &left_after) {
        return Err(LawViolation::PutGet {
            detail: format!("right-to-left sync equation failed: {detail}"),
        });
    }
    Ok(())
}

impl SymmetricLens {
    /// Create a symmetric lens from two asymmetric lenses that share the
    /// same source schema (the "middle").
    ///
    /// Synchronization puts a view back through one leg and gets through
    /// the other, so the instance one leg produces is read by the other
    /// against the schema it was built for. That only makes sense when
    /// the two source schemas are the same schema, and the check is over
    /// everything the two legs traverse, not just the vertex names: two
    /// schemas that agree on vertices while disagreeing on edges, kinds,
    /// or requiredness would lose arcs on every sync without saying so.
    ///
    /// Equality is exact canonical-schema equality, including coordinates
    /// that affect downstream compilation or diagnostics.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::NoSharedMiddle`] when the canonical source-schema
    /// encodings differ.
    pub fn from_span(left: Lens, right: Lens) -> Result<Self, LensError> {
        if let Some(detail) = middle_disagreement(&left.src_schema, &right.src_schema) {
            return Err(LensError::NoSharedMiddle { detail });
        }
        let middle = left.src_schema.clone();
        Ok(Self {
            left,
            right,
            middle,
        })
    }

    /// Synchronize from left to right: given a left view, produce a right view.
    ///
    /// Puts the left view back into the middle, then gets the right view.
    ///
    /// # Errors
    ///
    /// Returns `LensError` if either the put or get operation fails.
    pub fn sync_left_to_right(
        &self,
        left_view: &WInstance,
        left_complement: &Complement,
    ) -> Result<(WInstance, Complement), LensError> {
        let middle_instance = put(&self.left, left_view, left_complement)?;
        get(&self.right, &middle_instance)
    }

    /// Synchronize from right to left: given a right view, produce a left view.
    ///
    /// Puts the right view back into the middle, then gets the left view.
    ///
    /// # Errors
    ///
    /// Returns `LensError` if either the put or get operation fails.
    pub fn sync_right_to_left(
        &self,
        right_view: &WInstance,
        right_complement: &Complement,
    ) -> Result<(WInstance, Complement), LensError> {
        let middle_instance = put(&self.right, right_view, right_complement)?;
        get(&self.left, &middle_instance)
    }

    /// Build a symmetric lens from two protolens chains via a shared overlap.
    ///
    /// Each chain is instantiated at `overlap_schema` to produce left and
    /// right asymmetric lenses, which are then combined into a span.
    ///
    /// # Errors
    ///
    /// Returns [`LensError`] if either chain fails to instantiate or the
    /// resulting source schemas do not match.
    pub fn from_protolens_chains(
        left_chain: &ProtolensChain,
        right_chain: &ProtolensChain,
        overlap_schema: &Schema,
        protocol: &Protocol,
    ) -> Result<Self, LensError> {
        let left_lens = left_chain.instantiate(overlap_schema, protocol)?;
        let right_lens = right_chain.instantiate(overlap_schema, protocol)?;
        Self::from_span(left_lens, right_lens)
    }

    /// Verify complement coherence for this symmetric lens on a given
    /// middle instance.
    ///
    /// Complement coherence requires that round-tripping through one
    /// direction does not disturb the observation on the other direction:
    ///
    /// 1. Get left and right views with complements from the middle instance.
    /// 2. Put the right view back to get a restored middle instance.
    /// 3. Get the left view from the restored middle.
    /// 4. The indexed left view and every independent complement coordinate
    ///    must be stable; an operational refusal is reported as a violation.
    /// 5. Repeat symmetrically for the other direction.
    ///
    /// Returns a list of violations (empty means coherent).
    #[must_use]
    pub fn verify_complement_coherence(
        &self,
        middle_instance: &WInstance,
    ) -> Vec<CoherenceViolation> {
        verify_complement_coherence_shape::<TreeShape>(self, middle_instance, &(), &())
    }

    /// Check the symmetric-lens round-trip laws on a given middle
    /// instance. Each leg must individually satisfy `GetPut`; in
    /// addition, the *consistency relation* between the two legs (the
    /// span's witness that `(left_view, right_view)` arose from a
    /// shared middle) must be stable under one-sided round-trips.
    ///
    /// This is the Hofmann/Pierce / Diskin-Xiong-Czarnecki form
    /// adapted to span-based symmetric lenses: rather than parameterise
    /// over an explicit consistency relation, we use the span's middle
    /// as the witness — two views are consistent iff they `get` from a
    /// common middle, and stability requires that putting one side
    /// back and re-getting the other produces an equivalent view.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::LawViolation`] for the first failure observed.
    pub fn check_symmetric_laws(
        &self,
        middle_instance: &WInstance,
    ) -> Result<(), crate::error::LawViolation> {
        check_symmetric_laws_shape::<TreeShape>(self, middle_instance, &(), &())
    }

    /// Auto-generate a symmetric lens from two schemas.
    ///
    /// Runs one span search on the iso path and takes its apex as the middle
    /// schema, then builds a protolens chain for each projection.
    ///
    /// # Why the apex rather than a hand-assembled restriction
    ///
    /// The middle schema is the apex of a span, and a span's apex is the
    /// sub-schema of the source *induced* on the chosen vertices. Inducing
    /// carries the non-edge structure and rebuilds the adjacency indices;
    /// selecting vertices and edges by hand carries neither, so a middle
    /// schema assembled that way answers every adjacency query with nothing
    /// even though its edge map is populated, and silently drops entries,
    /// required sets, variants and recursion points.
    ///
    /// The iso path is the one that applies: a merge along the middle needs the
    /// right leg to be a mono, and only that path guarantees it.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if the search fails, if the two
    /// schemas share no induced sub-schema, or if automatic lens generation
    /// fails for either direction.
    pub fn auto_symmetric(
        left: &Schema,
        right: &Schema,
        protocol: &Protocol,
        config: &AutoLensConfig,
    ) -> Result<Self, LensError> {
        let mut opts = config.search_opts.clone();
        opts.iso = true;
        let span = find_span(left, right, protocol, &opts)
            .map_err(|e| LensError::ProtolensError(format!("overlap search failed: {e}")))?;

        if span.apex.vertices.is_empty() {
            return Err(LensError::ProtolensError(
                "no overlap found between schemas".into(),
            ));
        }
        let overlap_schema = span.apex;

        // Generate protolens chains: overlap -> left and overlap -> right
        let left_result = crate::auto_lens::auto_generate(&overlap_schema, left, protocol, config);
        let right_result =
            crate::auto_lens::auto_generate(&overlap_schema, right, protocol, config);

        match (left_result, right_result) {
            (Ok(lr), Ok(rr)) => Self::from_span(lr.lens, rr.lens),
            (Err(e), _) | (_, Err(e)) => Err(LensError::ProtolensError(format!(
                "auto_symmetric failed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_schema};
    use panproto_expr::{Expr, Literal};
    use panproto_gat::{CoercionClass, Name};
    use panproto_schema::{
        CoercionSpec, Constraint, HyperEdge, Key, KeyRole, KeyScope, RecursionPoint, Variant,
    };

    #[test]
    fn from_span_identical_schemas() {
        let schema = three_node_schema();
        let left = identity_lens(&schema);
        let right = identity_lens(&schema);
        let sym = SymmetricLens::from_span(left, right).unwrap();
        assert_eq!(sym.middle.vertices.len(), schema.vertices.len());
    }

    #[test]
    fn legs_that_agree_only_on_vertex_names_do_not_share_a_middle() {
        // Both middles carry the same three vertices; one of them has no
        // edges. Synchronizing puts a view back through one leg and reads
        // the result through the other, so a middle instance built to the
        // edgeless schema loses every arc when the other leg reads it.
        let schema = three_node_schema();
        let mut edgeless = schema.clone();
        edgeless.edges.clear();
        edgeless.outgoing.clear();
        edgeless.incoming.clear();
        edgeless.between.clear();
        assert_eq!(
            edgeless
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>(),
            schema
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>(),
            "the two middles must agree on vertex names for this test to mean anything",
        );

        let left = identity_lens(&schema);
        let right = identity_lens(&edgeless);
        match SymmetricLens::from_span(left, right) {
            Err(LensError::NoSharedMiddle { detail }) => {
                assert!(detail.contains("canonical schema encodings"), "{detail}");
            }
            other => panic!(
                "legs whose middles differ in their edges must be refused, got {:?}",
                other.map(|_| ()),
            ),
        }
    }

    fn middle_schema_variants_one(schema: &Schema) -> Vec<(&'static str, Schema)> {
        let mut variants = Vec::new();

        let mut protocol = schema.clone();
        protocol.protocol = "other".into();
        variants.push(("protocol", protocol));

        let mut vertices = schema.clone();
        vertices.vertices.get_mut("post:body").unwrap().kind = "record".into();
        variants.push(("vertices", vertices));

        let edge = schema.edges.keys().next().unwrap().clone();
        let mut edges = schema.clone();
        edges.edges.remove(&edge);
        variants.push(("edges", edges));

        let mut hyper_edges = schema.clone();
        hyper_edges.hyper_edges.insert(
            "fan".into(),
            HyperEdge {
                id: "fan".into(),
                kind: "relation".into(),
                signature: std::iter::once((Name::from("parent"), Name::from("post:body")))
                    .collect(),
                parent_label: "parent".into(),
            },
        );
        variants.push(("hyper_edges", hyper_edges));

        let mut constraints = schema.clone();
        constraints.constraints.insert(
            "post:body".into(),
            vec![Constraint {
                sort: "required".into(),
                value: "true".into(),
            }],
        );
        variants.push(("constraints", constraints));

        let mut required = schema.clone();
        required
            .required
            .insert("post:body".into(), vec![edge.clone()]);
        variants.push(("required", required));

        let mut entries = schema.clone();
        entries.entries.push("root".into());
        variants.push(("entries", entries));

        let mut nsids = schema.clone();
        nsids.nsids.insert("root".into(), "app.test.root".into());
        variants.push(("nsids", nsids));

        let mut variants_field = schema.clone();
        variants_field.variants.insert(
            "post:body".into(),
            vec![Variant {
                id: "post:body.text".into(),
                parent_vertex: "post:body".into(),
                tag: Some("text".into()),
            }],
        );
        variants.push(("variants", variants_field));

        variants
    }

    fn middle_schema_variants_two(schema: &Schema) -> Vec<(&'static str, Schema)> {
        let mut variants = Vec::new();
        let edge = schema.edges.keys().next().unwrap().clone();

        let mut orderings = schema.clone();
        orderings.orderings.insert(edge.clone(), 1);
        variants.push(("orderings", orderings));

        let mut recursion_points = schema.clone();
        recursion_points.recursion_points.insert(
            "post:body".into(),
            RecursionPoint {
                target_vertex: "post:body".into(),
            },
        );
        variants.push(("recursion_points", recursion_points));

        let mut spans = schema.clone();
        spans.spans.insert(
            "root-span".into(),
            panproto_schema::Span {
                id: "root-span".into(),
                left: "root".into(),
                right: "root".into(),
            },
        );
        variants.push(("spans", spans));

        let mut affine = schema.clone();
        affine
            .usage_modes
            .insert(edge, panproto_schema::UsageMode::Affine);
        variants.push(("usage_modes", affine));

        let mut nominal = schema.clone();
        nominal.nominal.insert("post:body".into(), true);
        variants.push(("nominal", nominal));

        let mut coercions = schema.clone();
        coercions.coercions.insert(
            ("object".into(), "string".into()),
            CoercionSpec {
                forward: Expr::Lit(Literal::Null),
                inverse: None,
                class: CoercionClass::Retraction,
            },
        );
        variants.push(("coercions", coercions));

        let mut mergers = schema.clone();
        mergers
            .mergers
            .insert("post:body".into(), Expr::Lit(Literal::Int(1)));
        variants.push(("mergers", mergers));

        let mut defaults = schema.clone();
        defaults
            .defaults
            .insert("post:body.text".into(), Expr::Lit(Literal::Null));
        variants.push(("defaults", defaults));

        let mut policies = schema.clone();
        policies
            .policies
            .insert("required".into(), Expr::Lit(Literal::Bool(true)));
        variants.push(("policies", policies));

        variants
    }

    fn middle_schema_variants_three(schema: &Schema) -> Vec<(&'static str, Schema)> {
        let mut variants = Vec::new();

        let mut outgoing = schema.clone();
        outgoing.outgoing.remove("post:body");
        variants.push(("outgoing", outgoing));

        let mut incoming = schema.clone();
        incoming.incoming.remove("post:body.text");
        variants.push(("incoming", incoming));

        let mut between = schema.clone();
        between
            .between
            .remove(&(Name::from("post:body"), Name::from("post:body.text")));
        variants.push(("between", between));

        let mut declarations = schema.clone();
        declarations.declarations.insert_key(
            "post:body".into(),
            Key {
                id: "primary".into(),
                parts: Vec::new(),
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        );
        variants.push(("declarations", declarations));

        variants
    }

    #[test]
    fn canonical_middle_equality_observes_formerly_omitted_coordinates() {
        let schema = three_node_schema();
        let variants = middle_schema_variants_one(&schema)
            .into_iter()
            .chain(middle_schema_variants_two(&schema))
            .chain(middle_schema_variants_three(&schema));

        for (field, distinct_middle) in variants {
            assert!(
                matches!(
                    SymmetricLens::from_span(
                        identity_lens(&schema),
                        identity_lens(&distinct_middle)
                    ),
                    Err(LensError::NoSharedMiddle { .. })
                ),
                "canonical middle comparison ignored {field}"
            );
        }
    }

    #[test]
    fn identity_lens_complement_coherent() {
        let schema = three_node_schema();
        let left = identity_lens(&schema);
        let right = identity_lens(&schema);
        let sym = SymmetricLens::from_span(left, right).unwrap();

        // Create a minimal middle instance to test coherence.
        let middle_instance = crate::tests::three_node_instance();
        let violations = sym.verify_complement_coherence(&middle_instance);
        assert!(
            violations.is_empty(),
            "identity lens should be complement-coherent, got violations: {violations:?}"
        );
    }

    #[test]
    fn identity_symmetric_lens_satisfies_laws() {
        let schema = three_node_schema();
        let left = identity_lens(&schema);
        let right = identity_lens(&schema);
        let sym = SymmetricLens::from_span(left, right).unwrap();
        let middle_instance = crate::tests::three_node_instance();
        sym.check_symmetric_laws(&middle_instance)
            .expect("identity symmetric lens should satisfy laws");
    }

    #[test]
    fn finite_alternating_trace_observes_every_state() {
        let schema = three_node_schema();
        let first =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let second =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let middle = crate::tests::three_node_instance();
        let first_state = get(&first.left, &middle).unwrap();
        let second_state = get(&second.left, &middle).unwrap();
        check_observational_trace::<TreeShape>(
            &TraceObservation {
                lens: &first,
                left_context: &(),
                right_context: &(),
                view: &first_state.0,
                complement: &first_state.1,
            },
            &TraceObservation {
                lens: &second,
                left_context: &(),
                right_context: &(),
                view: &second_state.0,
                complement: &second_state.1,
            },
            TraceSide::Left,
            6,
        )
        .unwrap();
    }

    #[test]
    fn equal_typed_refusals_at_the_same_step_are_observationally_equal() {
        let schema = three_node_schema();
        let first =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let second =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let middle = crate::tests::three_node_instance();
        let mut first_state = get(&first.left, &middle).unwrap();
        let mut second_state = get(&second.left, &middle).unwrap();
        first_state.1.identity.as_mut().unwrap().version += 1;
        second_state.1.identity.as_mut().unwrap().version += 1;

        check_observational_trace::<TreeShape>(
            &TraceObservation {
                lens: &first,
                left_context: &(),
                right_context: &(),
                view: &first_state.0,
                complement: &first_state.1,
            },
            &TraceObservation {
                lens: &second,
                left_context: &(),
                right_context: &(),
                view: &second_state.0,
                complement: &second_state.1,
            },
            TraceSide::Left,
            2,
        )
        .unwrap();
    }

    #[test]
    fn a_one_sided_refusal_is_observationally_distinguishable() {
        let schema = three_node_schema();
        let first =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let second =
            SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let middle = crate::tests::three_node_instance();
        let first_state = get(&first.left, &middle).unwrap();
        let mut second_state = get(&second.left, &middle).unwrap();
        second_state.1.identity.as_mut().unwrap().version += 1;

        let error = check_observational_trace::<TreeShape>(
            &TraceObservation {
                lens: &first,
                left_context: &(),
                right_context: &(),
                view: &first_state.0,
                complement: &first_state.1,
            },
            &TraceObservation {
                lens: &second,
                left_context: &(),
                right_context: &(),
                view: &second_state.0,
                complement: &second_state.1,
            },
            TraceSide::Left,
            2,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("refuses in only one implementation")
        );
    }

    #[test]
    fn complement_comparison_is_not_a_count_only_check() {
        let instance = crate::tests::three_node_instance();
        let mut first = Complement::default();
        let mut second = Complement::default();
        let mut nodes = instance.nodes.values();
        let left = nodes.next().unwrap().clone();
        let right = nodes.next().unwrap().clone();
        first.dropped_nodes.insert(left.id, left);
        second.dropped_nodes.insert(right.id, right);
        let detail =
            observation_divergence::<TreeShape>(&(instance.clone(), first), &(instance, second));
        assert_eq!(
            detail.as_deref(),
            Some("complements differ: dropped_nodes differ")
        );
    }

    #[test]
    fn coherence_reports_get_failures() {
        let schema = three_node_schema();
        let mut broken = identity_lens(&schema);
        broken.compiled.surviving_verts.clear();
        let symmetric = SymmetricLens::from_span(broken, identity_lens(&schema)).unwrap();
        let violations =
            symmetric.verify_complement_coherence(&crate::tests::three_node_instance());
        assert!(
            violations
                .iter()
                .any(|violation| violation.direction == "middle to left"),
            "get failure must be an observable violation: {violations:?}"
        );
    }

    #[test]
    fn from_protolens_empty_chains() {
        let schema = three_node_schema();
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Default::default()
        };
        let left_chain = ProtolensChain::new(vec![]);
        let right_chain = ProtolensChain::new(vec![]);
        let sym =
            SymmetricLens::from_protolens_chains(&left_chain, &right_chain, &schema, &protocol)
                .unwrap();
        assert_eq!(sym.middle.vertices.len(), schema.vertices.len());
    }

    // -------------------------------------------------------------------
    // `auto_symmetric`
    // -------------------------------------------------------------------

    fn auto_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "record".into(),
                "string".into(),
                "alpha".into(),
                "beta".into(),
            ],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    fn one_field_record(protocol: &Protocol, root: &str, field: &str, label: &str) -> Schema {
        panproto_schema::SchemaBuilder::new(protocol)
            .vertex(root, "record", None::<&str>)
            .unwrap()
            .vertex(field, "string", None::<&str>)
            .unwrap()
            .edge(root, field, "prop", Some(label))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A schema spans itself, and the middle is the whole of it.
    #[test]
    fn auto_symmetric_spans_a_schema_with_itself() {
        let protocol = auto_protocol();
        let schema = one_field_record(&protocol, "post", "post.text", "text");
        let sym =
            SymmetricLens::auto_symmetric(&schema, &schema, &protocol, &AutoLensConfig::default())
                .unwrap();
        assert_eq!(
            sym.middle.vertices.len(),
            schema.vertices.len(),
            "a schema shares all of itself, so the apex is the whole schema"
        );
    }

    /// Two schemas that share no vertex *name* still share structure, and the
    /// span search finds it.
    ///
    /// This is the case that separates the span search from the overlap
    /// discovery it replaced. `discover_overlap` matched on names and reported
    /// nothing here; the span search minimises a cost function in which pairing
    /// two kind-compatible records beats dropping both, so it answers with the
    /// sub-schema they do share and `auto_symmetric` builds a lens over it.
    #[test]
    fn auto_symmetric_finds_a_middle_when_only_the_names_differ() {
        let protocol = auto_protocol();
        let left = one_field_record(&protocol, "post", "post.text", "text");
        let right = one_field_record(&protocol, "profile", "profile.avatar", "avatar");
        let sym =
            SymmetricLens::auto_symmetric(&left, &right, &protocol, &AutoLensConfig::default())
                .unwrap();
        assert!(
            !sym.middle.vertices.is_empty(),
            "the two records are kind-compatible, so the optimal apex is not empty"
        );
    }

    /// Refusal is reachable, and reaching it takes schemas that share no *kind*.
    ///
    /// An empty apex is always feasible, so this is the only shape that still
    /// produces one: every pairing is infeasible and dropping everything is
    /// what is left.
    #[test]
    fn auto_symmetric_refuses_when_the_two_schemas_share_no_kind() {
        let protocol = auto_protocol();
        let left = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("x", "alpha", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let right = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("y", "beta", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let outcome =
            SymmetricLens::auto_symmetric(&left, &right, &protocol, &AutoLensConfig::default());
        let Err(err) = outcome else {
            panic!("no kind is shared, so there is no middle schema to hang the legs off");
        };
        assert!(
            err.to_string().contains("no overlap found"),
            "an empty apex must be reported as an absent overlap, got: {err}"
        );
    }

    #[test]
    fn auto_symmetric_honors_its_config_during_the_shared_search() {
        let protocol = auto_protocol();
        let schema = one_field_record(&protocol, "post", "post.text", "text");
        let mut config = AutoLensConfig::default();
        config
            .search_opts
            .hard_pins
            .insert("post".into(), "post.text".into());
        config
            .search_opts
            .hard_pins
            .insert("post.text".into(), "post".into());

        let outcome = SymmetricLens::auto_symmetric(&schema, &schema, &protocol, &config);
        assert!(
            matches!(outcome, Err(LensError::ProtolensError(ref detail)) if detail.contains("no overlap found")),
            "kind-incompatible hard pins must constrain the shared-apex search"
        );
    }
}
