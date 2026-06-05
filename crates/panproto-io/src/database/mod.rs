//! Instance codecs for database protocols.

use crate::registry::ProtocolRegistry;

/// Register all database protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    use crate::byte_tabular::ByteTabularCodec;
    // Redis (RESP key/value) is space-delimited with no tree-sitter grammar.
    // The byte-faithful tabular codec records the exact original layout and
    // replays it, so a `parse → emit` round-trip is byte-identical.
    registry.register(ByteTabularCodec::new("redis", "entries", b' ', None));

    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::json("mongodb"));
        registry.register_optional(UnifiedCodec::json("dynamodb"));
        registry.register_optional(UnifiedCodec::json("cassandra"));
        registry.register_optional(UnifiedCodec::json("neo4j"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("mongodb"));
        registry.register(JsonCodec::new("dynamodb"));
        registry.register(JsonCodec::new("cassandra"));
        registry.register(JsonCodec::new("neo4j"));
    }
}
