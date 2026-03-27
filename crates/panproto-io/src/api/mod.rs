//! Instance codecs for API specification protocols.
//!
//! All API protocols use JSON instance encoding (responses/payloads are JSON).

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all API protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("openapi"));
    registry.register(JsonCodec::new("asyncapi"));
    registry.register(JsonCodec::new("jsonapi"));
    registry.register(JsonCodec::new("raml"));
}
