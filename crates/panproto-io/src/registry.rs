//! Protocol registry mapping protocol names to parser/emitter implementations.
//!
//! The registry provides a uniform dispatch mechanism: given a protocol name,
//! look up the corresponding [`InstanceParser`] and [`InstanceEmitter`] and
//! call them. This enables generic code that works across all 77 protocols
//! without compile-time knowledge of which protocol is being used.

use std::collections::HashMap;

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A combined parser + emitter for a single protocol.
///
/// This trait object is stored in the [`ProtocolRegistry`]. Implementors
/// provide both directions of the presentation functor.
pub trait ProtocolCodec: InstanceParser + InstanceEmitter {}

impl<T: InstanceParser + InstanceEmitter> ProtocolCodec for T {}

/// Registry mapping protocol names to their instance-level codec.
///
/// # Example
///
/// ```ignore
/// let mut registry = ProtocolRegistry::new();
/// // register protocol codecs...
///
/// let instance = registry.parse_wtype("json_schema", &schema, &bytes)?;
/// let emitted = registry.emit_wtype("json_schema", &schema, &instance)?;
/// ```
pub struct ProtocolRegistry {
    codecs: HashMap<String, Box<dyn ProtocolCodec>>,
}

impl ProtocolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            codecs: HashMap::new(),
        }
    }

    /// Register a codec for a protocol.
    ///
    /// If a codec was already registered for this protocol name, it is replaced.
    pub fn register<C: ProtocolCodec + 'static>(&mut self, codec: C) {
        self.codecs
            .insert(InstanceParser::protocol_name(&codec).to_string(), Box::new(codec));
    }

    /// Look up the native representation for a protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol is not registered.
    pub fn native_repr(&self, protocol: &str) -> Result<NativeRepr, ParseInstanceError> {
        self.codecs
            .get(protocol)
            .map(|c| c.native_repr())
            .ok_or_else(|| ParseInstanceError::UnknownProtocol(protocol.to_string()))
    }

    /// Parse raw bytes into a W-type instance for the named protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol is not registered,
    /// or any parse error from the protocol's parser.
    pub fn parse_wtype(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| ParseInstanceError::UnknownProtocol(protocol.to_string()))?;
        codec.parse_wtype(schema, input)
    }

    /// Parse raw bytes into a functor instance for the named protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol is not registered,
    /// or any parse error from the protocol's parser.
    pub fn parse_functor(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| ParseInstanceError::UnknownProtocol(protocol.to_string()))?;
        codec.parse_functor(schema, input)
    }

    /// Emit a W-type instance to raw bytes for the named protocol.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if the protocol is not registered,
    /// or any emit error from the protocol's emitter.
    pub fn emit_wtype(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| EmitInstanceError::UnknownProtocol(protocol.to_string()))?;
        codec.emit_wtype(schema, instance)
    }

    /// Emit a functor instance to raw bytes for the named protocol.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if the protocol is not registered,
    /// or any emit error from the protocol's emitter.
    pub fn emit_functor(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| EmitInstanceError::UnknownProtocol(protocol.to_string()))?;
        codec.emit_functor(schema, instance)
    }

    /// Returns the number of registered protocols.
    #[must_use]
    pub fn len(&self) -> usize {
        self.codecs.len()
    }

    /// Returns `true` if no protocols are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codecs.is_empty()
    }

    /// Returns an iterator over registered protocol names.
    pub fn protocol_names(&self) -> impl Iterator<Item = &str> {
        self.codecs.keys().map(String::as_str)
    }
}

impl Default for ProtocolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
