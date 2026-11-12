//! The cloudformation reference on the page's own examples and scope.

#![allow(clippy::unwrap_used)]

use panproto_reference::enumerate::cloudformation::{
    base, document_count, documents, envelopes, values,
};
use panproto_reference::pages::cloudformation::{Doc, Kind, canonical_text, read, sub_variables};

#[test]
fn the_template_document_scope_has_its_closed_form_count() {
    assert_eq!(values().len(), 17);
    assert_eq!(envelopes().len(), 26);
    assert_eq!(document_count(17, 26), 639);
    assert_eq!(documents().len(), document_count(17, 26));
}

#[test]
fn a_template_reads_as_its_declarations_and_their_references() {
    let reading = read(&base(&[])).unwrap();
    let kinds: Vec<(&str, &str)> = reading
        .vertices
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Conditions.C", "condition"),
            ("Mappings.M", "mapping"),
            ("Parameters.P", "parameter"),
            ("Resources.R", "resource"),
            ("Resources.S", "resource"),
            ("Template", "template"),
        ]
    );
    assert_eq!(
        reading.edges["Conditions.C"],
        [("ref".to_owned(), "Parameters.P".to_owned(), None)]
    );
    let docs = documents();
    let sub = read(&docs[1 + 8]).unwrap();
    assert_eq!(
        sub.edges["Resources.S"],
        [
            ("ref".to_owned(), "Parameters.P".to_owned(), None),
            (
                "get-att".to_owned(),
                "Resources.R".to_owned(),
                Some("Arn".to_owned())
            ),
        ]
    );
    let import = read(&docs[1 + 12]).unwrap();
    assert_eq!(
        import.imports,
        [(
            "Resources.S.Properties.a.Fn::ImportValue".to_owned(),
            Some("e".to_owned())
        )]
    );
}

#[test]
fn template_refusals_name_the_field_and_the_rule() {
    let docs = documents();
    let refusal = |i: usize| read(&docs[1 + i]).unwrap_err();
    assert_eq!(refusal(4).kind, Kind::Unresolved("Z".to_owned()));
    assert_eq!(refusal(4).path, "Resources.S.Properties.a.Ref");
    assert_eq!(refusal(13).kind, Kind::Arguments("Fn::Join".to_owned()));
    assert_eq!(
        refusal(14).kind,
        Kind::Unsupported("a language extension function")
    );
    assert_eq!(
        refusal(15).kind,
        Kind::UnknownFunction("Fn::Bogus".to_owned())
    );
    let env = envelopes();
    let at = |i: usize| read(&env[i]).unwrap_err();
    assert_eq!(at(1).kind, Kind::FormatVersion);
    assert_eq!(at(2).kind, Kind::Unsupported("a transform"));
    assert_eq!(at(8).kind, Kind::Missing("Resources"));
    assert_eq!(at(10).kind, Kind::DuplicateName("R".to_owned()));
    assert_eq!(at(19).path, "Resources.S.DependsOn[1]");
    assert_eq!(at(21).path, "Conditions.C.Fn::Equals[0].Ref");
    assert_eq!(at(21).kind, Kind::Invalid);
}

#[test]
fn sub_variables_skip_literals_and_stop_at_an_unclosed_variable() {
    assert_eq!(
        sub_variables("arn:${AWS::Partition}:s3:::${Bucket}/${!Literal}/${Res.Arn}/${open"),
        ["AWS::Partition", "Bucket", "Res.Arn"]
    );
}

#[test]
fn the_canonical_template_orders_sections_and_attributes_as_the_page_states() {
    let env = envelopes();
    let dependent = &env[18];
    assert!(read(dependent).is_ok());
    assert_eq!(
        canonical_text(dependent),
        "{\"Parameters\":{\"P\":{\"Type\":\"String\"}},\"Mappings\":{\"M\":{\"k\":{\"v\":\"x\"}}},\
         \"Conditions\":{\"C\":{\"Fn::Equals\":[{\"Ref\":\"P\"},\"a\"]}},\"Resources\":{\"R\":\
         {\"Type\":\"AWS::S3::Bucket\"},\"S\":{\"Type\":\"AWS::SNS::Topic\",\"Properties\":\
         {\"a\":\"x\"},\"DependsOn\":[\"R\"]}}}\n"
    );
    let Doc::Map(entries) = &env[0] else {
        panic!("a template is a mapping");
    };
    assert_eq!(entries[0].0, "AWSTemplateFormatVersion");
}
