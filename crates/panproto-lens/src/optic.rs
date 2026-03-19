//! Optic classification for protolens chains.
//!
//! Classifies each protolens or protolens chain into an optic kind
//! (Iso, Lens, Prism, Affine, Traversal) to optimize complement
//! storage and composition.

use panproto_gat::TheoryTransform;
use serde::{Deserialize, Serialize};

/// The kind of optic a protolens or protolens chain represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
        | TheoryTransform::AddSort(_)
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
        let t = TheoryTransform::AddSort(panproto_gat::Sort::simple("new_sort"));
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
