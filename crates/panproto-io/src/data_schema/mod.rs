//! Instance codecs for data schema protocols.
//!
//! Instances of data schemas are documents conforming to those schemas.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all data schema protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("cddl"));
    registry.register(JsonCodec::new("bson"));
}
