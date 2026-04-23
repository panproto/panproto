//! Main compilation dispatcher and bundle compiler.
//!
//! Validates that a [`TheoryDocument`] has exactly one body variant,
//! then dispatches to the appropriate body-specific compiler. Also
//! provides [`compile_bundle`] for multi-definition files and
//! [`builtin_resolver`] for looking up panproto's built-in theories.

use std::collections::HashMap;

use panproto_gat::Theory;

use crate::compile_class::compile_class;
use crate::compile_compose::compile_composition;
use crate::compile_instance::compile_instance;
use crate::compile_morphism::compile_morphism;
use crate::compile_protocol::compile_protocol;
use crate::compile_theory::compile_theory;
use crate::document::{
    BundleSpec, ClassSpec, CompiledTheorySet, CompositionBody, InstanceSpec, MorphismSpec,
    ProtocolSpec, TheoryBody, TheoryDocument, TheorySpec,
};
use crate::error::TheoryDslError;

/// Compile a [`TheoryDocument`] into theories, morphisms, and protocols.
///
/// The `resolver` provides lookup for externally-defined theories (e.g.
/// built-in theories like `ThWType`, or theories from other packages).
///
/// # Errors
///
/// Returns errors from body validation, theory resolution, or
/// compilation of the specific body variant.
pub fn compile(
    doc: &TheoryDocument,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    match &doc.body {
        TheoryBody::Theory(spec) => compile_single_theory(&doc.id, spec),
        TheoryBody::Morphism(spec) => compile_single_morphism(&doc.id, spec, resolver),
        TheoryBody::Composition(body) => compile_single_composition(&doc.id, body, resolver),
        TheoryBody::Protocol(spec) => compile_single_protocol(&doc.id, spec, resolver),
        TheoryBody::Bundle(spec) => compile_bundle_inner(&doc.id, spec, resolver),
        TheoryBody::Class(spec) => compile_single_class(&doc.id, spec),
        TheoryBody::Instance(spec) => compile_single_instance(&doc.id, spec, resolver),
    }
}

/// Compile a [`BundleSpec`] with dependency ordering.
///
/// Processes theories first, then compositions, then morphisms,
/// then protocols. Each phase adds results to the local registry
/// so later phases can reference earlier definitions.
///
/// # Errors
///
/// Returns errors from any sub-compilation step, or
/// [`TheoryDslError::Duplicate`] for duplicate definitions.
pub fn compile_bundle(
    bundle: &BundleSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    compile_bundle_inner(&bundle.bundle, bundle, resolver)
}

/// Default resolver that knows about panproto's built-in theories.
///
/// Returns a closure that resolves names like `"ThGraph"`, `"ThConstraint"`,
/// `"ThMulti"`, `"ThWType"`, `"ThMeta"`, `"ThSimpleGraph"`, `"ThHypergraph"`,
/// `"ThInterface"`, `"ThFunctor"`, `"ThFlat"`, and `"ThGraphInstance"`.
pub fn builtin_resolver() -> impl Fn(&str) -> Option<Theory> {
    move |name: &str| -> Option<Theory> {
        match name {
            "ThGraph" => Some(panproto_protocols::theories::th_graph()),
            "ThConstraint" => Some(panproto_protocols::theories::th_constraint()),
            "ThMulti" => Some(panproto_protocols::theories::th_multi()),
            "ThWType" => Some(panproto_protocols::theories::th_wtype()),
            "ThMeta" => Some(panproto_protocols::theories::th_meta()),
            "ThSimpleGraph" => Some(panproto_protocols::theories::th_simple_graph()),
            "ThHypergraph" => Some(panproto_protocols::theories::th_hypergraph()),
            "ThInterface" => Some(panproto_protocols::theories::th_interface()),
            "ThFunctor" => Some(panproto_protocols::theories::th_functor()),
            "ThFlat" => Some(panproto_protocols::theories::th_flat()),
            "ThGraphInstance" => Some(panproto_protocols::theories::th_graph_instance()),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Internal dispatchers
// ═══════════════════════════════════════════════════════════════════

fn compile_single_theory(
    doc_id: &str,
    spec: &TheorySpec,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let theory = compile_theory(spec)?;
    let name = theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name, theory);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
    })
}

fn compile_single_morphism(
    doc_id: &str,
    spec: &MorphismSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let morphism = compile_morphism(spec, resolver)?;
    let name = morphism.name.to_string();
    let mut morphisms = HashMap::new();
    morphisms.insert(name, morphism);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories: HashMap::new(),
        morphisms,
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
    })
}

fn compile_single_composition(
    doc_id: &str,
    body: &CompositionBody,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let local = HashMap::new();
    let (theory, gat_spec) = compile_composition(body, resolver, &local)?;
    let name = theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name.clone(), theory);
    let mut composition_specs = HashMap::new();
    composition_specs.insert(name, gat_spec);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs,
    })
}

fn compile_single_protocol(
    doc_id: &str,
    spec: &ProtocolSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let local = HashMap::new();
    let protocol = compile_protocol(spec, resolver, &local)?;
    let name = protocol.name.clone();
    let mut protocols = HashMap::new();
    protocols.insert(name, protocol);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories: HashMap::new(),
        morphisms: HashMap::new(),
        protocols,
        composition_specs: HashMap::new(),
    })
}

fn compile_single_class(
    doc_id: &str,
    spec: &ClassSpec,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let theory = compile_class(spec)?;
    let name = theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name, theory);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
    })
}

fn compile_single_instance(
    doc_id: &str,
    spec: &InstanceSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let morphism = compile_instance(spec, resolver)?;
    let name = morphism.name.to_string();
    let mut morphisms = HashMap::new();
    morphisms.insert(name, morphism);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories: HashMap::new(),
        morphisms,
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
    })
}

fn compile_bundle_inner(
    doc_id: &str,
    bundle: &BundleSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let mut theories: HashMap<String, Theory> = HashMap::new();
    let mut morphisms = HashMap::new();
    let mut protocols = HashMap::new();
    let mut composition_specs = HashMap::new();

    // Phase 1: Compile all theories (no intra-bundle dependencies).
    for spec in &bundle.theories {
        if theories.contains_key(&spec.theory) {
            return Err(TheoryDslError::Duplicate {
                kind: "theory".to_owned(),
                name: spec.theory.clone(),
            });
        }
        let theory = compile_theory(spec)?;
        theories.insert(spec.theory.clone(), theory);
    }

    // Phase 2: Compile all compositions (may reference bundle theories).
    for spec in &bundle.compositions {
        if theories.contains_key(&spec.result) {
            return Err(TheoryDslError::Duplicate {
                kind: "theory (from composition)".to_owned(),
                name: spec.result.clone(),
            });
        }
        let body = CompositionBody {
            compose: spec.clone(),
        };
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let (theory, gat_spec) = compile_composition(&body, &combined, &theories)?;
        theories.insert(spec.result.clone(), theory);
        composition_specs.insert(spec.result.clone(), gat_spec);
    }

    // Phase 3: Compile all morphisms (reference theories by name).
    for spec in &bundle.morphisms {
        if morphisms.contains_key(&spec.morphism) {
            return Err(TheoryDslError::Duplicate {
                kind: "morphism".to_owned(),
                name: spec.morphism.clone(),
            });
        }
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let morphism = compile_morphism(spec, &combined)?;
        morphisms.insert(spec.morphism.clone(), morphism);
    }

    // Phase 4: Compile all protocols (reference theories by name or inline).
    for spec in &bundle.protocols {
        if protocols.contains_key(&spec.protocol) {
            return Err(TheoryDslError::Duplicate {
                kind: "protocol".to_owned(),
                name: spec.protocol.clone(),
            });
        }
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let protocol = compile_protocol(spec, &combined, &theories)?;
        protocols.insert(spec.protocol.clone(), protocol);
    }

    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms,
        protocols,
        composition_specs,
    })
}
