//! Core traits for instance-level presentation functors.
//!
//! Each protocol implements [`InstanceParser`] and [`InstanceEmitter`] to
//! connect raw format bytes to panproto's instance models. The choice of
//! [`NativeRepr`] is determined by the protocol's instance theory:
//!
//! - `ThWType` → [`NativeRepr::WType`] → [`panproto_inst::WInstance`]
//! - `ThFunctor` → [`NativeRepr::Functor`] → [`panproto_inst::FInstance`]
//! - Protocols supporting both → [`NativeRepr::Either`]

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};

/// Which instance representation a protocol natively uses.
///
/// This is not an implementation choice — it follows directly from the
/// protocol's `instance_theory` field in its [`panproto_schema::Protocol`]
/// definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeRepr {
    /// W-type trees (wellfounded trees per Gambino & Hyland 2004).
    /// Used by document, API, annotation, and most web protocols.
    WType,
    /// Set-valued functors (Spivak 2012).
    /// Used by relational, columnar, and tabular protocols.
    Functor,
    /// Protocol supports both representations.
    Either,
}

/// Parse raw format bytes into a panproto instance.
///
/// Each implementation is a presentation functor: it witnesses that the raw
/// format syntax is a faithful encoding of the algebraic model defined by
/// the protocol's instance theory.
pub trait InstanceParser: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Which representation is native for this protocol.
    fn native_repr(&self) -> NativeRepr;

    /// Parse bytes into a W-type instance (tree-shaped).
    ///
    /// The `schema` guides parsing: outgoing edges determine child structure,
    /// vertex kinds determine node types, constraints validate values.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support W-type instances.
    fn parse_wtype(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<WInstance, ParseInstanceError>;

    /// Parse bytes into a functor instance (tabular).
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support functor instances.
    fn parse_functor(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError>;
}

/// Emit a panproto instance to raw format bytes.
///
/// The reverse of [`InstanceParser`]: converts abstract instance models
/// back to concrete format syntax.
pub trait InstanceEmitter: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Emit a W-type instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support W-type instances.
    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;

    /// Emit a functor instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support functor instances.
    fn emit_functor(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;
}
