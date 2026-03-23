//! Complement cost computation for the Lawvere metric on lens graphs.
//!
//! Each [`ComplementConstructor`] carries an information cost representing
//! how much data is lost or fabricated by a protolens step. These costs
//! form a Lawvere metric: identity has cost 0, and composition satisfies
//! the triangle inequality.

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
}
