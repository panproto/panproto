//! The GraphQL reader keeps what a type system document says and refuses
//! what it does not read, by rule and place.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::api::graphql::{
    GraphQlErrorKind, NotRead, Rule, read_graphql, read_graphql_bundle, string_text, write_graphql,
};
use panproto_protocols::api::graphql::SCHEMA_VERTEX;
use panproto_schema::Schema;

fn kept<'s>(schema: &'s Schema, vertex: &str, sort: &str) -> Vec<&'s str> {
    schema
        .constraints
        .get(vertex)
        .map(|cs| {
            cs.iter()
                .filter(|c| c.sort.as_ref() == sort)
                .map(|c| c.value.as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn edges<'s>(schema: &'s Schema, vertex: &str, kind: &str) -> Vec<(&'s str, Option<&'s str>)> {
    schema
        .outgoing_edges(vertex)
        .iter()
        .filter(|e| e.kind.as_ref() == kind)
        .map(|e| (e.tgt.as_ref(), e.name.as_deref()))
        .collect()
}

fn kind<'s>(schema: &'s Schema, vertex: &str) -> &'s str {
    schema.vertices.get(vertex).map_or("", |v| v.kind.as_ref())
}

fn same(left: &Schema, right: &Schema) -> bool {
    panproto_schema::canonical_bytes(left) == panproto_schema::canonical_bytes(right)
}

const LIBRARY: &str = r#"
"""
The library.
"""
schema @audit(level: 2) {
  query: Root
  mutation: Change
}

"Marks a definition for audit."
directive @audit(level: Int = 1, tags: [String!]) repeatable on SCHEMA | OBJECT | FIELD_DEFINITION

scalar Date @specifiedBy(url: "https://scalars.graphql.org/andimarek/date-time")

interface Node { id: ID! }

interface Item implements Node {
  id: ID!
  title(locale: Locale = EN): String
}

type Book implements Node & Item @audit @audit(tags: ["a", "b"]) {
  id: ID!
  title(locale: Locale = EN, short: Boolean): String
  isbn: String @deprecated
  published: Date @deprecated(reason: "Use `released`.")
  released: Date
}

type Root {
  item(id: ID!): Item
  search(filter: Filter = {shelves: [1, 2]}): [Result!]!
}

type Change { borrow(id: ID!): Book }

union Result = | Book

enum Locale {
  "English"
  EN
  FR @deprecated(reason: """
    No longer
    printed.
  """)
}

input Filter @oneOf {
  text: String
  shelves: [Int]
}

extend type Book { pages: Int }
extend enum Locale { DE }
"#;

#[test]
fn a_type_system_is_read_into_its_vertices_edges_and_constraints() {
    let schema = read_graphql(LIBRARY.as_bytes()).unwrap().schema;
    assert_eq!(kind(&schema, SCHEMA_VERTEX), "schema");
    assert_eq!(
        edges(&schema, SCHEMA_VERTEX, "root"),
        [("Root", Some("query")), ("Change", Some("mutation"))]
    );
    assert_eq!(
        kept(&schema, SCHEMA_VERTEX, "description"),
        ["The library."]
    );
    assert_eq!(
        kept(&schema, SCHEMA_VERTEX, "directive"),
        ["@audit(level: 2)"]
    );
    assert_eq!(kind(&schema, "@audit"), "directive");
    assert_eq!(kept(&schema, "@audit", "repeatable"), ["true"]);
    assert_eq!(
        kept(&schema, "@audit", "location"),
        ["SCHEMA", "OBJECT", "FIELD_DEFINITION"]
    );
    assert_eq!(kept(&schema, "@audit(level:)", "default"), ["1"]);
    assert_eq!(kept(&schema, "@audit(tags:)", "type"), ["[String!]"]);
    assert_eq!(
        kept(&schema, "Date", "specified-by"),
        ["https://scalars.graphql.org/andimarek/date-time"]
    );
    assert_eq!(
        edges(&schema, "Item", "implements"),
        [("Node", Some("Node"))]
    );
    assert_eq!(
        edges(&schema, "Book", "implements"),
        [("Node", Some("Node")), ("Item", Some("Item"))]
    );
    assert_eq!(
        edges(&schema, "Book", "field")
            .iter()
            .map(|(_, n)| n.unwrap())
            .collect::<Vec<_>>(),
        ["id", "title", "isbn", "published", "released", "pages"]
    );
    assert_eq!(
        kept(&schema, "Book", "directive"),
        ["@audit", "@audit(tags: [\"a\", \"b\"])"]
    );
    assert_eq!(
        kept(&schema, "Book.isbn", "deprecated"),
        ["No longer supported"]
    );
    assert_eq!(
        kept(&schema, "Book.published", "deprecated"),
        ["Use `released`."]
    );
    assert_eq!(edges(&schema, "Book.id", "type"), [("ID", None)]);
    assert_eq!(kept(&schema, "ID", "built-in"), ["true"]);
    assert_eq!(kept(&schema, "Book.title(locale:)", "default"), ["EN"]);
    assert_eq!(kept(&schema, "Root.search", "type"), ["[Result!]!"]);
    assert_eq!(
        kept(&schema, "Root.search(filter:)", "default"),
        ["{shelves: [1, 2]}"]
    );
    assert_eq!(edges(&schema, "Result", "member"), [("Book", Some("Book"))]);
    assert_eq!(
        edges(&schema, "Locale", "value")
            .iter()
            .map(|(t, _)| *t)
            .collect::<Vec<_>>(),
        ["Locale.EN", "Locale.FR", "Locale.DE"]
    );
    assert_eq!(kept(&schema, "Locale.EN", "description"), ["English"]);
    assert_eq!(
        kept(&schema, "Locale.FR", "deprecated"),
        ["No longer\nprinted."]
    );
    assert_eq!(kept(&schema, "Filter", "one-of"), ["true"]);
    assert_eq!(kind(&schema, "Filter.text"), "input-field");
}

#[test]
fn the_canonical_text_reads_back_as_the_same_schema() {
    let schema = read_graphql(LIBRARY.as_bytes()).unwrap().schema;
    let text = write_graphql(&schema).unwrap();
    let again = read_graphql(text.as_bytes())
        .unwrap_or_else(|e| panic!("{text}\n{e}"))
        .schema;
    assert!(same(&schema, &again), "{text}");
    assert_eq!(write_graphql(&again).unwrap(), text);
}

/// Documents that differ only in the order of their definitions read as one
/// schema.
#[test]
fn the_order_of_definitions_is_no_part_of_the_schema() {
    let definitions = [
        "type Query { a: A b: B c(x: I): ID }",
        "type A { i: ID b: B }",
        "type B { i: ID a: A }",
        "input I { x: Int i: ID }",
        "extend type A { j: Int }",
    ];
    let read = |order: &[usize]| {
        let text: Vec<&str> = order.iter().map(|at| definitions[*at]).collect();
        read_graphql(text.join("\n").as_bytes()).unwrap().schema
    };
    let first = read(&[0, 1, 2, 3, 4]);
    for order in [[4, 3, 2, 1, 0], [2, 0, 4, 1, 3], [3, 4, 0, 2, 1]] {
        assert!(same(&first, &read(&order)), "{order:?}");
    }
}

#[test]
fn an_implicit_schema_takes_the_default_root_names() {
    let schema = read_graphql(b"type Query { a: Int } type Mutation { b: Int }")
        .unwrap()
        .schema;
    assert_eq!(
        edges(&schema, SCHEMA_VERTEX, "root"),
        [("Query", Some("query")), ("Mutation", Some("mutation"))]
    );
}

#[test]
fn a_bundle_is_one_type_system() {
    let reading = read_graphql_bundle(&[
        b"type Query { a: A }".as_slice(),
        b"type A { b: Int } extend type Query { c: A }".as_slice(),
    ])
    .unwrap();
    assert_eq!(edges(&reading.schema, "Query", "field").len(), 2);
    let err = read_graphql_bundle(&[
        b"type Query { a: Int }".as_slice(),
        b"type Query { b: Int }",
    ])
    .unwrap_err();
    assert_eq!(
        (err.document, err.path.as_str(), err.kind),
        (1, "Query", GraphQlErrorKind::Invalid(Rule::DuplicateType))
    );
}

/// The rule, schema coordinate and byte offset `source` is refused at.
fn refused(source: &str) -> (GraphQlErrorKind, String, usize) {
    let err = read_graphql(source.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("read: {source}"));
    (err.kind, err.path, err.offset)
}

fn rule(source: &str) -> (Rule, String) {
    match refused(source) {
        (GraphQlErrorKind::Invalid(rule), path, _) => (rule, path),
        other => panic!("{source}: {other:?}"),
    }
}

const Q: &str = "type Query { q: Int }\n";

/// Documents that break one rule after the definition of [`Q`], with the
/// rule and the schema coordinate of the definition at fault.
const BREAKING: &[(&str, Rule, &str)] = &[
    ("type __A { a: Int }", Rule::ReservedName, "__A"),
    ("type A { __a: Int }", Rule::ReservedName, "A.__a"),
    (
        "type A { a(x: String = \"a\rb\"): Int }",
        Rule::LineTerminatorInString,
        "A.a(x:)",
    ),
    ("enum E { true }", Rule::EnumValueName, "E.true"),
    ("type A @d(x: $v) { a: Int }", Rule::Variable, "A"),
    (
        "type A { a: Int } type A { b: Int }",
        Rule::DuplicateType,
        "A",
    ),
    ("scalar Int", Rule::BuiltInTypeName, "Int"),
    (
        "directive @d on OBJECT directive @d on OBJECT",
        Rule::DuplicateDirective,
        "@d",
    ),
    (
        "directive @deprecated on OBJECT",
        Rule::BuiltInDirectiveRedefined,
        "@deprecated",
    ),
    (
        "schema { query: Query } schema { query: Query }",
        Rule::SchemaDefinitions,
        SCHEMA_VERTEX,
    ),
    (
        "schema { query: Query query: Query }",
        Rule::DuplicateOperation,
        SCHEMA_VERTEX,
    ),
    (
        "schema { query: Query mutation: Query }",
        Rule::RootsNotDistinct,
        SCHEMA_VERTEX,
    ),
    (
        "extend schema { mutation: Query }",
        Rule::SchemaUndefined,
        SCHEMA_VERTEX,
    ),
    ("type A { a: B }", Rule::UnknownType, "A.a"),
    ("extend type A { a: Int }", Rule::ExtensionUndefined, "A"),
    (
        "enum A { X } extend type A { a: Int }",
        Rule::ExtensionKind,
        "A",
    ),
    ("union U = Query extend union U", Rule::EmptyExtension, "U"),
    ("type A", Rule::NoFields, "A"),
    ("type A { a: Int a: Int }", Rule::DuplicateField, "A.a"),
    (
        "input I { a: Int } type A { a: I }",
        Rule::NotOutputType,
        "A.a",
    ),
    ("type A { a(x: Query): Int }", Rule::NotInputType, "A.a(x:)"),
    (
        "type A { a(x: Int x: Int): Int }",
        Rule::DuplicateArgument,
        "A.a(x:)",
    ),
    (
        "type A { a(x: Int! @deprecated): Int }",
        Rule::DeprecatedRequired,
        "A.a(x:)",
    ),
    (
        "interface I { a: Int } type A implements I & I { a: Int }",
        Rule::DuplicateInterface,
        "A",
    ),
    (
        "type B { a: Int } type A implements B { a: Int }",
        Rule::ImplementsNonInterface,
        "A",
    ),
    (
        "interface I implements I { a: Int }",
        Rule::ImplementsSelf,
        "I",
    ),
    (
        "interface I { a: Int } interface J implements I { a: Int } type A implements J { a: Int }",
        Rule::TransitiveInterface,
        "A",
    ),
    (
        "interface I { a: Int } type A implements I { b: Int }",
        Rule::MissingInterfaceField,
        "A",
    ),
    (
        "interface I { a(x: Int): Int } type A implements I { a: Int }",
        Rule::MissingInterfaceArgument,
        "A.a",
    ),
    (
        "interface I { a(x: Int): Int } type A implements I { a(x: Int!): Int }",
        Rule::ArgumentNotInvariant,
        "A.a(x:)",
    ),
    (
        "interface I { a: Int } type A implements I { a(y: Int!): Int }",
        Rule::ExtraArgumentRequired,
        "A.a(y:)",
    ),
    (
        "interface I { a: Int! } type A implements I { a: Int }",
        Rule::FieldNotCovariant,
        "A.a",
    ),
    (
        "interface I { a: Int } type A implements I { a: Int @deprecated }",
        Rule::DeprecationNotInherited,
        "A.a",
    ),
    ("union U", Rule::NoMembers, "U"),
    (
        "interface I { a: Int } union U = I",
        Rule::MemberNotObject,
        "U",
    ),
    ("union U = Query | Query", Rule::DuplicateMember, "U"),
    ("enum E", Rule::NoValues, "E"),
    ("enum E { A A }", Rule::DuplicateValue, "E.A"),
    ("input I @oneOf { a: Int! }", Rule::OneOfNonNull, "I.a"),
    ("input I @oneOf { a: Int = 1 }", Rule::OneOfDefault, "I.a"),
    (
        "input I { a: Int } extend input I @oneOf",
        Rule::OneOfOnExtension,
        "I",
    ),
    (
        "input I { j: J! } input J { i: I! }",
        Rule::InputCycle,
        "J.i",
    ),
    ("input I { a: I = {} }", Rule::DefaultValueCycle, "I"),
    (
        "directive @d(x: Int @d) on ARGUMENT_DEFINITION",
        Rule::DirectiveSelfReference,
        "@d",
    ),
    (
        "directive @d(x: I) on ARGUMENT_DEFINITION input I { a: Int @d }",
        Rule::DirectiveSelfReference,
        "@d",
    ),
    ("type A @nope { a: Int }", Rule::DirectiveUndefined, "A"),
    (
        "type A @specifiedBy(url: \"u\") { a: Int }",
        Rule::DirectiveLocation,
        "A",
    ),
    (
        "type A { a: Int @deprecated @deprecated }",
        Rule::DirectiveRepeated,
        "A.a",
    ),
    (
        "type A { a: Int @deprecated(why: \"x\") }",
        Rule::ArgumentUndefined,
        "A.a",
    ),
    (
        "type A { a: Int @deprecated(reason: \"x\", reason: \"y\") }",
        Rule::ArgumentRepeated,
        "A.a",
    ),
    ("scalar S @specifiedBy", Rule::ArgumentMissing, "S"),
    (
        "type A { a(x: Int = \"1\"): Int }",
        Rule::ValueNotCoercible,
        "A.a(x:)",
    ),
    (
        "type A { a(x: Int = 2147483648): Int }",
        Rule::ValueNotCoercible,
        "A.a(x:)",
    ),
];

/// Whole documents that break one rule.
const BREAKING_WHOLE: &[(&str, Rule, &str)] = &[
    ("type A { a: Int }", Rule::QueryRootMissing, SCHEMA_VERTEX),
    ("scalar Query", Rule::RootNotObject, SCHEMA_VERTEX),
];

#[test]
fn every_checked_rule_is_refused_at_its_definition() {
    for (source, expected, path) in BREAKING {
        let source = format!("{Q}{source}");
        assert_eq!(rule(&source), (*expected, (*path).to_owned()), "{source}");
    }
    for (source, expected, path) in BREAKING_WHOLE {
        assert_eq!(rule(source), (*expected, (*path).to_owned()), "{source}");
    }
}

#[test]
fn values_coerce_as_the_input_coercion_rules_say() {
    let ok = [
        "Float = 1",
        "[Int] = 1",
        "[[Int]] = [1, null, 3]",
        "ID = 7",
        "ID = \"x\"",
        "Int = null",
        "E = A",
        "I = {a: 1}",
        "I = {a: 1, b: 2}",
        "J = {x: 1}",
    ];
    let bad = [
        "Float = 1e400",
        "Int! = null",
        "E = \"A\"",
        "E = B",
        "I = {}",
        "I = {a: 1, c: 2}",
        "I = {a: 1, a: 1}",
        "J = {}",
        "J = {x: 1, y: 2}",
        "J = {x: null}",
        "[Int] = [\"a\"]",
    ];
    let wrap = |ty: &str| {
        format!(
            "{Q}enum E {{ A }} input I {{ a: Int! b: Int = 3 }} input J @oneOf {{ x: Int y: Int }} \
             type A {{ a(x: {ty}): Int }}"
        )
    };
    for case in ok {
        read_graphql(wrap(case).as_bytes()).unwrap_or_else(|e| panic!("{case}: {e}"));
    }
    for case in bad {
        assert_eq!(
            rule(&wrap(case)),
            (Rule::ValueNotCoercible, "A.a(x:)".to_owned()),
            "{case}"
        );
    }
}

#[test]
fn constructs_the_grammar_does_not_parse_are_refused_by_name() {
    let gap = |source: &str, expected: NotRead| {
        assert_eq!(
            refused(source).0,
            GraphQlErrorKind::NotRead(expected),
            "{source}"
        );
    };
    gap(
        &format!("{Q}type A {{ a(x: String = \"a\\nb\"): Int }}"),
        NotRead::StringEscape,
    );
    gap(
        &format!("{Q}\"\\u00e9\" type A {{ a: Int }}"),
        NotRead::StringEscape,
    );
    gap(
        &format!("{Q}\"\"\" a \\\"\"\" b \"\"\" type A {{ a: Int }}"),
        NotRead::StringEscape,
    );
    gap(
        &format!("{Q}\"\"\"a\"\"\"\" scalar X"),
        NotRead::BlockStringQuote,
    );
    gap(
        &format!("{Q}schema {{ query: Query }} extend schema @a"),
        NotRead::SchemaExtension,
    );
    gap(
        &format!("{Q}schema {{ query: Query }} extend schema {{ mutation: A subscription: B }}"),
        NotRead::SchemaExtension,
    );
    gap(
        &format!("{Q}extend scalar Int @a"),
        NotRead::BuiltInScalarExtension,
    );
}

#[test]
fn executable_definitions_and_other_syntax_are_refused() {
    assert_eq!(
        refused(&format!("{Q}query {{ q }}")).0,
        GraphQlErrorKind::Executable("an operation")
    );
    assert_eq!(
        refused(&format!("{Q}fragment F on Query {{ q }}")).0,
        GraphQlErrorKind::Executable("a fragment")
    );
    assert!(matches!(refused("").0, GraphQlErrorKind::Syntax(_)));
    assert!(matches!(
        refused(&format!("{Q}type A {{ a: Int! ! }}")).0,
        GraphQlErrorKind::Syntax(_)
    ));
    assert!(matches!(
        refused(&format!("{Q}type A {{ a(x: Int = 0x1): Int }}")).0,
        GraphQlErrorKind::Syntax(_)
    ));
    let (kind, _, offset) = refused("type Query { q: Int }\n}");
    assert!(matches!(kind, GraphQlErrorKind::Syntax(_)));
    assert_eq!(offset, 22);
    assert_eq!(
        read_graphql(b"type Query { q: \xff }").unwrap_err().kind,
        GraphQlErrorKind::Encoding
    );
}

#[test]
fn deep_nesting_is_refused_not_fatal() {
    let deep = format!(
        "{Q}type A {{ a: {}Int{} }}",
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert!(read_graphql(deep.as_bytes()).is_err());
    let value = format!(
        "{Q}type A {{ a(x: Int = {}1{}): Int }}",
        "[".repeat(300),
        "]".repeat(300)
    );
    assert_eq!(refused(&value).0, GraphQlErrorKind::Nesting);
}

#[test]
fn strings_are_written_in_a_form_the_grammar_reads_back() {
    for value in [
        "",
        "plain",
        "   ",
        "a\"b",
        "a\\b",
        "line\n  indented",
        "  lead\nnext",
        "  lead\"",
        "ends\"",
        "\"starts",
    ] {
        let text = string_text(value).unwrap_or_else(|| panic!("{value:?}"));
        let source = format!("{Q}type A {{ a(x: String = {text}): Int }}");
        let schema = read_graphql(source.as_bytes())
            .unwrap_or_else(|e| panic!("{value:?} as {text}: {e}"))
            .schema;
        let default = kept(&schema, "A.a(x:)", "default");
        assert_eq!(default, [text.as_str()], "{value:?}");
    }
    assert_eq!(string_text("a\"\"\"b"), None);
}
