//! Compile a [`ProtocolSpec`] into a [`Protocol`].
//!
//! Resolves schema and instance theory references (named, inline,
//! or composed), maps edge rules, and constructs the protocol
//! with default feature flags.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::compile_compose::{ComposedTheory, compile_composition_spec_with_report};
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
    compile_protocol_with_compositions(spec, resolver, local_theories)
        .map(|compiled| compiled.protocol)
}

/// A compiled protocol with the compositions its inline composed theory
/// references built.
#[derive(Debug, Clone)]
pub struct CompiledProtocol {
    /// The protocol.
    pub protocol: Protocol,
    /// The composition of each inline composed theory reference, the
    /// schema theory's first, then the instance theory's.
    pub compositions: Vec<ComposedTheory>,
}

/// [`compile_protocol`], also returning each composition an inline
/// composed theory reference built, with its recipe and step reports.
///
/// # Errors
///
/// As [`compile_protocol`].
pub fn compile_protocol_with_compositions<S: BuildHasher>(
    spec: &ProtocolSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<CompiledProtocol, TheoryDslError> {
    let mut compositions = Vec::new();
    let schema_theory_name = resolve_theory_ref(
        &spec.schema_theory,
        resolver,
        local_theories,
        &spec.protocol,
        &mut compositions,
    )?;
    let instance_theory_name = resolve_theory_ref(
        &spec.instance_theory,
        resolver,
        local_theories,
        &spec.protocol,
        &mut compositions,
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

    let protocol = Protocol {
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
    };
    Ok(CompiledProtocol {
        protocol,
        compositions,
    })
}

/// Resolve a [`TheoryRef`] to a theory name.
///
/// For named refs, returns the name directly.
/// For inline definitions and compositions, compiles them and
/// returns the resulting theory name; an inline composition is pushed onto
/// `compositions`.
fn resolve_theory_ref<S: BuildHasher>(
    theory_ref: &TheoryRef,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
    protocol_name: &str,
    compositions: &mut Vec<ComposedTheory>,
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
            let composed = compile_composition_spec_with_report(spec, resolver, local_theories)?;
            let name = composed.theory.name.to_string();
            compositions.push(composed);
            Ok(name)
        }
    }
}
