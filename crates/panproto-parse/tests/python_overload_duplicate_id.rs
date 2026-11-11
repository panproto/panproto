//! Regression: parsing a Python file with `@typing.overload`'s
//! repeated `def foo(...)` shape does not raise
//! `SchemaError::DuplicateVertex`.
//!
//! Python's `tags.scm` tags every `function_definition` as
//! `@definition.function`, so a vertex ID of the bare form
//! `{scope}::{name}` would make three same-named `def foo`
//! declarations collide on `repro.py::foo`, and
//! `SchemaBuilder::vertex` would reject the second and third with
//! `duplicate vertex id: repro.py::foo`. Any Python source using
//! `@typing.overload` would be unparseable.
//!
//! Disambiguation lives in `IdGenerator`: the parent frame records
//! per-name occurrences and suffixes repeats `#1`, `#2`, …. The
//! disambiguated leaf is also propagated to the scope stack, so
//! descendants of the second `foo` are prefixed `foo#1::…`, never
//! re-colliding with descendants of the first.

#![cfg(all(feature = "grammars", feature = "lang-python"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const PYTHON_OVERLOAD: &[u8] = b"\
from typing import overload

@overload
def foo(x: int) -> int: ...

@overload
def foo(x: str) -> str: ...

def foo(x):
    return x
";

#[test]
fn python_overload_does_not_collide_on_vertex_id() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("python", PYTHON_OVERLOAD, "repro.py")
        .expect("python parser must accept @overload decorations");

    // Collect all vertex IDs whose kind is `function_definition`.
    let foo_ids: Vec<String> = schema
        .vertices
        .iter()
        .filter(|(_, v)| v.kind.as_ref() == "function_definition")
        .map(|(id, _)| id.to_string())
        .collect();

    assert_eq!(
        foo_ids.len(),
        3,
        "expected 3 function_definition vertices (the three `def foo`s); got {foo_ids:?}"
    );

    // Their IDs must all be distinct.
    let mut sorted = foo_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        foo_ids.len(),
        "function_definition IDs collided: {foo_ids:?}"
    );

    // The first should be the bare name; subsequent ones get `#N`.
    let mut leaves: Vec<String> = sorted
        .iter()
        .map(|id| id.rsplit("::").next().unwrap_or("").to_owned())
        .collect();
    leaves.sort();
    assert_eq!(
        leaves,
        vec!["foo", "foo#1", "foo#2"],
        "id leaves: {leaves:?}"
    );
}

const PYTHON_OVERLOAD_WITH_BODIES: &[u8] = b"\
from typing import overload

@overload
def foo(x):
    return 1

def foo(x):
    return 2
";

/// Descendants of the disambiguated scope must use the disambiguated
/// prefix. Two `return` statements (one per overload) must produce
/// two distinct vertices.
#[test]
fn python_overload_children_use_disambiguated_prefix() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("python", PYTHON_OVERLOAD_WITH_BODIES, "bodies.py")
        .expect("parse");

    let return_count = schema
        .vertices
        .values()
        .filter(|v| v.kind.as_ref() == "return_statement")
        .count();
    assert_eq!(
        return_count, 2,
        "expected 2 return_statement vertices, one per overload body"
    );
}
