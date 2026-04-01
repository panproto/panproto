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
///
/// When the `tree-sitter` feature is enabled, codecs may override the
/// format-preserving methods to support lossless round-trips.
pub trait ProtocolCodec: InstanceParser + InstanceEmitter {
    /// Parse with format preservation, returning a CST complement.
    ///
    /// The default implementation falls back to canonical parsing with
    /// no complement. Codecs that support format preservation (such as
    /// [`UnifiedCodec`](crate::unified_codec::UnifiedCodec)) override this.
    #[cfg(feature = "tree-sitter")]
    fn parse_wtype_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, Option<crate::cst_extract::CstComplement>), ParseInstanceError> {
        let instance = self.parse_wtype(schema, input)?;
        Ok((instance, None))
    }

    /// Emit with format preservation using a CST complement.
    ///
    /// The default implementation ignores the complement and emits canonically.
    #[cfg(feature = "tree-sitter")]
    fn emit_wtype_preserving(
        &self,
        schema: &Schema,
        instance: &WInstance,
        _complement: Option<&crate::cst_extract::CstComplement>,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        self.emit_wtype(schema, instance)
    }
}

// Explicit ProtocolCodec impls for each codec type.
// The blanket impl was removed so that UnifiedCodec can override the
// format-preserving methods.
#[allow(deprecated)]
impl ProtocolCodec for crate::json_codec::JsonCodec {}
#[allow(deprecated)]
impl ProtocolCodec for crate::xml_codec::XmlCodec {}
#[allow(deprecated)]
impl ProtocolCodec for crate::tabular_codec::TabularCodec {}
impl ProtocolCodec for crate::annotation::conllu::ConlluCodec {}

#[cfg(feature = "tree-sitter")]
impl ProtocolCodec for crate::unified_codec::UnifiedCodec {
    fn parse_wtype_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, Option<crate::cst_extract::CstComplement>), ParseInstanceError> {
        let (instance, complement) =
            crate::unified_codec::UnifiedCodec::parse_wtype_preserving(self, schema, input)?;
        Ok((instance, Some(complement)))
    }

    fn emit_wtype_preserving(
        &self,
        schema: &Schema,
        instance: &WInstance,
        complement: Option<&crate::cst_extract::CstComplement>,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        if let Some(comp) = complement {
            return crate::unified_codec::UnifiedCodec::emit_wtype_preserving(
                self, schema, instance, comp,
            );
        }
        self.emit_wtype(schema, instance)
    }
}

/// Registry mapping protocol names to their instance-level codec.
///
/// # Example
///
/// ```ignore
/// let mut registry = ProtocolRegistry::new();
/// // register protocol codecs...
///
/// let instance = registry.parse_wtype("openapi", &schema, &bytes)?;
/// let emitted = registry.emit_wtype("openapi", &schema, &instance)?;
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
        self.codecs.insert(
            InstanceParser::protocol_name(&codec).to_string(),
            Box::new(codec),
        );
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

    /// Parse with format preservation, returning both the instance and a
    /// CST complement that can be used for format-preserving emission.
    ///
    /// If the codec for this protocol implements [`FormatPreservingCodec`],
    /// the complement captures formatting information. Otherwise, the
    /// complement is `None` and emission will produce canonical output.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol is
    /// not registered, or any parse error from the codec.
    #[cfg(feature = "tree-sitter")]
    pub fn parse_wtype_preserving(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, Option<crate::cst_extract::CstComplement>), ParseInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| ParseInstanceError::UnknownProtocol(protocol.to_string()))?;

        codec.parse_wtype_preserving(schema, input)
    }

    /// Emit with format preservation using a CST complement.
    ///
    /// If the complement is `Some`, the emitter uses it to reconstruct the
    /// original formatting. If `None`, falls back to canonical emission.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if the protocol is
    /// not registered, or any emit error from the codec.
    #[cfg(feature = "tree-sitter")]
    pub fn emit_wtype_preserving(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &WInstance,
        complement: Option<&crate::cst_extract::CstComplement>,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let codec = self
            .codecs
            .get(protocol)
            .ok_or_else(|| EmitInstanceError::UnknownProtocol(protocol.to_string()))?;

        codec.emit_wtype_preserving(schema, instance, complement)
    }
}

impl Default for ProtocolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
