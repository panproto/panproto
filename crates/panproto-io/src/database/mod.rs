//! Instance codecs for database protocols.

use crate::registry::ProtocolRegistry;

/// Register all database protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("mongodb"));
        registry.register(UnifiedCodec::json("dynamodb"));
        registry.register(UnifiedCodec::json("cassandra"));
        registry.register(UnifiedCodec::json("neo4j"));
        // Redis uses space-delimited format; no tree-sitter grammar for that.
        // Fall back to legacy tabular codec.
        use crate::tabular_codec::TabularCodec;
        registry.register(TabularCodec::with_delimiter("redis", "entries", b' '));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        use crate::tabular_codec::TabularCodec;
        registry.register(JsonCodec::new("mongodb"));
        registry.register(JsonCodec::new("dynamodb"));
        registry.register(JsonCodec::new("cassandra"));
        registry.register(JsonCodec::new("neo4j"));
        registry.register(TabularCodec::with_delimiter("redis", "entries", b' '));
    }
}
