//! Complement cost computation forming a Lawvere metric on lens graphs.
//!
//! Each [`ComplementConstructor`] carries an information cost representing
//! how much data is lost or fabricated by a protolens step. These costs
//! form a Lawvere metric space `([0, ∞], ≥, +)`:
//!
//! - **Identity**: `cost(Empty) = 0` (identity lenses have zero cost).
//! - **Subadditivity**: `cost(Composite([a, b])) <= cost(a) + cost(b)`
//!   (currently with equality, since Composite sums).
//! - **Triangle inequality**: guaranteed by Floyd-Warshall in the lens graph.
//!
//! The enrichment structure provides the theoretical justification for the
//! "shortest path = minimal information loss" heuristic in [`crate::graph`].

use crate::protolens::{ComplementConstructor, ProtolensChain};

/// Cost of a single complement constructor.
///
/// Satisfies the enrichment axioms:
///   - `cost(Empty) = 0` (identity)
///   - `cost(Composite([a, b])) <= cost(a) + cost(b)` (triangle inequality)
#[must_use]
pub fn complement_cost(complement: &ComplementConstructor) -> f64 {
    match complement {
        ComplementConstructor::Empty => 0.0,
        ComplementConstructor::DroppedSortData { .. }
        | ComplementConstructor::DroppedOpData { .. }
        | ComplementConstructor::DroppedEdge { .. }
        // An enrichment fibre captures all per-vertex sort entries it
        // strips; cost is proportional to typical fibre size. Layout
        // enrichments carry a handful of constraints per vertex.
        | ComplementConstructor::Enrichment { .. } => 1.0,
        ComplementConstructor::NatTransKernel { .. } => 10.0,
        ComplementConstructor::AddedElement { .. } => 0.5,
        ComplementConstructor::CoercedSortData { class, .. } => match class {
            // Iso: lossless, no complement needed.
            // Projection: derived value is re-computed by `get`, so the
            // complement stores nothing for it. Cost is zero because the
            // field is deterministically re-derivable from the source fiber.
            panproto_gat::CoercionClass::Iso | panproto_gat::CoercionClass::Projection => 0.0,
            panproto_gat::CoercionClass::Retraction => 1.0,
            panproto_gat::CoercionClass::Opaque | _ => f64::INFINITY,
        },
        ComplementConstructor::Composite(children) => children.iter().map(complement_cost).sum(),
        ComplementConstructor::Scoped { inner, .. } => complement_cost(inner),
    }
}

/// Cost of an entire protolens chain (sum of step costs).
#[must_use]
pub fn chain_cost(chain: &ProtolensChain) -> f64 {
    chain
        .steps
        .iter()
        .map(|step| complement_cost(&step.complement_constructor))
        .sum()
}

/// Verify that the identity cost is zero.
#[must_use]
pub fn verify_identity_cost() -> bool {
    complement_cost(&ComplementConstructor::Empty).abs() < f64::EPSILON
}

/// Verify subadditivity: `cost(Composite([a, b])) <= cost(a) + cost(b)`.
#[must_use]
pub fn verify_subadditivity(a: &ComplementConstructor, b: &ComplementConstructor) -> bool {
    let composite_cost = complement_cost(&ComplementConstructor::Composite(vec![
        a.clone(),
        b.clone(),
    ]));
    let sum_cost = complement_cost(a) + complement_cost(b);
    composite_cost <= sum_cost + f64::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::Name;

    #[test]
    fn cost_empty_is_zero() {
        assert!((complement_cost(&ComplementConstructor::Empty)).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_dropped_sort_data() {
        let c = ComplementConstructor::DroppedSortData {
            sort: Name::from("MySort"),
        };
        assert!((complement_cost(&c) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_dropped_op_data() {
        let c = ComplementConstructor::DroppedOpData {
            op: Name::from("myOp"),
        };
        assert!((complement_cost(&c) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_nat_trans_kernel() {
        let c = ComplementConstructor::NatTransKernel {
            nat_trans_name: Name::from("eta"),
        };
        assert!((complement_cost(&c) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_added_element() {
        let c = ComplementConstructor::AddedElement {
            element_name: Name::from("newField"),
            element_kind: "string".to_owned(),
            default_value: None,
        };
        assert!((complement_cost(&c) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_composite_sums_children() {
        let c = ComplementConstructor::Composite(vec![
            ComplementConstructor::DroppedSortData {
                sort: Name::from("A"),
            },
            ComplementConstructor::DroppedOpData {
                op: Name::from("f"),
            },
            ComplementConstructor::AddedElement {
                element_name: Name::from("x"),
                element_kind: "int".to_owned(),
                default_value: None,
            },
        ]);
        // 1.0 + 1.0 + 0.5 = 2.5
        assert!((complement_cost(&c) - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn identity_cost_is_zero() {
        assert!(verify_identity_cost());
    }

    #[test]
    fn subadditivity_holds() {
        let a = ComplementConstructor::DroppedSortData {
            sort: Name::from("A"),
        };
        let b = ComplementConstructor::DroppedOpData {
            op: Name::from("f"),
        };
        assert!(verify_subadditivity(&a, &b));
    }

    #[test]
    fn subadditivity_with_empty() {
        let a = ComplementConstructor::Empty;
        let b = ComplementConstructor::DroppedSortData {
            sort: Name::from("A"),
        };
        assert!(verify_subadditivity(&a, &b));
        assert!(verify_subadditivity(&b, &a));
    }

    #[test]
    fn subadditivity_nested() {
        let a = ComplementConstructor::Composite(vec![
            ComplementConstructor::DroppedSortData {
                sort: Name::from("A"),
            },
            ComplementConstructor::NatTransKernel {
                nat_trans_name: Name::from("eta"),
            },
        ]);
        let b = ComplementConstructor::AddedElement {
            element_name: Name::from("x"),
            element_kind: "string".to_owned(),
            default_value: None,
        };
        assert!(verify_subadditivity(&a, &b));
    }

    #[test]
    fn cost_nested_composite() {
        let inner = ComplementConstructor::Composite(vec![
            ComplementConstructor::DroppedSortData {
                sort: Name::from("A"),
            },
            ComplementConstructor::Empty,
        ]);
        let outer = ComplementConstructor::Composite(vec![
            inner,
            ComplementConstructor::AddedElement {
                element_name: Name::from("x"),
                element_kind: "string".to_owned(),
                default_value: None,
            },
        ]);
        // (1.0 + 0.0) + 0.5 = 1.5
        assert!((complement_cost(&outer) - 1.5).abs() < f64::EPSILON);
    }

    // --- CoercedSortData cost tests ---

    #[test]
    fn cost_coerced_iso_is_zero() {
        let c = ComplementConstructor::CoercedSortData {
            sort: Name::from("MySort"),
            class: panproto_gat::CoercionClass::Iso,
        };
        assert!(complement_cost(&c).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_coerced_retraction_is_one() {
        let c = ComplementConstructor::CoercedSortData {
            sort: Name::from("MySort"),
            class: panproto_gat::CoercionClass::Retraction,
        };
        assert!((complement_cost(&c) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_coerced_opaque_is_infinity() {
        let c = ComplementConstructor::CoercedSortData {
            sort: Name::from("MySort"),
            class: panproto_gat::CoercionClass::Opaque,
        };
        assert!(complement_cost(&c).is_infinite());
    }

    #[test]
    fn subadditivity_coerced_retraction_pair() {
        let a = ComplementConstructor::CoercedSortData {
            sort: Name::from("A"),
            class: panproto_gat::CoercionClass::Retraction,
        };
        let b = ComplementConstructor::CoercedSortData {
            sort: Name::from("B"),
            class: panproto_gat::CoercionClass::Retraction,
        };
        // Composite cost = 1.0 + 1.0 = 2.0, sum = 2.0. Equal, so <= holds.
        assert!(verify_subadditivity(&a, &b));
    }

    #[test]
    fn subadditivity_coerced_with_opaque() {
        let a = ComplementConstructor::CoercedSortData {
            sort: Name::from("A"),
            class: panproto_gat::CoercionClass::Retraction,
        };
        let b = ComplementConstructor::CoercedSortData {
            sort: Name::from("B"),
            class: panproto_gat::CoercionClass::Opaque,
        };
        // Composite cost = 1.0 + inf = inf, sum = 1.0 + inf = inf. inf <= inf.
        assert!(verify_subadditivity(&a, &b));
    }
}

/// Property tests for the quantale triangle inequality
/// `cost(complement(g ∘ f)) <= cost(f) + cost(g)` over actual composition.
///
/// The Lawvere-metric enrichment obligation is that geodesic search over the
/// lens graph returns minimum-loss paths only if complement cost is
/// subadditive under composition. The existing `verify_subadditivity` and
/// `verify_metric` checks are near-tautological (one sums by definition,
/// the other checks Floyd-Warshall output). These proptests instead relate
/// the cost of complements produced by real composition —
/// [`vertical_compose`], [`horizontal_compose`], [`ProtolensChain::fuse`],
/// [`crate::compose::compose`] at the data level, and [`LensGraph`]
/// distances — to the sum of the part costs, over generated inputs.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod quantale_laws {
    use super::*;

    use crate::graph::LensGraph;
    use crate::protolens::{Protolens, elementary, horizontal_compose, vertical_compose};
    use panproto_gat::{
        CoercionClass, EnrichmentKind, Name, TheoryConstraint, TheoryEndofunctor, TheoryTransform,
    };
    use panproto_inst::value::{FieldPresence, Value};
    use panproto_inst::{Node, WInstance};
    use panproto_schema::{Edge, Protocol, Schema, Vertex};
    use proptest::prelude::*;
    use smallvec::SmallVec;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// `a <= b` with an additive tolerance. Infinities are handled by IEEE
    /// arithmetic: `INF <= INF + tol` is `INF <= INF`, which is `true`.
    fn le_with_tol(a: f64, b: f64) -> bool {
        a <= b + 1e-9
    }

    /// A `Name` strategy.
    fn arb_name() -> impl Strategy<Value = Name> {
        "[a-z]{1,6}".prop_map(|s: String| Name::from(s.as_str()))
    }

    /// A `CoercionClass` strategy over every known variant.
    fn arb_coercion_class() -> impl Strategy<Value = CoercionClass> {
        prop::sample::select(vec![
            CoercionClass::Iso,
            CoercionClass::Retraction,
            CoercionClass::Projection,
            CoercionClass::Opaque,
        ])
    }

    /// Strategy generating arbitrary `ComplementConstructor` trees over
    /// every variant, including `CoercedSortData` (all classes), `Scoped`,
    /// `Enrichment`, and nested `Composite`.
    fn arb_complement() -> impl Strategy<Value = ComplementConstructor> {
        let leaf = prop_oneof![
            Just(ComplementConstructor::Empty),
            arb_name().prop_map(|sort| ComplementConstructor::DroppedSortData { sort }),
            arb_name().prop_map(|op| ComplementConstructor::DroppedOpData { op }),
            (
                arb_name(),
                arb_name(),
                prop::option::of(arb_name()),
                arb_name()
            )
                .prop_map(|(src, tgt, edge_name, edge_kind)| {
                    ComplementConstructor::DroppedEdge {
                        src,
                        tgt,
                        edge_name,
                        edge_kind,
                    }
                }),
            arb_name().prop_map(|nat_trans_name| ComplementConstructor::NatTransKernel {
                nat_trans_name
            }),
            (arb_name(), "[a-z]{1,5}").prop_map(|(element_name, element_kind)| {
                ComplementConstructor::AddedElement {
                    element_name,
                    element_kind,
                    default_value: None,
                }
            }),
            (arb_name(), arb_coercion_class())
                .prop_map(|(sort, class)| ComplementConstructor::CoercedSortData { sort, class }),
            arb_name().prop_map(|enricher| ComplementConstructor::Enrichment {
                kind: EnrichmentKind::Layout,
                enricher: Arc::from(enricher.as_ref()),
            }),
        ];
        leaf.prop_recursive(4, 32, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..=4)
                    .prop_map(ComplementConstructor::Composite),
                (arb_name(), inner).prop_map(|(focus, inner)| ComplementConstructor::Scoped {
                    focus,
                    inner: Box::new(inner),
                }),
            ]
        })
    }

    /// An `Identity`-source, `Identity`-target endofunctor.
    fn id_endofunctor() -> TheoryEndofunctor {
        TheoryEndofunctor {
            name: Arc::from("id"),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::Identity,
        }
    }

    /// A protolens carrying `cc` with identity endofunctors (so any two
    /// are composable).
    fn id_protolens(name: &str, cc: ComplementConstructor) -> Protolens {
        Protolens {
            name: Name::from(name),
            source: id_endofunctor(),
            target: id_endofunctor(),
            complement_constructor: cc,
        }
    }

    /// Build a schema with a hand-rolled index (mirrors the law-test
    /// fixtures) from `(id, kind)` vertex specs and an edge list.
    fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
        let mut vertices = HashMap::new();
        let mut edges = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in verts {
            vertices.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        for e in edge_list {
            edges.insert(e.clone(), e.kind.clone());
            outgoing.entry(e.src.clone()).or_default().push(e.clone());
            incoming.entry(e.tgt.clone()).or_default().push(e.clone());
            between
                .entry((e.src.clone(), e.tgt.clone()))
                .or_default()
                .push(e.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
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
            outgoing,
            incoming,
            between,
        }
    }

    /// Protocol for the data-level scenario (kinds `object`, `ka`, `kb`).
    fn data_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "ka".into(), "kb".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    /// Build a source schema (`root` object with `na` `ka`-kind leaves and
    /// `nb` `kb`-kind leaves) plus a matching instance.
    fn build_data_scenario(na: usize, nb: usize) -> (Schema, WInstance) {
        let mut vert_specs: Vec<(String, String)> = vec![("root".to_owned(), "object".to_owned())];
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..na {
            let name = format!("a{i}");
            vert_specs.push((name.clone(), "ka".to_owned()));
            edges.push(Edge {
                src: "root".into(),
                tgt: Name::from(name.as_str()),
                kind: "prop".into(),
                name: Some(Name::from(name.as_str())),
            });
        }
        for i in 0..nb {
            let name = format!("b{i}");
            vert_specs.push((name.clone(), "kb".to_owned()));
            edges.push(Edge {
                src: "root".into(),
                tgt: Name::from(name.as_str()),
                kind: "prop".into(),
                name: Some(Name::from(name.as_str())),
            });
        }
        let vert_refs: Vec<(&str, &str)> = vert_specs
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let schema = make_schema(&vert_refs, &edges);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        let mut arcs = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            let id = u32::try_from(i + 1).unwrap();
            nodes.insert(
                id,
                Node::new(id, e.tgt.as_ref())
                    .with_value(FieldPresence::Present(Value::Str("v".to_owned()))),
            );
            arcs.push((0, id, e.clone()));
        }
        let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));
        (schema, instance)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// `cost(complement(vertical/horizontal compose)) <= cost(η) + cost(θ)`.
        #[test]
        fn triangle_inequality_vertical_compose(
            cc1 in arb_complement(),
            cc2 in arb_complement(),
        ) {
            let eta = id_protolens("eta", cc1.clone());
            let theta = id_protolens("theta", cc2.clone());

            let v = vertical_compose(&eta, &theta).unwrap();
            let h = horizontal_compose(&eta, &theta).unwrap();
            let sum = complement_cost(&cc1) + complement_cost(&cc2);

            prop_assert!(
                le_with_tol(complement_cost(&v.complement_constructor), sum),
                "vertical_compose cost exceeded the sum of part costs",
            );
            prop_assert!(
                le_with_tol(complement_cost(&h.complement_constructor), sum),
                "horizontal_compose cost exceeded the sum of part costs",
            );
        }

        /// `cost(complement(fuse(chain))) <= Σ cost(stepᵢ)`.
        #[test]
        fn triangle_inequality_fuse(
            ccs in prop::collection::vec(arb_complement(), 1..=5),
        ) {
            let steps: Vec<Protolens> = ccs
                .iter()
                .enumerate()
                .map(|(i, cc)| id_protolens(&format!("s{i}"), cc.clone()))
                .collect();
            let chain = ProtolensChain::new(steps);
            let fused = chain.fuse().unwrap();

            let sum: f64 = ccs.iter().map(complement_cost).sum();
            prop_assert!(
                le_with_tol(complement_cost(&fused.complement_constructor), sum),
                "fused complement cost exceeded the sum of step costs",
            );
            // chain_cost is by-definition the sum; assert the relationship holds.
            prop_assert!(le_with_tol(chain_cost(&chain), sum));
        }

        /// Data-level: the complement produced by `get` on `compose(f, g)`
        /// drops no more nodes than `f` and `g` do separately.
        #[test]
        fn triangle_inequality_data_level(
            na in 1usize..=3,
            nb in 1usize..=3,
        ) {
            let (schema, instance) = build_data_scenario(na, nb);
            let proto = data_protocol();

            let f = elementary::drop_sort("ka")
                .instantiate(&schema, &proto)
                .unwrap();
            let g = elementary::drop_sort("kb")
                .instantiate(&f.tgt_schema, &proto)
                .unwrap();
            let fg = crate::compose::compose(&f, &g).unwrap();

            let (_, comp_fg) = crate::asymmetric::get(&fg, &instance).unwrap();
            let (view_b, comp_f) = crate::asymmetric::get(&f, &instance).unwrap();
            let (_, comp_g) = crate::asymmetric::get(&g, &view_b).unwrap();

            prop_assert!(
                comp_fg.dropped_nodes.len()
                    <= comp_f.dropped_nodes.len() + comp_g.dropped_nodes.len(),
                "composed complement dropped more nodes ({}) than the parts ({} + {})",
                comp_fg.dropped_nodes.len(),
                comp_f.dropped_nodes.len(),
                comp_g.dropped_nodes.len(),
            );
        }

        /// A `LensGraph` built from generated chains satisfies the Lawvere
        /// metric axioms, and `distance(A, C) <= cost(A→B) + cost(B→C)`.
        #[test]
        fn lens_graph_metric_axioms(
            cc_ab in arb_complement(),
            cc_bc in arb_complement(),
            cc_ac in arb_complement(),
        ) {
            let (a, b, c) = (Name::from("A"), Name::from("B"), Name::from("C"));
            let mut graph = LensGraph::new();
            graph.add_lens(&a, &b, ProtolensChain::new(vec![id_protolens("ab", cc_ab.clone())]));
            graph.add_lens(&b, &c, ProtolensChain::new(vec![id_protolens("bc", cc_bc.clone())]));
            graph.add_lens(&a, &c, ProtolensChain::new(vec![id_protolens("ac", cc_ac)]));
            graph.compute_distances();

            prop_assert!(
                graph.verify_metric().is_empty(),
                "Floyd-Warshall distances violated the metric axioms",
            );
            let via = complement_cost(&cc_ab) + complement_cost(&cc_bc);
            prop_assert!(
                le_with_tol(graph.distance(&a, &c), via),
                "distance(A, C) exceeded the explicit A→B→C composite cost",
            );
        }
    }
}
