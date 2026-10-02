//! Migration compilation for fast per-record application.
//!
//! [`compile`] pre-computes surviving vertex/edge sets, remapping
//! tables, and resolver lookups so that `lift_wtype` and `lift_functor`
//! can process each record in O(nodes) time without re-deriving the
//! migration structure.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::CompiledMigration;
use panproto_schema::{Edge, Schema};

use crate::error::ExistenceError;
use crate::migration::Migration;

/// Compile a migration specification into a form optimized for
/// per-record application.
///
/// The compilation computes:
/// 1. The surviving vertex set (image of `vertex_map` in the target)
/// 2. The surviving edge set (image of `edge_map`)
/// 3. Vertex and edge remap tables
/// 4. Resolver copies
///
/// # Errors
///
/// Returns `ExistenceError::WellFormedness` if the migration references
/// vertices or edges not present in either schema.
pub fn compile(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Result<CompiledMigration, ExistenceError> {
    // Step 0: The mapped fragment must be a structure-preserving theory
    // morphism: every mapped edge must connect the images of its own
    // endpoints. This rejects crossed edge maps before any per-record
    // work is compiled.
    crate::schema_theory::check_migration_morphism(src, tgt, migration).map_err(|e| {
        ExistenceError::NotAMorphism {
            detail: e.to_string(),
        }
    })?;

    // Step 1: Compute surviving vertices (target vertices in the image of vertex_map).
    let mut surviving_verts = HashSet::new();
    let mut vertex_remap = HashMap::new();

    for (src_v, tgt_v) in &migration.vertex_map {
        if !tgt.has_vertex(tgt_v) {
            return Err(ExistenceError::WellFormedness {
                message: format!("vertex_map target {tgt_v} (from {src_v}) not in target schema"),
            });
        }
        surviving_verts.insert(tgt_v.clone());
        vertex_remap.insert(src_v.clone(), tgt_v.clone());
    }

    // Step 2: Compute surviving edges (target edges in the image of edge_map).
    let mut surviving_edges = HashSet::new();
    let mut edge_remap = HashMap::new();

    for (src_e, tgt_e) in &migration.edge_map {
        surviving_edges.insert(tgt_e.clone());
        edge_remap.insert(src_e.clone(), tgt_e.clone());
    }

    // Step 2b: Lower the label renames of mapped hyper-edges onto the
    // attribute arrows they name, as ordinary edge remaps.
    lower_label_renames(src, tgt, migration, &mut edge_remap, &mut surviving_edges)?;

    // Also include edges in the target that are between surviving vertices
    // and not explicitly remapped. These edges "survive" because both
    // endpoints do.
    for edge in tgt.edges.keys() {
        if surviving_verts.contains(&edge.src) && surviving_verts.contains(&edge.tgt) {
            surviving_edges.insert(edge.clone());
        }
    }

    // Step 3: Generate value transforms, carried as op-to-term assignments. A
    // kind-changing vertex map computes the coerced value by substituting the
    // source value into the coercion's forward term; `Delta` and `Sigma` apply
    // this by substitution.
    //
    // The migration's own coercion table wins where it has an entry. It is the
    // record of what this migration actually does to values, including the
    // composite of steps whose intermediate schema is no longer one of these
    // two; the kind pair looked up in the target schema is only how a migration
    // that carries nothing is read.
    let mut op_term_assignments = HashMap::new();
    for (src_v, tgt_v) in &migration.vertex_map {
        let spec = migration.coercions.get(src_v).or_else(|| {
            let src_vert = src.vertex(src_v)?;
            let tgt_vert = tgt.vertex(tgt_v)?;
            if src_vert.kind == tgt_vert.kind {
                return None;
            }
            tgt.coercions
                .get(&(src_vert.kind.clone(), tgt_vert.kind.clone()))
        });
        if let Some(coercion_spec) = spec {
            op_term_assignments
                .entry(src_v.clone())
                .or_insert_with(Vec::new)
                .push(panproto_inst::TermAssignment::Compute {
                    target: crate::migration::COERCION_INPUT.to_string(),
                    scope: panproto_inst::TermScope::Field,
                    term: coercion_spec.forward.clone(),
                    inverse: coercion_spec.inverse.clone(),
                    coercion_class: coercion_spec.class,
                });
        }
    }

    // Step 4: Copy resolver tables.
    let resolver = migration.resolver.clone();

    // Step 5: Build the hyper-resolver, one entry per fan shape.
    //
    // A hyper-edge may carry several resolver entries that differ only in
    // which child labels the fan holds, so the compiled table keeps the full
    // (hyper-edge, label-set) key. Label sets are canonicalized — sorted and
    // deduplicated — so that a fan matches on its shape rather than on the
    // order in which the specification happened to list its labels.
    let mut hyper_resolver = panproto_inst::HyperResolverTable::new();
    for ((he_id, labels), (tgt_he_id, label_map)) in &migration.hyper_resolver {
        let shape = panproto_inst::canonical_label_shape(labels.iter().map(Name::as_str));
        let value = (tgt_he_id.clone(), label_map.clone());
        if let Some(existing) = hyper_resolver.get(&(he_id.clone(), shape.clone())) {
            if *existing != value {
                return Err(ExistenceError::WellFormedness {
                    message: format!(
                        "hyper_resolver has conflicting entries for hyper-edge {he_id} \
                         with label set {shape:?}"
                    ),
                });
            }
            continue;
        }
        hyper_resolver.insert((he_id.clone(), shape), value);
    }

    Ok(CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver,
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments,
        expansion_path: HashMap::new(),
    })
}

/// The arrow of `label` in hyper-edge `hyper_edge` of `schema`: the schema
/// edge named by the label from the vertex at the hyper-edge's parent label
/// to the vertex at the label. `Ok(None)` when there is none, and `Err(())`
/// when there are several.
fn label_arrow(
    schema: &Schema,
    hyper_edge: &panproto_schema::HyperEdge,
    label: &Name,
) -> Result<Option<Edge>, ()> {
    let (Some(parent), Some(child)) = (
        hyper_edge.signature.get(&hyper_edge.parent_label),
        hyper_edge.signature.get(label),
    ) else {
        return Ok(None);
    };
    let mut arrows = schema
        .edges_between(parent, child)
        .iter()
        .filter(|e| e.name.as_ref() == Some(label));
    match (arrows.next(), arrows.next()) {
        (None, _) => Ok(None),
        (Some(arrow), None) => Ok(Some(arrow.clone())),
        (Some(_), Some(_)) => Err(()),
    }
}

/// Lower the label renames of every mapped hyper-edge onto its attribute
/// arrows.
///
/// For each `(H, H')` of `hyper_edge_map` and each label `l` of `H` other
/// than its parent label, in ascending `(H, l)`: the target label `l'` is
/// `label_map[(H, l)]` (keyed by the source label) when present and `l`
/// otherwise. When `l'` is a label of `H'`, the arrow of `l` in the source
/// is paired with the arrow of `l'` in the target: `edge_remap` gains it,
/// unless `edge_map` maps the source arrow already, in which case its image
/// must be the target arrow. When `l` is no label of `H'` and `label_map`
/// does not rename it, the label is removed: the target must hold no edge
/// of the arrow's kind named `l` between the images of its endpoints, so
/// the arrow has no image. A label without an arrow in the source is not
/// lowered.
///
/// # Errors
///
/// [`ExistenceError::LabelArrowMismatch`] for the first label, in
/// ascending `(H, l)`, with two arrows in the source or the target, a
/// `label_map` entry that names no label of `H'`, a lowered label with no
/// arrow in the target, an `edge_map` image other than the target arrow,
/// or a removed label whose arrow keeps a same-named image.
fn lower_label_renames(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
    edge_remap: &mut HashMap<Edge, Edge>,
    surviving_edges: &mut HashSet<Edge>,
) -> Result<(), ExistenceError> {
    let mut hyper_edges: Vec<(&Name, &Name)> = migration.hyper_edge_map.iter().collect();
    hyper_edges.sort();
    for (source_id, target_id) in hyper_edges {
        let (Some(source), Some(target)) = (
            src.hyper_edges.get(source_id),
            tgt.hyper_edges.get(target_id),
        ) else {
            continue;
        };
        let mut labels: Vec<&Name> = source
            .signature
            .keys()
            .filter(|l| **l != source.parent_label)
            .collect();
        labels.sort();
        for label in labels {
            let mismatch = || ExistenceError::LabelArrowMismatch {
                hyper_edge: source_id.to_string(),
                label: label.to_string(),
            };
            let renamed = migration.label_map.get(&(source_id.clone(), label.clone()));
            let target_label = renamed.unwrap_or(label);
            if renamed.is_some() && !target.signature.contains_key(target_label) {
                return Err(mismatch());
            }
            let Some(arrow) = label_arrow(src, source, label).map_err(|()| mismatch())? else {
                continue;
            };
            if target.signature.contains_key(target_label) {
                let image = label_arrow(tgt, target, target_label)
                    .map_err(|()| mismatch())?
                    .ok_or_else(mismatch)?;
                match migration.edge_map.get(&arrow) {
                    Some(mapped) if *mapped != image => return Err(mismatch()),
                    Some(_) => {}
                    None => {
                        edge_remap.insert(arrow, image.clone());
                        surviving_edges.insert(image);
                    }
                }
            } else {
                let endpoint = |v: &Name| migration.vertex_map.get(v).unwrap_or(v).clone();
                let (from, to) = (endpoint(&arrow.src), endpoint(&arrow.tgt));
                let keeps_image = tgt
                    .edges_between(&from, &to)
                    .iter()
                    .any(|e| e.kind == arrow.kind && e.name.as_ref() == Some(label));
                if keeps_image {
                    return Err(mismatch());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::{Edge, Vertex};

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }

        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
            outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
            incoming
                .entry(edge.tgt.clone())
                .or_default()
                .push(edge.clone());
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
            ..Default::default()
        }
    }

    #[test]
    fn compile_identity() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let schema = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );

        let mig = Migration::identity(&["a".into(), "b".into()], std::slice::from_ref(&edge));

        let compiled = compile(&schema, &schema, &mig);
        assert!(compiled.is_ok());
        let c = compiled.unwrap_or_else(|_| panic!("compile should succeed"));
        assert_eq!(c.surviving_verts.len(), 2);
        assert!(c.surviving_verts.contains("a"));
        assert!(c.surviving_verts.contains("b"));
        assert!(c.surviving_edges.contains(&edge));
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn compile_projection_drops_vertex() {
        let edge_ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let edge_ac = Edge {
            src: "a".into(),
            tgt: "c".into(),
            kind: "prop".into(),
            name: Some("y".into()),
        };

        let src = test_schema(
            &[("a", "object"), ("b", "string"), ("c", "string")],
            &[edge_ab.clone(), edge_ac],
        );
        let tgt = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge_ab),
        );

        let mig = Migration {
            vertex_map: HashMap::from([("a".into(), "a".into()), ("b".into(), "b".into())]),
            edge_map: HashMap::from([(edge_ab.clone(), edge_ab)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };

        let compiled = compile(&src, &tgt, &mig);
        assert!(compiled.is_ok());
        let c = compiled.unwrap_or_else(|_| panic!("compile should succeed"));
        assert_eq!(c.surviving_verts.len(), 2);
        assert!(!c.surviving_verts.contains("c"));
    }

    #[test]
    fn compile_emits_coercion_as_term_assignment() {
        use panproto_gat::CoercionClass;
        use panproto_schema::CoercionSpec;

        // Source vertex `a` is an `int`; target `a` is a `string`. The
        // target schema registers an int→string coercion, so `compile`
        // must carry it as an op-to-term assignment rather than a direct
        // field transform.
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let src = test_schema(
            &[("a", "int"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let mut tgt = test_schema(
            &[("a", "string"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        tgt.coercions.insert(
            (Name::from("int"), Name::from("string")),
            CoercionSpec {
                forward: panproto_expr::Expr::Builtin(
                    panproto_expr::BuiltinOp::IntToStr,
                    vec![panproto_expr::Expr::Var(std::sync::Arc::from("__value__"))],
                ),
                inverse: None,
                class: CoercionClass::Retraction,
            },
        );

        let mig = Migration::identity(&["a".into(), "b".into()], std::slice::from_ref(&edge));
        let compiled = compile(&src, &tgt, &mig).expect("compile should succeed");

        assert!(
            compiled.field_transforms.is_empty(),
            "compile routes value transforms through op-to-term assignments",
        );
        let assignments = compiled
            .op_term_assignments
            .get(&Name::from("a"))
            .expect("coercion for vertex a is present");
        assert!(
            matches!(
                assignments.as_slice(),
                [panproto_inst::TermAssignment::Compute { target, .. }] if target == "__value__"
            ),
            "the coercion is carried as a Compute term assignment",
        );
    }

    #[test]
    fn compile_rejects_non_morphism_migration() {
        // edge a->b is mapped to an edge c->b, whose source is not the
        // image of a, so the mapped fragment is not a morphism.
        let source_edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let crossed_edge = Edge {
            src: "c".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };

        let src = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&source_edge),
        );
        let tgt = test_schema(
            &[("a", "object"), ("b", "string"), ("c", "object")],
            std::slice::from_ref(&crossed_edge),
        );

        let mig = Migration {
            vertex_map: HashMap::from([("a".into(), "a".into()), ("b".into(), "b".into())]),
            edge_map: HashMap::from([(source_edge, crossed_edge)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };

        let compiled = compile(&src, &tgt, &mig);
        assert!(
            matches!(compiled, Err(ExistenceError::NotAMorphism { .. })),
            "expected NotAMorphism, got {compiled:?}"
        );
    }
}
