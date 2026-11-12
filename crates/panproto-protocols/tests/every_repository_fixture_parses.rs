//! Every schema fixture under the repository's `fixtures/` directory
//! parses with its protocol's parser, into the schema the file declares.
//!
//! The fixtures are real-world schemas (protobuf's own `descriptor.proto`
//! among them), so each one exercises constructs the parsers' unit tests do
//! not, such as option values spanning lines. Each construct has a minimal case of
//! its own below, and the parse is checked against what
//! the file declares, not only for success.

use std::path::{Path, PathBuf};

use panproto_protocols::parse_schema_source;
use panproto_schema::Schema;

/// A result a test requires to be `Ok`.
trait Required<T> {
    /// The value, or a panic naming `what` and the error.
    fn required(self, what: &str) -> T;
}

impl<T, E: std::fmt::Display> Required<T> for Result<T, E> {
    fn required(self, what: &str) -> T {
        self.unwrap_or_else(|e| panic!("{what}: {e}"))
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
}

fn source(protocol: &str, relative: &str) -> Schema {
    let path = fixture(relative);
    let text = std::fs::read_to_string(&path).required("the fixture exists");
    parse_schema_source(protocol, &text).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn has_edge(schema: &Schema, src: &str, tgt: &str, kind: &str) -> bool {
    schema
        .outgoing_edges(src)
        .iter()
        .any(|e| e.tgt == tgt && e.kind == kind)
}

fn constraint<'a>(schema: &'a Schema, vertex: &str, sort: &str) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex)?
        .iter()
        .find(|c| c.sort == sort)
        .map(|c| c.value.as_str())
}

// Protobuf.

#[test]
fn an_option_value_spanning_lines_is_one_statement() {
    let schema = parse_schema_source(
        "protobuf",
        r#"
message M {
  // a comment with a } brace
  optional int32 a = 1
      [default = 3, deprecated = true];
  extensions 100 to 200 [declaration = {
    number: 100
    full_name: ".x.y"
  }];
  /* reserved 5; */ optional string b = 2;
  enum E { option allow_alias = true; X = 0 [deprecated = true]; Y = 1; }
  optional E e = 3;
  map<string, M> children = 4;
}
"#,
    )
    .required("the message parses");
    for v in ["M.a", "M.b", "M.e", "M.E.X", "M.E.Y", "M.children"] {
        assert!(schema.has_vertex(v), "{v} is declared");
    }
    assert_eq!(constraint(&schema, "M.a", "field_number"), Some("1"));
    assert!(
        has_edge(&schema, "M.e", "M.E", "type-of"),
        "nested enum resolves"
    );
    assert!(has_edge(&schema, "M.children:value", "M", "type-of"));
    assert_eq!(schema.vertices.len(), 10);
}

#[test]
fn the_protobuf_descriptor_parses() {
    let schema = source("protobuf", "protobuf/descriptor.proto");
    // The extension declaration's `{ ... }` does not close the message.
    assert!(schema.has_vertex("FileDescriptorSet.file"));
    assert!(has_edge(
        &schema,
        "FileDescriptorSet.file",
        "FileDescriptorProto",
        "type-of"
    ));
    // A field typed by an enum nested in its message.
    assert!(has_edge(
        &schema,
        "FileOptions.optimize_for",
        "FileOptions.OptimizeMode",
        "type-of"
    ));
    assert_eq!(
        constraint(&schema, "FileOptions.optimize_for", "field_number"),
        Some("9")
    );
    assert!(has_edge(
        &schema,
        "FieldDescriptorProto.type",
        "FieldDescriptorProto.Type",
        "type-of"
    ));
    assert!(
        !schema.vertices.keys().any(|v| v.ends_with(".=")),
        "no option text is read as a field"
    );
}

#[test]
fn an_rpc_refers_to_its_request_and_response_apart() {
    let schema = parse_schema_source(
        "protobuf",
        "message M { string a = 1; }\nmessage N { string b = 1; }\n\
         service S {\n  rpc Echo(M) returns (M);\n  rpc Swap(stream M) returns (N);\n}",
    )
    .required("an rpc with one message both ways parses");
    assert!(has_edge(&schema, "S.Echo", "M", "input-of"));
    assert!(has_edge(&schema, "S.Echo", "M", "output-of"));
    assert!(has_edge(&schema, "S.Swap", "M", "input-of"));
    assert!(has_edge(&schema, "S.Swap", "N", "output-of"));
    assert!(!has_edge(&schema, "S.Swap", "N", "input-of"));
    assert!(
        !schema
            .edges
            .keys()
            .any(|e| e.src == "S.Echo" && e.kind == "type-of"),
        "an rpc's types are not field types"
    );
}
