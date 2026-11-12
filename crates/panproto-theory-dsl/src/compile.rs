//! Main compilation dispatcher and bundle compiler.
//!
//! A [`TheoryDocument`] carries exactly one body variant by
//! construction: [`TheoryBody`] is an untagged enum, so a document
//! deserializes to exactly one variant and the compiler simply matches
//! on it, dispatching to the appropriate body-specific compiler. Also
//! provides [`compile_bundle`] for multi-definition files and
//! [`builtin_resolver`] for looking up panproto's built-in theories.

use std::collections::HashMap;

use panproto_gat::{CompositionReport, CompositionSpec, Theory};
use panproto_schema::canonical_theory_bytes;

use crate::compile_class::compile_class;
use crate::compile_compose::{ComposedTheory, compile_composition_with_report};
use crate::compile_inductive::compile_inductive;
use crate::compile_instance::compile_instance;
use crate::compile_morphism::compile_morphism;
use crate::compile_protocol::compile_protocol_with_compositions;
use crate::document::{
    BundleSpec, ClassSpec, CompiledTheorySet, CompositionBody, InductiveSpec, InstanceSpec,
    MorphismSpec, ProtocolSpec, TheoryBody, TheoryDocument, TheoryRef, TheorySpec,
};
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

pub use panproto_dsl_eval::SourceFormat;

/// Attach the source text and the byte range of the refused element to a
/// refusal.
///
/// A refusal that names a document path is located with
/// [`panproto_dsl_eval::source::locate`]: by the byte range of the value in
/// JSON text and by its first character in YAML text; a refusal of Nickel
/// text, or of text in which the path names no value, is returned as it
/// is, still naming its path.
fn spanned(error: TheoryDslError, source: &str, format: SourceFormat) -> TheoryDslError {
    let Some(path) = error.path().cloned() else {
        return error;
    };
    match panproto_dsl_eval::source::locate(source, format, &path) {
        Some((offset, length)) => TheoryDslError::Spanned {
            path: Some(path),
            src: source.to_owned(),
            span: miette::SourceSpan::new(offset.into(), length),
            error: Box::new(error),
        },
        None => error,
    }
}

/// Read a document from its source text.
///
/// JSON and YAML text is parsed to a value, and Nickel text evaluated to
/// one, and the value is read by [`crate::shape::read_document`].
///
/// # Errors
///
/// A JSON syntax error is [`TheoryDslError::Spanned`] around a
/// [`TheoryDslError::Json`], its span the one byte at which the JSON reader
/// stopped (empty at the end of the text). A YAML syntax error is
/// [`TheoryDslError::Spanned`] around a [`TheoryDslError::Yaml`], its span the
/// character at which the YAML reader stopped. A Nickel evaluation error is
/// [`TheoryDslError::NickelEvalSpanned`] when Nickel's diagnostic locates
/// it in `source` and [`TheoryDslError::NickelEval`] otherwise. A value
/// the grammar refuses is [`TheoryDslError::Shape`], wrapped in
/// [`TheoryDslError::Spanned`] for JSON and YAML text: the byte range of the
/// refused value in JSON text, its first character in YAML text.
pub fn read_source(source: &str, format: SourceFormat) -> Result<TheoryDocument, TheoryDslError> {
    let value = match format {
        SourceFormat::Json => serde_json::from_str::<serde_json::Value>(source).map_err(|e| {
            let (offset, length) = panproto_dsl_eval::source::json_syntax_span(source, &e);
            TheoryDslError::Spanned {
                path: None,
                src: source.to_owned(),
                span: miette::SourceSpan::new(offset.into(), length),
                error: Box::new(TheoryDslError::Json(e)),
            }
        })?,
        SourceFormat::Yaml => crate::eval::yaml_value(source).map_err(|e| {
            match panproto_dsl_eval::source::yaml_syntax_span(source) {
                Some((offset, length)) => TheoryDslError::Spanned {
                    path: None,
                    src: source.to_owned(),
                    span: miette::SourceSpan::new(offset.into(), length),
                    error: Box::new(e),
                },
                None => e,
            }
        })?,
        SourceFormat::Nickel => crate::eval::nickel_value(source, &[])?,
    };
    crate::shape::read_document(&value).map_err(|e| spanned(e, source, format))
}

/// Read a document from its source text and compile it with [`compile`],
/// locating every refusal in `source`.
///
/// # Errors
///
/// The errors of [`read_source`] and of [`compile`]. A compile refusal
/// names the document path of the element refused; for JSON and YAML text
/// it is wrapped in [`TheoryDslError::Spanned`] whose span is the byte
/// range of that element's value as written in JSON, and its first
/// character in YAML.
pub fn compile_source(
    source: &str,
    format: SourceFormat,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let doc = read_source(source, format)?;
    compile_with_source(&doc, source, format, resolver)
}

/// Compile a document read from `source`, locating every refusal in it.
///
/// # Errors
///
/// The errors of [`compile`]; for JSON and YAML text a refusal that names a
/// document path is wrapped in [`TheoryDslError::Spanned`] whose span is
/// the byte range of that element's value in JSON `source`, and its first
/// character in YAML `source`.
pub fn compile_with_source(
    doc: &TheoryDocument,
    source: &str,
    format: SourceFormat,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    compile(doc, resolver).map_err(|e| spanned(e, source, format))
}

/// Compile a [`TheoryDocument`], enforcing coercion laws on every
/// compiled theory.
///
/// Dispatches on the document body and then runs the sample-based
/// coercion-law check, drawing samples from the built-in
/// [`CoercionSampleRegistry::with_defaults`](panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults),
/// on each theory in the result. A theory whose directed equations
/// declare a coercion class that its `impl`/`inverse` pair does not
/// satisfy on the default samples is rejected with
/// [`TheoryDslError::CoercionLawViolation`]. Documents that declare no
/// directed equations pass the check vacuously.
///
/// Use [`compile_unchecked`] to skip the law check, or
/// [`compile_with_registry`] to draw samples from a domain-specific
/// registry.
///
/// # Errors
///
/// Returns errors from body validation, theory resolution, compilation
/// of the specific body variant, or
/// [`TheoryDslError::CoercionLawViolation`] when a declared coercion
/// class is falsified on the default samples.
pub fn compile(
    doc: &TheoryDocument,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let registry = panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults();
    compile_with_registry(doc, resolver, &registry)
}

/// Compile a [`TheoryDocument`], enforcing coercion laws against a
/// caller-supplied sample registry.
///
/// Behaves like [`compile`] but draws coercion-law samples from
/// `registry` instead of the built-in defaults, letting callers pin
/// domain-specific edge cases. Samples are bound under the free
/// variable name `"x"`.
///
/// # Errors
///
/// Same as [`compile`].
pub fn compile_with_registry(
    doc: &TheoryDocument,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let set = compile_unchecked(doc, resolver)?;
    // Check theories in name order so the surfaced violation is
    // deterministic across runs regardless of map iteration order.
    let mut names: Vec<&String> = set.theories.keys().collect();
    names.sort();
    for name in names {
        crate::compile_theory::enforce_coercion_laws(&set.theories[name], registry, "x")
            .map_err(|e| e.within(&theory_path(&doc.body, name)))?;
    }
    Ok(set)
}

/// Compile a [`TheoryDocument`] without the coercion-law check.
///
/// Dispatches on the document body and returns the compiled set
/// without consulting declared coercion classes. Callers that have
/// already validated their coercion classes, or that intentionally
/// carry declarations they do not want sample-checked, use this to
/// skip the check that [`compile`] runs by default.
///
/// # Errors
///
/// Returns errors from body validation, theory resolution, or
/// compilation of the specific body variant. Every refusal names a
/// document path: it is a [`TheoryDslError::At`] or a
/// [`TheoryDslError::Shape`] whose path is that of the element refused, or
/// the root when the refusal concerns the body as a whole.
pub fn compile_unchecked(
    doc: &TheoryDocument,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let compiled = match &doc.body {
        TheoryBody::Theory(spec) => compile_single_theory(&doc.id, spec, resolver),
        TheoryBody::Morphism(spec) => compile_single_morphism(&doc.id, spec, resolver),
        TheoryBody::Composition(body) => compile_single_composition(&doc.id, body, resolver),
        TheoryBody::Protocol(spec) => compile_single_protocol(&doc.id, spec, resolver),
        TheoryBody::Bundle(spec) => compile_bundle_inner(&doc.id, spec, resolver),
        TheoryBody::Class(spec) => compile_single_class(&doc.id, spec),
        TheoryBody::Instance(spec) => compile_single_instance(&doc.id, spec, resolver),
        TheoryBody::Inductive(spec) => compile_single_inductive(&doc.id, spec),
    };
    compiled.map_err(|e| e.within(&DocumentPath::root()))
}

/// The path of the declaration of theory `name` in a document body: a
/// bundle's theory or composition by its index, and the body itself
/// otherwise.
fn theory_path(body: &TheoryBody, name: &str) -> DocumentPath {
    let root = DocumentPath::root();
    match body {
        TheoryBody::Protocol(protocol) => {
            inline_reference(protocol, name).map_or(root, |field| DocumentPath::root().key(field))
        }
        TheoryBody::Bundle(bundle) => {
            if let Some(index) = bundle.theories.iter().position(|t| t.theory == name) {
                return root.key("theories").index(index);
            }
            if let Some(index) = bundle.compositions.iter().position(|c| c.result == name) {
                return root.key("compositions").index(index);
            }
            bundle
                .protocols
                .iter()
                .enumerate()
                .find_map(|(index, protocol)| {
                    inline_reference(protocol, name)
                        .map(|field| root.key("protocols").index(index).key(field))
                })
                .unwrap_or(root)
        }
        _ => root,
    }
}

/// The member of `protocol`, `schema_theory` or `instance_theory`, whose
/// inline theory or composition is named `name`.
fn inline_reference(protocol: &ProtocolSpec, name: &str) -> Option<&'static str> {
    let builds = |reference: &TheoryRef| match reference {
        TheoryRef::Named(_) => false,
        TheoryRef::Inline(spec) => spec.theory == name,
        TheoryRef::Composed(spec) => spec.result == name,
    };
    if builds(&protocol.schema_theory) {
        Some("schema_theory")
    } else if builds(&protocol.instance_theory) {
        Some("instance_theory")
    } else {
        None
    }
}

fn compile_single_inductive(
    doc_id: &str,
    spec: &InductiveSpec,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let theory = compile_inductive(spec)?;
    let name = theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name, theory);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
        composition_reports: HashMap::new(),
    })
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
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let theory = crate::compile_theory::compile_theory_with_resolver(spec, resolver)?;
    let name = theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name, theory);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs: HashMap::new(),
        composition_reports: HashMap::new(),
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
        composition_reports: HashMap::new(),
    })
}

fn compile_single_composition(
    doc_id: &str,
    body: &CompositionBody,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let local = HashMap::new();
    let composed = compile_composition_with_report(body, resolver, &local)
        .map_err(|e| e.within(&DocumentPath::root().key("compose")))?;
    let name = composed.theory.name.to_string();
    let mut theories = HashMap::new();
    theories.insert(name.clone(), composed.theory);
    let mut composition_specs = HashMap::new();
    composition_specs.insert(name.clone(), composed.spec);
    let mut composition_reports = HashMap::new();
    composition_reports.insert(name, composed.reports);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories,
        morphisms: HashMap::new(),
        protocols: HashMap::new(),
        composition_specs,
        composition_reports,
    })
}

fn compile_single_protocol(
    doc_id: &str,
    spec: &ProtocolSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let local = HashMap::new();
    let compiled = compile_protocol_with_compositions(spec, resolver, &local)?;
    let mut recorded = Compositions::default();
    recorded.record_protocol_theories(compiled.theories, compiled.compositions)?;
    let name = compiled.protocol.name.clone();
    let mut protocols = HashMap::new();
    protocols.insert(name, compiled.protocol);
    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories: recorded.theories,
        morphisms: HashMap::new(),
        protocols,
        composition_specs: recorded.specs,
        composition_reports: recorded.reports,
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
        composition_reports: HashMap::new(),
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
        composition_reports: HashMap::new(),
    })
}

fn compile_bundle_inner(
    doc_id: &str,
    bundle: &BundleSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let mut recorded = Compositions::default();
    let mut morphisms = HashMap::new();
    let mut protocols = HashMap::new();

    let root = DocumentPath::root();
    // Phase 1: Compile the theories in listed order; each sees the theories
    // listed before it.
    for (index, spec) in bundle.theories.iter().enumerate() {
        let at = root.key("theories").index(index);
        let theories = &mut recorded.theories;
        if theories.contains_key(&spec.theory) {
            return Err(TheoryDslError::Duplicate {
                kind: "theory".to_owned(),
                name: spec.theory.clone(),
            }
            .within(&at.key("theory")));
        }
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let theory = crate::compile_theory::compile_theory_with_resolver(spec, &combined)
            .map_err(|e| e.within(&at))?;
        theories.insert(spec.theory.clone(), theory);
    }

    // Phase 2: Compile all compositions (may reference bundle theories).
    for (index, spec) in bundle.compositions.iter().enumerate() {
        let at = root.key("compositions").index(index);
        let theories = &recorded.theories;
        if theories.contains_key(&spec.result) {
            return Err(TheoryDslError::Duplicate {
                kind: "theory (from composition)".to_owned(),
                name: spec.result.clone(),
            }
            .within(&at.key("result")));
        }
        let body = CompositionBody {
            compose: spec.clone(),
        };
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let composed = compile_composition_with_report(&body, &combined, theories)
            .map_err(|e| e.within(&at))?;
        recorded.insert(composed);
    }

    // Phase 3: Compile all morphisms (reference theories by name).
    for (index, spec) in bundle.morphisms.iter().enumerate() {
        let at = root.key("morphisms").index(index);
        let theories = &recorded.theories;
        if morphisms.contains_key(&spec.morphism) {
            return Err(TheoryDslError::Duplicate {
                kind: "morphism".to_owned(),
                name: spec.morphism.clone(),
            }
            .within(&at.key("morphism")));
        }
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let morphism = compile_morphism(spec, &combined).map_err(|e| e.within(&at))?;
        morphisms.insert(spec.morphism.clone(), morphism);
    }

    // Phase 4: Compile all protocols (reference theories by name or inline).
    // Every protocol sees the theories of phases 1 and 2, and the inline
    // compositions are recorded once all protocols are compiled, so no
    // protocol can name another's inline composition, whatever their order.
    let mut inline = Vec::new();
    let mut inline_theories = Vec::new();
    for (index, spec) in bundle.protocols.iter().enumerate() {
        let at = root.key("protocols").index(index);
        let theories = &recorded.theories;
        if protocols.contains_key(&spec.protocol) {
            return Err(TheoryDslError::Duplicate {
                kind: "protocol".to_owned(),
                name: spec.protocol.clone(),
            }
            .within(&at.key("protocol")));
        }
        let combined = |name: &str| -> Option<Theory> {
            theories.get(name).cloned().or_else(|| resolver(name))
        };
        let compiled = compile_protocol_with_compositions(spec, &combined, theories)
            .map_err(|e| e.within(&at))?;
        inline.extend(compiled.compositions);
        inline_theories.extend(compiled.theories);
        protocols.insert(spec.protocol.clone(), compiled.protocol);
    }
    recorded
        .record_protocol_theories(inline_theories, inline)
        .map_err(|e| e.within(&root.key("protocols")))?;

    Ok(CompiledTheorySet {
        id: doc_id.to_owned(),
        theories: recorded.theories,
        morphisms,
        protocols,
        composition_specs: recorded.specs,
        composition_reports: recorded.reports,
    })
}

/// The theories a document compiles, with the recipe and the step reports
/// of each composed one, keyed by theory name.
#[derive(Default)]
struct Compositions {
    theories: HashMap<String, Theory>,
    specs: HashMap<String, CompositionSpec>,
    reports: HashMap<String, Vec<CompositionReport>>,
}

impl Compositions {
    /// Record `composed` under its theory's name.
    fn insert(&mut self, composed: ComposedTheory) {
        let name = composed.theory.name.to_string();
        self.theories.insert(name.clone(), composed.theory);
        self.specs.insert(name.clone(), composed.spec);
        self.reports.insert(name, composed.reports);
    }

    /// Record the theories and compositions of a protocol's inline theory
    /// references, the theories first.
    ///
    /// # Errors
    ///
    /// As [`Self::record_inline_theory`] and [`Self::record_inline`].
    fn record_protocol_theories(
        &mut self,
        theories: Vec<Theory>,
        compositions: Vec<ComposedTheory>,
    ) -> Result<(), TheoryDslError> {
        for theory in theories {
            self.record_inline_theory(theory)?;
        }
        for composed in compositions {
            self.record_inline(composed)?;
        }
        Ok(())
    }

    /// Record `theory`, the theory of an inline theory reference of a
    /// protocol, under its name.
    ///
    /// A name already recorded is kept when it names a theory that is not
    /// a composition and has the same canonical bytes as `theory` (the same
    /// theory written again), and is refused otherwise.
    ///
    /// # Errors
    ///
    /// [`TheoryDslError::Duplicate`] when the name is recorded with another
    /// theory or as a composition.
    fn record_inline_theory(&mut self, theory: Theory) -> Result<(), TheoryDslError> {
        let name = theory.name.to_string();
        let Some(existing) = self.theories.get(&name) else {
            self.theories.insert(name, theory);
            return Ok(());
        };
        if !self.specs.contains_key(&name)
            && canonical_theory_bytes(existing) == canonical_theory_bytes(&theory)
        {
            return Ok(());
        }
        Err(TheoryDslError::Duplicate {
            kind: "theory".to_owned(),
            name,
        })
    }

    /// Record `composed`, the composition of an inline composed theory
    /// reference of a protocol, under its theory's name.
    ///
    /// A name already recorded is kept when its theory has the same
    /// canonical bytes as `composed`'s and its step reports are equal (the
    /// same composition named again), and is refused otherwise.
    ///
    /// # Errors
    ///
    /// [`TheoryDslError::Duplicate`] when the name is recorded with another
    /// theory or other reports, or as a theory that is not a composition.
    fn record_inline(&mut self, composed: ComposedTheory) -> Result<(), TheoryDslError> {
        let name = composed.theory.name.to_string();
        let Some(existing) = self.theories.get(&name) else {
            self.insert(composed);
            return Ok(());
        };
        if canonical_theory_bytes(existing) == canonical_theory_bytes(&composed.theory)
            && self.reports.get(&name) == Some(&composed.reports)
        {
            return Ok(());
        }
        Err(TheoryDslError::Duplicate {
            kind: "theory (from composition)".to_owned(),
            name,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::document::{DirectedEqSpec, OpSpec, SortKindSpec, SortSpec};

    fn no_resolver(_name: &str) -> Option<Theory> {
        None
    }

    /// A theory whose directed equation declares an `iso` coercion
    /// class on `upper`, which is not invertible: `upper` collapses
    /// distinct inputs, so the declared round-trip does not hold.
    fn lying_iso_document() -> TheoryDocument {
        TheoryDocument {
            id: "doc.lying".to_owned(),
            description: String::new(),
            body: TheoryBody::Theory(TheorySpec {
                theory: "ThLying".to_owned(),
                extends: vec![],
                imports: vec![],
                sorts: vec![SortSpec {
                    name: "Str".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Val {
                        value_kind: "string".to_owned(),
                    },
                    closed: None,
                }],
                ops: vec![OpSpec {
                    name: "upper".to_owned(),
                    input: Some("Str".to_owned()),
                    inputs: None,
                    output: "Str".to_owned(),
                }],
                equations: vec![],
                directed_equations: vec![DirectedEqSpec {
                    name: "lying_upper_iso".to_owned(),
                    lhs: "upper(x)".to_owned(),
                    rhs: "x".to_owned(),
                    impl_expr: "upper(x)".to_owned(),
                    inverse: Some("x".to_owned()),
                    source_kind: Some("string".to_owned()),
                    target_kind: Some("string".to_owned()),
                    coercion_class: "iso".to_owned(),
                }],
                policies: vec![],
            }),
        }
    }

    #[test]
    fn default_compile_rejects_lying_iso() {
        let doc = lying_iso_document();
        let err = compile(&doc, &no_resolver).expect_err("default compile must reject a lying iso");
        match err.unlocated() {
            TheoryDslError::CoercionLawViolation {
                theory,
                violations,
                distinct_equations,
            } => {
                assert_eq!(theory, "ThLying");
                assert!(!violations.is_empty());
                assert!(violations.iter().all(|d| d.equation == "lying_upper_iso"));
                assert_eq!(*distinct_equations, 1);
            }
            other => panic!("expected CoercionLawViolation, got {other:?}"),
        }
    }

    #[test]
    fn unchecked_compile_accepts_lying_iso() {
        let doc = lying_iso_document();
        let set = compile_unchecked(&doc, &no_resolver)
            .expect("unchecked compile must not run the coercion-law check");
        let theory = set.theories.get("ThLying").expect("ThLying present");
        assert_eq!(theory.directed_eqs.len(), 1);
    }

    #[test]
    fn default_compile_accepts_theory_without_directed_equations() {
        // A theory with no directed equations passes the law check
        // vacuously on the default path.
        let doc = TheoryDocument {
            id: "doc.plain".to_owned(),
            description: String::new(),
            body: TheoryBody::Theory(TheorySpec {
                theory: "ThPlain".to_owned(),
                extends: vec![],
                imports: vec![],
                sorts: vec![SortSpec {
                    name: "Str".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Val {
                        value_kind: "string".to_owned(),
                    },
                    closed: None,
                }],
                ops: vec![],
                equations: vec![],
                directed_equations: vec![],
                policies: vec![],
            }),
        };
        let set =
            compile(&doc, &no_resolver).expect("plain theory must compile on the default path");
        assert!(set.theories.contains_key("ThPlain"));
    }
}
