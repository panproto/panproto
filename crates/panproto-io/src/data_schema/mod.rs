//! Instance codecs for data schema protocols.
//!
//! Instances of data schemas are documents conforming to those schemas.

use crate::registry::ProtocolRegistry;

/// Register all data schema protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("cddl"));
        registry.register(UnifiedCodec::json("bson"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("cddl"));
        registry.register(JsonCodec::new("bson"));
    }
}
