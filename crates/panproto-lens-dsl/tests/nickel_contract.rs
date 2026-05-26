//! Regression tests for the bundled lens.ncl Nickel contract module.
//!
//! These tests exercise the full Nickel evaluation path to catch
//! reserved-word collisions, missing-definition errors,
//! and infinite-recursion regressions before they ship.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_lens_dsl::eval::eval_nickel;

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

#[test]
fn nickel_rename_step_round_trips() {
    let ncl = r#"
let L = import "panproto/lens.ncl" in
{
  id = "test-rename",
  source = "src",
  target = "tgt",
  steps = [L.rename "old_field" "new_field"],
} | L.Lens
"#;
    let doc = eval_nickel(ncl, &[]).expect("rename step should evaluate");
    assert_eq!(doc.id, "test-rename");
    assert_eq!(doc.steps.as_ref().map_or(0, Vec::len), 1);
}

#[test]
fn nickel_add_step_with_fallback() {
    let ncl = r#"
let L = import "panproto/lens.ncl" in
{
  id = "test-add",
  source = "s",
  target = "t",
  steps = [L.add "count" "integer" 0],
} | L.Lens
"#;
    let doc = eval_nickel(ncl, &[]).expect("add step with fallback should evaluate");
    assert_eq!(doc.id, "test-add");
    assert_eq!(doc.steps.as_ref().map_or(0, Vec::len), 1);
}

#[test]
fn nickel_remove_step() {
    let ncl = r#"
let L = import "panproto/lens.ncl" in
{
  id = "test-remove",
  source = "s",
  target = "t",
  steps = [L.remove "old_field"],
} | L.Lens
"#;
    let doc = eval_nickel(ncl, &[]).expect("remove step should evaluate");
    assert_eq!(doc.steps.as_ref().map_or(0, Vec::len), 1);
}

#[test]
fn nickel_rule_with_pattern() {
    let ncl = r#"
let L = import "panproto/lens.ncl" in
{
  id = "test-rules",
  source = "s",
  target = "t",
  rules = [L.map_name "old" "new", L.drop_feature "obsolete"],
} | L.Lens
"#;
    let doc = eval_nickel(ncl, &[]).expect("rules with pattern/replace should evaluate");
    assert_eq!(doc.rules.as_ref().map_or(0, Vec::len), 2);
}

#[test]
fn nickel_multiple_combinators() {
    let ncl = r#"
let L = import "panproto/lens.ncl" in
{
  id = "multi",
  source = "s",
  target = "t",
  steps = [
    L.rename "a" "b",
    L.remove "c",
    L.add "d" "string" "",
    L.hoist "parent" "mid" "child",
    L.compute "x" "y + 1",
    L.apply "z" "z * 2",
  ],
} | L.Lens
"#;
    let doc = eval_nickel(ncl, &[]).expect("multiple combinators should evaluate");
    assert_eq!(doc.steps.as_ref().map_or(0, Vec::len), 6);
}

#[test]
fn nickel_all_exports_accessible() {
    with_big_stack(|| {
        let ncl = r#"
let L = import "panproto/lens.ncl" in
let _ = L.Lens in
let _ = L.Step in
let _ = L.Rule in
let _ = L.FeaturePattern in
let _ = L.Replacement in
let _ = L.ComposeSpec in
let _ = L.AutoSpec in
let _ = L.HintSpec in
let _ = L.Constraint in
let _ = L.Coercion in
let _ = L.iso in
let _ = L.retraction in
let _ = L.projection in
let _ = L.opaque in
let _ = L.remove in
let _ = L.rename in
let _ = L.add in
let _ = L.add_computed in
let _ = L.apply in
let _ = L.apply_invertible in
let _ = L.compute in
let _ = L.compute_invertible in
let _ = L.hoist in
let _ = L.nest in
let _ = L.map_items in
let _ = L.pullback in
let _ = L.coerce in
let _ = L.coerce_invertible in
let _ = L.merge in
let _ = L.add_sort in
let _ = L.drop_sort in
let _ = L.rename_sort in
let _ = L.add_op in
let _ = L.drop_op in
let _ = L.rename_op in
let _ = L.add_equation in
let _ = L.drop_equation in
let _ = L.anchor in
let _ = L.scope in
let _ = L.exclude_targets in
let _ = L.exclude_sources in
let _ = L.prefer_same_edge_name in
let _ = L.prefer_similar_name in
let _ = L.counter_fields in
let _ = L.string_fields in
let _ = L.map_name in
let _ = L.drop_feature in
{
  id = "export-check",
  source = "s",
  target = "t",
  steps = [],
} | L.Lens
"#;
        eval_nickel(ncl, &[]).expect("all contract exports should be accessible");
    });
}
