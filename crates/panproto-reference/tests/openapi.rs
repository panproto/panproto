//! The openapi reference on the page's own examples and scope.

use panproto_reference::enumerate::openapi::{
    accepted_count, additions, document, document_count, documents, parameters, responses, versions,
};
use panproto_reference::pages::json_schema::{Doc, m, s};
use panproto_reference::pages::openapi::{Refusal, canonical_text, read};

fn base() -> Doc {
    document(&versions()[0], &parameters()[0], &responses()[0], None)
}

fn refusal(doc: Doc) -> Refusal {
    match read(&[(None, doc)]) {
        Ok(reading) => panic!("{reading:?}"),
        Err(refusal) => refusal,
    }
}

#[test]
fn the_openapi_document_scope_has_its_closed_form_count() {
    assert_eq!(
        (
            versions().len(),
            parameters().len(),
            responses().len(),
            additions().len()
        ),
        (4, 13, 7, 10)
    );
    let all = documents();
    assert_eq!(all.len(), document_count());
    assert_eq!(document_count(), 3640);
    let read_count = all
        .iter()
        .filter(|d| read(&[(None, (*d).clone())]).is_ok())
        .count();
    assert_eq!(read_count, accepted_count());
    assert_eq!(accepted_count(), 96);
}

#[test]
fn a_document_reads_as_its_object_vertices_edges_and_constraints() {
    let got = read(&[(None, base())]).unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(got.vertices["#"], "openapi");
    assert_eq!(got.vertices["#/paths/~1p~1{id}/get"], "operation");
    assert_eq!(got.vertices["#/components/schemas/S"], "object");
    assert_eq!(
        got.constraints["#"],
        [("openapi".to_owned(), "\"3.1.0\"".to_owned())]
    );
    assert_eq!(
        got.edges["#/paths/~1p~1{id}/get/parameters/0"],
        [(
            "schema".to_owned(),
            "#/paths/~1p~1{id}/get/parameters/0/schema".to_owned(),
            None
        )]
    );
    assert_eq!(
        got.constraints["#/components/schemas/S"][1],
        (
            "discriminator".to_owned(),
            "{\"propertyName\":\"n/m\"}".to_owned()
        )
    );
    assert_eq!(got.entries, ["#"]);
    assert_eq!(got.roots, ["#"]);
}

#[test]
fn openapi_references_resolve_to_their_targets() {
    let doc = document(&versions()[0], &parameters()[1], &responses()[2], None);
    let got = read(&[(None, doc)]).unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(
        got.vertices["#/paths/~1p~1{id}/get/parameters/0"],
        "reference"
    );
    assert_eq!(
        got.edges["#/paths/~1p~1{id}/get/parameters/0"],
        [(
            "ref".to_owned(),
            "#/components/parameters/id".to_owned(),
            Some("#/components/parameters/id".to_owned())
        )]
    );
    assert_eq!(
        got.edges["#/paths/~1p~1{id}/get/responses/2XX/links/self"],
        [(
            "operation".to_owned(),
            "#/paths/~1p~1{id}/get".to_owned(),
            None
        )]
    );
}

#[test]
fn openapi_refusals_name_the_place_and_the_rule() {
    let cases: [(usize, usize, usize, usize, &str, &str); 6] = [
        (2, 0, 0, 0, "/openapi", "Version"),
        (0, 4, 0, 0, "/paths/~1p~1{id}/get/parameters/0", "Invalid"),
        (
            0,
            5,
            0,
            0,
            "/paths/~1p~1{id}/get/parameters/0/$ref",
            "RefTarget",
        ),
        (0, 8, 0, 0, "/paths/~1p~1{id}/get", "MissingPathParameter"),
        (0, 0, 3, 0, "/paths/~1p~1{id}/get/responses", "Invalid"),
        (0, 0, 0, 4, "/security/0/none", "UnknownScheme"),
    ];
    for (v, p, r, a, path, class) in cases {
        let doc = document(
            &versions()[v],
            &parameters()[p],
            &responses()[r],
            additions()[a].as_ref(),
        );
        let got = refusal(doc);
        assert_eq!((got.path.as_str(), got.class), (path, class));
    }
    let swagger = refusal(m(&[("swagger", s("2.0"))]));
    assert_eq!(
        (swagger.path.as_str(), swagger.class),
        ("/swagger", "Swagger")
    );
}

#[test]
fn the_canonical_openapi_document_orders_fields_as_the_page_states() {
    let doc = Doc::Map(vec![
        ("paths".into(), m(&[])),
        ("x-z".into(), Doc::Int(1)),
        ("info".into(), m(&[("version", s("1")), ("title", s("T"))])),
        ("openapi".into(), s("3.1.2")),
    ]);
    assert_eq!(
        canonical_text(&doc),
        "{\"openapi\":\"3.1.2\",\"info\":{\"title\":\"T\",\"version\":\"1\"},\"paths\":{},\"x-z\":1}\n"
    );
}
