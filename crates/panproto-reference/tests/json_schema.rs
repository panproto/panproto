//! The json-schema reference on the page's own examples and scope.

use panproto_reference::enumerate::json_schema::{
    base, document_count, documents, envelopes, schema_nodes,
};
use panproto_reference::pages::json_schema::{
    Doc, Kind, Reading, Refusal, canonical_text, m, read, resolve, s,
};

/// The reading of a bundle the test requires to read.
fn reading(bundle: &[(Option<String>, Doc)]) -> Reading {
    read(bundle).unwrap_or_else(|refusal| panic!("{refusal:?}"))
}

/// The refusal of a bundle the test requires to be refused.
fn refusal(bundle: &[(Option<String>, Doc)]) -> Refusal {
    match read(bundle) {
        Ok(reading) => panic!("{reading:?}"),
        Err(refusal) => refusal,
    }
}

#[test]
fn the_json_schema_bundle_scope_has_its_closed_form_count() {
    assert_eq!(schema_nodes().len(), 24);
    assert_eq!(envelopes().len(), 22);
    assert_eq!(document_count(24, 22), 1223);
    assert_eq!(documents().len(), document_count(24, 22));
}

#[test]
fn a_document_reads_as_its_subschema_vertices_edges_and_constraints() {
    let doc = base(vec![(
        "a".into(),
        m(&[
            ("type", s("object")),
            ("properties", m(&[("k", m(&[("type", s("string"))]))])),
            ("required", Doc::Seq(vec![s("k"), s("u")])),
            ("x-note", Doc::Int(1)),
        ]),
    )]);
    let got = reading(&[(None, doc)]);
    let r = "https://e.test/r#";
    let kinds: Vec<(&str, &str)> = got
        .vertices
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("https://e.test/r#", "any"),
            ("https://e.test/r#/$defs/d", "integer"),
            ("https://e.test/r#/properties/a", "object"),
            ("https://e.test/r#/properties/a/properties/k", "string"),
        ]
    );
    assert_eq!(
        got.edges[r],
        [
            (
                "def".to_owned(),
                format!("{r}/$defs/d"),
                Some("d".to_owned())
            ),
            (
                "prop".to_owned(),
                format!("{r}/properties/a"),
                Some("a".to_owned())
            ),
        ]
    );
    let a = format!("{r}/properties/a");
    assert_eq!(
        got.constraints[&a],
        [
            ("type".to_owned(), "\"object\"".to_owned()),
            ("required".to_owned(), "[\"k\",\"u\"]".to_owned()),
            ("annotation".to_owned(), "{\"x-note\":1}".to_owned()),
        ]
    );
    assert_eq!(got.required[&a].iter().collect::<Vec<_>>(), ["k"]);
    assert_eq!(got.entries, [format!("{r}/$defs/d"), r.to_owned()]);
    assert_eq!(got.roots, [r.to_owned()]);
}

#[test]
fn json_schema_refusals_name_the_place_and_the_rule() {
    let one = |node: Doc| refusal(&[(None, base(vec![("a".into(), node)]))]);
    let cases = [
        (
            m(&[("type", s("text"))]),
            "/properties/a/type",
            Kind::Invalid,
        ),
        (
            m(&[("minLength", Doc::Int(-1))]),
            "/properties/a/minLength",
            Kind::WrongType("a non-negative integer"),
        ),
        (
            m(&[("definitions", m(&[]))]),
            "/properties/a/definitions",
            Kind::Superseded("definitions"),
        ),
        (
            m(&[("$anchor", s("A"))]),
            "/properties/a/$anchor",
            Kind::DuplicateAnchor,
        ),
        (
            m(&[("items", Doc::Seq(vec![m(&[])]))]),
            "/properties/a/items",
            Kind::WrongType("a schema"),
        ),
        (
            m(&[("$ref", s("https://other.test/s"))]),
            "/properties/a/$ref",
            Kind::RemoteRef,
        ),
        (
            m(&[("$ref", s("#/properties/b"))]),
            "/properties/a/$ref",
            Kind::RefTarget,
        ),
    ];
    for (node, path, kind) in cases {
        let got = one(node);
        assert_eq!((got.document, got.path.as_str(), got.kind), (0, path, kind));
    }
    let draft7 = m(&[("$schema", s("http://json-schema.org/draft-07/schema#"))]);
    assert_eq!(
        refusal(&[(None, draft7)]),
        Refusal {
            document: 0,
            path: "/$schema".into(),
            kind: Kind::UnsupportedDialect,
        }
    );
    assert_eq!(
        refusal(&[(None, m(&[])), (None, m(&[]))]),
        Refusal {
            document: 1,
            path: String::new(),
            kind: Kind::DuplicateDocument,
        }
    );
}

#[test]
fn references_resolve_through_ids_anchors_pointers_and_rfc_3986() {
    // RFC 3986 Section 5.4.1, against the base http://a/b/c/d;p?q.
    let b = Some("http://a/b/c/d;p?q");
    for (reference, expected) in [
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("", "http://a/b/c/d;p?q"),
        ("..", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("g;x=1/../y", "http://a/b/c/y"),
    ] {
        assert_eq!(
            resolve(b, reference).as_deref(),
            Some(expected),
            "{reference}"
        );
    }
    assert_eq!(resolve(None, "g"), None);

    let node = m(&[("$id", s("n")), ("$ref", s("r#A"))]);
    let got = reading(&[(None, base(vec![("a".into(), node)]))]);
    let a = "https://e.test/r#/properties/a";
    assert_eq!(
        got.edges[a],
        [(
            "ref".to_owned(),
            "https://e.test/r#/$defs/d".to_owned(),
            Some("r#A".to_owned())
        )]
    );

    let dynamic = |extra: Vec<(String, Doc)>| {
        let mut props = vec![(
            "a".to_owned(),
            m(&[("$dynamicAnchor", s("D")), ("$dynamicRef", s("#D"))]),
        )];
        props.extend(extra);
        read(&[(None, base(props))])
    };
    let alone = dynamic(Vec::new()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(alone.edges[a][0].0, "dynamic-ref");
    let scoped = dynamic(vec![(
        "b".into(),
        m(&[("$id", s("m")), ("$dynamicAnchor", s("D"))]),
    )]);
    assert_eq!(
        scoped.map(|_| ()).map_err(|e| (e.path, e.kind)),
        Err(("/properties/a/$dynamicRef".to_owned(), Kind::DynamicScope))
    );
}

#[test]
fn the_canonical_document_orders_keywords_as_the_page_states() {
    let doc = m(&[
        ("items", m(&[("type", s("string"))])),
        ("prefixItems", Doc::Seq(vec![Doc::Bool(true)])),
        ("$ref", s("#/$defs/d")),
        ("minItems", Doc::Int(1)),
        ("properties", m(&[])),
        ("x-note", m(&[("k", Doc::Null)])),
        ("$defs", m(&[("d", Doc::Bool(false))])),
    ]);
    assert_eq!(
        canonical_text(&doc),
        "{\"minItems\":1,\"properties\":{},\"x-note\":{\"k\":null},\"$ref\":\"#/$defs/d\",\
         \"$defs\":{\"d\":false},\"prefixItems\":[true],\"items\":{\"type\":\"string\"}}\n"
    );
    assert_eq!(canonical_text(&Doc::Bool(true)), "true\n");
}
