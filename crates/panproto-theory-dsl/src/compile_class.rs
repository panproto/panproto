//! Compile a [`ClassSpec`] into a [`Theory`].
//!
//! A class declaration desugars to a theory whose sorts are the declared
//! parameter names (each a simple structural sort), whose operations are
//! the listed signatures, and whose equations are the listed axioms.

use panproto_gat::Theory;

use crate::compile_theory::compile_theory;
use crate::document::{ClassSpec, SortKindSpec, SortSpec, TheorySpec};

/// Compile a [`ClassSpec`] into a [`Theory`] via the standard theory
/// compiler.
///
/// # Errors
///
/// Returns any error from [`compile_theory`] (parse failure, typecheck
/// violation, etc.).
pub fn compile_class(spec: &ClassSpec) -> Result<Theory, crate::error::TheoryDslError> {
    let sorts = spec
        .params
        .iter()
        .map(|p| SortSpec {
            name: p.clone(),
            params: Vec::new(),
            kind: SortKindSpec::Structural,
            closed: None,
        })
        .collect();

    let theory_spec = TheorySpec {
        theory: spec.class.clone(),
        extends: Vec::new(),
        sorts,
        ops: spec.signatures.clone(),
        equations: spec.axioms.clone(),
        directed_equations: Vec::new(),
        policies: Vec::new(),
    };

    compile_theory(&theory_spec)
}
