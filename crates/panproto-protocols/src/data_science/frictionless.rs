//! Frictionless Table Schema descriptors.
//!
//! This module reads a Table Schema descriptor as the Data Package
//! standard v2.0 defines it (<https://datapackage.org/standard/table-schema/>),
//! including the v1.0 forms the standard requires a consumer to accept: a
//! `primaryKey` or foreign-key `fields` written as one string, a
//! foreign-key `resource` of `""` for a reference into the same table, a
//! `format` with the `fmt:` prefix, and field names that repeat.
//!
//! [`parse_table_schema`](crate::data_science::frictionless::parse_table_schema) returns a [`TableSchema`](crate::data_science::frictionless::TableSchema) that keeps every
//! property of the descriptor: the standard ones typed, every other one
//! as written in an `extra` map, so [`TableSchema::to_json`](crate::data_science::frictionless::TableSchema::to_json) writes a
//! descriptor that reads back as the same schema. Every rule the standard
//! states with `MUST` is checked, and a descriptor that breaks one is
//! refused with every [`TableSchemaError`](crate::data_science::frictionless::TableSchemaError) it holds, each naming the JSON
//! pointer of the offending property, in a fixed order: the fields in
//! order, then the schema properties, and the members of one object in the
//! order the JSON value holds them. A constraint the
//! standard does not define, or one it defines for other field types, is
//! refused rather than ignored, since the standard asks an implementation
//! to report a constraint it cannot evaluate.
//!
//! Reading the values of a table under a schema (casting a lexical value
//! with a field's type, format and missing values, and checking its
//! constraints) is not this module's task: it reads the descriptor.

use serde_json::{Map, Value};

/// The type of a field (Table Schema v2.0, Field Types).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldType {
    /// `string`.
    String,
    /// `number`.
    Number,
    /// `integer`.
    Integer,
    /// `boolean`.
    Boolean,
    /// `object`.
    Object,
    /// `array`.
    Array,
    /// `list`.
    List,
    /// `datetime`.
    Datetime,
    /// `date`.
    Date,
    /// `time`.
    Time,
    /// `year`.
    Year,
    /// `yearmonth`.
    Yearmonth,
    /// `duration`.
    Duration,
    /// `geopoint`.
    Geopoint,
    /// `geojson`.
    Geojson,
    /// `any`, also the type of a field that names none.
    Any,
}

impl FieldType {
    /// The type a descriptor names, if the standard defines it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "string" => Self::String,
            "number" => Self::Number,
            "integer" => Self::Integer,
            "boolean" => Self::Boolean,
            "object" => Self::Object,
            "array" => Self::Array,
            "list" => Self::List,
            "datetime" => Self::Datetime,
            "date" => Self::Date,
            "time" => Self::Time,
            "year" => Self::Year,
            "yearmonth" => Self::Yearmonth,
            "duration" => Self::Duration,
            "geopoint" => Self::Geopoint,
            "geojson" => Self::Geojson,
            "any" => Self::Any,
            _ => return None,
        })
    }

    /// The name a descriptor writes for this type.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Object => "object",
            Self::Array => "array",
            Self::List => "list",
            Self::Datetime => "datetime",
            Self::Date => "date",
            Self::Time => "time",
            Self::Year => "year",
            Self::Yearmonth => "yearmonth",
            Self::Duration => "duration",
            Self::Geopoint => "geopoint",
            Self::Geojson => "geojson",
            Self::Any => "any",
        }
    }

    /// Whether `format` is a format this type admits. A temporal type
    /// admits `default`, `any`, and any other string as a `strptime`
    /// pattern.
    #[must_use]
    pub fn admits_format(self, format: &str) -> bool {
        match self {
            Self::String => matches!(format, "default" | "email" | "uri" | "binary" | "uuid"),
            Self::Datetime | Self::Date | Self::Time => true,
            Self::Geopoint => matches!(format, "default" | "array" | "object"),
            Self::Geojson => matches!(format, "default" | "topojson"),
            _ => format == "default",
        }
    }

    /// Whether the length constraints apply to this type.
    const fn has_length(self) -> bool {
        matches!(self, Self::String | Self::Array | Self::Object | Self::List)
    }

    /// Whether the range constraints apply to this type.
    const fn is_ordered(self) -> bool {
        matches!(
            self,
            Self::Integer
                | Self::Number
                | Self::Date
                | Self::Time
                | Self::Datetime
                | Self::Duration
                | Self::Year
                | Self::Yearmonth
        )
    }
}

/// The item types a `list` field admits.
const LIST_ITEM_TYPES: [FieldType; 7] = [
    FieldType::String,
    FieldType::Integer,
    FieldType::Boolean,
    FieldType::Number,
    FieldType::Datetime,
    FieldType::Date,
    FieldType::Time,
];

/// A schema's or a field's `missingValues`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingValues {
    /// An array of unique strings.
    Strings(Vec<String>),
    /// An array of objects, each a unique `value` with an optional unique
    /// `label`.
    Labelled(Vec<(String, Option<String>)>),
}

impl MissingValues {
    /// The missing-value strings, labelled or not.
    #[must_use]
    pub fn values(&self) -> Vec<&str> {
        match self {
            Self::Strings(values) => values.iter().map(String::as_str).collect(),
            Self::Labelled(values) => values.iter().map(|(value, _)| value.as_str()).collect(),
        }
    }
}

/// A field's `categories`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Categories {
    /// An array of unique values.
    Values(Vec<Value>),
    /// An array of objects, each a unique `value` with an optional unique
    /// `label`.
    Labelled(Vec<(Value, Option<String>)>),
}

impl Categories {
    /// The category values, labelled or not.
    #[must_use]
    pub fn values(&self) -> Vec<&Value> {
        match self {
            Self::Values(values) => values.iter().collect(),
            Self::Labelled(values) => values.iter().map(|(value, _)| value).collect(),
        }
    }
}

/// A field's `constraints`, each as written.
///
/// A range bound or an `enum` entry is kept as the JSON value the
/// descriptor gives: the standard lets it be a primitive or a string to be
/// cast under the field's type and format, which reading the table does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Constraints {
    /// `required`.
    pub required: Option<bool>,
    /// `unique`.
    pub unique: Option<bool>,
    /// `minLength`.
    pub min_length: Option<u64>,
    /// `maxLength`.
    pub max_length: Option<u64>,
    /// `minimum`.
    pub minimum: Option<Value>,
    /// `maximum`.
    pub maximum: Option<Value>,
    /// `exclusiveMinimum`.
    pub exclusive_minimum: Option<Value>,
    /// `exclusiveMaximum`.
    pub exclusive_maximum: Option<Value>,
    /// `pattern`, an XML Schema regular expression.
    pub pattern: Option<String>,
    /// `enum`.
    pub enumeration: Option<Vec<Value>>,
    /// `jsonSchema`.
    pub json_schema: Option<Map<String, Value>>,
}

/// The properties a field's type adds to its descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeOptions {
    /// `decimalChar` of a `number` field.
    pub decimal_char: Option<String>,
    /// `groupChar` of a `number` or `integer` field.
    pub group_char: Option<String>,
    /// `bareNumber` of a `number` or `integer` field.
    pub bare_number: Option<bool>,
    /// `trueValues` of a `boolean` field.
    pub true_values: Option<Vec<String>>,
    /// `falseValues` of a `boolean` field.
    pub false_values: Option<Vec<String>>,
    /// `delimiter` of a `list` field.
    pub delimiter: Option<String>,
    /// `itemType` of a `list` field.
    pub item_type: Option<FieldType>,
}

/// One field descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// `name`.
    pub name: String,
    /// `type`, `None` when the descriptor names none (the field is `any`).
    pub field_type: Option<FieldType>,
    /// `format`, with a `fmt:` prefix removed; `None` is `default`.
    pub format: Option<String>,
    /// `title`.
    pub title: Option<String>,
    /// `description`.
    pub description: Option<String>,
    /// `example`.
    pub example: Option<Value>,
    /// `constraints`, `None` when the descriptor has none.
    pub constraints: Option<Constraints>,
    /// `categories`.
    pub categories: Option<Categories>,
    /// `categoriesOrdered`.
    pub categories_ordered: Option<bool>,
    /// The field's own `missingValues`, replacing the schema's.
    pub missing_values: Option<MissingValues>,
    /// `rdfType`.
    pub rdf_type: Option<String>,
    /// The properties the field's type defines.
    pub options: TypeOptions,
    /// Every other property, as written.
    pub extra: Map<String, Value>,
}

impl Field {
    /// The field's type: `any` when it names none.
    #[must_use]
    pub fn effective_type(&self) -> FieldType {
        self.field_type.unwrap_or(FieldType::Any)
    }
}

/// One `foreignKeys` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignKey {
    /// The fields of this table.
    pub fields: Vec<String>,
    /// The referenced resource; `None` for a reference into this table.
    pub resource: Option<String>,
    /// The referenced fields.
    pub reference_fields: Vec<String>,
}

/// `fieldsMatch`: how a table's columns map onto the schema's fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldsMatch {
    /// The same fields, mapped by order (the default).
    Exact,
    /// The same fields, mapped by name.
    Equal,
    /// All the schema's fields and possibly more, mapped by name.
    Subset,
    /// Only the schema's fields, possibly fewer, mapped by name.
    Superset,
    /// At least one of the schema's fields, mapped by name.
    Partial,
}

impl FieldsMatch {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "exact" => Self::Exact,
            "equal" => Self::Equal,
            "subset" => Self::Subset,
            "superset" => Self::Superset,
            "partial" => Self::Partial,
            _ => return None,
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Equal => "equal",
            Self::Subset => "subset",
            Self::Superset => "superset",
            Self::Partial => "partial",
        }
    }
}

/// A Table Schema descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSchema {
    /// `fields`, in order.
    pub fields: Vec<Field>,
    /// `$schema`.
    pub profile: Option<String>,
    /// `fieldsMatch`, `None` when the descriptor leaves it at `exact`.
    pub fields_match: Option<FieldsMatch>,
    /// `missingValues`, `None` when the descriptor leaves it at `[""]`.
    pub missing_values: Option<MissingValues>,
    /// `primaryKey`.
    pub primary_key: Option<Vec<String>>,
    /// `uniqueKeys`.
    pub unique_keys: Option<Vec<Vec<String>>>,
    /// `foreignKeys`.
    pub foreign_keys: Option<Vec<ForeignKey>>,
    /// Every other property, as written.
    pub extra: Map<String, Value>,
}

impl TableSchema {
    /// The missing values that apply to the field at `index`: its own, or
    /// the schema's, or the default `[""]`.
    #[must_use]
    pub fn missing_values_of(&self, index: usize) -> Vec<&str> {
        self.fields
            .get(index)
            .and_then(|field| field.missing_values.as_ref())
            .or(self.missing_values.as_ref())
            .map_or_else(|| vec![""], MissingValues::values)
    }

    /// The descriptor in the v2.0 form: keys and foreign-key fields as
    /// arrays, a reference into this table with no `resource`, and every
    /// property the schema holds, standard ones first in the standard's
    /// order, then the others as read.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        if let Some(profile) = &self.profile {
            out.insert("$schema".into(), Value::String(profile.clone()));
        }
        out.insert(
            "fields".into(),
            Value::Array(self.fields.iter().map(field_json).collect()),
        );
        if let Some(fields_match) = self.fields_match {
            out.insert("fieldsMatch".into(), fields_match.name().into());
        }
        if let Some(missing) = &self.missing_values {
            out.insert("missingValues".into(), missing_json(missing));
        }
        if let Some(key) = &self.primary_key {
            out.insert("primaryKey".into(), strings_json(key));
        }
        if let Some(keys) = &self.unique_keys {
            out.insert(
                "uniqueKeys".into(),
                Value::Array(keys.iter().map(|key| strings_json(key)).collect()),
            );
        }
        if let Some(keys) = &self.foreign_keys {
            out.insert(
                "foreignKeys".into(),
                Value::Array(keys.iter().map(foreign_key_json).collect()),
            );
        }
        for (key, value) in &self.extra {
            out.insert(key.clone(), value.clone());
        }
        Value::Object(out)
    }
}

fn strings_json(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn missing_json(missing: &MissingValues) -> Value {
    match missing {
        MissingValues::Strings(values) => strings_json(values),
        MissingValues::Labelled(values) => Value::Array(
            values
                .iter()
                .map(|(value, label)| {
                    let mut entry = Map::new();
                    entry.insert("value".into(), Value::String(value.clone()));
                    if let Some(label) = label {
                        entry.insert("label".into(), Value::String(label.clone()));
                    }
                    Value::Object(entry)
                })
                .collect(),
        ),
    }
}

fn foreign_key_json(key: &ForeignKey) -> Value {
    let mut reference = Map::new();
    if let Some(resource) = &key.resource {
        reference.insert("resource".into(), Value::String(resource.clone()));
    }
    reference.insert("fields".into(), strings_json(&key.reference_fields));
    let mut out = Map::new();
    out.insert("fields".into(), strings_json(&key.fields));
    out.insert("reference".into(), Value::Object(reference));
    Value::Object(out)
}

fn field_json(field: &Field) -> Value {
    let mut out = Map::new();
    let mut put = |key: &str, value: Value| {
        out.insert(key.to_owned(), value);
    };
    put("name", field.name.clone().into());
    if let Some(field_type) = field.field_type {
        put("type", field_type.name().into());
    }
    if let Some(format) = &field.format {
        put("format", format.clone().into());
    }
    if let Some(title) = &field.title {
        put("title", title.clone().into());
    }
    if let Some(description) = &field.description {
        put("description", description.clone().into());
    }
    if let Some(example) = &field.example {
        put("example", example.clone());
    }
    let options = &field.options;
    if let Some(value) = &options.decimal_char {
        put("decimalChar", value.clone().into());
    }
    if let Some(value) = &options.group_char {
        put("groupChar", value.clone().into());
    }
    if let Some(value) = options.bare_number {
        put("bareNumber", value.into());
    }
    if let Some(values) = &options.true_values {
        put("trueValues", strings_json(values));
    }
    if let Some(values) = &options.false_values {
        put("falseValues", strings_json(values));
    }
    if let Some(value) = &options.delimiter {
        put("delimiter", value.clone().into());
    }
    if let Some(item_type) = options.item_type {
        put("itemType", item_type.name().into());
    }
    if let Some(categories) = &field.categories {
        put(
            "categories",
            match categories {
                Categories::Values(values) => Value::Array(values.clone()),
                Categories::Labelled(values) => Value::Array(
                    values
                        .iter()
                        .map(|(value, label)| {
                            let mut entry = Map::new();
                            entry.insert("value".into(), value.clone());
                            if let Some(label) = label {
                                entry.insert("label".into(), Value::String(label.clone()));
                            }
                            Value::Object(entry)
                        })
                        .collect(),
                ),
            },
        );
    }
    if let Some(ordered) = field.categories_ordered {
        put("categoriesOrdered", ordered.into());
    }
    if let Some(missing) = &field.missing_values {
        put("missingValues", missing_json(missing));
    }
    if let Some(rdf_type) = &field.rdf_type {
        put("rdfType", rdf_type.clone().into());
    }
    if let Some(constraints) = &field.constraints {
        put("constraints", constraints_json(constraints));
    }
    for (key, value) in &field.extra {
        put(key, value.clone());
    }
    Value::Object(out)
}

fn constraints_json(constraints: &Constraints) -> Value {
    let mut out = Map::new();
    if let Some(value) = constraints.required {
        out.insert("required".into(), value.into());
    }
    if let Some(value) = constraints.unique {
        out.insert("unique".into(), value.into());
    }
    if let Some(value) = constraints.min_length {
        out.insert("minLength".into(), value.into());
    }
    if let Some(value) = constraints.max_length {
        out.insert("maxLength".into(), value.into());
    }
    for (key, value) in [
        ("minimum", &constraints.minimum),
        ("maximum", &constraints.maximum),
        ("exclusiveMinimum", &constraints.exclusive_minimum),
        ("exclusiveMaximum", &constraints.exclusive_maximum),
    ] {
        if let Some(value) = value {
            out.insert(key.into(), value.clone());
        }
    }
    if let Some(value) = &constraints.pattern {
        out.insert("pattern".into(), value.clone().into());
    }
    if let Some(values) = &constraints.enumeration {
        out.insert("enum".into(), Value::Array(values.clone()));
    }
    if let Some(schema) = &constraints.json_schema {
        out.insert("jsonSchema".into(), Value::Object(schema.clone()));
    }
    Value::Object(out)
}

/// Why a Table Schema descriptor is refused. Each names the JSON pointer
/// (RFC 6901) of the property at fault.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TableSchemaError {
    /// A required property is absent.
    #[error("{pointer}: missing required property {property}")]
    Missing {
        /// Where the property belongs.
        pointer: String,
        /// The property.
        property: String,
    },
    /// A property has a JSON type the standard does not allow there.
    #[error("{pointer}: expected {expected}")]
    WrongType {
        /// The property.
        pointer: String,
        /// What the standard requires.
        expected: String,
    },
    /// A field names a type the standard does not define.
    #[error("{pointer}: unknown field type {name:?}")]
    UnknownType {
        /// The `type` property.
        pointer: String,
        /// The name written.
        name: String,
    },
    /// A field names a format its type does not define.
    #[error("{pointer}: format {format:?} is not a format of type {field_type}")]
    UnknownFormat {
        /// The `format` property.
        pointer: String,
        /// The field's type.
        field_type: String,
        /// The format written, `fmt:` prefix removed.
        format: String,
    },
    /// A property names a value outside the set the standard allows.
    #[error("{pointer}: {value:?} is not one of {allowed}")]
    NotAllowed {
        /// The property.
        pointer: String,
        /// The value written.
        value: String,
        /// The values allowed.
        allowed: String,
    },
    /// A value the standard requires to be unique repeats.
    #[error("{pointer}: {value} repeats")]
    Repeated {
        /// The repeating entry.
        pointer: String,
        /// The repeated value.
        value: String,
    },
    /// A key names a field the schema does not hold.
    #[error("{pointer}: no field is named {name:?}")]
    UnknownField {
        /// The key entry.
        pointer: String,
        /// The name written.
        name: String,
    },
    /// A key names a field name that two fields share.
    #[error("{pointer}: two fields are named {name:?}")]
    AmbiguousField {
        /// The key entry.
        pointer: String,
        /// The shared name.
        name: String,
    },
    /// A constraint the standard does not define.
    #[error("{pointer}: unknown constraint {name:?}")]
    UnknownConstraint {
        /// The constraint.
        pointer: String,
        /// Its name.
        name: String,
    },
    /// A constraint the standard defines for other field types.
    #[error("{pointer}: constraint {name} does not apply to a {field_type} field")]
    InapplicableConstraint {
        /// The constraint.
        pointer: String,
        /// Its name.
        name: String,
        /// The field's type.
        field_type: String,
    },
    /// A foreign key's two field lists differ in length.
    #[error("{pointer}: {fields} fields reference {reference} fields")]
    ReferenceLength {
        /// The foreign key.
        pointer: String,
        /// The number of this table's fields.
        fields: usize,
        /// The number of referenced fields.
        reference: usize,
    },
    /// An `enum` value outside the field's `categories`.
    #[error("{pointer}: enum value {value} is not a category")]
    EnumOutsideCategories {
        /// The `enum` entry.
        pointer: String,
        /// The value.
        value: String,
    },
    /// An empty array where the standard requires a non-empty one.
    #[error("{pointer}: must not be empty")]
    Empty {
        /// The property.
        pointer: String,
    },
}

/// The pointer of `key` under `base`, escaped as RFC 6901 requires.
fn pointer(base: &str, key: &str) -> String {
    format!("{base}/{}", key.replace('~', "~0").replace('/', "~1"))
}

struct Reader {
    errors: Vec<TableSchemaError>,
}

impl Reader {
    fn wrong(&mut self, at: String, expected: &str) {
        self.errors.push(TableSchemaError::WrongType {
            pointer: at,
            expected: expected.to_owned(),
        });
    }

    fn string(&mut self, object: &Map<String, Value>, key: &str, base: &str) -> Option<String> {
        let value = object.get(key)?;
        if let Value::String(text) = value {
            return Some(text.clone());
        }
        self.wrong(pointer(base, key), "a string");
        None
    }

    fn boolean(&mut self, object: &Map<String, Value>, key: &str, base: &str) -> Option<bool> {
        let value = object.get(key)?;
        if let Value::Bool(flag) = value {
            return Some(*flag);
        }
        self.wrong(pointer(base, key), "a boolean");
        None
    }

    /// The value of a property the standard requires, reporting its
    /// absence.
    fn required<'a>(
        &mut self,
        object: &'a Map<String, Value>,
        key: &str,
        at: &str,
    ) -> Option<&'a Value> {
        let value = object.get(key);
        if value.is_none() {
            self.errors.push(TableSchemaError::Missing {
                pointer: at.to_owned(),
                property: key.to_owned(),
            });
        }
        value
    }

    fn strings(&mut self, value: &Value, at: &str) -> Option<Vec<String>> {
        let Value::Array(items) = value else {
            self.wrong(at.to_owned(), "an array of strings");
            return None;
        };
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            match item {
                Value::String(text) => out.push(text.clone()),
                _ => self.wrong(format!("{at}/{index}"), "a string"),
            }
        }
        (out.len() == items.len()).then_some(out)
    }

    /// A key written as an array of strings, or as one string (v1.0).
    fn key(&mut self, value: &Value, at: &str) -> Option<Vec<String>> {
        match value {
            Value::String(name) => Some(vec![name.clone()]),
            other => self.strings(other, at),
        }
    }

    fn unique_strings(&mut self, values: &[String], at: &str) {
        let mut seen = std::collections::HashSet::new();
        for (index, value) in values.iter().enumerate() {
            if !seen.insert(value) {
                self.errors.push(TableSchemaError::Repeated {
                    pointer: format!("{at}/{index}"),
                    value: format!("{value:?}"),
                });
            }
        }
    }

    fn missing_values(&mut self, value: &Value, at: &str) -> Option<MissingValues> {
        let Value::Array(items) = value else {
            self.wrong(at.to_owned(), "an array");
            return None;
        };
        if items.iter().all(Value::is_string) {
            let values = self.strings(value, at)?;
            self.unique_strings(&values, at);
            return Some(MissingValues::Strings(values));
        }
        if !items.iter().all(Value::is_object) {
            self.wrong(at.to_owned(), "an array of strings or an array of objects");
            return None;
        }
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let base = format!("{at}/{index}");
            let Value::Object(entry) = item else {
                continue;
            };
            let Some(value) = self.string(entry, "value", &base) else {
                if !entry.contains_key("value") {
                    self.errors.push(TableSchemaError::Missing {
                        pointer: base,
                        property: "value".into(),
                    });
                }
                continue;
            };
            let label = self.string(entry, "label", &base);
            out.push((value, label));
        }
        let values: Vec<String> = out.iter().map(|(value, _)| value.clone()).collect();
        self.unique_strings(&values, at);
        let labels: Vec<String> = out.iter().filter_map(|(_, label)| label.clone()).collect();
        self.unique_strings(&labels, at);
        Some(MissingValues::Labelled(out))
    }
}

/// Read a Frictionless Table Schema descriptor.
///
/// # Errors
///
/// Every rule of the standard the descriptor breaks, in the order the
/// module documentation states.
pub fn parse_table_schema(descriptor: &Value) -> Result<TableSchema, Vec<TableSchemaError>> {
    let mut reader = Reader { errors: Vec::new() };
    let Value::Object(object) = descriptor else {
        return Err(vec![TableSchemaError::WrongType {
            pointer: String::new(),
            expected: "an object".into(),
        }]);
    };
    let profile = reader.string(object, "$schema", "");
    let fields = match object.get("fields") {
        None => {
            reader.errors.push(TableSchemaError::Missing {
                pointer: String::new(),
                property: "fields".into(),
            });
            Vec::new()
        }
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| read_field(&mut reader, item, &format!("/fields/{index}")))
            .collect(),
        Some(_) => {
            reader.wrong("/fields".into(), "an array of field descriptors");
            Vec::new()
        }
    };
    let fields_match = reader.string(object, "fieldsMatch", "").and_then(|name| {
        let parsed = FieldsMatch::parse(&name);
        if parsed.is_none() {
            reader.errors.push(TableSchemaError::NotAllowed {
                pointer: "/fieldsMatch".into(),
                value: name,
                allowed: "exact, equal, subset, superset, partial".into(),
            });
        }
        parsed
    });
    let missing_values = object
        .get("missingValues")
        .and_then(|value| reader.missing_values(value, "/missingValues"));
    let names = FieldNames(fields.iter().map(|field| field.name.as_str()).collect());
    let primary_key = object.get("primaryKey").and_then(|value| {
        let key = reader.key(value, "/primaryKey")?;
        names.resolve(&mut reader, &key, "/primaryKey");
        Some(key)
    });
    let unique_keys = object
        .get("uniqueKeys")
        .and_then(|value| read_unique_keys(&mut reader, value, &names));
    let foreign_keys = object
        .get("foreignKeys")
        .and_then(|value| read_foreign_keys(&mut reader, value, &names));
    let extra = object
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "$schema"
                    | "fields"
                    | "fieldsMatch"
                    | "missingValues"
                    | "primaryKey"
                    | "uniqueKeys"
                    | "foreignKeys"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if reader.errors.is_empty() {
        Ok(TableSchema {
            fields,
            profile,
            fields_match,
            missing_values,
            primary_key,
            unique_keys,
            foreign_keys,
            extra,
        })
    } else {
        Err(reader.errors)
    }
}

/// The field names of a schema, in order, for resolving the names a key
/// gives.
struct FieldNames<'a>(Vec<&'a str>);

impl FieldNames<'_> {
    /// Report each name of `key` that no field or more than one field has.
    fn resolve(&self, reader: &mut Reader, key: &[String], at: &str) {
        for (index, name) in key.iter().enumerate() {
            match self
                .0
                .iter()
                .filter(|candidate| **candidate == name)
                .count()
            {
                0 => reader.errors.push(TableSchemaError::UnknownField {
                    pointer: format!("{at}/{index}"),
                    name: name.clone(),
                }),
                1 => {}
                _ => reader.errors.push(TableSchemaError::AmbiguousField {
                    pointer: format!("{at}/{index}"),
                    name: name.clone(),
                }),
            }
        }
    }
}

fn read_unique_keys(
    reader: &mut Reader,
    value: &Value,
    names: &FieldNames<'_>,
) -> Option<Vec<Vec<String>>> {
    let Value::Array(items) = value else {
        reader.wrong("/uniqueKeys".into(), "an array of keys");
        return None;
    };
    if items.is_empty() {
        reader.errors.push(TableSchemaError::Empty {
            pointer: "/uniqueKeys".into(),
        });
    }
    let mut keys = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let at = format!("/uniqueKeys/{index}");
        if let Some(key) = reader.strings(item, &at) {
            names.resolve(reader, &key, &at);
            keys.push(key);
        }
    }
    Some(keys)
}

fn read_foreign_keys(
    reader: &mut Reader,
    value: &Value,
    names: &FieldNames<'_>,
) -> Option<Vec<ForeignKey>> {
    let Value::Array(items) = value else {
        reader.wrong("/foreignKeys".into(), "an array of foreign keys");
        return None;
    };
    let mut keys = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        if let Some(key) = read_foreign_key(reader, item, &format!("/foreignKeys/{index}"), names) {
            keys.push(key);
        }
    }
    Some(keys)
}

fn read_foreign_key(
    reader: &mut Reader,
    item: &Value,
    at: &str,
    names: &FieldNames<'_>,
) -> Option<ForeignKey> {
    let Value::Object(entry) = item else {
        reader.wrong(at.to_owned(), "an object");
        return None;
    };
    let fields = reader
        .required(entry, "fields", at)
        .and_then(|value| reader.key(value, &format!("{at}/fields")));
    let reference_at = format!("{at}/reference");
    let reference = match entry.get("reference") {
        Some(Value::Object(reference)) => Some(reference),
        Some(_) => {
            reader.wrong(reference_at.clone(), "an object");
            None
        }
        None => {
            reader.errors.push(TableSchemaError::Missing {
                pointer: at.to_owned(),
                property: "reference".into(),
            });
            None
        }
    };
    let (resource, reference_fields) = match reference {
        Some(reference) => {
            // A `resource` of "" is v1.0's spelling of a reference into
            // this table.
            let resource = reader
                .string(reference, "resource", &reference_at)
                .filter(|name| !name.is_empty());
            let reference_fields = reader
                .required(reference, "fields", &reference_at)
                .and_then(|value| reader.key(value, &format!("{reference_at}/fields")));
            (resource, reference_fields)
        }
        None => (None, None),
    };
    let (fields, reference_fields) = (fields?, reference_fields?);
    names.resolve(reader, &fields, &format!("{at}/fields"));
    if resource.is_none() {
        names.resolve(reader, &reference_fields, &format!("{reference_at}/fields"));
    }
    if fields.len() != reference_fields.len() {
        reader.errors.push(TableSchemaError::ReferenceLength {
            pointer: at.to_owned(),
            fields: fields.len(),
            reference: reference_fields.len(),
        });
    }
    Some(ForeignKey {
        fields,
        resource,
        reference_fields,
    })
}

/// The properties a field descriptor defines, which do not go to `extra`.
const FIELD_PROPERTIES: [&str; 11] = [
    "name",
    "type",
    "format",
    "title",
    "description",
    "example",
    "constraints",
    "categories",
    "categoriesOrdered",
    "missingValues",
    "rdfType",
];

fn read_field(reader: &mut Reader, item: &Value, at: &str) -> Option<Field> {
    let Value::Object(object) = item else {
        reader.wrong(at.to_owned(), "a field descriptor object");
        return None;
    };
    let name = reader.string(object, "name", at);
    if !object.contains_key("name") {
        reader.errors.push(TableSchemaError::Missing {
            pointer: at.to_owned(),
            property: "name".into(),
        });
    }
    let field_type = reader.string(object, "type", at).and_then(|written| {
        let parsed = FieldType::parse(&written);
        if parsed.is_none() {
            reader.errors.push(TableSchemaError::UnknownType {
                pointer: pointer(at, "type"),
                name: written,
            });
        }
        parsed
    });
    let effective = field_type.unwrap_or(FieldType::Any);
    let format = reader.string(object, "format", at).map(|written| {
        let format = written
            .strip_prefix("fmt:")
            .map_or_else(|| written.clone(), str::to_owned);
        if !effective.admits_format(&format) {
            reader.errors.push(TableSchemaError::UnknownFormat {
                pointer: pointer(at, "format"),
                field_type: effective.name().into(),
                format: format.clone(),
            });
        }
        format
    });
    let title = reader.string(object, "title", at);
    let description = reader.string(object, "description", at);
    let rdf_type = reader.string(object, "rdfType", at);
    let missing_values = object
        .get("missingValues")
        .and_then(|value| reader.missing_values(value, &pointer(at, "missingValues")));
    let (options, option_keys) = read_type_options(reader, object, effective, at);
    let categories = object
        .get("categories")
        .and_then(|value| read_categories(reader, value, effective, &pointer(at, "categories")));
    let categories_ordered = reader.boolean(object, "categoriesOrdered", at);
    let constraints = object.get("constraints").and_then(|value| {
        read_constraints(
            reader,
            value,
            effective,
            categories.as_ref(),
            &pointer(at, "constraints"),
        )
    });
    let extra = object
        .iter()
        .filter(|(key, _)| {
            !FIELD_PROPERTIES.contains(&key.as_str()) && !option_keys.contains(&key.as_str())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Some(Field {
        name: name?,
        field_type,
        format,
        title,
        description,
        example: object.get("example").cloned(),
        constraints,
        categories,
        categories_ordered,
        missing_values,
        rdf_type,
        options,
        extra,
    })
}

/// The properties `field_type` adds, read, and their names. A property a
/// type does not define stays an ordinary extra property of the field.
fn read_type_options(
    reader: &mut Reader,
    object: &Map<String, Value>,
    field_type: FieldType,
    at: &str,
) -> (TypeOptions, &'static [&'static str]) {
    let mut options = TypeOptions::default();
    let keys: &'static [&'static str] = match field_type {
        FieldType::Number => &["decimalChar", "groupChar", "bareNumber"],
        FieldType::Integer => &["groupChar", "bareNumber"],
        FieldType::Boolean => &["trueValues", "falseValues"],
        FieldType::List => &["delimiter", "itemType"],
        _ => &[],
    };
    if keys.contains(&"decimalChar") {
        options.decimal_char = reader.string(object, "decimalChar", at);
    }
    if keys.contains(&"groupChar") {
        options.group_char = reader.string(object, "groupChar", at);
    }
    if keys.contains(&"bareNumber") {
        options.bare_number = reader.boolean(object, "bareNumber", at);
    }
    if keys.contains(&"trueValues") {
        options.true_values = object
            .get("trueValues")
            .and_then(|value| reader.strings(value, &pointer(at, "trueValues")));
        options.false_values = object
            .get("falseValues")
            .and_then(|value| reader.strings(value, &pointer(at, "falseValues")));
    }
    if keys.contains(&"delimiter") {
        options.delimiter = reader.string(object, "delimiter", at);
        options.item_type = reader.string(object, "itemType", at).and_then(|written| {
            let parsed = FieldType::parse(&written).filter(|item| LIST_ITEM_TYPES.contains(item));
            if parsed.is_none() {
                reader.errors.push(TableSchemaError::NotAllowed {
                    pointer: pointer(at, "itemType"),
                    value: written,
                    allowed: "string, integer, boolean, number, datetime, date, time".into(),
                });
            }
            parsed
        });
    }
    (options, keys)
}

fn read_categories(
    reader: &mut Reader,
    value: &Value,
    field_type: FieldType,
    at: &str,
) -> Option<Categories> {
    if !matches!(field_type, FieldType::String | FieldType::Integer) {
        reader
            .errors
            .push(TableSchemaError::InapplicableConstraint {
                pointer: at.to_owned(),
                name: "categories".into(),
                field_type: field_type.name().into(),
            });
        return None;
    }
    let Value::Array(items) = value else {
        reader.wrong(at.to_owned(), "an array");
        return None;
    };
    let of_type = |value: &Value| match field_type {
        FieldType::String => value.is_string(),
        _ => value.is_i64() || value.is_u64(),
    };
    let expected = if field_type == FieldType::String {
        "a string category"
    } else {
        "an integer category"
    };
    let check_unique = |reader: &mut Reader, values: &[&Value]| {
        for (index, value) in values.iter().enumerate() {
            if values[..index].contains(value) {
                reader.errors.push(TableSchemaError::Repeated {
                    pointer: format!("{at}/{index}"),
                    value: value.to_string(),
                });
            }
        }
    };
    if items.iter().all(Value::is_object) && !items.is_empty() {
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let base = format!("{at}/{index}");
            let Value::Object(entry) = item else {
                continue;
            };
            match entry.get("value") {
                Some(value) if of_type(value) => {
                    out.push((value.clone(), reader.string(entry, "label", &base)));
                }
                Some(_) => reader.wrong(pointer(&base, "value"), expected),
                None => reader.errors.push(TableSchemaError::Missing {
                    pointer: base,
                    property: "value".into(),
                }),
            }
        }
        check_unique(
            reader,
            &out.iter().map(|(value, _)| value).collect::<Vec<_>>(),
        );
        let labels: Vec<String> = out.iter().filter_map(|(_, label)| label.clone()).collect();
        reader.unique_strings(&labels, at);
        return Some(Categories::Labelled(out));
    }
    for (index, item) in items.iter().enumerate() {
        if !of_type(item) {
            reader.wrong(format!("{at}/{index}"), expected);
        }
    }
    check_unique(reader, &items.iter().collect::<Vec<_>>());
    Some(Categories::Values(items.clone()))
}

/// Store one constraint the field's type admits, checking its value.
fn store_constraint(
    reader: &mut Reader,
    out: &mut Constraints,
    name: &str,
    value: &Value,
    here: String,
    field_type: FieldType,
    categories: Option<&Categories>,
) {
    match name {
        "required" | "unique" => {
            let Some(flag) = value.as_bool() else {
                reader.wrong(here, "a boolean");
                return;
            };
            if name == "required" {
                out.required = Some(flag);
            } else {
                out.unique = Some(flag);
            }
        }
        "minLength" | "maxLength" => {
            let Some(length) = value.as_u64() else {
                reader.wrong(here, "a non-negative integer");
                return;
            };
            if name == "minLength" {
                out.min_length = Some(length);
            } else {
                out.max_length = Some(length);
            }
        }
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
            let valid = match field_type {
                FieldType::Integer => value.is_i64() || value.is_u64() || value.is_string(),
                FieldType::Number => value.is_number() || value.is_string(),
                _ => value.is_string() || value.is_i64() || value.is_u64(),
            };
            if !valid {
                reader.wrong(here, "a bound of the field's type");
                return;
            }
            let slot = match name {
                "minimum" => &mut out.minimum,
                "maximum" => &mut out.maximum,
                "exclusiveMinimum" => &mut out.exclusive_minimum,
                _ => &mut out.exclusive_maximum,
            };
            *slot = Some(value.clone());
        }
        "pattern" => match value {
            Value::String(pattern) => out.pattern = Some(pattern.clone()),
            _ => reader.wrong(here, "a string"),
        },
        "jsonSchema" => match value {
            Value::Object(schema) => out.json_schema = Some(schema.clone()),
            _ => reader.wrong(here, "an object"),
        },
        _ => {
            let Value::Array(values) = value else {
                reader.wrong(here, "an array");
                return;
            };
            if let Some(categories) = categories {
                let allowed = categories.values();
                for (index, value) in values.iter().enumerate() {
                    if !allowed.contains(&value) {
                        reader.errors.push(TableSchemaError::EnumOutsideCategories {
                            pointer: format!("{here}/{index}"),
                            value: value.to_string(),
                        });
                    }
                }
            }
            out.enumeration = Some(values.clone());
        }
    }
}

fn read_constraints(
    reader: &mut Reader,
    value: &Value,
    field_type: FieldType,
    categories: Option<&Categories>,
    at: &str,
) -> Option<Constraints> {
    let Value::Object(object) = value else {
        reader.wrong(at.to_owned(), "a constraints object");
        return None;
    };
    let mut out = Constraints::default();
    for (name, value) in object {
        let here = pointer(at, name);
        let applies = match name.as_str() {
            "required" | "unique" | "enum" => true,
            "minLength" | "maxLength" => field_type.has_length(),
            "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                field_type.is_ordered()
            }
            "pattern" => field_type == FieldType::String,
            "jsonSchema" => matches!(field_type, FieldType::Array | FieldType::Object),
            _ => {
                reader.errors.push(TableSchemaError::UnknownConstraint {
                    pointer: here,
                    name: name.clone(),
                });
                continue;
            }
        };
        if !applies {
            reader
                .errors
                .push(TableSchemaError::InapplicableConstraint {
                    pointer: here,
                    name: name.clone(),
                    field_type: field_type.name().into(),
                });
            continue;
        }
        store_constraint(reader, &mut out, name, value, here, field_type, categories);
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    fn errors(descriptor: &Value) -> Vec<TableSchemaError> {
        parse_table_schema(descriptor).expect_err("refused")
    }

    #[test]
    fn the_standard_examples_read_and_write_back() {
        let descriptor = json!({
            "$schema": "https://datapackage.org/profiles/2.0/tableschema.json",
            "fields": [
                {"name": "id", "type": "integer", "constraints": {"required": true, "minimum": 1}},
                {"name": "name", "type": "string", "constraints": {"pattern": "^a.*$", "enum": ["apple"]},
                 "categories": ["apple", "orange"], "categoriesOrdered": false},
                {"name": "price", "type": "number", "decimalChar": ",", "groupChar": ".", "bareNumber": false},
                {"name": "ok", "type": "boolean", "trueValues": ["Y"], "falseValues": ["N"]},
                {"name": "tags", "type": "list", "delimiter": ";", "itemType": "integer"},
                {"name": "seen", "type": "date", "format": "%d/%m/%Y"},
                {"name": "parent", "missingValues": [{"value": "-", "label": "none"}], "x-note": 1}
            ],
            "fieldsMatch": "equal",
            "missingValues": ["", "NA"],
            "primaryKey": ["id"],
            "uniqueKeys": [["name"], ["name", "price"]],
            "foreignKeys": [
                {"fields": ["parent"], "reference": {"fields": ["id"]}},
                {"fields": "name", "reference": {"resource": "fruit", "fields": "label"}}
            ],
            "x-owner": "panproto"
        });
        let schema = parse_table_schema(&descriptor).expect("reads");
        assert_eq!(schema.fields.len(), 7);
        assert_eq!(schema.fields[6].effective_type(), FieldType::Any);
        assert_eq!(schema.fields[6].extra.get("x-note"), Some(&json!(1)));
        assert_eq!(schema.fields[4].options.item_type, Some(FieldType::Integer));
        assert_eq!(schema.fields[2].options.decimal_char.as_deref(), Some(","));
        assert_eq!(schema.missing_values_of(0), ["", "NA"]);
        assert_eq!(schema.missing_values_of(6), ["-"]);
        assert_eq!(schema.foreign_keys.as_ref().unwrap()[0].resource, None);
        assert_eq!(
            schema.foreign_keys.as_ref().unwrap()[1].reference_fields,
            ["label"]
        );
        assert_eq!(schema.extra.get("x-owner"), Some(&json!("panproto")));
        let written = schema.to_json();
        assert_eq!(parse_table_schema(&written).expect("rereads"), schema);
        assert_eq!(written["foreignKeys"][1]["fields"], json!(["name"]));
    }

    #[test]
    fn the_v1_forms_read_as_their_v2_meaning() {
        let schema = parse_table_schema(&json!({
            "fields": [
                {"name": "a", "type": "date", "format": "fmt:%Y"},
                {"name": "a"},
                {"name": "b"}
            ],
            "primaryKey": "b",
            "foreignKeys": [{"fields": "b", "reference": {"resource": "", "fields": "b"}}]
        }))
        .expect("reads");
        assert_eq!(schema.fields[0].format.as_deref(), Some("%Y"));
        assert_eq!(schema.primary_key, Some(vec!["b".to_owned()]));
        let key = &schema.foreign_keys.as_ref().unwrap()[0];
        assert_eq!(key.resource, None);
        assert_eq!(key.fields, ["b"]);
        // Repeated field names read, as v1.0 allowed them; a key naming one
        // is ambiguous and refused.
        assert_eq!(
            errors(&json!({"fields": [{"name": "a"}, {"name": "a"}], "primaryKey": ["a"]})),
            [TableSchemaError::AmbiguousField {
                pointer: "/primaryKey/0".into(),
                name: "a".into()
            }]
        );
    }

    #[test]
    fn every_broken_rule_is_reported_in_its_fixed_order() {
        let found = errors(&json!({
            "fields": [
                {"type": "text"},
                {"name": "n", "type": "number", "format": "currency",
                 "constraints": {"between": [1, 2], "minLength": 2, "pattern": "x"}},
                {"name": "s", "type": "string", "categories": ["x", "x"],
                 "constraints": {"enum": ["y"], "minimum": 1}},
                {"name": "l", "type": "list", "itemType": "object"},
                {"name": "g", "type": "geopoint", "categories": [1]}
            ],
            "fieldsMatch": "loose",
            "missingValues": ["", 1],
            "primaryKey": ["missing"],
            "uniqueKeys": [],
            "foreignKeys": [{"fields": ["n", "s"], "reference": {"fields": ["n"]}}]
        }));
        assert_eq!(
            found,
            [
                TableSchemaError::Missing {
                    pointer: "/fields/0".into(),
                    property: "name".into()
                },
                TableSchemaError::UnknownType {
                    pointer: "/fields/0/type".into(),
                    name: "text".into()
                },
                TableSchemaError::UnknownFormat {
                    pointer: "/fields/1/format".into(),
                    field_type: "number".into(),
                    format: "currency".into()
                },
                TableSchemaError::UnknownConstraint {
                    pointer: "/fields/1/constraints/between".into(),
                    name: "between".into()
                },
                TableSchemaError::InapplicableConstraint {
                    pointer: "/fields/1/constraints/minLength".into(),
                    name: "minLength".into(),
                    field_type: "number".into()
                },
                TableSchemaError::InapplicableConstraint {
                    pointer: "/fields/1/constraints/pattern".into(),
                    name: "pattern".into(),
                    field_type: "number".into()
                },
                TableSchemaError::Repeated {
                    pointer: "/fields/2/categories/1".into(),
                    value: "\"x\"".into()
                },
                TableSchemaError::EnumOutsideCategories {
                    pointer: "/fields/2/constraints/enum/0".into(),
                    value: "\"y\"".into()
                },
                TableSchemaError::InapplicableConstraint {
                    pointer: "/fields/2/constraints/minimum".into(),
                    name: "minimum".into(),
                    field_type: "string".into()
                },
                TableSchemaError::NotAllowed {
                    pointer: "/fields/3/itemType".into(),
                    value: "object".into(),
                    allowed: "string, integer, boolean, number, datetime, date, time".into()
                },
                TableSchemaError::InapplicableConstraint {
                    pointer: "/fields/4/categories".into(),
                    name: "categories".into(),
                    field_type: "geopoint".into()
                },
                TableSchemaError::NotAllowed {
                    pointer: "/fieldsMatch".into(),
                    value: "loose".into(),
                    allowed: "exact, equal, subset, superset, partial".into()
                },
                TableSchemaError::WrongType {
                    pointer: "/missingValues".into(),
                    expected: "an array of strings or an array of objects".into()
                },
                TableSchemaError::UnknownField {
                    pointer: "/primaryKey/0".into(),
                    name: "missing".into()
                },
                TableSchemaError::Empty {
                    pointer: "/uniqueKeys".into()
                },
                TableSchemaError::ReferenceLength {
                    pointer: "/foreignKeys/0".into(),
                    fields: 2,
                    reference: 1
                },
            ]
        );
    }

    #[test]
    fn a_descriptor_without_fields_or_not_an_object_is_refused() {
        assert_eq!(
            errors(&json!({})),
            [TableSchemaError::Missing {
                pointer: String::new(),
                property: "fields".into()
            }]
        );
        assert_eq!(
            errors(&json!([])),
            [TableSchemaError::WrongType {
                pointer: String::new(),
                expected: "an object".into()
            }]
        );
    }

    #[test]
    fn a_property_a_type_does_not_define_stays_an_extra_property() {
        let schema = parse_table_schema(&json!({
            "fields": [{"name": "s", "type": "string", "decimalChar": ",", "a/b": true}]
        }))
        .expect("reads");
        let field = &schema.fields[0];
        assert_eq!(field.options, TypeOptions::default());
        assert_eq!(field.extra.get("decimalChar"), Some(&json!(",")));
        assert_eq!(schema.to_json()["fields"][0]["a/b"], json!(true));
        assert_eq!(pointer("/x", "a/b~c"), "/x/a~1b~0c");
    }
}
