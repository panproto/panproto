//! Complete finite scopes for the kubernetes-crds page.
//!
//! The document scope is a base CRD whose root schema holds zero, one or
//! two of the properties `a` and `b` (in either order), each one of the
//! [`schema_nodes`], together with the [`envelopes`]: the base CRD with one
//! property `a` of type string, changed in one field outside the schema.

use crate::pages::kubernetes_crds::{Doc, m, s};

/// The schema nodes a property of the scope may be.
#[must_use]
pub fn schema_nodes() -> Vec<Doc> {
    let string = || m(&[("type", s("string"))]);
    let keyed_items = |key: &str| {
        m(&[
            ("type", s("array")),
            (
                "items",
                m(&[
                    ("type", s("object")),
                    (
                        "properties",
                        m(&[("k", string()), ("v", m(&[("type", s("integer"))]))]),
                    ),
                    ("required", Doc::Seq(vec![s("k")])),
                ]),
            ),
            ("x-kubernetes-list-type", s("map")),
            ("x-kubernetes-list-map-keys", Doc::Seq(vec![s(key)])),
        ])
    };
    vec![
        string(),
        m(&[
            ("type", s("integer")),
            ("minimum", Doc::Int(1)),
            ("maximum", Doc::Int(10)),
        ]),
        m(&[
            ("type", s("string")),
            ("enum", Doc::Seq(vec![s("x"), s("y")])),
            ("default", s("x")),
        ]),
        m(&[
            ("type", s("array")),
            ("items", string()),
            ("x-kubernetes-list-type", s("set")),
        ]),
        keyed_items("k"),
        keyed_items("z"),
        m(&[("type", s("object")), ("additionalProperties", string())]),
        m(&[
            ("type", s("object")),
            ("properties", m(&[("x", m(&[("type", s("boolean"))]))])),
            ("required", Doc::Seq(vec![s("x"), s("y")])),
        ]),
        m(&[
            ("x-kubernetes-int-or-string", Doc::Bool(true)),
            ("pattern", s("^a")),
        ]),
        m(&[
            ("type", s("object")),
            ("x-kubernetes-preserve-unknown-fields", Doc::Bool(true)),
        ]),
        m(&[("description", s("d"))]),
        m(&[("type", s("string")), ("$ref", s("#/x"))]),
        m(&[("type", s("string")), ("foo", Doc::Int(1))]),
        m(&[("type", s("array"))]),
        m(&[
            ("type", s("object")),
            ("properties", m(&[("p", string())])),
            ("additionalProperties", string()),
        ]),
        m(&[("type", s("integer")), ("maxLength", Doc::Int(-1))]),
        m(&[("type", s("string")), ("uniqueItems", Doc::Bool(true))]),
        m(&[
            ("type", s("integer")),
            ("x-kubernetes-int-or-string", Doc::Bool(true)),
        ]),
    ]
}

/// One version entry.
fn version(name: &str, storage: bool, root: Doc) -> Doc {
    m(&[
        ("name", s(name)),
        ("served", Doc::Bool(true)),
        ("storage", Doc::Bool(storage)),
        ("schema", m(&[("openAPIV3Schema", root)])),
    ])
}

/// The root schema holding `properties`.
fn root(properties: Vec<(String, Doc)>) -> Doc {
    Doc::Map(vec![
        ("type".into(), s("object")),
        ("properties".into(), Doc::Map(properties)),
    ])
}

/// The base CRD with the given versions, spec extras and top-level extras.
fn crd(versions: Vec<Doc>, spec_extra: &[(&str, Doc)], top_extra: &[(&str, Doc)]) -> Doc {
    let mut spec = vec![
        ("group".to_owned(), s("test.com")),
        (
            "names".to_owned(),
            m(&[("plural", s("examples")), ("kind", s("Example"))]),
        ),
        ("scope".to_owned(), s("Namespaced")),
        ("versions".to_owned(), Doc::Seq(versions)),
    ];
    spec.extend(spec_extra.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    let mut top = vec![
        ("apiVersion".to_owned(), s("apiextensions.k8s.io/v1")),
        ("kind".to_owned(), s("CustomResourceDefinition")),
        (
            "metadata".to_owned(),
            m(&[("name", s("examples.test.com"))]),
        ),
        ("spec".to_owned(), Doc::Map(spec)),
    ];
    top.extend(top_extra.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    Doc::Map(top)
}

/// Replace the field `key` of mapping `doc` (appending it when absent), or
/// remove it when `value` is `None`.
fn set(doc: &Doc, key: &str, value: Option<Doc>) -> Doc {
    let Doc::Map(entries) = doc else {
        return doc.clone();
    };
    let mut out: Vec<(String, Doc)> = Vec::new();
    let mut done = false;
    for (k, v) in entries {
        if k == key {
            done = true;
            if let Some(new) = &value {
                out.push((k.clone(), new.clone()));
            }
        } else {
            out.push((k.clone(), v.clone()));
        }
    }
    if !done && let Some(new) = value {
        out.push((key.to_owned(), new));
    }
    Doc::Map(out)
}

/// The field `key` of mapping `doc`.
fn get(doc: &Doc, key: &str) -> Doc {
    match doc {
        Doc::Map(e) => e
            .iter()
            .find(|(k, _)| k == key)
            .map_or(Doc::Null, |(_, v)| v.clone()),
        _ => Doc::Null,
    }
}

/// The envelope documents: the base with one string property `a`, each
/// changed in one field outside the schema.
#[must_use]
pub fn envelopes() -> Vec<Doc> {
    let a = || root(vec![("a".into(), m(&[("type", s("string"))]))]);
    let base = crd(vec![version("v1", true, a())], &[], &[]);
    let spec = get(&base, "spec");
    let with_spec =
        |key: &str, value: Option<Doc>| set(&base, "spec", Some(set(&spec, key, value)));
    let names = get(&spec, "names");
    vec![
        set(&base, "apiVersion", Some(s("apiextensions.k8s.io/v1beta1"))),
        set(&base, "kind", Some(s("Foo"))),
        set(&base, "metadata", None),
        set(&base, "metadata", Some(m(&[("name", s("other.test.com"))]))),
        set(
            &base,
            "metadata",
            Some(m(&[
                ("name", s("examples.test.com")),
                ("labels", m(&[("app", s("x"))])),
            ])),
        ),
        set(&base, "extra", Some(Doc::Int(1))),
        with_spec("scope", Some(s("Global"))),
        with_spec("group", Some(s("nodots"))),
        with_spec("versions", Some(Doc::Seq(vec![]))),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![
                version("v1", true, a()),
                version("v1", false, a()),
            ])),
        ),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![
                version("v1", true, a()),
                version("v2", true, a()),
            ])),
        ),
        with_spec("versions", Some(Doc::Seq(vec![version("v1", false, a())]))),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![set(
                &version("v1", true, a()),
                "served",
                Some(s("yes")),
            )])),
        ),
        with_spec("conversion", Some(m(&[("strategy", s("Webhook"))]))),
        with_spec("conversion", Some(m(&[("strategy", s("None"))]))),
        with_spec("preserveUnknownFields", Some(Doc::Bool(true))),
        with_spec("versions", Some(Doc::Seq(vec![version("V1", true, a())]))),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![version(
                "v1",
                true,
                m(&[("type", s("string"))]),
            )])),
        ),
        set(
            &base,
            "status",
            Some(m(&[("acceptedNames", m(&[("kind", s(""))]))])),
        ),
        with_spec(
            "names",
            Some(m(&[
                ("plural", s("examples")),
                ("singular", s("example")),
                ("kind", s("Example")),
                ("listKind", s("ExampleList")),
                ("shortNames", Doc::Seq(vec![s("ex")])),
                ("categories", Doc::Seq(vec![s("all")])),
            ])),
        ),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![
                version("v1", true, a()),
                set(
                    &set(
                        &set(&version("v2", false, a()), "served", Some(Doc::Bool(false))),
                        "deprecated",
                        Some(Doc::Bool(true)),
                    ),
                    "deprecationWarning",
                    Some(s("use v1")),
                ),
            ])),
        ),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![set(
                &set(
                    &version("v1", true, a()),
                    "subresources",
                    Some(m(&[("status", m(&[]))])),
                ),
                "additionalPrinterColumns",
                Some(Doc::Seq(vec![m(&[
                    ("name", s("A")),
                    ("type", s("string")),
                    ("jsonPath", s(".spec.a")),
                ])])),
            )])),
        ),
        with_spec("names", Some(set(&names, "kind", None))),
        with_spec(
            "versions",
            Some(Doc::Seq(vec![set(
                &version("v1", true, a()),
                "schema",
                None,
            )])),
        ),
    ]
}

/// Every document of the scope: the base CRD with each list of at most two
/// distinct properties from `a` and `b` over [`schema_nodes`], then the
/// [`envelopes`].
#[must_use]
pub fn documents() -> Vec<Doc> {
    let nodes = schema_nodes();
    let with = |props: Vec<(String, Doc)>| crd(vec![version("v1", true, root(props))], &[], &[]);
    let mut out = vec![with(vec![])];
    for first in ["a", "b"] {
        for x in &nodes {
            out.push(with(vec![(first.into(), x.clone())]));
        }
    }
    for (first, second) in [("a", "b"), ("b", "a")] {
        for x in &nodes {
            for y in &nodes {
                out.push(with(vec![
                    (first.into(), x.clone()),
                    (second.into(), y.clone()),
                ]));
            }
        }
    }
    out.extend(envelopes());
    out
}

/// The size of [`documents`] by its closed form: with `n` schema nodes and
/// `e` envelopes, `1 + 2n + 2n^2 + e`.
#[must_use]
pub const fn document_count(n: usize, e: usize) -> usize {
    1 + 2 * n + 2 * n * n + e
}
