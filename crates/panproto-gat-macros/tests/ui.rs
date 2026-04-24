//! Compile-fail UI tests for malformed proc-macro input.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/class_missing_body.rs");
    t.compile_fail("tests/ui/class_missing_params.rs");
    t.compile_fail("tests/ui/instance_missing_target.rs");
    t.compile_fail("tests/ui/derive_theory_bare_attr.rs");
}
