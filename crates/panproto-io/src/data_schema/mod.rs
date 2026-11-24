//! Instance codecs and schema readers for data schema protocols.
//!
//! Instances of data schemas are documents conforming to those schemas.
//! Under the `tree-sitter` feature, `json_schema::read_schema` reads a JSON
//! Schema 2020-12 document, and `json_schema::read_bundle` a set of them,
//! through the shared JSON route into a `json-schema` schema. The `bson`
//! module decodes and encodes binary BSON 1.1 documents and writes MongoDB
//! Extended JSON in every build, reads Extended JSON through the shared
//! JSON route under the feature, and registers the `bson` instance codec.

pub mod bson;
#[cfg(feature = "tree-sitter")]
pub mod json_schema;

use crate::registry::ProtocolRegistry;

/// Register all data schema protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(bson::BsonCodec);
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::json("cddl"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("cddl"));
    }
}
