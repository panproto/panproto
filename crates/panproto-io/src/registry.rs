//! Protocol registry mapping protocol names to parser/emitter implementations.
//!
//! The registry provides a uniform dispatch mechanism: given a protocol name,
//! look up the corresponding [`InstanceParser`] and [`InstanceEmitter`] and
//! call them. This enables generic code that works across all registered
//! protocols without compile-time knowledge of which protocol is being used.

use std::collections::HashMap;

use panproto_inst::{FInstance, GInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{
    GraphInstanceEmitter, GraphInstanceParser, InstanceEmitter, InstanceParser, NativeRepr,
    TableInstanceEmitter, TableInstanceParser,
};

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
    /// `UnifiedCodec`) override this.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] when the underlying canonical
    /// parse fails.
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
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] when the underlying canonical
    /// emit fails.
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

/// The key a codec for `protocol` is filed under.
///
/// A name the protocol registry of `panproto-protocols` resolves, by its
/// canonical name, an alias, or either with `_` for `-`, is filed under
/// that protocol's canonical name. Any other name is filed with `_`
/// replaced by `-`, the normalization
/// [`ProtocolDescriptor::matches`](panproto_protocols::registry::ProtocolDescriptor::matches)
/// applies, so every spelling it accepts reaches the same codec.
fn registry_key(protocol: &str) -> String {
    panproto_protocols::registry::descriptor(protocol)
        .map_or_else(|| protocol.replace('_', "-"), |d| d.name.to_owned())
}

/// A combined graph parser and emitter for a single protocol, stored in the
/// graph table of the [`ProtocolRegistry`].
pub trait GraphCodec: GraphInstanceParser + GraphInstanceEmitter {}

impl<C: GraphInstanceParser + GraphInstanceEmitter> GraphCodec for C {}

/// A combined table parser and emitter for a single protocol, stored in the
/// table table of the [`ProtocolRegistry`].
pub trait TableCodec: TableInstanceParser + TableInstanceEmitter {}

impl<C: TableInstanceParser + TableInstanceEmitter> TableCodec for C {}

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
impl ProtocolCodec for crate::byte_tabular::ByteTabularCodec {}

#[cfg(feature = "tree-sitter")]
impl ProtocolCodec for crate::unified_codec::UnifiedCodec {
    fn parse_wtype_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, Option<crate::cst_extract::CstComplement>), ParseInstanceError> {
        // Disambiguate from the trait method of the same name to
        // call the inherent method on UnifiedCodec; `Self::…` would
        // recurse into the trait method we are implementing.
        #[allow(clippy::use_self)]
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
            #[allow(clippy::use_self)]
            return crate::unified_codec::UnifiedCodec::emit_wtype_preserving(
                self, schema, instance, comp,
            );
        }
        self.emit_wtype(schema, instance)
    }
}

/// Registry mapping protocol names to their instance-level codecs.
///
/// # Codec tables
///
/// The registry keeps three tables. [`register`](Self::register) fills the
/// main table with one [`ProtocolCodec`] per protocol, which reads the
/// protocol's syntax into its native tree or table and is reached through
/// [`parse_wtype`](Self::parse_wtype) and
/// [`parse_functor`](Self::parse_functor).
/// [`register_graph`](Self::register_graph) and
/// [`register_table`](Self::register_table) fill the graph and table
/// tables, reached through [`parse_graph`](Self::parse_graph),
/// [`emit_graph`](Self::emit_graph), [`parse_table`](Self::parse_table)
/// and [`emit_table`](Self::emit_table). A protocol may have a codec in
/// more than one table: a format with a tree syntax can also have a native
/// graph reader.
///
/// # Names
///
/// Codecs are filed under the canonical name of their protocol in
/// `panproto-protocols`, and every lookup accepts what that registry
/// accepts: the canonical name, an alias, and either with `_` in place of
/// `-`. A codec registered as `k8s_crd` is listed as `k8s-crd` and
/// reached by both spellings, and the `uima` codec is also reached as
/// `uima-cas`.
///
/// # Format preservation
///
/// Lossless, byte-preserving round-trips require the `tree-sitter`
/// feature, which pulls in the CST-extraction pipeline and the
/// `UnifiedCodec`. Without that
/// feature the registry falls back to the canonical text codecs, whose
/// round-trips preserve structure but not formatting (whitespace,
/// key order, comments). Callers that want preservation but must
/// compile in every configuration use
/// [`parse_wtype_preserving_or_canonical`](Self::parse_wtype_preserving_or_canonical)
/// and
/// [`emit_wtype_preserving_or_canonical`](Self::emit_wtype_preserving_or_canonical),
/// which are available in every build and announce the fallback on
/// stderr when the feature is compiled out. The exact
/// `parse_wtype_preserving` /
/// `emit_wtype_preserving` pair exists
/// only under the feature.
///
/// # Example
///
/// ```text
/// let mut registry = ProtocolRegistry::new();
/// // register protocol codecs...
///
/// let instance = registry.parse_wtype("openapi", &schema, &bytes)?;
/// let emitted = registry.emit_wtype("openapi", &schema, &instance)?;
/// ```
pub struct ProtocolRegistry {
    codecs: HashMap<String, Box<dyn ProtocolCodec>>,
    graph_codecs: HashMap<String, Box<dyn GraphCodec>>,
    table_codecs: HashMap<String, Box<dyn TableCodec>>,
}

impl ProtocolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            codecs: HashMap::new(),
            graph_codecs: HashMap::new(),
            table_codecs: HashMap::new(),
        }
    }

    /// Register a codec for a protocol.
    ///
    /// If a codec was already registered for this protocol name, it is replaced.
    pub fn register<C: ProtocolCodec + 'static>(&mut self, codec: C) {
        self.codecs.insert(
            registry_key(InstanceParser::protocol_name(&codec)),
            Box::new(codec),
        );
    }

    /// Register a codec produced by a fallible constructor, skipping
    /// registration on `Err`.
    ///
    /// Intended for grammar-gated codecs: a binary built without the
    /// underlying tree-sitter feature returns `Err(MissingGrammar)` from
    /// the codec constructor, and the resulting codec is silently absent
    /// from the registry rather than aborting registration of the rest.
    /// The constructor's error is returned so callers that want to
    /// surface a diagnostic can; the common pattern is to ignore it.
    ///
    /// # Errors
    ///
    /// Returns the constructor's error verbatim when `codec` is `Err`.
    pub fn try_register<C, E>(&mut self, codec: Result<C, E>) -> Result<(), E>
    where
        C: ProtocolCodec + 'static,
    {
        let codec = codec?;
        self.register(codec);
        Ok(())
    }

    /// Register a codec built by a `UnifiedCodec` constructor, distinguishing
    /// expected from unexpected failures.
    ///
    /// * `Ok(codec)` → register normally.
    /// * `Err(MissingGrammar)` → silently skip (the corresponding
    ///   `lang-*` cargo feature is disabled in this build, an
    ///   expected configuration).
    /// * `Err(ParserInit { source })` → log to stderr and skip.
    ///   A bundled grammar that fails parser-init is a build-system
    ///   bug rather than a runtime condition; logging surfaces the
    ///   regression without aborting the host process. Panicking here
    ///   would violate the WASM "no panics across the boundary"
    ///   contract and break web/Python/Node hosts whenever a single
    ///   bundled grammar regresses.
    ///
    /// Use this in place of `let _ = registry.try_register(...)` so
    /// that `ParserInit` failures are surfaced (loudly to stderr) and
    /// not silently swallowed.
    #[cfg(feature = "tree-sitter")]
    pub fn register_optional<C: ProtocolCodec + 'static>(
        &mut self,
        codec: Result<C, crate::error::UnifiedCodecError>,
    ) {
        match codec {
            Ok(codec) => self.register(codec),
            Err(crate::error::UnifiedCodecError::MissingGrammar { .. }) => {
                // Expected: grammar feature disabled.
            }
            Err(err @ crate::error::UnifiedCodecError::ParserInit { .. }) => {
                eprintln!(
                    "panproto-io: bundled grammar failed parser initialisation; \
                     skipping registration: {err}"
                );
            }
        }
    }

    /// Register a graph codec for a protocol.
    ///
    /// If a graph codec was already registered for this protocol name, it
    /// is replaced. The main codec of the protocol, if any, is kept.
    pub fn register_graph<C: GraphCodec + 'static>(&mut self, codec: C) {
        self.graph_codecs.insert(
            registry_key(GraphInstanceParser::protocol_name(&codec)),
            Box::new(codec),
        );
    }

    /// Register a table codec for a protocol.
    ///
    /// If a table codec was already registered for this protocol name, it
    /// is replaced. The main codec of the protocol, if any, is kept.
    pub fn register_table<C: TableCodec + 'static>(&mut self, codec: C) {
        self.table_codecs.insert(
            registry_key(TableInstanceParser::protocol_name(&codec)),
            Box::new(codec),
        );
    }

    /// Look up the native representation for a protocol.
    ///
    /// This is the representation of the protocol's main codec, the one
    /// [`register`](Self::register) installed. A protocol with no main
    /// codec reports [`NativeRepr::Graph`] when it has a graph codec and
    /// otherwise [`NativeRepr::Table`] when it has a table codec.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol is not registered.
    pub fn native_repr(&self, protocol: &str) -> Result<NativeRepr, ParseInstanceError> {
        let key = registry_key(protocol);
        if let Some(codec) = self.codecs.get(&key) {
            return Ok(codec.native_repr());
        }
        if self.graph_codecs.contains_key(&key) {
            return Ok(NativeRepr::Graph);
        }
        if self.table_codecs.contains_key(&key) {
            return Ok(NativeRepr::Table);
        }
        Err(ParseInstanceError::UnknownProtocol(protocol.to_string()))
    }

    /// Whether [`parse_graph`](Self::parse_graph) and
    /// [`emit_graph`](Self::emit_graph) have a codec for `protocol`.
    #[must_use]
    pub fn supports_graph(&self, protocol: &str) -> bool {
        self.graph_codecs.contains_key(&registry_key(protocol))
    }

    /// Whether [`parse_table`](Self::parse_table) and
    /// [`emit_table`](Self::emit_table) have a codec for `protocol`: a
    /// registered table codec, or a main codec whose native representation
    /// is [`NativeRepr::Table`].
    #[must_use]
    pub fn supports_table(&self, protocol: &str) -> bool {
        let key = registry_key(protocol);
        self.table_codecs.contains_key(&key)
            || self
                .codecs
                .get(&key)
                .is_some_and(|c| c.native_repr() == NativeRepr::Table)
    }

    /// Parse raw bytes into a graph instance for the named protocol,
    /// through its registered graph codec.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if no codec of any
    /// kind is registered for the protocol,
    /// [`ParseInstanceError::UnsupportedRepresentation`] if the protocol is
    /// registered without a graph codec, or any parse error from the codec.
    pub fn parse_graph(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<GInstance, ParseInstanceError> {
        match self.graph_codecs.get(&registry_key(protocol)) {
            Some(codec) => codec.parse_graph(schema, input),
            None => Err(ParseInstanceError::UnsupportedRepresentation {
                protocol: protocol.to_string(),
                requested: NativeRepr::Graph,
                native: self.native_repr(protocol)?,
            }),
        }
    }

    /// Emit a graph instance to raw bytes for the named protocol, through
    /// its registered graph codec.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if no codec of any
    /// kind is registered for the protocol,
    /// [`EmitInstanceError::UnsupportedRepresentation`] if the protocol is
    /// registered without a graph codec, or any emit error from the codec.
    pub fn emit_graph(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        match self.graph_codecs.get(&registry_key(protocol)) {
            Some(codec) => codec.emit_graph(schema, instance),
            None => Err(EmitInstanceError::UnsupportedRepresentation {
                protocol: protocol.to_string(),
                requested: NativeRepr::Graph,
                native: self.emit_native_repr(protocol)?,
            }),
        }
    }

    /// Parse raw bytes into a table instance for the named protocol.
    ///
    /// Uses the protocol's registered table codec, or else its main codec
    /// when that codec's native representation is [`NativeRepr::Table`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if no codec of any
    /// kind is registered for the protocol,
    /// [`ParseInstanceError::UnsupportedRepresentation`] if neither codec
    /// reads tables, or any parse error from the codec.
    pub fn parse_table(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        if let Some(codec) = self.table_codecs.get(&registry_key(protocol)) {
            return codec.parse_table(schema, input);
        }
        match self.native_repr(protocol)? {
            NativeRepr::Table if self.codecs.contains_key(&registry_key(protocol)) => {
                self.parse_functor(protocol, schema, input)
            }
            native => Err(ParseInstanceError::UnsupportedRepresentation {
                protocol: protocol.to_string(),
                requested: NativeRepr::Table,
                native,
            }),
        }
    }

    /// Emit a table instance to raw bytes for the named protocol.
    ///
    /// Uses the protocol's registered table codec, or else its main codec
    /// when that codec's native representation is [`NativeRepr::Table`].
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if no codec of any
    /// kind is registered for the protocol,
    /// [`EmitInstanceError::UnsupportedRepresentation`] if neither codec
    /// writes tables, or any emit error from the codec.
    pub fn emit_table(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        if let Some(codec) = self.table_codecs.get(&registry_key(protocol)) {
            return codec.emit_table(schema, instance);
        }
        match self.emit_native_repr(protocol)? {
            NativeRepr::Table if self.codecs.contains_key(&registry_key(protocol)) => {
                self.emit_functor(protocol, schema, instance)
            }
            native => Err(EmitInstanceError::UnsupportedRepresentation {
                protocol: protocol.to_string(),
                requested: NativeRepr::Table,
                native,
            }),
        }
    }

    /// [`native_repr`](Self::native_repr) with the emit error type.
    fn emit_native_repr(&self, protocol: &str) -> Result<NativeRepr, EmitInstanceError> {
        self.native_repr(protocol)
            .map_err(|_| EmitInstanceError::UnknownProtocol(protocol.to_string()))
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
            .get(&registry_key(protocol))
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
            .get(&registry_key(protocol))
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
            .get(&registry_key(protocol))
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
            .get(&registry_key(protocol))
            .ok_or_else(|| EmitInstanceError::UnknownProtocol(protocol.to_string()))?;
        codec.emit_functor(schema, instance)
    }

    /// Returns the number of registered protocols, counting a protocol
    /// with codecs in several tables once.
    #[must_use]
    pub fn len(&self) -> usize {
        self.name_set().len()
    }

    /// Returns `true` if no protocols are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codecs.is_empty() && self.graph_codecs.is_empty() && self.table_codecs.is_empty()
    }

    /// Returns the names of the registered protocols in ascending order,
    /// each once however many tables hold a codec for it.
    pub fn protocol_names(&self) -> impl Iterator<Item = &str> {
        self.name_set().into_iter()
    }

    /// The names with a codec in any table.
    fn name_set(&self) -> std::collections::BTreeSet<&str> {
        self.codecs
            .keys()
            .chain(self.graph_codecs.keys())
            .chain(self.table_codecs.keys())
            .map(String::as_str)
            .collect()
    }

    /// Parse with format preservation when this build supports it,
    /// falling back to a canonical parse otherwise.
    ///
    /// Unlike `parse_wtype_preserving`,
    /// which the `tree-sitter` feature gates out entirely, this method
    /// is available in every build. When the feature is compiled in it
    /// delegates to that method, capturing a
    /// [`CstComplement`](crate::cst_extract::CstComplement) when the
    /// codec supports preservation. When the feature is compiled out no
    /// codec can capture a complement, so this method prints a one-line
    /// notice to stderr and returns the canonical parse paired with
    /// `None`. A caller that asked for preservation therefore learns
    /// that the build cannot honor the request rather than silently
    /// receiving canonical output.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnknownProtocol`] if the protocol
    /// is not registered, or any parse error from the codec.
    pub fn parse_wtype_preserving_or_canonical(
        &self,
        protocol: &str,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, Option<crate::cst_extract::CstComplement>), ParseInstanceError> {
        #[cfg(feature = "tree-sitter")]
        {
            self.parse_wtype_preserving(protocol, schema, input)
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            eprintln!(
                "panproto-io: format-preserving parse requested for protocol '{protocol}', \
                 but this build lacks the 'tree-sitter' feature; returning a canonical parse \
                 with no complement (round-trips will not preserve formatting)"
            );
            let instance = self.parse_wtype(protocol, schema, input)?;
            Ok((instance, None))
        }
    }

    /// Emit with format preservation when this build supports it,
    /// falling back to canonical emission otherwise.
    ///
    /// Companion to
    /// [`parse_wtype_preserving_or_canonical`](Self::parse_wtype_preserving_or_canonical):
    /// available in every build. With the `tree-sitter` feature
    /// compiled in it delegates to
    /// `emit_wtype_preserving`, using the
    /// complement to reconstruct the original formatting. Without the
    /// feature the `complement` cannot have come from a preserving
    /// parse; this method emits canonically and prints a one-line notice
    /// to stderr when a caller nonetheless supplies a complement.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnknownProtocol`] if the protocol is
    /// not registered, or any emit error from the codec.
    pub fn emit_wtype_preserving_or_canonical(
        &self,
        protocol: &str,
        schema: &Schema,
        instance: &WInstance,
        complement: Option<&crate::cst_extract::CstComplement>,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        #[cfg(feature = "tree-sitter")]
        {
            self.emit_wtype_preserving(protocol, schema, instance, complement)
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            if complement.is_some() {
                eprintln!(
                    "panproto-io: format-preserving emit requested for protocol '{protocol}', \
                     but this build lacks the 'tree-sitter' feature; emitting canonically"
                );
            }
            self.emit_wtype(protocol, schema, instance)
        }
    }

    /// Parse with format preservation, returning both the instance and a
    /// CST complement that can be used for format-preserving emission.
    ///
    /// If the codec for this protocol implements
    /// [`FormatPreservingCodec`](crate::traits::FormatPreservingCodec),
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
            .get(&registry_key(protocol))
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
            .get(&registry_key(protocol))
            .ok_or_else(|| EmitInstanceError::UnknownProtocol(protocol.to_string()))?;

        codec.emit_wtype_preserving(schema, instance, complement)
    }
}

impl Default for ProtocolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod try_register_tests {
    use super::*;
    use crate::json_codec::JsonCodec;

    #[test]
    fn try_register_ok_inserts_codec() {
        let mut registry = ProtocolRegistry::new();
        #[allow(deprecated)]
        let codec: Result<JsonCodec, &'static str> = Ok(JsonCodec::new("ok-protocol"));
        let outcome = registry.try_register(codec);
        assert!(outcome.is_ok(), "Ok constructor should register");
        assert!(
            registry.protocol_names().any(|n| n == "ok-protocol"),
            "the codec should be visible in protocol_names"
        );
    }

    #[test]
    fn try_register_err_skips_and_propagates() {
        let mut registry = ProtocolRegistry::new();
        let before: usize = registry.protocol_names().count();
        let codec: Result<JsonCodec, &'static str> = Err("missing grammar");
        let outcome = registry.try_register(codec);
        assert_eq!(outcome, Err("missing grammar"));
        let after: usize = registry.protocol_names().count();
        assert_eq!(
            before, after,
            "registry size must not change when constructor returns Err"
        );
    }

    #[test]
    fn try_register_multiple_mixed_outcomes() {
        let mut registry = ProtocolRegistry::new();
        #[allow(deprecated)]
        let _ = registry.try_register::<JsonCodec, &'static str>(Ok(JsonCodec::new("a")));
        let _ = registry.try_register::<JsonCodec, &'static str>(Err("nope"));
        #[allow(deprecated)]
        let _ = registry.try_register::<JsonCodec, &'static str>(Ok(JsonCodec::new("b")));
        let names: std::collections::BTreeSet<&str> = registry.protocol_names().collect();
        assert!(names.contains("a"), "Ok codec 'a' must be present");
        assert!(names.contains("b"), "Ok codec 'b' must be present");
        assert!(
            !names.contains("nope"),
            "Err codec must not pollute the registry"
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn register_optional_missing_grammar_silently_skips() {
        use crate::error::UnifiedCodecError;
        let mut registry = ProtocolRegistry::new();
        let before: usize = registry.protocol_names().count();
        let codec: Result<JsonCodec, UnifiedCodecError> = Err(UnifiedCodecError::MissingGrammar {
            format: "json".into(),
        });
        registry.register_optional(codec);
        let after: usize = registry.protocol_names().count();
        assert_eq!(
            before, after,
            "MissingGrammar must skip registration without diagnostic noise"
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn register_optional_parser_init_skips_with_stderr_log() {
        use crate::error::UnifiedCodecError;
        let mut registry = ProtocolRegistry::new();
        let inner = panproto_parse::error::ParseError::TheoryExtraction {
            reason: "synthetic for test".to_owned(),
        };
        let codec: Result<JsonCodec, UnifiedCodecError> = Err(UnifiedCodecError::ParserInit {
            format: "yaml".into(),
            source: Box::new(inner),
        });
        registry.register_optional(codec);
        assert_eq!(
            registry.protocol_names().count(),
            0,
            "ParserInit must skip registration; the eprintln diagnostic surfaces the regression \
             without aborting the host process (WASM-safe contract)"
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn register_optional_ok_inserts_codec() {
        use crate::error::UnifiedCodecError;
        let mut registry = ProtocolRegistry::new();
        #[allow(deprecated)]
        let codec: Result<JsonCodec, UnifiedCodecError> = Ok(JsonCodec::new("ok"));
        registry.register_optional(codec);
        assert!(registry.protocol_names().any(|n| n == "ok"));
    }
}

#[cfg(test)]
mod shape_table_tests {
    use std::collections::HashMap;

    use panproto_inst::{FInstance, GInstance, Node, Value};
    use panproto_schema::{Edge, Schema};

    use super::ProtocolRegistry;
    use crate::error::{EmitInstanceError, ParseInstanceError};
    use crate::traits::{
        GraphInstanceEmitter, GraphInstanceParser, NativeRepr, TableInstanceEmitter,
        TableInstanceParser,
    };

    /// An edge list: one `src tgt` pair of node ids per line, every node
    /// anchored at `node` and every edge the schema edge `link`.
    struct EdgeListCodec;

    fn link() -> Edge {
        Edge {
            src: "node".into(),
            tgt: "node".into(),
            kind: "link".into(),
            name: None,
        }
    }

    fn parse_error(message: impl Into<String>) -> ParseInstanceError {
        ParseInstanceError::Parse {
            protocol: "edge-list".into(),
            message: message.into(),
        }
    }

    impl GraphInstanceParser for EdgeListCodec {
        fn protocol_name(&self) -> &'static str {
            "edge-list"
        }

        fn parse_graph(
            &self,
            _schema: &Schema,
            input: &[u8],
        ) -> Result<GInstance, ParseInstanceError> {
            let text = std::str::from_utf8(input).map_err(|e| parse_error(e.to_string()))?;
            let mut graph = GInstance::new();
            for line in text.lines() {
                let (src, tgt) = line
                    .split_once(' ')
                    .ok_or_else(|| parse_error(format!("no pair in {line:?}")))?;
                let src: u32 = src.parse().map_err(|_| parse_error(src))?;
                let tgt: u32 = tgt.parse().map_err(|_| parse_error(tgt))?;
                for id in [src, tgt] {
                    if graph.node(id).is_none() {
                        graph
                            .insert_node(Node::new(id, "node"))
                            .map_err(|e| parse_error(e.to_string()))?;
                    }
                }
                graph
                    .add_edge(src, tgt, link())
                    .map_err(|e| parse_error(e.to_string()))?;
            }
            Ok(graph)
        }
    }

    impl GraphInstanceEmitter for EdgeListCodec {
        fn protocol_name(&self) -> &'static str {
            "edge-list"
        }

        fn emit_graph(
            &self,
            _schema: &Schema,
            instance: &GInstance,
        ) -> Result<Vec<u8>, EmitInstanceError> {
            let lines: Vec<String> = instance
                .edges_sorted()
                .map(|(_, edge)| format!("{} {}\n", edge.src, edge.tgt))
                .collect();
            Ok(lines.concat().into_bytes())
        }
    }

    /// One `name` column of the table `row`, one cell per line.
    struct NameListCodec;

    impl TableInstanceParser for NameListCodec {
        fn protocol_name(&self) -> &'static str {
            "name-list"
        }

        fn parse_table(
            &self,
            _schema: &Schema,
            input: &[u8],
        ) -> Result<FInstance, ParseInstanceError> {
            let text = std::str::from_utf8(input).map_err(|e| ParseInstanceError::Parse {
                protocol: "name-list".into(),
                message: e.to_string(),
            })?;
            let rows = text
                .lines()
                .map(|line| HashMap::from([("name".to_owned(), Value::Str(line.to_owned()))]))
                .collect();
            Ok(FInstance::new().with_table("row", rows))
        }
    }

    impl TableInstanceEmitter for NameListCodec {
        fn protocol_name(&self) -> &'static str {
            "name-list"
        }

        fn emit_table(
            &self,
            _schema: &Schema,
            instance: &FInstance,
        ) -> Result<Vec<u8>, EmitInstanceError> {
            let mut out = String::new();
            for row in instance.tables.get("row").into_iter().flatten() {
                match row.get("name") {
                    Some(Value::Str(name)) => {
                        out.push_str(name);
                        out.push('\n');
                    }
                    other => {
                        return Err(EmitInstanceError::Emit {
                            protocol: "name-list".into(),
                            message: format!("cell `name` is {other:?}"),
                        });
                    }
                }
            }
            Ok(out.into_bytes())
        }
    }

    #[test]
    fn a_registered_graph_codec_parses_and_emits_through_the_registry()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = ProtocolRegistry::new();
        assert!(!registry.supports_graph("edge-list"));
        registry.register_graph(EdgeListCodec);
        assert!(registry.supports_graph("edge-list"));
        assert!(!registry.supports_table("edge-list"));
        assert_eq!(registry.native_repr("edge-list")?, NativeRepr::Graph);
        assert_eq!(registry.protocol_names().collect::<Vec<_>>(), ["edge-list"]);

        let schema = Schema::default();
        let input = b"1 2\n2 3\n3 1\n";
        let graph = registry.parse_graph("edge-list", &schema, input)?;
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(registry.emit_graph("edge-list", &schema, &graph)?, input);
        Ok(())
    }

    #[test]
    fn a_registered_table_codec_parses_and_emits_through_the_registry()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = ProtocolRegistry::new();
        assert!(!registry.supports_table("name-list"));
        registry.register_table(NameListCodec);
        assert!(registry.supports_table("name-list"));
        assert!(!registry.supports_graph("name-list"));
        assert_eq!(registry.native_repr("name-list")?, NativeRepr::Table);

        let schema = Schema::default();
        let input = b"ada\ngrace\n";
        let table = registry.parse_table("name-list", &schema, input)?;
        assert_eq!(table.row_count("row"), 2);
        assert_eq!(registry.emit_table("name-list", &schema, &table)?, input);
        Ok(())
    }

    #[test]
    fn a_tree_protocol_without_a_graph_codec_reports_the_missing_shape() {
        let registry = crate::default_registry();
        let schema = Schema::default();
        assert!(!registry.supports_graph("openapi"));
        assert!(!registry.supports_table("openapi"));
        assert!(matches!(
            registry.parse_graph("openapi", &schema, b"{}"),
            Err(ParseInstanceError::UnsupportedRepresentation {
                requested: NativeRepr::Graph,
                native: NativeRepr::Tree,
                ..
            })
        ));
        assert!(matches!(
            registry.emit_table("openapi", &schema, &FInstance::new()),
            Err(EmitInstanceError::UnsupportedRepresentation {
                requested: NativeRepr::Table,
                native: NativeRepr::Tree,
                ..
            })
        ));
        assert!(matches!(
            registry.parse_graph("no-such-protocol", &schema, b""),
            Err(ParseInstanceError::UnknownProtocol(_))
        ));
        assert!(matches!(
            registry.emit_graph("no-such-protocol", &schema, &GInstance::new()),
            Err(EmitInstanceError::UnknownProtocol(_))
        ));
    }

    #[test]
    fn a_table_native_main_codec_answers_the_table_entries()
    -> Result<(), Box<dyn std::error::Error>> {
        let registry = crate::default_registry();
        assert_eq!(registry.native_repr("redis")?, NativeRepr::Table);
        assert!(registry.supports_table("redis"));
        let schema = Schema::default();
        let input = b"SET k v\n";
        let via_table = registry.parse_table("redis", &schema, input)?;
        let via_functor = registry.parse_functor("redis", &schema, input)?;
        assert_eq!(
            serde_json::to_value(&via_table)?,
            serde_json::to_value(&via_functor)?
        );
        assert_eq!(
            registry.emit_table("redis", &schema, &via_table)?,
            registry.emit_functor("redis", &schema, &via_functor)?
        );
        Ok(())
    }
}
