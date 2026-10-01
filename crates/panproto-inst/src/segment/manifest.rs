//! Segment records, manifests and their verification.
//!
//! A [`SegmentManifest`] lists one [`SegmentRecord`] per segment and binds
//! them under a Merkle root: leaf `i` is the digest of record `i`'s
//! canonical bytes under [`MANIFEST_LEAF_CONTEXT`], and for `n > 1`
//!
//! ```text
//! MTH(D[0:n]) = derive(MANIFEST_NODE_CONTEXT, MTH(D[0:k]) || MTH(D[k:n]))
//! ```
//!
//! with `k` the largest power of two smaller than `n`, the shape of Laurie
//! et al. 2013, Sec 2.1 (stated), with the leaf and node domains separated
//! by context in place of its one-byte prefixes (inferred). The root of no
//! records is `derive(MANIFEST_NODE_CONTEXT, [])`. One record is checked
//! against a root with its audit path, at most `ceil(log2 n)` sibling
//! digests (inferred), without the other records.

use std::collections::BTreeMap;

use panproto_schema::{CanonicalWriter, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::body::{AnySegment, SegmentStats};
use super::check::check_segment_with;
use super::error::{PartitionDefect, SegmentError};
use super::ids::{IdRange, SegmentId, check_edge_range_above};
use super::partition::{Partition, SegmentSpec, shape_tag, write_opt_range, write_ranges};
use super::stream::{RecordSource, instance_digest};
use crate::ginstance::EdgeId;
use crate::identity::{
    CanonicalSink, ComplementShape, Digest, IdentityEnvelope, MANIFEST_CONTEXT,
    MANIFEST_LEAF_CONTEXT, MANIFEST_NODE_CONTEXT, SEGMENT_MANIFEST_VERSION,
};

/// The record of one segment in a manifest.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentRecord {
    /// The segment.
    pub id: SegmentId,
    /// Its node ranges.
    pub nodes: Vec<IdRange>,
    /// Its node allocation range.
    pub alloc_nodes: Option<IdRange>,
    /// Its edge allocation range.
    pub alloc_edges: Option<IdRange>,
    /// Its counts.
    pub stats: SegmentStats,
    /// Its largest owned node id.
    pub max_node: Option<u32>,
    /// Its largest owned edge id.
    pub max_edge: Option<u32>,
    /// Its content digest ([`AnySegment::digest`]).
    pub content: Digest,
    /// The cross digest of its edges into each peer.
    pub cross_out: BTreeMap<SegmentId, Digest>,
    /// The cross digest of each peer's edges into it.
    pub cross_in: BTreeMap<SegmentId, Digest>,
}

/// What a manifest's instance is: a source, a view of one, or the
/// complement of a view, each bound to its lens by the identity envelope.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestRole {
    /// A source instance.
    Source,
    /// The view `get` made of the source with manifest digest `source`.
    View {
        /// The source manifest's digest.
        source: Digest,
        /// The lens's envelope.
        envelope: IdentityEnvelope,
    },
    /// The complement `get` made beside the view with manifest digest
    /// `view`, of the source with manifest digest `source`.
    Complement {
        /// The source manifest's digest.
        source: Digest,
        /// The view manifest's digest.
        view: Digest,
        /// The lens's envelope.
        envelope: IdentityEnvelope,
    },
}

/// The manifest of an instance held as segments.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentManifest {
    /// The manifest layout version, [`SEGMENT_MANIFEST_VERSION`].
    pub version: u16,
    /// The instance's shape.
    pub shape: InstanceShapeKind,
    /// The schema digest every segment is written under.
    pub schema: Digest,
    /// What the instance is.
    pub role: ManifestRole,
    /// An opaque caller digest (an application profile, for instance),
    /// covered by the manifest digest and never interpreted.
    pub binding: Option<Digest>,
    /// One record per segment, in ascending dense id order.
    pub segments: Vec<SegmentRecord>,
    /// The Merkle root over the records.
    pub root: Digest,
    /// The instance digest, when the manifest was sealed with it
    /// ([`seal_full`]); an edit clears it.
    pub instance_digest: Option<Digest>,
}

/// The fields of a manifest that its records do not determine.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ManifestHeader {
    /// The instance's shape.
    pub shape: InstanceShapeKind,
    /// The schema digest every segment is written under.
    pub schema: Digest,
    /// What the instance is.
    pub role: ManifestRole,
    /// An opaque caller digest.
    pub binding: Option<Digest>,
}

/// Write the stats' canonical bytes.
fn write_stats<S: CanonicalSink + ?Sized>(sink: &mut S, stats: &SegmentStats) {
    sink.u32(stats.segment.0);
    sink.u64(stats.nodes);
    sink.u64(stats.edges);
    sink.u64(stats.fans);
    sink.u64(stats.ghosts);
    sink.u64(stats.in_stubs);
    sink.u64(stats.payload_bytes);
}

/// Write an optional digest as a presence byte and the digest.
fn write_opt_digest<S: CanonicalSink + ?Sized>(sink: &mut S, digest: Option<&Digest>) {
    match digest {
        None => sink.bool(false),
        Some(d) => {
            sink.bool(true);
            sink.write(&d.0);
        }
    }
}

/// Write a map of digests by segment: count, then `(segment, digest)`.
fn write_digest_map<S: CanonicalSink + ?Sized>(sink: &mut S, map: &BTreeMap<SegmentId, Digest>) {
    sink.len(map.len());
    for (segment, digest) in map {
        sink.u32(segment.0);
        sink.write(&digest.0);
    }
}

/// Write an identity envelope: version, the three digests, then the
/// complement shape tag and format version.
fn write_envelope<S: CanonicalSink + ?Sized>(sink: &mut S, envelope: &IdentityEnvelope) {
    sink.write(&envelope.version.to_le_bytes());
    sink.write(&envelope.source_schema.0);
    sink.write(&envelope.target_schema.0);
    sink.write(&envelope.lens.0);
    sink.tag(match envelope.complement_format.shape {
        ComplementShape::WType => 0,
        ComplementShape::Graph => 1,
        ComplementShape::Functor => 2,
        ComplementShape::Resolution => 3,
    });
    sink.write(&envelope.complement_format.version.to_le_bytes());
}

impl SegmentRecord {
    /// The record of `segment`, with the ranges of `spec`, written under
    /// the schema digest `schema`.
    #[must_use]
    pub fn of(segment: &AnySegment, spec: &SegmentSpec, schema: &Digest) -> Self {
        Self {
            id: segment.segment(),
            nodes: spec.nodes.clone(),
            alloc_nodes: spec.alloc_nodes,
            alloc_edges: spec.alloc_edges,
            stats: segment.stats(),
            max_node: segment.max_node(),
            max_edge: segment.max_edge(),
            content: segment.digest_with(schema, spec),
            cross_out: segment.cross_out(),
            cross_in: segment.cross_in(),
        }
    }

    /// The spec the record states.
    #[must_use]
    pub fn spec(&self) -> SegmentSpec {
        SegmentSpec {
            id: self.id,
            nodes: self.nodes.clone(),
            alloc_nodes: self.alloc_nodes,
            alloc_edges: self.alloc_edges,
        }
    }

    /// Write the record's canonical bytes: every field in declaration
    /// order.
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.u32(self.id.0);
        write_ranges(sink, &self.nodes);
        write_opt_range(sink, self.alloc_nodes);
        write_opt_range(sink, self.alloc_edges);
        write_stats(sink, &self.stats);
        for max in [self.max_node, self.max_edge] {
            match max {
                None => sink.bool(false),
                Some(id) => {
                    sink.bool(true);
                    sink.u32(id);
                }
            }
        }
        sink.write(&self.content.0);
        write_digest_map(sink, &self.cross_out);
        write_digest_map(sink, &self.cross_in);
    }

    /// The record's canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut writer = CanonicalWriter::new();
        self.write(&mut writer);
        writer.into_bytes()
    }

    /// The record's Merkle leaf: its canonical bytes under
    /// [`MANIFEST_LEAF_CONTEXT`].
    #[must_use]
    pub fn leaf(&self) -> Digest {
        let mut hasher = Digest::hasher(MANIFEST_LEAF_CONTEXT);
        self.write(&mut hasher);
        hasher.finalize()
    }
}

/// The node digest of two subtrees: [`MANIFEST_NODE_CONTEXT`] over the
/// left digest followed by the right.
fn node_digest(left: &Digest, right: &Digest) -> Digest {
    let mut hasher = Digest::hasher(MANIFEST_NODE_CONTEXT);
    hasher.update(&left.0);
    hasher.update(&right.0);
    hasher.finalize()
}

/// The largest power of two smaller than `n`, for `n > 1`.
const fn split_point(n: usize) -> usize {
    let mut k = 1;
    while k * 2 < n {
        k *= 2;
    }
    k
}

/// The Merkle tree hash of the leaves.
fn tree_hash(leaves: &[Digest]) -> Digest {
    match leaves {
        [] => Digest::derive(MANIFEST_NODE_CONTEXT, &[]),
        [leaf] => *leaf,
        _ => {
            let k = split_point(leaves.len());
            node_digest(&tree_hash(&leaves[..k]), &tree_hash(&leaves[k..]))
        }
    }
}

/// The Merkle root over `records`: see the module documentation.
#[must_use]
pub fn merkle_root(records: &[SegmentRecord]) -> Digest {
    let leaves: Vec<Digest> = records.iter().map(SegmentRecord::leaf).collect();
    tree_hash(&leaves)
}

/// The audit path of leaf `m` among `leaves`: the sibling digests from the
/// leaf up to the root.
fn path(m: usize, leaves: &[Digest]) -> Vec<Digest> {
    if leaves.len() <= 1 {
        return Vec::new();
    }
    let k = split_point(leaves.len());
    if m < k {
        let mut out = path(m, &leaves[..k]);
        out.push(tree_hash(&leaves[k..]));
        out
    } else {
        let mut out = path(m - k, &leaves[k..]);
        out.push(tree_hash(&leaves[..k]));
        out
    }
}

/// The audit path of segment `id` in `manifest`: the sibling digests from
/// its leaf up to the root, at most `ceil(log2 n)` of them for `n` records.
/// Empty when the manifest holds no record for `id`.
#[must_use]
pub fn audit_path(manifest: &SegmentManifest, id: SegmentId) -> Vec<Digest> {
    let leaves: Vec<Digest> = manifest.segments.iter().map(SegmentRecord::leaf).collect();
    match usize::try_from(id.0) {
        Ok(m) if m < leaves.len() => path(m, &leaves),
        _ => Vec::new(),
    }
}

/// Whether `leaf`, at position `index` among `n` leaves, is bound to `root`
/// by the audit path `path`, without any other leaf.
///
/// ```
/// use panproto_inst::identity::Digest;
/// use panproto_inst::segment::verify_audit_path;
///
/// // One leaf is its own root, with an empty path.
/// let leaf = Digest([7; 32]);
/// assert!(verify_audit_path(&leaf, &leaf, 0, 1, &[]));
/// assert!(!verify_audit_path(&leaf, &leaf, 1, 1, &[]));
/// ```
#[must_use]
pub fn verify_audit_path(
    root: &Digest,
    leaf: &Digest,
    index: usize,
    n: usize,
    path: &[Digest],
) -> bool {
    if index >= n {
        return false;
    }
    let (mut fn_, mut sn) = (index, n - 1);
    let mut r = *leaf;
    for sibling in path {
        if sn == 0 {
            return false;
        }
        if fn_ & 1 == 1 || fn_ == sn {
            r = node_digest(sibling, &r);
            if fn_ & 1 == 0 {
                while fn_ & 1 == 0 && fn_ != 0 {
                    fn_ >>= 1;
                    sn >>= 1;
                }
            }
        } else {
            r = node_digest(&r, sibling);
        }
        fn_ >>= 1;
        sn >>= 1;
    }
    sn == 0 && r == *root
}

/// The decode error of a manifest.
const fn manifest_error(message: String) -> SegmentError {
    SegmentError::Decode {
        artifact: super::store::ArtifactKind::Manifest,
        message,
    }
}

/// The partition the records state, checked: dense ids, valid ranges, and
/// each record's stats naming its own segment.
fn records_partition(
    records: &[SegmentRecord],
    shape: InstanceShapeKind,
) -> Result<Partition, SegmentError> {
    if let Some(r) = records.iter().find(|r| r.stats.segment != r.id) {
        return Err(manifest_error(format!(
            "the record of {} carries the stats of {}",
            r.id, r.stats.segment
        )));
    }
    let partition = Partition {
        shape,
        specs: records.iter().map(SegmentRecord::spec).collect(),
    };
    partition.validate()?;
    Ok(partition)
}

/// Check that every cross digest has its counterpart: `cross_out[j]` of `i`
/// equals `cross_in[i]` of `j`, in both directions.
fn check_cross_pairs(records: &[SegmentRecord]) -> Result<(), SegmentError> {
    let mut mismatches: Vec<(SegmentId, SegmentId)> = Vec::new();
    let record = |id: SegmentId| records.get(usize::try_from(id.0).unwrap_or(usize::MAX));
    for rec in records {
        for (to, digest) in &rec.cross_out {
            if record(*to).and_then(|peer| peer.cross_in.get(&rec.id)) != Some(digest)
                || *to == rec.id
            {
                mismatches.push((rec.id, *to));
            }
        }
        for (from, digest) in &rec.cross_in {
            if record(*from).and_then(|peer| peer.cross_out.get(&rec.id)) != Some(digest)
                || *from == rec.id
            {
                mismatches.push((*from, rec.id));
            }
        }
    }
    mismatches.into_iter().min().map_or(Ok(()), |(from, to)| {
        Err(SegmentError::CrossMismatch { from, to })
    })
}

/// Check that every edge allocation range lies above the largest edge id
/// any record holds.
fn check_edge_floors(records: &[SegmentRecord]) -> Result<(), SegmentError> {
    let max_edge = records.iter().filter_map(|r| r.max_edge).max();
    for rec in records {
        if let Some(range) = rec.alloc_edges {
            check_edge_range_above(rec.id, range, max_edge)?;
        }
    }
    Ok(())
}

/// A sorted run of one segment's owned edge ids.
pub type EdgeIdRun<'a> = (SegmentId, Box<dyn Iterator<Item = EdgeId> + 'a>);

/// Merge the segments' sorted edge id runs and check that no edge id is
/// held twice, holding one id per run at a time.
///
/// # Errors
///
/// [`SegmentError::DuplicateEdgeId`] for the least repeated id, with every
/// segment holding it, and [`SegmentError::Decode`] when a run is not
/// strictly ascending.
pub fn check_edge_id_runs(runs: Vec<EdgeIdRun<'_>>) -> Result<(), SegmentError> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut runs = runs;
    let mut heap = BinaryHeap::new();
    for (i, (_, run)) in runs.iter_mut().enumerate() {
        if let Some(id) = run.next() {
            heap.push(Reverse((id, i)));
        }
    }
    let mut group: Option<(EdgeId, Vec<SegmentId>)> = None;
    while let Some(Reverse((id, i))) = heap.pop() {
        let segment = runs[i].0;
        match &mut group {
            Some((held, segments)) if *held == id => segments.push(segment),
            _ => {
                if let Some((edge, mut segments)) = group.take()
                    && segments.len() > 1
                {
                    segments.sort_unstable();
                    return Err(SegmentError::DuplicateEdgeId { edge, segments });
                }
                group = Some((id, vec![segment]));
            }
        }
        if let Some(next) = runs[i].1.next() {
            if next <= id {
                return Err(SegmentError::Decode {
                    artifact: super::store::ArtifactKind::EdgeIdRun,
                    message: format!("the edge id run of {segment} is not strictly ascending"),
                });
            }
            heap.push(Reverse((next, i)));
        }
    }
    match group {
        Some((edge, mut segments)) if segments.len() > 1 => {
            segments.sort_unstable();
            Err(SegmentError::DuplicateEdgeId { edge, segments })
        }
        _ => Ok(()),
    }
}

/// Write a manifest role: a one-byte tag (`0` source, `1` view, `2`
/// complement), then its digests and envelope.
fn write_role<S: CanonicalSink + ?Sized>(sink: &mut S, role: &ManifestRole) {
    match role {
        ManifestRole::Source => sink.tag(0),
        ManifestRole::View { source, envelope } => {
            sink.tag(1);
            sink.write(&source.0);
            write_envelope(sink, envelope);
        }
        ManifestRole::Complement {
            source,
            view,
            envelope,
        } => {
            sink.tag(2);
            sink.write(&source.0);
            sink.write(&view.0);
            write_envelope(sink, envelope);
        }
    }
}

impl SegmentManifest {
    /// Write the manifest's canonical bytes: every field in declaration
    /// order, the records as their count and each record's bytes
    /// ([`SegmentRecord::write`]).
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.write(&self.version.to_le_bytes());
        sink.tag(shape_tag(self.shape));
        sink.write(&self.schema.0);
        write_role(sink, &self.role);
        write_opt_digest(sink, self.binding.as_ref());
        sink.len(self.segments.len());
        for record in &self.segments {
            record.write(sink);
        }
        sink.write(&self.root.0);
        write_opt_digest(sink, self.instance_digest.as_ref());
    }

    /// The manifest's canonical bytes, as [`SegmentManifest::write`] writes
    /// them.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut writer = CanonicalWriter::new();
        self.write(&mut writer);
        writer.into_bytes()
    }

    /// The manifest's digest: its canonical bytes under
    /// [`MANIFEST_CONTEXT`]. The caller's `binding` is among them, so two
    /// manifests that differ only in their binding have different digests.
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(MANIFEST_CONTEXT);
        self.write(&mut hasher);
        hasher.finalize()
    }

    /// The partition the manifest's records state.
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] of the manifest when a record carries the
    /// stats of another segment, and the partition's own error when the
    /// records' ids are not dense or their ranges are malformed.
    pub fn partition(&self) -> Result<Partition, SegmentError> {
        records_partition(&self.segments, self.shape)
    }

    /// The record of segment `id`, if the manifest lists it.
    #[must_use]
    pub fn record(&self, id: SegmentId) -> Option<&SegmentRecord> {
        self.segments
            .get(usize::try_from(id.0).unwrap_or(usize::MAX))
            .filter(|r| r.id == id)
    }

    /// The largest edge id any record holds, the floor every edge
    /// allocation range of a run over this manifest lies above.
    #[must_use]
    pub fn max_edge(&self) -> Option<u32> {
        self.segments.iter().filter_map(|r| r.max_edge).max()
    }

    /// Record that the instance was edited: the streaming instance digest
    /// no longer describes it, so it is cleared.
    pub const fn clear_instance_digest(&mut self) {
        self.instance_digest = None;
    }
}

/// Seal the records of a segmented instance into a manifest.
///
/// The records must be dense from segment 0 with valid, disjoint ranges;
/// every cross digest must have its counterpart (`cross_out[j]` of `i`
/// equal to `cross_in[i]` of `j`, both ways); every edge allocation range
/// must lie above the largest edge id any record holds; and the segments'
/// sorted edge id runs, one per segment in `runs`, must hold no edge id
/// twice. The merge holds one id per run at a time.
///
/// # Errors
///
/// The partition's error when the records' ranges are malformed,
/// [`SegmentError::CrossMismatch`] for the least `(from, to)` pair whose
/// digests disagree or lack a counterpart,
/// [`SegmentError::AllocationBelowFloor`] for the first segment whose edge
/// allocation range does not lie above every edge id, and
/// [`SegmentError::DuplicateEdgeId`] for the least edge id two runs hold.
pub fn seal(
    records: Vec<SegmentRecord>,
    header: ManifestHeader,
    runs: Vec<EdgeIdRun<'_>>,
) -> Result<SegmentManifest, SegmentError> {
    records_partition(&records, header.shape)?;
    check_cross_pairs(&records)?;
    check_edge_floors(&records)?;
    check_edge_id_runs(runs)?;
    let root = merkle_root(&records);
    Ok(SegmentManifest {
        version: SEGMENT_MANIFEST_VERSION,
        shape: header.shape,
        schema: header.schema,
        role: header.role,
        binding: header.binding,
        segments: records,
        root,
        instance_digest: None,
    })
}

/// [`seal`], and then the streaming instance digest of the segments that
/// `sources` stream ([`instance_digest`]), recorded in the manifest.
///
/// # Errors
///
/// As [`seal`], and as [`instance_digest`]; [`SegmentError::Decode`] of the
/// manifest when a source's stats differ from its record's.
pub fn seal_full<'a>(
    records: Vec<SegmentRecord>,
    header: ManifestHeader,
    runs: Vec<EdgeIdRun<'_>>,
    sources: Vec<Box<dyn RecordSource + 'a>>,
) -> Result<SegmentManifest, SegmentError> {
    let shape = header.shape;
    let mut manifest = seal(records, header, runs)?;
    let stated: Vec<SegmentStats> = manifest.segments.iter().map(|r| r.stats).collect();
    let streamed: Vec<SegmentStats> = sources.iter().map(|s| s.stats()).collect();
    if stated != streamed {
        return Err(manifest_error(
            "the record sources do not carry the records' stats".to_owned(),
        ));
    }
    manifest.instance_digest = Some(instance_digest(shape, sources)?);
    Ok(manifest)
}

/// Verify a manifest from its records alone, reading no segment: its
/// version, the partition its records state, its Merkle root and the
/// pairing of every cross digest.
///
/// # Errors
///
/// [`SegmentError::Decode`] of the manifest for another layout version, the
/// partition's error when the records' ranges are malformed,
/// [`SegmentError::DigestMismatch`] (with no segment) when the root is not
/// the Merkle root of the records, and [`SegmentError::CrossMismatch`] for
/// the least unpaired cross digest.
pub fn verify_manifest(manifest: &SegmentManifest) -> Result<(), SegmentError> {
    if manifest.version != SEGMENT_MANIFEST_VERSION {
        return Err(manifest_error(format!(
            "manifest layout version {}",
            manifest.version
        )));
    }
    records_partition(&manifest.segments, manifest.shape)?;
    let root = merkle_root(&manifest.segments);
    if root != manifest.root {
        return Err(SegmentError::DigestMismatch {
            segment: None,
            expected: manifest.root,
            found: root,
        });
    }
    check_cross_pairs(&manifest.segments)
}

/// Verify one segment against its record in `manifest`: the segment keeps
/// its own contract under the manifest's partition, its content digest
/// under the manifest's schema is the recorded one, and every other field
/// of its record (stats, largest ids, cross digests) is the one it yields.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] for a segment of another shape,
/// [`SegmentError::MalformedPartition`] when the manifest lists no record
/// for it, the partition's error for malformed records,
/// [`SegmentError::Invalid`] when it breaks its contract, and
/// [`SegmentError::DigestMismatch`] for the segment when its content digest
/// differs from the record's (or, with the leaf digests, when another field
/// of the record does).
pub fn verify_segment(
    manifest: &SegmentManifest,
    segment: &AnySegment,
) -> Result<(), SegmentError> {
    if segment.shape() != manifest.shape {
        return Err(SegmentError::ShapeMismatch {
            expected: manifest.shape,
            found: segment.shape(),
        });
    }
    let id = segment.segment();
    let record = manifest
        .record(id)
        .ok_or(SegmentError::MalformedPartition {
            segment: id,
            defect: PartitionDefect::UnknownSegment,
        })?;
    let partition = manifest.partition()?;
    let index = partition.owner_index()?;
    check_segment_with(segment, &index)?;
    let found = SegmentRecord::of(segment, &record.spec(), &manifest.schema);
    if found.content != record.content {
        return Err(SegmentError::DigestMismatch {
            segment: Some(id),
            expected: record.content,
            found: found.content,
        });
    }
    if found != *record {
        return Err(SegmentError::DigestMismatch {
            segment: Some(id),
            expected: record.leaf(),
            found: found.leaf(),
        });
    }
    Ok(())
}

/// Decode a segment from its canonical bytes and verify it against its
/// record in `manifest` ([`verify_segment`]), first checking that the bytes
/// were written under the manifest's schema.
///
/// # Errors
///
/// As [`AnySegment::from_canonical_bytes`] under the manifest's partition;
/// [`SegmentError::SchemaMismatch`] when the bytes carry another schema
/// digest; and as [`verify_segment`].
pub fn verify_segment_bytes(
    manifest: &SegmentManifest,
    bytes: &[u8],
) -> Result<AnySegment, SegmentError> {
    let partition = manifest.partition()?;
    let (segment, schema) = AnySegment::from_canonical_bytes(bytes, &partition)?;
    if schema != manifest.schema {
        return Err(SegmentError::SchemaMismatch {
            expected: manifest.schema,
            found: schema,
        });
    }
    verify_segment(manifest, &segment)?;
    Ok(segment)
}
