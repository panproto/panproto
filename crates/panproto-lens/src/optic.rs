//! Optic classification for protolens chains.
//!
//! Classifies each protolens or protolens chain into an optic kind
//! (Iso, Lens, Prism, Affine, Traversal) to optimize complement
//! storage and composition.
//!
//! ## Lawfulness assumption
//!
//! The classification in [`classify_transform`] is structural: it assigns
//! optic kinds based on the shape of the [`TheoryTransform`], not by
//! verifying the corresponding optic laws at runtime. This is correct for
//! elementary transforms (rename, add, drop), which are lawful by
//! construction. For composite or auto-generated transforms, use
//! [`check_optic_laws`] to verify that the classified kind's laws hold
//! on a concrete instance.

use panproto_gat::TheoryTransform;
use serde::{Deserialize, Serialize};

/// The kind of optic a protolens or protolens chain represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpticKind {
    /// Bijection -- no complement needed (complement is Unit).
    Iso,
    /// Projection -- complement captures dropped data.
    Lens,
    /// Injection -- complement is a variant tag.
    Prism,
    /// Lens composed with Prism -- complement is (variant tag, dropped data).
    Affine,
    /// Multi-focus -- complement tracks positions.
    Traversal,
}

impl PartialOrd for OpticKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::{Greater, Less};
        if self == other {
            return Some(std::cmp::Ordering::Equal);
        }
        match (self, other) {
            (Self::Iso, _) | (Self::Lens | Self::Prism, Self::Affine) => Some(Less),
            (_, Self::Iso) | (Self::Traversal, _) | (Self::Affine, Self::Lens | Self::Prism) => {
                Some(Greater)
            }
            (_, Self::Traversal) => Some(Less),
            _ => None,
        }
    }
}

impl OpticKind {
    /// Compose two optic kinds according to the optics hierarchy.
    ///
    /// The composition table follows the standard optics lattice:
    /// - Iso is the identity for composition.
    /// - Traversal absorbs everything.
    /// - Lens + Prism (or Prism + Lens) yields Affine.
    /// - Affine composed with Lens or Prism stays Affine.
    #[must_use]
    pub const fn compose(self, other: Self) -> Self {
        match (self, other) {
            // Iso is the identity element.
            (Self::Iso, x) | (x, Self::Iso) => x,

            // Traversal absorbs everything.
            (Self::Traversal, _) | (_, Self::Traversal) => Self::Traversal,

            // Homogeneous composition.
            (Self::Lens, Self::Lens) => Self::Lens,
            (Self::Prism, Self::Prism) => Self::Prism,

            // Anything involving Affine, or mixing Lens+Prism, yields Affine.
            _ => Self::Affine,
        }
    }
}

/// Classify a [`TheoryTransform`] into an [`OpticKind`].
///
/// The mapping follows from the data-preservation properties of each
/// transform:
///
/// - **Iso**: `Identity`, `RenameSort`, `RenameOp` (bijections).
/// - **Lens**: `DropSort`, `DropOp`, `DropEquation`, `AddSort`, `AddOp`,
///   `AddEquation`, `Pullback` (projections or extensions with defaults).
/// - **Compose**: recursively composes the inner classifications.
#[must_use]
pub fn classify_transform(transform: &TheoryTransform) -> OpticKind {
    match transform {
        // Bijections: no data loss, complement is unit.
        TheoryTransform::Identity
        | TheoryTransform::RenameSort { .. }
        | TheoryTransform::RenameOp { .. } => OpticKind::Iso,

        // Projections and extensions: complement captures dropped or default data.
        TheoryTransform::DropSort(_)
        | TheoryTransform::DropOp(_)
        | TheoryTransform::DropEquation(_)
        | TheoryTransform::AddSort { .. }
        | TheoryTransform::AddOp(_)
        | TheoryTransform::AddEquation(_)
        | TheoryTransform::Pullback(_)
        | TheoryTransform::CoerceSort { .. }
        | TheoryTransform::MergeSorts { .. }
        | TheoryTransform::AddSortWithDefault { .. }
        | TheoryTransform::AddDirectedEquation(_)
        | TheoryTransform::DropDirectedEquation(_) => OpticKind::Lens,

        // Composition: recursively classify and compose.
        TheoryTransform::Compose(a, b) => {
            let kind_a = classify_transform(a);
            let kind_b = classify_transform(b);
            kind_a.compose(kind_b)
        }
    }
}

/// Verify that the optic laws hold for a classified transform on a
/// concrete lens and instance.
///
/// Checks the laws appropriate to the classified [`OpticKind`]:
///
/// - **Iso**: `get(put(v, c)) == v` AND `put(get(s), c) == s`
///   AND complement is empty.
/// - **Lens**: `get(put(v, c)) == v` and `put(get(s), c) == s`.
/// - **Prism/Affine/Traversal**: falls through to Lens-level checks
///   (the full prism laws require a `review` operation not yet implemented).
///
/// # Errors
///
/// Returns [`OpticLawViolation`] describing the first law that fails.
pub fn check_optic_laws(
    kind: OpticKind,
    lens: &crate::Lens,
    instance: &panproto_inst::WInstance,
) -> Result<(), OpticLawViolation> {
    use crate::asymmetric::{get, put};

    let (view, complement) = get(lens, instance).map_err(|e| OpticLawViolation {
        kind,
        law: "get",
        detail: format!("get failed: {e}"),
    })?;

    // PutGet: put(get(s), complement) should reconstruct s.
    let restored = put(lens, &view, &complement).map_err(|e| OpticLawViolation {
        kind,
        law: "PutGet",
        detail: format!("put failed: {e}"),
    })?;

    if !crate::laws::instances_equivalent(instance, &restored) {
        return Err(OpticLawViolation {
            kind,
            law: "PutGet",
            detail: format!(
                "structural mismatch: original {} nodes/{} arcs, restored {} nodes/{} arcs",
                instance.node_count(),
                instance.arc_count(),
                restored.node_count(),
                restored.arc_count()
            ),
        });
    }

    // GetPut: get(put(v, c)) should return v.
    let (view2, _complement2) = get(lens, &restored).map_err(|e| OpticLawViolation {
        kind,
        law: "GetPut",
        detail: format!("get after put failed: {e}"),
    })?;

    if !crate::laws::instances_equivalent(&view, &view2) {
        return Err(OpticLawViolation {
            kind,
            law: "GetPut",
            detail: format!(
                "view structural mismatch: original {} nodes/{} arcs, after round-trip {} nodes/{} arcs",
                view.node_count(),
                view.arc_count(),
                view2.node_count(),
                view2.arc_count()
            ),
        });
    }

    // For Iso: complement must be empty.
    if kind == OpticKind::Iso
        && (!complement.dropped_nodes.is_empty() || !complement.dropped_arcs.is_empty())
    {
        return Err(OpticLawViolation {
            kind,
            law: "Iso complement must be empty",
            detail: format!(
                "complement has {} dropped nodes, {} dropped arcs",
                complement.dropped_nodes.len(),
                complement.dropped_arcs.len()
            ),
        });
    }

    Ok(())
}

/// A violation of an optic law.
#[derive(Debug)]
pub struct OpticLawViolation {
    /// The classified optic kind.
    pub kind: OpticKind,
    /// Which law was violated.
    pub law: &'static str,
    /// Details about the violation.
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Composition table tests
    // ---------------------------------------------------------------

    #[test]
    fn iso_is_identity_element() {
        for kind in all_kinds() {
            assert_eq!(OpticKind::Iso.compose(kind), kind);
            assert_eq!(kind.compose(OpticKind::Iso), kind);
        }
    }

    #[test]
    fn traversal_absorbs_everything() {
        for kind in all_kinds() {
            assert_eq!(OpticKind::Traversal.compose(kind), OpticKind::Traversal);
            assert_eq!(kind.compose(OpticKind::Traversal), OpticKind::Traversal);
        }
    }

    #[test]
    fn lens_compose_lens_is_lens() {
        assert_eq!(OpticKind::Lens.compose(OpticKind::Lens), OpticKind::Lens);
    }

    #[test]
    fn prism_compose_prism_is_prism() {
        assert_eq!(OpticKind::Prism.compose(OpticKind::Prism), OpticKind::Prism);
    }

    #[test]
    fn lens_and_prism_yield_affine() {
        assert_eq!(OpticKind::Lens.compose(OpticKind::Prism), OpticKind::Affine);
        assert_eq!(OpticKind::Prism.compose(OpticKind::Lens), OpticKind::Affine);
    }

    #[test]
    fn affine_stays_affine_with_lens_prism_affine() {
        assert_eq!(
            OpticKind::Affine.compose(OpticKind::Lens),
            OpticKind::Affine
        );
        assert_eq!(
            OpticKind::Affine.compose(OpticKind::Prism),
            OpticKind::Affine
        );
        assert_eq!(
            OpticKind::Affine.compose(OpticKind::Affine),
            OpticKind::Affine
        );
        assert_eq!(
            OpticKind::Lens.compose(OpticKind::Affine),
            OpticKind::Affine
        );
        assert_eq!(
            OpticKind::Prism.compose(OpticKind::Affine),
            OpticKind::Affine
        );
    }

    #[test]
    fn composition_is_commutative() {
        for a in all_kinds() {
            for b in all_kinds() {
                assert_eq!(
                    a.compose(b),
                    b.compose(a),
                    "compose should be commutative: {a:?} + {b:?}"
                );
            }
        }
    }

    #[test]
    fn composition_is_associative() {
        for a in all_kinds() {
            for b in all_kinds() {
                for c in all_kinds() {
                    assert_eq!(
                        a.compose(b).compose(c),
                        a.compose(b.compose(c)),
                        "compose should be associative: ({a:?} + {b:?}) + {c:?}"
                    );
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // Classification tests
    // ---------------------------------------------------------------

    #[test]
    fn classify_identity_is_iso() {
        assert_eq!(
            classify_transform(&TheoryTransform::Identity),
            OpticKind::Iso
        );
    }

    #[test]
    fn classify_rename_sort_is_iso() {
        let t = TheoryTransform::RenameSort {
            old: "foo".into(),
            new: "bar".into(),
        };
        assert_eq!(classify_transform(&t), OpticKind::Iso);
    }

    #[test]
    fn classify_rename_op_is_iso() {
        let t = TheoryTransform::RenameOp {
            old: "f".into(),
            new: "g".into(),
        };
        assert_eq!(classify_transform(&t), OpticKind::Iso);
    }

    #[test]
    fn classify_drop_sort_is_lens() {
        let t = TheoryTransform::DropSort("x".into());
        assert_eq!(classify_transform(&t), OpticKind::Lens);
    }

    #[test]
    fn classify_drop_op_is_lens() {
        let t = TheoryTransform::DropOp("f".into());
        assert_eq!(classify_transform(&t), OpticKind::Lens);
    }

    #[test]
    fn classify_add_sort_is_lens() {
        let t = TheoryTransform::AddSort { sort: panproto_gat::Sort::simple("new_sort"), vertex_kind: None };
        assert_eq!(classify_transform(&t), OpticKind::Lens);
    }

    #[test]
    fn classify_compose_two_isos_is_iso() {
        let t = TheoryTransform::Compose(
            Box::new(TheoryTransform::RenameSort {
                old: "a".into(),
                new: "b".into(),
            }),
            Box::new(TheoryTransform::RenameOp {
                old: "f".into(),
                new: "g".into(),
            }),
        );
        assert_eq!(classify_transform(&t), OpticKind::Iso);
    }

    #[test]
    fn classify_compose_iso_and_lens_is_lens() {
        let t = TheoryTransform::Compose(
            Box::new(TheoryTransform::RenameSort {
                old: "a".into(),
                new: "b".into(),
            }),
            Box::new(TheoryTransform::DropSort("x".into())),
        );
        assert_eq!(classify_transform(&t), OpticKind::Lens);
    }

    #[test]
    fn classify_compose_two_lenses_is_lens() {
        let t = TheoryTransform::Compose(
            Box::new(TheoryTransform::DropSort("x".into())),
            Box::new(TheoryTransform::DropOp("f".into())),
        );
        assert_eq!(classify_transform(&t), OpticKind::Lens);
    }

    // ---------------------------------------------------------------
    // Serde round-trip
    // ---------------------------------------------------------------

    #[test]
    fn optic_kind_serde_round_trip() {
        for kind in all_kinds() {
            let json =
                serde_json::to_string(&kind).unwrap_or_else(|e| panic!("serialize {kind:?}: {e}"));
            let back: OpticKind =
                serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize {kind:?}: {e}"));
            assert_eq!(kind, back);
        }
    }

    // ---------------------------------------------------------------
    // Law-checking tests
    // ---------------------------------------------------------------

    #[test]
    fn identity_lens_satisfies_iso_laws() {
        use crate::tests::{identity_lens, three_node_instance, three_node_schema};

        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_optic_laws(OpticKind::Iso, &lens, &instance);
        assert!(
            result.is_ok(),
            "identity lens should satisfy Iso laws: {result:?}"
        );
    }

    #[test]
    fn projection_lens_satisfies_lens_laws() {
        use crate::tests::{projection_lens, three_node_instance, three_node_schema};

        let schema = three_node_schema();
        let lens = projection_lens(&schema, "text");
        let instance = three_node_instance();

        let result = check_optic_laws(OpticKind::Lens, &lens, &instance);
        assert!(
            result.is_ok(),
            "projection lens should satisfy Lens laws: {result:?}"
        );
    }

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    fn all_kinds() -> [OpticKind; 5] {
        [
            OpticKind::Iso,
            OpticKind::Lens,
            OpticKind::Prism,
            OpticKind::Affine,
            OpticKind::Traversal,
        ]
    }
}
