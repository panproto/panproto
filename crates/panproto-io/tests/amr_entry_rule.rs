//! The AMR codec applies one entry rule whichever build registers it.
//!
//! The default build registers a byte-tabular codec for `amr`, the
//! `tree-sitter` build the TSV `UnifiedCodec`. Both read rows into the
//! configured table vertex `amr_graph`, and a codec configured with a table
//! vertex refuses to anchor rows at a vertex the schema does not declare.
//! So every schema-taking route of the registry's `amr` codec, parse and
//! emit, canonical and preserving, refuses a schema without `amr_graph`
//! with the same error in either build, and accepts one that declares it.

use std::error::Error;

use panproto_io::byte_tabular::ByteTabularCodec;
use panproto_io::registry::ProtocolCodec;
use panproto_io::{EmitInstanceError, ParseInstanceError, default_registry};
use panproto_schema::{Protocol, Schema, SchemaBuilder};

type TestResult = Result<(), Box<dyn Error>>;

const INPUT: &[u8] = b"variable\tconcept\trole\ttarget\nb\tboy\t_\t_\nw\twant-01\tARG0\tb\n";
const MISSING: &str = "configured entry vertex 'amr_graph' does not exist in schema";

fn schema(with_table: bool) -> Result<Schema, Box<dyn Error>> {
    let proto = Protocol {
        name: "fixture".into(),
        schema_theory: "ThfixtureSchema".into(),
        instance_theory: "ThfixtureInstance".into(),
        ..Default::default()
    };
    let mut builder = SchemaBuilder::new(&proto).vertex("root", "object", None)?;
    if with_table {
        builder = builder.vertex("amr_graph", "table", None)?;
    }
    Ok(builder.build()?)
}

/// The outcome of every schema-taking route of `codec` under `schema`, as
/// text: `ok` or the error. The emit routes emit what the same codec
/// parsed under the schema that declares the table.
fn outcomes(codec: &dyn ProtocolCodec, schema: &Schema) -> Result<Vec<String>, Box<dyn Error>> {
    let (table, complement) = codec.parse_functor_preserving(&self::schema(true)?, INPUT)?;
    let parse = |r: Result<(), ParseInstanceError>| match r {
        Ok(()) => "ok".to_owned(),
        Err(ParseInstanceError::SchemaMismatch { protocol, message }) => {
            format!("parse SchemaMismatch {protocol}: {message}")
        }
        Err(other) => format!("parse other: {other}"),
    };
    let emit = |r: Result<(), EmitInstanceError>| match r {
        Ok(()) => "ok".to_owned(),
        Err(EmitInstanceError::Emit { protocol, message }) => {
            format!("emit Emit {protocol}: {message}")
        }
        Err(other) => format!("emit other: {other}"),
    };
    Ok(vec![
        parse(codec.parse_functor(schema, INPUT).map(drop)),
        parse(codec.parse_functor_preserving(schema, INPUT).map(drop)),
        emit(codec.emit_functor(schema, &table).map(drop)),
        emit(
            codec
                .emit_functor_preserving(schema, &table, complement.as_ref())
                .map(drop),
        ),
    ])
}

#[test]
fn the_registered_amr_codec_refuses_a_schema_without_its_table() -> TestResult {
    let registry = default_registry();
    let missing = schema(false)?;
    let refused = registry.parse_functor("amr", &missing, INPUT).err();
    assert!(
        matches!(
            &refused,
            Some(ParseInstanceError::SchemaMismatch { protocol, message })
                if protocol == "amr" && message == MISSING
        ),
        "{refused:?}"
    );
    let table = registry.parse_functor("amr", &schema(true)?, INPUT)?;
    assert!(!table.rows("amr_graph").is_empty());
    let refused = registry.emit_functor("amr", &missing, &table).err();
    assert!(
        matches!(
            &refused,
            Some(EmitInstanceError::Emit { protocol, message })
                if protocol == "amr" && message == MISSING
        ),
        "{refused:?}"
    );
    Ok(())
}

/// The byte-tabular route as the default build registers it applies the
/// rule on each schema-taking route.
#[test]
fn the_byte_tabular_amr_route_applies_the_entry_rule() -> TestResult {
    let codec = ByteTabularCodec::new("amr", "amr_graph", b'\t', None).with_schema_entry();
    assert_eq!(
        outcomes(&codec, &schema(false)?)?,
        vec![
            format!("parse SchemaMismatch amr: {MISSING}"),
            format!("parse SchemaMismatch amr: {MISSING}"),
            format!("emit Emit amr: {MISSING}"),
            format!("emit Emit amr: {MISSING}"),
        ]
    );
    assert_eq!(outcomes(&codec, &schema(true)?)?, vec!["ok"; 4]);
    Ok(())
}

/// Both routes, run side by side in one build, give the same outcome on
/// every schema-taking route, with and without the table.
#[cfg(feature = "tree-sitter")]
#[test]
fn both_amr_routes_agree() -> TestResult {
    use panproto_io::unified_codec::UnifiedCodec;

    let byte_tabular = ByteTabularCodec::new("amr", "amr_graph", b'\t', None).with_schema_entry();
    let unified = UnifiedCodec::tsv("amr", "amr_graph")?;
    for with_table in [false, true] {
        let schema = schema(with_table)?;
        assert_eq!(
            outcomes(&byte_tabular, &schema)?,
            outcomes(&unified, &schema)?,
            "with_table = {with_table}"
        );
    }
    Ok(())
}
