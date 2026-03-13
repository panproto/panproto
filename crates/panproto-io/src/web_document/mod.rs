//! Instance codecs for web and document protocols.
//!
//! - `ATProto` records use JSON encoding (via `JsonCodec`)
//! - HTML uses SIMD-accelerated `tl` parser
//! - Markdown uses `pulldown-cmark` pull parser
//! - XML/XSD, CSS, JSX, Vue, Svelte, DOCX, ODF use XML or specialized parsers

pub mod html;
pub mod markdown;

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::xml_codec::XmlCodec;

/// Register all web/document protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    // JSON-based
    registry.register(JsonCodec::new("atproto"));

    // SIMD HTML parser
    registry.register(html::HtmlCodec::new());

    // Markdown pull parser
    registry.register(markdown::MarkdownCodec::new());

    // XML-based
    registry.register(XmlCodec::new("xml_xsd"));
    registry.register(XmlCodec::new("docx"));
    registry.register(XmlCodec::new("odf"));

    // These use JSON for their AST representation
    registry.register(JsonCodec::new("jsx"));
    registry.register(JsonCodec::new("vue"));
    registry.register(JsonCodec::new("svelte"));
    registry.register(JsonCodec::new("css"));
}
