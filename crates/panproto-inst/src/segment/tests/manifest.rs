//! Manifests: sealing, the Merkle root, verification without the segments,
//! audit paths, and the run storage vocabulary.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::process::Command;

use panproto_schema::InstanceShapeKind;
use proptest::prelude::*;

use super::{TestResult, arb_ginstance, interleaved, label, next_free};
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::identity::{
    ComplementShape, Digest, IdentityEnvelope, MANIFEST_LEAF_CONTEXT, MANIFEST_NODE_CONTEXT,
};
use crate::metadata::Node;
use crate::segment::{
    AnySegment, ArtifactKey, ArtifactKind, EdgeIdRun, IdKind, ManifestHeader, ManifestRole,
    Partition, Phase, RunJournal, SegmentError, SegmentId, SegmentManifest, SegmentRecord,
    SegmentStore, TaskKey, TaskRecord, audit_path, merkle_root, partition_graph, seal, seal_full,
    verify_audit_path, verify_manifest, verify_segment,
};

/// The graph of [`segments`]: nodes 0..6, an edge out of every node into
/// the next segment, and a self-loop.
fn ring() -> GInstance {
    let mut graph = GInstance::new();
    for id in 0..6 {
        graph = graph.with_node(Node::new(id, "a"));
    }
    let x = label("a", "a", "prop", None);
    for (id, (s, t)) in [(0, 2), (1, 3), (2, 4), (3, 5), (4, 0), (5, 5)]
        .into_iter()
        .enumerate()
    {
        graph = graph.with_edge_id(EdgeId(u32::try_from(id).unwrap_or(0)), s, t, x.clone());
    }
    graph
}

/// The segments of `graph` under `partition`.
fn segments(graph: &GInstance, partition: &Partition) -> Result<Vec<AnySegment>, SegmentError> {
    Ok(partition_graph(graph, partition)?
        .into_iter()
        .map(AnySegment::Graph)
        .collect())
}

/// The records of `segments` under `schema`.
fn records(segments: &[AnySegment], partition: &Partition, schema: &Digest) -> Vec<SegmentRecord> {
    segments
        .iter()
        .map(|s| SegmentRecord::of(s, &partition.specs[s.segment().0 as usize], schema))
        .collect()
}

/// The sorted edge id run of every segment.
fn runs(segments: &[AnySegment]) -> Vec<EdgeIdRun<'static>> {
    segments
        .iter()
        .map(|s| {
            (
                s.segment(),
                Box::new(s.edge_id_run().into_iter()) as Box<dyn Iterator<Item = EdgeId>>,
            )
        })
        .collect()
}

fn header(binding: Option<Digest>) -> ManifestHeader {
    ManifestHeader {
        shape: InstanceShapeKind::Graph,
        schema: Digest([5; 32]),
        role: ManifestRole::Source,
        binding,
    }
}

/// The sealed manifest of [`ring`] in three segments.
fn ring_manifest(
    binding: Option<Digest>,
) -> Result<(SegmentManifest, Vec<AnySegment>, Partition), SegmentError> {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 3, 6)?;
    let parts = segments(&ring(), &partition)?;
    let manifest = seal(
        records(&parts, &partition, &Digest([5; 32])),
        header(binding),
        runs(&parts),
    )?;
    Ok((manifest, parts, partition))
}

#[test]
fn manifest_binding_changes_the_digest_and_edits_clear_instance_digest() -> TestResult {
    let (plain, _, _) = ring_manifest(None)?;
    let (bound, _, _) = ring_manifest(Some(Digest([8; 32])))?;
    assert_eq!(plain.root, bound.root);
    assert_ne!(plain.digest(), bound.digest());
    assert_eq!(plain.instance_digest, None);
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 3, 6)?;
    let parts = segments(&ring(), &partition)?;
    let sources = parts
        .iter()
        .map(|s| Box::new(s.records()) as Box<dyn crate::segment::RecordSource>)
        .collect();
    let mut full = seal_full(
        records(&parts, &partition, &Digest([5; 32])),
        header(None),
        runs(&parts),
        sources,
    )?;
    assert_eq!(full.instance_digest, Some(ring().digest()));
    let before = full.digest();
    full.clear_instance_digest();
    assert_eq!(full.instance_digest, None);
    assert_eq!(full.digest(), plain.digest());
    assert_ne!(full.digest(), before);
    Ok(())
}

#[test]
fn the_caller_binding_is_covered_by_the_manifest_digest() -> TestResult {
    let (source, _, _) = ring_manifest(Some(Digest([1; 32])))?;
    let (rebound, _, _) = ring_manifest(Some(Digest([2; 32])))?;
    // A view manifest names its source by the source manifest's digest; a
    // source with another binding is not the source it names.
    let envelope = IdentityEnvelope {
        version: 1,
        source_schema: Digest([5; 32]),
        target_schema: Digest([6; 32]),
        lens: Digest([7; 32]),
        complement_format: ComplementShape::Graph.current_format(),
    };
    let role = ManifestRole::View {
        source: source.digest(),
        envelope,
    };
    let ManifestRole::View { source: named, .. } = &role else {
        return Err("a view role".into());
    };
    assert_eq!(*named, source.digest());
    assert_ne!(*named, rebound.digest());
    let mut view = source;
    view.role = role.clone();
    let mut other = view.clone();
    other.role = ManifestRole::View {
        source: rebound.digest(),
        envelope: other_envelope(&role),
    };
    assert_ne!(view.digest(), other.digest());
    Ok(())
}

fn other_envelope(role: &ManifestRole) -> IdentityEnvelope {
    match role {
        ManifestRole::View { envelope, .. } | ManifestRole::Complement { envelope, .. } => {
            envelope.clone()
        }
        ManifestRole::Source => IdentityEnvelope {
            version: 1,
            source_schema: Digest([0; 32]),
            target_schema: Digest([0; 32]),
            lens: Digest([0; 32]),
            complement_format: ComplementShape::Graph.current_format(),
        },
    }
}

#[test]
fn segment_ids_are_instance_global_and_a_caller_local_key_map_never_enters_digests() -> TestResult {
    let (manifest, parts, partition) = ring_manifest(None)?;
    // A caller numbers each segment's nodes densely from 0 in its own map.
    // The map is a presentation: renumbering it changes no digest, because
    // records, cross digests and the root are written over instance-global
    // ids only.
    let local: BTreeMap<SegmentId, BTreeMap<u32, u32>> = parts
        .iter()
        .map(|s| {
            (
                s.segment(),
                s.node_anchors()
                    .iter()
                    .enumerate()
                    .map(|(i, (id, _))| (*id, u32::try_from(i).unwrap_or(0)))
                    .collect(),
            )
        })
        .collect();
    let renumbered: BTreeMap<SegmentId, BTreeMap<u32, u32>> = local
        .iter()
        .map(|(s, m)| (*s, m.iter().map(|(g, l)| (*g, l + 100)).collect()))
        .collect();
    assert_ne!(local, renumbered);
    let again = seal(
        records(&parts, &partition, &Digest([5; 32])),
        header(None),
        runs(&parts),
    )?;
    assert_eq!(again.digest(), manifest.digest());
    // Every segment holds its nodes under their global ids.
    for part in &parts {
        for (id, _) in part.node_anchors() {
            assert_eq!(partition.owner_of_node(id), Some(part.segment()));
        }
    }
    Ok(())
}

/// The reference Merkle tree hash, written from its definition: a single
/// leaf is its own hash; otherwise split at the largest power of two below
/// `n` and combine under the node context.
fn reference_mth(leaves: &[Digest]) -> Digest {
    match leaves.len() {
        0 => Digest::derive(MANIFEST_NODE_CONTEXT, &[]),
        1 => leaves[0],
        n => {
            let k = 1usize << (usize::BITS - 1 - (n - 1).leading_zeros());
            let mut bytes = reference_mth(&leaves[..k]).0.to_vec();
            bytes.extend_from_slice(&reference_mth(&leaves[k..]).0);
            Digest::derive(MANIFEST_NODE_CONTEXT, &bytes)
        }
    }
}

#[test]
fn manifest_root_matches_the_reference_merkle_construction() -> TestResult {
    let (manifest, _, _) = ring_manifest(None)?;
    let leaves: Vec<Digest> = manifest
        .segments
        .iter()
        .map(|r| Digest::derive(MANIFEST_LEAF_CONTEXT, &r.canonical_bytes()))
        .collect();
    assert_eq!(manifest.root, reference_mth(&leaves));
    assert_eq!(merkle_root(&[]), Digest::derive(MANIFEST_NODE_CONTEXT, &[]));
    // Every size from 1 to 17 records, over one record repeated with its id
    // rewritten.
    let template = manifest.segments[0].clone();
    for n in 1..=17u32 {
        let recs: Vec<SegmentRecord> = (0..n)
            .map(|i| {
                let mut r = template.clone();
                r.id = SegmentId(i);
                r.stats.segment = SegmentId(i);
                r
            })
            .collect();
        let leaves: Vec<Digest> = recs.iter().map(SegmentRecord::leaf).collect();
        assert_eq!(merkle_root(&recs), reference_mth(&leaves), "n = {n}");
    }
    Ok(())
}

#[test]
fn a_missing_cross_counterpart_is_a_cross_mismatch() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 3, 6)?;
    let parts = segments(&ring(), &partition)?;
    let schema = Digest([5; 32]);
    let mut recs = records(&parts, &partition, &schema);
    recs[1].cross_in.remove(&SegmentId(0));
    assert_eq!(
        seal(recs, header(None), runs(&parts)).err(),
        Some(SegmentError::CrossMismatch {
            from: SegmentId(0),
            to: SegmentId(1)
        })
    );
    let mut recs = records(&parts, &partition, &schema);
    recs[2].cross_out.insert(SegmentId(1), Digest([0; 32]));
    assert_eq!(
        seal(recs, header(None), runs(&parts)).err(),
        Some(SegmentError::CrossMismatch {
            from: SegmentId(2),
            to: SegmentId(1)
        })
    );
    let mut recs = records(&parts, &partition, &schema);
    let digest = recs[0].cross_out[&SegmentId(1)];
    recs[0]
        .cross_out
        .insert(SegmentId(1), Digest([digest.0[0] ^ 1; 32]));
    assert_eq!(
        seal(recs, header(None), runs(&parts)).err(),
        Some(SegmentError::CrossMismatch {
            from: SegmentId(0),
            to: SegmentId(1)
        })
    );
    Ok(())
}

#[test]
fn duplicate_edge_ids_in_two_segments_are_rejected_at_seal() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 3, 6)?;
    let mut parts = segments(&ring(), &partition)?;
    if let AnySegment::Graph(s) = &mut parts[2] {
        // Edge id 1 is also segment 0's edge 1 -> 3.
        s.edges
            .insert(EdgeId(1), GEdge::new(4, 5, label("a", "a", "prop", None)));
    }
    let schema = Digest([5; 32]);
    assert_eq!(
        seal(
            records(&parts, &partition, &schema),
            header(None),
            runs(&parts)
        )
        .err(),
        Some(SegmentError::DuplicateEdgeId {
            edge: EdgeId(1),
            segments: vec![SegmentId(0), SegmentId(2)]
        })
    );
    Ok(())
}

#[test]
fn an_allocation_range_below_the_global_max_edge_is_rejected_at_seal() -> TestResult {
    // Edge ids reach 5; segment 1 allocates edges from 5 up.
    let mut partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 0, 6)?;
    partition.specs[1].alloc_edges = Some(crate::segment::IdRange { lo: 5, hi: 9 });
    let parts = segments(&ring(), &partition)?;
    assert_eq!(
        seal(
            records(&parts, &partition, &Digest([5; 32])),
            header(None),
            runs(&parts)
        )
        .err(),
        Some(SegmentError::AllocationBelowFloor {
            segment: SegmentId(1),
            kind: IdKind::Edge
        })
    );
    Ok(())
}

/// A store that counts every read, and holds nothing.
#[derive(Default)]
struct CountingStore {
    reads: Cell<u32>,
}

#[derive(Debug, thiserror::Error)]
#[error("the counting store holds nothing")]
struct NothingHeld;

impl SegmentStore for CountingStore {
    type Error = NothingHeld;

    fn read(&self, _: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
        self.reads.set(self.reads.get() + 1);
        Ok(None)
    }

    fn stage(&mut self, _: &ArtifactKey, _: Vec<u8>) -> Result<(), Self::Error> {
        Err(NothingHeld)
    }

    fn commit(&mut self, _: &ArtifactKey, _: &TaskRecord) -> Result<(), Self::Error> {
        Err(NothingHeld)
    }

    fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
        self.reads.set(self.reads.get() + 1);
        Ok(RunJournal::new(*run))
    }

    fn staged_uncommitted(&self, _: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
        Ok(Vec::new())
    }
}

#[test]
fn verify_manifest_reads_no_segment() -> TestResult {
    let (manifest, _, _) = ring_manifest(None)?;
    // The manifest travels as JSON; verification needs nothing else.
    let stored = serde_json::to_vec(&manifest)?;
    let store = CountingStore::default();
    let read: SegmentManifest = serde_json::from_slice(&stored)?;
    verify_manifest(&read)?;
    assert_eq!(store.reads.get(), 0);
    let mut tampered = read.clone();
    tampered.segments[1].stats.payload_bytes += 1;
    assert!(matches!(
        verify_manifest(&tampered),
        Err(SegmentError::DigestMismatch { segment: None, .. })
    ));
    let mut unpaired = read;
    unpaired.segments[1].cross_in.clear();
    unpaired.root = merkle_root(&unpaired.segments);
    assert_eq!(
        verify_manifest(&unpaired),
        Err(SegmentError::CrossMismatch {
            from: SegmentId(0),
            to: SegmentId(1)
        })
    );
    assert_eq!(store.reads.get(), 0);
    Ok(())
}

#[test]
fn a_modified_segment_fails_verify_segment() -> TestResult {
    let (manifest, parts, _) = ring_manifest(None)?;
    for part in &parts {
        verify_segment(&manifest, part)?;
    }
    let mut changed = parts[1].clone();
    if let AnySegment::Graph(s) = &mut changed {
        if let Some(node) = s.nodes.get_mut(&2) {
            node.value = Some(crate::value::FieldPresence::Present(
                crate::value::Value::Int(1),
            ));
        }
    }
    assert!(matches!(
        verify_segment(&manifest, &changed),
        Err(SegmentError::DigestMismatch {
            segment: Some(SegmentId(1)),
            ..
        })
    ));
    // A segment verified against another segment's record.
    let mut moved = parts[1].clone();
    if let AnySegment::Graph(s) = &mut moved {
        s.segment = SegmentId(2);
    }
    assert!(verify_segment(&manifest, &moved).is_err());
    // A record whose content digest is right but whose other fields are
    // not: the segment no longer matches it.
    let mut misrecorded = manifest;
    misrecorded.segments[1].stats.payload_bytes += 1;
    let found = misrecorded.segments[1].leaf();
    misrecorded.segments[1].stats.payload_bytes -= 1;
    let expected = misrecorded.segments[1].leaf();
    misrecorded.segments[1].stats.payload_bytes += 1;
    assert_eq!(
        verify_segment(&misrecorded, &parts[1]),
        Err(SegmentError::DigestMismatch {
            segment: Some(SegmentId(1)),
            expected: found,
            found: expected,
        })
    );
    Ok(())
}

#[test]
fn one_segment_verifies_against_the_root_with_its_audit_path() -> TestResult {
    let (manifest, _, _) = ring_manifest(None)?;
    let template = manifest.segments[0].clone();
    for n in 1..=13u32 {
        let mut m = manifest.clone();
        m.segments = (0..n)
            .map(|i| {
                let mut r = template.clone();
                r.id = SegmentId(i);
                r.content = Digest([u8::try_from(i).unwrap_or(0); 32]);
                r
            })
            .collect();
        m.root = merkle_root(&m.segments);
        let bound = usize::BITS - (n as usize).saturating_sub(1).leading_zeros();
        for i in 0..n {
            let path = audit_path(&m, SegmentId(i));
            assert!(path.len() <= bound as usize, "n = {n}, i = {i}");
            let leaf = m.segments[i as usize].leaf();
            assert!(
                verify_audit_path(&m.root, &leaf, i as usize, n as usize, &path),
                "n = {n}, i = {i}"
            );
            // Another leaf, or another position, does not verify.
            assert!(!verify_audit_path(
                &m.root,
                &Digest([0xEE; 32]),
                i as usize,
                n as usize,
                &path
            ));
            if n > 1 {
                let j = (i + 1) % n;
                assert!(
                    !verify_audit_path(&m.root, &leaf, j as usize, n as usize, &path),
                    "n = {n}, i = {i}"
                );
            }
        }
    }
    Ok(())
}

/// The environment variable that turns a test process into a child that
/// prints one manifest root.
const ROOT_DUMP: &str = "PP_SEGMENT_MANIFEST_ROOT_DUMP";

/// How many separate processes the root is compared across.
const PROCESSES: usize = 32;

#[test]
fn manifest_roots_are_identical_across_processes() -> TestResult {
    let (manifest, _, _) = ring_manifest(Some(Digest([4; 32])))?;
    let answer = format!("{} {}", manifest.root, manifest.digest());
    if std::env::var_os(ROOT_DUMP).is_some() {
        println!("\nROOT {answer}");
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let name = "segment::tests::manifest::manifest_roots_are_identical_across_processes";
    for _ in 0..PROCESSES {
        let out = Command::new(&exe)
            .args([name, "--exact", "--nocapture", "--test-threads=1"])
            .env(ROOT_DUMP, "1")
            .output()?;
        let stdout = String::from_utf8(out.stdout)?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "the child failed: {stdout}{stderr}");
        let line = stdout
            .lines()
            .find_map(|l| l.split_once("ROOT ").map(|(_, root)| root))
            .ok_or_else(|| format!("the child printed no root: {stdout}{stderr}"))?;
        assert_eq!(line, answer);
    }
    Ok(())
}

proptest! {
    #[test]
    fn a_sealed_manifest_verifies_with_every_segment(graph in arb_ginstance(), k in 1u32..4) {
        let fail = |e: SegmentError| TestCaseError::fail(e.to_string());
        let free = next_free(graph.max_node_id(), graph.max_edge_id().map(|e| e.0));
        let partition = interleaved(InstanceShapeKind::Graph, free.max(1), k, 2);
        let parts = segments(&graph, &partition).map_err(fail)?;
        let manifest = seal(records(&parts, &partition, &Digest([5; 32])), header(None), runs(&parts)).map_err(fail)?;
        verify_manifest(&manifest).map_err(fail)?;
        for part in &parts {
            verify_segment(&manifest, part).map_err(fail)?;
        }
    }
}

#[test]
fn task_inputs_name_the_lens_phase_segment_and_inputs_only() {
    let lens = Digest([1; 32]);
    let (a, b) = (Digest([2; 32]), Digest([3; 32]));
    let emit = TaskKey {
        phase: Phase::Emit,
        segment: Some(SegmentId(0)),
    };
    let base = TaskRecord::inputs_digest(&lens, emit, &[a, b]);
    assert_eq!(base, TaskRecord::inputs_digest(&lens, emit, &[b, a]));
    assert_ne!(
        base,
        TaskRecord::inputs_digest(&Digest([9; 32]), emit, &[a, b])
    );
    assert_ne!(
        base,
        TaskRecord::inputs_digest(
            &lens,
            TaskKey {
                phase: Phase::Seal,
                segment: Some(SegmentId(0))
            },
            &[a, b]
        )
    );
    assert_ne!(
        base,
        TaskRecord::inputs_digest(
            &lens,
            TaskKey {
                phase: Phase::Emit,
                segment: None
            },
            &[a, b]
        )
    );
    assert_ne!(base, TaskRecord::inputs_digest(&lens, emit, &[a]));
    let phases = [
        Phase::Validate,
        Phase::Export,
        Phase::Summarize,
        Phase::Assemble,
        Phase::Emit,
        Phase::Seal,
    ];
    assert!(phases.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        serde_json::to_string(&phases).ok(),
        Some(r#"["validate","export","summarize","assemble","emit","seal"]"#.to_owned())
    );
    let mut journal = RunJournal::new(Digest([7; 32]));
    journal.tasks.insert(
        emit,
        TaskRecord {
            inputs: base,
            output: a,
            bytes: 10,
        },
    );
    assert!(journal.reusable(&emit, &base, &a));
    assert!(!journal.reusable(&emit, &b, &a));
    assert!(!journal.reusable(&emit, &base, &b));
    let kinds = [
        ArtifactKind::Segment,
        ArtifactKind::ComplementSegment,
        ArtifactKind::Export,
        ArtifactKind::Summary,
        ArtifactKind::Assembly,
        ArtifactKind::Manifest,
        ArtifactKind::EdgeIdRun,
    ];
    assert_eq!(
        serde_json::to_string(&kinds).ok(),
        Some(r#"["segment","complement_segment","export","summary","assembly","manifest","edge_id_run"]"#.to_owned())
    );
}

#[test]
fn a_run_journal_round_trips_and_its_run_digest_covers_every_input() -> TestResult {
    let lens = Digest([1; 32]);
    let input = Digest([2; 32]);
    let p = interleaved(InstanceShapeKind::Graph, 6, 2, 1);
    let run = RunJournal::run_digest(&lens, &input, &p, b"get");
    assert_ne!(run, RunJournal::run_digest(&lens, &input, &p, b"put"));
    assert_ne!(
        run,
        RunJournal::run_digest(&lens, &Digest([3; 32]), &p, b"get")
    );
    assert_ne!(
        run,
        RunJournal::run_digest(
            &lens,
            &input,
            &interleaved(InstanceShapeKind::Graph, 6, 3, 1),
            b"get"
        )
    );
    let mut journal = RunJournal::new(run);
    journal.tasks.insert(
        TaskKey {
            phase: Phase::Assemble,
            segment: None,
        },
        TaskRecord {
            inputs: input,
            output: lens,
            bytes: 3,
        },
    );
    journal.tasks.insert(
        TaskKey {
            phase: Phase::Validate,
            segment: Some(SegmentId(1)),
        },
        TaskRecord {
            inputs: lens,
            output: input,
            bytes: 4,
        },
    );
    let json = serde_json::to_string(&journal)?;
    assert_eq!(serde_json::from_str::<RunJournal>(&json)?, journal);
    let key = ArtifactKey {
        run,
        task: TaskKey {
            phase: Phase::Emit,
            segment: Some(SegmentId(0)),
        },
        kind: ArtifactKind::ComplementSegment,
    };
    let back: ArtifactKey = serde_json::from_str(&serde_json::to_string(&key)?)?;
    assert_eq!(back, key);
    Ok(())
}
