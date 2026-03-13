//! Instance codecs for type system protocols.
//!
//! Type system instances are typed values, typically serialized as JSON.
//! The schema guides which fields and types are expected.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all type system protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("typescript"));
    registry.register(JsonCodec::new("python"));
    registry.register(JsonCodec::new("rust_serde"));
    registry.register(JsonCodec::new("java"));
    registry.register(JsonCodec::new("go_struct"));
    registry.register(JsonCodec::new("kotlin"));
    registry.register(JsonCodec::new("csharp"));
    registry.register(JsonCodec::new("swift"));
}
