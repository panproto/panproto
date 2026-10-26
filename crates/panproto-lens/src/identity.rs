//! The identity of a lens, and the check that a complement belongs to it.
//!
//! A [`Lens`]'s identity is an [`IdentityEnvelope`] naming its source
//! schema, its target schema, its compiled migration and the shape of
//! complement it produces. It is computed on demand from those parts by
//! [`Lens::identity`], so it can never drift from the lens it names: `Lens`
//! has public fields, and a cached identity would go stale the moment one
//! of them changed.
//!
//! `get` stamps the envelope on every complement it produces, and `put`
//! checks it. [`ComplementIdentity`] exposes that check for any complement
//! shape: [`status`](ComplementIdentity::status) says how the complement is
//! bound to a lens, [`verify`](ComplementIdentity::verify) applies an
//! [`IdentityPolicy`] to that status, and
//! [`upgrade_identity`](ComplementIdentity::upgrade_identity) stamps a
//! legacy complement whose fingerprint matches.
//!
//! See [`panproto_inst::identity`] for the digests, the envelope's wire
//! form, the legacy fingerprint and the full status table.

use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityEnvelope, IdentityError, IdentityPolicy,
    IdentityStatus, identity_status, lens_identity,
};

use crate::Lens;
use crate::error::LensError;

impl Lens {
    /// This lens's identity, as stamped on the complements of `shape` it
    /// produces.
    ///
    /// Computed from the source schema, the target schema and the compiled
    /// migration each time it is called. A lens whose schemas were
    /// synthesized from a bare migration (as the language bindings do for a
    /// migration handle that carries no schemas) is named by those
    /// synthesized schemas, and so is every complement it stamps.
    #[must_use]
    pub fn identity(&self, shape: ComplementShape) -> IdentityEnvelope {
        lens_identity(&self.src_schema, &self.tgt_schema, &self.compiled, shape)
    }
}

/// The identity check, for every complement shape.
///
/// Implemented for every [`IdentityCarrier`], which is every complement
/// type in the workspace.
pub trait ComplementIdentity {
    /// How this complement is bound to `lens`. See
    /// [`panproto_inst::identity::identity_status`] for the table.
    ///
    /// # Errors
    ///
    /// [`IdentityError::Mismatch`] when the complement's envelope names a
    /// different lens; [`IdentityError::LegacyComplementRejected`] when its
    /// legacy fingerprint matches the lens's source schema at neither width.
    fn status(&self, lens: &Lens) -> Result<IdentityStatus, IdentityError>;

    /// The complement's [`status`](Self::status), if `policy` accepts it
    /// for this complement.
    ///
    /// # Errors
    ///
    /// [`LensError::Identity`] carrying the status error, or the
    /// [`IdentityPolicy::admit`] refusal.
    fn verify(&self, lens: &Lens, policy: IdentityPolicy) -> Result<IdentityStatus, LensError>;

    /// Stamp `lens`'s envelope on a complement that does not yet carry
    /// one, when nothing contradicts it: a legacy complement whose
    /// fingerprint matches the lens's source schema, or an empty unbound
    /// one. A complement that already carries this lens's envelope is left
    /// as it is.
    ///
    /// # Errors
    ///
    /// [`LensError::Identity`] when the complement names another lens, its
    /// fingerprint does not match, or it carries data but no identity.
    fn upgrade_identity(&mut self, lens: &Lens) -> Result<(), LensError>;
}

impl<C: IdentityCarrier> ComplementIdentity for C {
    fn status(&self, lens: &Lens) -> Result<IdentityStatus, IdentityError> {
        identity_status(self, &lens.identity(C::SHAPE), &lens.src_schema)
    }

    fn verify(&self, lens: &Lens, policy: IdentityPolicy) -> Result<IdentityStatus, LensError> {
        verify_with_identity(self, lens, &lens.identity(C::SHAPE), policy)
    }

    fn upgrade_identity(&mut self, lens: &Lens) -> Result<(), LensError> {
        let envelope = lens.identity(C::SHAPE);
        match identity_status(self, &envelope, &lens.src_schema)? {
            IdentityStatus::Verified => Ok(()),
            IdentityStatus::Legacy { .. } => {
                self.stamp(envelope, &lens.src_schema);
                Ok(())
            }
            IdentityStatus::Unbound => {
                if self.records_nothing() {
                    self.stamp(envelope, &lens.src_schema);
                    Ok(())
                } else {
                    Err(IdentityError::UnboundComplement.into())
                }
            }
        }
    }
}

/// [`ComplementIdentity::verify`] against an envelope already computed for
/// `lens`, so a caller that checks many complements computes it once.
pub(crate) fn verify_with_identity<C: IdentityCarrier + ?Sized>(
    complement: &C,
    lens: &Lens,
    envelope: &IdentityEnvelope,
    policy: IdentityPolicy,
) -> Result<IdentityStatus, LensError> {
    let status = identity_status(complement, envelope, &lens.src_schema)?;
    policy.admit(&status, complement.records_nothing())?;
    Ok(status)
}
