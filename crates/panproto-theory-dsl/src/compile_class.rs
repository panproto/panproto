//! Compile a [`ClassSpec`] into a [`Theory`].
//!
//! A class declaration desugars to a theory whose sorts are the declared
//! parameter names (each a simple structural sort), whose operations are
//! the listed signatures, and whose equations are the listed axioms.

use panproto_gat::Theory;

use crate::compile_theory::compile_theory;
use crate::document::{ClassSpec, SortKindSpec, SortSpec, TheorySpec};
use crate::error::TheoryDslError;
use crate::path::{DocumentPath, PathSegment};

/// Compile a [`ClassSpec`] into a [`Theory`] via the standard theory
/// compiler.
///
/// A class has no parents or imports: its sorts are its parameters, so a
/// signature or axiom may mention no other sort.
///
/// # Errors
///
/// Returns any error from [`compile_theory`] (parse failure, typecheck
/// violation, etc.), located at the class member it concerns: a parameter
/// at its `params` entry, a signature under `signatures` and an axiom under
/// `axioms`.
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
        imports: Vec::new(),
        sorts,
        ops: spec.signatures.clone(),
        equations: spec.axioms.clone(),
        directed_equations: Vec::new(),
        policies: Vec::new(),
    };

    compile_theory(&theory_spec).map_err(located_in_class)
}

/// `error`, located at the class member that the theory member its path
/// names was built from.
fn located_in_class(error: TheoryDslError) -> TheoryDslError {
    let TheoryDslError::At { path, error } = error else {
        return error;
    };
    let mut segments = path.0;
    if let Some(PathSegment::Key(first)) = segments.first_mut() {
        let member = match first.as_str() {
            "sorts" => "params",
            "ops" => "signatures",
            "equations" => "axioms",
            _ => "",
        };
        if !member.is_empty() {
            member.clone_into(first);
        }
        if member == "params" {
            // A parameter is a name: a refusal of its sort is the name's.
            segments.truncate(2);
        }
    }
    TheoryDslError::At {
        path: DocumentPath(segments),
        error,
    }
}
