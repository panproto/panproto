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
use panproto_core::gat::{CoercionClass, Name};
use panproto_core::inst::value::FieldPresence;
use panproto_core::inst::{
    CaseBranch, CompiledMigration, FieldTransform, PutDomainError, Value, WInstance, parse_json,
    to_json,
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

/// An expression written as source text, or as the expression itself.
fn expr(source: &Json) -> Expr {
    let Some(text) = source.as_str() else {
        return serde_json::from_value(source.clone()).unwrap();
    };
    let tokens = panproto_expr_parser::tokenize(text).unwrap();
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
        let mut defs = serde_json::json!({"main": {"type": "record", "key": "any", "record": {
            "type": "object", "properties": json["properties"],
        }}});
        if let Some(more) = json.get("defs").and_then(Json::as_object) {
            for (name, def) in more {
                defs[name] = def.clone();
            }
        }
        let lexicon = serde_json::json!({"lexicon": 1, "id": "com.example.note", "defs": defs});
        let schema: Schema =
            panproto_protocols::parse_schema_bundle("atproto", &[lexicon]).unwrap();
        let program = |transforms: &Json| -> Vec<FieldTransform> {
            transforms
                .as_array()
                .unwrap()
                .iter()
                .map(transform)
                .collect()
        };
        let programs: Vec<(Name, Vec<FieldTransform>)> = json.get("programs").map_or_else(
            || vec![(text(&json["at"]).into(), program(&json["transforms"]))],
            |programs| {
                programs
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(anchor, transforms)| (anchor.as_str().into(), program(transforms)))
                    .collect()
            },
        );
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                field_transforms: programs.into_iter().collect(),
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

/// A complement release 0.78.0 took, stored as bytes and read back by this
/// release, puts back what this release's complement does wherever the two
/// are the same, and otherwise only sources whose view is the one put.
#[test]
fn a_complement_taken_by_0_78_0_and_stored_puts_back_through_this_release() {
    let mut failures = Vec::new();
    let (mut same, mut other) = (0, 0);
    for case in Case::all() {
        let stored = rmp_serde::to_vec(&case.recorded_complement()).unwrap();
        let earlier: Complement = rmp_serde::from_slice(&stored).unwrap();
        let source = case.source();
        let (view, complement) = get(&case.lens, &source).unwrap();
        let alike = case.json["same_complement"] == true;
        let unedited = case.json["view"].clone();
        let back = put(&case.lens, &view, &earlier).map(|restored| case.write(&restored));
        let back_json = case
            .put_json(&unedited, &earlier)
            .map_err(|error| error.to_string());
        if alike {
            same += 1;
            check(
                &mut failures,
                &format!("{} unedited", case.name()),
                &case.json["source"],
                &back.map_err(|error| error.to_string()),
            );
        } else {
            other += 1;
        }
        let mut views: Vec<(Json, Result<Json, String>)> = vec![(unedited, back_json)];
        for expected in case.json["puts"].as_array().unwrap() {
            let found = case
                .put_json(&expected["view"], &earlier)
                .map_err(|error| error.to_string());
            if alike {
                let now = case
                    .put_json(&expected["view"], &complement)
                    .map_err(|error| error.to_string());
                if found.is_ok() != now.is_ok() || found.as_ref().ok() != now.as_ref().ok() {
                    failures.push(format!(
                        "{} at {}: {found:?} through the 0.78.0 complement, {now:?} through this one",
                        case.name(),
                        expected["view"]
                    ));
                }
            }
            views.push((expected["view"].clone(), found));
        }
        for (shown, found) in views {
            if let Ok(found) = found {
                let (again, _) = get(&case.lens, &case.read(&found)).unwrap();
                if case.write(&again) != shown {
                    failures.push(format!(
                        "{} at {shown}: put {found} through the 0.78.0 complement, which shows {}",
                        case.name(),
                        case.write(&again)
                    ));
                }
            }
        }
    }
    assert_eq!((same, other), (57, 15));
    report(&failures);
}

/// An edit of a field a default supplies, which a rewrite then changes, is
/// refused as an edit of a derived field: no source value of the field is
/// what the view shows.
#[test]
fn an_edit_of_a_rewritten_default_is_refused_as_derived() {
    let case = Case::new(serde_json::json!({
        "name": "default then rewrite",
        "at": ROOT,
        "properties": {"n": {"type": "string"}, "k": {"type": "string"}},
        "transforms": [
            {"add": "k", "value": "d"},
            {"rewrite": "k", "to": "k ++ \"!\"", "from": "replace k \"!\" \"\""},
        ],
        "source": {"n": "a"},
    }));
    let (view, complement) = get(&case.lens, &case.source()).unwrap();
    assert_eq!(case.write(&view), serde_json::json!({"k": "d!", "n": "a"}));
    for edited in [
        serde_json::json!({"k": "e!", "n": "a"}),
        serde_json::json!({"n": "a"}),
    ] {
        match case.put_json(&edited, &complement) {
            Err(lens::LensError::PutDomain(violation)) => assert!(
                matches!(
                    &*violation,
                    PutDomainError::DerivedCellEdited { field, .. } if field == "k"
                ),
                "{edited}: {violation}"
            ),
            found => panic!("{edited}: {found:?}"),
        }
    }
}

/// The put of each edited view of `case` through `put_json`, against the
/// sources given, `"refused"` where put refuses.
fn check_puts(case: &Case, puts: &[(Json, Json)]) {
    let (view, complement) = get(&case.lens, &case.source()).unwrap();
    let mut failures = Vec::new();
    let unedited = case.write(&view);
    let found = case
        .put_json(&unedited, &complement)
        .map_err(|error| error.to_string());
    check(
        &mut failures,
        &format!("{} unedited", case.name()),
        &case.json["source"],
        &found,
    );
    for (edited, expected) in puts {
        let found = case
            .put_json(edited, &complement)
            .map_err(|error| error.to_string());
        if let Ok(found) = &found {
            let (again, _) = get(&case.lens, &case.read(found)).unwrap();
            if case.write(&again) != *edited {
                failures.push(format!(
                    "{} at {edited}: put {found}, which shows {}",
                    case.name(),
                    case.write(&again)
                ));
            }
        }
        check(
            &mut failures,
            &format!("{} at {edited}", case.name()),
            expected,
            &found,
        );
    }
    report(&failures);
}

/// A guard of the record's program reads a field of a nested record, which
/// that record's own program rewrites: the two fields are read together.
#[test]
fn a_guard_on_a_field_of_a_nested_record_is_searched_with_that_field() {
    let case = Case::new(serde_json::json!({
        "name": "guard through a nested record",
        "properties": {
            "a": {"type": "string"},
            "r": {"type": "object", "properties": {"b": {"type": "string"}}},
        },
        "programs": {
            ROOT: [{"case": [["r.b == \"XY\"", [
                {"rewrite": "a", "to": "concat a \"!\"", "from": "replace a \"!\" \"\""},
            ]]]}],
            "com.example.note:body.r": [{"rewrite": "b", "to": "lower b", "from": "upper b"}],
        },
        "source": {"a": "A", "r": {"b": "XY"}},
    }));
    let (view, _) = get(&case.lens, &case.source()).unwrap();
    assert_eq!(
        case.write(&view),
        serde_json::json!({"a": "A!", "r": {"b": "xy"}})
    );
    check_puts(
        &case,
        &[
            (
                serde_json::json!({"a": "B", "r": {"b": "cd"}}),
                serde_json::json!({"a": "B", "r": {"b": "CD"}}),
            ),
            (
                serde_json::json!({"a": "A!", "r": {"b": "cd"}}),
                serde_json::json!({"a": "A!", "r": {"b": "CD"}}),
            ),
            (
                serde_json::json!({"a": "B!", "r": {"b": "xy"}}),
                serde_json::json!({"a": "B", "r": {"b": "XY"}}),
            ),
        ],
    );
}

/// A guard reading a field by its qualified name `attrs.b` reads `b`.
#[test]
fn a_guard_on_a_qualified_name_is_searched_with_the_field_it_names() {
    let case = Case::new(serde_json::json!({
        "name": "guard through a qualified name",
        "at": ROOT,
        "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
        "transforms": [
            {"rewrite": "b", "to": "lower b", "from": "upper b"},
            {"case": [[
                {"Builtin": ["Eq", [{"Var": "attrs.b"}, {"Lit": {"Str": "xy"}}]]},
                [{"rewrite": "a", "to": "concat a \"!\"", "from": "replace a \"!\" \"\""}],
            ]]},
        ],
        "source": {"a": "A", "b": "XY"},
    }));
    let (view, _) = get(&case.lens, &case.source()).unwrap();
    assert_eq!(case.write(&view), serde_json::json!({"a": "A!", "b": "xy"}));
    check_puts(
        &case,
        &[
            (
                serde_json::json!({"a": "B", "b": "cd"}),
                serde_json::json!({"a": "B", "b": "CD"}),
            ),
            (
                serde_json::json!({"a": "A!", "b": "cd"}),
                serde_json::json!({"a": "A!", "b": "CD"}),
            ),
            (
                serde_json::json!({"a": "B!", "b": "xy"}),
                serde_json::json!({"a": "B", "b": "XY"}),
            ),
        ],
    );
}

/// Two computations of a field from its own value, each with an inverse,
/// are carried back one after the other: a serialized view, whose child
/// holds the result, puts back the source, and edits put back through both.
#[test]
fn a_field_computed_twice_from_itself_reads_back_from_its_serialization() {
    let twice = serde_json::json!([
        {"compute": "d", "to": "d + 1", "from": "d - 1"},
        {"compute": "d", "to": "d + 1", "from": "d - 1"},
    ]);
    let case = Case::new(serde_json::json!({
        "name": "computed twice",
        "at": ROOT,
        "properties": {"d": {"type": "integer"}},
        "transforms": twice,
        "source": {"d": 5},
    }));
    let (view, _) = get(&case.lens, &case.source()).unwrap();
    assert_eq!(case.write(&view), serde_json::json!({"d": 7}));
    check_puts(
        &case,
        &[(serde_json::json!({"d": 9}), serde_json::json!({"d": 7}))],
    );
    let beside = Case::new(serde_json::json!({
        "name": "computed twice beside another field",
        "at": ROOT,
        "properties": {"d": {"type": "integer"}, "e": {"type": "integer"}},
        "transforms": twice,
        "source": {"d": 5, "e": 1},
    }));
    check_puts(
        &beside,
        &[
            (
                serde_json::json!({"d": 7, "e": 4}),
                serde_json::json!({"d": 5, "e": 4}),
            ),
            (
                serde_json::json!({"d": 9, "e": 1}),
                serde_json::json!({"d": 7, "e": 1}),
            ),
        ],
    );
}
