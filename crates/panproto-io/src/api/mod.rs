//! Instance codecs and schema readers for API specification protocols.
//!
//! All API protocols use JSON instance encoding (responses/payloads are
//! JSON). Under the `tree-sitter` feature, `openapi::read_openapi` reads an
//! `OpenAPI` 3.1 document, and `openapi::read_openapi_bundle` a description
//! split over several documents, through the shared JSON and YAML routes
//! into an `openapi` schema, and `asyncapi::read_asyncapi` an `AsyncAPI` 3
//! document, with `asyncapi::read_asyncapi_bundle` for a description split
//! over several documents, into an `asyncapi` schema.

#[cfg(feature = "tree-sitter")]
pub mod asyncapi;
#[cfg(feature = "tree-sitter")]
pub mod openapi;

use crate::registry::ProtocolRegistry;

/// Register all API protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::json("openapi"));
        registry.register_optional(UnifiedCodec::json("asyncapi"));
        registry.register_optional(UnifiedCodec::json("jsonapi"));
        registry.register_optional(UnifiedCodec::json("raml"));
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
