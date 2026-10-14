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
