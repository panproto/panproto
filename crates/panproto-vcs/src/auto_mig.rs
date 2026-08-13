//! Automatic migration derivation from schema diffs.
//!
//! Given an old schema, a new schema, and their structural diff, derives
//! a [`Migration`] that maps surviving vertices and edges via identity.
//! This handles the common cases of additions, removals, and constraint
//! changes.
//!
//! When potential renames are detected (vertices removed in old and added
//! in new), the module searches for a span between the two schemas and takes
//! its right leg, which accounts for renamed elements the diff reads as a
//! removal paired with an addition. The span search covers as much of the old
//! schema as it can rather than insisting on a total morphism, so it still has
//! something to say when the change dropped a field outright. Detected renames
//! from [`crate::rename_detect`] are correspondences this crate computed, so
//! they pin the search through [`SearchOptions::hard_pins`].
//!
//! The search is asked for an injective vertex map ([`SearchOptions::monic`]).
//! A derived migration carries no resolver, so it has nothing to say about how
//! two old vertices sent to one new vertex combine, and a search ranking
//! candidates by coverage alone prefers exactly that: on the ordinary edit that
//! renames one field and drops its siblings, sending every dropped sibling onto
//! the renamed one covers more of the old schema than keeping the rename and
//! dropping the rest. Insisting on injectivity leaves the honest partial map as
//! the best answer, and leaves contraction to an explicit migration file.

use panproto_gat::Name;
use std::collections::{HashMap, HashSet};

use panproto_check::diff::SchemaDiff;
use panproto_mig::Migration;
use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_schema::{Edge, Protocol, Schema};
use rustc_hash::FxHashSet;

use crate::rename_detect;

/// Derive a migration from a [`SchemaDiff`] between two schemas.
///
/// The derived migration uses identity mappings for all vertices and
/// edges that survive between the old and new schemas. Resolvers and
/// hyper-resolvers are left empty; if the migration requires contraction
/// resolution, the user must supply an explicit migration file.
///
/// # Algorithm
///
/// 1. **Vertex map**: For each vertex in `old` that also exists in `new`
///    (regardless of kind changes), map `id → id`.
/// 2. **Edge map**: For each edge in `old` that also exists in `new`,
///    map it to itself. For edges whose endpoints survive but the edge
///    itself changed (due to kind change), attempt to find a matching
///    edge in `new` between the same vertices with the same name.
/// 3. **Hyper-edge map**: Identity for hyper-edges present in both.
/// 4. **Label map**: Identity for labels within surviving hyper-edges
///    whose signatures still reference surviving vertices.
/// 5. **Resolver / hyper-resolver**: Empty.
#[must_use]
pub fn derive_migration(old: &Schema, new: &Schema, diff: &SchemaDiff) -> Migration {
    let removed_verts: FxHashSet<&str> = diff.removed_vertices.iter().map(String::as_str).collect();

    let removed_edges: FxHashSet<&Edge> = diff.removed_edges.iter().collect();

    // Vertex map: identity for surviving vertices.
    let vertex_map: HashMap<Name, Name> = old
        .vertices
        .keys()
        .filter(|id| !removed_verts.contains(id.as_str()))
        .map(|id| (id.clone(), id.clone()))
        .collect();

    // Edge map: identity for surviving edges, plus attempt to remap
    // edges affected by kind changes.
    let mut edge_map: HashMap<Edge, Edge> = HashMap::new();

    for edge in old.edges.keys() {
        if removed_edges.contains(edge) {
            continue;
        }
        // Both endpoints must survive.
        if removed_verts.contains(edge.src.as_str()) || removed_verts.contains(edge.tgt.as_str()) {
            continue;
        }

        if new.edges.contains_key(edge) {
            // Edge exists identically in new schema.
            edge_map.insert(edge.clone(), edge.clone());
        } else {
            // Edge was removed from new but endpoints survive; look for
            // a matching edge with the same name between the same vertices.
            if let Some(matching) =
                find_matching_edge(new, &edge.src, &edge.tgt, edge.name.as_deref())
            {
                edge_map.insert(edge.clone(), matching);
            }
        }
    }

    // Hyper-edge map: identity for surviving hyper-edges.
    let hyper_edge_map: HashMap<Name, Name> = old
        .hyper_edges
        .keys()
        .filter(|id| new.hyper_edges.contains_key(*id))
        .map(|id| (id.clone(), id.clone()))
        .collect();

    // Label map: identity for labels within surviving hyper-edges whose
    // target vertices survive.
    let mut label_map: HashMap<(Name, Name), Name> = HashMap::new();
    for (he_id, old_he) in &old.hyper_edges {
        if let Some(new_he) = new.hyper_edges.get(he_id) {
            for (label, vertex_id) in &old_he.signature {
                // Only map labels whose target vertex survives in both.
                if vertex_map.contains_key(vertex_id) {
                    if let Some(new_label) = find_label_for_vertex(new_he, vertex_id) {
                        label_map.insert((he_id.clone(), label.clone()), new_label);
                    }
                }
            }
        }
    }

    let identity_mig = Migration {
        vertex_map,
        edge_map,
        hyper_edge_map,
        label_map,
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        domain: None,
        codomain: None,
    };

    // If there are both removed and added vertices (potential renames),
    // try to find a better migration via homomorphism search.
    if !diff.removed_vertices.is_empty() && !diff.added_vertices.is_empty() {
        if let Some(enhanced) = try_hom_search_enhancement(old, new, &identity_mig) {
            return enhanced;
        }
    }

    identity_mig
}

/// The protocol the span search validates its apex against.
///
/// The apex is the sub-schema of `old` induced on the vertices the search gave
/// a target, so every kind it carries is a kind `old` already carries. Naming
/// exactly those kinds, with no edge rules and no constraint sorts, makes
/// validation a statement about the induction rather than about how well a
/// guessed protocol happens to describe the schema in hand. A repository that
/// has a stored `Protocol` object should be validating against that instead,
/// but a diff-derived migration is computed from two schemas alone.
fn induction_protocol(old: &Schema) -> Protocol {
    let mut obj_kinds: Vec<String> = old
        .vertices
        .values()
        .map(|vertex| vertex.kind.to_string())
        .collect();
    obj_kinds.sort_unstable();
    obj_kinds.dedup();

    Protocol {
        name: old.protocol.clone(),
        obj_kinds,
        ..Protocol::default()
    }
}

/// Attempt to find a better migration by searching for a span, with rename
/// detection providing the pinned assignments.
///
/// Returns `Some(migration)` if the span's right leg maps more vertices than
/// the diff-derived migration does and the spliced result is a theory
/// morphism, `None` otherwise.
///
/// The leg is injective on **vertices**, which is what
/// [`SearchOptions::monic`] promises and all it promises. It says nothing about
/// edges: two parallel source edges between one vertex pair are distinct keys
/// that may share an image, and the objective charges the loser an ordinary
/// rename penalty rather than rejecting it. The edge map is therefore pruned
/// here before the migration is stored.
fn try_hom_search_enhancement(
    old: &Schema,
    new: &Schema,
    identity_mig: &Migration,
) -> Option<Migration> {
    // Detected renames are correspondences this crate computed, so they pin
    // the search rather than merely steering it.
    //
    // A pin must agree on kind. `domain_of` keeps a pinned target only when its
    // kind matches the source vertex's, so a kind-incompatible pin leaves the
    // vertex with `⊥` as its only value and the search drops it — silently, and
    // even when a kind-compatible target was available. That is the one way a
    // pin can lose a field, and it is reachable: `detect_vertex_renames` scores
    // 0.2 for matching incoming edge labels plus 0.2 for a short edit distance
    // and never requires the kinds to agree, so a 0.4-confidence pin can rename
    // an integer field onto a string one. Raising the threshold cannot fix it,
    // because outgoing names alone already score 0.5 with no kind credit.
    //
    // Dropping a low-confidence pin strands nothing: the vertex falls back to
    // its whole kind-compatible domain and the search picks the best target.
    let renames = rename_detect::detect_vertex_renames(old, new, 0.3);
    let mut hard_pins: HashMap<Name, Name> = HashMap::new();
    for detected in &renames {
        let old_name = Name::from(detected.rename.old.as_ref());
        let new_name = Name::from(detected.rename.new.as_ref());
        let kinds_agree = old
            .vertices
            .get(&old_name)
            .zip(new.vertices.get(&new_name))
            .is_some_and(|(from, to)| from.kind == to.kind);
        if kinds_agree {
            hard_pins.insert(old_name, new_name);
        }
    }

    // The search is asked for an injective vertex map. A span that sends two
    // old vertices to one new vertex is a contraction, and a contraction needs
    // a resolver to say how the two sources combine. Auto-derivation has no
    // resolver to offer: it leaves `resolver` empty and tells the user to
    // supply an explicit migration file when contraction resolution is needed.
    // Without this the search maximises coverage alone, so the ordinary edit
    // that renames one field and drops its siblings scores the map sending
    // every dropped sibling onto the renamed one above the map that keeps the
    // rename and drops the rest, and the higher-coverage answer wins the
    // adoption test below. Under `Pi` such a migration is rejected outright as
    // a non-injective vertex map; under `Sigma` it silently reproduces each
    // dropped field's data under the survivor.
    let opts = SearchOptions {
        monic: true,
        hard_pins,
        ..SearchOptions::default()
    };

    // The span search never refuses *for want of a match*: the empty apex is
    // always feasible. It can still fail to be posed — a source vertex offered
    // more kind-compatible targets than one domain word holds is refused — and
    // that is a fact about the network rather than about these two schemas.
    // Falling back to the diff-derived migration is the same conservative
    // choice the adoption test below makes, and it is why this is `.ok()?`
    // rather than a propagated error: rename detection is an enhancement, and
    // losing it costs a rename, not a commit.
    let protocol = induction_protocol(old);
    let span = find_span(old, new, &protocol, &opts).ok()?;

    // Only use the span-based migration if it maps more vertices than the
    // identity-based one. A span that maps fewer is the search agreeing with
    // the diff about what survives.
    if span.right.vertex_map.len() > identity_mig.vertex_map.len() {
        // The right leg is a morphism out of the apex. Spliced onto `old` it
        // becomes the partial migration `old -> new` this crate stores, so it
        // carries the two maps and not the leg's own endpoints.
        // Hyper-edge and label maps come from the identity migration, since
        // the span search does not cover those.
        let right = span.right;
        let hom_mig = Migration {
            vertex_map: right.vertex_map,
            edge_map: injective_edge_map(right.edge_map),
            hyper_edge_map: identity_mig.hyper_edge_map.clone(),
            label_map: identity_mig.label_map.clone(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            domain: None,
            codomain: None,
        };
        // Validate the spliced candidate as a theory morphism before
        // adopting it; a heuristic candidate that is not structure-
        // preserving falls back to the diff-derived identity migration.
        let (dom, cod, morph) = panproto_mig::induced_theory_morphism(old, new, &hom_mig);
        if panproto_gat::check_morphism(&morph, &dom, &cod).is_ok() {
            Some(hom_mig)
        } else {
            None
        }
    } else {
        None
    }
}

/// Drop the losers of any edge-map collision, keeping the name-matched image.
///
/// `monic` is injectivity on vertices only, so the right leg of a monic span
/// may still send two parallel source edges to one target edge. A stored
/// migration whose edge map contracts is a migration that says two fields
/// become one without saying how, and there is nothing in the lift path that
/// could carry that out: `Migration::resolver` is an edge disambiguation table,
/// not a merge rule, and nothing reads `Schema::mergers`.
///
/// Leaving the loser *unmapped* rather than declining the whole leg is
/// deliberate. Declining falls back to the diff-derived identity migration,
/// which does not know about the rename either, so the surviving field's own
/// data is lost along with the contracted one — strictly more loss than the
/// defect being repaired. An unmapped edge is simply an edge the migration does
/// not carry.
///
/// The name-matched preimage wins, and the tie among the rest is broken on the
/// source edge's own order so that two runs over one schema pair agree.
fn injective_edge_map(edge_map: HashMap<Edge, Edge>) -> HashMap<Edge, Edge> {
    let mut pairs: Vec<(Edge, Edge)> = edge_map.into_iter().collect();
    pairs.sort_by(|left, right| {
        let name_matched = |pair: &(Edge, Edge)| pair.0.name != pair.1.name;
        name_matched(left)
            .cmp(&name_matched(right))
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut taken: HashSet<Edge> = HashSet::new();
    let mut kept: HashMap<Edge, Edge> = HashMap::new();
    for (source, image) in pairs {
        if taken.insert(image.clone()) {
            kept.insert(source, image);
        }
    }
    kept
}

/// Find an edge in `schema` between `src` and `tgt` with the given `name`.
fn find_matching_edge(schema: &Schema, src: &str, tgt: &str, name: Option<&str>) -> Option<Edge> {
    schema
        .edges
        .keys()
        .find(|e| e.src == src && e.tgt == tgt && e.name.as_deref() == name)
        .cloned()
}

/// Find a label in a hyper-edge that points to the given vertex.
fn find_label_for_vertex(he: &panproto_schema::HyperEdge, vertex_id: &str) -> Option<Name> {
    he.signature
        .iter()
        .find(|(_, v)| **v == *vertex_id)
        .map(|(label, _)| label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_check::diff::diff;
    use panproto_schema::Vertex;

    fn make_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
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

    #[test]
    fn derive_identity_for_unchanged() {
        let s = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let d = diff(&s, &s);
        let m = derive_migration(&s, &s, &d);
        assert_eq!(m.vertex_map.len(), 2);
        assert_eq!(m.vertex_map["a"], "a");
        assert_eq!(m.vertex_map["b"], "b");
    }

    #[test]
    fn derive_drops_removed_vertices() {
        let old = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let new = make_schema(&[("a", "object")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert_eq!(m.vertex_map.len(), 1);
        assert!(m.vertex_map.contains_key("a"));
        assert!(!m.vertex_map.contains_key("b"));
    }

    #[test]
    fn derive_keeps_edges_with_surviving_endpoints() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let old = make_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let new = make_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert_eq!(m.edge_map.len(), 1);
    }

    #[test]
    fn derive_drops_edges_with_removed_endpoints() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let old = make_schema(&[("a", "object"), ("b", "string")], &[edge]);
        let new = make_schema(&[("a", "object")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert!(m.edge_map.is_empty());
    }

    #[test]
    fn derive_handles_addition() {
        let old = make_schema(&[("a", "object")], &[]);
        let new = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        // Only 'a' exists in old, so only 'a' is mapped.
        assert_eq!(m.vertex_map.len(), 1);
        assert!(m.vertex_map.contains_key("a"));
    }
}
