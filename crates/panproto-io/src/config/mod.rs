//! Instance codecs for configuration protocols.
//!
//! Configuration instances (playbooks, templates, manifests)
//! use JSON as their canonical instance representation.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all config protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("cloudformation"));
    registry.register(JsonCodec::new("ansible"));
    registry.register(JsonCodec::new("k8s_crd"));
}
