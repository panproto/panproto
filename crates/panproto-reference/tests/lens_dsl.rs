//! Tests of the lens DSL reference on hand-written documents.

use panproto_reference::dsl_path::{Path, locate};
use panproto_reference::enumerate::lens_dsl::library;
use panproto_reference::json::{Value, parse};
use panproto_reference::pages::lens_dsl::{
    Class, CompiledLens, Context, Datum, Elementary, ExprSpec, FieldTransform, Kind, Oracles,
    Outcome, Proto, Refusal, ShapeProblem, Stage, TemplatePart, Transforms, check_document,
    compile_document, horizontal, vertical,
};

fn expression(text: &str) -> bool {
    !text.contains("((")
}

const fn honest(_: &str, _: Option<&str>, _: Class, _: Kind) -> bool {
    true
}

fn directed(equation: &Elementary) -> bool {
    !matches!(equation, Elementary::DirectedEq { name, .. } if name == "dishonest")
}

const ORACLES: Oracles<'static> = Oracles {
    expression: &expression,
    coercion: &honest,
    directed: &directed,
};

fn value(text: &str) -> Value {
    parse(text).unwrap_or(Value::Null)
}

fn compiled(body: &str) -> Outcome<CompiledLens> {
    let context = Context {
        body: "b",
        oracles: &ORACLES,
        documents: &library,
    };
    compile_document(
        &value(&format!(
            r#"{{"id":"top","source":"s","target":"t",{body}}}"#
        )),
        &context,
    )
}

fn stage(chain: Vec<Elementary>) -> Stage {
    Stage {
        chain: chain.into_iter().map(Proto::Elementary).collect(),
        transforms: Transforms::new(),
    }
}

#[test]
fn the_lens_grammar_refuses_in_field_table_order() {
    let refused = |text: &str| check_document(&value(text)).err();
    assert_eq!(
        refused(r#"{"id":"x","source":"s","target":"t","steps":[],"rules":[]}"#),
        Some((
            Path::root(),
            ShapeProblem::SeveralBodies(vec!["steps".to_owned(), "rules".to_owned()])
        ))
    );
    assert_eq!(
        refused(r#"{"zz":1,"b":2,"id":"x","source":"s","target":"t","steps":[]}"#),
        Some((Path::root().key("b"), ShapeProblem::UnknownField))
    );
    assert_eq!(
        refused(r#"{"id":"x","source":"s","steps":[{"zz":1,"drop_op":"f"}]}"#),
        Some((
            Path::root(),
            ShapeProblem::MissingField("target".to_owned())
        ))
    );
    assert_eq!(
        refused(r#"{"id":"x","source":"s","target":"t","steps":[{"zz":1,"drop_op":"f"}]}"#),
        Some((
            Path::root().key("steps").index(0).key("zz"),
            ShapeProblem::UnknownVariant("zz".to_owned())
        ))
    );
    assert_eq!(
        refused(
            r#"{"id":"x","source":"s","target":"t","steps":[{"drop_op":"f","drop_sort":"s"}]}"#
        ),
        Some((
            Path::root().key("steps").index(0),
            ShapeProblem::SeveralVariants(vec!["drop_sort".to_owned(), "drop_op".to_owned()])
        ))
    );
    assert_eq!(
        refused(
            r#"{"id":"x","source":"s","target":"t","symmetric":{"left":[],"right":[]},"directed_equations":[]}"#
        ),
        Some((
            Path::root().key("directed_equations"),
            ShapeProblem::UnknownField
        ))
    );
    assert_eq!(
        refused(r#"{"id":"x","source":"s","target":"t","auto":{"max_results":1.0}}"#),
        Some((
            Path::root().key("auto").key("max_results"),
            ShapeProblem::WrongType(panproto_reference::pages::lens_dsl::Expected::Natural)
        ))
    );
}

#[test]
fn a_step_list_compiles_to_one_stage_per_step() {
    let Outcome::Accepted(lens) = compiled(
        r#""steps":[{"remove_field":"a"},{"add_field":{"name":"c","kind":"integer","default":1,"expr":"a"}},{"scoped":{"focus":"i","inner":[]}}]"#,
    ) else {
        panic!("the steps compile");
    };
    let mut added = stage(vec![
        Elementary::AddSort {
            sort: "b.c".to_owned(),
            kind: "integer".to_owned(),
            default: Some(Datum::Int(1)),
        },
        Elementary::AddEdge {
            src: "b".to_owned(),
            tgt: "b.c".to_owned(),
            name: "c".to_owned(),
            kind: "prop".to_owned(),
        },
    ]);
    added.transforms.insert(
        "b".to_owned(),
        vec![FieldTransform::Compute {
            target: "c".to_owned(),
            expr: ExprSpec::Text("a".to_owned()),
            inverse: None,
            class: Class::Projection,
        }],
    );
    assert_eq!(
        lens.stages,
        vec![stage(vec![Elementary::DropSort("b.a".to_owned())]), added]
    );
}

#[test]
fn rule_maps_expand_in_key_order() {
    let Outcome::Accepted(lens) = compiled(
        r#""rules":[{"pattern":{"name":"h"},"replace":{"name":{"template":"h{ n }"},"rename_attrs":{"z":"y","a":"b"}}}],"passthrough":"drop""#,
    ) else {
        panic!("the rules compile");
    };
    let names: Vec<&Stage> = lens.stages.iter().collect();
    assert_eq!(names.len(), 4);
    assert_eq!(
        names[0].transforms["b"],
        vec![FieldTransform::Compute {
            target: "name".to_owned(),
            expr: ExprSpec::Template(vec![
                TemplatePart::Lit("h".to_owned()),
                TemplatePart::Var("n".to_owned()),
            ]),
            inverse: None,
            class: Class::Projection,
        }]
    );
    assert!(matches!(
        &names[1].chain[..],
        [Proto::Elementary(Elementary::RenameEdgeName { old, .. })] if old == "a"
    ));
    assert_eq!(
        names[3].transforms["b"],
        vec![FieldTransform::Keep(vec!["h".to_owned()])]
    );
}

#[test]
fn vertical_composition_concatenates_and_resolves_references_transitively() {
    let first = vec![stage(vec![Elementary::DropOp("f".to_owned())])];
    let second = vec![stage(vec![Elementary::DropSort("s".to_owned())])];
    let mut both = first.clone();
    both.extend(second.clone());
    assert_eq!(vertical(vec![first, Vec::new(), second]), both);

    let Outcome::Refused(at, Refusal::InRef(id, inner)) =
        compiled(r#""compose":{"mode":"vertical","lenses":[{"ref":"cycle.a"}]}"#)
    else {
        panic!("a cycle is refused");
    };
    assert_eq!(
        at,
        Path::root()
            .key("compose")
            .key("lenses")
            .index(0)
            .key("ref")
    );
    assert_eq!(id, "cycle.a");
    assert!(matches!(&inner.1, Refusal::InRef(b, deeper)
        if b == "cycle.b"
            && deeper.1 == Refusal::RefCycle(vec!["top".to_owned(), "cycle.a".to_owned(), "cycle.b".to_owned(), "cycle.a".to_owned()])));

    assert!(matches!(
        compiled(r#""compose":{"mode":"vertical","lenses":[{"ref":"ungrammatical.doc"}]}"#),
        Outcome::OutsideDomain(_)
    ));
}

#[test]
fn horizontal_composition_folds_fused_parts_from_the_left() {
    let part = |e: Elementary| vec![stage(vec![e])];
    let a = Elementary::DropOp("f".to_owned());
    let b = Elementary::DropSort("s".to_owned());
    let c = Elementary::RenameOp {
        old: "g".to_owned(),
        new: "h".to_owned(),
    };
    let Ok(stages) = horizontal(vec![
        part(a.clone()),
        vec![stage(vec![b.clone(), c.clone()])],
    ]) else {
        panic!("non-empty parts compose");
    };
    assert_eq!(
        stages[0].chain,
        vec![Proto::Horizontal(
            Box::new(Proto::Elementary(a)),
            Box::new(Proto::Fused(vec![
                Proto::Elementary(b),
                Proto::Elementary(c)
            ]))
        )]
    );
    assert_eq!(horizontal(Vec::new()), Ok(Vec::new()));
    assert_eq!(
        horizontal(vec![part(Elementary::DropOp("f".to_owned())), Vec::new()]),
        Err((1, Refusal::EmptyHorizontalPart))
    );
}

#[test]
fn refusals_are_located_at_the_member_they_concern() {
    let at = |body: &str| match compiled(body) {
        Outcome::Refused(at, refusal) => Some((at, refusal)),
        _ => None,
    };
    assert_eq!(
        at(r#""steps":[{"drop_op":"f"},{"apply_expr":{"field":"a","expr":"(("}}]"#),
        Some((
            Path::root()
                .key("steps")
                .index(1)
                .key("apply_expr")
                .key("expr"),
            Refusal::ExprParse
        ))
    );
    assert_eq!(
        at(
            r#""rules":[{"pattern":{"name":"p"},"replace":{"add_attrs":{"n":18446744073709551615}}}]"#
        ),
        Some((
            Path::root()
                .key("rules")
                .index(0)
                .key("replace")
                .key("add_attrs")
                .key("n"),
            Refusal::DefaultValue
        ))
    );
    assert_eq!(
        at(r#""steps":[{"drop_sort":"s"}],"invertible":true"#),
        Some((Path::root().key("invertible"), Refusal::NotInvertible))
    );
    assert_eq!(
        at(r#""symmetric":{"left":[],"right":[{"apply_expr":{"field":"a","expr":"a"}}]}"#),
        Some((
            Path::root().key("symmetric").key("right").index(0),
            Refusal::ValueStepInSymmetricLeg
        ))
    );
    assert_eq!(
        at(
            r#""steps":[],"directed_equations":[{"name":"dishonest","lhs":"x","rhs":"x","impl":"x"}]"#
        ),
        Some((
            Path::root().key("directed_equations").index(0),
            Refusal::CoercionNotHonest
        ))
    );
    assert_eq!(
        at(r#""from_diff":{}"#),
        Some((Path::root().key("from_diff"), Refusal::AutoRequiresSchemas))
    );
}

#[test]
fn locate_finds_the_last_occurrence_of_a_member_in_a_lens_document() {
    let text = r#"{"id":"x","steps":[{"drop_op":"f"}],"steps":[ {"drop_op" : "g"} ]}"#;
    let path = Path::root().key("steps").index(0).key("drop_op");
    let Some((offset, length)) = locate(text, &path) else {
        panic!("the path names a value");
    };
    assert_eq!(&text[offset..offset + length], r#""g""#);
    assert_eq!(locate(text, &Path::root().key("missing")), None);
}
