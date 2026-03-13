//! Instance codecs for database protocols.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::tabular_codec::TabularCodec;

/// Register all database protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("mongodb"));
    registry.register(JsonCodec::new("dynamodb"));
    registry.register(JsonCodec::new("cassandra"));
    registry.register(JsonCodec::new("neo4j"));
    registry.register(TabularCodec::tsv("sql", "result_set"));
    registry.register(TabularCodec::with_delimiter("redis", "entries", b' '));
}
