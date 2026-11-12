//! Instance codecs and schema readers for configuration protocols.
//!
//! Configuration documents (playbooks, templates, manifests) are YAML, and
//! no configuration protocol reads YAML itself: under the `tree-sitter`
//! feature each reads through the shared YAML route (`tree::read_yaml`).
//! The `k8s_crd` codec reads custom resources through that route, and
//! `k8s_crd::read_crd` reads a `CustomResourceDefinition` into a schema.
//! Without the feature the codecs read the JSON form of a document only.

#[cfg(feature = "tree-sitter")]
pub mod k8s_crd;
#[cfg(feature = "tree-sitter")]
pub mod tree;

use crate::registry::ProtocolRegistry;

/// Register all config protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::json("cloudformation"));
        registry.register_optional(UnifiedCodec::json("ansible"));
        registry.register_optional(UnifiedCodec::yaml("k8s_crd"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        registry.register(JsonCodec::new("cloudformation"));
        registry.register(JsonCodec::new("ansible"));
        registry.register(JsonCodec::new("k8s_crd"));
    }
}
