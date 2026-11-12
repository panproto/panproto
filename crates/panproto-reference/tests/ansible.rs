//! The ansible reference on the page's own examples and scope.

#![allow(clippy::unwrap_used)]

use panproto_reference::enumerate::ansible::{base, document_count, documents, envelopes, tasks};
use panproto_reference::pages::ansible::{Class, Doc, Kind, canonical_text, read, variable_name};

#[test]
fn the_ansible_document_scope_has_its_closed_form_count() {
    assert_eq!(tasks().len(), 17);
    assert_eq!(envelopes().len(), 23);
    assert_eq!(document_count(17, 23), 330);
    assert_eq!(documents().len(), document_count(17, 23));
}

#[test]
fn a_playbook_reads_as_its_plays_tasks_and_handlers() {
    let t = tasks();
    let reading = read(&base(vec![t[1].clone()]), Class::Playbook).unwrap();
    let kinds: Vec<(&str, &str)> = reading
        .vertices
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("/", "playbook"),
            ("/[0]", "play"),
            ("/[0].handlers[0]", "handler"),
            ("/[0].tasks[0]", "task"),
        ]
    );
    assert_eq!(
        reading.edges["/[0].tasks[0]"],
        [("notify".to_owned(), "/[0].handlers[0]".to_owned(), None)]
    );
    assert_eq!(
        reading.constraints["/[0].tasks[0]"],
        [
            ("module".to_owned(), "\"copy\"".to_owned()),
            (
                "arguments".to_owned(),
                "{\"src\":\"a\",\"dest\":\"b\"}".to_owned()
            ),
            ("notify".to_owned(), "\"restart\"".to_owned()),
        ]
    );
    let missing = read(&base(vec![t[2].clone()]), Class::Playbook).unwrap();
    assert_eq!(
        missing.external,
        [(
            "[0].tasks[0].notify[0]".to_owned(),
            "handler",
            Some("missing".to_owned())
        )]
    );
}

#[test]
fn ansible_refusals_name_the_field_and_the_rule() {
    let t = tasks();
    let refusal = |i: usize| read(&base(vec![t[i].clone()]), Class::Playbook).unwrap_err();
    assert_eq!(refusal(5).path, "[0].tasks[0].with_items");
    assert_eq!(refusal(5).kind, Kind::Invalid);
    assert_eq!(refusal(6).path, "[0].tasks[0].copy");
    assert_eq!(refusal(7).kind, Kind::Missing("a module"));
    assert_eq!(
        refusal(8).kind,
        Kind::Unsupported("the action and local_action keywords")
    );
    assert_eq!(refusal(10).kind, Kind::UnknownField("foo".to_owned()));
    assert_eq!(refusal(12).kind, Kind::VariableName("bad-name".to_owned()));
    let env = envelopes();
    let at = |i: usize| read(&env[i].0, env[i].1).unwrap_err();
    assert_eq!(at(0).kind, Kind::Missing("hosts"));
    assert_eq!(at(4).kind, Kind::Missing("role"));
    assert_eq!(at(17).kind, Kind::VariableName("not".to_owned()));
    assert!(!variable_name("1x"));
    assert!(variable_name("_x1"));
}

#[test]
fn the_canonical_file_writes_keywords_then_variables_then_sections() {
    let env = envelopes();
    let (roles, class) = &env[3];
    assert_eq!(
        canonical_text(roles, *class),
        "[{\"hosts\":\"all\",\"roles\":[{\"role\":\"common\"},{\"role\":\"web\",\"tags\":[\"t\"],\
         \"http_port\":80}],\"tasks\":[{\"ping\":null}],\"handlers\":[{\"name\":\"restart\",\
         \"service\":{\"name\":\"x\"}}]}]\n"
    );
    assert_eq!(canonical_text(&Doc::Null, Class::Vars), "{}\n");
}
