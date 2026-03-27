//! Symmetric lenses via span composition.
//!
//! A symmetric lens between schemas S and T is a pair of asymmetric lenses
//! that share a common complement. This module provides the span-based
//! construction where the "middle" schema M serves as the shared state.

use std::collections::HashMap;

use panproto_inst::WInstance;
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

    /// Auto-generate a symmetric lens from two schemas.
    ///
    /// Uses overlap discovery to find shared structure, then builds
    /// protolens chains for each projection.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if no overlap is found or
    /// if automatic lens generation fails for either direction.
    pub fn auto_symmetric(
        left: &Schema,
        right: &Schema,
        protocol: &Protocol,
        _config: &AutoLensConfig,
    ) -> Result<Self, LensError> {
        use panproto_mig::overlap::discover_overlap;

        let overlap = discover_overlap(left, right);

        if overlap.vertex_pairs.is_empty() {
            return Err(LensError::ProtolensError(
                "no overlap found between schemas".into(),
            ));
        }

        // Build the overlap schema from the left schema restricted to
        // overlapping vertices.
        let mut overlap_vertices = HashMap::new();
        let mut overlap_edges = HashMap::new();
        for (src_id, _tgt_id) in &overlap.vertex_pairs {
            if let Some(v) = left.vertices.get(src_id) {
                overlap_vertices.insert(src_id.clone(), v.clone());
            }
        }
        // Edges where both endpoints are in the overlap
        for (edge, kind) in &left.edges {
            if overlap_vertices.contains_key(&edge.src) && overlap_vertices.contains_key(&edge.tgt)
            {
                overlap_edges.insert(edge.clone(), kind.clone());
            }
        }

        let overlap_schema = Schema {
            protocol: left.protocol.clone(),
            vertices: overlap_vertices,
            edges: overlap_edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };

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
#[allow(clippy::unwrap_used)]
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
}
