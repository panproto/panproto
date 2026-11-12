//! The lens document grammar, checked on a document value before it is read.
//!
//! A lens document arrives as a JSON-like value: parsed JSON, a YAML
//! document, or an evaluated Nickel record. [`read_document`] checks the
//! value against the grammar below and refuses the first value that breaks
//! it, naming its [`DocumentPath`], before the value is deserialized into a
//! [`LensDocument`]. The visiting order is the one
//! [`panproto_dsl_eval::shape`] states.
//!
//! The grammar is closed: no object carries a member it does not name, and
//! no member has a second spelling. A document carries `id`, `source` and
//! `target` strings, an optional `description`, exactly one body key
//! (`steps`, `rules`, `compose`, `auto`, `from_diff`, `symmetric`), and the
//! body's own members. `directed_equations` accompanies every body except
//! `symmetric`; `passthrough` belongs to the `rules` body alone; `invertible`
//! and `extensions` accompany every body. A step, and a member of a
//! `compose` body's `lenses`, is an object with exactly one member, whose
//! name selects the variant. A value kind (`coerce_sort` and directed
//! equation kinds) is one of the kinds the expression language knows.

use panproto_dsl_eval::shape::{
    ANY, BOOL, Field, NATURAL, NUMBER, STR, STR_MAP, STRS, Shape, ShapeError, check_record, opt,
    req, single_body,
};
pub use panproto_dsl_eval::shape::{Expected, ShapeProblem};
use serde_json::Value;

use crate::document::LensDocument;
use crate::error::LensDslError;
use crate::path::DocumentPath;

/// The value kinds a coercion or directed equation may name.
pub const VALUE_KINDS: &[&str] = &[
    "boolean", "bool", "integer", "int", "float", "number", "string", "str", "bytes", "token",
    "null", "any",
];

static VALUE_KIND: Shape = Shape::Choice(VALUE_KINDS);
static COERCION: Shape = Shape::Choice(&["iso", "retraction", "projection", "opaque"]);
static RENAME: Shape = Shape::Record(&[req("old", &STR), req("new", &STR)]);

static ADD_FIELD: Shape = Shape::Record(&[
    req("name", &STR),
    req("kind", &STR),
    opt("default", &ANY),
    opt("expr", &STR),
]);
static APPLY_EXPR: Shape = Shape::Record(&[
    req("field", &STR),
    req("expr", &STR),
    opt("inverse", &STR),
    opt("coercion", &COERCION),
]);
static COMPUTE_FIELD: Shape = Shape::Record(&[
    req("target", &STR),
    req("expr", &STR),
    opt("inverse", &STR),
    opt("coercion", &COERCION),
]);
static HOIST: Shape = Shape::Record(&[
    req("parent", &STR),
    req("intermediate", &STR),
    req("child", &STR),
]);
static NEST: Shape = Shape::Record(&[
    req("parent", &STR),
    req("child", &STR),
    req("intermediate", &STR),
    req("intermediate_kind", &STR),
    req("edge_kind", &STR),
    opt("old_edge_name", &STR),
    opt("parent_to_intermediate", &STR),
    opt("intermediate_to_child", &STR),
]);
static SCOPED: Shape = Shape::Record(&[req("focus", &STR), req("inner", &STEPS)]);
static PULLBACK: Shape = Shape::Record(&[
    req("name", &STR),
    req("domain", &STR),
    req("codomain", &STR),
    opt("sort_map", &STR_MAP),
    opt("op_map", &STR_MAP),
]);
static COERCE_SORT: Shape = Shape::Record(&[
    req("sort", &STR),
    opt("source_kind", &VALUE_KIND),
    req("target_kind", &VALUE_KIND),
    req("expr", &STR),
    opt("inverse", &STR),
    req("coercion", &COERCION),
]);
static MERGE_SORTS: Shape = Shape::Record(&[
    req("sort_a", &STR),
    req("sort_b", &STR),
    req("merged", &STR),
    req("expr", &STR),
]);
static ADD_SORT: Shape =
    Shape::Record(&[req("name", &STR), req("kind", &STR), opt("default", &ANY)]);
static ADD_OP: Shape = Shape::Record(&[
    req("name", &STR),
    req("src", &STR),
    req("tgt", &STR),
    req("kind", &STR),
]);
static EQUATION: Shape = Shape::Record(&[req("name", &STR), req("lhs", &STR), req("rhs", &STR)]);

/// The step variants, in grammar order.
pub static STEP_VARIANTS: [(&str, &Shape); 19] = [
    ("remove_field", &STR),
    ("rename_field", &RENAME),
    ("add_field", &ADD_FIELD),
    ("apply_expr", &APPLY_EXPR),
    ("compute_field", &COMPUTE_FIELD),
    ("hoist_field", &HOIST),
    ("nest_field", &NEST),
    ("scoped", &SCOPED),
    ("pullback", &PULLBACK),
    ("coerce_sort", &COERCE_SORT),
    ("merge_sorts", &MERGE_SORTS),
    ("add_sort", &ADD_SORT),
    ("drop_sort", &STR),
    ("rename_sort", &RENAME),
    ("add_op", &ADD_OP),
    ("drop_op", &STR),
    ("rename_op", &RENAME),
    ("add_equation", &EQUATION),
    ("drop_equation", &STR),
];
static STEP: Shape = Shape::Keyed(&STEP_VARIANTS);
static STEPS: Shape = Shape::List(&STEP);

static PATTERN: Shape = Shape::Record(&[req("name", &STR)]);
static TEMPLATE: Shape = Shape::Record(&[req("template", &STR)]);
static REPLACEMENT_NAME: Shape = Shape::StrOr(&TEMPLATE);
static ATTR_OP: Shape = Shape::Tagged {
    tag: "op",
    variants: &[
        ("add", &[req("value", &NUMBER)]),
        ("subtract", &[req("value", &NUMBER)]),
        ("multiply", &[req("value", &NUMBER)]),
        ("prefix", &[req("value", &STR)]),
        ("suffix", &[req("value", &STR)]),
        ("negate", &[]),
        ("to-string", &[]),
        ("to-number", &[]),
        ("to-boolean", &[]),
    ],
};
static ATTR_OPS: Shape = Shape::Map(&ATTR_OP);
static ANY_MAP: Shape = Shape::Map(&ANY);
static REPLACEMENT: Shape = Shape::Record(&[
    opt("name", &REPLACEMENT_NAME),
    opt("rename_attrs", &STR_MAP),
    opt("add_attrs", &ANY_MAP),
    opt("drop_attrs", &STRS),
    opt("keep_attrs", &STRS),
    opt("map_attr_value", &ATTR_OPS),
]);
static OPT_REPLACEMENT: Shape = Shape::Nullable(&REPLACEMENT);
static RULE: Shape = Shape::Record(&[req("pattern", &PATTERN), opt("replace", &OPT_REPLACEMENT)]);
static RULES: Shape = Shape::List(&RULE);
static PASSTHROUGH: Shape = Shape::Choice(&["keep", "drop"]);

static INLINE: Shape = Shape::Record(&[req("steps", &STEPS)]);
static LENS_REF: Shape = Shape::Keyed(&[("ref", &STR), ("inline", &INLINE)]);
static LENS_REFS: Shape = Shape::List(&LENS_REF);
static MODE: Shape = Shape::Choice(&["vertical", "horizontal"]);
static COMPOSE: Shape = Shape::Record(&[req("mode", &MODE), req("lenses", &LENS_REFS)]);

static PREDICATE: Shape = Shape::Tagged {
    tag: "kind",
    variants: &[
        ("same_edge_name", &[]),
        ("similar_name", &[req("threshold", &NUMBER)]),
        ("same_kind", &[]),
    ],
};
static CONSTRAINT: Shape = Shape::Tagged {
    tag: "type",
    variants: &[
        ("scope", &[req("under", &STR), req("targets", &STR)]),
        ("exclude_targets", &[req("vertices", &STRS)]),
        ("exclude_sources", &[req("vertices", &STRS)]),
        (
            "prefer",
            &[req("predicate", &PREDICATE), req("weight", &NUMBER)],
        ),
    ],
};
static CONSTRAINTS: Shape = Shape::List(&CONSTRAINT);
static STRINGENCY: Shape = Shape::Choice(&["strict", "balanced", "lenient", "exploratory"]);
static CLUSTERS: Shape = Shape::List(&STRS);
static HINTS: Shape = Shape::Record(&[
    opt("anchors", &STR_MAP),
    opt("constraints", &CONSTRAINTS),
    opt("stringency", &STRINGENCY),
    opt("alias_clusters", &CLUSTERS),
]);
static AUTO: Shape = Shape::Record(&[
    opt("quality_threshold", &NUMBER),
    opt("enable_overlap", &BOOL),
    opt("max_results", &NATURAL),
    opt("hints", &HINTS),
]);
static FROM_DIFF: Shape = Shape::Record(&[]);
static SYMMETRIC: Shape = Shape::Record(&[
    req("left", &STEPS),
    req("right", &STEPS),
    opt("focus", &STR),
]);

static DIRECTED: Shape = Shape::Record(&[
    req("name", &STR),
    req("lhs", &STR),
    req("rhs", &STR),
    req("impl", &STR),
    opt("inverse", &STR),
    opt("source_kind", &VALUE_KIND),
    opt("target_kind", &VALUE_KIND),
    opt("coercion", &COERCION),
]);
static DIRECTEDS: Shape = Shape::List(&DIRECTED);

/// The members every document carries, before its body's members.
const HEADER: &[Field] = &[
    req("id", &STR),
    opt("description", &STR),
    req("source", &STR),
    req("target", &STR),
];

/// The body keys, in grammar order, with each body's field table.
const BODIES: &[(&str, &[Field])] = &[
    (
        "steps",
        &[
            req("steps", &STEPS),
            opt("directed_equations", &DIRECTEDS),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
    (
        "rules",
        &[
            req("rules", &RULES),
            opt("passthrough", &PASSTHROUGH),
            opt("directed_equations", &DIRECTEDS),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
    (
        "compose",
        &[
            req("compose", &COMPOSE),
            opt("directed_equations", &DIRECTEDS),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
    (
        "auto",
        &[
            req("auto", &AUTO),
            opt("directed_equations", &DIRECTEDS),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
    (
        "from_diff",
        &[
            req("from_diff", &FROM_DIFF),
            opt("directed_equations", &DIRECTEDS),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
    (
        "symmetric",
        &[
            req("symmetric", &SYMMETRIC),
            opt("invertible", &BOOL),
            opt("extensions", &ANY_MAP),
        ],
    ),
];

/// The body keys, in grammar order.
pub fn body_keys() -> impl Iterator<Item = &'static str> {
    BODIES.iter().map(|(key, _)| *key)
}

fn refuse(error: ShapeError) -> LensDslError {
    LensDslError::Shape {
        path: error.path,
        problem: error.problem,
    }
}

/// Check a document value against the grammar.
///
/// # Errors
///
/// [`LensDslError::Shape`] naming the path of the first value the grammar
/// refuses: a root that is not an object, a missing or second body key
/// (at the root), and then the members in the order the module
/// documentation states, the header first.
pub fn check_document(value: &Value) -> Result<(), LensDslError> {
    let root = DocumentPath::root();
    let Some(members) = value.as_object() else {
        return Err(LensDslError::Shape {
            path: root,
            problem: ShapeProblem::WrongType {
                expected: Expected::Object,
            },
        });
    };
    let body = single_body(members, body_keys()).map_err(refuse)?;
    let fields = BODIES
        .iter()
        .find(|(key, _)| *key == body)
        .map_or(&[][..], |(_, fields)| *fields);
    check_record(value, fields, HEADER, &root).map_err(refuse)
}

/// Read a document value into a [`LensDocument`].
///
/// # Errors
///
/// The refusals of [`check_document`], and [`LensDslError::Json`] should a
/// value the grammar admits fail to deserialize.
pub fn read_document(value: &Value) -> Result<LensDocument, LensDslError> {
    check_document(value)?;
    Ok(serde_json::from_value(value.clone())?)
}
