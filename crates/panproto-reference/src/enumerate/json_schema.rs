//! Complete finite scopes for the json-schema page.
//!
//! The bundle scope is a base document, identified as `https://e.test/r`,
//! whose `$defs` holds `d` (anchored `A`) and whose `properties` holds zero,
//! one or two of the properties `a` and `b` (in either order), each one of
//! the [`schema_nodes`], together with the [`envelopes`]: bundles that
//! change the base in one respect outside its properties, or add a second
//! document.

use crate::pages::json_schema::{Doc, m, s};

/// A bundle: each document with its retrieval URI.
pub type Bundle = Vec<(Option<String>, Doc)>;

/// The schema nodes a property of the scope may be.
#[must_use]
pub fn schema_nodes() -> Vec<Doc> {
    let ty = |name: &str| m(&[("type", s(name))]);
    vec![
        m(&[("type", s("string")), ("minLength", Doc::Int(1))]),
        m(&[("type", Doc::Seq(vec![s("integer"), s("null")]))]),
        Doc::Bool(true),
        Doc::Bool(false),
        m(&[]),
        m(&[
            ("enum", Doc::Seq(vec![s("x"), Doc::Int(1), Doc::Null])),
            ("const", s("x")),
            ("title", s("t")),
        ]),
        m(&[
            ("items", ty("string")),
            ("prefixItems", Doc::Seq(vec![ty("integer")])),
            ("minContains", Doc::Int(1)),
            ("contains", m(&[("const", Doc::Int(1))])),
        ]),
        m(&[
            ("properties", m(&[("k", ty("string"))])),
            ("required", Doc::Seq(vec![s("k"), s("u")])),
            ("additionalProperties", Doc::Bool(false)),
        ]),
        m(&[("$ref", s("#/$defs/d"))]),
        m(&[("$ref", s("#A"))]),
        m(&[("$ref", s("#/properties/a"))]),
        m(&[("$id", s("n")), ("$ref", s("r#A"))]),
        m(&[
            ("allOf", Doc::Seq(vec![m(&[("minimum", Doc::Int(0))])])),
            ("not", ty("null")),
            ("if", ty("string")),
            ("then", m(&[("maxLength", Doc::Int(3))])),
        ]),
        m(&[
            ("x-note", m(&[("$id", s("ignored"))])),
            ("$comment", s("c")),
            ("patternProperties", m(&[])),
        ]),
        m(&[("$ref", s("https://other.test/s"))]),
        m(&[("$dynamicAnchor", s("D")), ("$dynamicRef", s("#D"))]),
        m(&[("$id", s("m")), ("$dynamicAnchor", s("D"))]),
        ty("text"),
        m(&[("minLength", Doc::Int(-1))]),
        m(&[("definitions", m(&[]))]),
        m(&[("$anchor", s("A"))]),
        m(&[("$id", s("#frag"))]),
        m(&[("items", Doc::Seq(vec![m(&[])]))]),
        m(&[("$schema", s("https://json-schema.org/draft/2020-12/schema"))]),
    ]
}

/// The base document's `$defs`.
fn defs() -> (&'static str, Doc) {
    (
        "$defs",
        m(&[("d", m(&[("$anchor", s("A")), ("type", s("integer"))]))]),
    )
}

/// The base document with `properties` holding `props`.
#[must_use]
pub fn base(props: Vec<(String, Doc)>) -> Doc {
    m(&[
        ("$id", s("https://e.test/r")),
        defs(),
        ("properties", Doc::Map(props)),
    ])
}

/// A bundle of one document with no retrieval URI.
fn one(doc: Doc) -> Bundle {
    vec![(None, doc)]
}

/// The envelopes: whole bundles outside the property scope.
#[must_use]
pub fn envelopes() -> Vec<Bundle> {
    let other = m(&[("$defs", m(&[("e", m(&[("type", s("string"))]))]))]);
    let with = |extra: (&str, Doc)| one(m(&[("$id", s("https://e.test/r")), defs(), extra]));
    let mut out = vec![
        one(m(&[
            defs(),
            ("properties", m(&[("a", m(&[("$ref", s("#A"))]))])),
        ])),
        with(("$schema", s("https://json-schema.org/draft/2020-12/schema"))),
        with(("$schema", s("http://json-schema.org/draft-07/schema#"))),
        one(m(&[("$id", s("r")), defs()])),
        one(Doc::Bool(true)),
        one(Doc::Int(1)),
        vec![
            (
                None,
                base(vec![("a".into(), m(&[("$ref", s("o#/$defs/e"))]))]),
            ),
            (Some("https://e.test/o".into()), other.clone()),
        ],
        vec![(None, m(&[])), (None, other.clone())],
        vec![(None, m(&[])), (Some("o".into()), other)],
        one(m(&[("$ref", s("other"))])),
        one(m(&[("$id", s("https://e.test/r#")), defs()])),
        with((
            "$vocabulary",
            m(&[(
                "https://json-schema.org/draft/2020-12/vocab/core",
                Doc::Bool(true),
            )]),
        )),
        with(("required", Doc::Seq(vec![s("a"), s("a")]))),
    ];
    out.extend(envelopes_continued());
    out
}

/// The envelopes that change the base by one keyword of the root schema.
fn envelopes_continued() -> Vec<Bundle> {
    let with = |extra: Vec<(&str, Doc)>| {
        let mut members = vec![("$id", s("https://e.test/r")), defs()];
        members.extend(extra);
        vec![(None, m(&members))]
    };
    vec![
        with(vec![
            ("dependentRequired", m(&[("a", Doc::Seq(vec![s("b")]))])),
            (
                "dependentSchemas",
                m(&[("a", m(&[("maxProperties", Doc::Int(2))]))]),
            ),
        ]),
        with(vec![
            (
                "patternProperties",
                m(&[("^x", m(&[("type", s("string"))]))]),
            ),
            ("propertyNames", m(&[("pattern", s("^[a-z]"))])),
            ("unevaluatedProperties", Doc::Bool(false)),
        ]),
        with(vec![("anyOf", Doc::Seq(vec![]))]),
        with(vec![("$ref", s("#/$defs/d/type"))]),
        with(vec![("$ref", s("#/$defs/z"))]),
        with(vec![("$ref", s("#B"))]),
        with(vec![("multipleOf", Doc::Int(0))]),
        with(vec![("$ref", s("#/%24defs/d"))]),
        with(vec![(
            "oneOf",
            Doc::Seq(vec![m(&[("type", s("string"))]), m(&[("$ref", s("#"))])]),
        )]),
    ]
}

/// The number of bundles in the scope with `nodes` schema nodes and
/// `envelopes` envelopes: the base with no property, with `a` or `b` alone,
/// with both in either order, and the envelopes.
#[must_use]
pub const fn document_count(nodes: usize, envelopes: usize) -> usize {
    1 + 2 * nodes + 2 * nodes * nodes + envelopes
}

/// Every bundle of the scope, in a fixed order.
#[must_use]
pub fn documents() -> Vec<Bundle> {
    let nodes = schema_nodes();
    let mut out = vec![one(base(Vec::new()))];
    for name in ["a", "b"] {
        for node in &nodes {
            out.push(one(base(vec![(name.into(), node.clone())])));
        }
    }
    for (first, second) in [("a", "b"), ("b", "a")] {
        for x in &nodes {
            for y in &nodes {
                out.push(one(base(vec![
                    (first.into(), x.clone()),
                    (second.into(), y.clone()),
                ])));
            }
        }
    }
    out.extend(envelopes());
    out
}
