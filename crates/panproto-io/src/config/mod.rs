//! Instance codecs for configuration protocols.
//!
//! Configuration instances (playbooks, templates, manifests, HCL files)
//! use JSON as their canonical instance representation. Dedicated parsers
//! (e.g., `hcl-rs`) can be registered as drop-in replacements.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all config protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("cloudformation"));
    registry.register(JsonCodec::new("ansible"));
    registry.register(JsonCodec::new("k8s_crd"));
    registry.register(JsonCodec::new("hcl"));
}
