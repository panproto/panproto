//! `.musicxml` files dispatch to the existing tree-sitter-xml grammar.
//!
//! Per the W3C Music Notation Community Group's `MusicXML` 4.0
//! specification, a `.musicxml` file is plain XML; the schema
//! (DTD / XSD) constrains element names and attribute values, not the
//! lexical form. So a fresh grammar is unnecessary — the existing
//! tree-sitter-xml grammar handles the file as-is. This test confirms
//! the extension dispatcher routes `.musicxml` to `xml` and that a
//! representative fragment recovers the expected structural kinds.

#![cfg(all(feature = "grammars", feature = "lang-xml"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

/// A minimal but well-formed `MusicXML` excerpt: declaration, partwise
/// score with one part containing one measure of a single C4 quarter
/// note. Drawn from the `MusicXML` 4.0 "hello world" sample at
/// https://www.w3.org/2021/06/musicxml40/tutorial/hello-world/.
const HELLO_MUSICXML: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE score-partwise PUBLIC
    "-//Recordare//DTD MusicXML 4.0 Partwise//EN"
    "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0">
  <part-list>
    <score-part id="P1">
      <part-name>Music</part-name>
    </score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>1</divisions>
        <key>
          <fifths>0</fifths>
        </key>
        <time>
          <beats>4</beats>
          <beat-type>4</beat-type>
        </time>
        <clef>
          <sign>G</sign>
          <line>2</line>
        </clef>
      </attributes>
      <note>
        <pitch>
          <step>C</step>
          <octave>4</octave>
        </pitch>
        <duration>4</duration>
        <type>whole</type>
      </note>
    </measure>
  </part>
</score-partwise>
"#;

#[test]
fn musicxml_extension_routes_to_xml_protocol() {
    let reg = ParserRegistry::new();
    let detected = reg.detect_language(std::path::Path::new("hello.musicxml"));
    assert_eq!(
        detected,
        Some("xml"),
        ".musicxml extension must dispatch to the xml protocol"
    );
}

#[test]
fn musicxml_parses_through_xml_grammar() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("xml", HELLO_MUSICXML, "hello.musicxml")
        .expect("xml parser must accept a well-formed `MusicXML` score");

    let kinds: std::collections::BTreeSet<String> = schema
        .vertices
        .values()
        .map(|v| v.kind.to_string())
        .collect();

    // tree-sitter-xml recognises elements as `STag` / `ETag` /
    // `element` (the exact set varies with the grammar version;
    // present here are the structural names common to every release
    // of tree-sitter-xml that ships in panproto-grammars).
    assert!(
        kinds.contains("document"),
        "musicxml schema missing `document` root; got {kinds:?}"
    );
    assert!(
        kinds.contains("element"),
        "musicxml schema missing `element`; got {kinds:?}"
    );
    // Non-trivial structural recovery: the score has many nested
    // elements, attributes, and a doctype.
    assert!(schema.vertices.len() >= 20, "non-trivial `MusicXML` schema");
}
