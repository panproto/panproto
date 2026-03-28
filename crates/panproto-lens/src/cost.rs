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
        | ComplementConstructor::DroppedOpData { .. } => 1.0,
        ComplementConstructor::NatTransKernel { .. } => 10.0,
        ComplementConstructor::AddedElement { .. } => 0.5,
        ComplementConstructor::CoercedSortData { class, .. } => match class {
            panproto_gat::CoercionClass::Iso => 0.0,
            panproto_gat::CoercionClass::Retraction => 1.0,
            panproto_gat::CoercionClass::Opaque | _ => f64::INFINITY,
        },
        ComplementConstructor::Composite(children) => children.iter().map(complement_cost).sum(),
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
