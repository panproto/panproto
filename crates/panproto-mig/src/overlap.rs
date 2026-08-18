//! Automatic overlap discovery between two schemas.
//!
//! [`discover_overlap`] finds the largest sub-schema the two share and packages
//! it as the pair list [`schema_pushout`](panproto_schema::schema_pushout)
//! expects. It is the maximum common induced sub-schema of the pair, which is
//! what merging them along their shared part means.

use panproto_schema::{Protocol, Schema, SchemaOverlap};

use crate::error::SpanError;
use crate::hom_search::{SearchOptions, find_span};

/// The largest shared sub-schema of two schemas, as an overlap.
///
/// # What this used to do, and why it was wrong
///
/// It used to run two total-morphism searches, one in each direction, and take
/// whichever embedded more. A total morphism from one schema into the other
/// exists only when the first embeds *wholly* in the second, and on the measured
/// schema corpus that holds for a small minority of real pairs. For every other
/// pair both searches returned nothing and this returned an empty overlap, so
/// two schemas sharing most of their structure merged as though they shared
/// none.
///
/// It is now one span search on the iso path. The apex is the maximum common
/// induced sub-schema, which always exists. The iso path is the right one
/// because a merge needs the right leg to be a mono: the pushout of a span
/// whose right leg collapses two apex vertices onto one is not a
/// common-sub-schema merge.
///
/// # What an empty overlap means
///
/// It means the two share no common **induced** sub-schema, which is a stricter
/// statement than sharing no vertex. Inducing carries every arc between the
/// chosen vertices along with them, so a vertex pair that agrees on kind and
/// name is still unshareable when the target's copy carries an arc the source's
/// does not: a single self-loop on the target side is enough. On randomly
/// generated pairs that happens often enough to be the ordinary case rather
/// than a corner one, so an empty overlap here is not evidence that the two
/// schemas have nothing in common vertex by vertex.
///
/// # Errors
///
/// [`SpanError`], for the reasons [`find_span`] gives. None of them means "no
/// overlap"; an empty overlap is a value, not an error.
///
/// # Examples
///
/// ```
/// use panproto_mig::discover_overlap;
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
///     ..Protocol::default()
/// };
///
/// // Neither schema embeds wholly in the other: each has a property the other
/// // lacks. They still share the object and one string.
/// let left = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.name", "string", None::<&str>)?
///     .vertex("root.count", "integer", None::<&str>)?
///     .edge("root", "root.name", "prop", Some("name"))?
///     .edge("root", "root.count", "prop", Some("count"))?
///     .entry("root")
///     .build()?;
/// let right = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.name", "string", None::<&str>)?
///     .vertex("root.slug", "string", None::<&str>)?
///     .edge("root", "root.name", "prop", Some("name"))?
///     .edge("root", "root.slug", "prop", Some("slug"))?
///     .entry("root")
///     .build()?;
///
/// let overlap = discover_overlap(&left, &right, &protocol)?;
/// assert_eq!(overlap.vertex_pairs.len(), 2);
/// assert_eq!(overlap.edge_pairs.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn discover_overlap(
    left: &Schema,
    right: &Schema,
    protocol: &Protocol,
) -> Result<SchemaOverlap, SpanError> {
    let opts = SearchOptions {
        iso: true,
        ..SearchOptions::default()
    };
    find_span(left, right, protocol, &opts).map(|span| span.to_overlap())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        if let Some((entry, _)) = vertices.first() {
            builder = builder.entry(entry);
        }
        builder.build().unwrap()
    }

    #[test]
    fn overlap_of_identical_schemas_has_all_vertices() {
        let s = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let overlap = discover_overlap(&s, &s, &test_protocol()).unwrap();

        assert_eq!(
            overlap.vertex_pairs.len(),
            s.vertex_count(),
            "all vertices should be paired"
        );
        assert_eq!(
            overlap.edge_pairs.len(),
            s.edge_count(),
            "all edges should be paired"
        );
    }

    #[test]
    fn overlap_of_disjoint_schemas_is_empty() {
        let left = build_schema(
            &[("a", "object"), ("a.x", "string")],
            &[("a", "a.x", "prop", "x")],
        );
        // Right uses only `integer` vertices, so no kind is shared.
        let right = build_schema(
            &[("b", "integer"), ("c", "integer")],
            &[("b", "c", "prop", "y")],
        );
        let overlap = discover_overlap(&left, &right, &test_protocol()).unwrap();

        assert!(
            overlap.vertex_pairs.is_empty(),
            "disjoint schemas should have no vertex overlap"
        );
        assert!(
            overlap.edge_pairs.is_empty(),
            "disjoint schemas should have no edge overlap"
        );
    }

    #[test]
    fn overlap_finds_shared_subgraph() {
        // Both schemas share an `object → string` sub-graph.
        let left = build_schema(
            &[
                ("root", "object"),
                ("root.x", "string"),
                ("root.extra", "integer"),
            ],
            &[
                ("root", "root.x", "prop", "x"),
                ("root", "root.extra", "prop", "extra"),
            ],
        );
        let right = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );

        let overlap = discover_overlap(&left, &right, &test_protocol()).unwrap();

        assert_eq!(
            overlap.vertex_pairs.len(),
            2,
            "the shared sub-graph is the object and the string"
        );
        assert_eq!(overlap.edge_pairs.len(), 1, "and the arc between them");
    }

    #[test]
    fn overlap_survives_when_neither_schema_embeds_in_the_other() {
        // This is the case the two-total-searches version answered with nothing:
        // each side has a property the other lacks, so no total morphism exists
        // in either direction, and yet they share most of their structure.
        let left = build_schema(
            &[
                ("root", "object"),
                ("root.name", "string"),
                ("root.count", "integer"),
            ],
            &[
                ("root", "root.name", "prop", "name"),
                ("root", "root.count", "prop", "count"),
            ],
        );
        let right = build_schema(
            &[
                ("root", "object"),
                ("root.name", "string"),
                ("root.slug", "string"),
            ],
            &[
                ("root", "root.name", "prop", "name"),
                ("root", "root.slug", "prop", "slug"),
            ],
        );

        let overlap = discover_overlap(&left, &right, &test_protocol()).unwrap();
        assert_eq!(overlap.vertex_pairs.len(), 2);
        assert_eq!(overlap.edge_pairs.len(), 1);
        assert!(
            overlap
                .vertex_pairs
                .iter()
                .any(|(l, r)| l.as_str() == "root.name" && r.as_str() == "root.name"),
            "the shared property is matched by name"
        );
    }

    #[test]
    fn an_overlap_merges() {
        let left = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let right = build_schema(
            &[("root", "object"), ("root.b", "string")],
            &[("root", "root.b", "prop", "b")],
        );

        let overlap = discover_overlap(&left, &right, &test_protocol()).unwrap();
        let (merged, into_left, into_right) =
            panproto_schema::schema_pushout(&left, &right, &overlap).unwrap();

        assert!(!merged.vertices.is_empty());
        assert_eq!(into_left.vertex_map.len(), left.vertices.len());
        assert_eq!(into_right.vertex_map.len(), right.vertices.len());
    }
}
