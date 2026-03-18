//! Automatic protolens generation pipeline.
//!
//! Given two schemas, auto-discovers morphism alignment, factorizes
//! it into elementary endofunctors, maps each to a protolens, and
//! composes the result.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{Name, Theory, TheoryEndofunctor, TheoryMorphism, TheoryTransform, factorize};
use panproto_inst::value::Value;
use panproto_mig::hom_search::{FoundMorphism, SearchOptions, find_best_morphism};
use panproto_schema::{Protocol, Schema};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{Protolens, ProtolensChain, elementary};

/// Result of automatic protolens generation.
pub struct AutoLensResult {
    /// The protolens chain (schema-independent, reusable).
    pub chain: ProtolensChain,
    /// The concrete lens (schema-specific).
    pub lens: Lens,
    /// Quality score of the morphism alignment (0.0 to 1.0).
    pub alignment_quality: f64,
}

/// Configuration for automatic lens generation.
#[derive(Debug, Clone, Default)]
pub struct AutoLensConfig {
    /// User-provided default values for new sorts.
    pub defaults: HashMap<Name, Value>,
    /// Search options for morphism discovery.
    pub search_opts: SearchOptions,
    /// Whether to attempt overlap-based alignment when direct morphism fails.
    pub try_overlap: bool,
}

/// Generate a protolens chain and concrete lens from two schemas.
///
/// # Pipeline
///
/// 1. Discover the best morphism alignment between `src` and `tgt`.
/// 2. Convert the alignment to a GAT-level `TheoryMorphism`.
/// 3. Factorize the morphism into elementary endofunctors.
/// 4. Map each endofunctor to an elementary `Protolens`.
/// 5. Compose into a `ProtolensChain`.
/// 6. Instantiate the chain at `src` to produce a concrete `Lens`.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if no morphism is found,
/// factorization fails, or instantiation fails.
pub fn auto_generate(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
) -> Result<AutoLensResult, LensError> {
    // Step 1: Find best morphism alignment
    let alignment = find_best_morphism(src, tgt, &config.search_opts)
        .ok_or_else(|| LensError::ProtolensError("no morphism found between schemas".into()))?;

    let quality = alignment.quality;

    // Step 2: Build protolens chain from alignment
    let chain = protolens_from_alignment(&alignment, src, tgt)?;

    // Step 3: Instantiate at source schema
    let lens = chain.instantiate(src, protocol)?;

    Ok(AutoLensResult {
        chain,
        lens,
        alignment_quality: quality,
    })
}

/// Generate a protolens chain from a pre-computed morphism alignment.
///
/// Converts the schema-level alignment to a GAT-level theory morphism,
/// factorizes it into elementary endofunctors, and maps each to a
/// protolens.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if factorization fails or
/// an endofunctor cannot be mapped to a protolens.
pub fn protolens_from_alignment(
    alignment: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
) -> Result<ProtolensChain, LensError> {
    // Convert schema-level alignment to GAT-level theory morphism
    let src_theory = schema_to_implicit_theory(src);
    let tgt_theory = schema_to_implicit_theory(tgt);
    let morphism = alignment_to_theory_morphism(alignment, src, tgt);

    // Factorize the morphism
    let factorization = factorize(&morphism, &src_theory, &tgt_theory)
        .map_err(|e| LensError::ProtolensError(format!("factorization failed: {e}")))?;

    // Map each elementary endofunctor to a protolens
    let mut steps = Vec::new();
    for endofunctor in &factorization.steps {
        let protolens = endofunctor_to_protolens(endofunctor)?;
        steps.push(protolens);
    }

    Ok(ProtolensChain::new(steps))
}

/// Convert a schema to its implicit theory (sorts = vertex kinds,
/// ops = edge kinds).
fn schema_to_implicit_theory(schema: &Schema) -> Theory {
    crate::protolens::schema_to_implicit_theory(schema)
}

/// Convert a `FoundMorphism` to a `TheoryMorphism`.
///
/// Builds the sort map from vertex kind mappings and the op map from
/// edge kind mappings. Ensures all sorts and ops in the source theory
/// are represented in the morphism (identity-mapping any unmapped ones).
fn alignment_to_theory_morphism(
    found: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
) -> TheoryMorphism {
    // Build sort map from vertex kind mappings
    let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (src_id, tgt_id) in &found.vertex_map {
        if let (Some(src_v), Some(tgt_v)) = (src.vertices.get(src_id), tgt.vertices.get(tgt_id)) {
            let src_kind: Arc<str> = Arc::from(src_v.kind.as_str());
            let tgt_kind: Arc<str> = Arc::from(tgt_v.kind.as_str());
            sort_map.entry(src_kind).or_insert(tgt_kind);
        }
    }

    // Build op map from edge kind mappings
    let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (src_edge, tgt_edge) in &found.edge_map {
        let src_kind: Arc<str> = Arc::from(src_edge.kind.as_str());
        let tgt_kind: Arc<str> = Arc::from(tgt_edge.kind.as_str());
        op_map.entry(src_kind).or_insert(tgt_kind);
    }

    // Ensure all sorts and ops in the source theory are mapped
    let src_theory = crate::protolens::schema_to_implicit_theory(src);
    for sort in &src_theory.sorts {
        sort_map
            .entry(Arc::clone(&sort.name))
            .or_insert_with(|| Arc::clone(&sort.name));
    }
    for op in &src_theory.ops {
        op_map
            .entry(Arc::clone(&op.name))
            .or_insert_with(|| Arc::clone(&op.name));
    }

    TheoryMorphism::new(
        "auto_morphism",
        "src_implicit",
        "tgt_implicit",
        sort_map,
        op_map,
    )
}

/// Convert a `TheoryEndofunctor` to a `Protolens`.
///
/// Each elementary endofunctor maps directly to one of the elementary
/// protolens constructors. `Identity` and `Compose` transforms are
/// rejected since they should not appear in a factorized sequence.
fn endofunctor_to_protolens(endofunctor: &TheoryEndofunctor) -> Result<Protolens, LensError> {
    match &endofunctor.transform {
        TheoryTransform::AddSort(sort) => Ok(elementary::add_sort(
            Name::from(&*sort.name),
            Name::from(&*sort.name),
            Value::Null,
        )),
        TheoryTransform::DropSort(name) => Ok(elementary::drop_sort(Name::from(&**name))),
        TheoryTransform::RenameSort { old, new } => Ok(elementary::rename_sort(
            Name::from(&**old),
            Name::from(&**new),
        )),
        TheoryTransform::AddOp(op) => {
            let src = if op.inputs.is_empty() {
                Name::from("unknown")
            } else {
                Name::from(&*op.inputs[0].1)
            };
            Ok(elementary::add_op(
                Name::from(&*op.name),
                src,
                Name::from(&*op.output),
                Name::from(&*op.name),
            ))
        }
        TheoryTransform::DropOp(name) => Ok(elementary::drop_op(Name::from(&**name))),
        TheoryTransform::RenameOp { old, new } => Ok(elementary::rename_op(
            Name::from(&**old),
            Name::from(&**new),
        )),
        TheoryTransform::AddEquation(eq) => Ok(elementary::add_equation(eq.clone())),
        TheoryTransform::DropEquation(name) => Ok(elementary::drop_equation(Name::from(&**name))),
        TheoryTransform::Pullback(morphism) => Ok(elementary::pullback(morphism.clone())),
        TheoryTransform::Identity => Err(LensError::ProtolensError(
            "unexpected Identity in factorization".into(),
        )),
        TheoryTransform::Compose(_, _) => Err(LensError::ProtolensError(
            "unexpected Compose in factorization".into(),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Sort;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "record".into(),
                "string".into(),
                "boolean".into(),
                "array".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema_v1(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.done", "boolean", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.done", "prop", Some("done"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn schema_v2(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.status", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.status", "prop", Some("status"))
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn auto_generate_between_same_schemas() {
        let protocol = test_protocol();
        let s = schema_v1(&protocol);
        let config = AutoLensConfig::default();
        let result = auto_generate(&s, &s, &protocol, &config).unwrap();
        assert!(result.chain.is_empty() || result.alignment_quality > 0.0);
    }

    #[test]
    fn auto_generate_between_different_schemas() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let v2 = schema_v2(&protocol);
        let config = AutoLensConfig::default();
        let result = auto_generate(&v1, &v2, &protocol, &config);
        // Should either succeed or fail with a clear error
        match result {
            Ok(r) => {
                assert!(!r.chain.is_empty());
                assert!(r.alignment_quality > 0.0);
            }
            Err(e) => {
                // Acceptable if no morphism found
                assert!(e.to_string().contains("morphism"));
            }
        }
    }

    #[test]
    fn alignment_to_morphism_preserves_kinds() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let v2 = schema_v1(&protocol); // same schema
        let alignment = FoundMorphism {
            vertex_map: v1.vertices.keys().map(|k| (k.clone(), k.clone())).collect(),
            edge_map: v1.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
            quality: 1.0,
        };
        let morphism = alignment_to_theory_morphism(&alignment, &v1, &v2);
        // All source sorts should be in the sort map
        let src_theory = schema_to_implicit_theory(&v1);
        for sort in &src_theory.sorts {
            assert!(morphism.sort_map.contains_key(&sort.name));
        }
    }

    #[test]
    fn protolens_from_identity_alignment() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let alignment = FoundMorphism {
            vertex_map: v1.vertices.keys().map(|k| (k.clone(), k.clone())).collect(),
            edge_map: v1.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
            quality: 1.0,
        };
        let chain = protolens_from_alignment(&alignment, &v1, &v1).unwrap();
        // Identity alignment should produce empty or near-empty chain
        // (depends on factorize behavior for identity morphism)
        assert!(chain.len() <= 1);
    }

    #[test]
    fn endofunctor_to_protolens_add_sort() {
        let ef = TheoryEndofunctor {
            name: Arc::from("add_tags"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddSort(Sort::simple("tags")),
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.name.contains("add_sort"));
    }

    #[test]
    fn endofunctor_to_protolens_drop_sort() {
        let ef = TheoryEndofunctor {
            name: Arc::from("drop_foo"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("foo")),
            transform: TheoryTransform::DropSort(Arc::from("foo")),
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.name.contains("drop_sort"));
        assert!(!p.is_lossless());
    }

    #[test]
    fn endofunctor_to_protolens_rename() {
        let ef = TheoryEndofunctor {
            name: Arc::from("rename"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("old")),
            transform: TheoryTransform::RenameSort {
                old: Arc::from("old"),
                new: Arc::from("new"),
            },
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.is_lossless());
    }

    #[test]
    fn endofunctor_to_protolens_rejects_identity() {
        let ef = TheoryEndofunctor {
            name: Arc::from("id"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::Identity,
        };
        assert!(endofunctor_to_protolens(&ef).is_err());
    }
}
