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
//! The check visits a value in a fixed order. An object is checked first
//! for a member the grammar does not name (the first such member in the
//! object's order), then field by field in the order of its field table:
//! a required field that is absent is refused, and a present one is checked
//! recursively before the next field. An array is checked element by
//! element.

use serde_json::Value;

use crate::document::{
    BundleSpec, ClassSpec, CompositionBody, InductiveSpec, InstanceSpec, MorphismSpec,
    ProtocolSpec, TheoryBody, TheoryDocument, TheorySpec,
};
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

/// What a refused value was expected to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Expected {
    /// An object.
    Object,
    /// An array.
    Array,
    /// A string.
    String,
    /// A boolean.
    Boolean,
    /// A theory name, an inline theory or an inline composition.
    TheoryReference,
}

/// Why a document value is refused by the grammar.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShapeProblem {
    /// The value has the wrong JSON type.
    WrongType {
        /// What the grammar expects at this place.
        expected: Expected,
    },
    /// An object carries a member the grammar does not name; the path ends
    /// at that member.
    UnknownField,
    /// A required member is absent; the path ends at the object.
    MissingField {
        /// The absent member.
        field: String,
    },
    /// A string names no variant the grammar admits at this place (a sort
    /// kind, a strategy, a composition mode or an identification kind).
    UnknownVariant {
        /// The string given.
        found: String,
    },
    /// The document carries none of the body keys `theory`, `morphism`,
    /// `compose`, `protocol`, `bundle`, `class`, `instance`, `inductive`.
    NoBody,
    /// The document carries more than one body key. A `class` member of an
    /// `instance` document is that instance's field and not a body key.
    SeveralBodies {
        /// The body keys present, in grammar order.
        keys: Vec<String>,
    },
}

impl std::fmt::Display for ShapeProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongType { expected } => write!(f, "expected {expected:?}"),
            Self::UnknownField => f.write_str("unknown field"),
            Self::MissingField { field } => write!(f, "missing field `{field}`"),
            Self::UnknownVariant { found } => write!(f, "unknown variant `{found}`"),
            Self::NoBody => f.write_str("the document has no body"),
            Self::SeveralBodies { keys } => {
                write!(f, "the document has several bodies: {}", keys.join(", "))
            }
        }
    }
}

/// One member of an object's field table.
struct Field {
    name: &'static str,
    required: bool,
    shape: &'static Shape,
}

const fn req(name: &'static str, shape: &'static Shape) -> Field {
    Field {
        name,
        required: true,
        shape,
    }
}

const fn opt(name: &'static str, shape: &'static Shape) -> Field {
    Field {
        name,
        required: false,
        shape,
    }
}

/// The grammar of one value.
enum Shape {
    Str,
    Bool,
    /// A string or `null`.
    OptStr,
    List(&'static Self),
    /// An array or `null`.
    OptList(&'static Self),
    /// An object whose every member has this shape.
    Map(&'static Self),
    Record(&'static [Field]),
    /// An object whose `type` member selects one of these field tables.
    Tagged(&'static [(&'static str, &'static [Field])]),
    /// One of these strings.
    Choice(&'static [&'static str]),
    /// A theory name, an inline theory (an object with a `theory` member)
    /// or an inline composition.
    TheoryRef,
}

static STR: Shape = Shape::Str;
static BOOL: Shape = Shape::Bool;
static OPT_STR: Shape = Shape::OptStr;
static STRS: Shape = Shape::List(&STR);
static OPT_STRS: Shape = Shape::OptList(&STR);
static STR_MAP: Shape = Shape::Map(&STR);

static PARAM: Shape =
    Shape::Record(&[req("name", &STR), req("sort", &STR), opt("implicit", &BOOL)]);
static PARAMS: Shape = Shape::List(&PARAM);
static SORT_KIND: Shape = Shape::Tagged(&[
    ("structural", &[]),
    ("val", &[req("value_kind", &STR)]),
    (
        "coercion",
        &[req("from", &STR), req("to", &STR), req("class", &STR)],
    ),
    ("merger", &[req("value_kind", &STR)]),
]);
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
static STRATEGY: Shape = Shape::Tagged(&[
    ("keep_left", &[]),
    ("keep_right", &[]),
    ("fail", &[]),
    ("custom", &[req("expr", &STR)]),
]);
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

static OPT_PARAMS: Shape = Shape::OptList(&PARAM);
static IMPORTS: Shape = Shape::List(&IMPORT);
static SORTS: Shape = Shape::List(&SORT);
static DIRECTEDS: Shape = Shape::List(&DIRECTED);
static POLICIES: Shape = Shape::List(&POLICY);
static IDENTIFY_KIND: Shape = Shape::Choice(&["sort", "op"]);
static MODE: Shape = Shape::Choice(&["amalgamate", "explicit_identity"]);
static IDENTIFIES: Shape = Shape::List(&IDENTIFY);
static STEPS: Shape = Shape::List(&STEP);
static THEORY_REF: Shape = Shape::TheoryRef;
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

const fn refuse(path: DocumentPath, problem: ShapeProblem) -> TheoryDslError {
    TheoryDslError::Shape { path, problem }
}

fn check(value: &Value, shape: &Shape, path: &DocumentPath) -> Result<(), TheoryDslError> {
    let wrong = |expected| Err(refuse(path.clone(), ShapeProblem::WrongType { expected }));
    match shape {
        Shape::Str => {
            if value.is_string() {
                Ok(())
            } else {
                wrong(Expected::String)
            }
        }
        Shape::OptStr => {
            if value.is_string() || value.is_null() {
                Ok(())
            } else {
                wrong(Expected::String)
            }
        }
        Shape::Bool => {
            if value.is_boolean() {
                Ok(())
            } else {
                wrong(Expected::Boolean)
            }
        }
        Shape::List(element) => check_list(value, element, path),
        Shape::OptList(element) => {
            if value.is_null() {
                Ok(())
            } else {
                check_list(value, element, path)
            }
        }
        Shape::Map(member) => {
            let Some(members) = value.as_object() else {
                return wrong(Expected::Object);
            };
            members
                .iter()
                .try_for_each(|(key, value)| check(value, member, &path.key(key)))
        }
        Shape::Record(fields) => check_record(value, fields, &[], path),
        Shape::Tagged(variants) => {
            let Some(members) = value.as_object() else {
                return wrong(Expected::Object);
            };
            let Some(tag) = members.get("type") else {
                return Err(refuse(
                    path.clone(),
                    ShapeProblem::MissingField {
                        field: "type".to_owned(),
                    },
                ));
            };
            let Some(tag) = tag.as_str() else {
                return Err(refuse(
                    path.key("type"),
                    ShapeProblem::WrongType {
                        expected: Expected::String,
                    },
                ));
            };
            let Some((_, fields)) = variants.iter().find(|(name, _)| *name == tag) else {
                return Err(refuse(
                    path.key("type"),
                    ShapeProblem::UnknownVariant {
                        found: tag.to_owned(),
                    },
                ));
            };
            check_record(value, fields, &[req("type", &STR)], path)
        }
        Shape::Choice(choices) => {
            let Some(found) = value.as_str() else {
                return wrong(Expected::String);
            };
            if choices.contains(&found) {
                Ok(())
            } else {
                Err(refuse(
                    path.clone(),
                    ShapeProblem::UnknownVariant {
                        found: found.to_owned(),
                    },
                ))
            }
        }
        Shape::TheoryRef => match value {
            Value::String(_) => Ok(()),
            Value::Object(members) if members.contains_key("theory") => {
                check_record(value, THEORY_FIELDS, &[], path)
            }
            Value::Object(_) => check_record(value, COMPOSITION_FIELDS, &[], path),
            _ => wrong(Expected::TheoryReference),
        },
    }
}

fn check_list(value: &Value, element: &Shape, path: &DocumentPath) -> Result<(), TheoryDslError> {
    let Some(items) = value.as_array() else {
        return Err(refuse(
            path.clone(),
            ShapeProblem::WrongType {
                expected: Expected::Array,
            },
        ));
    };
    items
        .iter()
        .enumerate()
        .try_for_each(|(index, item)| check(item, element, &path.index(index)))
}

/// Check an object against `leading` and then `fields`: the first member
/// neither table names is refused, then each field in table order.
fn check_record(
    value: &Value,
    fields: &[Field],
    leading: &[Field],
    path: &DocumentPath,
) -> Result<(), TheoryDslError> {
    let Some(members) = value.as_object() else {
        return Err(refuse(
            path.clone(),
            ShapeProblem::WrongType {
                expected: Expected::Object,
            },
        ));
    };
    let table = || leading.iter().chain(fields);
    if let Some(unknown) = members
        .keys()
        .find(|key| !table().any(|field| field.name == key.as_str()))
    {
        return Err(refuse(path.key(unknown), ShapeProblem::UnknownField));
    }
    for field in table() {
        match members.get(field.name) {
            Some(member) => check(member, field.shape, &path.key(field.name))?,
            None if field.required => {
                return Err(refuse(
                    path.clone(),
                    ShapeProblem::MissingField {
                        field: field.name.to_owned(),
                    },
                ));
            }
            None => {}
        }
    }
    Ok(())
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
        return Err(refuse(
            root,
            ShapeProblem::WrongType {
                expected: Expected::Object,
            },
        ));
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
        [] => return Err(refuse(root, ShapeProblem::NoBody)),
        [one] => *one,
        several => {
            return Err(refuse(
                root,
                ShapeProblem::SeveralBodies {
                    keys: several.iter().map(|key| (*key).to_owned()).collect(),
                },
            ));
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
    )?;
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
