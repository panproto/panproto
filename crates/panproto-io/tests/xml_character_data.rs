//! Character data survives the parse, whatever events it arrives as.
//!
//! `quick-xml` 0.41 stopped folding references into the surrounding
//! text: `a &amp; b` arrives as `Text("a ")`, `GeneralRef("amp")`,
//! `Text(" b")` rather than as one already-unescaped `Text`. Code that
//! wrote each `Text` event straight onto the node kept only the last
//! fragment, so the upgrade is a text-pathway migration and not a
//! version bump.
//!
//! CDATA is the same question asked a different way: it was reaching a
//! catch-all arm and being discarded, so an element whose whole content
//! was a CDATA section read back with no value at all.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Write as _;

use panproto_inst::value::{FieldPresence, Value};
use panproto_io::xml_pathway::parse_xml_bytes;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("doc", "object", None)
        .expect("doc vertex")
        .build()
        .expect("build schema")
}

/// The root element's text value, or `None` when it has none.
fn root_text(xml: &str) -> Option<String> {
    let schema = open_schema();
    let instance = parse_xml_bytes(&schema, xml.as_bytes(), "test").expect("parses");
    let root = instance.nodes.get(&instance.root).expect("root node");
    match &root.value {
        Some(FieldPresence::Present(Value::Str(s))) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn plain_text_is_the_element_value() {
    assert_eq!(root_text("<doc>hello</doc>").as_deref(), Some("hello"));
}

/// The migration's whole point: text split across events by a reference
/// is one value, not the last fragment.
#[test]
fn text_interrupted_by_an_entity_is_reassembled() {
    assert_eq!(
        root_text("<doc>a &amp; b</doc>").as_deref(),
        Some("a & b"),
        "an entity reference must not truncate the text around it",
    );
}

#[test]
fn every_predefined_entity_resolves() {
    assert_eq!(
        root_text("<doc>&lt;&gt;&amp;&apos;&quot;</doc>").as_deref(),
        Some("<>&'\""),
    );
}

#[test]
fn a_decimal_character_reference_resolves() {
    assert_eq!(root_text("<doc>&#65;&#66;</doc>").as_deref(), Some("AB"));
}

#[test]
fn a_hexadecimal_character_reference_resolves() {
    assert_eq!(root_text("<doc>&#x41;&#x42;</doc>").as_deref(), Some("AB"));
}

/// A document-defined entity is not resolved: expanding one is what the
/// billion-laughs class of attack turns on. It is written back
/// literally rather than dropped, so no character disappears.
#[test]
fn an_unknown_entity_is_preserved_literally_rather_than_dropped() {
    assert_eq!(
        root_text("<doc>x &custom; y</doc>").as_deref(),
        Some("x &custom; y"),
    );
}

#[test]
fn cdata_is_character_data() {
    assert_eq!(
        root_text("<doc><![CDATA[raw <not> markup]]></doc>").as_deref(),
        Some("raw <not> markup"),
        "a CDATA section is the element's content, not something to skip",
    );
}

/// CDATA does not unescape: `&amp;` inside it is three characters plus
/// two, not one ampersand.
#[test]
fn cdata_content_is_not_unescaped() {
    assert_eq!(
        root_text("<doc><![CDATA[a &amp; b]]></doc>").as_deref(),
        Some("a &amp; b"),
    );
}

#[test]
fn text_and_cdata_in_one_element_concatenate() {
    assert_eq!(
        root_text("<doc>before <![CDATA[middle]]> after</doc>").as_deref(),
        Some("before middle after"),
    );
}

/// Whitespace between elements is layout, not a value.
#[test]
fn whitespace_only_content_is_not_a_value() {
    assert_eq!(root_text("<doc>\n   \n</doc>"), None);
}

/// Text belonging to one element must not leak into the next, which is
/// the failure an accumulating buffer invites if it is never cleared.
#[test]
fn text_does_not_leak_between_siblings() {
    let schema = open_schema();
    let xml = "<doc><a>first</a><b></b></doc>";
    let instance = parse_xml_bytes(&schema, xml.as_bytes(), "test").expect("parses");

    let values: Vec<Option<&str>> = {
        let mut ids: Vec<&u32> = instance.nodes.keys().collect();
        ids.sort();
        ids.iter()
            .map(|id| match &instance.nodes[id].value {
                Some(FieldPresence::Present(Value::Str(s))) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        values.iter().filter(|v| **v == Some("first")).count(),
        1,
        "exactly one element carries the text, got {values:?}",
    );
}

// ---------------------------------------------------------------------------
// The advisories this upgrade clears
// ---------------------------------------------------------------------------

/// RUSTSEC-2026-0194 is a quadratic duplicate-attribute scan. The
/// upgraded reader must answer a document built to exercise it in
/// bounded time rather than hanging.
#[test]
fn a_document_with_many_duplicate_attributes_terminates() {
    let mut attrs = String::new();
    for i in 0..2_000 {
        write!(attrs, " a{i}=\"v\"").expect("writing to a String cannot fail");
    }
    let xml = format!("<doc{attrs}>text</doc>");

    let schema = open_schema();
    let start = std::time::Instant::now();
    let instance = parse_xml_bytes(&schema, xml.as_bytes(), "test").expect("parses");
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "2000 attributes took {elapsed:?}, which is the quadratic curve",
    );
    let root = instance.nodes.get(&instance.root).expect("root node");
    assert_eq!(root.extra_fields.len(), 2_000, "every attribute is kept");
}

/// RUSTSEC-2026-0195 concerns namespace resolution. This pathway uses a
/// `Reader` without it, and a namespace-heavy document must still parse
/// in bounded time.
#[test]
fn a_namespace_heavy_document_terminates() {
    let mut ns = String::new();
    for i in 0..1_000 {
        write!(ns, " xmlns:n{i}=\"urn:example:{i}\"").expect("writing to a String cannot fail");
    }
    let xml = format!("<doc{ns}>text</doc>");

    let schema = open_schema();
    let start = std::time::Instant::now();
    let result = parse_xml_bytes(&schema, xml.as_bytes(), "test");
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "a namespace-heavy document parses");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "1000 namespace declarations took {elapsed:?}",
    );
}
