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
        registry.register(UnifiedCodec::tsv("amr", "amr_graph"));
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
        registry.register(UnifiedCodec::json("brat"));
        registry.register(UnifiedCodec::json("decomp"));
        registry.register(UnifiedCodec::json("ucca"));
        registry.register(UnifiedCodec::json("fovea"));
        registry.register(UnifiedCodec::json("bead"));
        registry.register(UnifiedCodec::json("web_annotation"));
        registry.register(UnifiedCodec::json("concrete"));
        registry.register(UnifiedCodec::json("nif"));

        // XML-based
        registry.register(UnifiedCodec::xml("naf"));
        registry.register(UnifiedCodec::xml("uima"));
        registry.register(UnifiedCodec::xml("folia"));
        registry.register(UnifiedCodec::xml("tei"));
        registry.register(UnifiedCodec::xml("timeml"));
        registry.register(UnifiedCodec::xml("elan"));
        registry.register(UnifiedCodec::xml("iso_space"));
        registry.register(UnifiedCodec::xml("paula"));
        registry.register(UnifiedCodec::xml("laf_graf"));
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
