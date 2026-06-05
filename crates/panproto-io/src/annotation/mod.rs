//! Instance codecs for linguistic annotation protocols.
//!
//! - JSON-based: brat, decomp, ucca, fovea, bead, web_annotation, concrete, nif
//! - XML-based: naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf
//! - Tab-delimited: conllu
//! - Line-based: amr

pub mod conllu;

use crate::byte_tabular::ByteTabularCodec;
use crate::registry::ProtocolRegistry;

/// Register all annotation protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    // CoNLL-U: tab-delimited tokens with `#` comment lines and blank-line
    // sentence boundaries, no tree-sitter grammar. The byte-faithful tabular
    // codec preserves comments, blank lines, multiword-token rows, `_`
    // sentinels, and line endings verbatim, splicing only edited cells.
    registry.register(ByteTabularCodec::new("conllu", "rows", b'\t', Some(b'#')));

    // Line-based (AMR PENMAN notation represented as TSV)
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::tsv("amr", "amr_graph"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        registry.register(ByteTabularCodec::new("amr", "amr_graph", b'\t', None));
    }

    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;

        // JSON-based
        registry.register_optional(UnifiedCodec::json("brat"));
        registry.register_optional(UnifiedCodec::json("decomp"));
        registry.register_optional(UnifiedCodec::json("ucca"));
        registry.register_optional(UnifiedCodec::json("fovea"));
        registry.register_optional(UnifiedCodec::json("bead"));
        registry.register_optional(UnifiedCodec::json("web_annotation"));
        registry.register_optional(UnifiedCodec::json("concrete"));
        registry.register_optional(UnifiedCodec::json("nif"));

        // XML-based
        registry.register_optional(UnifiedCodec::xml("naf"));
        registry.register_optional(UnifiedCodec::xml("uima"));
        registry.register_optional(UnifiedCodec::xml("folia"));
        registry.register_optional(UnifiedCodec::xml("tei"));
        registry.register_optional(UnifiedCodec::xml("timeml"));
        registry.register_optional(UnifiedCodec::xml("elan"));
        registry.register_optional(UnifiedCodec::xml("iso_space"));
        registry.register_optional(UnifiedCodec::xml("paula"));
        registry.register_optional(UnifiedCodec::xml("laf_graf"));
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
