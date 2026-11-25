//! The instance digest of an instance held as segments, computed without
//! assembling it.
//!
//! Each segment yields its records already in canonical order; a k-way merge
//! interleaves them into the one canonical order of the whole instance and
//! streams them into one hasher. Only one record per segment is held at a
//! time, so the merge holds `O(S)` records for `S` segments.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind};

use super::body::{AnySegment, SegmentStats};
use super::error::SegmentError;
use super::reader::Reader;
use super::store::ArtifactKind;
use super::tree_bytes::{FanRecord, write_arc_record, write_winstance_header};
use crate::functor::{Row, write_pair_record, write_row_record};
use crate::ginstance::{EdgeId, GEdge, write_edge_record, write_node_record};
use crate::identity::{
    CanonicalSink, Digest, GRAPH_INSTANCE_CONTEXT, TABLE_INSTANCE_CONTEXT, TREE_INSTANCE_CONTEXT,
};
use crate::metadata::Node;
use crate::view_ref::{RowId, ViewRef};

/// One record of an instance's canonical bytes.
#[derive(Clone, Debug)]
pub enum InstanceRecord {
    /// A graph or tree node.
    Node(Node),
    /// A graph edge.
    Edge(EdgeId, GEdge),
    /// A table row with its table.
    Row(RowId, Name, Row),
    /// A table pair.
    Pair(Edge, RowId, RowId),
    /// A tree arc, with the embedding edge id that orders it.
    Arc {
        /// The arc's embedding edge id.
        id: EdgeId,
        /// The parent.
        parent: u32,
        /// The child.
        child: u32,
        /// The label.
        label: Edge,
    },
    /// A tree fan.
    Fan(FanRecord),
}

/// The position of a record in canonical order: nodes (and rows) by id,
/// then edges (and arcs) by edge id or pairs by `(edge, src, tgt)`, then fans
/// by record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MergeKey {
    Node(u32),
    Edge(u32),
    Pair(Edge, u32, u32),
    Fan(FanRecord),
}

impl InstanceRecord {
    fn key(&self) -> MergeKey {
        match self {
            Self::Node(node) => MergeKey::Node(node.id),
            Self::Row(id, _, _) => MergeKey::Node(id.0),
            Self::Edge(id, _) | Self::Arc { id, .. } => MergeKey::Edge(id.0),
            Self::Pair(edge, src, tgt) => MergeKey::Pair(edge.clone(), src.0, tgt.0),
            Self::Fan(fan) => MergeKey::Fan(fan.clone()),
        }
    }

    /// Whether a stream of `shape` may carry this record.
    const fn fits(&self, shape: InstanceShapeKind) -> bool {
        matches!(
            (self, shape),
            (Self::Node(_) | Self::Edge(..), InstanceShapeKind::Graph)
                | (Self::Row(..) | Self::Pair(..), InstanceShapeKind::Table)
                | (
                    Self::Node(_) | Self::Arc { .. } | Self::Fan(_),
                    InstanceShapeKind::Tree
                )
        )
    }

    /// Write the record as the instance's canonical bytes write it.
    fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        match self {
            Self::Node(node) => write_node_record(sink, node),
            Self::Edge(id, edge) => write_edge_record(sink, *id, edge),
            Self::Row(id, table, row) => write_row_record(sink, *id, table, row),
            Self::Pair(edge, src, tgt) => write_pair_record(sink, edge, *src, *tgt),
            Self::Arc {
                parent,
                child,
                label,
                ..
            } => write_arc_record(sink, *parent, *child, label),
            Self::Fan(fan) => fan.write(sink),
        }
    }

    /// The record's frame tag: `0` node, `1` edge, `2` row, `3` pair, `4`
    /// arc, `5` fan.
    const fn frame_tag(&self) -> u8 {
        match self {
            Self::Node(_) => 0,
            Self::Edge(..) => 1,
            Self::Row(..) => 2,
            Self::Pair(..) => 3,
            Self::Arc { .. } => 4,
            Self::Fan(_) => 5,
        }
    }

    /// The record's frame: its frame tag (`0` node, `1` edge, `2` row, `3`
    /// pair, `4` arc, `5` fan), then, for an arc, its embedding edge id,
    /// then the record as the instance's canonical bytes write it.
    ///
    /// The frame holds everything the record is, so [`InstanceRecord::from_frame`]
    /// reads it back.
    #[must_use]
    pub fn frame(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.tag(self.frame_tag());
        if let Self::Arc { id, .. } = self {
            sink.u32(id.0);
        }
        self.write(&mut sink);
        sink.into_bytes()
    }

    /// The record a frame holds ([`InstanceRecord::frame`]).
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] of a segment when the frame has an unknown
    /// tag, ends early, holds a value the value types refuse or a fan whose
    /// children do not strictly ascend, or has bytes after its record.
    pub fn from_frame(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Segment);
        let record = match r.u8()? {
            0 => Self::Node(r.node()?),
            1 => {
                let (id, edge) = r.edge_record()?;
                Self::Edge(id, edge)
            }
            2 => {
                let (id, table, row) = r.row_record()?;
                Self::Row(id, table, row)
            }
            3 => {
                let (edge, src, tgt) = r.pair_record()?;
                Self::Pair(edge, src, tgt)
            }
            4 => {
                let id = EdgeId(r.u32()?);
                let (parent, child, label) = r.arc_record()?;
                Self::Arc {
                    id,
                    parent,
                    child,
                    label,
                }
            }
            5 => Self::Fan(r.fan_record()?),
            other => return Err(r.error(format!("record frame tag {other}"))),
        };
        r.finish()?;
        Ok(record)
    }
}

/// The record stream of `records`: each record's frame
/// ([`InstanceRecord::frame`]) after its length as eight little-endian
/// bytes, in the order given.
#[must_use]
pub fn record_stream<'a>(records: impl IntoIterator<Item = &'a InstanceRecord>) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    for record in records {
        sink.bytes(&record.frame());
    }
    sink.into_bytes()
}

/// The records of a record stream ([`record_stream`]), in stream order.
///
/// # Errors
///
/// [`SegmentError::Decode`] of a segment when a length runs past the end of
/// the stream or a frame does not decode ([`InstanceRecord::from_frame`]).
pub fn decode_record_stream(bytes: &[u8]) -> Result<Vec<InstanceRecord>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Segment);
    let mut records = Vec::new();
    while !r.finished() {
        records.push(InstanceRecord::from_frame(r.byte_string()?)?);
    }
    Ok(records)
}

/// A stream of one segment's records in canonical order, with the counts
/// its manifest record states.
pub trait RecordSource {
    /// The segment's counts: the header of the instance digest is their sum
    /// over every source.
    fn stats(&self) -> SegmentStats;

    /// The next record, or `None` when the stream is done.
    ///
    /// # Errors
    ///
    /// The source's error when the record cannot be read.
    fn next_record(&mut self) -> Result<Option<InstanceRecord>, SegmentError>;
}

/// The records of a segment held in memory, as a [`RecordSource`].
#[derive(Clone, Debug)]
pub struct SegmentRecords {
    stats: SegmentStats,
    records: std::vec::IntoIter<InstanceRecord>,
}

impl SegmentRecords {
    /// The records not yet streamed, in canonical order.
    #[must_use]
    pub fn as_slice(&self) -> &[InstanceRecord] {
        self.records.as_slice()
    }
}

impl RecordSource for SegmentRecords {
    fn stats(&self) -> SegmentStats {
        self.stats
    }

    fn next_record(&mut self) -> Result<Option<InstanceRecord>, SegmentError> {
        Ok(self.records.next())
    }
}

impl AnySegment {
    /// The segment's instance records in canonical order: its nodes (tree
    /// nodes only, not fan nodes) or rows, then its edges, tree arcs or
    /// pairs, then (for trees) its fans as records, each fan's parent and
    /// children read from its fan edges.
    #[must_use]
    pub fn records(&self) -> SegmentRecords {
        let mut records = Vec::new();
        match self {
            Self::Graph(s) => {
                records.extend(s.nodes.values().cloned().map(InstanceRecord::Node));
                records.extend(
                    s.edges
                        .iter()
                        .map(|(id, e)| InstanceRecord::Edge(*id, e.clone())),
                );
            }
            Self::Table(s) => {
                records.extend(
                    s.rows
                        .iter()
                        .map(|(id, (t, r))| InstanceRecord::Row(*id, t.clone(), r.clone())),
                );
                records.extend(
                    s.pairs
                        .iter()
                        .map(|(e, src, tgt)| InstanceRecord::Pair(e.clone(), *src, *tgt)),
                );
            }
            Self::Tree(s) => {
                let fan_nodes = s.fan_node_ids();
                let fan_edges = s.fan_edge_ids();
                records.extend(
                    s.nodes
                        .values()
                        .filter(|n| !fan_nodes.contains(&n.id))
                        .cloned()
                        .map(InstanceRecord::Node),
                );
                records.extend(s.arcs.iter().filter(|(id, _)| !fan_edges.contains(id)).map(
                    |(id, (parent, child, label))| InstanceRecord::Arc {
                        id: *id,
                        parent: *parent,
                        child: *child,
                        label: label.clone(),
                    },
                ));
                let target = |id: &EdgeId| s.arcs.get(id).map_or(0, |(_, tgt, _)| *tgt);
                let mut fans: Vec<FanRecord> = s
                    .fans
                    .iter()
                    .map(|f| FanRecord {
                        parent: target(&f.parent_edge),
                        hyper_edge_id: s
                            .nodes
                            .get(&f.node)
                            .map(|n| n.anchor.to_string())
                            .unwrap_or_default(),
                        children: f
                            .child_edges
                            .iter()
                            .map(|(label, id)| (label.clone(), target(id)))
                            .collect(),
                    })
                    .collect();
                fans.sort_unstable();
                records.extend(fans.into_iter().map(InstanceRecord::Fan));
            }
        }
        SegmentRecords {
            stats: self.stats(),
            records: records.into_iter(),
        }
    }

    /// The segment's record stream ([`record_stream`] of
    /// [`AnySegment::records`]): the records the instance digest reads, in
    /// canonical order.
    #[must_use]
    pub fn record_stream(&self) -> Vec<u8> {
        record_stream(self.records().as_slice())
    }
}

/// The decode error of a record stream.
const fn stream_error(message: String) -> SegmentError {
    SegmentError::Decode {
        artifact: ArtifactKind::Segment,
        message,
    }
}

/// The error for a record two segments both hold.
fn held_twice(key: &MergeKey, a: super::ids::SegmentId, b: super::ids::SegmentId) -> SegmentError {
    let (lo, hi) = (a.min(b), a.max(b));
    match key {
        MergeKey::Edge(id) => SegmentError::DuplicateEdgeId {
            edge: EdgeId(*id),
            segments: vec![lo, hi],
        },
        MergeKey::Node(id) => SegmentError::WrongOwner {
            elem: ViewRef::Node(*id),
            expected: lo,
            found: hi,
        },
        MergeKey::Pair(edge, src, tgt) => SegmentError::WrongOwner {
            elem: ViewRef::Pair {
                edge: edge.clone(),
                src: RowId(*src),
                tgt: RowId(*tgt),
            },
            expected: lo,
            found: hi,
        },
        MergeKey::Fan(_) => SegmentError::Decode {
            artifact: ArtifactKind::Segment,
            message: format!("{hi} repeats a fan record of {lo}"),
        },
    }
}

/// The instance digest of an instance of `shape` held as the segments that
/// `sources` stream, without assembling it.
///
/// The header's counts are the sums of the sources' [`SegmentStats`]; the
/// records are merged into canonical order and streamed into one hasher
/// under the shape's instance context. The result equals the whole
/// instance's digest ([`GInstance::digest`](crate::GInstance::digest),
/// [`FInstance::digest`](crate::FInstance::digest) or
/// [`WInstance::digest`](crate::WInstance::digest)) of the assembled
/// instance. For a tree, arcs merge by embedding edge id, which is arc order,
/// and fans by record, which needs no global position.
///
/// # Errors
///
/// A source's own error; [`SegmentError::Decode`] when a source yields a
/// record of another shape, out of order, or a count other than its stats
/// state; [`SegmentError::DuplicateEdgeId`] when two sources hold one edge
/// id, and [`SegmentError::WrongOwner`] when they hold one node, row or
/// pair.
pub fn instance_digest<'a>(
    shape: InstanceShapeKind,
    mut sources: Vec<Box<dyn RecordSource + 'a>>,
) -> Result<Digest, SegmentError> {
    let context = match shape {
        InstanceShapeKind::Graph => GRAPH_INSTANCE_CONTEXT,
        InstanceShapeKind::Table => TABLE_INSTANCE_CONTEXT,
        InstanceShapeKind::Tree => TREE_INSTANCE_CONTEXT,
    };
    let stats: Vec<SegmentStats> = sources.iter().map(|s| s.stats()).collect();
    let sum =
        |field: fn(&SegmentStats) -> u64| stats.iter().map(field).fold(0_u64, u64::saturating_add);
    let (nodes, edges, fans) = (sum(|s| s.nodes), sum(|s| s.edges), sum(|s| s.fans));
    let mut hasher = Digest::hasher(context);
    match shape {
        InstanceShapeKind::Tree => write_winstance_header(&mut hasher, nodes, edges, fans),
        InstanceShapeKind::Graph | InstanceShapeKind::Table => {
            hasher.u64(nodes);
            hasher.u64(edges);
        }
    }
    let mut heads: Vec<Option<InstanceRecord>> = Vec::with_capacity(sources.len());
    let mut emitted = vec![[0_u64; 3]; sources.len()];
    let mut queue = BinaryHeap::new();
    for (i, source) in sources.iter_mut().enumerate() {
        let head = source.next_record()?;
        if let Some(record) = &head {
            queue.push(Reverse((record.key(), i)));
        }
        heads.push(head);
    }
    let mut last: Option<(MergeKey, usize)> = None;
    while let Some(Reverse((key, i))) = queue.pop() {
        let Some(record) = heads[i].take() else {
            continue;
        };
        let segment = stats[i].segment;
        if !record.fits(shape) {
            return Err(stream_error(format!(
                "{segment} yields a record of another shape than {shape:?}"
            )));
        }
        if let Some((previous, j)) = &last
            && *previous == key
            && !matches!(key, MergeKey::Fan(_))
        {
            return Err(held_twice(&key, stats[*j].segment, segment));
        }
        record.write(&mut hasher);
        let slot = match record {
            InstanceRecord::Node(_) | InstanceRecord::Row(..) => 0,
            InstanceRecord::Fan(_) => 2,
            _ => 1,
        };
        emitted[i][slot] += 1;
        let next = sources[i].next_record()?;
        if let Some(next_record) = &next {
            let next_key = next_record.key();
            if next_key < key || (next_key == key && !matches!(key, MergeKey::Fan(_))) {
                return Err(stream_error(format!(
                    "{segment} yields records out of order"
                )));
            }
            queue.push(Reverse((next_key, i)));
        }
        heads[i] = next;
        last = Some((key, i));
    }
    for (i, counts) in emitted.iter().enumerate() {
        let s = &stats[i];
        if *counts != [s.nodes, s.edges, s.fans] {
            return Err(stream_error(format!(
                "{} yields {counts:?} records, its stats state {:?}",
                s.segment,
                [s.nodes, s.edges, s.fans]
            )));
        }
    }
    Ok(hasher.finalize())
}
