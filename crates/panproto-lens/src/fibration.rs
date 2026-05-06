//! Grothendieck fibration structure for the protolens framework.
//!
//! The projection from the total category of schema-keyed instances down
//! to the category of schemas is a Grothendieck fibration. The fibre
//! over a schema `S` is the set of `WInstance`s anchored at `S`;
//! reindexing along a lens `f: S → T` is the operation `f* := put_f`,
//! which pulls a `T`-fibre datum back to an `S`-fibre datum given a
//! complement. The Cartesian arrow `φ_f: f*Y → Y` is then `get_f`
//! applied to `put_f(Y, c)`.
//!
//! This module provides:
//!
//! - [`Fibration`]: a trait capturing the Cartesian lifting property.
//! - [`WTypeFibration`]: an implementation where fibres are
//!   `WInstance`/`Complement` pairs and base morphisms are [`Lens`]es.
//! - [`verify_cartesian_universal`]: confirms the cleavage's section
//!   laws (`GetPut` and `PutGet`) on a single base morphism.
//! - [`verify_cartesian_factorization`]: the *universal property*
//!   itself — given a downstream morphism `h: W → S` and an instance
//!   `Z` over `W`, the reindexing through `f∘h` agrees with the
//!   composite reindexing `h* ∘ f*`. This is the genuine universal
//!   factorization, not just a round-trip check.
//!
//! The connection to Johnson-Rosebrugh delta lenses: `get` is the
//! functor `G` from the total category to the base, and `put` is the
//! cleavage `P`; the get-put law says `P` is a section of `G`, the
//! put-get law says `G ∘ P` is the identity on the base.
//!
//! ## Bifibration domain
//!
//! Reindexing has a left adjoint (an opcartesian lift) only on the
//! sub-category of base morphisms whose `Lens` is *pure rename* — i.e.
//! whose `compiled.surviving_verts` covers the entire source schema and
//! whose remaps are bijective. On that subcategory, [`Fibration::opcartesian_lift`]
//! is meaningful as a left adjoint to reindexing; outside it, the
//! "opcartesian" name is a misnomer for the schema-projection direction
//! `get`. Callers that require a true opcartesian lift must verify
//! that their base morphism is a pure rename (bijective `vertex_remap`,
//! full `surviving_verts`) before invoking it.

use panproto_inst::WInstance;

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::error::LensError;

/// A Grothendieck fibration with explicit cartesian lifting.
///
/// The total category has objects of type `Fiber`, morphisms (base changes)
/// of type `BaseMorphism`, and the fibration projects fibers to the base.
/// The cartesian lift reconstructs a fiber from a base morphism and a target
/// fiber; the opcartesian lift pushes a fiber forward along a base morphism.
pub trait Fibration {
    /// The fiber type (objects in the total category over a base object).
    type Fiber;
    /// The base morphism type (structure-preserving maps in the base category).
    type BaseMorphism;
    /// Error type for lifting failures.
    type Error;

    /// Cartesian lift: given a base morphism and a target fiber, lift to
    /// the total category. This is the `put` direction.
    ///
    /// # Errors
    ///
    /// Returns an error if the lift cannot be constructed.
    fn cartesian_lift(
        &self,
        morphism: &Self::BaseMorphism,
        view: &Self::Fiber,
        complement: &Complement,
    ) -> Result<Self::Fiber, Self::Error>;

    /// Opcartesian lift: push a fiber forward along a base morphism.
    /// This is the `get` direction.
    ///
    /// # Errors
    ///
    /// Returns an error if the lift cannot be constructed.
    fn opcartesian_lift(
        &self,
        morphism: &Self::BaseMorphism,
        source: &Self::Fiber,
    ) -> Result<(Self::Fiber, Complement), Self::Error>;
}

/// The W-type fibration: fibers are W-type instances, base morphisms are lenses.
///
/// This implements the Grothendieck fibration structure where:
/// - The base category is the category of schemas.
/// - The total category has objects as (Schema, `WInstance`) pairs.
/// - Cartesian lifts are `put` (restore from complement).
/// - Opcartesian lifts are `get` (project to target schema).
pub struct WTypeFibration;

impl Fibration for WTypeFibration {
    type Fiber = WInstance;
    type BaseMorphism = Lens;
    type Error = LensError;

    fn cartesian_lift(
        &self,
        morphism: &Self::BaseMorphism,
        view: &Self::Fiber,
        complement: &Complement,
    ) -> Result<Self::Fiber, Self::Error> {
        put(morphism, view, complement)
    }

    fn opcartesian_lift(
        &self,
        morphism: &Self::BaseMorphism,
        source: &Self::Fiber,
    ) -> Result<(Self::Fiber, Complement), Self::Error> {
        get(morphism, source)
    }
}

/// Verify the cartesian universal property for a lens.
///
/// Checks both section/retraction conditions (`GetPut` and `PutGet`), and the
/// complement records exact arc edges so that `put` is deterministic,
/// ensuring the cartesian lift is unique up to structural equivalence:
/// - Opcartesian then cartesian (get then put) satisfies `PutGet`.
/// - Cartesian then opcartesian (put then get) satisfies `GetPut`.
///
/// # Errors
///
/// Returns a [`CartesianViolation`] if either direction fails.
pub fn verify_cartesian_universal(
    lens: &Lens,
    instance: &WInstance,
) -> Result<(), CartesianViolation> {
    let fib = WTypeFibration;

    // Get (opcartesian lift)
    let (view, complement) =
        fib.opcartesian_lift(lens, instance)
            .map_err(|e| CartesianViolation {
                law: "opcartesian_lift (get)",
                detail: format!("{e}"),
            })?;

    // Put (cartesian lift)
    let restored =
        fib.cartesian_lift(lens, &view, &complement)
            .map_err(|e| CartesianViolation {
                law: "cartesian_lift (put)",
                detail: format!("{e}"),
            })?;

    // PutGet: restored should have same structure as original.
    if !crate::laws::instances_equivalent(instance, &restored) {
        return Err(CartesianViolation {
            law: "PutGet (structural equivalence)",
            detail: format!(
                "original {} nodes/{} arcs, restored {} nodes/{} arcs",
                instance.node_count(),
                instance.arc_count(),
                restored.node_count(),
                restored.arc_count()
            ),
        });
    }

    // GetPut: getting from restored should yield same view.
    let (view2, _) = fib
        .opcartesian_lift(lens, &restored)
        .map_err(|e| CartesianViolation {
            law: "GetPut (get after put)",
            detail: format!("{e}"),
        })?;

    if !crate::laws::instances_equivalent(&view, &view2) {
        return Err(CartesianViolation {
            law: "GetPut (view structural equivalence)",
            detail: format!(
                "original view {} nodes/{} arcs, round-tripped view {} nodes/{} arcs",
                view.node_count(),
                view.arc_count(),
                view2.node_count(),
                view2.arc_count()
            ),
        });
    }

    Ok(())
}

/// Verify the Cartesian universal factorization on a span by
/// exercising both directions.
///
/// Given base morphisms `f: S → T` and `h: W → S` (as lenses) and an
/// instance over `W`, verify:
///
/// 1. **Projection functoriality**: `get(f∘h, x) = get(f, get(h, x))`.
///    The composite reindexing must agree with the iterated
///    reindexing on the projected view.
/// 2. **Cleavage functoriality**: `put(f∘h, view, c_composite)` and
///    `put(h, put(f, view, c_f), c_h)` must agree, where the
///    complements come from the matching `get` decompositions on the
///    same source. Together with (1) under the lens laws this is the
///    full Cartesian universal property: the mediator from any
///    alternative arrow into the Cartesian lift is unique.
///
/// Both checks are performed; failure of either is reported as a
/// [`CartesianViolation`]. The `_factorization` name is only honest
/// when both sides are exercised — the single-direction version
/// missed the put-side coherence that the universal property requires
/// when the underlying lenses are not statically guaranteed
/// well-behaved.
///
/// # Errors
///
/// Returns [`CartesianViolation`] on disagreement, or operational
/// failures (lens composition, get/put failures).
pub fn verify_cartesian_factorization(
    f: &Lens,
    h: &Lens,
    source_instance: &WInstance,
) -> Result<(), CartesianViolation> {
    use crate::compose::compose;

    let composite = compose(h, f).map_err(|e| CartesianViolation {
        law: "compose(h, f)",
        detail: format!("{e}"),
    })?;

    // Projection functoriality: get(f∘h, x) = get(f, get(h, x)).
    let (view_via_composite, c_composite) =
        get(&composite, source_instance).map_err(|e| CartesianViolation {
            law: "get(f∘h, source)",
            detail: format!("{e}"),
        })?;

    let (intermediate, c_h) = get(h, source_instance).map_err(|e| CartesianViolation {
        law: "get(h, source)",
        detail: format!("{e}"),
    })?;
    let (view_via_chain, c_f) = get(f, &intermediate).map_err(|e| CartesianViolation {
        law: "get(f, get(h, source))",
        detail: format!("{e}"),
    })?;

    if !crate::laws::instances_equivalent(&view_via_composite, &view_via_chain) {
        return Err(CartesianViolation {
            law: "Cartesian universal factorization: get(f∘h) ≠ get(f) ∘ get(h)",
            detail: format!(
                "composite get yielded {} nodes / {} arcs, chained get yielded {} / {}",
                view_via_composite.node_count(),
                view_via_composite.arc_count(),
                view_via_chain.node_count(),
                view_via_chain.arc_count(),
            ),
        });
    }

    // Cleavage functoriality: put(f∘h, view, c_composite) =
    // put(h, put(f, view, c_f), c_h). Round-trip the projected view
    // back through both paths and assert structural equality. (Either
    // path independently round-tripping to the original `source_instance`
    // is GetPut, which is checked elsewhere; here we are checking
    // that the *two* paths land at the same instance, which is the
    // put-side functoriality the universal property requires.)
    let restored_via_composite =
        put(&composite, &view_via_composite, &c_composite).map_err(|e| CartesianViolation {
            law: "put(f∘h, view, c_composite)",
            detail: format!("{e}"),
        })?;
    let restored_intermediate = put(f, &view_via_chain, &c_f).map_err(|e| CartesianViolation {
        law: "put(f, view, c_f)",
        detail: format!("{e}"),
    })?;
    let restored_via_chain =
        put(h, &restored_intermediate, &c_h).map_err(|e| CartesianViolation {
            law: "put(h, put(f, view, c_f), c_h)",
            detail: format!("{e}"),
        })?;

    if !crate::laws::instances_equivalent(&restored_via_composite, &restored_via_chain) {
        return Err(CartesianViolation {
            law: "Cartesian universal factorization: put(f∘h) ≠ put(h) ∘ put(f)",
            detail: format!(
                "composite put yielded {} nodes / {} arcs, chained put yielded {} / {}",
                restored_via_composite.node_count(),
                restored_via_composite.arc_count(),
                restored_via_chain.node_count(),
                restored_via_chain.arc_count(),
            ),
        });
    }
    Ok(())
}

/// A violation of the cartesian universal property.
#[derive(Debug)]
pub struct CartesianViolation {
    /// Which law was violated.
    pub law: &'static str,
    /// Details about the violation.
    pub detail: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, projection_lens, three_node_instance, three_node_schema};

    #[test]
    fn identity_lens_cartesian_universal() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = verify_cartesian_universal(&lens, &instance);
        assert!(
            result.is_ok(),
            "identity lens should satisfy cartesian universal property: {result:?}"
        );
    }

    #[test]
    fn projection_lens_cartesian_universal() {
        let schema = three_node_schema();
        let lens = projection_lens(&schema, "text");
        let instance = three_node_instance();

        let result = verify_cartesian_universal(&lens, &instance);
        assert!(
            result.is_ok(),
            "projection lens should satisfy cartesian universal property: {result:?}"
        );
    }

    #[test]
    fn wtype_fibration_opcartesian_then_cartesian() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let fib = WTypeFibration;

        let (view, complement) = fib.opcartesian_lift(&lens, &instance).unwrap();
        let restored = fib.cartesian_lift(&lens, &view, &complement).unwrap();
        assert_eq!(restored.node_count(), instance.node_count());
    }

    /// Universal factorization: composing two identity lenses and
    /// running `get` agrees with `get(h)` then `get(f)`. This is
    /// the categorical pushout-projection law — the *real* universal
    /// property of Cartesian lifts, not just round-trip stability.
    #[test]
    fn cartesian_factorization_holds_for_identity_chain() {
        let schema = three_node_schema();
        let h = identity_lens(&schema);
        let f = identity_lens(&schema);
        let instance = three_node_instance();
        verify_cartesian_factorization(&f, &h, &instance).expect("identity chain must factor");
    }

    #[test]
    fn cartesian_factorization_holds_for_projection_chain() {
        let schema = three_node_schema();
        let h = identity_lens(&schema);
        let f = projection_lens(&schema, "text");
        let instance = three_node_instance();
        verify_cartesian_factorization(&f, &h, &instance)
            .expect("identity-then-projection must factor");
    }
}
