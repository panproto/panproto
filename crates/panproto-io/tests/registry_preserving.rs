//! Registry-level format-preservation dispatch.
//!
//! Exercises [`ProtocolRegistry::parse_wtype_preserving_or_canonical`]
//! and [`ProtocolRegistry::emit_wtype_preserving_or_canonical`], the
//! entry points that are available in every build. With the
//! `tree-sitter` feature compiled in they route through the
//! format-preserving `UnifiedCodec` and yield a byte-equal round-trip;
//! without it they fall back to a canonical codec and return no
//! complement.

#![allow(clippy::expect_used, clippy::unwrap_used, deprecated)]

use panproto_io::ProtocolRegistry;
use panproto_schema::{Protocol, SchemaBuilder};

/// With the `tree-sitter` feature, the registry's always-available
/// preserving entry points route through the format-preserving
/// `UnifiedCodec` and reproduce the input byte for byte, including the
/// non-canonical whitespace that a canonical emitter would normalize.
#[cfg(feature = "tree-sitter")]
#[test]
fn registry_preserving_round_trip_is_byte_equal() {
    use panproto_io::unified_codec::UnifiedCodec;

    let proto = Protocol {
        name: "demo".into(),
        schema_theory: "ThDemoSchema".into(),
        instance_theory: "ThDemoInstance".into(),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema");

    let mut registry = ProtocolRegistry::new();
    registry.register(UnifiedCodec::json("demo").expect("json codec"));

    // Indentation and spacing a canonical serializer would collapse; a
    // preserving round-trip must keep every byte.
    let input = b"{\n  \"name\": \"Alice\",\n  \"age\": 30\n}\n";

    let (instance, complement) = registry
        .parse_wtype_preserving_or_canonical("demo", &schema, input)
        .expect("preserving parse");
    assert!(
        complement.is_some(),
        "a tree-sitter build must capture a CST complement for a preserving codec"
    );

    let emitted = registry
        .emit_wtype_preserving_or_canonical("demo", &schema, &instance, complement.as_ref())
        .expect("preserving emit");
    assert_eq!(
        emitted,
        input.as_slice(),
        "the registry preserving round-trip must be byte-equal"
    );
}

/// The generic text-format protocols (`yaml`, `toml`) are wired into
/// [`default_registry`](panproto_io::default_registry) through the
/// format-preserving `UnifiedCodec`: parsing through the registry
/// captures a CST complement and the round-trip is byte-equal, including
/// comments and irregular spacing a canonical emitter would normalize.
#[cfg(feature = "tree-sitter")]
#[test]
fn default_registry_wires_preserving_text_format_codecs() {
    let cases: [(&str, &[u8]); 2] = [
        ("yaml", b"# header\nname: Alice\nage:   30\n"),
        ("toml", b"# header\ntitle = \"demo\"\nport  = 8080\n"),
    ];

    for (protocol, input) in cases {
        let proto = Protocol {
            name: protocol.into(),
            schema_theory: format!("Th{protocol}Schema"),
            instance_theory: format!("Th{protocol}Instance"),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .expect("root vertex")
            .build()
            .expect("build schema");

        let registry = panproto_io::default_registry();
        let (instance, complement) = registry
            .parse_wtype_preserving_or_canonical(protocol, &schema, input)
            .expect("preserving parse");
        assert!(
            complement.is_some(),
            "{protocol}: default_registry must wire the format-preserving codec"
        );

        let emitted = registry
            .emit_wtype_preserving_or_canonical(protocol, &schema, &instance, complement.as_ref())
            .expect("preserving emit");
        assert_eq!(
            emitted, input,
            "{protocol}: registry preserving round-trip must be byte-equal"
        );
    }
}

/// The `csv` protocol is likewise wired into the default registry, as a
/// format-preserving functor codec (`Functor` native representation).
#[cfg(feature = "tree-sitter")]
#[test]
fn default_registry_registers_csv_as_functor() {
    use panproto_io::traits::NativeRepr;
    let registry = panproto_io::default_registry();
    assert_eq!(
        registry.native_repr("csv").expect("csv registered"),
        NativeRepr::Functor,
        "csv should register as a functor codec"
    );
}

/// Without the feature, the same entry points fall back to the canonical
/// codec: no complement is produced and canonical emission still yields
/// a re-parseable document.
#[cfg(not(feature = "tree-sitter"))]
#[test]
fn registry_preserving_falls_back_to_canonical_without_feature() {
    use panproto_io::json_codec::JsonCodec;

    let proto = Protocol {
        name: "demo".into(),
        schema_theory: "ThDemoSchema".into(),
        instance_theory: "ThDemoInstance".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .vertex("root:name", "string", None)
        .expect("name vertex")
        .edge("root", "root:name", "prop", Some("name"))
        .expect("name edge")
        .build()
        .expect("build schema");

    let mut registry = ProtocolRegistry::new();
    registry.register(JsonCodec::new("demo"));

    let input = br#"{"name": "Alice"}"#;

    let (instance, complement) = registry
        .parse_wtype_preserving_or_canonical("demo", &schema, input)
        .expect("canonical fallback parse");
    assert!(
        complement.is_none(),
        "a build without tree-sitter cannot capture a complement"
    );

    let emitted = registry
        .emit_wtype_preserving_or_canonical("demo", &schema, &instance, None)
        .expect("canonical fallback emit");
    let reparsed = registry
        .parse_wtype("demo", &schema, &emitted)
        .expect("canonical emit must re-parse");
    assert_eq!(
        instance.node_count(),
        reparsed.node_count(),
        "canonical fallback must round-trip structurally"
    );
}
