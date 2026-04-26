//! Instance codecs for linguistic annotation protocols.
//!
//! - JSON-based: brat, decomp, ucca, fovea, bead, web_annotation, concrete, nif
//! - XML-based: naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf
//! - Tab-delimited: conllu
//! - Line-based: amr

pub mod conllu;

use crate::registry::ProtocolRegistry;

/// Register all annotation protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    // Tab-delimited (custom codec, always legacy for now)
    registry.register(conllu::ConlluCodec::new());

    // Line-based (AMR PENMAN notation represented as TSV)
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        let _ = registry.try_register(UnifiedCodec::tsv("amr", "amr_graph"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::tabular_codec::TabularCodec;
        registry.register(TabularCodec::tsv("amr", "amr_graph"));
    }

    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;

        // JSON-based
        let _ = registry.try_register(UnifiedCodec::json("brat"));
        let _ = registry.try_register(UnifiedCodec::json("decomp"));
        let _ = registry.try_register(UnifiedCodec::json("ucca"));
        let _ = registry.try_register(UnifiedCodec::json("fovea"));
        let _ = registry.try_register(UnifiedCodec::json("bead"));
        let _ = registry.try_register(UnifiedCodec::json("web_annotation"));
        let _ = registry.try_register(UnifiedCodec::json("concrete"));
        let _ = registry.try_register(UnifiedCodec::json("nif"));

        // XML-based
        let _ = registry.try_register(UnifiedCodec::xml("naf"));
        let _ = registry.try_register(UnifiedCodec::xml("uima"));
        let _ = registry.try_register(UnifiedCodec::xml("folia"));
        let _ = registry.try_register(UnifiedCodec::xml("tei"));
        let _ = registry.try_register(UnifiedCodec::xml("timeml"));
        let _ = registry.try_register(UnifiedCodec::xml("elan"));
        let _ = registry.try_register(UnifiedCodec::xml("iso_space"));
        let _ = registry.try_register(UnifiedCodec::xml("paula"));
        let _ = registry.try_register(UnifiedCodec::xml("laf_graf"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        use crate::xml_codec::XmlCodec;

        // JSON-based
        registry.register(JsonCodec::new("brat"));
        registry.register(JsonCodec::new("decomp"));
        registry.register(JsonCodec::new("ucca"));
        registry.register(JsonCodec::new("fovea"));
        registry.register(JsonCodec::new("bead"));
        registry.register(JsonCodec::new("web_annotation"));
        registry.register(JsonCodec::new("concrete"));
        registry.register(JsonCodec::new("nif"));

        // XML-based
        registry.register(XmlCodec::new("naf"));
        registry.register(XmlCodec::new("uima"));
        registry.register(XmlCodec::new("folia"));
        registry.register(XmlCodec::new("tei"));
        registry.register(XmlCodec::new("timeml"));
        registry.register(XmlCodec::new("elan"));
        registry.register(XmlCodec::new("iso_space"));
        registry.register(XmlCodec::new("paula"));
        registry.register(XmlCodec::new("laf_graf"));
    }
}
