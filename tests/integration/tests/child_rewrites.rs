#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Lenses that rewrite a field held by a child of its record's node.
//!
//! The cases are in `tests/fixtures/child_rewrites/cases.json`, shared with
//! the Python suite. Each has a source record, the view `get` shows of it,
//! and for each edited view the source `put` gives, or that it refuses. The
//! views are put both through JSON (parsed, aligned and folded) and, where
//! the edit changes values the view holds on children, in memory.

use std::collections::HashSet;

use panproto_core::expr::Expr;
use panproto_core::gat::CoercionClass;
use panproto_core::inst::value::FieldPresence;
use panproto_core::inst::{
    CaseBranch, CompiledMigration, FieldTransform, Value, WInstance, parse_json, to_json,
};
use panproto_core::lens::{
    self, Complement, Lens, align_view_to_complement, check_get_put, check_put_get,
    fold_computed_children_into_fields, get, put,
};
use panproto_core::schema::{Protocol, Schema};
use serde_json::Value as Json;

const ROOT: &str = "com.example.note:body";

fn cases() -> Vec<Json> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/child_rewrites/cases.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn expr(source: &Json) -> Expr {
    let tokens = panproto_expr_parser::tokenize(source.as_str().unwrap()).unwrap();
    panproto_expr_parser::parse(&tokens).unwrap()
}

fn text(json: &Json) -> String {
    json.as_str().unwrap().to_owned()
}

fn transform(json: &Json) -> FieldTransform {
    let inverse = json.get("from").map(expr);
    let coercion_class = if inverse.is_some() {
        CoercionClass::Iso
    } else {
        CoercionClass::Projection
    };
    if let Some(key) = json.get("rewrite") {
        FieldTransform::ApplyExpr {
            key: text(key),
            expr: expr(&json["to"]),
            inverse,
            coercion_class,
        }
    } else if let Some(key) = json.get("compute") {
        FieldTransform::ComputeField {
            target_key: text(key),
            expr: expr(&json["to"]),
            inverse,
            coercion_class,
        }
    } else if let Some(old) = json.get("rename") {
        FieldTransform::RenameField {
            old_key: text(old),
            new_key: text(&json["as"]),
        }
    } else if let Some(key) = json.get("drop") {
        FieldTransform::DropField { key: text(key) }
    } else if let Some(key) = json.get("add") {
        FieldTransform::AddField {
            key: text(key),
            value: Value::Str(text(&json["value"])),
        }
    } else if let Some(keys) = json.get("keep") {
        FieldTransform::KeepFields {
            keys: keys.as_array().unwrap().iter().map(text).collect(),
        }
    } else {
        FieldTransform::Case {
            branches: json["case"]
                .as_array()
                .unwrap()
                .iter()
                .map(|branch| CaseBranch {
                    predicate: expr(&branch[0]),
                    transforms: branch[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(transform)
                        .collect(),
                })
                .collect(),
        }
    }
}

struct Case {
    json: Json,
    protocol: Protocol,
    lens: Lens,
}

impl Case {
    fn all() -> Vec<Self> {
        cases().into_iter().map(Self::new).collect()
    }

    fn new(json: Json) -> Self {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "com.example.note",
            "defs": {"main": {"type": "record", "key": "any", "record": {
                "type": "object", "properties": json["properties"],
            }}},
        });
        let schema: Schema =
            panproto_protocols::parse_schema_bundle("atproto", &[lexicon]).unwrap();
        let transforms: Vec<FieldTransform> = json["transforms"]
            .as_array()
            .unwrap()
            .iter()
            .map(transform)
            .collect();
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                field_transforms: [(text(&json["at"]).into(), transforms)].into(),
                ..CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema,
        };
        Self {
            json,
            protocol: panproto_protocols::atproto::protocol(),
            lens,
        }
    }

    fn name(&self) -> &str {
        self.json["name"].as_str().unwrap()
    }

    fn read(&self, json: &Json) -> WInstance {
        parse_json(&self.lens.src_schema, &self.protocol, ROOT, json).unwrap()
    }

    fn write(&self, tree: &WInstance) -> Json {
        to_json(&self.lens.src_schema, tree).unwrap()
    }

    fn source(&self) -> WInstance {
        self.read(&self.json["source"])
    }

    /// Put a JSON view, as every serialized entry point does.
    fn put_json(&self, view: &Json, complement: &Complement) -> Result<Json, lens::LensError> {
        let aligned = align_view_to_complement(&self.lens, &self.read(view), complement)?;
        let folded = fold_computed_children_into_fields(&self.lens, &aligned, complement);
        Ok(self.write(&put(&self.lens, &folded, complement)?))
    }

    /// The complement release 0.78.0 took of the source, bound to this lens.
    fn recorded_complement(&self) -> Complement {
        let mut complement: Complement =
            serde_json::from_value(self.json["complement_0_78_0"].clone()).unwrap();
        complement.identity = Some(
            self.lens
                .identity(panproto_core::inst::ComplementShape::WType),
        );
        complement
    }
}

/// `found` against the expected source, which is the string `refused` where
/// put refuses.
fn check(failures: &mut Vec<String>, what: &str, expected: &Json, found: &Result<Json, String>) {
    let agrees = found
        .as_ref()
        .map_or_else(|_| expected == "refused", |found| found == expected);
    if !agrees {
        failures.push(format!("{what}: expected {expected}, found {found:?}"));
    }
}

fn report(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn get_shows_the_recorded_view() {
    let mut failures = Vec::new();
    for case in Case::all() {
        let (view, _) = get(&case.lens, &case.source()).unwrap();
        let found = case.write(&view);
        check(&mut failures, case.name(), &case.json["view"], &Ok(found));
    }
    report(&failures);
}

#[test]
fn an_unedited_view_puts_back_the_source() {
    let mut failures = Vec::new();
    for case in Case::all() {
        let source = case.source();
        let (view, complement) = get(&case.lens, &source).unwrap();
        match put(&case.lens, &view, &complement) {
            Ok(restored) if lens::instances_equivalent(&restored, &source) => {}
            Ok(restored) => failures.push(format!(
                "{}: {} for {}",
                case.name(),
                case.write(&restored),
                case.json["source"]
            )),
            Err(error) => failures.push(format!("{}: {error}", case.name())),
        }
    }
    report(&failures);
}

#[test]
fn the_strict_law_checks_hold() {
    let mut failures = Vec::new();
    for case in Case::all() {
        let source = case.source();
        if let Err(error) = check_get_put(&case.lens, &source) {
            failures.push(format!("{}: {error}", case.name()));
        }
        if let Err(error) = check_put_get(&case.lens, &source) {
            failures.push(format!("{}: {error}", case.name()));
        }
    }
    report(&failures);
}

#[test]
fn a_view_through_json_puts_back_the_expected_source() {
    let mut failures = Vec::new();
    let mut puts = 0;
    for case in Case::all() {
        let (_, complement) = get(&case.lens, &case.source()).unwrap();
        for expected in case.json["puts"].as_array().unwrap() {
            let what = format!("{} at {}", case.name(), expected["view"]);
            let found = case.put_json(&expected["view"], &complement);
            let found = found.map_err(|error| error.to_string());
            if let Ok(found) = &found {
                let (again, _) = get(&case.lens, &case.read(found)).unwrap();
                let shown = case.write(&again);
                if shown != expected["view"] {
                    failures.push(format!("{what}: put {found}, which shows {shown}"));
                }
            }
            check(&mut failures, &what, &expected["source"], &found);
            puts += 1;
        }
    }
    assert_eq!(puts, 258);
    report(&failures);
}

/// The edits of a view that change only values its root's children hold,
/// put as edits of those nodes.
#[test]
fn a_view_edited_in_memory_puts_back_the_expected_source() {
    let mut failures = Vec::new();
    let (mut edited_in_memory, mut other) = (0, 0);
    for case in Case::all() {
        let (view, complement) = get(&case.lens, &case.source()).unwrap();
        for expected in case.json["puts"].as_array().unwrap() {
            let Some(edited) = edited_in_place(&case, &view, &expected["view"]) else {
                other += 1;
                continue;
            };
            edited_in_memory += 1;
            let found = put(&case.lens, &edited, &complement);
            if let Ok(found) = &found {
                let (again, _) = get(&case.lens, found).unwrap();
                if case.write(&again) != expected["view"] {
                    failures.push(format!(
                        "{} at {}: shows {}",
                        case.name(),
                        expected["view"],
                        case.write(&again)
                    ));
                }
            }
            // The view get returned puts back the source, whatever a view
            // read from its serialization does.
            // A view that holds a field on the record keeps the child that
            // field shadows as it is, which a serialization of it cannot.
            let source = if expected["view"] == case.json["view"] {
                &case.json["source"]
            } else {
                expected
                    .get("memory")
                    .unwrap_or_else(|| &expected["source"])
            };
            check(
                &mut failures,
                &format!("{} at {}", case.name(), expected["view"]),
                source,
                &found
                    .map(|found| case.write(&found))
                    .map_err(|error| error.to_string()),
            );
        }
    }
    assert_eq!((edited_in_memory, other), (196, 62));
    report(&failures);
}

/// `view` with each scalar member of `shown` that its root shows otherwise
/// set on the child that holds it; nothing when a member is added, removed,
/// structured or shown by a field of the root.
fn edited_in_place(case: &Case, view: &WInstance, shown: &Json) -> Option<WInstance> {
    let before = case.write(view);
    let (before, shown) = (before.as_object()?, shown.as_object()?);
    if !before.keys().eq(shown.keys()) {
        return None;
    }
    let mut edited = view.clone();
    for (key, value) in shown {
        if &before[key] == value {
            continue;
        }
        if view.nodes[&view.root].extra_fields.contains_key(key) {
            return None;
        }
        let presence = match value {
            Json::Null => FieldPresence::Null,
            Json::Bool(b) => FieldPresence::Present(Value::Bool(*b)),
            Json::Number(n) => FieldPresence::Present(
                n.as_i64()
                    .map_or_else(|| Value::Float(n.as_f64().unwrap()), Value::Int),
            ),
            Json::String(s) => FieldPresence::Present(Value::Str(s.clone())),
            Json::Array(_) | Json::Object(_) => return None,
        };
        let (_, child, _) = view
            .arcs
            .iter()
            .find(|(parent, _, edge)| *parent == view.root && edge.name.as_deref() == Some(key))?;
        edited.nodes.get_mut(child).unwrap().value = Some(presence);
    }
    Some(edited)
}

/// Editing a view away and back restores the source.
#[test]
fn an_edit_undone_restores_the_source() {
    let mut failures = Vec::new();
    let mut undone = 0;
    for case in Case::all() {
        if case.json["undo"] != true {
            continue;
        }
        let (_, complement) = get(&case.lens, &case.source()).unwrap();
        for away in case.json["puts"].as_array().unwrap() {
            if away["source"] == "refused" || away["view"]["t"] != case.json["view"]["t"] {
                continue;
            }
            let Ok(there) = case.put_json(&away["view"], &complement) else {
                failures.push(format!("{} at {}", case.name(), away["view"]));
                continue;
            };
            let (_, complement) = get(&case.lens, &case.read(&there)).unwrap();
            let back = case.put_json(&case.json["view"], &complement);
            check(
                &mut failures,
                &format!("{} away to {}", case.name(), away["view"]),
                &case.json["source"],
                &back.map_err(|error| error.to_string()),
            );
            undone += 1;
        }
    }
    assert!(undone > 0);
    report(&failures);
}

/// The complement is the one release 0.78.0 took, byte for byte, except in
/// the cases that record what put needs and 0.78.0 did not record.
#[test]
fn the_complement_is_recorded_as_before_where_it_suffices() {
    let mut failures = Vec::new();
    let (mut same, mut more) = (0, 0);
    for case in Case::all() {
        let (_, complement) = get(&case.lens, &case.source()).unwrap();
        let before = case.recorded_complement();
        let bytes = |complement: &Complement| rmp_serde::to_vec(complement).unwrap();
        let identical = bytes(&complement) == bytes(&before)
            && serde_json::to_string(&complement).unwrap()
                == serde_json::to_string(&before).unwrap();
        if identical != (case.json["same_complement"] == true) {
            failures.push(format!(
                "{}: {} against {}",
                case.name(),
                serde_json::to_string(&complement).unwrap(),
                serde_json::to_string(&before).unwrap()
            ));
        }
        if identical {
            same += 1;
        } else {
            more += 1;
        }
        // Nothing recorded before is lost.
        for (id, value) in &before.original_values {
            assert_eq!(complement.original_values.get(id), Some(value));
        }
        for (id, fields) in &before.original_extra_fields {
            assert_eq!(complement.original_extra_fields.get(id), Some(fields));
        }
    }
    assert!(same > 0 && more > 0, "{same} the same, {more} with more");
    report(&failures);
}
