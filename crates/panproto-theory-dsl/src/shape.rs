//! The document grammar, checked on a document value before it is read.
//!
//! A theory document arrives as a JSON-like value: parsed JSON, a YAML
//! document, or an evaluated Nickel record. [`read_document`] checks the
//! value against the grammar below and refuses the first value that breaks
//! it, naming its [`DocumentPath`], before the value is deserialized into a
//! [`TheoryDocument`]. The grammar is the one the serde types of
//! [`crate::document`] accept, with two further rules those types cannot
//! state: a document has exactly one body, and no object carries a member
//! the grammar does not name.
//!
//! The check visits a value in a fixed order, which does not depend on the
//! order of an object's members in the source. An object is checked first
//! for a member the grammar does not name (the least such name, comparing
//! names by their UTF-8 bytes), then field by field in the order of its
//! field table: a required field that is absent is refused, and a present
//! one is checked recursively before the next field. A map's members are
//! checked in the order of their names, and an array element by element.
//! When an object names a member more than once, the last occurrence is the
//! member read.

use panproto_dsl_eval::shape::{
    BOOL, Field, STR, STR_MAP, STRS, Shape, ShapeError, check_record, opt, req,
};
pub use panproto_dsl_eval::shape::{Expected, ShapeProblem};
use serde_json::Value;

use crate::document::{
    BundleSpec, ClassSpec, CompositionBody, InductiveSpec, InstanceSpec, MorphismSpec,
    ProtocolSpec, TheoryBody, TheoryDocument, TheorySpec,
};
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

static OPT_STR: Shape = Shape::Nullable(&STR);
static OPT_STRS: Shape = Shape::Nullable(&STRS);

static PARAM: Shape =
    Shape::Record(&[req("name", &STR), req("sort", &STR), opt("implicit", &BOOL)]);
static PARAMS: Shape = Shape::List(&PARAM);
static SORT_KIND: Shape = Shape::Tagged {
    tag: "type",
    variants: &[
        ("structural", &[]),
        ("val", &[req("value_kind", &STR)]),
        (
            "coercion",
            &[req("from", &STR), req("to", &STR), req("class", &STR)],
        ),
        ("merger", &[req("value_kind", &STR)]),
    ],
};
static SORT: Shape = Shape::Record(&[
    req("name", &STR),
    opt("params", &PARAMS),
    opt("kind", &SORT_KIND),
    opt("closed", &OPT_STRS),
]);
static OP: Shape = Shape::Record(&[
    req("name", &STR),
    opt("input", &OPT_STR),
    opt("inputs", &OPT_PARAMS),
    req("output", &STR),
]);
static OPS: Shape = Shape::List(&OP);
static EQUATION: Shape = Shape::Record(&[req("name", &STR), req("lhs", &STR), req("rhs", &STR)]);
static EQUATIONS: Shape = Shape::List(&EQUATION);
static DIRECTED: Shape = Shape::Record(&[
    req("name", &STR),
    req("lhs", &STR),
    req("rhs", &STR),
    req("impl_expr", &STR),
    opt("inverse", &OPT_STR),
    opt("source_kind", &OPT_STR),
    opt("target_kind", &OPT_STR),
    opt("coercion_class", &STR),
]);
static STRATEGY: Shape = Shape::Tagged {
    tag: "type",
    variants: &[
        ("keep_left", &[]),
        ("keep_right", &[]),
        ("fail", &[]),
        ("custom", &[req("expr", &STR)]),
    ],
};
static POLICY: Shape = Shape::Record(&[
    req("name", &STR),
    req("value_kind", &STR),
    req("strategy", &STRATEGY),
]);
static IMPORT: Shape = Shape::Record(&[
    req("from", &STR),
    opt("alias", &OPT_STR),
    opt("expose", &STRS),
]);

const THEORY_FIELDS: &[Field] = &[
    req("theory", &STR),
    opt("extends", &STRS),
    opt("imports", &IMPORTS),
    opt("sorts", &SORTS),
    opt("ops", &OPS),
    opt("equations", &EQUATIONS),
    opt("directed_equations", &DIRECTEDS),
    opt("policies", &POLICIES),
];
static THEORY: Shape = Shape::Record(THEORY_FIELDS);

const MORPHISM_FIELDS: &[Field] = &[
    req("morphism", &STR),
    req("domain", &STR),
    req("codomain", &STR),
    req("sort_map", &STR_MAP),
    req("op_map", &STR_MAP),
];

static IDENTIFY: Shape = Shape::Record(&[
    req("kind", &IDENTIFY_KIND),
    req("left", &STR),
    req("right", &STR),
]);
static STEP: Shape = Shape::Record(&[
    req("left", &STR),
    req("right", &STR),
    req("shared_sorts", &STRS),
    opt("shared_ops", &STRS),
    opt("mode", &MODE),
    opt("identify", &IDENTIFIES),
]);
const COMPOSITION_FIELDS: &[Field] = &[
    req("result", &STR),
    opt("bases", &STRS),
    req("steps", &STEPS),
];
static COMPOSITION: Shape = Shape::Record(COMPOSITION_FIELDS);

static EDGE_RULE: Shape = Shape::Record(&[
    req("edge_kind", &STR),
    req("src_kind", &STR),
    req("tgt_kind", &STR),
]);
const PROTOCOL_FIELDS: &[Field] = &[
    req("protocol", &STR),
    req("schema_theory", &THEORY_REF),
    req("instance_theory", &THEORY_REF),
    opt("edge_rules", &EDGE_RULES),
];

const BUNDLE_FIELDS: &[Field] = &[
    req("bundle", &STR),
    opt("theories", &THEORIES),
    opt("morphisms", &MORPHISMS),
    opt("compositions", &COMPOSITIONS),
    opt("protocols", &PROTOCOLS),
];

const CLASS_FIELDS: &[Field] = &[
    req("class", &STR),
    req("params", &STRS),
    req("signatures", &OPS),
    opt("axioms", &EQUATIONS),
];

const INSTANCE_FIELDS: &[Field] = &[
    req("instance", &STR),
    req("class", &STR),
    req("target", &STR),
    req("bindings", &STR_MAP),
];

static CONSTRUCTOR: Shape = Shape::Record(&[req("name", &STR), opt("inputs", &PARAMS)]);
const INDUCTIVE_FIELDS: &[Field] = &[
    req("inductive", &STR),
    opt("params", &PARAMS),
    req("constructors", &CONSTRUCTORS),
];

static OPT_PARAMS: Shape = Shape::Nullable(&PARAMS);
static IMPORTS: Shape = Shape::List(&IMPORT);
static SORTS: Shape = Shape::List(&SORT);
static DIRECTEDS: Shape = Shape::List(&DIRECTED);
static POLICIES: Shape = Shape::List(&POLICY);
static IDENTIFY_KIND: Shape = Shape::Choice(&["sort", "op"]);
static MODE: Shape = Shape::Choice(&["amalgamate", "explicit_identity"]);
static IDENTIFIES: Shape = Shape::List(&IDENTIFY);
static STEPS: Shape = Shape::List(&STEP);
static THEORY_REF: Shape = Shape::StrOr(&THEORY_OR_COMPOSITION);
static THEORY_OR_COMPOSITION: Shape = Shape::Switch {
    member: "theory",
    present: THEORY_FIELDS,
    absent: COMPOSITION_FIELDS,
};
static EDGE_RULES: Shape = Shape::List(&EDGE_RULE);
static THEORIES: Shape = Shape::List(&THEORY);
static MORPHISM: Shape = Shape::Record(MORPHISM_FIELDS);
static MORPHISMS: Shape = Shape::List(&MORPHISM);
static COMPOSITIONS: Shape = Shape::List(&COMPOSITION);
static PROTOCOL: Shape = Shape::Record(PROTOCOL_FIELDS);
static PROTOCOLS: Shape = Shape::List(&PROTOCOL);
static CONSTRUCTORS: Shape = Shape::List(&CONSTRUCTOR);
const COMPOSE_FIELDS: &[Field] = &[req("compose", &COMPOSITION)];

/// The body keys, in grammar order, with each body's field table.
const BODIES: &[(&str, &[Field])] = &[
    ("theory", THEORY_FIELDS),
    ("morphism", MORPHISM_FIELDS),
    ("compose", COMPOSE_FIELDS),
    ("protocol", PROTOCOL_FIELDS),
    ("bundle", BUNDLE_FIELDS),
    ("class", CLASS_FIELDS),
    ("instance", INSTANCE_FIELDS),
    ("inductive", INDUCTIVE_FIELDS),
];

fn refuse(error: ShapeError) -> TheoryDslError {
    TheoryDslError::Shape {
        path: error.path,
        problem: error.problem,
    }
}

/// Read a document value into a [`TheoryDocument`].
///
/// The value must be an object carrying `id` and `description` strings and
/// exactly one body key; the body key selects the field table the whole
/// object is checked against (with `id` and `description` first).
///
/// # Errors
///
/// [`TheoryDslError::Shape`] naming the path of the first value the grammar
/// refuses, in the order the module documentation states.
pub fn read_document(value: &Value) -> Result<TheoryDocument, TheoryDslError> {
    let root = DocumentPath::root();
    let Some(members) = value.as_object() else {
        return Err(TheoryDslError::Shape {
            path: root,
            problem: ShapeProblem::WrongType {
                expected: Expected::Object,
            },
        });
    };
    let mut present: Vec<&str> = BODIES
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| members.contains_key(*key))
        .collect();
    if present.contains(&"instance") {
        present.retain(|key| *key != "class");
    }
    let body = match present.as_slice() {
        [] => {
            return Err(TheoryDslError::Shape {
                path: root,
                problem: ShapeProblem::NoBody,
            });
        }
        [one] => *one,
        several => {
            return Err(TheoryDslError::Shape {
                path: root,
                problem: ShapeProblem::SeveralBodies {
                    keys: several.iter().map(|key| (*key).to_owned()).collect(),
                },
            });
        }
    };
    let fields = BODIES
        .iter()
        .find(|(key, _)| *key == body)
        .map_or(&[][..], |(_, fields)| *fields);
    check_record(
        value,
        fields,
        &[req("id", &STR), req("description", &STR)],
        &root,
    )
    .map_err(refuse)?;
    let text = |key: &str| {
        members
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let deserialized = TheoryDslError::Json;
    let body = match body {
        "theory" => TheoryBody::Theory(
            serde_json::from_value::<TheorySpec>(value.clone()).map_err(deserialized)?,
        ),
        "morphism" => TheoryBody::Morphism(
            serde_json::from_value::<MorphismSpec>(value.clone()).map_err(deserialized)?,
        ),
        "compose" => TheoryBody::Composition(
            serde_json::from_value::<CompositionBody>(value.clone()).map_err(deserialized)?,
        ),
        "protocol" => TheoryBody::Protocol(Box::new(
            serde_json::from_value::<ProtocolSpec>(value.clone()).map_err(deserialized)?,
        )),
        "bundle" => TheoryBody::Bundle(Box::new(
            serde_json::from_value::<BundleSpec>(value.clone()).map_err(deserialized)?,
        )),
        "class" => TheoryBody::Class(
            serde_json::from_value::<ClassSpec>(value.clone()).map_err(deserialized)?,
        ),
        "instance" => TheoryBody::Instance(
            serde_json::from_value::<InstanceSpec>(value.clone()).map_err(deserialized)?,
        ),
        _ => TheoryBody::Inductive(
            serde_json::from_value::<InductiveSpec>(value.clone()).map_err(deserialized)?,
        ),
    };
    Ok(TheoryDocument {
        id: text("id"),
        description: text("description"),
        body,
    })
}
