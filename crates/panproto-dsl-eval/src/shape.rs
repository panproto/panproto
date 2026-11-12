//! A grammar for document values, checked before a value is deserialized.
//!
//! A declarative document arrives as a JSON-like value: parsed JSON, a YAML
//! document, or an evaluated Nickel record. Each DSL states its grammar as a
//! table of [`Shape`]s, and [`check`] refuses the first value that breaks
//! it, naming its [`DocumentPath`].
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

use serde_json::Value;

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
    /// A number.
    Number,
    /// A whole number from zero to `2^32 - 1`.
    Natural,
    /// A string or an object.
    StringOrObject,
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
    /// A string names no variant the grammar admits at this place, or a
    /// member of a variant object names no variant (the path then ends at
    /// that member).
    UnknownVariant {
        /// The string or member name given.
        found: String,
    },
    /// A variant object names no variant: it has no member.
    NoVariant,
    /// A variant object names more than one variant.
    SeveralVariants {
        /// The variant names present, in grammar order.
        keys: Vec<String>,
    },
    /// The document carries none of its grammar's body keys.
    NoBody,
    /// The document carries more than one body key.
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
            Self::NoVariant => f.write_str("the object names no variant"),
            Self::SeveralVariants { keys } => {
                write!(f, "the object names several variants: {}", keys.join(", "))
            }
            Self::NoBody => f.write_str("the document has no body"),
            Self::SeveralBodies { keys } => {
                write!(f, "the document has several bodies: {}", keys.join(", "))
            }
        }
    }
}

/// A refusal by the grammar: the path of the refused value (for a missing
/// field, of the object that lacks it) and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeError {
    /// The path of the refused value.
    pub path: DocumentPath,
    /// What is wrong with it.
    pub problem: ShapeProblem,
}

/// One member of an object's field table.
pub struct Field {
    /// The member name.
    pub name: &'static str,
    /// Whether the member must be present.
    pub required: bool,
    /// The grammar of the member's value.
    pub shape: &'static Shape,
}

/// A required member.
#[must_use]
pub const fn req(name: &'static str, shape: &'static Shape) -> Field {
    Field {
        name,
        required: true,
        shape,
    }
}

/// An optional member.
#[must_use]
pub const fn opt(name: &'static str, shape: &'static Shape) -> Field {
    Field {
        name,
        required: false,
        shape,
    }
}

/// The grammar of one value.
pub enum Shape {
    /// Any JSON value.
    Any,
    /// A string.
    Str,
    /// A boolean.
    Bool,
    /// A number.
    Number,
    /// A whole number from zero to `2^32 - 1`.
    Natural,
    /// `null`, or a value of the inner shape.
    Nullable(&'static Self),
    /// An array whose every element has this shape.
    List(&'static Self),
    /// An object whose every member has this shape.
    Map(&'static Self),
    /// An object with this field table.
    Record(&'static [Field]),
    /// An object whose `tag` member selects one of these field tables.
    Tagged {
        /// The selecting member.
        tag: &'static str,
        /// Each variant name with its field table (the tag excluded).
        variants: &'static [(&'static str, &'static [Field])],
    },
    /// An object with exactly one member, whose name selects the variant
    /// and whose value has that variant's shape.
    Keyed(&'static [(&'static str, &'static Self)]),
    /// One of these strings.
    Choice(&'static [&'static str]),
    /// A string, or an object of the inner shape.
    StrOr(&'static Self),
    /// An object read with the `present` field table when it carries the
    /// member `member` and with the `absent` table otherwise.
    Switch {
        /// The deciding member.
        member: &'static str,
        /// The table when the member is present.
        present: &'static [Field],
        /// The table when it is absent.
        absent: &'static [Field],
    },
}

/// A string shape, for building tables.
pub static STR: Shape = Shape::Str;
/// A boolean shape, for building tables.
pub static BOOL: Shape = Shape::Bool;
/// A number shape, for building tables.
pub static NUMBER: Shape = Shape::Number;
/// A natural-number shape, for building tables.
pub static NATURAL: Shape = Shape::Natural;
/// The shape of any value, for building tables.
pub static ANY: Shape = Shape::Any;
/// A list of strings.
pub static STRS: Shape = Shape::List(&STR);
/// A map from names to strings.
pub static STR_MAP: Shape = Shape::Map(&STR);

const fn refuse(path: DocumentPath, problem: ShapeProblem) -> ShapeError {
    ShapeError { path, problem }
}

fn wrong(path: &DocumentPath, expected: Expected) -> Result<(), ShapeError> {
    Err(refuse(path.clone(), ShapeProblem::WrongType { expected }))
}

/// Check `value`, found at `path`, against `shape`.
///
/// # Errors
///
/// The first value the grammar refuses, in the order the module
/// documentation states.
pub fn check(value: &Value, shape: &Shape, path: &DocumentPath) -> Result<(), ShapeError> {
    match shape {
        Shape::Any => Ok(()),
        Shape::Str if value.is_string() => Ok(()),
        Shape::Str => wrong(path, Expected::String),
        Shape::Bool if value.is_boolean() => Ok(()),
        Shape::Bool => wrong(path, Expected::Boolean),
        Shape::Number if value.is_number() => Ok(()),
        Shape::Number => wrong(path, Expected::Number),
        Shape::Natural if value.as_u64().is_some_and(|n| u32::try_from(n).is_ok()) => Ok(()),
        Shape::Natural => wrong(path, Expected::Natural),
        Shape::Nullable(_) if value.is_null() => Ok(()),
        Shape::Nullable(inner) => check(value, inner, path),
        Shape::List(element) => {
            let Some(items) = value.as_array() else {
                return wrong(path, Expected::Array);
            };
            items
                .iter()
                .enumerate()
                .try_for_each(|(index, item)| check(item, element, &path.index(index)))
        }
        Shape::Map(member) => {
            let Some(members) = value.as_object() else {
                return wrong(path, Expected::Object);
            };
            let mut entries: Vec<(&String, &Value)> = members.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            entries
                .into_iter()
                .try_for_each(|(key, value)| check(value, member, &path.key(key)))
        }
        Shape::Record(fields) => check_record(value, fields, &[], path),
        Shape::Tagged { tag, variants } => check_tagged(value, tag, variants, path),
        Shape::Keyed(variants) => check_keyed(value, variants, path),
        Shape::Choice(choices) => {
            let Some(found) = value.as_str() else {
                return wrong(path, Expected::String);
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
        Shape::StrOr(inner) => match value {
            Value::String(_) => Ok(()),
            Value::Object(_) => check(value, inner, path),
            _ => wrong(path, Expected::StringOrObject),
        },
        Shape::Switch {
            member,
            present,
            absent,
        } => {
            let Some(members) = value.as_object() else {
                return wrong(path, Expected::Object);
            };
            let fields = if members.contains_key(*member) {
                present
            } else {
                absent
            };
            check_record(value, fields, &[], path)
        }
    }
}

fn check_tagged(
    value: &Value,
    tag: &'static str,
    variants: &[(&'static str, &'static [Field])],
    path: &DocumentPath,
) -> Result<(), ShapeError> {
    let Some(members) = value.as_object() else {
        return wrong(path, Expected::Object);
    };
    let Some(found) = members.get(tag) else {
        return Err(refuse(
            path.clone(),
            ShapeProblem::MissingField {
                field: tag.to_owned(),
            },
        ));
    };
    let Some(found) = found.as_str() else {
        return wrong(&path.key(tag), Expected::String);
    };
    let Some((_, fields)) = variants.iter().find(|(name, _)| *name == found) else {
        return Err(refuse(
            path.key(tag),
            ShapeProblem::UnknownVariant {
                found: found.to_owned(),
            },
        ));
    };
    let leading = [req(tag, &STR)];
    check_record(value, fields, &leading, path)
}

fn check_keyed(
    value: &Value,
    variants: &[(&'static str, &'static Shape)],
    path: &DocumentPath,
) -> Result<(), ShapeError> {
    let Some(members) = value.as_object() else {
        return wrong(path, Expected::Object);
    };
    if let Some(unknown) = members
        .keys()
        .filter(|key| !variants.iter().any(|(name, _)| *name == key.as_str()))
        .min()
    {
        return Err(refuse(
            path.key(unknown),
            ShapeProblem::UnknownVariant {
                found: unknown.clone(),
            },
        ));
    }
    let present: Vec<&(&str, &Shape)> = variants
        .iter()
        .filter(|(name, _)| members.contains_key(*name))
        .collect();
    match present.as_slice() {
        [] => Err(refuse(path.clone(), ShapeProblem::NoVariant)),
        [(name, shape)] => members
            .get(*name)
            .map_or(Ok(()), |member| check(member, shape, &path.key(name))),
        several => Err(refuse(
            path.clone(),
            ShapeProblem::SeveralVariants {
                keys: several.iter().map(|(name, _)| (*name).to_owned()).collect(),
            },
        )),
    }
}

/// Check an object against `leading` and then `fields`: the least member
/// name neither table names is refused, then each field in table order.
///
/// # Errors
///
/// The first value refused, in the order the module documentation states.
pub fn check_record(
    value: &Value,
    fields: &[Field],
    leading: &[Field],
    path: &DocumentPath,
) -> Result<(), ShapeError> {
    let Some(members) = value.as_object() else {
        return wrong(path, Expected::Object);
    };
    let table = || leading.iter().chain(fields);
    if let Some(unknown) = members
        .keys()
        .filter(|key| !table().any(|field| field.name == key.as_str()))
        .min()
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

/// The single body key a document object carries.
///
/// `bodies` lists the body keys in grammar order; `members` is the
/// document object.
///
/// # Errors
///
/// [`ShapeProblem::NoBody`] or [`ShapeProblem::SeveralBodies`] at the
/// document root.
pub fn single_body<'a>(
    members: &serde_json::Map<String, Value>,
    bodies: impl IntoIterator<Item = &'a str>,
) -> Result<&'a str, ShapeError> {
    let present: Vec<&str> = bodies
        .into_iter()
        .filter(|key| members.contains_key(*key))
        .collect();
    match present.as_slice() {
        [] => Err(refuse(DocumentPath::root(), ShapeProblem::NoBody)),
        [one] => Ok(one),
        several => Err(refuse(
            DocumentPath::root(),
            ShapeProblem::SeveralBodies {
                keys: several.iter().map(|key| (*key).to_owned()).collect(),
            },
        )),
    }
}
