//! Instance codecs for serialization protocols.
//!
//! Serialization protocols use JSON as their canonical instance representation.
//! Binary wire formats (Avro, FlatBuffers, etc.) are represented as
//! their JSON canonical encoding, matching the JSON pathway used by other
//! protocols. Dedicated binary parsers (`apache-avro`, etc.) can
//! be registered as drop-in replacements for higher throughput.

use crate::registry::ProtocolRegistry;

/// Register all serialization protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("avro"));
        registry.register(UnifiedCodec::json("flatbuffers"));
        registry.register(UnifiedCodec::json("asn1"));
        registry.register(UnifiedCodec::json("bond"));
        registry.register(UnifiedCodec::json("msgpack_schema"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("avro"));
        registry.register(JsonCodec::new("flatbuffers"));
        registry.register(JsonCodec::new("asn1"));
        registry.register(JsonCodec::new("bond"));
        registry.register(JsonCodec::new("msgpack_schema"));
    }
}
