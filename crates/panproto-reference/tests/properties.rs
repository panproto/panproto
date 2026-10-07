#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Randomised laws of the reference: exact arithmetic against machine
//! integers, the canonical value encoding against its decoder, graph edits
//! against their inverses, and coercion against itself.

use panproto_reference::canonical::{decode_value, value_bytes};
use panproto_reference::coerce::{ScalarKind, ValueSpec, coerce_value};
use panproto_reference::edit::{self, GraphDelta};
use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::num::{Int, Nat};
use panproto_reference::value::Value;
use proptest::prelude::*;

fn scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        any::<u64>().prop_map(Value::Float),
        "[a-z\u{e9}\u{1f600}]{0,6}".prop_map(Value::Str),
        proptest::collection::vec(any::<u8>(), 0..6).prop_map(Value::Bytes),
        Just(Value::Null),
        (0u64..4).prop_map(Value::LabeledNull),
    ]
}

fn value() -> impl Strategy<Value = Value> {
    scalar().prop_recursive(2, 12, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::List),
            proptest::collection::btree_map("[a-c]", inner, 0..3).prop_map(Value::Unknown),
        ]
    })
}

/// A graph on nodes drawn from `0..4`, anchored at `a` or `b`, with an
/// optional integer value, and up to four edges between present nodes.
fn graph() -> impl Strategy<Value = Graph> {
    let nodes = proptest::collection::btree_map(
        0u32..4,
        (prop::bool::ANY, proptest::option::of(0i64..3)),
        1..4,
    );
    nodes.prop_flat_map(|nodes| {
        let ids: Vec<u32> = nodes.keys().copied().collect();
        let edges = proptest::collection::btree_map(
            0u32..4,
            (
                proptest::sample::select(ids.clone()),
                proptest::sample::select(ids),
                prop::bool::ANY,
            ),
            0..4,
        );
        (Just(nodes), edges).prop_map(|(nodes, edges)| {
            let mut out = Graph::default();
            for (id, (in_a, value)) in nodes {
                let node = Node::new(id, if in_a { "a" } else { "b" });
                out.nodes.insert(
                    id,
                    value.map_or_else(|| node.clone(), |v| node.clone().with_value(Value::Int(v))),
                );
            }
            for (id, (src, tgt, first)) in edges {
                let (from, to) = (
                    out.nodes[&src].anchor.clone(),
                    out.nodes[&tgt].anchor.clone(),
                );
                let label = SEdge::new(&from, &to, "prop", Some(if first { "p" } else { "q" }));
                out.edges.insert(
                    id,
                    GEdge {
                        src,
                        tgt,
                        label,
                        position: None,
                    },
                );
            }
            out
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn natural_arithmetic_agrees_with_u128(a in any::<u64>(), b in any::<u64>()) {
        let (x, y) = (Nat::from_u64(a), Nat::from_u64(b));
        let (wa, wb) = (u128::from(a), u128::from(b));
        prop_assert_eq!(x.add(&y).to_u128(), Some(wa + wb));
        prop_assert_eq!(x.mul(&y).to_u128(), Some(wa * wb));
        prop_assert_eq!(x.checked_sub(&y).and_then(|n| n.to_u128()), wa.checked_sub(wb));
        prop_assert_eq!(x.cmp(&y), a.cmp(&b));
        let product = Nat::from_u128(wa * wb + wa);
        let split = product.div_rem(&y).map(|(q, r)| (q.to_u128(), r.to_u128()));
        prop_assert_eq!(split, (wa * wb + wa).checked_div(wb).map(|q| (Some(q), Some((wa * wb + wa) % wb))));
        prop_assert_eq!(Nat::parse(&(wa * wb).to_string()).and_then(|n| n.to_u128()), Some(wa * wb));
    }

    #[test]
    fn integer_arithmetic_agrees_with_i128(a in any::<i64>(), b in any::<i64>()) {
        let (x, y) = (Int::from_i64(a), Int::from_i64(b));
        let (wa, wb) = (i128::from(a), i128::from(b));
        prop_assert_eq!(x.add(&y), Int::from_i128(wa + wb));
        prop_assert_eq!(x.sub(&y), Int::from_i128(wa - wb));
        prop_assert_eq!(x.mul(&y), Int::from_i128(wa * wb));
        prop_assert_eq!(x.neg(), Int::from_i128(-wa));
        prop_assert_eq!(x.to_i64(), Some(a));
    }

    #[test]
    fn canonical_value_bytes_decode_to_the_value(v in value()) {
        let bytes = value_bytes(&v).unwrap();
        prop_assert_eq!(decode_value(&bytes), Some(v));
    }

    #[test]
    fn distinct_values_have_distinct_canonical_bytes(v in value(), w in value()) {
        prop_assert_eq!(value_bytes(&v) == value_bytes(&w), v == w);
    }

    #[test]
    fn a_diff_applies_and_its_inverse_undoes_it(g in graph(), h in graph()) {
        let d = edit::diff(&g, &h);
        prop_assert_eq!(edit::apply(&d, &g), Ok(h.clone()));
        prop_assert_eq!(edit::apply(&d.invert(), &h), Ok(g.clone()));
        prop_assert_eq!(d.invert().invert(), d.clone());
        prop_assert_eq!(edit::diff(&g, &g), GraphDelta::identity());
        let there_and_back = d.compose(&d.invert()).unwrap();
        prop_assert_eq!(edit::apply(&there_and_back, &g), Ok(g));
    }

    #[test]
    fn diffs_compose_to_the_diff_of_the_ends(g in graph(), h in graph(), k in graph()) {
        let (first, second) = (edit::diff(&g, &h), edit::diff(&h, &k));
        let both = first.compose(&second).unwrap();
        prop_assert_eq!(edit::apply(&both, &g), Ok(k));
        prop_assert_eq!(both.invert(), second.invert().compose(&first.invert()).unwrap());
    }

    #[test]
    fn coerce_value_fixes_its_own_results(bits in any::<u64>(), width in prop::sample::select(vec![16u8, 32, 64])) {
        let spec = ValueSpec::of(ScalarKind::Float { bits: width });
        if let Some(Ok(w)) = coerce_value(&Value::Float(bits), &spec) {
            prop_assert_eq!(coerce_value(&w, &spec), Some(Ok(w)));
        }
    }
}
