//! Tests of the theory DSL reference on hand-written documents.

use panproto_reference::enumerate::theory_dsl::library;
use panproto_reference::json::{Value, parse};
use panproto_reference::pages::theories::theory::{Defect, MorphismError, Site, SortExpr};
use panproto_reference::pages::theory_dsl::{
    Outcome, Path, Refusal, ShapeProblem, check_document, compile_document, compile_morphism,
    locate,
};

fn value(text: &str) -> Value {
    parse(text).unwrap_or(Value::Null)
}

fn compiled(text: &str) -> Outcome<panproto_reference::pages::theory_dsl::Compiled> {
    compile_document(&value(text), &library)
}

#[test]
fn the_document_grammar_refuses_in_field_table_order() {
    let refused = |text: &str| check_document(&value(text)).err();
    assert_eq!(
        refused(r#"{"id":"x","description":"","theory":"T","morphism":"F"}"#),
        Some((
            Path::root(),
            ShapeProblem::SeveralBodies(vec!["theory".to_owned(), "morphism".to_owned()])
        ))
    );
    assert_eq!(
        refused(r#"{"id":"x","description":""}"#),
        Some((Path::root(), ShapeProblem::NoBody))
    );
    assert_eq!(
        refused(r#"{"zz":1,"b":2,"id":"x","description":"","theory":"T"}"#),
        Some((Path::root().key("b"), ShapeProblem::UnknownField))
    );
    assert_eq!(
        refused(r#"{"id":"x","theory":"T","sorts":[{"name":1}]}"#),
        Some((
            Path::root(),
            ShapeProblem::MissingField("description".to_owned())
        ))
    );
    assert_eq!(
        refused(
            r#"{"id":"x","description":"","theory":"T","sorts":[{"name":"S","kind":{"type":"nope"}}]}"#
        ),
        Some((
            Path::root().key("sorts").index(0).key("kind").key("type"),
            ShapeProblem::UnknownVariant("nope".to_owned())
        ))
    );
    assert_eq!(
        refused(
            r#"{"id":"x","description":"","instance":"I","class":"C","target":"T","bindings":{}}"#
        ),
        None
    );
}

#[test]
fn imports_rename_and_parents_take_precedence_over_bare_imported_names() {
    let Outcome::Accepted(out) = compiled(
        r#"{"id":"x","description":"","theory":"T","extends":["L"],"imports":[{"from":"L"}],"ops":[{"name":"f","input":"V","output":"L.E"}]}"#,
    ) else {
        panic!("the theory compiles");
    };
    let theory = &out.theories["T"];
    let names: Vec<&str> = theory.sorts.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["V", "E", "L_V", "L_E"]);
    let f = theory
        .ops
        .iter()
        .find(|o| o.name == "f")
        .map(|o| (&o.inputs[0], &o.output));
    let Some(((binder, input, _), output)) = f else {
        panic!("f is declared");
    };
    assert_eq!(binder, "v");
    assert_eq!(*input, SortExpr::Name("V".to_owned()));
    assert_eq!(*output, SortExpr::Name("L_E".to_owned()));

    assert_eq!(
        compiled(
            r#"{"id":"x","description":"","theory":"T","imports":[{"from":"L","expose":["W"]}]}"#
        ),
        Outcome::Refused(
            Path::root().key("imports").index(0).key("expose").index(0),
            Refusal::UnknownExposed("W".to_owned())
        )
    );
    assert_eq!(
        compiled(
            r#"{"id":"x","description":"","theory":"T","equations":[{"name":"e","lhs":"f(x,)","rhs":"x"}]}"#
        ),
        Outcome::Refused(
            Path::root().key("equations").index(0).key("lhs"),
            Refusal::TermParse
        )
    );
}

#[test]
fn implicit_inputs_are_inferred_and_equation_variables_stay_fixed() {
    let with_equation = |lhs: &str, rhs: &str| {
        compiled(&format!(
            r#"{{"id":"x","description":"","theory":"T","imports":[{{"from":"C","alias":"D","expose":["Hom","Ob","id","comp"]}}],"equations":[{{"name":"e","lhs":"{lhs}","rhs":"{rhs}"}}]}}"#
        ))
    };
    let Outcome::Accepted(out) = with_equation("comp(id(a), f)", "f") else {
        panic!("comp(id(a), f) = f types f at Hom(x, a)");
    };
    let hom = out.theories["T"]
        .sorts
        .iter()
        .find(|s| s.name == "Hom")
        .map(|s| s.params.len());
    assert_eq!(hom, Some(2));
    assert_eq!(
        with_equation("comp(id(a), id(b))", "id(a)"),
        Outcome::Refused(
            Path::root().key("equations").index(0),
            Refusal::Equation("e".to_owned())
        )
    );
    assert_eq!(
        compiled(
            r#"{"id":"x","description":"","theory":"T","sorts":[{"name":"Ob"}],"ops":[{"name":"g","inputs":[{"name":"a","sort":"Ob","implicit":true},{"name":"x","sort":"Ob"}],"output":"Ob"}]}"#
        ),
        Outcome::Refused(
            Path::root().key("ops").index(0),
            Refusal::NonInferableImplicit("g".to_owned())
        )
    );
    assert_eq!(
        compiled(
            r#"{"id":"x","description":"","theory":"T","imports":[{"from":"C"}],"sorts":[{"name":"Sq","params":[{"name":"x","sort":"Ob"},{"name":"f","sort":"Hom(x, id(x))"}]}]}"#
        ),
        Outcome::Refused(
            Path::root()
                .key("sorts")
                .index(0)
                .key("params")
                .index(1)
                .key("sort"),
            Refusal::Signature(Defect::ParamIllTyped(
                Site::SortParam("Sq".to_owned(), 1),
                "C_Hom".to_owned(),
                1,
                "C_Ob".to_owned(),
                "C_Hom(x, x)".to_owned()
            ))
        )
    );
}

#[test]
fn morphism_refusals_are_located_at_the_entry_they_concern() {
    let morphism = |sorts: &str, ops: &str| {
        compile_morphism(
            &value(&format!(
                r#"{{"morphism":"F","domain":"L","codomain":"K","sort_map":{sorts},"op_map":{ops}}}"#
            )),
            &library,
        )
    };
    assert!(matches!(
        morphism(r#"{"V":"P","E":"Q"}"#, r#"{"s":"a","t":"b","r":"c"}"#),
        Outcome::Accepted(_)
    ));
    assert_eq!(
        morphism(r#"{"V":"P"}"#, "{}"),
        Outcome::Refused(
            Path::root().key("sort_map"),
            Refusal::MorphismCheck(MorphismError::MissingSortMapping("E".to_owned()))
        )
    );
    assert_eq!(
        morphism(r#"{"V":"P","E":"Q","Z":"P"}"#, "{}"),
        Outcome::Refused(
            Path::root().key("sort_map").key("Z"),
            Refusal::UnknownMapKey("sort".to_owned(), "Z".to_owned())
        )
    );
    let Outcome::Refused(at, Refusal::MorphismCheck(_)) =
        morphism(r#"{"V":"P","E":"Q"}"#, r#"{"s":"c","t":"b","r":"c"}"#)
    else {
        panic!("s mapped to c is refused");
    };
    assert_eq!(at, Path::root().key("op_map").key("s"));
}

#[test]
fn locate_finds_the_last_occurrence_of_a_member() {
    let text = r#" {"a": [1, {"b": "x"}], "a": {"c" : true}} "#;
    assert_eq!(locate(text, &Path::root()), Some((1, text.len() - 2)));
    assert_eq!(locate(text, &Path::root().key("a").key("c")), Some((36, 4)));
    assert_eq!(locate(text, &Path::root().key("a").index(1)), None);
    assert_eq!(locate("{", &Path::root()), None);
}

#[test]
fn protocol_references_resolve_compile_and_record_inline_theories() {
    let protocol = |schema: &str, instance: &str| {
        compiled(&format!(
            r#"{{"id":"p","description":"","protocol":"P","schema_theory":{schema},"instance_theory":{instance},"edge_rules":[{{"edge_kind":"prop","src_kind":"object","tgt_kind":"string"}}]}}"#
        ))
    };
    let Outcome::Accepted(out) = protocol(r#"{"theory":"S","extends":["L"]}"#, r#""K""#) else {
        panic!("an inline theory extending a library theory compiles");
    };
    let p = &out.protocols["P"];
    assert_eq!(
        (p.schema_theory.as_str(), p.instance_theory.as_str()),
        ("S", "K")
    );
    assert_eq!(
        p.edge_rules,
        [("prop".to_owned(), "object".to_owned(), "string".to_owned())]
    );
    assert_eq!(out.theories["S"].sorts.len(), 2);
    assert_eq!(
        protocol(r#""L""#, r#""M""#),
        Outcome::Refused(
            Path::root().key("instance_theory"),
            Refusal::TheoryNotFound("M".to_owned())
        )
    );
    assert_eq!(
        protocol(
            r#"{"theory":"S","ops":[{"name":"f","output":"Q"}]}"#,
            r#""K""#
        ),
        Outcome::Refused(
            Path::root()
                .key("schema_theory")
                .key("ops")
                .index(0)
                .key("output"),
            Refusal::Signature(Defect::UndeclaredSort(
                Site::OpOutput("f".to_owned()),
                "Q".to_owned()
            ))
        )
    );
    assert_eq!(
        protocol(
            r#"{"theory":"S","sorts":[{"name":"X"}]}"#,
            r#"{"theory":"S","sorts":[{"name":"Y"}]}"#
        ),
        Outcome::Refused(Path::root(), Refusal::Duplicate("S".to_owned()))
    );
    assert!(matches!(
        protocol(r#"{"result":"R","steps":[]}"#, r#""K""#),
        Outcome::OutsideDomain(_)
    ));
}
