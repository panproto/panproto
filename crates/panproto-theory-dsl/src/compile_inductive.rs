//! Compile an [`InductiveSpec`] into a [`Theory`].
//!
//! An inductive declaration desugars to a theory with one closed sort
//! whose constructor list enumerates every declared constructor, plus
//! one operation per constructor. The output sort of every constructor
//! is the inductive sort applied to the spec's `params` (as variable
//! terms), so recursive constructors like `succ(n: Nat) : Nat` and
//! parameterized ones like `cons(x: A, xs: List(A)) : List(A)` share a
//! single schema.

use panproto_gat::Theory;

use crate::compile_theory::compile_theory;
use crate::document::{InductiveSpec, OpSpec, ParamSpec, SortKindSpec, SortSpec, TheorySpec};
use crate::error::TheoryDslError;
use crate::path::{DocumentPath, PathSegment};

/// Compile an [`InductiveSpec`] into a [`Theory`] by expanding to a
/// `TheorySpec` with one closed sort and one op per constructor.
///
/// # Errors
///
/// Returns errors from [`compile_theory`] (parse failure, typecheck
/// violation, unknown sort reference, etc.), located at the member of the
/// inductive document the refused declaration was built from (see
/// [`inductive_to_theory_spec`]).
pub fn compile_inductive(spec: &InductiveSpec) -> Result<Theory, TheoryDslError> {
    let theory_spec = inductive_to_theory_spec(spec);
    compile_theory(&theory_spec).map_err(|error| located_in_inductive(error, spec, &theory_spec))
}

/// `error`, located at the member of `spec` that the declaration of
/// `theory_spec` its path names was built from: a parameter sort at the
/// first parameter of that sort, the inductive sort's parameter at that
/// parameter, its closure at `constructors`, a constructor at its entry,
/// a constructor's leading implicit input at the parameter it copies and
/// its other inputs at the constructor's own inputs. Anything else is
/// located at the root.
fn located_in_inductive(
    error: TheoryDslError,
    spec: &InductiveSpec,
    theory_spec: &TheorySpec,
) -> TheoryDslError {
    let TheoryDslError::At { path, error } = error else {
        return error;
    };
    let root = DocumentPath::root();
    let rest = |from: usize| DocumentPath(path.0.get(from..).unwrap_or_default().to_vec());
    let parameters = spec.params.len();
    let located = match path.0.as_slice() {
        [PathSegment::Key(sorts), PathSegment::Index(k), ..] if sorts == "sorts" => {
            let inductive_sort = theory_spec.sorts.len().saturating_sub(1);
            if *k < inductive_sort {
                let name = &theory_spec.sorts[*k].name;
                spec.params
                    .iter()
                    .position(|p| p.sort.trim() == name)
                    .map_or_else(DocumentPath::root, |j| {
                        root.key("params").index(j).key("sort")
                    })
            } else {
                match path.0.get(2) {
                    Some(PathSegment::Key(member)) if member == "params" => {
                        root.key("params").join(&rest(3))
                    }
                    Some(PathSegment::Key(member)) if member == "closed" => {
                        root.key("constructors")
                    }
                    _ => root,
                }
            }
        }
        [PathSegment::Key(ops), PathSegment::Index(i), tail @ ..] if ops == "ops" => {
            let constructor = root.key("constructors").index(*i);
            match tail {
                [PathSegment::Key(inputs), PathSegment::Index(j), ..] if inputs == "inputs" => {
                    if *j < parameters {
                        root.key("params").index(*j).join(&rest(4))
                    } else {
                        constructor
                            .key("inputs")
                            .index(j - parameters)
                            .join(&rest(4))
                    }
                }
                [PathSegment::Key(name), ..] if name == "name" => constructor.key("name"),
                _ => constructor,
            }
        }
        _ => root,
    };
    TheoryDslError::At {
        path: located,
        error,
    }
}

/// Build the `TheorySpec` that an `InductiveSpec` desugars to.
#[must_use]
pub fn inductive_to_theory_spec(spec: &InductiveSpec) -> TheorySpec {
    let constructors: Vec<String> = spec.constructors.iter().map(|c| c.name.clone()).collect();
    // Collect every bare sort name referenced by the params (e.g. `A`
    // in `List<A>`) that isn't the inductive sort itself, and declare
    // each as a simple structural sort so the inductive declaration
    // stands on its own as a theory.
    let mut param_sorts: Vec<SortSpec> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &spec.params {
        let base = p.sort.trim();
        if base != spec.inductive && !base.contains('(') && seen.insert(base.to_owned()) {
            param_sorts.push(SortSpec {
                name: base.to_owned(),
                params: Vec::new(),
                kind: SortKindSpec::Structural,
                closed: None,
            });
        }
    }
    let sort = SortSpec {
        name: spec.inductive.clone(),
        params: spec.params.clone(),
        kind: SortKindSpec::Structural,
        closed: Some(constructors),
    };
    // Output sort for every constructor: the inductive's name applied to
    // its params as variable terms, or a bare name when there are no
    // params.
    let output_sort = if spec.params.is_empty() {
        spec.inductive.clone()
    } else {
        let arg_list = spec
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name}({arg_list})", name = spec.inductive)
    };
    let ops: Vec<OpSpec> = spec
        .constructors
        .iter()
        .map(|c| {
            let inputs = build_constructor_inputs(&spec.params, &c.inputs);
            OpSpec {
                name: c.name.clone(),
                input: None,
                inputs: if inputs.is_empty() {
                    None
                } else {
                    Some(inputs)
                },
                output: output_sort.clone(),
            }
        })
        .collect();
    let mut all_sorts = param_sorts;
    all_sorts.push(sort);
    TheorySpec {
        theory: spec.inductive.clone(),
        extends: Vec::new(),
        imports: Vec::new(),
        sorts: all_sorts,
        ops,
        equations: Vec::new(),
        directed_equations: Vec::new(),
        policies: Vec::new(),
    }
}

/// Prepend the inductive's parameters (as implicit inputs) to the
/// constructor's declared inputs. This is what turns a surface form like
/// `cons(x: A, xs: List(A)) : List(A)` into a well-typed operation whose
/// input list implicitly binds `A`.
fn build_constructor_inputs(
    inductive_params: &[ParamSpec],
    ctor_inputs: &[ParamSpec],
) -> Vec<ParamSpec> {
    let mut out = Vec::with_capacity(inductive_params.len() + ctor_inputs.len());
    for p in inductive_params {
        out.push(ParamSpec {
            name: p.name.clone(),
            sort: p.sort.clone(),
            implicit: true,
        });
    }
    out.extend(ctor_inputs.iter().cloned());
    out
}
