//! The asyncapi reference on the page's own examples and scope.

use panproto_reference::enumerate::asyncapi::{
    accepted_count, additions, channels, document, document_count, documents, messages, operations,
    versions,
};
use panproto_reference::pages::asyncapi::{Refusal, canonical_text, read};
use panproto_reference::pages::json_schema::{Doc, m, s};

fn base() -> Doc {
    document(
        &versions()[0],
        &channels()[0],
        &operations()[0],
        &messages()[0],
        None,
    )
}

fn refusal(doc: Doc) -> Refusal {
    match read(&[(None, doc, false)]) {
        Ok(reading) => panic!("{reading:?}"),
        Err(refusal) => refusal,
    }
}

#[test]
fn the_asyncapi_document_scope_has_its_closed_form_count() {
    assert_eq!(
        (
            versions().len(),
            channels().len(),
            operations().len(),
            messages().len(),
            additions().len()
        ),
        (4, 6, 6, 7, 5)
    );
    let all = documents();
    assert_eq!(all.len(), document_count());
    assert_eq!(document_count(), 5040);
    let read_count = all
        .iter()
        .filter(|d| read(&[(None, (*d).clone(), false)]).is_ok())
        .count();
    assert_eq!(read_count, accepted_count());
    assert_eq!(accepted_count(), 216);
}

#[test]
fn an_asyncapi_document_reads_as_its_object_vertices_edges_and_constraints() {
    let got = read(&[(None, base(), false)]).unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(got.vertices["#"], "asyncapi");
    assert_eq!(got.vertices["#/channels/c"], "channel");
    assert_eq!(got.vertices["#/operations/o/channel"], "reference");
    assert_eq!(got.vertices["#/components/schemas/S"], "object");
    assert_eq!(
        got.vertices["#/components/schemas/F"],
        "multi-format-schema"
    );
    assert_eq!(got.vertices["#/components/schemas/F/schema"], "string");
    assert_eq!(
        got.constraints["#"],
        [("asyncapi".to_owned(), "\"3.1.0\"".to_owned())]
    );
    assert_eq!(
        got.constraints["#/channels/d/bindings"],
        [("mqtt".to_owned(), "{\"qos\":1}".to_owned())]
    );
    assert_eq!(
        got.constraints["#/components/schemas/S"],
        [
            ("type".to_owned(), "\"object\"".to_owned()),
            ("required".to_owned(), "[\"n/m\"]".to_owned()),
            ("discriminator".to_owned(), "\"n/m\"".to_owned()),
        ]
    );
    assert_eq!(
        got.edges["#/components/schemas/F"],
        [(
            "schema".to_owned(),
            "#/components/schemas/F/schema".to_owned(),
            None
        )]
    );
    assert_eq!(got.roots, ["#"]);
}

#[test]
fn asyncapi_references_resolve_to_their_targets() {
    let got = read(&[(None, base(), false)]).unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(
        got.edges["#/operations/o/channel"],
        [(
            "ref".to_owned(),
            "#/channels/c".to_owned(),
            Some("#/channels/c".to_owned())
        )]
    );
    assert_eq!(
        got.edges["#/components/messages/M/payload"],
        [(
            "ref".to_owned(),
            "#/components/schemas/S".to_owned(),
            Some("#/components/schemas/S".to_owned())
        )]
    );
    let fragment = m(&[(
        "messages",
        m(&[("x", m(&[("payload", m(&[("$ref", s("#/schemas/p"))]))]))]),
    )]);
    let with_schemas = match fragment {
        Doc::Map(mut members) => {
            members.push((
                "schemas".to_owned(),
                m(&[("p", m(&[("type", s("string"))]))]),
            ));
            Doc::Map(members)
        }
        other => other,
    };
    let entry = m(&[
        ("asyncapi", s("3.1.0")),
        ("info", m(&[("title", s("T")), ("version", s("1"))])),
        (
            "channels",
            m(&[(
                "c",
                m(&[(
                    "messages",
                    m(&[("x", m(&[("$ref", s("common.yaml#/messages/x"))]))]),
                )]),
            )]),
        ),
    ]);
    let got = read(&[
        (Some("https://e.test/api.yaml".to_owned()), entry, false),
        (
            Some("https://e.test/common.yaml".to_owned()),
            with_schemas,
            true,
        ),
    ])
    .unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(
        got.vertices["https://e.test/common.yaml#/messages/x"],
        "message"
    );
    assert_eq!(
        got.vertices["https://e.test/common.yaml#/schemas/p"],
        "string"
    );
}

#[test]
fn asyncapi_refusals_name_the_place_and_the_rule() {
    let with = |channel: usize, operation: usize, message: usize, addition: usize| {
        document(
            &versions()[0],
            &channels()[channel],
            &operations()[operation],
            &messages()[message],
            additions()[addition].as_ref(),
        )
    };
    let cases = [
        (
            document(
                &versions()[2],
                &channels()[0],
                &operations()[0],
                &messages()[0],
                None,
            ),
            "/asyncapi",
            "Version",
        ),
        (with(3, 0, 0, 0), "/channels/d/address", "Invalid"),
        (
            with(4, 0, 0, 0),
            "/channels/d/parameters",
            "MissingParameter",
        ),
        (with(5, 0, 0, 0), "/channels/d/servers/0/$ref", "Placement"),
        (with(0, 3, 0, 0), "/operations/o/action", "Invalid"),
        (with(0, 4, 0, 0), "/operations/o/channel/$ref", "Placement"),
        (
            with(0, 5, 0, 0),
            "/operations/o/messages/0/$ref",
            "NotInChannel",
        ),
        (
            with(0, 0, 4, 0),
            "/components/messages/M/payload/schemaFormat",
            "SchemaFormat",
        ),
        (
            with(0, 0, 5, 0),
            "/components/messages/M/payload/$ref",
            "FormatMismatch",
        ),
        (
            with(0, 0, 6, 0),
            "/components/messages/M/payload/$id",
            "Schema",
        ),
        (with(0, 0, 0, 3), "/paths", "UnknownField"),
        (with(0, 0, 0, 4), "/id", "Invalid"),
    ];
    for (doc, path, class) in cases {
        let got = refusal(doc);
        assert_eq!((got.path.as_str(), got.class), (path, class));
    }
}

#[test]
fn the_canonical_asyncapi_document_orders_fields_as_the_page_states() {
    let doc = m(&[
        (
            "info",
            m(&[("version", s("1")), ("x-i", Doc::Int(1)), ("title", s("T"))]),
        ),
        ("asyncapi", s("3.1.0")),
        (
            "channels",
            m(&[(
                "c",
                m(&[
                    ("description", s("d")),
                    ("address", Doc::Null),
                    (
                        "messages",
                        m(&[("m", m(&[("x", s("i")), ("$ref", s("#/m"))]))]),
                    ),
                ]),
            )]),
        ),
    ]);
    assert_eq!(
        canonical_text(&doc),
        "{\"asyncapi\":\"3.1.0\",\"info\":{\"title\":\"T\",\"version\":\"1\",\"x-i\":1},\
         \"channels\":{\"c\":{\"address\":null,\"messages\":{\"m\":{\"$ref\":\"#/m\",\"x\":\"i\"}},\
         \"description\":\"d\"}}}\n"
    );
}
