//! Weighted lens graph with Floyd-Warshall shortest paths.
//!
//! This module models a collection of schemas as a Lawvere metric space:
//! objects are schemas, hom-values are complement costs, composition is
//! addition, and identity cost is 0. The [`LensGraph`] computes shortest
//! paths via Floyd-Warshall, enabling "preferred path" queries that find
//! the cheapest chain of protolenses between any two schemas.

use panproto_gat::Name;
use rustc_hash::FxHashMap;

use crate::cost::chain_cost;
use crate::protolens::{Protolens, ProtolensChain};

/// A weighted directed graph of schemas with lens costs.
///
/// This is a Lawvere metric space: objects are schemas,
/// hom-values are costs, composition is addition,
/// identity cost is 0.
pub struct LensGraph {
    schemas: Vec<Name>,
    schema_index: FxHashMap<Name, usize>,
    edges: FxHashMap<(usize, usize), (f64, ProtolensChain)>,
    distances: Option<Vec<Vec<f64>>>,
    next: Option<Vec<Vec<Option<usize>>>>,
}

impl LensGraph {
    /// Create a new empty lens graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schemas: Vec::new(),
            schema_index: FxHashMap::default(),
            edges: FxHashMap::default(),
            distances: None,
            next: None,
        }
    }

    /// Add a schema to the graph, returning its index.
    ///
    /// If the schema is already present, returns the existing index.
    pub fn add_schema(&mut self, name: Name) -> usize {
        if let Some(&idx) = self.schema_index.get(&name) {
            return idx;
        }
        let idx = self.schemas.len();
        self.schema_index.insert(name.clone(), idx);
        self.schemas.push(name);
        idx
    }

    /// Add a lens (protolens chain) between two schemas.
    ///
    /// Schemas are auto-added if not already present. The chain's cost
    /// is computed, and the edge is only stored if its cost is strictly
    /// cheaper than any existing edge between the same pair. Adding a
    /// lens invalidates any previously computed distances.
    pub fn add_lens(&mut self, src: &Name, tgt: &Name, chain: ProtolensChain) {
        let src_idx = self.add_schema(src.clone());
        let tgt_idx = self.add_schema(tgt.clone());
        let cost = chain_cost(&chain);

        let key = (src_idx, tgt_idx);
        let dominated = self
            .edges
            .get(&key)
            .is_some_and(|(existing_cost, _)| *existing_cost <= cost);

        if !dominated {
            self.edges.insert(key, (cost, chain));
            // Invalidate cached shortest paths.
            self.distances = None;
            self.next = None;
        }
    }

    /// Compute all-pairs shortest distances via Floyd-Warshall.
    ///
    /// After calling this, [`preferred_path`](Self::preferred_path) and
    /// [`distance`](Self::distance) return meaningful results.
    pub fn compute_distances(&mut self) {
        let n = self.schemas.len();
        let mut dist = vec![vec![f64::INFINITY; n]; n];
        let mut next: Vec<Vec<Option<usize>>> = vec![vec![None; n]; n];

        // Identity: d[i][i] = 0
        for (i, row) in dist.iter_mut().enumerate() {
            row[i] = 0.0;
        }

        // Direct edges
        for (&(i, j), (cost, _)) in &self.edges {
            dist[i][j] = *cost;
            next[i][j] = Some(j);
        }

        // Floyd-Warshall relaxation
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    let via_k = dist[i][k] + dist[k][j];
                    if via_k < dist[i][j] {
                        dist[i][j] = via_k;
                        next[i][j] = next[i][k];
                    }
                }
            }
        }

        self.distances = Some(dist);
        self.next = Some(next);
    }

    /// Find the preferred (cheapest) path between two schemas.
    ///
    /// Returns the total cost and a composed [`ProtolensChain`] along
    /// the shortest path. Returns `None` if no path exists or if
    /// distances have not been computed.
    #[must_use]
    pub fn preferred_path(&self, src: &Name, tgt: &Name) -> Option<(f64, ProtolensChain)> {
        let dist = self.distances.as_ref()?;
        let next_matrix = self.next.as_ref()?;

        let &src_idx = self.schema_index.get(src)?;
        let &tgt_idx = self.schema_index.get(tgt)?;

        if src_idx == tgt_idx {
            return Some((0.0, ProtolensChain::new(vec![])));
        }

        let d = dist[src_idx][tgt_idx];
        if d.is_infinite() {
            return None;
        }

        // Reconstruct the path from the next-hop matrix.
        let mut steps: Vec<Protolens> = Vec::new();
        let mut current = src_idx;
        while current != tgt_idx {
            let hop = next_matrix[current][tgt_idx]?;
            let (_, chain) = self.edges.get(&(current, hop))?;
            steps.extend(chain.steps.iter().cloned());
            current = hop;
        }

        Some((d, ProtolensChain::new(steps)))
    }

    /// Return the shortest distance between two schemas.
    ///
    /// Returns [`f64::INFINITY`] if no path exists, the schemas are
    /// unknown, or distances have not been computed.
    #[must_use]
    pub fn distance(&self, src: &Name, tgt: &Name) -> f64 {
        let Some(dist) = &self.distances else {
            return f64::INFINITY;
        };
        let Some(&i) = self.schema_index.get(src) else {
            return f64::INFINITY;
        };
        let Some(&j) = self.schema_index.get(tgt) else {
            return f64::INFINITY;
        };
        dist[i][j]
    }

    /// Number of schemas in the graph.
    #[must_use]
    pub fn schema_count(&self) -> usize {
        self.schemas.len()
    }

    /// Verify that the distance matrix satisfies the Lawvere metric axioms.
    ///
    /// Checks:
    /// 1. `d(A, A) = 0` for all schemas A (identity).
    /// 2. `d(A, C) <= d(A, B) + d(B, C)` for all triples (triangle inequality).
    ///
    /// Must be called after [`compute_distances()`](Self::compute_distances).
    /// Returns an empty list if the metric axioms hold (they always do when
    /// distances are computed via Floyd-Warshall, but this serves as a
    /// correctness assertion and documentation).
    #[must_use]
    pub fn verify_metric(&self) -> Vec<MetricViolation> {
        let mut violations = Vec::new();

        let Some(dist) = &self.distances else {
            return violations;
        };

        let n = self.schemas.len();

        // Axiom 1: d(A, A) = 0
        for (i, row) in dist.iter().enumerate().take(n) {
            if row[i].abs() > f64::EPSILON {
                violations.push(MetricViolation::IdentityNonZero {
                    schema: self.schemas[i].clone(),
                    cost: row[i],
                });
            }
        }

        // Axiom 2: triangle inequality
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let d_ik = dist[i][k];
                    let d_ij_plus_d_jk = dist[i][j] + dist[j][k];
                    if d_ik > d_ij_plus_d_jk + f64::EPSILON {
                        violations.push(MetricViolation::TriangleInequality {
                            x: self.schemas[i].clone(),
                            y: self.schemas[j].clone(),
                            z: self.schemas[k].clone(),
                            d_xz: d_ik,
                            d_xy_plus_d_yz: d_ij_plus_d_jk,
                        });
                    }
                }
            }
        }

        violations
    }
}

/// A violation of the Lawvere metric axioms.
#[derive(Debug)]
pub enum MetricViolation {
    /// Self-distance is not zero.
    IdentityNonZero {
        /// The schema with non-zero self-distance.
        schema: Name,
        /// The actual self-distance.
        cost: f64,
    },
    /// The triangle inequality is violated.
    TriangleInequality {
        /// Source schema.
        x: Name,
        /// Intermediate schema.
        y: Name,
        /// Target schema.
        z: Name,
        /// Direct distance d(x, z).
        d_xz: f64,
        /// Sum d(x, y) + d(y, z).
        d_xy_plus_d_yz: f64,
    },
}

impl Default for LensGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protolens::ComplementConstructor;
    use panproto_gat::{Name, TheoryConstraint, TheoryEndofunctor, TheoryTransform};

    fn trivial_endofunctor() -> TheoryEndofunctor {
        TheoryEndofunctor {
            name: "id".into(),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::Identity,
        }
    }

    fn chain_with_complement(name: &str, complement: ComplementConstructor) -> ProtolensChain {
        ProtolensChain::new(vec![Protolens {
            name: Name::from(name),
            source: trivial_endofunctor(),
            target: trivial_endofunctor(),
            complement_constructor: complement,
        }])
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn chain_with_cost(name: &str, target_cost: f64) -> ProtolensChain {
        let mut complements = Vec::new();
        let whole = target_cost as usize;
        let has_half = (target_cost - whole as f64 - 0.5).abs() < f64::EPSILON;

        for i in 0..whole {
            complements.push(ComplementConstructor::DroppedSortData {
                sort: Name::from(format!("{name}_sort_{i}")),
            });
        }
        if has_half {
            complements.push(ComplementConstructor::AddedElement {
                element_name: Name::from(format!("{name}_added")),
                element_kind: "string".to_owned(),
                default_value: None,
            });
        }

        let composite = if complements.len() == 1 {
            complements.remove(0)
        } else {
            ComplementConstructor::Composite(complements)
        };

        chain_with_complement(name, composite)
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_triangle_indirect_cheaper() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");
        let c = Name::from("C");

        g.add_lens(&a, &b, chain_with_cost("ab", 2.0));
        g.add_lens(&b, &c, chain_with_cost("bc", 3.0));
        g.add_lens(&a, &c, chain_with_cost("ac", 10.0));

        g.compute_distances();

        let (cost, path) = g.preferred_path(&a, &c).expect("path should exist");
        assert!(
            (cost - 5.0).abs() < f64::EPSILON,
            "A->B->C should cost 5, got {cost}"
        );
        assert_eq!(
            path.steps.len(),
            2,
            "path should have two steps (A->B, B->C)"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_direct_cheaper() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");
        let c = Name::from("C");

        g.add_lens(&a, &b, chain_with_cost("ab", 1.0));
        g.add_lens(&b, &c, chain_with_cost("bc", 5.0));
        g.add_lens(&a, &c, chain_with_cost("ac", 3.0));

        g.compute_distances();

        let (cost, path) = g.preferred_path(&a, &c).expect("path should exist");
        assert!(
            (cost - 3.0).abs() < f64::EPSILON,
            "direct A->C should cost 3, got {cost}"
        );
        assert_eq!(path.steps.len(), 1, "direct path should have one step");
    }

    #[test]
    fn graph_no_path_disconnected() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");

        g.add_schema(a.clone());
        g.add_schema(b.clone());

        g.compute_distances();

        assert!(
            g.preferred_path(&a, &b).is_none(),
            "disconnected nodes should have no path"
        );
        assert!(
            g.distance(&a, &b).is_infinite(),
            "distance should be infinity for disconnected nodes"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_identity_distance_zero() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        g.add_schema(a.clone());
        g.compute_distances();

        assert!(
            (g.distance(&a, &a)).abs() < f64::EPSILON,
            "self distance should be 0"
        );

        let (cost, path) = g
            .preferred_path(&a, &a)
            .expect("identity path should exist");
        assert!((cost).abs() < f64::EPSILON);
        assert!(path.steps.is_empty(), "identity path should have no steps");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_single_edge() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");

        g.add_lens(&a, &b, chain_with_cost("ab", 2.0));
        g.compute_distances();

        let (cost, path) = g.preferred_path(&a, &b).expect("path should exist");
        assert!(
            (cost - 2.0).abs() < f64::EPSILON,
            "single edge should cost 2"
        );
        assert_eq!(path.steps.len(), 1);
    }

    #[test]
    fn graph_schema_count() {
        let mut g = LensGraph::new();
        assert_eq!(g.schema_count(), 0);

        g.add_schema(Name::from("A"));
        assert_eq!(g.schema_count(), 1);

        g.add_schema(Name::from("B"));
        assert_eq!(g.schema_count(), 2);

        // Duplicate should not increase count.
        g.add_schema(Name::from("A"));
        assert_eq!(g.schema_count(), 2);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_add_lens_keeps_cheaper() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");

        g.add_lens(&a, &b, chain_with_cost("expensive", 5.0));
        g.add_lens(&a, &b, chain_with_cost("cheap", 1.0));

        g.compute_distances();
        let (cost, _) = g.preferred_path(&a, &b).expect("path should exist");
        assert!(
            (cost - 1.0).abs() < f64::EPSILON,
            "should keep cheaper edge, got {cost}"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_add_lens_does_not_replace_cheaper() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");

        g.add_lens(&a, &b, chain_with_cost("cheap", 1.0));
        g.add_lens(&a, &b, chain_with_cost("expensive", 5.0));

        g.compute_distances();
        let (cost, _) = g.preferred_path(&a, &b).expect("path should exist");
        assert!(
            (cost - 1.0).abs() < f64::EPSILON,
            "should keep cheaper edge, got {cost}"
        );
    }

    #[test]
    fn graph_no_compute_returns_infinity() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");
        g.add_lens(&a, &b, chain_with_cost("ab", 1.0));

        assert!(g.distance(&a, &b).is_infinite());
        assert!(g.preferred_path(&a, &b).is_none());
    }

    #[test]
    fn graph_add_lens_auto_adds_schemas() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");

        assert_eq!(g.schema_count(), 0);
        g.add_lens(&a, &b, chain_with_cost("ab", 1.0));
        assert_eq!(g.schema_count(), 2);
    }

    #[test]
    fn verify_metric_triangle_graph() {
        let mut g = LensGraph::new();
        let a = Name::from("A");
        let b = Name::from("B");
        let c = Name::from("C");

        g.add_lens(&a, &b, chain_with_cost("ab", 2.0));
        g.add_lens(&b, &c, chain_with_cost("bc", 3.0));
        g.add_lens(&a, &c, chain_with_cost("ac", 10.0));

        g.compute_distances();

        let violations = g.verify_metric();
        assert!(
            violations.is_empty(),
            "triangle graph should satisfy Lawvere metric axioms: {violations:?}"
        );
    }

    #[test]
    fn verify_metric_single_node() {
        let mut g = LensGraph::new();
        g.add_schema(Name::from("A"));
        g.compute_distances();

        let violations = g.verify_metric();
        assert!(violations.is_empty());
    }

    #[test]
    fn cost_function_basic() {
        use crate::cost::complement_cost;

        assert!(complement_cost(&ComplementConstructor::Empty).abs() < f64::EPSILON);

        let dropped = ComplementConstructor::DroppedSortData {
            sort: Name::from("S"),
        };
        assert!((complement_cost(&dropped) - 1.0).abs() < f64::EPSILON);

        let composite = ComplementConstructor::Composite(vec![
            ComplementConstructor::DroppedSortData {
                sort: Name::from("A"),
            },
            ComplementConstructor::DroppedSortData {
                sort: Name::from("B"),
            },
        ]);
        assert!((complement_cost(&composite) - 2.0).abs() < f64::EPSILON);
    }
}
