//! Instance codecs for API specification protocols.
//!
//! All API protocols use JSON instance encoding (responses/payloads are JSON).

use crate::registry::ProtocolRegistry;

/// Register all API protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("openapi"));
        registry.register(UnifiedCodec::json("asyncapi"));
        registry.register(UnifiedCodec::json("jsonapi"));
        registry.register(UnifiedCodec::json("raml"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("openapi"));
        registry.register(JsonCodec::new("asyncapi"));
        registry.register(JsonCodec::new("jsonapi"));
        registry.register(JsonCodec::new("raml"));
    }
}
