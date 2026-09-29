//! Compile a [`ProtocolSpec`] into a [`Protocol`].
//!
//! Resolves schema and instance theory references (named, inline,
//! or composed), maps edge rules, and constructs the protocol
//! with default feature flags.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::compile_compose::compile_composition_spec;
use crate::compile_theory::compile_theory;
use crate::document::{ProtocolSpec, TheoryRef};
use crate::error::TheoryDslError;

/// Compile a [`ProtocolSpec`] into a [`Protocol`].
///
/// Theory references are resolved via the `resolver` callback.
/// Inline theory definitions and compositions are compiled on the fly.
///
/// # Errors
///
/// Returns errors from theory resolution, inline compilation, or
/// edge rule mapping.
pub fn compile_protocol<S: BuildHasher>(
    spec: &ProtocolSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<Protocol, TheoryDslError> {
    let schema_theory_name = resolve_theory_ref(
        &spec.schema_theory,
        resolver,
        local_theories,
        &spec.protocol,
    )?;
    let instance_theory_name = resolve_theory_ref(
        &spec.instance_theory,
        resolver,
        local_theories,
        &spec.protocol,
    )?;

    let edge_rules: Vec<EdgeRule> = spec
        .edge_rules
        .iter()
        .map(|er| EdgeRule {
            edge_kind: er.edge_kind.clone(),
            src_kinds: vec![er.src_kind.clone()],
            tgt_kinds: vec![er.tgt_kind.clone()],
        })
        .collect();

    Ok(Protocol {
        name: spec.protocol.clone(),
        schema_theory: schema_theory_name,
        instance_theory: instance_theory_name,
        schema_composition: None,
        instance_composition: None,
        edge_rules,
        obj_kinds: vec![],
        constraint_sorts: vec![],
        has_order: false,
        has_coproducts: false,
        has_recursion: false,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
        ..Default::default()
    })
}

/// Resolve a [`TheoryRef`] to a theory name.
///
/// For named refs, returns the name directly.
/// For inline definitions and compositions, compiles them and
/// returns the resulting theory name.
fn resolve_theory_ref<S: BuildHasher>(
    theory_ref: &TheoryRef,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
    protocol_name: &str,
) -> Result<String, TheoryDslError> {
    match theory_ref {
        TheoryRef::Named(name) => {
            // Verify the theory exists.
            if local_theories.contains_key(name) || resolver(name).is_some() {
                Ok(name.clone())
            } else {
                Err(TheoryDslError::TheoryNotFound {
                    name: name.clone(),
                    context: format!("protocol '{protocol_name}'"),
                })
            }
        }
        TheoryRef::Inline(spec) => {
            let theory = compile_theory(spec)?;
            Ok(theory.name.to_string())
        }
        TheoryRef::Composed(spec) => {
            let (theory, _) = compile_composition_spec(spec, resolver, local_theories)?;
            Ok(theory.name.to_string())
        }
    }
}
