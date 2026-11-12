//! Compile a [`ProtocolSpec`] into a [`Protocol`].
//!
//! Resolves schema and instance theory references (named, inline,
//! or composed), maps edge rules, and constructs the protocol
//! with every feature flag off.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::{CompositionSpec, Theory};
use panproto_schema::{EdgeRule, Protocol};

use crate::compile_compose::{ComposedTheory, compile_composition_spec_with_report};
use crate::compile_theory::compile_theory_with_resolver;
use crate::document::{ProtocolSpec, TheoryRef};
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

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

/// A compiled protocol with the theories its inline theory references and
/// the compositions its inline composed theory references built.
#[derive(Debug, Clone)]
pub struct CompiledProtocol {
    /// The protocol.
    pub protocol: Protocol,
    /// The theory of each inline theory reference, the schema theory's
    /// first, then the instance theory's.
    pub theories: Vec<Theory>,
    /// The composition of each inline composed theory reference, the
    /// schema theory's first, then the instance theory's.
    pub compositions: Vec<ComposedTheory>,
}

/// [`compile_protocol`], also returning each theory an inline theory
/// reference compiled and each composition an inline composed theory
/// reference built, with its recipe and step reports.
///
/// A named reference must name a theory of `local_theories` or of the
/// resolver; an inline theory compiles as a theory body whose parents and
/// imports resolve likewise; an inline composition compiles as a
/// composition body, and its recipe is the protocol's composition for that
/// theory. Every refusal is located under the reference's member,
/// `schema_theory` or `instance_theory`.
///
/// # Errors
///
/// As [`compile_protocol`].
pub fn compile_protocol_with_compositions<S: BuildHasher>(
    spec: &ProtocolSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<CompiledProtocol, TheoryDslError> {
    let mut built = Built::default();
    let combined = |name: &str| -> Option<Theory> {
        local_theories.get(name).cloned().or_else(|| resolver(name))
    };
    let (schema_theory, schema_composition) = built.resolve(
        &spec.schema_theory,
        "schema_theory",
        &combined,
        local_theories,
        &spec.protocol,
    )?;
    let (instance_theory, instance_composition) = built.resolve(
        &spec.instance_theory,
        "instance_theory",
        &combined,
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

    let protocol = Protocol {
        name: spec.protocol.clone(),
        schema_theory,
        instance_theory,
        schema_composition,
        instance_composition,
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
        theories: built.theories,
        compositions: built.compositions,
    })
}

/// The theories and compositions a protocol's inline references build.
#[derive(Default)]
struct Built {
    theories: Vec<Theory>,
    compositions: Vec<ComposedTheory>,
}

impl Built {
    /// Resolve the theory reference at member `field` to a theory name and,
    /// for an inline composition, its recipe.
    fn resolve<S: BuildHasher>(
        &mut self,
        theory_ref: &TheoryRef,
        field: &str,
        resolver: &dyn Fn(&str) -> Option<Theory>,
        local_theories: &HashMap<String, Theory, S>,
        protocol_name: &str,
    ) -> Result<(String, Option<CompositionSpec>), TheoryDslError> {
        let at = DocumentPath::root().key(field);
        match theory_ref {
            TheoryRef::Named(name) => {
                if resolver(name).is_some() {
                    Ok((name.clone(), None))
                } else {
                    Err(TheoryDslError::TheoryNotFound {
                        name: name.clone(),
                        context: format!("protocol '{protocol_name}'"),
                    }
                    .within(&at))
                }
            }
            TheoryRef::Inline(spec) => {
                let theory =
                    compile_theory_with_resolver(spec, resolver).map_err(|e| e.within(&at))?;
                let name = theory.name.to_string();
                self.theories.push(theory);
                Ok((name, None))
            }
            TheoryRef::Composed(spec) => {
                let composed = compile_composition_spec_with_report(spec, resolver, local_theories)
                    .map_err(|e| e.within(&at))?;
                let name = composed.theory.name.to_string();
                let recipe = composed.spec.clone();
                self.compositions.push(composed);
                Ok((name, Some(recipe)))
            }
        }
    }
}
