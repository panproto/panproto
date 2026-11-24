//! The kubernetes-crds reference on the page's own examples and scope.

#![allow(clippy::unwrap_used)]

use panproto_reference::enumerate::kubernetes_crds::{
    document_count, documents, envelopes, schema_nodes,
};
use panproto_reference::pages::kubernetes_crds::{Doc, Kind, canonical_text, read};

#[test]
fn the_crd_document_scope_has_its_closed_form_count() {
    assert_eq!(schema_nodes().len(), 18);
    assert_eq!(envelopes().len(), 24);
    assert_eq!(document_count(18, 24), 709);
    assert_eq!(documents().len(), document_count(18, 24));
}

#[test]
fn a_crd_reads_as_its_resource_versions_and_schema_nodes() {
    let docs = documents();
    let reading = read(&docs[1]).unwrap();
    let kinds: Vec<(&str, &str)> = reading
        .vertices
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("examples.test.com", "custom-resource"),
            ("examples.test.com/v1", "version"),
            ("examples.test.com/v1#", "object"),
            ("examples.test.com/v1#.a", "string"),
        ]
    );
    assert_eq!(
        reading.constraints["examples.test.com"],
        [
            ("group".to_owned(), "\"test.com\"".to_owned()),
            ("plural".to_owned(), "\"examples\"".to_owned()),
            ("kind".to_owned(), "\"Example\"".to_owned()),
            ("scope".to_owned(), "\"Namespaced\"".to_owned()),
        ]
    );
}

#[test]
fn crd_refusals_name_the_field_and_the_rule() {
    let refusal = |doc: &Doc| read(doc).unwrap_err();
    let env = envelopes();
    assert_eq!(refusal(&env[0]).kind, Kind::ApiVersion);
    assert_eq!(refusal(&env[3]).path, "metadata.name");
    assert_eq!(
        refusal(&env[9]).kind,
        Kind::DuplicateVersion("v1".to_owned())
    );
    assert_eq!(refusal(&env[10]).kind, Kind::StorageVersions(2));
    assert_eq!(refusal(&env[11]).kind, Kind::StorageVersions(0));
    assert_eq!(
        refusal(&env[13]).kind,
        Kind::Unsupported("a conversion webhook")
    );
    let docs = documents();
    let at = |i: usize| &docs[1 + i];
    assert_eq!(
        read(at(5)).unwrap_err().kind,
        Kind::ListMapKey("z".to_owned())
    );
    assert_eq!(read(at(11)).unwrap_err().kind, Kind::Forbidden("$ref"));
    assert_eq!(read(at(10)).unwrap_err().kind, Kind::Missing("type"));
}

#[test]
fn unresolved_required_entries_are_reported_and_list_maps_declare_keys() {
    let docs = documents();
    let reading = read(&docs[1 + 7]).unwrap();
    assert_eq!(
        reading.unresolved,
        [(
            "spec.versions[0].schema.openAPIV3Schema.properties.a.required[1]".to_owned(),
            "y".to_owned()
        )]
    );
    let keyed = read(&docs[1 + 4]).unwrap();
    assert_eq!(
        keyed.keys["examples.test.com/v1#.a[]"],
        ("examples.test.com/v1#.a".to_owned(), vec!["k".to_owned()])
    );
}

#[test]
fn the_canonical_crd_orders_fields_as_the_page_states() {
    let docs = documents();
    assert_eq!(
        canonical_text(&docs[1]),
        "{\"apiVersion\":\"apiextensions.k8s.io/v1\",\"kind\":\"CustomResourceDefinition\",\
         \"metadata\":{\"name\":\"examples.test.com\"},\"spec\":{\"group\":\"test.com\",\
         \"names\":{\"plural\":\"examples\",\"kind\":\"Example\"},\"scope\":\"Namespaced\",\
         \"versions\":[{\"name\":\"v1\",\"served\":true,\"storage\":true,\"schema\":\
         {\"openAPIV3Schema\":{\"type\":\"object\",\"properties\":{\"a\":{\"type\":\"string\"}}}}}]}}\n"
    );
}
