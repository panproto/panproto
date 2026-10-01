//! Canonical bytes and digests of segments, and the cross digests that pair
//! segments up.
//!
//! A segment's bytes are a header and then self-contained records, written
//! with the record encoders the whole instances use, so a record has one
//! encoding everywhere:
//!
//! ```text
//! header  = version: u16, shape: u8, schema: [u8; 32], segment: u32,
//!           node ranges, node allocation range?, edge allocation range?
//!           (trees add) root: u32?
//! graph   = nodes: count, node records (ascending id);
//!           edges: count, edge records (ascending edge id)
//! table   = rows: count, row records (ascending row id);
//!           pairs: count, pair records (ascending (edge, src, tgt))
//! tree    = nodes: count, node records (ascending id);
//!           arcs: count, (edge id, parent, child, label) (ascending edge id);
//!           fans: count, (node, parent edge, child edges: count,
//!                 (label, edge id) in label order) (ascending fan node)
//! ghosts  = count, (node, owner, anchor)             (ascending node)
//! stubs   = count, (element, src, src owner, src anchor, tgt, label)
//!                                                    (ascending element)
//! ```
//!
//! A range is its two bounds; an optional field is a presence byte and then
//! the field; an element is a one-byte tag (`0` node, `1` edge, `2` arc,
//! `3` row, `4` pair) and its fields. The schema digest in the header binds
//! the segment to its schema: the same contents under another schema have
//! another digest.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::CanonicalWriter;

use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub, TableSegment, TreeSegment};
use super::check::{check_listed, check_segment_with};
use super::error::{PartitionDefect, SegmentError};
use super::ids::SegmentId;
use super::partition::{Partition, SegmentSpec, shape_from_tag, shape_tag, write_opt_range, write_ranges};
use super::reader::Reader;
use super::store::ArtifactKind;
use crate::functor::{write_pair_record, write_row_record};
use crate::ginstance::{EdgeId, FanEmbedding, GEdge, write_edge_record, write_node_record};
use crate::identity::{
    CanonicalSink, Digest, SEGMENT_CONTEXT, SEGMENT_CROSS_CONTEXT, SEGMENT_FORMAT_VERSION,
    write_value_map,
};
use crate::metadata::Node;
use crate::value::FieldPresence;
use crate::view_ref::{RowId, ViewRef};

/// Write an element reference: a one-byte tag and its fields.
pub fn write_view_ref<S: CanonicalSink + ?Sized>(sink: &mut S, elem: &ViewRef) {
    match elem {
        ViewRef::Node(id) => {
            sink.tag(0);
            sink.u32(*id);
        }
        ViewRef::Edge(id) => {
            sink.tag(1);
            sink.u32(id.0);
        }
        ViewRef::Arc { parent, child } => {
            sink.tag(2);
            sink.u32(*parent);
            sink.u32(*child);
        }
        ViewRef::Row(id) => {
            sink.tag(3);
            sink.u32(id.0);
        }
        ViewRef::Pair { edge, src, tgt } => {
            sink.tag(4);
            sink.edge(edge);
            sink.u32(src.0);
            sink.u32(tgt.0);
        }
    }
}

/// A sink that only counts the bytes written to it.
#[derive(Default)]
struct Counter(u64);

impl CanonicalSink for Counter {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = self
            .0
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    }
}

/// The canonical bytes of every value a segment holds: each node's value,
/// extra fields and annotations, and each row's cells.
pub(crate) fn payload_bytes(segment: &AnySegment) -> u64 {
    let mut counter = Counter::default();
    let mut node = |n: &Node| {
        if let Some(FieldPresence::Present(value)) = &n.value {
            counter.value(value);
        }
        write_value_map(&mut counter, &n.extra_fields);
        write_value_map(&mut counter, &n.annotations);
    };
    match segment {
        AnySegment::Graph(s) => s.nodes.values().for_each(&mut node),
        AnySegment::Tree(s) => s.nodes.values().for_each(&mut node),
        AnySegment::Table(s) => {
            for (_, row) in s.rows.values() {
                write_value_map(&mut counter, row);
            }
        }
    }
    counter.0
}

/// Write the boundary: ghosts ascending by node, then in-stubs ascending by
/// element.
fn write_boundary<S: CanonicalSink + ?Sized>(sink: &mut S, boundary: &Boundary) {
    sink.len(boundary.ghosts.len());
    for ghost in boundary.ghosts.values() {
        sink.u32(ghost.node);
        sink.u32(ghost.owner.0);
        sink.name(&ghost.anchor);
    }
    sink.len(boundary.in_stubs.len());
    for (elem, stub) in &boundary.in_stubs {
        write_view_ref(sink, elem);
        sink.u32(stub.src);
        sink.u32(stub.src_owner.0);
        sink.name(&stub.src_anchor);
        sink.u32(stub.tgt);
        sink.edge(&stub.label);
    }
}

/// Write a fan's embedding record: the fan node, the parent edge, then the
/// child edges in label order.
fn write_fan_embedding<S: CanonicalSink + ?Sized>(sink: &mut S, fan: &FanEmbedding) {
    sink.u32(fan.node);
    sink.u32(fan.parent_edge.0);
    sink.len(fan.child_edges.len());
    for (label, id) in &fan.child_edges {
        sink.str(label);
        sink.u32(id.0);
    }
}

/// Write a segment's canonical bytes into `sink`, with the header of
/// `spec` under the schema digest `schema`. See the module documentation for
/// the layout.
pub fn write_segment<S: CanonicalSink + ?Sized>(
    sink: &mut S,
    segment: &AnySegment,
    schema: &Digest,
    spec: &SegmentSpec,
) {
    sink.write(&SEGMENT_FORMAT_VERSION.to_le_bytes());
    sink.tag(shape_tag(segment.shape()));
    sink.write(&schema.0);
    sink.u32(segment.segment().0);
    write_ranges(sink, &spec.nodes);
    write_opt_range(sink, spec.alloc_nodes);
    write_opt_range(sink, spec.alloc_edges);
    match segment {
        AnySegment::Graph(s) => {
            sink.len(s.nodes.len());
            for node in s.nodes.values() {
                write_node_record(sink, node);
            }
            sink.len(s.edges.len());
            for (id, edge) in &s.edges {
                write_edge_record(sink, *id, edge);
            }
        }
        AnySegment::Table(s) => {
            sink.len(s.rows.len());
            for (id, (table, row)) in &s.rows {
                write_row_record(sink, *id, table, row);
            }
            sink.len(s.pairs.len());
            for (edge, src, tgt) in &s.pairs {
                write_pair_record(sink, edge, *src, *tgt);
            }
        }
        AnySegment::Tree(s) => {
            match s.root {
                None => sink.bool(false),
                Some(root) => {
                    sink.bool(true);
                    sink.u32(root);
                }
            }
            sink.len(s.nodes.len());
            for node in s.nodes.values() {
                write_node_record(sink, node);
            }
            sink.len(s.arcs.len());
            for (id, (parent, child, edge)) in &s.arcs {
                sink.u32(id.0);
                sink.u32(*parent);
                sink.u32(*child);
                sink.edge(edge);
            }
            sink.len(s.fans.len());
            for fan in &s.fans {
                write_fan_embedding(sink, fan);
            }
        }
    }
    write_boundary(sink, segment.boundary());
}

/// The spec of `segment` in `partition`, checking that the partition lists
/// it with its shape.
fn spec_of<'p>(segment: &AnySegment, partition: &'p Partition) -> Result<&'p SegmentSpec, SegmentError> {
    check_listed(segment, partition)?;
    partition
        .spec(segment.segment())
        .ok_or(SegmentError::MalformedPartition {
            segment: segment.segment(),
            defect: PartitionDefect::UnknownSegment,
        })
}

/// Fail unless `key` is strictly above the previous key.
fn ascending<K: Ord + Clone>(reader: &Reader<'_>, last: &mut Option<K>, key: &K) -> Result<(), SegmentError> {
    if last.as_ref().is_some_and(|l| l >= key) {
        return Err(reader.error("records are not strictly ascending"));
    }
    *last = Some(key.clone());
    Ok(())
}

/// Read the node records of a graph or tree body.
fn read_nodes(r: &mut Reader<'_>) -> Result<BTreeMap<u32, Node>, SegmentError> {
    let n = r.count(20)?;
    let mut nodes = BTreeMap::new();
    let mut last = None;
    for _ in 0..n {
        let node = r.node()?;
        ascending(r, &mut last, &node.id)?;
        nodes.insert(node.id, node);
    }
    Ok(nodes)
}

/// Read a boundary.
fn read_boundary(r: &mut Reader<'_>) -> Result<Boundary, SegmentError> {
    let mut boundary = Boundary::default();
    let n = r.count(16)?;
    let mut last = None;
    for _ in 0..n {
        let ghost = Ghost {
            node: r.u32()?,
            owner: SegmentId(r.u32()?),
            anchor: r.name()?,
        };
        ascending(r, &mut last, &ghost.node)?;
        boundary.ghosts.insert(ghost.node, ghost);
    }
    let n = r.count(30)?;
    let mut last = None;
    for _ in 0..n {
        let elem = r.view_ref()?;
        let stub = InStub {
            src: r.u32()?,
            src_owner: SegmentId(r.u32()?),
            src_anchor: r.name()?,
            tgt: r.u32()?,
            label: r.edge()?,
        };
        ascending(r, &mut last, &elem)?;
        boundary.in_stubs.insert(elem, stub);
    }
    Ok(boundary)
}

/// The header of a segment's canonical bytes.
struct Header {
    schema: Digest,
    spec: SegmentSpec,
    shape_tag: u8,
}

/// Read a segment's header.
fn read_header(r: &mut Reader<'_>) -> Result<Header, SegmentError> {
    let version = r.u16()?;
    if version != SEGMENT_FORMAT_VERSION {
        return Err(r.error(format!("segment format version {version}")));
    }
    let shape_tag = r.u8()?;
    let schema = r.digest()?;
    let id = SegmentId(r.u32()?);
    let spec = SegmentSpec {
        id,
        nodes: r.ranges(id)?,
        alloc_nodes: r.opt_range(id)?,
        alloc_edges: r.opt_range(id)?,
    };
    Ok(Header {
        schema,
        spec,
        shape_tag,
    })
}

/// Read a segment body of the shape `shape_tag` names.
fn read_body(r: &mut Reader<'_>, segment: SegmentId, shape_tag: u8) -> Result<AnySegment, SegmentError> {
    match shape_tag {
        2 => {
            let nodes = read_nodes(r)?;
            let n = r.count(32)?;
            let mut edges = BTreeMap::new();
            let mut last = None;
            for _ in 0..n {
                let id = EdgeId(r.u32()?);
                let edge = GEdge {
                    src: r.u32()?,
                    tgt: r.u32()?,
                    edge: r.edge()?,
                    position: r.opt_u32()?,
                };
                ascending(r, &mut last, &id)?;
                edges.insert(id, edge);
            }
            let boundary = read_boundary(r)?;
            Ok(AnySegment::Graph(GraphSegment {
                segment,
                nodes,
                edges,
                boundary,
            }))
        }
        1 => {
            let n = r.count(20)?;
            let mut rows = BTreeMap::new();
            let mut last = None;
            for _ in 0..n {
                let id = RowId(r.u32()?);
                let table = r.name()?;
                let row = r.value_map(0)?;
                ascending(r, &mut last, &id)?;
                rows.insert(id, (table, row));
            }
            let n = r.count(41)?;
            let mut pairs = BTreeSet::new();
            let mut last = None;
            for _ in 0..n {
                let pair = (r.edge()?, RowId(r.u32()?), RowId(r.u32()?));
                ascending(r, &mut last, &pair)?;
                pairs.insert(pair);
            }
            let boundary = read_boundary(r)?;
            Ok(AnySegment::Table(TableSegment {
                segment,
                rows,
                pairs,
                boundary,
            }))
        }
        0 => {
            let root = r.opt_u32()?;
            let nodes = read_nodes(r)?;
            let n = r.count(45)?;
            let mut arcs = BTreeMap::new();
            let mut last = None;
            for _ in 0..n {
                let id = EdgeId(r.u32()?);
                let arc = (r.u32()?, r.u32()?, r.edge()?);
                ascending(r, &mut last, &id)?;
                arcs.insert(id, arc);
            }
            let n = r.count(16)?;
            let mut fans = Vec::with_capacity(n);
            let mut last = None;
            for _ in 0..n {
                let node = r.u32()?;
                let parent_edge = EdgeId(r.u32()?);
                let m = r.count(12)?;
                let mut child_edges = BTreeMap::new();
                let mut last_label: Option<String> = None;
                for _ in 0..m {
                    let label = r.string()?;
                    ascending(r, &mut last_label, &label)?;
                    child_edges.insert(label, EdgeId(r.u32()?));
                }
                ascending(r, &mut last, &node)?;
                fans.push(FanEmbedding {
                    node,
                    parent_edge,
                    child_edges,
                });
            }
            let boundary = read_boundary(r)?;
            Ok(AnySegment::Tree(TreeSegment {
                segment,
                nodes,
                arcs,
                fans,
                root,
                boundary,
            }))
        }
        other => Err(r.error(format!("shape tag {other}"))),
    }
}

/// The records crossing from a segment into each peer, or into it from
/// each peer, as `(element, src, src anchor, tgt, tgt anchor, label)`.
type CrossRecords<'a> = BTreeMap<SegmentId, Vec<(ViewRef, u32, &'a Name, u32, &'a Name, &'a panproto_schema::Edge)>>;

/// The cross digest of one peer's records: [`SEGMENT_CROSS_CONTEXT`] over
/// their count and each record, ascending by element.
fn cross_digest(mut records: Vec<(ViewRef, u32, &Name, u32, &Name, &panproto_schema::Edge)>) -> Digest {
    records.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Digest::hasher(SEGMENT_CROSS_CONTEXT);
    hasher.len(records.len());
    for (elem, src, src_anchor, tgt, tgt_anchor, label) in records {
        write_view_ref(&mut hasher, &elem);
        hasher.u32(src);
        hasher.name(src_anchor);
        hasher.u32(tgt);
        hasher.name(tgt_anchor);
        hasher.edge(label);
    }
    hasher.finalize()
}

impl AnySegment {
    /// The segment's canonical bytes under the schema digest `schema`, with
    /// the header of its spec in `partition`. See the module documentation
    /// for the layout.
    ///
    /// # Errors
    ///
    /// [`SegmentError::ShapeMismatch`] or
    /// [`SegmentError::MalformedPartition`] when the partition does not
    /// list the segment with its shape.
    pub fn canonical_bytes(&self, schema: &Digest, partition: &Partition) -> Result<Vec<u8>, SegmentError> {
        let spec = spec_of(self, partition)?;
        let mut writer = CanonicalWriter::new();
        write_segment(&mut writer, self, schema, spec);
        Ok(writer.into_bytes())
    }

    /// The segment's digest: its canonical bytes under [`SEGMENT_CONTEXT`],
    /// streamed into the hasher.
    ///
    /// # Errors
    ///
    /// As [`AnySegment::canonical_bytes`].
    pub fn digest(&self, schema: &Digest, partition: &Partition) -> Result<Digest, SegmentError> {
        let spec = spec_of(self, partition)?;
        Ok(self.digest_with(schema, spec))
    }

    /// [`AnySegment::digest`] with the spec in hand.
    pub(crate) fn digest_with(&self, schema: &Digest, spec: &SegmentSpec) -> Digest {
        let mut hasher = Digest::hasher(SEGMENT_CONTEXT);
        write_segment(&mut hasher, self, schema, spec);
        hasher.finalize()
    }

    /// Read a segment back from its canonical bytes, with the schema digest
    /// its header carries, and check it against `partition`.
    ///
    /// The bytes must be exactly the canonical bytes of the segment they
    /// decode to; the header's ranges must be the spec's in the partition;
    /// and the segment must keep its own contract (ownership, ghosts and
    /// in-stubs).
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] when the bytes are not the canonical bytes of
    /// a segment, or carry other ranges than the partition's;
    /// [`SegmentError::ShapeMismatch`] or
    /// [`SegmentError::MalformedPartition`] when the partition does not list
    /// the segment with its shape; [`SegmentError::Invalid`] with every
    /// violation when the segment breaks its contract.
    pub fn from_canonical_bytes(bytes: &[u8], partition: &Partition) -> Result<(Self, Digest), SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Segment);
        let header = read_header(&mut r)?;
        if shape_from_tag(header.shape_tag).is_none() {
            return Err(r.error(format!("shape tag {}", header.shape_tag)));
        }
        let segment = read_body(&mut r, header.spec.id, header.shape_tag)?;
        r.finish()?;
        let index = partition.owner_index()?;
        let spec = spec_of(&segment, partition)?;
        if *spec != header.spec {
            return Err(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: format!(
                    "the ranges of {} differ from the partition's",
                    header.spec.id
                ),
            });
        }
        check_segment_with(&segment, &index)?;
        let mut writer = CanonicalWriter::new();
        write_segment(&mut writer, &segment, &header.schema, spec);
        if writer.as_bytes() != bytes {
            return Err(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "the bytes are not in canonical form".to_owned(),
            });
        }
        Ok((segment, header.schema))
    }

    /// The records of the owned edges that cross into each peer.
    fn crossing_out(&self) -> CrossRecords<'_> {
        let boundary = self.boundary();
        let mut out: CrossRecords<'_> = BTreeMap::new();
        for edge in self.owned_edges() {
            if self.anchor_of(edge.tgt).is_some() {
                continue;
            }
            let (Some(ghost), Some(src_anchor)) = (boundary.ghosts.get(&edge.tgt), self.anchor_of(edge.src)) else {
                continue;
            };
            out.entry(ghost.owner).or_default().push((
                edge.elem,
                edge.src,
                src_anchor,
                edge.tgt,
                &ghost.anchor,
                edge.label,
            ));
        }
        out
    }

    /// The cross digest of the owned edges into each peer, from the
    /// segment's own data: each edge's reference, source and source anchor,
    /// target and the target's ghost anchor, and label.
    #[must_use]
    pub fn cross_out(&self) -> BTreeMap<SegmentId, Digest> {
        self.crossing_out()
            .into_iter()
            .map(|(peer, records)| (peer, cross_digest(records)))
            .collect()
    }

    /// The cross digest of the edges from each peer into this segment, from
    /// its in-stubs and the anchors of its own targets. Equal to the peer's
    /// [`AnySegment::cross_out`] entry for this segment exactly when both
    /// copies of every crossing edge agree.
    #[must_use]
    pub fn cross_in(&self) -> BTreeMap<SegmentId, Digest> {
        let empty = Name::default();
        let mut by_peer: CrossRecords<'_> = BTreeMap::new();
        for (elem, stub) in &self.boundary().in_stubs {
            by_peer.entry(stub.src_owner).or_default().push((
                elem.clone(),
                stub.src,
                &stub.src_anchor,
                stub.tgt,
                self.anchor_of(stub.tgt).unwrap_or(&empty),
                &stub.label,
            ));
        }
        by_peer
            .into_iter()
            .map(|(peer, records)| (peer, cross_digest(records)))
            .collect()
    }
}
