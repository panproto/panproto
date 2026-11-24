//! Complete finite scopes for the `OpenAPI` page.
//!
//! The document scope is the cartesian product of four choices made on one
//! base document: its `openapi` version ([`versions`]), the parameter of its
//! one operation ([`parameters`]), that operation's responses
//! ([`responses`]), and one member added to the document ([`additions`]).

use crate::pages::json_schema::{Doc, m, s};

/// The versions the scope writes in `openapi`; the first two are read.
#[must_use]
pub fn versions() -> Vec<Doc> {
    vec![s("3.1.0"), s("3.1.2"), s("3.0.3"), s("3.2.0")]
}

/// The parameters the operation may hold; the first four are read.
#[must_use]
pub fn parameters() -> Vec<Doc> {
    let path = |extra: &[(&str, Doc)]| {
        let mut members = vec![
            ("name", s("id")),
            ("in", s("path")),
            ("required", Doc::Bool(true)),
        ];
        members.extend(extra.iter().cloned());
        m(&members)
    };
    vec![
        path(&[("schema", m(&[("type", s("string"))]))]),
        m(&[
            ("$ref", s("#/components/parameters/id")),
            ("description", s("d")),
            ("x-kept", Doc::Bool(true)),
        ]),
        path(&[("schema", m(&[("$ref", s("#/components/schemas/S"))]))]),
        path(&[
            (
                "content",
                m(&[(
                    "text/plain",
                    m(&[
                        ("schema", m(&[("type", s("string"))])),
                        ("examples", m(&[("e", m(&[("value", Doc::Int(1))]))])),
                    ]),
                )]),
            ),
            ("x-p", Doc::Seq(vec![Doc::Null])),
        ]),
        m(&[("name", s("id")), ("in", s("path")), ("schema", m(&[]))]),
        m(&[("$ref", s("#/components/parameters/none"))]),
        m(&[("$ref", s("#/components/responses/ok"))]),
        m(&[
            ("name", s("q")),
            ("in", s("query")),
            ("style", s("matrix")),
            ("schema", m(&[])),
        ]),
        m(&[
            ("name", s("q")),
            ("in", s("query")),
            (
                "schema",
                m(&[("type", s("integer")), ("minimum", Doc::Int(1))]),
            ),
        ]),
        path(&[
            ("schema", m(&[])),
            ("content", m(&[("text/plain", m(&[]))])),
        ]),
        m(&[
            ("name", s("x")),
            ("in", s("path")),
            ("required", Doc::Bool(true)),
            ("schema", m(&[])),
        ]),
        m(&[
            ("name", s("h")),
            ("in", s("header")),
            ("allowReserved", Doc::Bool(true)),
            ("schema", m(&[])),
        ]),
        path(&[("schema", m(&[("$ref", s("#/components/schemas/T"))]))]),
    ]
}

/// The responses the operation may hold; the first three are read.
#[must_use]
pub fn responses() -> Vec<Doc> {
    let described = |extra: &[(&str, Doc)]| {
        let mut members = vec![("description", s("ok"))];
        members.extend(extra.iter().cloned());
        m(&members)
    };
    vec![
        m(&[("200", described(&[]))]),
        m(&[("default", m(&[("$ref", s("#/components/responses/ok"))]))]),
        m(&[(
            "2XX",
            described(&[("links", m(&[("self", m(&[("operationId", s("getP"))]))]))]),
        )]),
        m(&[]),
        m(&[("600", described(&[]))]),
        m(&[("200", m(&[]))]),
        m(&[(
            "200",
            described(&[("links", m(&[("l", m(&[("operationId", s("none"))]))]))]),
        )]),
    ]
}

/// The members the document may gain, each a name and a value; the first
/// four are read, and the first is no member at all.
#[must_use]
pub fn additions() -> Vec<Option<(String, Doc)>> {
    let member = |name: &str, value: Doc| Some((name.to_owned(), value));
    vec![
        None,
        member("security", Doc::Seq(vec![m(&[("key", Doc::Seq(vec![]))])])),
        member("x-a", m(&[("b", Doc::Seq(vec![Doc::Int(1), Doc::Null]))])),
        member(
            "webhooks",
            m(&[("w", m(&[("$ref", s("#/paths/~1p~1{id}"))]))]),
        ),
        member("security", Doc::Seq(vec![m(&[("none", Doc::Seq(vec![]))])])),
        member(
            "tags",
            Doc::Seq(vec![m(&[("name", s("t"))]), m(&[("name", s("t"))])]),
        ),
        member(
            "jsonSchemaDialect",
            s("http://json-schema.org/draft-07/schema#"),
        ),
        member("swagger", s("2.0")),
        member("basePath", s("/v1")),
        member(
            "servers",
            Doc::Seq(vec![m(&[
                ("url", s("https://{r}.e.test")),
                (
                    "variables",
                    m(&[(
                        "r",
                        m(&[("enum", Doc::Seq(vec![s("a")])), ("default", s("b"))]),
                    )]),
                ),
            ])]),
        ),
    ]
}

/// The base document with the given version, parameter, responses and
/// added member.
#[must_use]
pub fn document(
    version: &Doc,
    parameter: &Doc,
    responses: &Doc,
    addition: Option<&(String, Doc)>,
) -> Doc {
    let operation = m(&[
        ("operationId", s("getP")),
        ("parameters", Doc::Seq(vec![parameter.clone()])),
        ("responses", responses.clone()),
    ]);
    let components = m(&[
        (
            "schemas",
            m(&[(
                "S",
                m(&[
                    ("type", s("object")),
                    ("properties", m(&[("n/m", m(&[("type", s("string"))]))])),
                    ("discriminator", m(&[("propertyName", s("n/m"))])),
                ]),
            )]),
        ),
        (
            "parameters",
            m(&[(
                "id",
                m(&[
                    ("name", s("id")),
                    ("in", s("path")),
                    ("required", Doc::Bool(true)),
                    ("schema", m(&[("type", s("string"))])),
                ]),
            )]),
        ),
        ("responses", m(&[("ok", m(&[("description", s("ok"))]))])),
        (
            "securitySchemes",
            m(&[(
                "key",
                m(&[("type", s("apiKey")), ("name", s("k")), ("in", s("header"))]),
            )]),
        ),
    ]);
    let mut members = vec![
        ("openapi".to_owned(), version.clone()),
        (
            "info".to_owned(),
            m(&[("title", s("T")), ("version", s("1"))]),
        ),
        (
            "paths".to_owned(),
            m(&[("/p/{id}", m(&[("get", operation)]))]),
        ),
        ("components".to_owned(), components),
    ];
    if let Some((name, value)) = addition {
        members.push((name.clone(), value.clone()));
    }
    Doc::Map(members)
}

/// Every document of the scope, in the order version, parameter,
/// responses, addition.
#[must_use]
pub fn documents() -> Vec<Doc> {
    let mut out = Vec::new();
    for version in &versions() {
        for parameter in &parameters() {
            for response in &responses() {
                for addition in &additions() {
                    out.push(document(version, parameter, response, addition.as_ref()));
                }
            }
        }
    }
    out
}

/// The number of documents of the scope, the product of the four choices'
/// sizes: $4 \cdot 13 \cdot 7 \cdot 10 = 3640$.
#[must_use]
pub const fn document_count() -> usize {
    4 * 13 * 7 * 10
}

/// The number of documents the page reads: those whose four choices are
/// each read, $2 \cdot 4 \cdot 3 \cdot 4 = 96$.
#[must_use]
pub const fn accepted_count() -> usize {
    2 * 4 * 3 * 4
}
