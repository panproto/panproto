//! Canonical bytes and digests of trees and segments, and cross digests.

use std::collections::{BTreeMap, HashMap};

use panproto_schema::{CanonicalWriter, InstanceShapeKind};
use proptest::prelude::*;

use super::{TestResult, arb_finstance, arb_winstance, interleaved, label, next_free};
use crate::fan::Fan;
use crate::ginstance::{EdgeId, FanEmbedding, GInstance, TreeEmbedding, write_node_record};
use crate::identity::{CanonicalSink, Digest, TREE_INSTANCE_CONTEXT};
use crate::metadata::Node;
use crate::segment::{
    AnySegment, ManifestHeader, ManifestRole, Partition, SegmentError, SegmentId, SegmentRecord,
    canonical_winstance_bytes, partition_table, partition_tree, seal, verify_segment,
    verify_segment_bytes, winstance_digest,
};
use crate::wtype::WInstance;

/// A tree `0 -> 1, 0 -> 2` whose root carries a fan over both children.
fn small_tree() -> WInstance {
    let nodes = HashMap::from([
        (0, Node::new(0, "doc")),
        (1, Node::new(1, "para")),
        (2, Node::new(2, "para")),
    ]);
    let arcs = vec![
        (0, 2, label("doc", "para", "prop", Some("b"))),
        (0, 1, label("doc", "para", "prop", Some("a"))),
    ];
    let fan = Fan {
        hyper_edge_id: "rel".into(),
        parent: 0,
        children: HashMap::from([("y".to_owned(), 2), ("x".to_owned(), 1)]),
    };
    WInstance::new(nodes, arcs, vec![fan], 0, "doc".into())
}

#[test]
fn canonical_winstance_bytes_follow_the_record_layout() {
    let tree = small_tree();
    let mut w = CanonicalWriter::new();
    // Header: node, arc and fan counts.
    w.u64(3);
    w.u64(2);
    w.u64(1);
    // Node records in ascending id.
    for id in 0..3 {
        write_node_record(&mut w, &tree.nodes[&id]);
    }
    // Arc records `(parent, child, edge)` in arc order, with no edge id.
    w.u32(0);
    w.u32(2);
    CanonicalSink::edge(&mut w, &label("doc", "para", "prop", Some("b")));
    w.u32(0);
    w.u32(1);
    CanonicalSink::edge(&mut w, &label("doc", "para", "prop", Some("a")));
    // Fan records `(parent, hyper_edge_id, children sorted)`.
    w.u32(0);
    CanonicalSink::str(&mut w, "rel");
    w.len(2);
    CanonicalSink::str(&mut w, "x");
    w.u32(1);
    CanonicalSink::str(&mut w, "y");
    w.u32(2);
    assert_eq!(canonical_winstance_bytes(&tree), w.into_bytes());
    assert_eq!(
        winstance_digest(&tree),
        Digest::derive(TREE_INSTANCE_CONTEXT, &canonical_winstance_bytes(&tree))
    );
}

#[test]
fn a_tree_digest_ignores_its_embedding() -> TestResult {
    let tree = small_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    // Another embedding of the same tree: other edge ids and another fan
    // node id.
    let other = TreeEmbedding {
        arcs: embedding
            .arcs
            .iter()
            .map(|(c, id)| (*c, EdgeId(id.0 + 100)))
            .collect(),
        fans: embedding
            .fans
            .iter()
            .map(|f| FanEmbedding {
                node: f.node + 50,
                parent_edge: EdgeId(f.parent_edge.0 + 100),
                child_edges: f
                    .child_edges
                    .iter()
                    .map(|(l, id)| (l.clone(), EdgeId(id.0 + 100)))
                    .collect(),
            })
            .collect(),
        next_node: embedding.next_node + 50,
        next_edge: embedding.next_edge + 100,
    };
    let (a, b) = (
        GInstance::from_wtype_under(&tree, &embedding, None)?,
        GInstance::from_wtype_under(&tree, &other, None)?,
    );
    assert_ne!(a.digest(), b.digest());
    assert_eq!(winstance_digest(&tree), tree.digest());
    // The digest is a function of the tree alone, so equal trees with
    // fans listed in another order have one digest too.
    let mut reordered = small_tree();
    reordered.fans.push(Fan {
        hyper_edge_id: "pair".into(),
        parent: 1,
        children: HashMap::new(),
    });
    let mut swapped = small_tree();
    swapped.fans.insert(
        0,
        Fan {
            hyper_edge_id: "pair".into(),
            parent: 1,
            children: HashMap::new(),
        },
    );
    assert_eq!(winstance_digest(&reordered), winstance_digest(&swapped));
    assert_ne!(winstance_digest(&reordered), winstance_digest(&tree));
    Ok(())
}

/// A source manifest over `segments` under `schema`, sealed.
fn manifest_of(
    segments: &[AnySegment],
    partition: &Partition,
    schema: Digest,
) -> Result<crate::segment::SegmentManifest, SegmentError> {
    let records: Vec<SegmentRecord> = segments
        .iter()
        .map(|s| SegmentRecord::of(s, &partition.specs()[s.segment().0 as usize], &schema))
        .collect();
    let runs = segments
        .iter()
        .map(|s| {
            (
                s.segment(),
                Box::new(s.edge_id_run().into_iter()) as Box<dyn Iterator<Item = EdgeId>>,
            )
        })
        .collect();
    seal(
        records,
        ManifestHeader {
            shape: partition.shape(),
            schema,
            role: ManifestRole::Source,
            binding: None,
        },
        runs,
    )
}

#[test]
fn a_segment_replayed_under_another_schema_fails_verification() -> TestResult {
    let tree = small_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let p = interleaved(
        InstanceShapeKind::Tree,
        next_free(Some(embedding.next_node - 1), Some(embedding.next_edge - 1)),
        2,
        3,
    );
    let segments: Vec<AnySegment> = partition_tree(&tree, &embedding, &p)?
        .into_iter()
        .map(AnySegment::Tree)
        .collect();
    let (schema_a, schema_b) = (Digest([1; 32]), Digest([2; 32]));
    let manifest_b = manifest_of(&segments, &p, schema_b)?;
    let bytes_a = segments[1].canonical_bytes(&schema_a, &p)?;
    assert_eq!(
        verify_segment_bytes(&manifest_b, &bytes_a).err(),
        Some(SegmentError::SchemaMismatch {
            expected: schema_b,
            found: schema_a
        })
    );
    // The same contents under schema a have another digest.
    assert_ne!(
        segments[1].digest(&schema_a, &p)?,
        segments[1].digest(&schema_b, &p)?
    );
    let bytes_b = segments[1].canonical_bytes(&schema_b, &p)?;
    verify_segment_bytes(&manifest_b, &bytes_b)?;
    Ok(())
}

#[test]
fn flipping_a_tree_segment_root_or_fan_changes_its_digest_and_fails_verify_segment() -> TestResult {
    let tree = small_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let p = Partition::single(
        InstanceShapeKind::Tree,
        Some(embedding.next_node.max(embedding.next_edge)),
    );
    let segments: Vec<AnySegment> = partition_tree(&tree, &embedding, &p)?
        .into_iter()
        .map(AnySegment::Tree)
        .collect();
    let schema = Digest([3; 32]);
    let manifest = manifest_of(&segments, &p, schema)?;
    verify_segment(&manifest, &segments[0])?;
    let AnySegment::Tree(original) = &segments[0] else {
        return Err("a tree segment".into());
    };
    // The root moved to another node.
    let mut rooted = original.clone();
    rooted.root = Some(1);
    // The fan's child edges swapped between its labels.
    let mut fanned = original.clone();
    let fan = &mut fanned.fans[0];
    let (x, y) = (fan.child_edges["x"], fan.child_edges["y"]);
    fan.child_edges.insert("x".into(), y);
    fan.child_edges.insert("y".into(), x);
    for changed in [rooted, fanned] {
        let changed = AnySegment::Tree(changed);
        let digest = changed.digest(&schema, &p)?;
        assert_ne!(digest, manifest.segments[0].content);
        assert!(matches!(
            verify_segment(&manifest, &changed),
            Err(SegmentError::DigestMismatch {
                segment: Some(SegmentId(0)),
                ..
            } | SegmentError::Invalid { .. })
        ));
    }
    // A root flip alone keeps the segment's contract, so it is the digest
    // that catches it.
    let mut rooted = original.clone();
    rooted.root = Some(1);
    assert_eq!(
        verify_segment(&manifest, &AnySegment::Tree(rooted.clone())),
        Err(SegmentError::DigestMismatch {
            segment: Some(SegmentId(0)),
            expected: manifest.segments[0].content,
            found: AnySegment::Tree(rooted).digest(&schema, &p)?,
        })
    );
    Ok(())
}

proptest! {
    #[test]
    fn tree_and_table_segments_round_trip_through_canonical_bytes(
        (tree, embedding) in arb_winstance(),
        tables in arb_finstance(),
        k in 1u32..4,
    ) {
        let fail = |e: SegmentError| TestCaseError::fail(e.to_string());
        let schema = Digest([9; 32]);
        let free = next_free(embedding.next_node.checked_sub(1), embedding.next_edge.checked_sub(1));
        let p = interleaved(InstanceShapeKind::Tree, free.max(1), k, 2);
        for segment in partition_tree(&tree, &embedding, &p).map_err(fail)? {
            let segment = AnySegment::Tree(segment);
            let bytes = segment.canonical_bytes(&schema, &p).map_err(fail)?;
            let (back, back_schema) = AnySegment::from_canonical_bytes(&bytes, &p).map_err(fail)?;
            prop_assert_eq!(back_schema, schema);
            prop_assert_eq!(back.canonical_bytes(&schema, &p).map_err(fail)?, bytes);
            let (AnySegment::Tree(a), AnySegment::Tree(b)) = (&segment, &back) else {
                return Err(TestCaseError::fail("a tree segment"));
            };
            prop_assert_eq!(&a.fans, &b.fans);
            prop_assert_eq!(a.root, b.root);
            prop_assert_eq!(&a.arcs, &b.arcs);
            prop_assert_eq!(&a.boundary, &b.boundary);
        }
        let free = next_free(tables.row_ids().last().map(|r| r.0), None);
        let p = interleaved(InstanceShapeKind::Table, free.max(1), k, 2);
        for segment in partition_table(&tables, &p).map_err(fail)? {
            let any = AnySegment::Table(segment.clone());
            let bytes = any.canonical_bytes(&schema, &p).map_err(fail)?;
            let (back, _) = AnySegment::from_canonical_bytes(&bytes, &p).map_err(fail)?;
            let AnySegment::Table(back) = back else {
                return Err(TestCaseError::fail("a table segment"));
            };
            prop_assert_eq!(back, segment);
        }
    }
}

#[test]
fn non_canonical_or_truncated_segment_bytes_are_refused() -> TestResult {
    let tree = small_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let p = Partition::single(
        InstanceShapeKind::Tree,
        Some(embedding.next_node.max(embedding.next_edge)),
    );
    let segment = AnySegment::Tree(partition_tree(&tree, &embedding, &p)?.remove(0));
    let bytes = segment.canonical_bytes(&Digest([0; 32]), &p)?;
    for cut in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        assert!(matches!(
            AnySegment::from_canonical_bytes(&bytes[..cut], &p),
            Err(SegmentError::Decode { .. })
        ));
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        AnySegment::from_canonical_bytes(&trailing, &p),
        Err(SegmentError::Decode { .. })
    ));
    // Under another partition the header's ranges differ from the spec's.
    let other = Partition::single(InstanceShapeKind::Tree, Some(99));
    assert!(matches!(
        AnySegment::from_canonical_bytes(&bytes, &other),
        Err(SegmentError::Decode { .. })
    ));
    Ok(())
}

#[test]
fn a_tampered_ghost_anchor_is_caught_by_the_cross_digests() -> TestResult {
    let mut tables = crate::functor::FInstance::new();
    tables.insert_row("a", crate::view_ref::RowId(0), HashMap::new())?;
    tables.insert_row("b", crate::view_ref::RowId(5), HashMap::new())?;
    let fk = label("a", "b", "fk", None);
    tables.insert_foreign_key(fk, crate::view_ref::RowId(0), crate::view_ref::RowId(5))?;
    let p = Partition::by_node_ranges(InstanceShapeKind::Table, &[3], 0, 6)?;
    let mut parts: Vec<AnySegment> = partition_table(&tables, &p)?
        .into_iter()
        .map(AnySegment::Table)
        .collect();
    let schema = Digest([4; 32]);
    let clean = manifest_of(&parts, &p, schema)?;
    assert_eq!(
        clean.segments[0].cross_out,
        BTreeMap::from([(SegmentId(1), clean.segments[1].cross_in[&SegmentId(0)])])
    );
    // Segment 0's ghost of row 5 claims table `c`: the segment alone keeps
    // its contract, but its cross digest no longer pairs with segment 1's.
    if let AnySegment::Table(s) = &mut parts[0] {
        if let Some(ghost) = s.boundary.ghosts.get_mut(&5) {
            ghost.anchor = "c".into();
        }
    }
    crate::segment::check_segment(&parts[0], &p)?;
    assert_eq!(
        manifest_of(&parts, &p, schema).err(),
        Some(SegmentError::CrossMismatch {
            from: SegmentId(0),
            to: SegmentId(1)
        })
    );
    Ok(())
}

#[test]
fn a_fan_without_its_embedding_records_is_not_a_tree_segment() -> TestResult {
    let tree = small_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let p = Partition::single(
        InstanceShapeKind::Tree,
        Some(embedding.next_node.max(embedding.next_edge)),
    );
    let mut segment = partition_tree(&tree, &embedding, &p)?.remove(0);
    let node = segment.fans[0].node;
    segment.fans.clear();
    assert_eq!(
        crate::segment::check_segment(&AnySegment::Tree(segment), &p),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: crate::view_ref::ViewRef::Node(node)
            }]
        })
    );
    Ok(())
}
