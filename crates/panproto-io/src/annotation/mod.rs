//! Instance codecs for linguistic annotation protocols.
//!
//! - JSON-based: brat, decomp, ucca, fovea, bead, web_annotation, concrete, nif
//! - XML-based: naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf
//! - Tab-delimited: conllu
//! - Line-based: amr

pub mod conllu;

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::tabular_codec::TabularCodec;
use crate::xml_codec::XmlCodec;

/// Register all annotation protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
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

    // Tab-delimited
    registry.register(conllu::ConlluCodec::new());

    // Line-based (AMR PENMAN notation represented as TSV)
    registry.register(TabularCodec::tsv("amr", "amr_graph"));
}
