//! The GAT theory a schema induces, and the theory morphism a
//! migration induces between two such theories.
//!
//! A schema presents a small category: each vertex is a sort and each
//! edge is a unary operation from its source sort to its target sort.
//! [`schema_to_theory`] builds that presentation. A [`Migration`] between
//! two schemas then induces a [`TheoryMorphism`] on the mapped fragment,
//! which [`check_migration_morphism`] validates with
//! [`check_morphism`]: every mapped edge
//! must land between the images of its own endpoints.

use std::sync::Arc;

use panproto_gat::{GatError, Operation, Sort, Theory, TheoryMorphism, check_morphism};
use panproto_schema::{Edge, Schema};

use crate::migration::Migration;

/// Build the GAT theory induced by `schema`.
///
/// Each vertex becomes a sort, and each edge becomes a unary operation
/// from the source sort to the target sort. Sorts and operations are
/// emitted in a deterministic order so that the same schema always
/// yields the same theory.
#[must_use]
pub fn schema_to_theory(name: &str, schema: &Schema) -> Theory {
    // Sort vertex IDs for deterministic sort ordering.
    let mut vertex_ids: Vec<&panproto_gat::Name> = schema.vertices.keys().collect();
    vertex_ids.sort();
    let sorts: Vec<Sort> = vertex_ids
        .iter()
        .map(|vid| Sort::simple(Arc::from(vid.as_str())))
        .collect();

    // Sort edges for deterministic operation ordering and stable names.
    let mut edges: Vec<&Edge> = schema.edges.keys().collect();
    edges.sort();
    let ops: Vec<Operation> = edges
        .iter()
        .enumerate()
        .map(|(i, edge)| {
            Operation::unary(
                edge_op_name(edge, i),
                "x",
                Arc::from(edge.src.as_str()),
                Arc::from(edge.tgt.as_str()),
            )
        })
        .collect();

    Theory::new(name, sorts, ops, Vec::new())
}

/// The operation name [`schema_to_theory`] gives `edge`, where `index`
/// is its position in the schema's sorted edge list.
///
/// Uses unambiguous separators: `->` for the source-target arrow and `#`
/// for the label/index discriminator. These characters do not appear in
/// vertex IDs, so distinct edges never collide.
fn edge_op_name(edge: &Edge, index: usize) -> Arc<str> {
    edge.name.as_ref().map_or_else(
        || Arc::from(format!("{}->{}#{}", edge.src, edge.tgt, index)),
        |label| Arc::from(format!("{}->{}#{}", edge.src, edge.tgt, label)),
    )
}

/// Map every edge of `schema` to the operation name it receives in
/// [`schema_to_theory`].
fn edge_op_names(schema: &Schema) -> std::collections::HashMap<&Edge, Arc<str>> {
    let mut edges: Vec<&Edge> = schema.edges.keys().collect();
    edges.sort();
    edges
        .iter()
        .enumerate()
        .map(|(i, edge)| (*edge, edge_op_name(edge, i)))
        .collect()
}

/// Build the [`TheoryMorphism`] a migration induces on its mapped
/// fragment.
///
/// The domain theory is the source schema restricted to the vertices in
/// `m.vertex_map` and the edges in `m.edge_map` whose endpoints both
/// survive; the codomain theory is [`schema_to_theory`] of the whole
/// target schema. The `sort_map` is `m.vertex_map`, and the `op_map`
/// sends each mapped edge's operation to its image's operation using the
/// same naming scheme as [`schema_to_theory`].
///
/// The pair `(domain_theory, morphism)` obliges only the mapped
/// fragment: vertices a migration legitimately drops never enter the
/// domain, so a partial migration is checkable as a total morphism on
/// what it keeps.
#[must_use]
pub fn induced_theory_morphism(
    src: &Schema,
    tgt: &Schema,
    m: &Migration,
) -> (Theory, Theory, TheoryMorphism) {
    let src_op_names = edge_op_names(src);
    let tgt_op_names = edge_op_names(tgt);

    // Domain sorts: the mapped source vertices.
    let mut domain_sort_names: Vec<&panproto_gat::Name> = m.vertex_map.keys().collect();
    domain_sort_names.sort();
    let domain_sorts: Vec<Sort> = domain_sort_names
        .iter()
        .map(|v| Sort::simple(Arc::from(v.as_str())))
        .collect();

    // Domain ops and the op-map: mapped edges whose endpoints both survive.
    let mut sort_map: std::collections::HashMap<Arc<str>, Arc<str>> =
        std::collections::HashMap::new();
    for (s, t) in &m.vertex_map {
        sort_map.insert(Arc::from(s.as_str()), Arc::from(t.as_str()));
    }

    let mut domain_ops: Vec<(Arc<str>, Operation)> = Vec::new();
    let mut op_map: std::collections::HashMap<Arc<str>, Arc<str>> =
        std::collections::HashMap::new();
    for (src_edge, tgt_edge) in &m.edge_map {
        // Only the fragment whose endpoints are mapped is an obligation.
        if !m.vertex_map.contains_key(&src_edge.src) || !m.vertex_map.contains_key(&src_edge.tgt) {
            continue;
        }
        let Some(src_name) = src_op_names.get(src_edge) else {
            continue;
        };
        let Some(tgt_name) = tgt_op_names.get(tgt_edge) else {
            continue;
        };
        let op = Operation::unary(
            Arc::clone(src_name),
            "x",
            Arc::from(src_edge.src.as_str()),
            Arc::from(src_edge.tgt.as_str()),
        );
        domain_ops.push((Arc::clone(src_name), op));
        op_map.insert(Arc::clone(src_name), Arc::clone(tgt_name));
    }
    // Deterministic op order.
    domain_ops.sort_by(|a, b| a.0.cmp(&b.0));
    let domain_ops: Vec<Operation> = domain_ops.into_iter().map(|(_, op)| op).collect();

    let domain_theory = Theory::new(
        format!("dom_{}", src.protocol),
        domain_sorts,
        domain_ops,
        Vec::new(),
    );
    let codomain_theory = schema_to_theory(&format!("cod_{}", tgt.protocol), tgt);

    let morphism = TheoryMorphism::new(
        "induced_migration_morphism",
        Arc::clone(&domain_theory.name),
        Arc::clone(&codomain_theory.name),
        sort_map,
        op_map,
    );

    (domain_theory, codomain_theory, morphism)
}

/// Check that a migration is a structure-preserving theory morphism on
/// its mapped fragment.
///
/// Builds the induced morphism with [`induced_theory_morphism`] and
/// validates it with [`check_morphism`]:
/// every mapped edge must connect the images of its own endpoints, and
/// every mapped vertex must land on a vertex of the target schema.
///
/// # Errors
///
/// Returns the [`GatError`] describing the first structural violation,
/// e.g. an edge mapped between vertices that are not the images of its
/// endpoints.
pub fn check_migration_morphism(src: &Schema, tgt: &Schema, m: &Migration) -> Result<(), GatError> {
    let (domain_theory, codomain_theory, morphism) = induced_theory_morphism(src, tgt, m);
    check_morphism(&morphism, &domain_theory, &codomain_theory)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::{Edge, Vertex};
    use std::collections::HashMap;

    fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn edge(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    #[test]
    fn valid_rename_is_a_morphism() {
        let e = edge("a", "b", "x");
        let src = schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&e),
        );
        let e2 = edge("a2", "b2", "x");
        let tgt = schema(
            &[("a2", "object"), ("b2", "string")],
            std::slice::from_ref(&e2),
        );
        let m = Migration {
            vertex_map: HashMap::from([("a".into(), "a2".into()), ("b".into(), "b2".into())]),
            edge_map: HashMap::from([(e, e2)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };
        assert!(check_migration_morphism(&src, &tgt, &m).is_ok());
    }

    #[test]
    fn crossed_edge_is_not_a_morphism() {
        // edge a->b is mapped to an edge whose endpoints are not the
        // images of a and b.
        let e = edge("a", "b", "x");
        let src = schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&e),
        );
        let crossed = edge("c2", "b2", "x");
        let tgt = schema(
            &[("a2", "object"), ("b2", "string"), ("c2", "object")],
            std::slice::from_ref(&crossed),
        );
        let m = Migration {
            vertex_map: HashMap::from([("a".into(), "a2".into()), ("b".into(), "b2".into())]),
            edge_map: HashMap::from([(e, crossed)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };
        assert!(check_migration_morphism(&src, &tgt, &m).is_err());
    }
}
