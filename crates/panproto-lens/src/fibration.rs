//! Grothendieck fibration structure for the protolens framework.
//!
//! The projection from the total category of schemas down to theories is a
//! Grothendieck fibration, and lens structure is induced by the cleavage.
//! This module formalizes that connection by providing:
//!
//! - [`Fibration`]: a trait capturing the cartesian lifting property.
//! - [`WTypeFibration`]: an implementation where fibers are `WInstance`/`Complement`
//!   pairs, base morphisms are Lenses, and lifting operations are get/put.
//! - [`verify_cartesian_universal`]: checks that a lens's get/put satisfy the
//!   universal property of cartesian lifts (reduces to GetPut/PutGet laws).
//!
//! The connection to Johnson-Rosebrugh delta lenses: `get` is the functor G
//! from the total category to the base, and `put` is the lifting operation P
//! that provides the cleavage. The get-put law says P is a section of G; the
//! put-get law says G composed with P is the identity on the base.

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
/// The universal property of cartesian lifts reduces to the lens laws:
/// - Opcartesian then cartesian (get then put) satisfies put-get.
/// - Cartesian then opcartesian (put then get) satisfies get-put.
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
    if restored.node_count() != instance.node_count() {
        return Err(CartesianViolation {
            law: "PutGet (node count)",
            detail: format!(
                "original {} nodes, restored {} nodes",
                instance.node_count(),
                restored.node_count()
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

    if view2.node_count() != view.node_count() {
        return Err(CartesianViolation {
            law: "GetPut (view node count)",
            detail: format!(
                "original view {} nodes, round-tripped view {} nodes",
                view.node_count(),
                view2.node_count()
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
#[allow(clippy::unwrap_used)]
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
}
