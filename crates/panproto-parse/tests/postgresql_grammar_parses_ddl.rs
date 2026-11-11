//! The vendored PostgreSQL grammar loads, claims the `.pgsql` extension and
//! parses table DDL into the node types `gram.y` names.

#![cfg(feature = "lang-postgresql")]

use panproto_parse::ParserRegistry;

const DDL: &[u8] = b"CREATE TABLE film (
    film_id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    title text NOT NULL COLLATE \"C\",
    language_id smallint REFERENCES language (language_id) ON DELETE RESTRICT,
    CONSTRAINT film_title_key UNIQUE (title) DEFERRABLE INITIALLY DEFERRED
);
CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1 $body$ LANGUAGE sql;
";

#[test]
fn postgresql_grammar_parses_table_ddl_without_error_nodes() {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol("postgresql", DDL, "schema.pgsql")
        .unwrap_or_else(|e| panic!("the PostgreSQL grammar parses DDL: {e}"));
    let kinds: std::collections::BTreeSet<&str> =
        schema.vertices.values().map(|v| v.kind.as_ref()).collect();
    assert!(
        !kinds.iter().any(|k| k.contains("ERROR") || *k == "MISSING"),
        "error nodes in the parse: {kinds:?}"
    );
    for expected in [
        "CreateStmt",
        "columnDef",
        "ColQualList",
        "TableConstraint",
        "ConstraintElem",
        "CreateFunctionStmt",
    ] {
        assert!(
            kinds.contains(expected),
            "{expected} missing from {kinds:?}"
        );
    }
}

#[test]
fn pgsql_extension_selects_the_postgresql_grammar() {
    assert_eq!(
        panproto_grammars::extension_to_language("pgsql"),
        Some("postgresql")
    );
}

#[test]
fn postgresql_grammar_json_loads_for_emit() {
    let grammar = panproto_grammars::grammars()
        .into_iter()
        .find(|g| g.name == "postgresql")
        .unwrap_or_else(|| panic!("the postgresql grammar is compiled in"));
    let bytes = grammar
        .grammar_json
        .unwrap_or_else(|| panic!("the postgresql grammar ships grammar.json"));
    let loaded = panproto_parse::emit_pretty::Grammar::from_bytes("postgresql", bytes)
        .unwrap_or_else(|e| panic!("grammar.json loads: {e}"));
    assert!(loaded.rules.contains_key("CreateStmt"));
}
