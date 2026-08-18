//! Symmetric lenses via span composition.
//!
//! A symmetric lens between schemas S and T is a pair of asymmetric lenses
//! that share a common complement. This module provides the span-based
//! construction where the "middle" schema M serves as the shared state.

use panproto_inst::WInstance;
use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_schema::{Protocol, Schema};

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::auto_lens::AutoLensConfig;
use crate::error::LensError;
use crate::protolens::ProtolensChain;

/// A violation of complement coherence in a symmetric lens.
#[derive(Debug)]
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

impl SymmetricLens {
    /// Create a symmetric lens from two asymmetric lenses that share the
    /// same source schema (the "middle").
    ///
    /// # Errors
    ///
    /// Returns `LensError::CompositionMismatch` if the source schemas of
    /// the two lenses do not match.
    pub fn from_span(left: Lens, right: Lens) -> Result<Self, LensError> {
        // Verify that both lenses have the same source schema (middle)
        if left.src_schema.protocol != right.src_schema.protocol
            || left.src_schema.vertex_count() != right.src_schema.vertex_count()
        {
            return Err(LensError::CompositionMismatch);
        }
        // Check that vertex IDs match exactly
        if left
            .src_schema
            .vertices
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
            != right
                .src_schema
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
        {
            return Err(LensError::CompositionMismatch);
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
    /// direction does not disturb the complement of the other direction:
    ///
    /// 1. Get left and right views with complements from the middle instance.
    /// 2. Put the right view back to get a restored middle instance.
    /// 3. Get the left view from the restored middle.
    /// 4. The left complement must be stable (same dropped node count).
    /// 5. Repeat symmetrically for the other direction.
    ///
    /// Returns a list of violations (empty means coherent).
    #[must_use]
    pub fn verify_complement_coherence(
        &self,
        middle_instance: &WInstance,
    ) -> Vec<CoherenceViolation> {
        let mut violations = Vec::new();

        // Forward: left -> right -> left, check left complement stability.
        if let Ok((left_view, left_complement)) = get(&self.left, middle_instance) {
            if let Ok((right_view, right_complement)) = get(&self.right, middle_instance) {
                // Round-trip through right.
                if let Ok(middle_restored) = put(&self.right, &right_view, &right_complement) {
                    if let Ok((_left_view_2, left_complement_2)) = get(&self.left, &middle_restored)
                    {
                        if left_complement.dropped_nodes.len()
                            != left_complement_2.dropped_nodes.len()
                        {
                            violations.push(CoherenceViolation {
                                direction: "right round-trip disturbs left complement",
                                detail: format!(
                                    "left complement dropped nodes: {} before, {} after",
                                    left_complement.dropped_nodes.len(),
                                    left_complement_2.dropped_nodes.len()
                                ),
                            });
                        }
                    }
                }

                // Round-trip through left.
                if let Ok(middle_restored) = put(&self.left, &left_view, &left_complement) {
                    if let Ok((_right_view_2, right_complement_2)) =
                        get(&self.right, &middle_restored)
                    {
                        if right_complement.dropped_nodes.len()
                            != right_complement_2.dropped_nodes.len()
                        {
                            violations.push(CoherenceViolation {
                                direction: "left round-trip disturbs right complement",
                                detail: format!(
                                    "right complement dropped nodes: {} before, {} after",
                                    right_complement.dropped_nodes.len(),
                                    right_complement_2.dropped_nodes.len()
                                ),
                            });
                        }
                    }
                }
            }
        }

        violations
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
        use crate::error::LawViolation;
        use crate::laws::instances_equivalent;

        // Per-leg GetPut.
        crate::laws::check_get_put(&self.left, middle_instance)?;
        crate::laws::check_get_put(&self.right, middle_instance)?;

        let (left_view, left_complement) =
            get(&self.left, middle_instance).map_err(LawViolation::Error)?;
        let (right_view, right_complement) =
            get(&self.right, middle_instance).map_err(LawViolation::Error)?;

        // Stability of right view under a left-side round-trip.
        let middle_after_left =
            put(&self.left, &left_view, &left_complement).map_err(LawViolation::Error)?;
        let (right_view_after, _) =
            get(&self.right, &middle_after_left).map_err(LawViolation::Error)?;
        if !instances_equivalent(&right_view, &right_view_after) {
            return Err(LawViolation::PutGet {
                detail: format!(
                    "right view drift after left round-trip: {} vs {} nodes",
                    right_view.node_count(),
                    right_view_after.node_count(),
                ),
            });
        }

        // Stability of left view under a right-side round-trip.
        let middle_after_right =
            put(&self.right, &right_view, &right_complement).map_err(LawViolation::Error)?;
        let (left_view_after, _) =
            get(&self.left, &middle_after_right).map_err(LawViolation::Error)?;
        if !instances_equivalent(&left_view, &left_view_after) {
            return Err(LawViolation::PutGet {
                detail: format!(
                    "left view drift after right round-trip: {} vs {} nodes",
                    left_view.node_count(),
                    left_view_after.node_count(),
                ),
            });
        }
        Ok(())
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
        _config: &AutoLensConfig,
    ) -> Result<Self, LensError> {
        let opts = SearchOptions {
            iso: true,
            ..SearchOptions::default()
        };
        let span = find_span(left, right, protocol, &opts)
            .map_err(|e| LensError::ProtolensError(format!("overlap search failed: {e}")))?;

        if span.apex.vertices.is_empty() {
            return Err(LensError::ProtolensError(
                "no overlap found between schemas".into(),
            ));
        }
        let overlap_schema = span.apex;

        // Generate protolens chains: overlap -> left and overlap -> right
        let config = AutoLensConfig::default();
        let left_result = crate::auto_lens::auto_generate(&overlap_schema, left, protocol, &config);
        let right_result =
            crate::auto_lens::auto_generate(&overlap_schema, right, protocol, &config);

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

    #[test]
    fn from_span_identical_schemas() {
        let schema = three_node_schema();
        let left = identity_lens(&schema);
        let right = identity_lens(&schema);
        let sym = SymmetricLens::from_span(left, right).unwrap();
        assert_eq!(sym.middle.vertices.len(), schema.vertices.len());
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
    fn from_protolens_empty_chains() {
        let schema = three_node_schema();
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
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
            ..Protocol::default()
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
}
