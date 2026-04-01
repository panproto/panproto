//! Instance codecs for configuration protocols.
//!
//! Configuration instances (playbooks, templates, manifests)
//! use JSON as their canonical instance representation.

use crate::registry::ProtocolRegistry;

/// Register all config protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("cloudformation"));
        registry.register(UnifiedCodec::json("ansible"));
        registry.register(UnifiedCodec::json("k8s_crd"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("cloudformation"));
        registry.register(JsonCodec::new("ansible"));
        registry.register(JsonCodec::new("k8s_crd"));
    }
}
