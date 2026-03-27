//! Instance codecs for serialization protocols.
//!
//! Serialization protocols use JSON as their canonical instance representation.
//! Binary wire formats (Avro, FlatBuffers, etc.) are represented as
//! their JSON canonical encoding, matching the JSON pathway used by other
//! protocols. Dedicated binary parsers (`apache-avro`, etc.) can
//! be registered as drop-in replacements for higher throughput.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all serialization protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("avro"));
    registry.register(JsonCodec::new("flatbuffers"));
    registry.register(JsonCodec::new("asn1"));
    registry.register(JsonCodec::new("bond"));
    registry.register(JsonCodec::new("msgpack_schema"));
}
