//! Instance codecs for data schema protocols.
//!
//! Instances of data schemas are documents conforming to those schemas.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::tabular_codec::TabularCodec;

/// Register all data schema protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("json_schema"));
    registry.register(JsonCodec::new("yaml_schema"));
    registry.register(JsonCodec::new("toml_schema"));
    registry.register(JsonCodec::new("cddl"));
    registry.register(JsonCodec::new("bson"));
    registry.register(TabularCodec::csv("csv_table", "rows"));
    registry.register(TabularCodec::with_delimiter("ini_schema", "sections", b'='));
}
