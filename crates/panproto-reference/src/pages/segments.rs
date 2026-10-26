//! Reference operations assigned to `segments`.
//!
//! This module follows SG.1--SG.3, SG.6, the SG.7 storage vocabulary,
//! SG.9, S10 and S10a for graph, table and tree instances. The algorithms are
//! deliberately naive: the runner checks and glues the complete value, then
//! independently repartitions and seals it. This is an executable definition,
//! not a streaming implementation. The finite-scope module records the
//! parameters that still prevent a full C10 carrier claim.

pub mod segment {
    //! Segmented execution as whole-state execution on the checked glued state.

    use std::cell::OnceCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::convert::Infallible;
    use std::error::Error;

    use crate::canonical::{
        ComplementShape, IdentityEnvelope, Reader, Writer, decode_graph, decode_table, graph_bytes,
        read_graph_edge, read_map, read_node, read_value, table_bytes, tree_bytes,
        write_graph_edge, write_map, write_node, write_value,
    };
    use crate::encode::TreeEmbedding;
    use crate::model::{Fan, GEdge, Graph, Name, Node, NodeShape, SEdge, Table, Tree};
    use crate::pages::lenses::SynthIndex;
    use crate::pages::lenses::table::{DroppedRow, TableComplement};
    use crate::restrict::{Complement, SynthKey};
    use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
    use crate::value::{Presence, Value};

    /// Segment wire-format version.
    pub const SEGMENT_FORMAT_VERSION: u16 = 1;
    /// Manifest wire-format version.
    pub const SEGMENT_MANIFEST_VERSION: u16 = 1;
    /// The number of edge ids in the complete `u32` domain.
    pub const VALIDATION_VIEW_EDGE_CAPACITY: u64 = u32::MAX as u64 + 1;

    /// Segment-content digest context.
    pub const SEGMENT_CONTEXT: &str = "panproto 2026-09 instance segment v1";
    /// Complement-segment digest context.
    pub const SEGMENT_COMPLEMENT_CONTEXT: &str = "panproto 2026-09 complement segment v1";
    /// Cross-reference digest context.
    pub const SEGMENT_CROSS_CONTEXT: &str = "panproto 2026-09 segment cross references v1";
    /// Task-artifact digest context.
    pub const SEGMENT_TASK_CONTEXT: &str = "panproto 2026-09 segment task artifact v1";
    /// Manifest-leaf digest context.
    pub const MANIFEST_LEAF_CONTEXT: &str = "panproto 2026-09 segment manifest leaf v1";
    /// Manifest-node digest context.
    pub const MANIFEST_NODE_CONTEXT: &str = "panproto 2026-09 segment manifest node v1";
    /// Manifest digest context.
    pub const MANIFEST_CONTEXT: &str = "panproto 2026-09 segment manifest v1";
    /// Run-journal digest context.
    pub const RUN_JOURNAL_CONTEXT: &str = "panproto 2026-09 segment run journal v1";
    /// Sorted edge-id-run digest context.
    pub const SEGMENT_EDGE_IDS_CONTEXT: &str = "panproto 2026-09 segment edge ids v1";

    /// A BLAKE3 digest under an explicit domain separator.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Digest(
        /// Digest bytes.
        pub [u8; 32],
    );

    impl Digest {
        /// Derive a digest under an explicit BLAKE3 context string.
        #[must_use]
        pub fn derive(context: &'static str, bytes: &[u8]) -> Self {
            Self(crate::blake3_ref::derive_key(context, bytes))
        }
    }

    /// Instance shapes, in canonical order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum InstanceShapeKind {
        /// Rooted tree with arcs and fans.
        Tree,
        /// Rows and foreign-key pairs.
        Table,
        /// Nodes and labelled edges.
        Graph,
    }

    /// A dense segment id.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SegmentId(
        /// Numeric id.
        pub u32,
    );

    /// An inclusive id range.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct IdRange {
        /// Least id.
        pub lo: u32,
        /// Greatest id.
        pub hi: u32,
    }

    impl IdRange {
        /// Whether the range holds `id`.
        #[must_use]
        pub const fn contains(self, id: u32) -> bool {
            self.lo <= id && id <= self.hi
        }
    }

    /// Allocated id kind.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum IdKind {
        /// Node or row.
        Node,
        /// Edge.
        Edge,
    }

    /// One segment's owned and allocation ranges.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct SegmentSpec {
        /// Segment id.
        pub id: SegmentId,
        /// Owned node or row ranges.
        pub nodes: Vec<IdRange>,
        /// Node allocation range.
        pub alloc_nodes: Option<IdRange>,
        /// Edge allocation range, forbidden for tables.
        pub alloc_edges: Option<IdRange>,
    }

    /// A shape-stamped partition.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Partition {
        /// Instance shape.
        pub shape: InstanceShapeKind,
        /// Dense segment specifications.
        pub specs: Vec<SegmentSpec>,
    }

    /// A shape-independent instance reference.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ViewRef {
        /// Graph/tree node.
        Node(u32),
        /// Graph edge or tree fan edge.
        Edge(u32),
        /// Ordinary tree arc.
        Arc {
            /// Parent node.
            parent: u32,
            /// Child node.
            child: u32,
        },
        /// Table row.
        Row(u32),
        /// Table foreign-key pair.
        Pair {
            /// Schema edge.
            edge: SEdge,
            /// Referencing row.
            src: u32,
            /// Referenced row.
            tgt: u32,
        },
    }

    /// A malformed partition or segment set.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PartitionDefect {
        /// The spec at this position carries another id.
        NotDense {
            /// Found id.
            found: SegmentId,
        },
        /// Ranges are not ascending.
        UnsortedRanges,
        /// Node allocation is not covered by owned ranges.
        AllocationOutsideNodes,
        /// A table has an edge allocation range.
        EdgeAllocationOnTable,
        /// A listed segment is absent.
        SegmentMissing,
        /// A segment is supplied twice.
        SegmentRepeated,
        /// A segment is not listed.
        UnknownSegment,
        /// Tree counters occur without the root.
        CountersWithoutRoot,
    }

    /// Deterministic restriction-error key in engine precedence order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ErrorKey {
        /// Engine stage.
        pub stage: u8,
        /// Primary element id.
        pub id: u32,
        /// Secondary element id.
        pub id2: u32,
        /// Error kind within the element.
        pub kind: u8,
    }

    impl ErrorKey {
        /// Return the canonical field-order bytes.
        #[must_use]
        pub fn canonical_bytes(self) -> [u8; 10] {
            let mut bytes = [0; 10];
            bytes[0] = self.stage;
            bytes[1..5].copy_from_slice(&self.id.to_le_bytes());
            bytes[5..9].copy_from_slice(&self.id2.to_le_bytes());
            bytes[9] = self.kind;
            bytes
        }
    }

    /// Deterministic error position.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ErrorAt {
        /// Segment containing the error.
        pub segment: SegmentId,
        /// Engine-local precedence key.
        pub key: ErrorKey,
    }

    /// Serializable projected error detail.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ErrorDetail(
        /// Detail text.
        pub String,
    );
    /// Serializable put-domain refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PutDomainError(
        /// Refusal text.
        pub String,
    );
    /// Serializable identity refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct IdentityError(
        /// Refusal text.
        pub String,
    );

    const fn restrict_error_kind(error: &crate::restrict::EngineError) -> u8 {
        match error {
            crate::restrict::EngineError::NoEdgeFound { .. } => 0,
            crate::restrict::EngineError::AmbiguousEdge { .. } => 1,
            crate::restrict::EngineError::IncoherentEdgeImage { .. } => 2,
            crate::restrict::EngineError::Layer(_) => 3,
            crate::restrict::EngineError::IdsExhausted { .. } => 4,
            crate::restrict::EngineError::ExpansionTooLong { .. } => 5,
            crate::restrict::EngineError::FanReconstructionFailed { .. } => 6,
            crate::restrict::EngineError::Predicate(_) => 7,
        }
    }

    /// Project an element-local restriction failure into the SG.9 carrier.
    ///
    /// This adapter deliberately accepts the concrete restriction engine
    /// types. Plan and root failures have no element-local [`ErrorAt`] and are
    /// therefore not represented by this function.
    #[must_use]
    pub fn adapt_restrict_at(
        segment: SegmentId,
        key: crate::restrict::ErrorKey,
        error: &crate::restrict::EngineError,
    ) -> SegmentError {
        SegmentError::Restrict {
            at: ErrorAt {
                segment,
                key: ErrorKey {
                    stage: key.stage,
                    id: key.id,
                    id2: key.id2,
                    kind: restrict_error_kind(error),
                },
            },
            detail: Box::new(ErrorDetail(format!("{error:?}"))),
        }
    }

    /// Project a concrete graph-lens refusal into the SG.9 carrier without
    /// extending its exact 26-variant vocabulary.
    ///
    /// # Errors
    ///
    /// A tree-carrier operational error, which lies outside SG.9's
    /// mathematical put-domain carrier.
    pub fn adapt_lens_error(
        error: &crate::lens::LensError,
    ) -> Result<SegmentError, crate::lens::TreeOperationalError> {
        match error {
            crate::lens::LensError::Identity(error) => {
                Ok(SegmentError::Identity(IdentityError(format!("{error:?}"))))
            }
            crate::lens::LensError::PutDomain(error) => {
                Ok(SegmentError::PutDomain(vec![PutDomainError(format!(
                    "{error:?}"
                ))]))
            }
            crate::lens::LensError::PutDomainReport(errors) => Ok(SegmentError::PutDomain(
                errors
                    .iter()
                    .map(|error| PutDomainError(format!("{error:?}")))
                    .collect(),
            )),
            crate::lens::LensError::TreeOperational(error) => Err(error.clone()),
        }
    }

    /// Project a concrete graph-edit application refusal into the SG.9
    /// carrier at its owning segment.
    #[must_use]
    pub fn adapt_edit_error(segment: SegmentId, error: &crate::edit::ApplyError) -> SegmentError {
        SegmentError::Edit {
            segment,
            detail: Box::new(ErrorDetail(format!("{error:?}"))),
        }
    }

    /// Stored artifacts, in contract order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ArtifactKind {
        /// Instance segment.
        Segment,
        /// Complement segment.
        ComplementSegment,
        /// Boundary export.
        Export,
        /// Segment summary.
        Summary,
        /// Coordinator assembly.
        Assembly,
        /// Manifest.
        Manifest,
        /// Sorted edge-id run.
        EdgeIdRun,
    }

    /// Execution phases, in contract order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Phase {
        /// Validate.
        Validate,
        /// Export boundaries.
        Export,
        /// Summarize.
        Summarize,
        /// Assemble.
        Assemble,
        /// Emit.
        Emit,
        /// Seal.
        Seal,
    }

    /// The 26 SG.9 error variants, in contract order.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SegmentError {
        /// An inverted range.
        InvertedRange {
            /// Owning segment.
            segment: SegmentId,
            /// Range.
            range: IdRange,
        },
        /// Overlapping ranges.
        OverlappingRanges {
            /// First segment.
            a: SegmentId,
            /// Second segment.
            b: SegmentId,
        },
        /// No range owns an element.
        Unowned {
            /// Element.
            elem: ViewRef,
        },
        /// Wrong segment holder.
        WrongOwner {
            /// Element.
            elem: ViewRef,
            /// Required owner.
            expected: SegmentId,
            /// Actual holder.
            found: SegmentId,
        },
        /// Node key and id differ.
        NodeIdMismatch {
            /// Map key.
            key: u32,
            /// Stored id.
            found: u32,
        },
        /// Ghost missing, extra, or unequal.
        GhostMismatch {
            /// Remote node.
            node: u32,
        },
        /// In-stub missing, extra, or unequal.
        InStubMismatch {
            /// Edge reference.
            elem: ViewRef,
        },
        /// Duplicate edge id.
        DuplicateEdgeId {
            /// Edge id.
            edge: u32,
            /// Holders.
            segments: Vec<SegmentId>,
        },
        /// Cross-boundary records differ.
        CrossMismatch {
            /// Source owner.
            from: SegmentId,
            /// Target owner.
            to: SegmentId,
        },
        /// Stored and computed digests differ.
        DigestMismatch {
            /// Segment or `None` for the manifest.
            segment: Option<SegmentId>,
            /// Stored digest.
            expected: Digest,
            /// Computed digest.
            found: Digest,
        },
        /// Schema binding differs.
        SchemaMismatch {
            /// Required digest.
            expected: Digest,
            /// Supplied digest.
            found: Digest,
        },
        /// Shape tags differ.
        ShapeMismatch {
            /// Required shape.
            expected: InstanceShapeKind,
            /// Supplied shape.
            found: InstanceShapeKind,
        },
        /// Required allocation range absent.
        NoAllocationRange {
            /// Segment.
            segment: SegmentId,
            /// Id kind.
            kind: IdKind,
        },
        /// Allocation exhausted.
        AllocationExhausted {
            /// Segment.
            segment: SegmentId,
            /// Id kind.
            kind: IdKind,
        },
        /// Allocation range begins below the floor.
        AllocationBelowFloor {
            /// Segment.
            segment: SegmentId,
            /// Id kind.
            kind: IdKind,
        },
        /// Edge id outside its owner's allocation range.
        EdgeIdOutsideAllocation {
            /// Edge id.
            edge: u32,
            /// Owner.
            segment: SegmentId,
        },
        /// All local violations.
        Invalid {
            /// Violations in element order.
            violations: Vec<Self>,
        },
        /// Projected restriction failure.
        Restrict {
            /// Error position.
            at: ErrorAt,
            /// Serializable projection.
            detail: Box<ErrorDetail>,
        },
        /// Put-domain failures.
        PutDomain(Vec<PutDomainError>),
        /// Projected edit failure.
        Edit {
            /// Segment.
            segment: SegmentId,
            /// Serializable projection.
            detail: Box<ErrorDetail>,
        },
        /// Identity failure.
        Identity(IdentityError),
        /// Canonical decode failure.
        Decode {
            /// Artifact kind.
            artifact: ArtifactKind,
            /// Detail.
            message: String,
        },
        /// Malformed partition or segment set.
        MalformedPartition {
            /// Affected segment.
            segment: SegmentId,
            /// Defect.
            defect: PartitionDefect,
        },
        /// Absent edge endpoint.
        DanglingEndpoint {
            /// Edge reference.
            elem: ViewRef,
            /// Missing node.
            node: u32,
        },
        /// Tree element not represented exactly.
        NotEmbedded {
            /// Element.
            elem: ViewRef,
        },
        /// Validation view exceeds the `u32` id domain.
        ValidationViewUnrepresentable {
            /// Segment.
            segment: SegmentId,
            /// Required edge count.
            edges: u64,
        },
    }

    impl SegmentError {
        /// Stable zero-based position in the SG.9 carrier.
        #[must_use]
        pub const fn variant_index(&self) -> u8 {
            match self {
                Self::InvertedRange { .. } => 0,
                Self::OverlappingRanges { .. } => 1,
                Self::Unowned { .. } => 2,
                Self::WrongOwner { .. } => 3,
                Self::NodeIdMismatch { .. } => 4,
                Self::GhostMismatch { .. } => 5,
                Self::InStubMismatch { .. } => 6,
                Self::DuplicateEdgeId { .. } => 7,
                Self::CrossMismatch { .. } => 8,
                Self::DigestMismatch { .. } => 9,
                Self::SchemaMismatch { .. } => 10,
                Self::ShapeMismatch { .. } => 11,
                Self::NoAllocationRange { .. } => 12,
                Self::AllocationExhausted { .. } => 13,
                Self::AllocationBelowFloor { .. } => 14,
                Self::EdgeIdOutsideAllocation { .. } => 15,
                Self::Invalid { .. } => 16,
                Self::Restrict { .. } => 17,
                Self::PutDomain(_) => 18,
                Self::Edit { .. } => 19,
                Self::Identity(_) => 20,
                Self::Decode { .. } => 21,
                Self::MalformedPartition { .. } => 22,
                Self::DanglingEndpoint { .. } => 23,
                Self::NotEmbedded { .. } => 24,
                Self::ValidationViewUnrepresentable { .. } => 25,
            }
        }

        /// Stable externally-tagged snake-case variant name.
        #[must_use]
        pub const fn external_name(&self) -> &'static str {
            match self {
                Self::InvertedRange { .. } => "inverted_range",
                Self::OverlappingRanges { .. } => "overlapping_ranges",
                Self::Unowned { .. } => "unowned",
                Self::WrongOwner { .. } => "wrong_owner",
                Self::NodeIdMismatch { .. } => "node_id_mismatch",
                Self::GhostMismatch { .. } => "ghost_mismatch",
                Self::InStubMismatch { .. } => "in_stub_mismatch",
                Self::DuplicateEdgeId { .. } => "duplicate_edge_id",
                Self::CrossMismatch { .. } => "cross_mismatch",
                Self::DigestMismatch { .. } => "digest_mismatch",
                Self::SchemaMismatch { .. } => "schema_mismatch",
                Self::ShapeMismatch { .. } => "shape_mismatch",
                Self::NoAllocationRange { .. } => "no_allocation_range",
                Self::AllocationExhausted { .. } => "allocation_exhausted",
                Self::AllocationBelowFloor { .. } => "allocation_below_floor",
                Self::EdgeIdOutsideAllocation { .. } => "edge_id_outside_allocation",
                Self::Invalid { .. } => "invalid",
                Self::Restrict { .. } => "restrict",
                Self::PutDomain(_) => "put_domain",
                Self::Edit { .. } => "edit",
                Self::Identity(_) => "identity",
                Self::Decode { .. } => "decode",
                Self::MalformedPartition { .. } => "malformed_partition",
                Self::DanglingEndpoint { .. } => "dangling_endpoint",
                Self::NotEmbedded { .. } => "not_embedded",
                Self::ValidationViewUnrepresentable { .. } => "validation_view_unrepresentable",
            }
        }
    }

    impl Partition {
        /// Check all SG.1 partition invariants.
        ///
        /// # Errors
        ///
        /// Returns the first range, density, allocation, or overlap defect.
        pub fn validate(&self) -> Result<(), SegmentError> {
            for (position, spec) in self.specs.iter().enumerate() {
                let position = SegmentId(u32::try_from(position).unwrap_or(u32::MAX));
                if spec.id != position {
                    return Err(SegmentError::MalformedPartition {
                        segment: position,
                        defect: PartitionDefect::NotDense { found: spec.id },
                    });
                }
                for range in &spec.nodes {
                    check_range(spec.id, *range)?;
                }
                for pair in spec.nodes.windows(2) {
                    if pair[1].lo > pair[0].hi {
                        continue;
                    }
                    if pair[1].hi < pair[0].lo {
                        return Err(SegmentError::MalformedPartition {
                            segment: spec.id,
                            defect: PartitionDefect::UnsortedRanges,
                        });
                    }
                    return Err(SegmentError::OverlappingRanges {
                        a: spec.id,
                        b: spec.id,
                    });
                }
                if let Some(range) = spec.alloc_nodes {
                    check_range(spec.id, range)?;
                    if !covered(&spec.nodes, range) {
                        return Err(SegmentError::MalformedPartition {
                            segment: spec.id,
                            defect: PartitionDefect::AllocationOutsideNodes,
                        });
                    }
                }
                if let Some(range) = spec.alloc_edges {
                    check_range(spec.id, range)?;
                    if self.shape == InstanceShapeKind::Table {
                        return Err(SegmentError::MalformedPartition {
                            segment: spec.id,
                            defect: PartitionDefect::EdgeAllocationOnTable,
                        });
                    }
                }
            }
            first_overlap(
                self.specs
                    .iter()
                    .flat_map(|spec| spec.nodes.iter().map(move |range| (*range, spec.id)))
                    .collect(),
            )?;
            first_overlap(
                self.specs
                    .iter()
                    .filter_map(|spec| spec.alloc_edges.map(|range| (range, spec.id)))
                    .collect(),
            )
        }

        /// Owner of a node or row id.
        #[must_use]
        pub fn owner_of_node(&self, id: u32) -> Option<SegmentId> {
            let mut ranges: Vec<_> = self
                .specs
                .iter()
                .flat_map(|spec| spec.nodes.iter().map(move |range| (*range, spec.id)))
                .collect();
            ranges.sort_unstable();
            let at = ranges.partition_point(|(range, _)| range.hi < id);
            ranges
                .get(at)
                .filter(|(range, _)| range.contains(id))
                .map(|(_, owner)| *owner)
        }

        /// Spec of `id`.
        #[must_use]
        pub fn spec(&self, id: SegmentId) -> Option<&SegmentSpec> {
            self.specs
                .get(usize::try_from(id.0).unwrap_or(usize::MAX))
                .filter(|spec| spec.id == id)
        }
    }

    fn check_range(segment: SegmentId, range: IdRange) -> Result<(), SegmentError> {
        (range.lo <= range.hi)
            .then_some(())
            .ok_or(SegmentError::InvertedRange { segment, range })
    }

    fn covered(ranges: &[IdRange], target: IdRange) -> bool {
        let mut next = u64::from(target.lo);
        for range in ranges {
            if u64::from(range.hi) < next {
                continue;
            }
            if u64::from(range.lo) > next {
                return false;
            }
            next = u64::from(range.hi) + 1;
            if next > u64::from(target.hi) {
                return true;
            }
        }
        false
    }

    fn first_overlap(mut ranges: Vec<(IdRange, SegmentId)>) -> Result<(), SegmentError> {
        ranges.sort_unstable();
        for pair in ranges.windows(2) {
            if pair[1].0.lo <= pair[0].0.hi {
                return Err(SegmentError::OverlappingRanges {
                    a: pair[0].1.min(pair[1].1),
                    b: pair[0].1.max(pair[1].1),
                });
            }
        }
        Ok(())
    }

    /// A remote target of an owned edge.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Ghost {
        /// Remote node.
        pub node: u32,
        /// Owning segment.
        pub owner: SegmentId,
        /// Remote anchor or table.
        pub anchor: Name,
    }
    /// An edge owned elsewhere whose target is owned here.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct InStub {
        /// Source node or row.
        pub src: u32,
        /// Source owner.
        pub src_owner: SegmentId,
        /// Source anchor or table.
        pub src_anchor: Name,
        /// Target node or row.
        pub tgt: u32,
        /// Edge label.
        pub label: SEdge,
    }
    /// A segment boundary.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Boundary {
        /// Ghosts by remote id.
        pub ghosts: BTreeMap<u32, Ghost>,
        /// Incoming crossing edges by semantic reference.
        pub in_stubs: BTreeMap<ViewRef, InStub>,
    }
    /// One fan's stable graph ids.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct FanEmbedding {
        /// Reified fan node.
        pub node: u32,
        /// Parent edge id.
        pub parent_edge: u32,
        /// Child edge ids by label.
        pub child_edges: BTreeMap<Name, u32>,
        /// Original position in `Tree.fans` and `TreeEmbedding.fans`.
        pub ordinal: u64,
    }
    /// Graph segment.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct GraphSegment {
        /// Segment id.
        pub segment: SegmentId,
        /// Owned nodes.
        pub nodes: BTreeMap<u32, Node>,
        /// Owned edges.
        pub edges: BTreeMap<u32, GEdge>,
        /// Boundary.
        pub boundary: Boundary,
    }
    /// Table segment.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TableSegment {
        /// Segment id.
        pub segment: SegmentId,
        /// Owned rows with their tables.
        pub rows: BTreeMap<u32, (Name, BTreeMap<String, crate::value::Value>)>,
        /// Owned foreign-key pairs.
        pub pairs: BTreeSet<(SEdge, u32, u32)>,
        /// Boundary.
        pub boundary: Boundary,
    }
    /// Tree segment in stable graph form.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TreeSegment {
        /// Segment id.
        pub segment: SegmentId,
        /// Owned tree and fan nodes.
        pub nodes: BTreeMap<u32, Node>,
        /// Owned arcs and fan edges.
        pub arcs: BTreeMap<u32, (u32, u32, SEdge)>,
        /// Owned fan embeddings.
        pub fans: Vec<FanEmbedding>,
        /// Root, only in its owner.
        pub root: Option<u32>,
        /// Node counter, only with the root.
        pub next_node: Option<u32>,
        /// Edge counter, only with the root.
        pub next_edge: Option<u32>,
        /// Boundary.
        pub boundary: Boundary,
    }
    /// Shape-stamped segment.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AnySegment {
        /// Tree segment.
        Tree(TreeSegment),
        /// Table segment.
        Table(TableSegment),
        /// Graph segment.
        Graph(GraphSegment),
    }

    impl AnySegment {
        /// Return the dense segment id.
        #[must_use]
        pub const fn segment(&self) -> SegmentId {
            match self {
                Self::Tree(v) => v.segment,
                Self::Table(v) => v.segment,
                Self::Graph(v) => v.segment,
            }
        }
        /// Return the carried instance shape.
        #[must_use]
        pub const fn shape(&self) -> InstanceShapeKind {
            match self {
                Self::Tree(_) => InstanceShapeKind::Tree,
                Self::Table(_) => InstanceShapeKind::Table,
                Self::Graph(_) => InstanceShapeKind::Graph,
            }
        }
        /// Return the segment boundary.
        #[must_use]
        pub const fn boundary(&self) -> &Boundary {
            match self {
                Self::Tree(v) => &v.boundary,
                Self::Table(v) => &v.boundary,
                Self::Graph(v) => &v.boundary,
            }
        }
    }

    /// Whole value, with a tree's stable embedding beside it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AnyInstance {
        /// Tree and stable embedding.
        Tree(Tree, TreeEmbedding),
        /// Table.
        Table(Table),
        /// Graph.
        Graph(Graph),
    }

    /// Per-segment allocation floors replacing a whole complement's global
    /// id floor.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct SegmentFloors {
        /// Next admissible node or row id in this segment's range.
        pub node: u32,
        /// Next admissible edge id in this segment's range.
        pub edge: u32,
    }

    /// The owned restriction of a graph complement, in whole-complement field
    /// order with `floors` replacing `floor` and no identity envelope.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct GraphComplementSegment {
        /// Source nodes absent from the view and owned by this segment.
        pub dropped_nodes: BTreeMap<u32, Node>,
        /// Source edges absent from the view and source-owned here.
        pub dropped_edges: BTreeMap<u32, GEdge>,
        /// Surviving edges whose original representation must be restored.
        pub original_edges: BTreeMap<u32, GEdge>,
        /// Surviving nodes whose original anchors must be restored.
        pub original_anchors: BTreeMap<u32, Name>,
        /// Surviving nodes whose original fields must be restored.
        pub original_fields: BTreeMap<u32, BTreeMap<String, Value>>,
        /// Surviving nodes whose original values must be restored.
        pub original_values: BTreeMap<u32, Option<Presence>>,
        /// Synthesized nodes owned by this segment.
        pub synthesized_nodes: BTreeMap<u32, SynthKey>,
        /// Synthesized edges owned by this segment.
        pub synthesized_edges: BTreeMap<u32, SynthKey>,
        /// Allocation floors for this segment.
        pub floors: SegmentFloors,
    }

    /// The owned restriction of a table complement, in whole-complement field
    /// order with `floors` replacing `id_floor` and no identity envelope.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct TableComplementSegment {
        /// Dropped rows owned by this segment.
        pub dropped_rows: BTreeMap<u32, DroppedRow>,
        /// Dropped cells of rows owned by this segment.
        pub dropped_cells: BTreeMap<(u32, String), Value>,
        /// Dropped foreign-key pairs by source edge.
        pub dropped_fk_pairs: BTreeMap<SEdge, BTreeSet<(u32, u32)>>,
        /// Original edges of surviving foreign-key pairs.
        pub original_fk_edges: BTreeMap<(SEdge, u32, u32), BTreeSet<SEdge>>,
        /// Original tables of surviving rows.
        pub original_tables: BTreeMap<u32, Name>,
        /// Original cells of surviving rows.
        pub original_cells: BTreeMap<(u32, String), Value>,
        /// Synthesized foreign-key pairs owned by this segment.
        pub synthesized_fk_pairs: BTreeSet<(SEdge, u32, u32)>,
        /// Allocation floors for this segment.
        pub floors: SegmentFloors,
    }

    /// The owned restriction of the reference tree complement.
    ///
    /// Its graph-form residue has the graph-complement field order;
    /// `arc_order` stores owned ordinary arcs by embedded edge id and `floors`
    /// replaces the global floor. The identity envelope lives in the manifest.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct TreeComplementSegment {
        /// Dropped non-fan nodes.
        pub dropped_nodes: BTreeMap<u32, Node>,
        /// Dropped embedded arcs and fan edges by edge id.
        pub dropped_edges: BTreeMap<u32, GEdge>,
        /// Original representations of surviving embedded edges.
        pub original_edges: BTreeMap<u32, GEdge>,
        /// Original anchors of surviving nodes.
        pub original_anchors: BTreeMap<u32, Name>,
        /// Original fields of surviving nodes.
        pub original_fields: BTreeMap<u32, BTreeMap<String, Value>>,
        /// Original values of surviving nodes.
        pub original_values: BTreeMap<u32, Option<Presence>>,
        /// Synthesized embedded nodes.
        pub synthesized_nodes: BTreeMap<u32, SynthKey>,
        /// Synthesized embedded edges.
        pub synthesized_edges: BTreeMap<u32, SynthKey>,
        /// Ordinary source arc order as ascending embedded edge ids.
        pub arc_order: Vec<u32>,
        /// Allocation floors for this segment.
        pub floors: SegmentFloors,
    }

    /// A whole tree complement assembled from owned segment restrictions.
    ///
    /// `graph` is the graph-form lens residue and bookkeeping. `arc_order` is
    /// separate reconstruction bookkeeping: it affects emptiness but does not
    /// make the hidden value residue nontrivial.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct AssembledTreeComplement {
        /// Graph-form whole complement.
        pub graph: Complement,
        /// Ordinary source arc order by embedded edge id.
        pub arc_order: Vec<u32>,
    }

    impl AssembledTreeComplement {
        /// Whether no hidden source value residue is present.
        #[must_use]
        pub fn residue_is_trivial(&self) -> bool {
            self.graph.dropped_nodes.is_empty()
                && self.graph.dropped_edges.is_empty()
                && self.graph.original_edges.is_empty()
                && self.graph.original_anchors.is_empty()
                && self.graph.original_fields.is_empty()
                && self.graph.original_values.is_empty()
        }

        /// Whether value residue and all reconstruction bookkeeping are empty.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.residue_is_trivial()
                && self.graph.synthesized_nodes.is_empty()
                && self.graph.synthesized_edges.is_empty()
                && self.graph.floor == (0, 0)
                && self.graph.identity.is_none()
                && self.arc_order.is_empty()
        }
    }

    /// Shape-tagged complement segment. Variant order is tree, table, graph.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AnyComplementSegment {
        /// Tree complement segment.
        Tree(TreeComplementSegment),
        /// Table complement segment.
        Table(TableComplementSegment),
        /// Graph complement segment.
        Graph(GraphComplementSegment),
    }

    impl AnyComplementSegment {
        /// Carried instance shape.
        #[must_use]
        pub const fn shape(&self) -> InstanceShapeKind {
            match self {
                Self::Tree(_) => InstanceShapeKind::Tree,
                Self::Table(_) => InstanceShapeKind::Table,
                Self::Graph(_) => InstanceShapeKind::Graph,
            }
        }

        /// Whether every residue and bookkeeping field is empty or zero.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            match self {
                Self::Graph(value) => value.is_empty(),
                Self::Table(value) => value.is_empty(),
                Self::Tree(value) => value.is_empty(),
            }
        }

        /// Whether the segment records no hidden source residue.
        #[must_use]
        pub fn residue_is_trivial(&self) -> bool {
            match self {
                Self::Graph(value) => value.residue_is_trivial(),
                Self::Table(value) => value.residue_is_trivial(),
                Self::Tree(value) => value.residue_is_trivial(),
            }
        }

        /// Number of complement elements retained for budget accounting.
        #[must_use]
        pub fn element_count(&self) -> u64 {
            let count = match self {
                Self::Graph(value) => value.element_count(),
                Self::Table(value) => value.element_count(),
                Self::Tree(value) => value.element_count(),
            };
            u64::try_from(count).unwrap_or(u64::MAX)
        }
    }

    macro_rules! graph_complement_methods {
        ($type:ty $(, $arc_order:ident)?) => {
            impl $type {
                /// Whether no hidden source residue is present.
                #[must_use]
                pub fn residue_is_trivial(&self) -> bool {
                    self.dropped_nodes.is_empty()
                        && self.dropped_edges.is_empty()
                        && self.original_edges.is_empty()
                        && self.original_anchors.is_empty()
                        && self.original_fields.is_empty()
                        && self.original_values.is_empty()
                }

                /// Whether residue and allocation bookkeeping are empty.
                #[must_use]
                pub fn is_empty(&self) -> bool {
                    self.residue_is_trivial()
                        && self.synthesized_nodes.is_empty()
                        && self.synthesized_edges.is_empty()
                        && self.floors == SegmentFloors::default()
                        $(&& self.$arc_order.is_empty())?
                }

                fn element_count(&self) -> usize {
                    self.dropped_nodes.len()
                        + self.dropped_edges.len()
                        + self.original_edges.len()
                        + self.original_anchors.len()
                        + self.original_fields.len()
                        + self.original_values.len()
                        + self.synthesized_nodes.len()
                        + self.synthesized_edges.len()
                        $(+ self.$arc_order.len())?
                }
            }
        };
    }

    graph_complement_methods!(GraphComplementSegment);
    graph_complement_methods!(TreeComplementSegment, arc_order);

    impl TableComplementSegment {
        /// Whether no hidden source residue is present.
        #[must_use]
        pub fn residue_is_trivial(&self) -> bool {
            self.dropped_rows.is_empty()
                && self.dropped_cells.is_empty()
                && self.dropped_fk_pairs.is_empty()
                && self.original_fk_edges.is_empty()
                && self.original_tables.is_empty()
                && self.original_cells.is_empty()
        }

        /// Whether residue and allocation bookkeeping are empty.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.residue_is_trivial()
                && self.synthesized_fk_pairs.is_empty()
                && self.floors == SegmentFloors::default()
        }

        fn element_count(&self) -> usize {
            self.dropped_rows.len()
                + self.dropped_cells.len()
                + self
                    .dropped_fk_pairs
                    .values()
                    .map(BTreeSet::len)
                    .sum::<usize>()
                + self
                    .original_fk_edges
                    .values()
                    .map(BTreeSet::len)
                    .sum::<usize>()
                + self.original_tables.len()
                + self.original_cells.len()
                + self.synthesized_fk_pairs.len()
        }
    }

    fn extend_owned<K, V>(
        target: &mut BTreeMap<K, V>,
        source: &BTreeMap<K, V>,
    ) -> Result<(), SegmentError>
    where
        K: Ord + Clone,
        V: Clone,
    {
        for (key, value) in source {
            if target.insert(key.clone(), value.clone()).is_some() {
                return Err(decode_error(
                    ArtifactKind::ComplementSegment,
                    "complement element occurs in more than one segment",
                ));
            }
        }
        Ok(())
    }

    fn extend_owned_sets<K, V>(
        target: &mut BTreeMap<K, BTreeSet<V>>,
        source: &BTreeMap<K, BTreeSet<V>>,
    ) -> Result<(), SegmentError>
    where
        K: Ord + Clone,
        V: Ord + Clone,
    {
        for (key, values) in source {
            let union = target.entry(key.clone()).or_default();
            for value in values {
                if !union.insert(value.clone()) {
                    return Err(decode_error(
                        ArtifactKind::ComplementSegment,
                        "complement pair occurs in more than one segment",
                    ));
                }
            }
        }
        Ok(())
    }

    fn merge_graph_complement_fields(
        whole: &mut Complement,
        value: &GraphComplementSegment,
    ) -> Result<(), SegmentError> {
        extend_owned(&mut whole.dropped_nodes, &value.dropped_nodes)?;
        extend_owned(&mut whole.dropped_edges, &value.dropped_edges)?;
        extend_owned(&mut whole.original_edges, &value.original_edges)?;
        extend_owned(&mut whole.original_anchors, &value.original_anchors)?;
        extend_owned(&mut whole.original_fields, &value.original_fields)?;
        extend_owned(&mut whole.original_values, &value.original_values)?;
        extend_owned(&mut whole.synthesized_nodes, &value.synthesized_nodes)?;
        extend_owned(&mut whole.synthesized_edges, &value.synthesized_edges)?;
        whole.floor.0 = whole.floor.0.max(value.floors.node);
        whole.floor.1 = whole.floor.1.max(value.floors.edge);
        Ok(())
    }

    struct SynthRekey {
        values: BTreeMap<u32, SynthKey>,
        ids: BTreeMap<u32, u32>,
    }

    fn synth_targets(
        index: &BTreeMap<u32, SynthKey>,
    ) -> Result<BTreeMap<SynthKey, u32>, SegmentError> {
        let mut targets = BTreeMap::new();
        for (id, key) in index {
            if targets.insert(key.clone(), *id).is_some() {
                return Err(decode_error(
                    ArtifactKind::ComplementSegment,
                    "assembled SynthIndex repeats a structural key",
                ));
            }
        }
        Ok(targets)
    }

    fn coherent_synth_rekey(
        values: &BTreeMap<u32, SynthKey>,
        targets: &BTreeMap<SynthKey, u32>,
    ) -> Result<SynthRekey, SegmentError> {
        let mut rekeyed = BTreeMap::new();
        let mut ids = BTreeMap::new();
        for (source, key) in values {
            let target = targets.get(key).copied().ok_or_else(|| {
                decode_error(
                    ArtifactKind::ComplementSegment,
                    "segment synthesized key is absent from the assembled SynthIndex",
                )
            })?;
            if rekeyed.insert(target, key.clone()).is_some() {
                return Err(decode_error(
                    ArtifactKind::ComplementSegment,
                    "segment synthesized keys collide after coherent renumbering",
                ));
            }
            ids.insert(*source, target);
        }
        Ok(SynthRekey {
            values: rekeyed,
            ids,
        })
    }

    fn rekey_graph_complement(
        value: &GraphComplementSegment,
        node_targets: &BTreeMap<SynthKey, u32>,
        edge_targets: &BTreeMap<SynthKey, u32>,
    ) -> Result<(GraphComplementSegment, BTreeMap<u32, u32>), SegmentError> {
        let nodes = coherent_synth_rekey(&value.synthesized_nodes, node_targets)?;
        let edges = coherent_synth_rekey(&value.synthesized_edges, edge_targets)?;
        let mut value = value.clone();
        value.synthesized_nodes = nodes.values;
        value.synthesized_edges = edges.values;
        Ok((value, edges.ids))
    }

    fn check_assembled_index(
        whole: &Complement,
        assembled: &SynthIndex,
    ) -> Result<(), SegmentError> {
        if whole.synthesized_nodes == assembled.nodes && whole.synthesized_edges == assembled.edges
        {
            Ok(())
        } else {
            Err(decode_error(
                ArtifactKind::ComplementSegment,
                "complement union does not cover the assembled SynthIndex",
            ))
        }
    }

    /// Assemble graph complement segments into the reference whole
    /// complement. Duplicate owned elements are refused.
    ///
    /// # Errors
    ///
    /// Returns a complement decode error when two segments claim one element
    /// or their local synthesized ids cannot be coherently renamed to the
    /// assembled index.
    pub fn assemble_graph_complement(
        segments: &[GraphComplementSegment],
        assembled: &SynthIndex,
    ) -> Result<Complement, SegmentError> {
        let node_targets = synth_targets(&assembled.nodes)?;
        let edge_targets = synth_targets(&assembled.edges)?;
        let mut whole = Complement::default();
        for value in segments {
            let (value, _) = rekey_graph_complement(value, &node_targets, &edge_targets)?;
            merge_graph_complement_fields(&mut whole, &value)?;
        }
        check_assembled_index(&whole, assembled)?;
        Ok(whole)
    }

    /// Assemble table complement segments into the reference whole
    /// complement. Duplicate owned elements are refused.
    ///
    /// # Errors
    ///
    /// Returns a complement decode error when two segments claim one element.
    pub fn assemble_table_complement(
        segments: &[TableComplementSegment],
    ) -> Result<TableComplement, SegmentError> {
        let mut whole = TableComplement::empty();
        for value in segments {
            validate_table_complement(value)?;
            extend_owned(&mut whole.dropped_rows, &value.dropped_rows)?;
            extend_owned(&mut whole.dropped_cells, &value.dropped_cells)?;
            extend_owned_sets(&mut whole.dropped_fk_pairs, &value.dropped_fk_pairs)?;
            extend_owned(&mut whole.original_fk_edges, &value.original_fk_edges)?;
            extend_owned(&mut whole.original_tables, &value.original_tables)?;
            extend_owned(&mut whole.original_cells, &value.original_cells)?;
            for pair in &value.synthesized_fk_pairs {
                if !whole.synthesized_fk_pairs.insert(pair.clone()) {
                    return Err(decode_error(
                        ArtifactKind::ComplementSegment,
                        "synthesized pair occurs in more than one segment",
                    ));
                }
            }
            whole.id_floor = whole.id_floor.max(value.floors.node);
        }
        Ok(whole)
    }

    /// Assemble tree complement segments into the reference graph-form whole
    /// complement and its embedded ordinary-arc order.
    ///
    /// # Errors
    ///
    /// Returns a complement decode error for duplicate residue or arc ids, or
    /// a synthesized presentation that cannot be coherently renamed to the
    /// assembled index.
    pub fn assemble_tree_complement_value(
        segments: &[TreeComplementSegment],
        assembled: &SynthIndex,
    ) -> Result<AssembledTreeComplement, SegmentError> {
        let node_targets = synth_targets(&assembled.nodes)?;
        let edge_targets = synth_targets(&assembled.edges)?;
        let mut whole = Complement::default();
        let mut arc_order = BTreeSet::new();
        for value in segments {
            validate_tree_arc_order(value)?;
            let graph = GraphComplementSegment {
                dropped_nodes: value.dropped_nodes.clone(),
                dropped_edges: value.dropped_edges.clone(),
                original_edges: value.original_edges.clone(),
                original_anchors: value.original_anchors.clone(),
                original_fields: value.original_fields.clone(),
                original_values: value.original_values.clone(),
                synthesized_nodes: value.synthesized_nodes.clone(),
                synthesized_edges: value.synthesized_edges.clone(),
                floors: value.floors,
            };
            let (graph, edge_ids) = rekey_graph_complement(&graph, &node_targets, &edge_targets)?;
            merge_graph_complement_fields(&mut whole, &graph)?;
            for edge in &value.arc_order {
                let edge = edge_ids.get(edge).copied().unwrap_or(*edge);
                if !arc_order.insert(edge) {
                    return Err(decode_error(
                        ArtifactKind::ComplementSegment,
                        "tree arc order contains a duplicate edge id",
                    ));
                }
            }
        }
        check_assembled_index(&whole, assembled)?;
        Ok(AssembledTreeComplement {
            graph: whole,
            arc_order: arc_order.into_iter().collect(),
        })
    }

    /// Assemble tree complement segments into the legacy two-part result.
    ///
    /// # Errors
    ///
    /// As [`assemble_tree_complement_value`].
    pub fn assemble_tree_complement(
        segments: &[TreeComplementSegment],
        assembled: &SynthIndex,
    ) -> Result<(Complement, Vec<u32>), SegmentError> {
        let value = assemble_tree_complement_value(segments, assembled)?;
        Ok((value.graph, value.arc_order))
    }

    impl AnyInstance {
        /// Return the whole instance shape.
        #[must_use]
        pub const fn shape(&self) -> InstanceShapeKind {
            match self {
                Self::Tree(_, _) => InstanceShapeKind::Tree,
                Self::Table(_) => InstanceShapeKind::Table,
                Self::Graph(_) => InstanceShapeKind::Graph,
            }
        }
    }

    fn index(id: SegmentId) -> usize {
        usize::try_from(id.0).unwrap_or(usize::MAX)
    }
    fn owner(p: &Partition, id: u32, elem: ViewRef) -> Result<SegmentId, SegmentError> {
        p.owner_of_node(id).ok_or(SegmentError::Unowned { elem })
    }
    fn ensure_shape(p: &Partition, expected: InstanceShapeKind) -> Result<(), SegmentError> {
        if p.shape == expected {
            p.validate()
        } else {
            Err(SegmentError::ShapeMismatch {
                expected,
                found: p.shape,
            })
        }
    }
    fn record_cross(
        boundaries: &mut [Boundary],
        key: ViewRef,
        stub: InStub,
        target: SegmentId,
        anchor: Name,
    ) {
        let source = stub.src_owner;
        boundaries[index(source)]
            .ghosts
            .entry(stub.tgt)
            .or_insert(Ghost {
                node: stub.tgt,
                owner: target,
                anchor,
            });
        boundaries[index(target)].in_stubs.insert(key, stub);
    }

    /// Partition a graph by source ownership.
    ///
    /// # Errors
    ///
    /// Returns the first partition, ownership, identity, or endpoint defect.
    pub fn partition_graph(g: &Graph, p: &Partition) -> Result<Vec<GraphSegment>, SegmentError> {
        ensure_shape(p, InstanceShapeKind::Graph)?;
        let mut out: Vec<_> = p
            .specs
            .iter()
            .map(|spec| GraphSegment {
                segment: spec.id,
                nodes: BTreeMap::new(),
                edges: BTreeMap::new(),
                boundary: Boundary::default(),
            })
            .collect();
        for (key, node) in &g.nodes {
            if *key != node.id {
                return Err(SegmentError::NodeIdMismatch {
                    key: *key,
                    found: node.id,
                });
            }
            let holder = owner(p, *key, ViewRef::Node(*key))?;
            out[index(holder)].nodes.insert(*key, node.clone());
        }
        let mut boundaries = vec![Boundary::default(); out.len()];
        for (id, edge) in &g.edges {
            for endpoint in [edge.src, edge.tgt] {
                if !g.nodes.contains_key(&endpoint) {
                    return Err(SegmentError::DanglingEndpoint {
                        elem: ViewRef::Edge(*id),
                        node: endpoint,
                    });
                }
            }
            let so = owner(p, edge.src, ViewRef::Edge(*id))?;
            let to = owner(p, edge.tgt, ViewRef::Edge(*id))?;
            out[index(so)].edges.insert(*id, edge.clone());
            if so != to {
                record_cross(
                    &mut boundaries,
                    ViewRef::Edge(*id),
                    InStub {
                        src: edge.src,
                        src_owner: so,
                        src_anchor: g.nodes[&edge.src].anchor.clone(),
                        tgt: edge.tgt,
                        label: edge.label.clone(),
                    },
                    to,
                    g.nodes[&edge.tgt].anchor.clone(),
                );
            }
        }
        for (segment, boundary) in out.iter_mut().zip(boundaries) {
            segment.boundary = boundary;
        }
        Ok(out)
    }

    /// Partition a table by referencing-row ownership.
    ///
    /// # Errors
    ///
    /// Returns the first partition, ownership, or endpoint defect.
    pub fn partition_table(t: &Table, p: &Partition) -> Result<Vec<TableSegment>, SegmentError> {
        ensure_shape(p, InstanceShapeKind::Table)?;
        let mut out: Vec<_> = p
            .specs
            .iter()
            .map(|spec| TableSegment {
                segment: spec.id,
                rows: BTreeMap::new(),
                pairs: BTreeSet::new(),
                boundary: Boundary::default(),
            })
            .collect();
        for (id, row) in &t.rows {
            let holder = owner(p, *id, ViewRef::Row(*id))?;
            out[index(holder)].rows.insert(*id, row.clone());
        }
        let mut boundaries = vec![Boundary::default(); out.len()];
        for (edge, src, tgt) in &t.pairs {
            let key = ViewRef::Pair {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            };
            for endpoint in [*src, *tgt] {
                if !t.rows.contains_key(&endpoint) {
                    return Err(SegmentError::DanglingEndpoint {
                        elem: key,
                        node: endpoint,
                    });
                }
            }
            let so = owner(p, *src, key.clone())?;
            let to = owner(p, *tgt, key.clone())?;
            out[index(so)].pairs.insert((edge.clone(), *src, *tgt));
            if so != to {
                record_cross(
                    &mut boundaries,
                    key,
                    InStub {
                        src: *src,
                        src_owner: so,
                        src_anchor: t.rows[src].0.clone(),
                        tgt: *tgt,
                        label: edge.clone(),
                    },
                    to,
                    t.rows[tgt].0.clone(),
                );
            }
        }
        for (segment, boundary) in out.iter_mut().zip(boundaries) {
            segment.boundary = boundary;
        }
        Ok(out)
    }

    fn fan_label(hyper: &str, target: &str, child: Option<&str>) -> SEdge {
        SEdge::new(
            hyper,
            target,
            child.map_or(HYPER_PARENT, |_| HYPER_LABEL),
            child,
        )
    }

    fn check_embedded_arcs(
        t: &Tree,
        e: &TreeEmbedding,
    ) -> Result<(BTreeSet<u32>, BTreeSet<u32>), SegmentError> {
        let mut incoming = BTreeSet::new();
        let mut edges = BTreeSet::new();
        let mut previous = None;
        for (parent, child, label) in &t.arcs {
            if !t.nodes.contains_key(parent) || !t.nodes.contains_key(child) {
                return Err(SegmentError::DanglingEndpoint {
                    elem: ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    },
                    node: if t.nodes.contains_key(parent) {
                        *child
                    } else {
                        *parent
                    },
                });
            }
            if is_fan_kind(&label.kind) {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    },
                });
            }
            if !incoming.insert(*child) {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Node(*child),
                });
            }
            let Some(id) = e.arcs.get(child).copied() else {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    },
                });
            };
            if previous.is_some_and(|old| old >= id) || !edges.insert(id) {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Edge(id),
                });
            }
            previous = Some(id);
        }
        Ok((incoming, edges))
    }

    fn check_embedded_fans(
        t: &Tree,
        e: &TreeEmbedding,
        edges: &mut BTreeSet<u32>,
    ) -> Result<BTreeSet<u32>, SegmentError> {
        let mut fan_nodes = BTreeSet::new();
        for (fan, (node, parent, children)) in t.fans.iter().zip(&e.fans) {
            if t.nodes.contains_key(node)
                || !fan_nodes.insert(*node)
                || children.keys().ne(fan.children.keys())
            {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Node(*node),
                });
            }
            if !t.nodes.contains_key(&fan.parent)
                || fan
                    .children
                    .values()
                    .any(|child| !t.nodes.contains_key(child))
            {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Node(*node),
                });
            }
            for id in std::iter::once(*parent).chain(children.values().copied()) {
                if !edges.insert(id) {
                    return Err(SegmentError::DuplicateEdgeId {
                        edge: id,
                        segments: vec![SegmentId(0), SegmentId(0)],
                    });
                }
            }
        }
        Ok(fan_nodes)
    }

    fn check_embedded_reachability(t: &Tree, incoming: &BTreeSet<u32>) -> Result<(), SegmentError> {
        let roots: Vec<_> = t
            .nodes
            .keys()
            .filter(|id| !incoming.contains(id))
            .copied()
            .collect();
        if roots != [t.root] {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(t.root),
            });
        }
        let mut reached = BTreeSet::from([t.root]);
        loop {
            let old = reached.len();
            for (parent, child, _) in &t.arcs {
                if reached.contains(parent) {
                    reached.insert(*child);
                }
            }
            if old == reached.len() {
                break;
            }
        }
        if let Some(id) = t.nodes.keys().find(|id| !reached.contains(id)) {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(*id),
            });
        }
        Ok(())
    }

    fn check_tree_embedding(t: &Tree, e: &TreeEmbedding) -> Result<(), SegmentError> {
        if !t.nodes.contains_key(&t.root)
            || e.arcs.len() != t.arcs.len()
            || e.fans.len() != t.fans.len()
        {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(t.root),
            });
        }
        let (incoming, mut edges) = check_embedded_arcs(t, e)?;
        let fan_nodes = check_embedded_fans(t, e, &mut edges)?;
        if t.nodes
            .keys()
            .chain(fan_nodes.iter())
            .any(|id| *id >= e.next_node)
        {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(e.next_node),
            });
        }
        if edges.iter().any(|id| *id >= e.next_edge) {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Edge(e.next_edge),
            });
        }
        check_embedded_reachability(t, &incoming)
    }

    fn partition_tree_fans(
        t: &Tree,
        e: &TreeEmbedding,
        p: &Partition,
        out: &mut [TreeSegment],
        boundaries: &mut [Boundary],
    ) -> Result<(), SegmentError> {
        for (ordinal, (fan, (node, parent_edge, child_edges))) in
            t.fans.iter().zip(&e.fans).enumerate()
        {
            let so = owner(p, *node, ViewRef::Node(*node))?;
            let mut fan_node = Node::new(*node, &fan.hyper_edge);
            fan_node.shape = NodeShape::HyperEdge;
            out[index(so)].nodes.insert(*node, fan_node);
            out[index(so)].fans.push(FanEmbedding {
                node: *node,
                parent_edge: *parent_edge,
                child_edges: child_edges.clone(),
                ordinal: u64::try_from(ordinal).unwrap_or(u64::MAX),
            });
            let occurrences = std::iter::once((None, *parent_edge, fan.parent)).chain(
                fan.children
                    .iter()
                    .map(|(label, child)| (Some(label.as_str()), child_edges[label], *child)),
            );
            for (label, id, target) in occurrences {
                let edge = fan_label(&fan.hyper_edge, &t.nodes[&target].anchor, label);
                out[index(so)]
                    .arcs
                    .insert(id, (*node, target, edge.clone()));
                let to = owner(p, target, ViewRef::Edge(id))?;
                if so != to {
                    record_cross(
                        boundaries,
                        ViewRef::Edge(id),
                        InStub {
                            src: *node,
                            src_owner: so,
                            src_anchor: fan.hyper_edge.clone(),
                            tgt: target,
                            label: edge,
                        },
                        to,
                        t.nodes[&target].anchor.clone(),
                    );
                }
            }
        }
        Ok(())
    }

    /// Partition a tree with its stable graph embedding.
    ///
    /// # Errors
    ///
    /// Returns the first partition, ownership, endpoint, or embedding defect.
    pub fn partition_tree(
        t: &Tree,
        e: &TreeEmbedding,
        p: &Partition,
    ) -> Result<Vec<TreeSegment>, SegmentError> {
        ensure_shape(p, InstanceShapeKind::Tree)?;
        check_tree_embedding(t, e)?;
        let mut out: Vec<_> = p
            .specs
            .iter()
            .map(|spec| TreeSegment {
                segment: spec.id,
                nodes: BTreeMap::new(),
                arcs: BTreeMap::new(),
                fans: Vec::new(),
                root: None,
                next_node: None,
                next_edge: None,
                boundary: Boundary::default(),
            })
            .collect();
        for (id, node) in &t.nodes {
            if *id != node.id {
                return Err(SegmentError::NodeIdMismatch {
                    key: *id,
                    found: node.id,
                });
            }
            let holder = owner(p, *id, ViewRef::Node(*id))?;
            out[index(holder)].nodes.insert(*id, node.clone());
            if *id == t.root {
                out[index(holder)].root = Some(*id);
                out[index(holder)].next_node = Some(e.next_node);
                out[index(holder)].next_edge = Some(e.next_edge);
            }
        }
        let mut boundaries = vec![Boundary::default(); out.len()];
        for (parent, child, label) in &t.arcs {
            let key = ViewRef::Arc {
                parent: *parent,
                child: *child,
            };
            let id = e.arcs[child];
            let so = owner(p, *parent, key.clone())?;
            let to = owner(p, *child, key.clone())?;
            out[index(so)]
                .arcs
                .insert(id, (*parent, *child, label.clone()));
            if so != to {
                record_cross(
                    &mut boundaries,
                    key,
                    InStub {
                        src: *parent,
                        src_owner: so,
                        src_anchor: t.nodes[parent].anchor.clone(),
                        tgt: *child,
                        label: label.clone(),
                    },
                    to,
                    t.nodes[child].anchor.clone(),
                );
            }
        }
        partition_tree_fans(t, e, p, &mut out, &mut boundaries)?;
        for (segment, boundary) in out.iter_mut().zip(boundaries) {
            segment.boundary = boundary;
            segment.fans.sort_by_key(|fan| fan.ordinal);
        }
        Ok(out)
    }

    /// Partition any shape after checking shape tags.
    ///
    /// # Errors
    ///
    /// Returns a shape mismatch or the first shape-specific partition defect.
    pub fn partition(
        instance: &AnyInstance,
        p: &Partition,
    ) -> Result<Vec<AnySegment>, SegmentError> {
        if instance.shape() != p.shape {
            return Err(SegmentError::ShapeMismatch {
                expected: instance.shape(),
                found: p.shape,
            });
        }
        match instance {
            AnyInstance::Graph(g) => Ok(partition_graph(g, p)?
                .into_iter()
                .map(AnySegment::Graph)
                .collect()),
            AnyInstance::Table(t) => Ok(partition_table(t, p)?
                .into_iter()
                .map(AnySegment::Table)
                .collect()),
            AnyInstance::Tree(t, e) => Ok(partition_tree(t, e, p)?
                .into_iter()
                .map(AnySegment::Tree)
                .collect()),
        }
    }

    fn arrange<'a>(
        segments: &'a [AnySegment],
        p: &Partition,
    ) -> Result<Vec<&'a AnySegment>, SegmentError> {
        p.validate()?;
        let mut slots = vec![None; p.specs.len()];
        for segment in segments {
            if segment.shape() != p.shape {
                return Err(SegmentError::ShapeMismatch {
                    expected: p.shape,
                    found: segment.shape(),
                });
            }
            let Some(slot) = slots.get_mut(index(segment.segment())) else {
                return Err(SegmentError::MalformedPartition {
                    segment: segment.segment(),
                    defect: PartitionDefect::UnknownSegment,
                });
            };
            if slot.replace(segment).is_some() {
                return Err(SegmentError::MalformedPartition {
                    segment: segment.segment(),
                    defect: PartitionDefect::SegmentRepeated,
                });
            }
            if let AnySegment::Tree(tree) = segment
                && tree.root.is_none()
                && (tree.next_node.is_some() || tree.next_edge.is_some())
            {
                return Err(SegmentError::MalformedPartition {
                    segment: tree.segment,
                    defect: PartitionDefect::CountersWithoutRoot,
                });
            }
        }
        slots
            .into_iter()
            .enumerate()
            .map(|(position, segment)| {
                segment.ok_or_else(|| SegmentError::MalformedPartition {
                    segment: SegmentId(u32::try_from(position).unwrap_or(u32::MAX)),
                    defect: PartitionDefect::SegmentMissing,
                })
            })
            .collect()
    }

    #[derive(Clone)]
    struct OwnedEdge {
        holder: SegmentId,
        key: ViewRef,
        id: Option<u32>,
        src: u32,
        tgt: u32,
        label: SEdge,
    }

    fn anchors(segment: &AnySegment) -> BTreeMap<u32, Name> {
        match segment {
            AnySegment::Graph(value) => value
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.anchor.clone()))
                .collect(),
            AnySegment::Table(value) => value
                .rows
                .iter()
                .map(|(id, (table, _))| (*id, table.clone()))
                .collect(),
            AnySegment::Tree(value) => value
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.anchor.clone()))
                .collect(),
        }
    }

    fn owned_edges(segment: &AnySegment) -> Vec<OwnedEdge> {
        match segment {
            AnySegment::Graph(value) => value
                .edges
                .iter()
                .map(|(id, edge)| OwnedEdge {
                    holder: value.segment,
                    key: ViewRef::Edge(*id),
                    id: Some(*id),
                    src: edge.src,
                    tgt: edge.tgt,
                    label: edge.label.clone(),
                })
                .collect(),
            AnySegment::Table(value) => value
                .pairs
                .iter()
                .map(|(edge, src, tgt)| OwnedEdge {
                    holder: value.segment,
                    key: ViewRef::Pair {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    },
                    id: None,
                    src: *src,
                    tgt: *tgt,
                    label: edge.clone(),
                })
                .collect(),
            AnySegment::Tree(value) => {
                let fan_edges: BTreeSet<_> = value
                    .fans
                    .iter()
                    .flat_map(|fan| {
                        std::iter::once(fan.parent_edge).chain(fan.child_edges.values().copied())
                    })
                    .collect();
                value
                    .arcs
                    .iter()
                    .map(|(id, (src, tgt, label))| OwnedEdge {
                        holder: value.segment,
                        key: if fan_edges.contains(id) {
                            ViewRef::Edge(*id)
                        } else {
                            ViewRef::Arc {
                                parent: *src,
                                child: *tgt,
                            }
                        },
                        id: Some(*id),
                        src: *src,
                        tgt: *tgt,
                        label: label.clone(),
                    })
                    .collect()
            }
        }
    }

    fn require_node_owner(
        p: &Partition,
        found: SegmentId,
        id: u32,
        elem: ViewRef,
    ) -> Result<(), SegmentError> {
        let expected = p
            .owner_of_node(id)
            .ok_or_else(|| SegmentError::Unowned { elem: elem.clone() })?;
        if expected == found {
            Ok(())
        } else {
            Err(SegmentError::WrongOwner {
                elem,
                expected,
                found,
            })
        }
    }

    fn embedded_edge_source(instance: &AnyInstance, edge: u32) -> Option<u32> {
        match instance {
            AnyInstance::Graph(graph) => graph.edges.get(&edge).map(|value| value.src),
            AnyInstance::Table(_) => None,
            AnyInstance::Tree(tree, embedding) => {
                let ordinary = embedding
                    .arcs
                    .iter()
                    .find_map(|(child, id)| (*id == edge).then_some(*child))
                    .and_then(|child| {
                        tree.arcs
                            .iter()
                            .find_map(|(parent, found, _)| (*found == child).then_some(*parent))
                    });
                ordinary.or_else(|| {
                    embedding.fans.iter().find_map(|(node, parent, children)| {
                        (*parent == edge || children.values().any(|id| *id == edge))
                            .then_some(*node)
                    })
                })
            }
        }
    }

    fn instance_has_node(instance: &AnyInstance, node: u32) -> bool {
        match instance {
            AnyInstance::Graph(graph) => graph.nodes.contains_key(&node),
            AnyInstance::Table(table) => table.rows.contains_key(&node),
            AnyInstance::Tree(tree, embedding) => {
                tree.nodes.contains_key(&node)
                    || embedding.fans.iter().any(|(fan, _, _)| *fan == node)
            }
        }
    }

    fn synth_source(
        key: &SynthKey,
        value: &GraphComplementFieldRefs<'_>,
        instance: &AnyInstance,
    ) -> Result<u32, SegmentError> {
        match key {
            SynthKey::Contract { src, .. } | SynthKey::ContractExpand { src, .. } => {
                instance_has_node(instance, *src)
                    .then_some(*src)
                    .ok_or(SegmentError::NotEmbedded {
                        elem: ViewRef::Node(*src),
                    })
            }
            SynthKey::Expand { source_edge, .. } => value
                .dropped_edges
                .get(source_edge)
                .or_else(|| value.original_edges.get(source_edge))
                .map(|edge| edge.src)
                .or_else(|| embedded_edge_source(instance, *source_edge))
                .ok_or(SegmentError::NotEmbedded {
                    elem: ViewRef::Edge(*source_edge),
                }),
        }
    }

    fn validate_graph_complement_ownership(
        value: GraphComplementFieldRefs<'_>,
        output: &AnySegment,
        instance: &AnyInstance,
        p: &Partition,
    ) -> Result<(), SegmentError> {
        let segment = output.segment();
        for id in value
            .dropped_nodes
            .keys()
            .chain(value.original_anchors.keys())
            .chain(value.original_fields.keys())
            .chain(value.original_values.keys())
        {
            require_node_owner(p, segment, *id, ViewRef::Node(*id))?;
        }
        for (id, edge) in value
            .dropped_edges
            .iter()
            .chain(value.original_edges.iter())
        {
            require_node_owner(p, segment, edge.src, ViewRef::Edge(*id))?;
        }
        for (id, key) in value.synthesized_nodes {
            let source = synth_source(key, &value, instance)?;
            require_node_owner(p, segment, source, ViewRef::Node(*id))?;
        }
        for (id, key) in value.synthesized_edges {
            let source = synth_source(key, &value, instance)?;
            require_node_owner(p, segment, source, ViewRef::Edge(*id))?;
        }
        Ok(())
    }

    fn validate_table_complement_ownership(
        value: &TableComplementSegment,
        output: &AnySegment,
        p: &Partition,
    ) -> Result<(), SegmentError> {
        let segment = output.segment();
        for id in value
            .dropped_rows
            .keys()
            .chain(value.dropped_cells.keys().map(|(id, _)| id))
            .chain(value.original_tables.keys())
            .chain(value.original_cells.keys().map(|(id, _)| id))
        {
            require_node_owner(p, segment, *id, ViewRef::Row(*id))?;
        }
        for pairs in value.dropped_fk_pairs.values() {
            for (source, _) in pairs {
                require_node_owner(p, segment, *source, ViewRef::Row(*source))?;
            }
        }
        for (edge, source, target) in value.original_fk_edges.keys() {
            require_node_owner(
                p,
                segment,
                *source,
                ViewRef::Pair {
                    edge: edge.clone(),
                    src: *source,
                    tgt: *target,
                },
            )?;
        }
        for (edge, source, target) in &value.synthesized_fk_pairs {
            require_node_owner(
                p,
                segment,
                *source,
                ViewRef::Pair {
                    edge: edge.clone(),
                    src: *source,
                    tgt: *target,
                },
            )?;
        }
        Ok(())
    }

    fn validate_complement_ownership(
        complement: &AnyComplementSegment,
        output: &AnySegment,
        instance: &AnyInstance,
        p: &Partition,
    ) -> Result<(), SegmentError> {
        match (complement, output) {
            (AnyComplementSegment::Graph(value), AnySegment::Graph(_)) => {
                validate_graph_complement_ownership(graph_fields(value), output, instance, p)
            }
            (AnyComplementSegment::Table(value), AnySegment::Table(_)) => {
                validate_table_complement_ownership(value, output, p)
            }
            (AnyComplementSegment::Tree(value), AnySegment::Tree(_)) => {
                validate_graph_complement_ownership(tree_fields(value), output, instance, p)?;
                for edge in &value.arc_order {
                    let source = value
                        .dropped_edges
                        .get(edge)
                        .or_else(|| value.original_edges.get(edge))
                        .map(|value| value.src)
                        .or_else(|| embedded_edge_source(instance, *edge))
                        .ok_or(SegmentError::NotEmbedded {
                            elem: ViewRef::Edge(*edge),
                        })?;
                    require_node_owner(p, output.segment(), source, ViewRef::Edge(*edge))?;
                }
                Ok(())
            }
            _ => Err(SegmentError::ShapeMismatch {
                expected: output.shape(),
                found: complement.shape(),
            }),
        }
    }

    fn node_ref(shape: InstanceShapeKind, id: u32) -> ViewRef {
        if shape == InstanceShapeKind::Table {
            ViewRef::Row(id)
        } else {
            ViewRef::Node(id)
        }
    }

    fn stub_key_matches(shape: InstanceShapeKind, key: &ViewRef, stub: &InStub) -> bool {
        match (shape, key) {
            (InstanceShapeKind::Graph, ViewRef::Edge(_)) => true,
            (InstanceShapeKind::Tree, ViewRef::Edge(_)) => {
                matches!(stub.label.kind.as_str(), HYPER_PARENT | HYPER_LABEL)
            }
            (InstanceShapeKind::Tree, ViewRef::Arc { parent, child }) => {
                *parent == stub.src && *child == stub.tgt && !is_fan_kind(&stub.label.kind)
            }
            (InstanceShapeKind::Table, ViewRef::Pair { edge, src, tgt }) => {
                *edge == stub.label && *src == stub.src && *tgt == stub.tgt
            }
            _ => false,
        }
    }

    fn push_node_violations(
        segment: &AnySegment,
        p: &Partition,
        held: &BTreeMap<u32, Name>,
        out: &mut Vec<(ViewRef, SegmentError)>,
    ) {
        let nodes = match segment {
            AnySegment::Graph(value) => Some(&value.nodes),
            AnySegment::Tree(value) => Some(&value.nodes),
            AnySegment::Table(_) => None,
        };
        if let Some(nodes) = nodes {
            for (key, node) in nodes {
                if *key != node.id {
                    out.push((
                        ViewRef::Node(*key),
                        SegmentError::NodeIdMismatch {
                            key: *key,
                            found: node.id,
                        },
                    ));
                }
            }
        }
        for id in held.keys() {
            let elem = node_ref(segment.shape(), *id);
            match p.owner_of_node(*id) {
                None => out.push((elem.clone(), SegmentError::Unowned { elem })),
                Some(expected) if expected != segment.segment() => out.push((
                    elem.clone(),
                    SegmentError::WrongOwner {
                        elem,
                        expected,
                        found: segment.segment(),
                    },
                )),
                Some(_) => {}
            }
        }
    }

    fn push_edge_violations(
        segment: &AnySegment,
        p: &Partition,
        held: &BTreeMap<u32, Name>,
        out: &mut Vec<(ViewRef, SegmentError)>,
    ) {
        let me = segment.segment();
        let mut remote = BTreeMap::new();
        for edge in owned_edges(segment) {
            match p.owner_of_node(edge.src) {
                None => out.push((
                    edge.key.clone(),
                    SegmentError::Unowned {
                        elem: edge.key.clone(),
                    },
                )),
                Some(expected) if expected != me => out.push((
                    edge.key.clone(),
                    SegmentError::WrongOwner {
                        elem: edge.key.clone(),
                        expected,
                        found: me,
                    },
                )),
                Some(_) if !held.contains_key(&edge.src) => out.push((
                    edge.key.clone(),
                    SegmentError::DanglingEndpoint {
                        elem: edge.key.clone(),
                        node: edge.src,
                    },
                )),
                Some(_) => {}
            }
            match p.owner_of_node(edge.tgt) {
                None => out.push((
                    edge.key.clone(),
                    SegmentError::Unowned {
                        elem: edge.key.clone(),
                    },
                )),
                Some(owner) if owner == me && !held.contains_key(&edge.tgt) => out.push((
                    edge.key.clone(),
                    SegmentError::DanglingEndpoint {
                        elem: edge.key.clone(),
                        node: edge.tgt,
                    },
                )),
                Some(owner) if owner != me => {
                    remote.insert(edge.tgt, owner);
                }
                Some(_) => {}
            }
        }
        let ghost_ids: BTreeSet<_> = remote
            .keys()
            .chain(segment.boundary().ghosts.keys())
            .copied()
            .collect();
        for node in ghost_ids {
            let valid = segment.boundary().ghosts.get(&node).is_some_and(|ghost| {
                ghost.node == node
                    && remote.get(&node).copied() == Some(ghost.owner)
                    && ghost.owner != me
            });
            if !valid {
                out.push((
                    node_ref(segment.shape(), node),
                    SegmentError::GhostMismatch { node },
                ));
            }
        }
    }

    fn push_stub_violations(
        segment: &AnySegment,
        p: &Partition,
        held: &BTreeMap<u32, Name>,
        out: &mut Vec<(ViewRef, SegmentError)>,
    ) {
        for (key, stub) in &segment.boundary().in_stubs {
            let valid = stub_key_matches(segment.shape(), key, stub)
                && p.owner_of_node(stub.tgt) == Some(segment.segment())
                && p.owner_of_node(stub.src) == Some(stub.src_owner)
                && stub.src_owner != segment.segment();
            if !valid {
                out.push((
                    key.clone(),
                    SegmentError::InStubMismatch { elem: key.clone() },
                ));
            } else if !held.contains_key(&stub.tgt) {
                out.push((
                    key.clone(),
                    SegmentError::DanglingEndpoint {
                        elem: key.clone(),
                        node: stub.tgt,
                    },
                ));
            }
        }
    }

    fn tree_fan_sets(
        tree: &TreeSegment,
        out: &mut Vec<(ViewRef, SegmentError)>,
    ) -> (BTreeSet<u32>, BTreeSet<u32>) {
        let mut fan_nodes = BTreeSet::new();
        let mut fan_edges = BTreeSet::new();
        let mut previous_ordinal = None;
        for fan in &tree.fans {
            let fan_node = tree.nodes.get(&fan.node);
            let node_ok = previous_ordinal.is_none_or(|old| old < fan.ordinal)
                && fan_nodes.insert(fan.node)
                && fan_node.is_some_and(|node| node.shape == NodeShape::HyperEdge);
            previous_ordinal = Some(fan.ordinal);
            let anchor_of = |target: u32| {
                tree.nodes
                    .get(&target)
                    .map(|node| node.anchor.as_str())
                    .or_else(|| {
                        tree.boundary
                            .ghosts
                            .get(&target)
                            .map(|ghost| ghost.anchor.as_str())
                    })
            };
            let edge_matches = |id: u32, child: Option<&str>| {
                tree.arcs.get(&id).is_some_and(|(src, target, edge)| {
                    *src == fan.node
                        && fan_node.is_some_and(|node| {
                            anchor_of(*target).is_some_and(|anchor| {
                                *edge == fan_label(&node.anchor, anchor, child)
                            })
                        })
                })
            };
            let parent_ok =
                fan_edges.insert(fan.parent_edge) && edge_matches(fan.parent_edge, None);
            let mut children_ok = true;
            for (label, id) in &fan.child_edges {
                let unique = fan_edges.insert(*id);
                let matches = edge_matches(*id, Some(label.as_str()));
                children_ok &= unique && matches;
            }
            if !(node_ok && parent_ok && children_ok) {
                let elem = ViewRef::Node(fan.node);
                out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
            }
        }
        for (id, node) in &tree.nodes {
            if node.shape == NodeShape::HyperEdge && !fan_nodes.contains(id) {
                let elem = ViewRef::Node(*id);
                out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
            }
        }
        (fan_nodes, fan_edges)
    }

    fn push_tree_structure_violations(
        tree: &TreeSegment,
        p: &Partition,
        fan_nodes: &BTreeSet<u32>,
        fan_edges: &BTreeSet<u32>,
        out: &mut Vec<(ViewRef, SegmentError)>,
    ) {
        if tree.root.is_some() != (tree.next_node.is_some() && tree.next_edge.is_some()) {
            let elem = ViewRef::Node(tree.root.unwrap_or(0));
            out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
        }
        if let Some(root) = tree.root
            && (!tree
                .nodes
                .get(&root)
                .is_some_and(|node| node.shape != NodeShape::HyperEdge)
                || p.owner_of_node(root) != Some(tree.segment))
        {
            let elem = ViewRef::Node(root);
            out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
        }
        if let Some(next) = tree.next_node
            && let Some(id) = tree.nodes.range(next..).next().map(|(id, _)| *id)
        {
            let elem = ViewRef::Node(id);
            out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
        }
        if let Some(next) = tree.next_edge
            && let Some(id) = tree.arcs.range(next..).next().map(|(id, _)| *id)
        {
            let elem = ViewRef::Edge(id);
            out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
        }
        let mut incoming: BTreeMap<u32, u32> = BTreeMap::new();
        for (id, (src, target, label)) in &tree.arcs {
            if !fan_edges.contains(id) {
                if fan_nodes.contains(src) || fan_nodes.contains(target) || is_fan_kind(&label.kind)
                {
                    let elem = ViewRef::Arc {
                        parent: *src,
                        child: *target,
                    };
                    out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
                }
                *incoming.entry(*target).or_default() += 1;
            }
        }
        for (key, stub) in &tree.boundary.in_stubs {
            if let ViewRef::Arc { .. } = key {
                if fan_nodes.contains(&stub.tgt) {
                    out.push((
                        key.clone(),
                        SegmentError::InStubMismatch { elem: key.clone() },
                    ));
                }
                *incoming.entry(stub.tgt).or_default() += 1;
            }
        }
        for (node, count) in incoming {
            if count > 1 && !fan_nodes.contains(&node) {
                let elem = ViewRef::Node(node);
                out.push((elem.clone(), SegmentError::NotEmbedded { elem }));
            }
        }
    }

    fn local_violations(segment: &AnySegment, p: &Partition) -> Vec<(ViewRef, SegmentError)> {
        let mut out = Vec::new();
        let held = anchors(segment);
        push_node_violations(segment, p, &held, &mut out);
        push_edge_violations(segment, p, &held, &mut out);
        push_stub_violations(segment, p, &held, &mut out);
        if let AnySegment::Tree(tree) = segment {
            let (fan_nodes, fan_edges) = tree_fan_sets(tree, &mut out);
            push_tree_structure_violations(tree, p, &fan_nodes, &fan_edges, &mut out);
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out
    }

    /// S10a preflight without allocating the `2^32` carrier.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::ValidationViewUnrepresentable`] when the sum
    /// exceeds the complete `u32` id domain.
    pub const fn validation_view_preflight(
        segment: SegmentId,
        owned: u64,
        stubs: u64,
    ) -> Result<u64, SegmentError> {
        let edges = owned.saturating_add(stubs);
        if edges > VALIDATION_VIEW_EDGE_CAPACITY {
            Err(SegmentError::ValidationViewUnrepresentable { segment, edges })
        } else {
            Ok(edges)
        }
    }

    /// Check one segment's local SG.2 contract and S10a preflight.
    ///
    /// # Errors
    ///
    /// Returns a shape or partition defect, a capacity refusal, or all sorted
    /// local contract violations inside [`SegmentError::Invalid`].
    pub fn validate_segment_contract(
        segment: &AnySegment,
        p: &Partition,
    ) -> Result<(), SegmentError> {
        if segment.shape() != p.shape {
            return Err(SegmentError::ShapeMismatch {
                expected: p.shape,
                found: segment.shape(),
            });
        }
        p.validate()?;
        if p.spec(segment.segment()).is_none() {
            return Err(SegmentError::MalformedPartition {
                segment: segment.segment(),
                defect: PartitionDefect::UnknownSegment,
            });
        }
        let violations = local_violations(segment, p);
        if !violations.is_empty() {
            return Err(SegmentError::Invalid {
                violations: violations.into_iter().map(|(_, error)| error).collect(),
            });
        }
        if segment.shape() != InstanceShapeKind::Table {
            validation_view_preflight(
                segment.segment(),
                u64::try_from(owned_edges(segment).len()).unwrap_or(u64::MAX),
                u64::try_from(segment.boundary().in_stubs.len()).unwrap_or(u64::MAX),
            )?;
        }
        Ok(())
    }

    fn check_cross(ordered: &[&AnySegment], p: &Partition) -> Result<(), SegmentError> {
        let all_anchors: BTreeMap<_, _> = ordered
            .iter()
            .flat_map(|segment| anchors(segment))
            .collect();
        let edges: Vec<_> = ordered
            .iter()
            .flat_map(|segment| owned_edges(segment))
            .collect();
        for edge in &edges {
            let to = owner(p, edge.tgt, edge.key.clone())?;
            if to == edge.holder {
                continue;
            }
            let source = ordered[index(edge.holder)];
            let target = ordered[index(to)];
            if !source
                .boundary()
                .ghosts
                .get(&edge.tgt)
                .is_some_and(|ghost| {
                    ghost.node == edge.tgt
                        && ghost.owner == to
                        && all_anchors.get(&edge.tgt) == Some(&ghost.anchor)
                })
            {
                return Err(SegmentError::GhostMismatch { node: edge.tgt });
            }
            let stub_matches = target
                .boundary()
                .in_stubs
                .get(&edge.key)
                .is_some_and(|stub| {
                    let source_matches = (stub.src, stub.src_owner) == (edge.src, edge.holder);
                    let source_anchor_matches =
                        all_anchors.get(&edge.src) == Some(&stub.src_anchor);
                    let target_matches = stub.tgt == edge.tgt;
                    let label_matches = stub.label == edge.label;
                    source_matches && source_anchor_matches && target_matches && label_matches
                });
            if !stub_matches {
                return Err(SegmentError::InStubMismatch {
                    elem: edge.key.clone(),
                });
            }
        }
        let expected: BTreeSet<_> = edges
            .iter()
            .filter_map(|edge| {
                let to = p.owner_of_node(edge.tgt)?;
                (to != edge.holder).then_some((to, edge.key.clone()))
            })
            .collect();
        for segment in ordered {
            for key in segment.boundary().in_stubs.keys() {
                if !expected.contains(&(segment.segment(), key.clone())) {
                    return Err(SegmentError::InStubMismatch { elem: key.clone() });
                }
            }
        }
        Ok(())
    }

    fn check_edge_ids(ordered: &[&AnySegment]) -> Result<(), SegmentError> {
        let mut holders: BTreeMap<u32, Vec<SegmentId>> = BTreeMap::new();
        for edge in ordered.iter().flat_map(|segment| owned_edges(segment)) {
            if let Some(id) = edge.id {
                holders.entry(id).or_default().push(edge.holder);
            }
        }
        if let Some((edge, segments)) = holders.iter().find(|(_, owners)| owners.len() > 1) {
            Err(SegmentError::DuplicateEdgeId {
                edge: *edge,
                segments: segments.clone(),
            })
        } else {
            Ok(())
        }
    }

    /// Glue any shape after exact local, cross-boundary, and identity checks.
    ///
    /// # Errors
    ///
    /// Returns the first defect in segment and element order.
    pub fn glue(segments: &[AnySegment], p: &Partition) -> Result<AnyInstance, SegmentError> {
        let ordered = arrange(segments, p)?;
        for segment in &ordered {
            if let Some((_, error)) = local_violations(segment, p).into_iter().next() {
                return Err(error);
            }
            if segment.shape() != InstanceShapeKind::Table {
                validation_view_preflight(
                    segment.segment(),
                    u64::try_from(owned_edges(segment).len()).unwrap_or(u64::MAX),
                    u64::try_from(segment.boundary().in_stubs.len()).unwrap_or(u64::MAX),
                )?;
            }
        }
        check_cross(&ordered, p)?;
        check_edge_ids(&ordered)?;
        match p.shape {
            InstanceShapeKind::Graph => {
                let mut graph = Graph::default();
                for segment in ordered {
                    let AnySegment::Graph(segment) = segment else {
                        unreachable!("shape checked")
                    };
                    graph.nodes.extend(segment.nodes.clone());
                    graph.edges.extend(segment.edges.clone());
                }
                Ok(AnyInstance::Graph(graph))
            }
            InstanceShapeKind::Table => {
                let mut table = Table::default();
                for segment in ordered {
                    let AnySegment::Table(segment) = segment else {
                        unreachable!("shape checked")
                    };
                    table.rows.extend(segment.rows.clone());
                    table.pairs.extend(segment.pairs.clone());
                }
                Ok(AnyInstance::Table(table))
            }
            InstanceShapeKind::Tree => assemble_tree_any(&ordered),
        }
    }

    type AssembledFan = (u64, Fan, (u32, u32, BTreeMap<Name, u32>));

    #[derive(Default)]
    struct TreeAssemblyParts {
        nodes: BTreeMap<u32, Node>,
        ordinary: BTreeMap<u32, (u32, u32, SEdge)>,
        fans: Vec<AssembledFan>,
        roots: Vec<(u32, Option<u32>, Option<u32>)>,
    }

    fn absorb_tree_segment(
        parts: &mut TreeAssemblyParts,
        segment: &TreeSegment,
    ) -> Result<(), SegmentError> {
        let fan_edges: BTreeSet<_> = segment
            .fans
            .iter()
            .flat_map(|fan| {
                std::iter::once(fan.parent_edge).chain(fan.child_edges.values().copied())
            })
            .collect();
        parts.nodes.extend(
            segment
                .nodes
                .iter()
                .filter(|(_, node)| node.shape != NodeShape::HyperEdge)
                .map(|(id, node)| (*id, node.clone())),
        );
        parts.ordinary.extend(
            segment
                .arcs
                .iter()
                .filter(|(id, _)| !fan_edges.contains(id))
                .map(|(id, arc)| (*id, arc.clone())),
        );
        for fan in &segment.fans {
            let node = segment
                .nodes
                .get(&fan.node)
                .ok_or(SegmentError::NotEmbedded {
                    elem: ViewRef::Node(fan.node),
                })?;
            let (_, parent, _) =
                segment
                    .arcs
                    .get(&fan.parent_edge)
                    .ok_or(SegmentError::NotEmbedded {
                        elem: ViewRef::Edge(fan.parent_edge),
                    })?;
            let mut children = BTreeMap::new();
            for (label, edge) in &fan.child_edges {
                let (_, child, _) = segment.arcs.get(edge).ok_or(SegmentError::NotEmbedded {
                    elem: ViewRef::Edge(*edge),
                })?;
                children.insert(label.clone(), *child);
            }
            parts.fans.push((
                fan.ordinal,
                Fan {
                    parent: *parent,
                    hyper_edge: node.anchor.clone(),
                    children,
                },
                (fan.node, fan.parent_edge, fan.child_edges.clone()),
            ));
        }
        if let Some(root) = segment.root {
            parts
                .roots
                .push((root, segment.next_node, segment.next_edge));
        }
        Ok(())
    }

    fn assemble_tree_any(ordered: &[&AnySegment]) -> Result<AnyInstance, SegmentError> {
        let mut parts = TreeAssemblyParts::default();
        for segment in ordered {
            let AnySegment::Tree(segment) = segment else {
                unreachable!("shape checked")
            };
            absorb_tree_segment(&mut parts, segment)?;
        }
        if parts.roots.len() != 1 {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(parts.roots.first().map_or(0, |v| v.0)),
            });
        }
        let (root, Some(next_node), Some(next_edge)) = parts.roots[0] else {
            return Err(SegmentError::NotEmbedded {
                elem: ViewRef::Node(parts.roots[0].0),
            });
        };
        parts.fans.sort_by_key(|(ordinal, _, _)| *ordinal);
        for (expected, (ordinal, _, ids)) in parts.fans.iter().enumerate() {
            if *ordinal != u64::try_from(expected).unwrap_or(u64::MAX) {
                return Err(SegmentError::NotEmbedded {
                    elem: ViewRef::Node(ids.0),
                });
            }
        }
        let tree = Tree {
            nodes: parts.nodes,
            arcs: parts.ordinary.values().cloned().collect(),
            fans: parts
                .fans
                .iter()
                .map(|(_, value, _)| value.clone())
                .collect(),
            root,
        };
        let embedding = TreeEmbedding {
            arcs: parts
                .ordinary
                .iter()
                .map(|(edge, (_, child, _))| (*child, *edge))
                .collect(),
            fans: parts.fans.into_iter().map(|(_, _, ids)| ids).collect(),
            next_node,
            next_edge,
        };
        check_tree_embedding(&tree, &embedding)?;
        Ok(AnyInstance::Tree(tree, embedding))
    }

    /// Shape-specific graph assembly wrapper.
    ///
    /// # Errors
    ///
    /// Returns the first graph segment or partition defect.
    pub fn assemble_graph(segments: &[GraphSegment], p: &Partition) -> Result<Graph, SegmentError> {
        let any: Vec<_> = segments.iter().cloned().map(AnySegment::Graph).collect();
        match glue(&any, p)? {
            AnyInstance::Graph(value) => Ok(value),
            _ => unreachable!("shape checked"),
        }
    }
    /// Shape-specific table assembly wrapper.
    ///
    /// # Errors
    ///
    /// Returns the first table segment or partition defect.
    pub fn assemble_table(segments: &[TableSegment], p: &Partition) -> Result<Table, SegmentError> {
        let any: Vec<_> = segments.iter().cloned().map(AnySegment::Table).collect();
        match glue(&any, p)? {
            AnyInstance::Table(value) => Ok(value),
            _ => unreachable!("shape checked"),
        }
    }
    /// Shape-specific tree assembly wrapper.
    ///
    /// # Errors
    ///
    /// Returns the first tree segment, partition, or embedding defect.
    pub fn assemble_tree(
        segments: &[TreeSegment],
        p: &Partition,
    ) -> Result<(Tree, TreeEmbedding), SegmentError> {
        let any: Vec<_> = segments.iter().cloned().map(AnySegment::Tree).collect();
        match glue(&any, p)? {
            AnyInstance::Tree(tree, embedding) => Ok((tree, embedding)),
            _ => unreachable!("shape checked"),
        }
    }

    /// Append-floor allocation. This is deliberately not least-absent.
    #[must_use]
    pub fn fresh(range: IdRange, source: &BTreeSet<u32>, used: &BTreeSet<u32>) -> Option<u32> {
        let greatest =
            |set: &BTreeSet<u32>| set.iter().copied().filter(|id| range.contains(*id)).max();
        let floor = greatest(source).map_or(Some(range.lo), |id| {
            id.checked_add(1).map(|next| next.max(range.lo))
        })?;
        let candidate = greatest(used).map_or(Some(floor), |id| {
            id.checked_add(1).map(|next| next.max(floor))
        })?;
        (candidate <= range.hi && candidate != u32::MAX).then_some(candidate)
    }

    /// Allocate from one segment's declared range with the append-floor rule.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::NoAllocationRange`] when the selected range is
    /// absent and [`SegmentError::AllocationExhausted`] when no admissible id
    /// remains.
    pub fn allocate_fresh(
        p: &Partition,
        segment: SegmentId,
        kind: IdKind,
        source: &BTreeSet<u32>,
        used: &BTreeSet<u32>,
    ) -> Result<u32, SegmentError> {
        let spec = p.spec(segment).ok_or(SegmentError::MalformedPartition {
            segment,
            defect: PartitionDefect::UnknownSegment,
        })?;
        let range = match kind {
            IdKind::Node => spec.alloc_nodes,
            IdKind::Edge => spec.alloc_edges,
        }
        .ok_or(SegmentError::NoAllocationRange { segment, kind })?;
        fresh(range, source, used).ok_or(SegmentError::AllocationExhausted { segment, kind })
    }

    /// Check the SG.8 admission rule for a newly added edge id. Replacements
    /// retain their id; every other addition must lie in the source owner's
    /// edge-allocation range.
    ///
    /// # Errors
    ///
    /// Returns a typed missing-range or outside-range refusal.
    pub fn check_new_edge_id(
        p: &Partition,
        segment: SegmentId,
        edge: u32,
        replacement: bool,
    ) -> Result<(), SegmentError> {
        if replacement {
            return Ok(());
        }
        let range = p
            .spec(segment)
            .ok_or(SegmentError::MalformedPartition {
                segment,
                defect: PartitionDefect::UnknownSegment,
            })?
            .alloc_edges
            .ok_or(SegmentError::NoAllocationRange {
                segment,
                kind: IdKind::Edge,
            })?;
        if range.contains(edge) {
            Ok(())
        } else {
            Err(SegmentError::EdgeIdOutsideAllocation { edge, segment })
        }
    }

    #[derive(Default)]
    struct Canonical(Vec<u8>);
    impl Canonical {
        fn tag(&mut self, value: u8) {
            self.0.push(value);
        }
        fn bool(&mut self, value: bool) {
            self.tag(u8::from(value));
        }
        fn u16(&mut self, value: u16) {
            self.0.extend(value.to_le_bytes());
        }
        fn u32(&mut self, value: u32) {
            self.0.extend(value.to_le_bytes());
        }
        fn u64(&mut self, value: u64) {
            self.0.extend(value.to_le_bytes());
        }
        fn len(&mut self, value: usize) {
            self.u64(u64::try_from(value).unwrap_or(u64::MAX));
        }
        fn bytes(&mut self, value: &[u8]) {
            self.len(value.len());
            self.0.extend(value);
        }
        fn string(&mut self, value: &str) {
            self.bytes(value.as_bytes());
        }
        fn digest(&mut self, value: Digest) {
            self.0.extend(value.0);
        }
    }
    const fn shape_tag(shape: InstanceShapeKind) -> u8 {
        match shape {
            InstanceShapeKind::Tree => 0,
            InstanceShapeKind::Table => 1,
            InstanceShapeKind::Graph => 2,
        }
    }
    fn write_edge(w: &mut Canonical, edge: &SEdge) {
        w.string(&edge.src);
        w.string(&edge.tgt);
        w.string(&edge.kind);
        w.bool(edge.name.is_some());
        if let Some(name) = &edge.name {
            w.string(name);
        }
    }
    fn write_ref(w: &mut Canonical, elem: &ViewRef) {
        match elem {
            ViewRef::Node(id) => {
                w.tag(0);
                w.u32(*id);
            }
            ViewRef::Edge(id) => {
                w.tag(1);
                w.u32(*id);
            }
            ViewRef::Arc { parent, child } => {
                w.tag(2);
                w.u32(*parent);
                w.u32(*child);
            }
            ViewRef::Row(id) => {
                w.tag(3);
                w.u32(*id);
            }
            ViewRef::Pair { edge, src, tgt } => {
                w.tag(4);
                write_edge(w, edge);
                w.u32(*src);
                w.u32(*tgt);
            }
        }
    }
    fn write_range(w: &mut Canonical, range: IdRange) {
        w.u32(range.lo);
        w.u32(range.hi);
    }
    fn write_opt_range(w: &mut Canonical, range: Option<IdRange>) {
        w.bool(range.is_some());
        if let Some(range) = range {
            write_range(w, range);
        }
    }
    fn write_spec(w: &mut Canonical, spec: &SegmentSpec) {
        w.u32(spec.id.0);
        w.len(spec.nodes.len());
        for range in &spec.nodes {
            write_range(w, *range);
        }
        write_opt_range(w, spec.alloc_nodes);
        write_opt_range(w, spec.alloc_edges);
    }
    fn write_boundary(w: &mut Canonical, boundary: &Boundary) {
        w.len(boundary.ghosts.len());
        for (id, ghost) in &boundary.ghosts {
            w.u32(*id);
            w.u32(ghost.node);
            w.u32(ghost.owner.0);
            w.string(&ghost.anchor);
        }
        w.len(boundary.in_stubs.len());
        for (elem, stub) in &boundary.in_stubs {
            write_ref(w, elem);
            w.u32(stub.src);
            w.u32(stub.src_owner.0);
            w.string(&stub.src_anchor);
            w.u32(stub.tgt);
            write_edge(w, &stub.label);
        }
    }

    fn write_value_canonical(w: &mut Canonical, value: &Value) -> Result<(), SegmentError> {
        let mut record = Writer::default();
        write_value(&mut record, value)
            .ok_or_else(|| decode_error(ArtifactKind::ComplementSegment, "noncanonical value"))?;
        w.0.extend(record.bytes);
        Ok(())
    }

    fn write_presence_canonical(
        w: &mut Canonical,
        value: Option<&Presence>,
    ) -> Result<(), SegmentError> {
        match value {
            None => w.tag(0),
            Some(Presence::Present(value)) => {
                w.tag(1);
                write_value_canonical(w, value)?;
            }
            Some(Presence::Null) => w.tag(2),
            Some(Presence::Absent) => w.tag(3),
        }
        Ok(())
    }

    fn write_synth_key(w: &mut Canonical, key: &SynthKey) {
        match key {
            SynthKey::Contract { src, tgt, edge } => {
                w.tag(0);
                w.u32(*src);
                w.u32(*tgt);
                write_edge(w, edge);
            }
            SynthKey::Expand { source_edge, hop } => {
                w.tag(1);
                w.u32(*source_edge);
                w.u16(*hop);
            }
            SynthKey::ContractExpand { src, tgt, hop } => {
                w.tag(2);
                w.u32(*src);
                w.u32(*tgt);
                w.u16(*hop);
            }
        }
    }

    fn write_graph_complement_fields<T>(
        w: &mut Canonical,
        value: &T,
        fields: fn(&T) -> GraphComplementFieldRefs<'_>,
    ) -> Result<(), SegmentError> {
        let value = fields(value);
        if let Some((key, node)) = value
            .dropped_nodes
            .iter()
            .find(|(key, node)| **key != node.id)
        {
            return Err(SegmentError::NodeIdMismatch {
                key: *key,
                found: node.id,
            });
        }
        w.len(value.dropped_nodes.len());
        let mut records = Writer::default();
        for node in value.dropped_nodes.values() {
            write_node(&mut records, node).ok_or_else(|| {
                decode_error(ArtifactKind::ComplementSegment, "noncanonical dropped node")
            })?;
        }
        w.0.extend(records.bytes);
        for edges in [value.dropped_edges, value.original_edges] {
            w.len(edges.len());
            let mut records = Writer::default();
            for (id, edge) in edges {
                write_graph_edge(&mut records, *id, edge);
            }
            w.0.extend(records.bytes);
        }
        w.len(value.original_anchors.len());
        for (id, anchor) in value.original_anchors {
            w.u32(*id);
            w.string(anchor);
        }
        w.len(value.original_fields.len());
        for (id, map) in value.original_fields {
            w.u32(*id);
            let mut record = Writer::default();
            write_map(&mut record, map).ok_or_else(|| {
                decode_error(
                    ArtifactKind::ComplementSegment,
                    "noncanonical original fields",
                )
            })?;
            w.0.extend(record.bytes);
        }
        w.len(value.original_values.len());
        for (id, original) in value.original_values {
            w.u32(*id);
            write_presence_canonical(w, original.as_ref())?;
        }
        for synthesized in [value.synthesized_nodes, value.synthesized_edges] {
            w.len(synthesized.len());
            for (id, key) in synthesized {
                w.u32(*id);
                write_synth_key(w, key);
            }
        }
        Ok(())
    }

    fn validate_tree_arc_order(value: &TreeComplementSegment) -> Result<(), SegmentError> {
        if value.arc_order.windows(2).any(|pair| pair[0] >= pair[1]) {
            Err(decode_error(
                ArtifactKind::ComplementSegment,
                "tree arc order is not strictly ascending",
            ))
        } else {
            Ok(())
        }
    }

    fn validate_table_complement(value: &TableComplementSegment) -> Result<(), SegmentError> {
        if value.floors.edge == 0 {
            Ok(())
        } else {
            Err(decode_error(
                ArtifactKind::ComplementSegment,
                "table complement has a nonzero edge floor",
            ))
        }
    }

    #[derive(Clone, Copy)]
    struct GraphComplementFieldRefs<'a> {
        dropped_nodes: &'a BTreeMap<u32, Node>,
        dropped_edges: &'a BTreeMap<u32, GEdge>,
        original_edges: &'a BTreeMap<u32, GEdge>,
        original_anchors: &'a BTreeMap<u32, Name>,
        original_fields: &'a BTreeMap<u32, BTreeMap<String, Value>>,
        original_values: &'a BTreeMap<u32, Option<Presence>>,
        synthesized_nodes: &'a BTreeMap<u32, SynthKey>,
        synthesized_edges: &'a BTreeMap<u32, SynthKey>,
    }

    const fn graph_fields(value: &GraphComplementSegment) -> GraphComplementFieldRefs<'_> {
        GraphComplementFieldRefs {
            dropped_nodes: &value.dropped_nodes,
            dropped_edges: &value.dropped_edges,
            original_edges: &value.original_edges,
            original_anchors: &value.original_anchors,
            original_fields: &value.original_fields,
            original_values: &value.original_values,
            synthesized_nodes: &value.synthesized_nodes,
            synthesized_edges: &value.synthesized_edges,
        }
    }

    const fn tree_fields(value: &TreeComplementSegment) -> GraphComplementFieldRefs<'_> {
        GraphComplementFieldRefs {
            dropped_nodes: &value.dropped_nodes,
            dropped_edges: &value.dropped_edges,
            original_edges: &value.original_edges,
            original_anchors: &value.original_anchors,
            original_fields: &value.original_fields,
            original_values: &value.original_values,
            synthesized_nodes: &value.synthesized_nodes,
            synthesized_edges: &value.synthesized_edges,
        }
    }

    fn write_pair(w: &mut Canonical, pair: &(SEdge, u32, u32)) {
        write_edge(w, &pair.0);
        w.u32(pair.1);
        w.u32(pair.2);
    }

    fn write_table_complement_fields(
        w: &mut Canonical,
        value: &TableComplementSegment,
    ) -> Result<(), SegmentError> {
        w.len(value.dropped_rows.len());
        for (id, row) in &value.dropped_rows {
            w.u32(*id);
            w.string(&row.table);
            let mut record = Writer::default();
            write_map(&mut record, &row.row).ok_or_else(|| {
                decode_error(ArtifactKind::ComplementSegment, "noncanonical dropped row")
            })?;
            w.0.extend(record.bytes);
        }
        w.len(value.dropped_cells.len());
        for ((id, field), cell) in &value.dropped_cells {
            w.u32(*id);
            w.string(field);
            write_value_canonical(w, cell)?;
        }
        w.len(value.dropped_fk_pairs.len());
        for (edge, pairs) in &value.dropped_fk_pairs {
            write_edge(w, edge);
            w.len(pairs.len());
            for (source, target) in pairs {
                w.u32(*source);
                w.u32(*target);
            }
        }
        w.len(value.original_fk_edges.len());
        for ((edge, source, target), originals) in &value.original_fk_edges {
            write_edge(w, edge);
            w.u32(*source);
            w.u32(*target);
            w.len(originals.len());
            for original in originals {
                write_edge(w, original);
            }
        }
        w.len(value.original_tables.len());
        for (id, table) in &value.original_tables {
            w.u32(*id);
            w.string(table);
        }
        w.len(value.original_cells.len());
        for ((id, field), cell) in &value.original_cells {
            w.u32(*id);
            w.string(field);
            write_value_canonical(w, cell)?;
        }
        w.len(value.synthesized_fk_pairs.len());
        for pair in &value.synthesized_fk_pairs {
            write_pair(w, pair);
        }
        Ok(())
    }

    /// Canonical externally-tagged complement-segment bytes.
    ///
    /// # Errors
    ///
    /// Returns a structural or decode error if a map key, tree order, or
    /// retained value has no canonical encoding.
    pub fn canonical_complement_segment_bytes(
        segment: &AnyComplementSegment,
    ) -> Result<Vec<u8>, SegmentError> {
        let mut bytes = Canonical::default();
        bytes.u16(SEGMENT_FORMAT_VERSION);
        match segment {
            AnyComplementSegment::Tree(value) => {
                validate_tree_arc_order(value)?;
                bytes.tag(0);
                write_graph_complement_fields(&mut bytes, value, tree_fields)?;
                bytes.len(value.arc_order.len());
                for edge in &value.arc_order {
                    bytes.u32(*edge);
                }
                bytes.u32(value.floors.node);
                bytes.u32(value.floors.edge);
            }
            AnyComplementSegment::Table(value) => {
                validate_table_complement(value)?;
                bytes.tag(1);
                write_table_complement_fields(&mut bytes, value)?;
                bytes.u32(value.floors.node);
                bytes.u32(value.floors.edge);
            }
            AnyComplementSegment::Graph(value) => {
                bytes.tag(2);
                write_graph_complement_fields(&mut bytes, value, graph_fields)?;
                bytes.u32(value.floors.node);
                bytes.u32(value.floors.edge);
            }
        }
        Ok(bytes.0)
    }

    impl AnyComplementSegment {
        /// Canonical externally-tagged bytes.
        ///
        /// # Errors
        ///
        /// Returns a structural or decode error for a noncanonical carrier.
        pub fn canonical_bytes(&self) -> Result<Vec<u8>, SegmentError> {
            canonical_complement_segment_bytes(self)
        }

        /// Digest canonical bytes under [`SEGMENT_COMPLEMENT_CONTEXT`].
        ///
        /// # Errors
        ///
        /// Returns a structural or decode error for a noncanonical carrier.
        pub fn digest(&self) -> Result<Digest, SegmentError> {
            Ok(complement_segment_digest(&self.canonical_bytes()?))
        }
    }

    /// Canonical segment bytes, including shape, schema, ranges and boundary.
    ///
    /// # Errors
    ///
    /// Returns a segment-contract defect or an unencodable canonical value.
    pub fn canonical_segment_bytes(
        segment: &AnySegment,
        schema: Digest,
        p: &Partition,
    ) -> Result<Vec<u8>, SegmentError> {
        validate_segment_contract(segment, p)?;
        let spec = p
            .spec(segment.segment())
            .ok_or_else(|| SegmentError::MalformedPartition {
                segment: segment.segment(),
                defect: PartitionDefect::UnknownSegment,
            })?;
        let mut w = Canonical::default();
        w.u16(SEGMENT_FORMAT_VERSION);
        w.tag(shape_tag(segment.shape()));
        w.digest(schema);
        write_spec(&mut w, spec);
        match segment {
            AnySegment::Graph(value) => {
                w.len(value.nodes.len());
                w.len(value.edges.len());
                let mut body = Writer::default();
                for node in value.nodes.values() {
                    write_node(&mut body, node).ok_or_else(|| {
                        decode_error(ArtifactKind::Segment, "noncanonical graph node")
                    })?;
                }
                for (id, edge) in &value.edges {
                    write_graph_edge(&mut body, *id, edge);
                }
                w.0.extend(body.bytes);
                write_boundary(&mut w, &value.boundary);
            }
            AnySegment::Table(value) => {
                w.len(value.rows.len());
                w.len(value.pairs.len());
                let mut body = Writer::default();
                for (id, (table, cells)) in &value.rows {
                    body.u32(*id);
                    body.str(table);
                    write_map(&mut body, cells).ok_or_else(|| {
                        decode_error(ArtifactKind::Segment, "noncanonical table row")
                    })?;
                }
                for (edge, source, target) in &value.pairs {
                    body.edge(edge);
                    body.u32(*source);
                    body.u32(*target);
                }
                w.0.extend(body.bytes);
                write_boundary(&mut w, &value.boundary);
            }
            AnySegment::Tree(value) => {
                for item in [value.root, value.next_node, value.next_edge] {
                    w.bool(item.is_some());
                    if let Some(item) = item {
                        w.u32(item);
                    }
                }
                w.len(value.nodes.len());
                w.len(value.arcs.len());
                w.len(value.fans.len());
                let mut body = Writer::default();
                for node in value.nodes.values() {
                    write_node(&mut body, node).ok_or_else(|| {
                        decode_error(ArtifactKind::Segment, "noncanonical tree node")
                    })?;
                }
                for (id, (parent, child, edge)) in &value.arcs {
                    body.u32(*id);
                    body.u32(*parent);
                    body.u32(*child);
                    body.edge(edge);
                }
                w.0.extend(body.bytes);
                let mut canonical_fans: Vec<_> = value.fans.iter().collect();
                canonical_fans.sort_by_key(|fan| fan.node);
                for fan in canonical_fans {
                    w.u32(fan.node);
                    w.u32(fan.parent_edge);
                    w.len(fan.child_edges.len());
                    for (label, edge) in &fan.child_edges {
                        w.string(label);
                        w.u32(*edge);
                    }
                    w.u64(fan.ordinal);
                }
                write_boundary(&mut w, &value.boundary);
            }
        }
        Ok(w.0)
    }

    #[derive(Clone, Copy, Debug)]
    struct DecodeFailure;

    fn read_bool(reader: &mut Reader<'_>) -> Option<bool> {
        match reader.tag()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn read_digest(reader: &mut Reader<'_>) -> Option<Digest> {
        Some(Digest(reader.bytes(32)?.try_into().ok()?))
    }

    fn read_range(reader: &mut Reader<'_>) -> Option<IdRange> {
        Some(IdRange {
            lo: reader.u32()?,
            hi: reader.u32()?,
        })
    }

    fn read_opt_range(reader: &mut Reader<'_>) -> Result<Option<IdRange>, DecodeFailure> {
        if read_bool(reader).ok_or(DecodeFailure)? {
            Ok(Some(read_range(reader).ok_or(DecodeFailure)?))
        } else {
            Ok(None)
        }
    }

    fn read_spec(reader: &mut Reader<'_>) -> Option<SegmentSpec> {
        let id = SegmentId(reader.u32()?);
        let nodes = (0..reader.count()?)
            .map(|_| read_range(reader))
            .collect::<Option<Vec<_>>>()?;
        Some(SegmentSpec {
            id,
            nodes,
            alloc_nodes: read_opt_range(reader).ok()?,
            alloc_edges: read_opt_range(reader).ok()?,
        })
    }

    fn read_ref(reader: &mut Reader<'_>) -> Option<ViewRef> {
        match reader.tag()? {
            0 => Some(ViewRef::Node(reader.u32()?)),
            1 => Some(ViewRef::Edge(reader.u32()?)),
            2 => Some(ViewRef::Arc {
                parent: reader.u32()?,
                child: reader.u32()?,
            }),
            3 => Some(ViewRef::Row(reader.u32()?)),
            4 => Some(ViewRef::Pair {
                edge: reader.edge()?,
                src: reader.u32()?,
                tgt: reader.u32()?,
            }),
            _ => None,
        }
    }

    fn read_boundary(reader: &mut Reader<'_>) -> Option<Boundary> {
        let mut ghosts = BTreeMap::new();
        for _ in 0..reader.count()? {
            let key = reader.u32()?;
            if ghosts.keys().next_back().is_some_and(|old| *old >= key) {
                return None;
            }
            ghosts.insert(
                key,
                Ghost {
                    node: reader.u32()?,
                    owner: SegmentId(reader.u32()?),
                    anchor: reader.str()?,
                },
            );
        }
        let mut in_stubs = BTreeMap::new();
        for _ in 0..reader.count()? {
            let key = read_ref(reader)?;
            if in_stubs.keys().next_back().is_some_and(|old| old >= &key) {
                return None;
            }
            in_stubs.insert(
                key,
                InStub {
                    src: reader.u32()?,
                    src_owner: SegmentId(reader.u32()?),
                    src_anchor: reader.str()?,
                    tgt: reader.u32()?,
                    label: reader.edge()?,
                },
            );
        }
        Some(Boundary { ghosts, in_stubs })
    }

    fn read_presence(reader: &mut Reader<'_>) -> Result<Option<Presence>, DecodeFailure> {
        match reader.tag().ok_or(DecodeFailure)? {
            0 => Ok(None),
            1 => Ok(Some(Presence::Present(
                read_value(reader).ok_or(DecodeFailure)?,
            ))),
            2 => Ok(Some(Presence::Null)),
            3 => Ok(Some(Presence::Absent)),
            _ => Err(DecodeFailure),
        }
    }

    fn read_synth_key(reader: &mut Reader<'_>) -> Option<SynthKey> {
        match reader.tag()? {
            0 => Some(SynthKey::Contract {
                src: reader.u32()?,
                tgt: reader.u32()?,
                edge: reader.edge()?,
            }),
            1 => Some(SynthKey::Expand {
                source_edge: reader.u32()?,
                hop: u16::from_le_bytes(reader.bytes(2)?.try_into().ok()?),
            }),
            2 => Some(SynthKey::ContractExpand {
                src: reader.u32()?,
                tgt: reader.u32()?,
                hop: u16::from_le_bytes(reader.bytes(2)?.try_into().ok()?),
            }),
            _ => None,
        }
    }

    fn read_edge(reader: &mut Reader<'_>) -> Option<SEdge> {
        reader.edge()
    }

    struct GraphComplementFieldsOwned {
        dropped_nodes: BTreeMap<u32, Node>,
        dropped_edges: BTreeMap<u32, GEdge>,
        original_edges: BTreeMap<u32, GEdge>,
        original_anchors: BTreeMap<u32, Name>,
        original_fields: BTreeMap<u32, BTreeMap<String, Value>>,
        original_values: BTreeMap<u32, Option<Presence>>,
        synthesized_nodes: BTreeMap<u32, SynthKey>,
        synthesized_edges: BTreeMap<u32, SynthKey>,
    }

    fn read_node_map(reader: &mut Reader<'_>) -> Option<BTreeMap<u32, Node>> {
        let mut values = BTreeMap::new();
        for _ in 0..reader.count()? {
            let node = read_node(reader)?;
            if values.keys().next_back().is_some_and(|old| *old >= node.id) {
                return None;
            }
            values.insert(node.id, node);
        }
        Some(values)
    }

    fn read_edge_map(reader: &mut Reader<'_>) -> Option<BTreeMap<u32, GEdge>> {
        let mut values = BTreeMap::new();
        for _ in 0..reader.count()? {
            let (id, edge) = read_graph_edge(reader)?;
            if values.keys().next_back().is_some_and(|old| *old >= id) {
                return None;
            }
            values.insert(id, edge);
        }
        Some(values)
    }

    fn read_graph_complement_fields(reader: &mut Reader<'_>) -> Option<GraphComplementFieldsOwned> {
        let dropped_nodes = read_node_map(reader)?;
        let dropped_edges = read_edge_map(reader)?;
        let original_edges = read_edge_map(reader)?;
        let mut original_anchors = BTreeMap::new();
        for _ in 0..reader.count()? {
            let id = reader.u32()?;
            if original_anchors
                .keys()
                .next_back()
                .is_some_and(|old| *old >= id)
            {
                return None;
            }
            original_anchors.insert(id, reader.str()?);
        }
        let mut original_fields = BTreeMap::new();
        for _ in 0..reader.count()? {
            let id = reader.u32()?;
            if original_fields
                .keys()
                .next_back()
                .is_some_and(|old| *old >= id)
            {
                return None;
            }
            original_fields.insert(id, read_map(reader)?);
        }
        let mut original_values = BTreeMap::new();
        for _ in 0..reader.count()? {
            let id = reader.u32()?;
            if original_values
                .keys()
                .next_back()
                .is_some_and(|old| *old >= id)
            {
                return None;
            }
            original_values.insert(id, read_presence(reader).ok()?);
        }
        let mut read_synth_map = || {
            let mut values = BTreeMap::new();
            for _ in 0..reader.count()? {
                let id = reader.u32()?;
                if values.keys().next_back().is_some_and(|old| *old >= id) {
                    return None;
                }
                values.insert(id, read_synth_key(reader)?);
            }
            Some(values)
        };
        let synthesized_nodes = read_synth_map()?;
        let synthesized_edges = read_synth_map()?;
        Some(GraphComplementFieldsOwned {
            dropped_nodes,
            dropped_edges,
            original_edges,
            original_anchors,
            original_fields,
            original_values,
            synthesized_nodes,
            synthesized_edges,
        })
    }

    fn graph_complement_segment(
        value: GraphComplementFieldsOwned,
        floors: SegmentFloors,
    ) -> GraphComplementSegment {
        GraphComplementSegment {
            dropped_nodes: value.dropped_nodes,
            dropped_edges: value.dropped_edges,
            original_edges: value.original_edges,
            original_anchors: value.original_anchors,
            original_fields: value.original_fields,
            original_values: value.original_values,
            synthesized_nodes: value.synthesized_nodes,
            synthesized_edges: value.synthesized_edges,
            floors,
        }
    }

    fn read_ordered_map<K: Ord, V>(
        reader: &mut Reader<'_>,
        mut entry: impl FnMut(&mut Reader<'_>) -> Option<(K, V)>,
    ) -> Option<BTreeMap<K, V>> {
        let mut values = BTreeMap::new();
        for _ in 0..reader.count()? {
            let (key, value) = entry(reader)?;
            if values.keys().next_back().is_some_and(|old| old >= &key) {
                return None;
            }
            values.insert(key, value);
        }
        Some(values)
    }

    fn read_ordered_set<T: Ord>(
        reader: &mut Reader<'_>,
        mut item: impl FnMut(&mut Reader<'_>) -> Option<T>,
    ) -> Option<BTreeSet<T>> {
        let mut values = BTreeSet::new();
        for _ in 0..reader.count()? {
            let value = item(reader)?;
            if values.iter().next_back().is_some_and(|old| old >= &value) {
                return None;
            }
            values.insert(value);
        }
        Some(values)
    }

    fn read_table_complement(reader: &mut Reader<'_>) -> Option<TableComplementSegment> {
        let dropped_rows = read_ordered_map(reader, |reader| {
            Some((
                reader.u32()?,
                DroppedRow {
                    table: reader.str()?,
                    row: read_map(reader)?,
                },
            ))
        })?;
        let dropped_cells = read_ordered_map(reader, |reader| {
            Some(((reader.u32()?, reader.str()?), read_value(reader)?))
        })?;
        let dropped_fk_pairs = read_ordered_map(reader, |reader| {
            Some((
                reader.edge()?,
                read_ordered_set(reader, |reader| Some((reader.u32()?, reader.u32()?)))?,
            ))
        })?;
        let original_fk_edges = read_ordered_map(reader, |reader| {
            Some((
                (reader.edge()?, reader.u32()?, reader.u32()?),
                read_ordered_set(reader, read_edge)?,
            ))
        })?;
        let original_tables =
            read_ordered_map(reader, |reader| Some((reader.u32()?, reader.str()?)))?;
        let original_cells = read_ordered_map(reader, |reader| {
            Some(((reader.u32()?, reader.str()?), read_value(reader)?))
        })?;
        let synthesized_fk_pairs = read_ordered_set(reader, |reader| {
            Some((reader.edge()?, reader.u32()?, reader.u32()?))
        })?;
        let floors = SegmentFloors {
            node: reader.u32()?,
            edge: reader.u32()?,
        };
        if floors.edge != 0 {
            return None;
        }
        Some(TableComplementSegment {
            dropped_rows,
            dropped_cells,
            dropped_fk_pairs,
            original_fk_edges,
            original_tables,
            original_cells,
            synthesized_fk_pairs,
            floors,
        })
    }

    fn read_complement_floors(reader: &mut Reader<'_>) -> Result<SegmentFloors, SegmentError> {
        Ok(SegmentFloors {
            node: reader.u32().ok_or_else(|| {
                decode_error(ArtifactKind::ComplementSegment, "truncated node floor")
            })?,
            edge: reader.u32().ok_or_else(|| {
                decode_error(ArtifactKind::ComplementSegment, "truncated edge floor")
            })?,
        })
    }

    fn decode_tree_complement(
        reader: &mut Reader<'_>,
    ) -> Result<AnyComplementSegment, SegmentError> {
        let fields = read_graph_complement_fields(reader).ok_or_else(|| {
            decode_error(ArtifactKind::ComplementSegment, "malformed tree complement")
        })?;
        let count = reader.count().ok_or_else(|| {
            decode_error(ArtifactKind::ComplementSegment, "malformed tree arc order")
        })?;
        let mut arc_order = Vec::new();
        for _ in 0..count {
            let edge = reader.u32().ok_or_else(|| {
                decode_error(ArtifactKind::ComplementSegment, "truncated tree arc order")
            })?;
            if arc_order.last().is_some_and(|old| *old >= edge) {
                return Err(decode_error(
                    ArtifactKind::ComplementSegment,
                    "tree arc order is not strictly ascending",
                ));
            }
            arc_order.push(edge);
        }
        let graph = graph_complement_segment(fields, read_complement_floors(reader)?);
        Ok(AnyComplementSegment::Tree(TreeComplementSegment {
            dropped_nodes: graph.dropped_nodes,
            dropped_edges: graph.dropped_edges,
            original_edges: graph.original_edges,
            original_anchors: graph.original_anchors,
            original_fields: graph.original_fields,
            original_values: graph.original_values,
            synthesized_nodes: graph.synthesized_nodes,
            synthesized_edges: graph.synthesized_edges,
            arc_order,
            floors: graph.floors,
        }))
    }

    fn decode_graph_complement(
        reader: &mut Reader<'_>,
    ) -> Result<AnyComplementSegment, SegmentError> {
        let fields = read_graph_complement_fields(reader).ok_or_else(|| {
            decode_error(
                ArtifactKind::ComplementSegment,
                "malformed graph complement",
            )
        })?;
        Ok(AnyComplementSegment::Graph(graph_complement_segment(
            fields,
            read_complement_floors(reader)?,
        )))
    }

    /// Decode canonical externally-tagged complement-segment bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::Decode`] for a bad version or tag,
    /// noncanonical map/set order, a malformed retained value, or trailing
    /// bytes.
    pub fn decode_complement_segment_bytes(
        bytes: &[u8],
    ) -> Result<AnyComplementSegment, SegmentError> {
        let mut reader = Reader::new(bytes);
        let version = u16::from_le_bytes(
            reader
                .bytes(2)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| {
                    decode_error(
                        ArtifactKind::ComplementSegment,
                        "truncated complement version",
                    )
                })?,
        );
        if version != SEGMENT_FORMAT_VERSION {
            return Err(decode_error(
                ArtifactKind::ComplementSegment,
                "unsupported complement version",
            ));
        }
        let segment = match reader.tag() {
            Some(0) => decode_tree_complement(&mut reader)?,
            Some(1) => AnyComplementSegment::Table(read_table_complement(&mut reader).ok_or_else(
                || {
                    decode_error(
                        ArtifactKind::ComplementSegment,
                        "malformed table complement",
                    )
                },
            )?),
            Some(2) => decode_graph_complement(&mut reader)?,
            _ => {
                return Err(decode_error(
                    ArtifactKind::ComplementSegment,
                    "invalid complement shape tag",
                ));
            }
        };
        if !reader.done() {
            return Err(decode_error(
                ArtifactKind::ComplementSegment,
                "trailing complement bytes",
            ));
        }
        Ok(segment)
    }

    impl AnyComplementSegment {
        /// Decode canonical externally-tagged complement-segment bytes.
        ///
        /// # Errors
        ///
        /// Returns a complement decode error for malformed or noncanonical
        /// bytes.
        pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SegmentError> {
            decode_complement_segment_bytes(bytes)
        }
    }

    fn read_graph_segment(reader: &mut Reader<'_>, id: SegmentId) -> Option<AnySegment> {
        let (node_count, edge_count) = (reader.count()?, reader.count()?);
        let mut nodes = BTreeMap::new();
        for _ in 0..node_count {
            let node = read_node(reader)?;
            if nodes.keys().next_back().is_some_and(|old| *old >= node.id) {
                return None;
            }
            nodes.insert(node.id, node);
        }
        let mut edges = BTreeMap::new();
        for _ in 0..edge_count {
            let (edge_id, edge) = read_graph_edge(reader)?;
            if edges.keys().next_back().is_some_and(|old| *old >= edge_id) {
                return None;
            }
            edges.insert(edge_id, edge);
        }
        Some(AnySegment::Graph(GraphSegment {
            segment: id,
            nodes,
            edges,
            boundary: read_boundary(reader)?,
        }))
    }

    fn read_table_segment(reader: &mut Reader<'_>, id: SegmentId) -> Option<AnySegment> {
        let (row_count, pair_count) = (reader.count()?, reader.count()?);
        let mut rows = BTreeMap::new();
        for _ in 0..row_count {
            let row = reader.u32()?;
            if rows.keys().next_back().is_some_and(|old| *old >= row) {
                return None;
            }
            rows.insert(row, (reader.str()?, read_map(reader)?));
        }
        let mut pairs = BTreeSet::new();
        for _ in 0..pair_count {
            let pair = (reader.edge()?, reader.u32()?, reader.u32()?);
            if pairs.iter().next_back().is_some_and(|old| old >= &pair) {
                return None;
            }
            pairs.insert(pair);
        }
        Some(AnySegment::Table(TableSegment {
            segment: id,
            rows,
            pairs,
            boundary: read_boundary(reader)?,
        }))
    }

    fn read_tree_segment(reader: &mut Reader<'_>, id: SegmentId) -> Option<AnySegment> {
        let mut optional = || {
            read_bool(reader).and_then(|present| {
                if present {
                    reader.u32().map(Some)
                } else {
                    Some(None)
                }
            })
        };
        let (root, next_node, next_edge) = (optional()?, optional()?, optional()?);
        let (node_count, arc_count, fan_count) =
            (reader.count()?, reader.count()?, reader.count()?);
        let mut nodes = BTreeMap::new();
        for _ in 0..node_count {
            let node = read_node(reader)?;
            if nodes.keys().next_back().is_some_and(|old| *old >= node.id) {
                return None;
            }
            nodes.insert(node.id, node);
        }
        let mut arcs = BTreeMap::new();
        for _ in 0..arc_count {
            let edge = reader.u32()?;
            if arcs.keys().next_back().is_some_and(|old| *old >= edge) {
                return None;
            }
            arcs.insert(edge, (reader.u32()?, reader.u32()?, reader.edge()?));
        }
        let mut fans = Vec::with_capacity(fan_count);
        let mut previous_fan_node = None;
        let mut ordinals = BTreeSet::new();
        for _ in 0..fan_count {
            let node = reader.u32()?;
            if previous_fan_node.is_some_and(|old| old >= node) {
                return None;
            }
            previous_fan_node = Some(node);
            let parent_edge = reader.u32()?;
            let mut child_edges = BTreeMap::new();
            for _ in 0..reader.count()? {
                let label = reader.str()?;
                if child_edges
                    .keys()
                    .next_back()
                    .is_some_and(|old| old >= &label)
                {
                    return None;
                }
                child_edges.insert(label, reader.u32()?);
            }
            let ordinal = reader.u64()?;
            if !ordinals.insert(ordinal) {
                return None;
            }
            fans.push(FanEmbedding {
                node,
                parent_edge,
                child_edges,
                ordinal,
            });
        }
        fans.sort_by_key(|fan| fan.ordinal);
        Some(AnySegment::Tree(TreeSegment {
            segment: id,
            nodes,
            arcs,
            fans,
            root,
            next_node,
            next_edge,
            boundary: read_boundary(reader)?,
        }))
    }

    /// Decode canonical segment bytes and return the segment with its schema
    /// digest. The decoder refuses noncanonical ordering, malformed headers,
    /// trailing bytes, and every local SG.2 contract defect.
    ///
    /// # Errors
    ///
    /// Returns a typed partition/shape/local-contract error when one can be
    /// recovered, and [`SegmentError::Decode`] for malformed bytes.
    pub fn decode_segment_bytes(
        bytes: &[u8],
        p: &Partition,
    ) -> Result<(AnySegment, Digest), SegmentError> {
        p.validate()?;
        let mut reader = Reader::new(bytes);
        let version = u16::from_le_bytes(
            reader
                .bytes(2)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| decode_error(ArtifactKind::Segment, "truncated segment version"))?,
        );
        if version != SEGMENT_FORMAT_VERSION {
            return Err(decode_error(
                ArtifactKind::Segment,
                "unsupported segment version",
            ));
        }
        let shape = match reader.tag() {
            Some(0) => InstanceShapeKind::Tree,
            Some(1) => InstanceShapeKind::Table,
            Some(2) => InstanceShapeKind::Graph,
            _ => return Err(decode_error(ArtifactKind::Segment, "invalid shape tag")),
        };
        if shape != p.shape {
            return Err(SegmentError::ShapeMismatch {
                expected: p.shape,
                found: shape,
            });
        }
        let schema = read_digest(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Segment, "truncated schema digest"))?;
        let encoded_spec = read_spec(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Segment, "malformed segment spec"))?;
        if p.spec(encoded_spec.id) != Some(&encoded_spec) {
            return Err(decode_error(
                ArtifactKind::Segment,
                "segment spec differs from partition",
            ));
        }
        let segment = match shape {
            InstanceShapeKind::Graph => read_graph_segment(&mut reader, encoded_spec.id),
            InstanceShapeKind::Table => read_table_segment(&mut reader, encoded_spec.id),
            InstanceShapeKind::Tree => read_tree_segment(&mut reader, encoded_spec.id),
        }
        .ok_or_else(|| decode_error(ArtifactKind::Segment, "malformed segment body"))?;
        if !reader.done() {
            return Err(decode_error(
                ArtifactKind::Segment,
                "trailing segment bytes",
            ));
        }
        validate_segment_contract(&segment, p)?;
        Ok((segment, schema))
    }

    impl AnySegment {
        /// Encode this segment with its schema and partition binding.
        ///
        /// # Errors
        ///
        /// Returns the first local segment-contract error.
        pub fn canonical_bytes(
            &self,
            schema: &Digest,
            p: &Partition,
        ) -> Result<Vec<u8>, SegmentError> {
            canonical_segment_bytes(self, *schema, p)
        }

        /// Decode canonical bytes under the supplied partition.
        ///
        /// # Errors
        ///
        /// Returns the first typed header, canonicality, or local-contract
        /// defect.
        pub fn from_canonical_bytes(
            bytes: &[u8],
            p: &Partition,
        ) -> Result<(Self, Digest), SegmentError> {
            decode_segment_bytes(bytes, p)
        }

        /// Digest this segment under [`SEGMENT_CONTEXT`].
        ///
        /// # Errors
        ///
        /// Returns the first local segment-contract error.
        pub fn digest(&self, schema: &Digest, p: &Partition) -> Result<Digest, SegmentError> {
            Ok(Digest::derive(
                SEGMENT_CONTEXT,
                &self.canonical_bytes(schema, p)?,
            ))
        }
    }
    fn decode_error(artifact: ArtifactKind, message: &str) -> SegmentError {
        SegmentError::Decode {
            artifact,
            message: message.to_owned(),
        }
    }

    /// Deterministic segment counts.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct SegmentStats {
        /// Segment id.
        pub segment: SegmentId,
        /// Nodes or rows.
        pub nodes: u64,
        /// Edges, arcs, or pairs.
        pub edges: u64,
        /// Tree fans.
        pub fans: u64,
        /// Ghosts.
        pub ghosts: u64,
        /// Incoming stubs.
        pub in_stubs: u64,
        /// Canonical payload bytes.
        pub payload_bytes: u64,
    }
    /// Per-segment coordinator summary.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentSummary {
        /// Counts.
        pub stats: SegmentStats,
        /// Largest node or row id.
        pub max_node: Option<u32>,
        /// Largest edge id.
        pub max_edge: Option<u32>,
        /// Largest held edge id in each allocation range.
        pub range_max: BTreeMap<SegmentId, u32>,
    }

    fn segment_shape_counts(segment: &AnySegment) -> (usize, usize, usize, Option<u32>) {
        match segment {
            AnySegment::Graph(value) => (
                value.nodes.len(),
                value.edges.len(),
                0,
                value.nodes.keys().next_back().copied(),
            ),
            AnySegment::Table(value) => (
                value.rows.len(),
                value.pairs.len(),
                0,
                value.rows.keys().next_back().copied(),
            ),
            AnySegment::Tree(value) => {
                let fan_nodes: BTreeSet<_> = value.fans.iter().map(|fan| fan.node).collect();
                let fan_edges: BTreeSet<_> = value
                    .fans
                    .iter()
                    .flat_map(|fan| {
                        std::iter::once(fan.parent_edge).chain(fan.child_edges.values().copied())
                    })
                    .collect();
                (
                    value
                        .nodes
                        .keys()
                        .filter(|id| !fan_nodes.contains(id))
                        .count(),
                    value
                        .arcs
                        .keys()
                        .filter(|id| !fan_edges.contains(id))
                        .count(),
                    value.fans.len(),
                    value.nodes.keys().next_back().copied(),
                )
            }
        }
    }

    fn range_maxima(edge_ids: &[u32], p: &Partition) -> BTreeMap<SegmentId, u32> {
        p.specs
            .iter()
            .filter_map(|spec| {
                let range = spec.alloc_edges?;
                let maximum = edge_ids
                    .iter()
                    .copied()
                    .filter(|edge| range.contains(*edge))
                    .max()?;
                Some((spec.id, maximum))
            })
            .collect()
    }

    fn segment_summary(
        segment: &AnySegment,
        p: &Partition,
        schema: Digest,
    ) -> Result<SegmentSummary, SegmentError> {
        let edge_ids: Vec<_> = owned_edges(segment)
            .into_iter()
            .filter_map(|edge| edge.id)
            .collect();
        let (nodes, edges, fans, max_node) = segment_shape_counts(segment);
        let range_max = range_maxima(&edge_ids, p);
        let bytes = canonical_segment_bytes(segment, schema, p)?;
        Ok(SegmentSummary {
            stats: SegmentStats {
                segment: segment.segment(),
                nodes: u64::try_from(nodes).unwrap_or(u64::MAX),
                edges: u64::try_from(edges).unwrap_or(u64::MAX),
                fans: u64::try_from(fans).unwrap_or(u64::MAX),
                ghosts: u64::try_from(segment.boundary().ghosts.len()).unwrap_or(u64::MAX),
                in_stubs: u64::try_from(segment.boundary().in_stubs.len()).unwrap_or(u64::MAX),
                payload_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            },
            max_node,
            max_edge: edge_ids.into_iter().max(),
            range_max,
        })
    }

    /// One canonical manifest record.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentRecord {
        /// Segment id.
        pub id: SegmentId,
        /// Owned ranges.
        pub nodes: Vec<IdRange>,
        /// Node allocation range.
        pub alloc_nodes: Option<IdRange>,
        /// Edge allocation range.
        pub alloc_edges: Option<IdRange>,
        /// Counts.
        pub stats: SegmentStats,
        /// Largest node or row id.
        pub max_node: Option<u32>,
        /// Largest edge id.
        pub max_edge: Option<u32>,
        /// Sorted edge-id-run digest.
        pub edge_ids: Digest,
        /// Content digest.
        pub content: Digest,
        /// Cross digests by target.
        pub cross_out: BTreeMap<SegmentId, Digest>,
        /// Cross digests by source.
        pub cross_in: BTreeMap<SegmentId, Digest>,
    }
    /// Manifest role.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum ManifestRole {
        /// Source instance.
        Source,
        /// View of a source.
        View {
            /// Source manifest digest.
            source: Digest,
            /// Lens identity.
            envelope: IdentityEnvelope,
        },
        /// Complement of a view.
        Complement {
            /// Source manifest digest.
            source: Digest,
            /// View manifest digest.
            view: Digest,
            /// Lens identity.
            envelope: IdentityEnvelope,
        },
    }
    /// Canonical segment manifest.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentManifest {
        /// Format version.
        pub version: u16,
        /// Shape.
        pub shape: InstanceShapeKind,
        /// Schema digest.
        pub schema: Digest,
        /// Role.
        pub role: ManifestRole,
        /// Opaque caller binding.
        pub binding: Option<Digest>,
        /// Dense segment records.
        pub segments: Vec<SegmentRecord>,
        /// Merkle root.
        pub root: Digest,
        /// Whole-instance digest.
        pub instance_digest: Option<Digest>,
    }

    fn write_stats(w: &mut Canonical, stats: SegmentStats) {
        w.u32(stats.segment.0);
        w.u64(stats.nodes);
        w.u64(stats.edges);
        w.u64(stats.fans);
        w.u64(stats.ghosts);
        w.u64(stats.in_stubs);
        w.u64(stats.payload_bytes);
    }
    fn write_opt_u32(w: &mut Canonical, value: Option<u32>) {
        w.bool(value.is_some());
        if let Some(value) = value {
            w.u32(value);
        }
    }
    fn write_digest_map(w: &mut Canonical, map: &BTreeMap<SegmentId, Digest>) {
        w.len(map.len());
        for (segment, digest) in map {
            w.u32(segment.0);
            w.digest(*digest);
        }
    }
    fn record_bytes(record: &SegmentRecord) -> Vec<u8> {
        let mut w = Canonical::default();
        w.u32(record.id.0);
        w.len(record.nodes.len());
        for range in &record.nodes {
            write_range(&mut w, *range);
        }
        write_opt_range(&mut w, record.alloc_nodes);
        write_opt_range(&mut w, record.alloc_edges);
        write_stats(&mut w, record.stats);
        write_opt_u32(&mut w, record.max_node);
        write_opt_u32(&mut w, record.max_edge);
        w.digest(record.edge_ids);
        w.digest(record.content);
        write_digest_map(&mut w, &record.cross_out);
        write_digest_map(&mut w, &record.cross_in);
        w.0
    }
    fn write_role(w: &mut Canonical, role: &ManifestRole) {
        match role {
            ManifestRole::Source => w.tag(0),
            ManifestRole::View { source, envelope } => {
                w.tag(1);
                w.digest(*source);
                w.0.extend(crate::canonical::identity(envelope));
            }
            ManifestRole::Complement {
                source,
                view,
                envelope,
            } => {
                w.tag(2);
                w.digest(*source);
                w.digest(*view);
                w.0.extend(crate::canonical::identity(envelope));
            }
        }
    }
    /// Canonical manifest bytes.
    #[must_use]
    pub fn canonical_manifest_bytes(manifest: &SegmentManifest) -> Vec<u8> {
        let mut w = Canonical::default();
        w.u16(manifest.version);
        w.tag(shape_tag(manifest.shape));
        w.digest(manifest.schema);
        write_role(&mut w, &manifest.role);
        w.bool(manifest.binding.is_some());
        if let Some(binding) = manifest.binding {
            w.digest(binding);
        }
        w.len(manifest.segments.len());
        for record in &manifest.segments {
            w.bytes(&record_bytes(record));
        }
        w.digest(manifest.root);
        w.bool(manifest.instance_digest.is_some());
        if let Some(digest) = manifest.instance_digest {
            w.digest(digest);
        }
        w.0
    }

    fn read_opt_u32(reader: &mut Reader<'_>) -> Result<Option<u32>, DecodeFailure> {
        if read_bool(reader).ok_or(DecodeFailure)? {
            Ok(Some(reader.u32().ok_or(DecodeFailure)?))
        } else {
            Ok(None)
        }
    }

    fn read_stats(reader: &mut Reader<'_>) -> Option<SegmentStats> {
        Some(SegmentStats {
            segment: SegmentId(reader.u32()?),
            nodes: reader.u64()?,
            edges: reader.u64()?,
            fans: reader.u64()?,
            ghosts: reader.u64()?,
            in_stubs: reader.u64()?,
            payload_bytes: reader.u64()?,
        })
    }

    fn read_digest_map(reader: &mut Reader<'_>) -> Option<BTreeMap<SegmentId, Digest>> {
        let mut values = BTreeMap::new();
        for _ in 0..reader.count()? {
            let segment = SegmentId(reader.u32()?);
            if values.keys().next_back().is_some_and(|old| *old >= segment) {
                return None;
            }
            values.insert(segment, read_digest(reader)?);
        }
        Some(values)
    }

    fn read_record(bytes: &[u8]) -> Option<SegmentRecord> {
        let mut reader = Reader::new(bytes);
        let id = SegmentId(reader.u32()?);
        let nodes = (0..reader.count()?)
            .map(|_| read_range(&mut reader))
            .collect::<Option<Vec<_>>>()?;
        let record = SegmentRecord {
            id,
            nodes,
            alloc_nodes: read_opt_range(&mut reader).ok()?,
            alloc_edges: read_opt_range(&mut reader).ok()?,
            stats: read_stats(&mut reader)?,
            max_node: read_opt_u32(&mut reader).ok()?,
            max_edge: read_opt_u32(&mut reader).ok()?,
            edge_ids: read_digest(&mut reader)?,
            content: read_digest(&mut reader)?,
            cross_out: read_digest_map(&mut reader)?,
            cross_in: read_digest_map(&mut reader)?,
        };
        reader.done().then_some(record)
    }

    fn read_envelope(reader: &mut Reader<'_>) -> Option<IdentityEnvelope> {
        let version = u16::from_le_bytes(reader.bytes(2)?.try_into().ok()?);
        if version != 2 {
            return None;
        }
        let source_schema = reader.bytes(32)?.try_into().ok()?;
        let target_schema = reader.bytes(32)?.try_into().ok()?;
        let lens = reader.bytes(32)?.try_into().ok()?;
        let shape = match reader.tag()? {
            0 => ComplementShape::WType,
            1 => ComplementShape::Graph,
            2 => ComplementShape::Functor,
            3 => ComplementShape::Resolution,
            _ => return None,
        };
        let format_version = u16::from_le_bytes(reader.bytes(2)?.try_into().ok()?);
        Some(IdentityEnvelope {
            source_schema,
            target_schema,
            lens,
            shape,
            format_version,
        })
    }

    fn read_role(reader: &mut Reader<'_>) -> Option<ManifestRole> {
        match reader.tag()? {
            0 => Some(ManifestRole::Source),
            1 => Some(ManifestRole::View {
                source: read_digest(reader)?,
                envelope: read_envelope(reader)?,
            }),
            2 => Some(ManifestRole::Complement {
                source: read_digest(reader)?,
                view: read_digest(reader)?,
                envelope: read_envelope(reader)?,
            }),
            _ => None,
        }
    }

    /// Decode canonical manifest bytes and verify their record order, cross
    /// pairing, and Merkle root.
    ///
    /// # Errors
    ///
    /// Returns a manifest decode error for malformed or noncanonical bytes,
    /// or the first typed manifest verification error.
    pub fn decode_manifest_bytes(bytes: &[u8]) -> Result<SegmentManifest, SegmentError> {
        let mut reader = Reader::new(bytes);
        let version = u16::from_le_bytes(
            reader
                .bytes(2)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| {
                    decode_error(ArtifactKind::Manifest, "truncated manifest version")
                })?,
        );
        let shape = match reader.tag() {
            Some(0) => InstanceShapeKind::Tree,
            Some(1) => InstanceShapeKind::Table,
            Some(2) => InstanceShapeKind::Graph,
            _ => return Err(decode_error(ArtifactKind::Manifest, "invalid shape tag")),
        };
        let schema = read_digest(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Manifest, "truncated schema digest"))?;
        let role = read_role(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Manifest, "malformed manifest role"))?;
        let binding = if read_bool(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Manifest, "malformed binding flag"))?
        {
            Some(
                read_digest(&mut reader)
                    .ok_or_else(|| decode_error(ArtifactKind::Manifest, "truncated binding"))?,
            )
        } else {
            None
        };
        let count = reader
            .count()
            .ok_or_else(|| decode_error(ArtifactKind::Manifest, "malformed record count"))?;
        let mut segments = Vec::with_capacity(count);
        for position in 0..count {
            let record = reader
                .raw()
                .and_then(|record| read_record(&record))
                .ok_or_else(|| decode_error(ArtifactKind::Manifest, "malformed segment record"))?;
            let expected = SegmentId(u32::try_from(position).unwrap_or(u32::MAX));
            if record.id != expected {
                return Err(SegmentError::MalformedPartition {
                    segment: expected,
                    defect: PartitionDefect::NotDense { found: record.id },
                });
            }
            if record.stats.segment != expected {
                return Err(decode_error(
                    ArtifactKind::Manifest,
                    "record and statistics segment ids differ",
                ));
            }
            segments.push(record);
        }
        let root = read_digest(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Manifest, "truncated root"))?;
        let instance_digest =
            if read_bool(&mut reader).ok_or_else(|| {
                decode_error(ArtifactKind::Manifest, "malformed instance digest flag")
            })? {
                Some(read_digest(&mut reader).ok_or_else(|| {
                    decode_error(ArtifactKind::Manifest, "truncated instance digest")
                })?)
            } else {
                None
            };
        if !reader.done() {
            return Err(decode_error(
                ArtifactKind::Manifest,
                "trailing manifest bytes",
            ));
        }
        let manifest = SegmentManifest {
            version,
            shape,
            schema,
            role,
            binding,
            segments,
            root,
            instance_digest,
        };
        verify_manifest(&manifest)?;
        Ok(manifest)
    }
    fn merkle(records: &[SegmentRecord]) -> Digest {
        match records.len() {
            0 => Digest::derive(MANIFEST_NODE_CONTEXT, &[]),
            1 => Digest::derive(MANIFEST_LEAF_CONTEXT, &record_bytes(&records[0])),
            n => {
                let split = n.next_power_of_two() / 2;
                let mut bytes = Vec::with_capacity(64);
                bytes.extend(merkle(&records[..split]).0);
                bytes.extend(merkle(&records[split..]).0);
                Digest::derive(MANIFEST_NODE_CONTEXT, &bytes)
            }
        }
    }
    fn instance_bytes(instance: &AnyInstance) -> Result<Vec<u8>, SegmentError> {
        match instance {
            AnyInstance::Graph(value) => graph_bytes(value),
            AnyInstance::Table(value) => table_bytes(value),
            AnyInstance::Tree(value, _) => tree_bytes(value),
        }
        .ok_or_else(|| decode_error(ArtifactKind::Assembly, "noncanonical whole instance"))
    }

    /// The whole-state result computed by the reference Assemble phase.
    /// Complement segments are keyed by their dense output segment id.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentOperationResult {
        /// The operation's whole output before repartitioning.
        pub instance: AnyInstance,
        /// Typed complement residue to emit beside output segments.
        pub complements: BTreeMap<SegmentId, AnyComplementSegment>,
    }

    impl SegmentOperationResult {
        /// Identity operation with no complement residue.
        #[must_use]
        pub fn identity(instance: &AnyInstance) -> Self {
            Self {
                instance: instance.clone(),
                complements: BTreeMap::new(),
            }
        }
    }

    fn validate_operation_result(
        result: &SegmentOperationResult,
        p: &Partition,
    ) -> Result<Vec<AnySegment>, SegmentError> {
        if result.instance.shape() != p.shape {
            return Err(SegmentError::ShapeMismatch {
                expected: p.shape,
                found: result.instance.shape(),
            });
        }
        if let Some(id) = result
            .complements
            .keys()
            .find(|id| p.spec(**id).is_none())
            .copied()
        {
            return Err(SegmentError::MalformedPartition {
                segment: id,
                defect: PartitionDefect::UnknownSegment,
            });
        }
        let emitted = partition(&result.instance, p)?;
        for (id, complement) in &result.complements {
            if complement.shape() != result.instance.shape() {
                return Err(SegmentError::ShapeMismatch {
                    expected: result.instance.shape(),
                    found: complement.shape(),
                });
            }
            complement.canonical_bytes()?;
            let output = emitted
                .get(index(*id))
                .ok_or(SegmentError::MalformedPartition {
                    segment: *id,
                    defect: PartitionDefect::UnknownSegment,
                })?;
            validate_complement_ownership(complement, output, &result.instance, p)?;
        }
        Ok(emitted)
    }

    fn run_instance_bytes(instance: &AnyInstance) -> Result<Vec<u8>, SegmentError> {
        let mut bytes = Canonical::default();
        bytes.tag(shape_tag(instance.shape()));
        match instance {
            AnyInstance::Graph(graph) => bytes.bytes(&graph_bytes(graph).ok_or_else(|| {
                decode_error(ArtifactKind::Assembly, "noncanonical graph result")
            })?),
            AnyInstance::Table(table) => bytes.bytes(&table_bytes(table).ok_or_else(|| {
                decode_error(ArtifactKind::Assembly, "noncanonical table result")
            })?),
            AnyInstance::Tree(tree, embedding) => {
                check_tree_embedding(tree, embedding)?;
                bytes.len(tree.nodes.len());
                bytes.len(tree.arcs.len());
                bytes.len(tree.fans.len());
                let mut records = Writer::default();
                for node in tree.nodes.values() {
                    write_node(&mut records, node).ok_or_else(|| {
                        decode_error(ArtifactKind::Assembly, "noncanonical tree node")
                    })?;
                }
                for (parent, child, edge) in &tree.arcs {
                    records.u32(*parent);
                    records.u32(*child);
                    records.edge(edge);
                }
                for fan in &tree.fans {
                    records.u32(fan.parent);
                    records.str(&fan.hyper_edge);
                    records.len(fan.children.len());
                    for (label, child) in &fan.children {
                        records.str(label);
                        records.u32(*child);
                    }
                }
                bytes.0.extend(records.bytes);
                bytes.u32(tree.root);
                bytes.len(embedding.arcs.len());
                for (child, edge) in &embedding.arcs {
                    bytes.u32(*child);
                    bytes.u32(*edge);
                }
                bytes.len(embedding.fans.len());
                for (node, parent, children) in &embedding.fans {
                    bytes.u32(*node);
                    bytes.u32(*parent);
                    bytes.len(children.len());
                    for (label, edge) in children {
                        bytes.string(label);
                        bytes.u32(*edge);
                    }
                }
                bytes.u32(embedding.next_node);
                bytes.u32(embedding.next_edge);
            }
        }
        Ok(bytes.0)
    }

    fn read_run_tree(reader: &mut Reader<'_>) -> Option<AnyInstance> {
        let (node_count, arc_count, fan_count) =
            (reader.count()?, reader.count()?, reader.count()?);
        let mut nodes = BTreeMap::new();
        for _ in 0..node_count {
            let node = read_node(reader)?;
            if nodes.keys().next_back().is_some_and(|old| *old >= node.id) {
                return None;
            }
            nodes.insert(node.id, node);
        }
        let mut arcs = Vec::with_capacity(arc_count);
        for _ in 0..arc_count {
            arcs.push((reader.u32()?, reader.u32()?, reader.edge()?));
        }
        let mut fans = Vec::with_capacity(fan_count);
        for _ in 0..fan_count {
            let parent = reader.u32()?;
            let hyper_edge = reader.str()?;
            let mut children = BTreeMap::new();
            for _ in 0..reader.count()? {
                let label = reader.str()?;
                if children.keys().next_back().is_some_and(|old| old >= &label) {
                    return None;
                }
                children.insert(label, reader.u32()?);
            }
            fans.push(Fan {
                parent,
                hyper_edge,
                children,
            });
        }
        let root = reader.u32()?;
        let mut embedding_arcs = BTreeMap::new();
        for _ in 0..reader.count()? {
            let child = reader.u32()?;
            if embedding_arcs
                .keys()
                .next_back()
                .is_some_and(|old| *old >= child)
            {
                return None;
            }
            embedding_arcs.insert(child, reader.u32()?);
        }
        let mut embedding_fans = Vec::new();
        for _ in 0..reader.count()? {
            let node = reader.u32()?;
            let parent = reader.u32()?;
            let mut children = BTreeMap::new();
            for _ in 0..reader.count()? {
                let label = reader.str()?;
                if children.keys().next_back().is_some_and(|old| old >= &label) {
                    return None;
                }
                children.insert(label, reader.u32()?);
            }
            embedding_fans.push((node, parent, children));
        }
        let tree = Tree {
            nodes,
            arcs,
            fans,
            root,
        };
        let embedding = TreeEmbedding {
            arcs: embedding_arcs,
            fans: embedding_fans,
            next_node: reader.u32()?,
            next_edge: reader.u32()?,
        };
        check_tree_embedding(&tree, &embedding).ok()?;
        Some(AnyInstance::Tree(tree, embedding))
    }

    fn decode_run_instance_bytes(bytes: &[u8]) -> Option<AnyInstance> {
        let mut reader = Reader::new(bytes);
        let instance = match reader.tag()? {
            0 => read_run_tree(&mut reader)?,
            1 => AnyInstance::Table(decode_table(&reader.raw()?)?),
            2 => AnyInstance::Graph(decode_graph(&reader.raw()?)?),
            _ => return None,
        };
        reader.done().then_some(instance)
    }

    fn operation_result_bytes(result: &SegmentOperationResult) -> Result<Vec<u8>, SegmentError> {
        let mut bytes = Canonical::default();
        bytes.bytes(&run_instance_bytes(&result.instance)?);
        bytes.len(result.complements.len());
        for (segment, complement) in &result.complements {
            if complement.shape() != result.instance.shape() {
                return Err(SegmentError::ShapeMismatch {
                    expected: result.instance.shape(),
                    found: complement.shape(),
                });
            }
            bytes.u32(segment.0);
            bytes.bytes(&complement.canonical_bytes()?);
        }
        Ok(bytes.0)
    }

    fn decode_operation_result(bytes: &[u8]) -> Result<SegmentOperationResult, SegmentError> {
        let mut reader = Reader::new(bytes);
        let instance = reader
            .raw()
            .and_then(|bytes| decode_run_instance_bytes(&bytes))
            .ok_or_else(|| decode_error(ArtifactKind::Assembly, "malformed assembly instance"))?;
        let mut complements = BTreeMap::new();
        let count = reader
            .count()
            .ok_or_else(|| decode_error(ArtifactKind::Assembly, "malformed complement count"))?;
        for _ in 0..count {
            let segment = SegmentId(
                reader
                    .u32()
                    .ok_or_else(|| decode_error(ArtifactKind::Assembly, "truncated segment id"))?,
            );
            if complements
                .keys()
                .next_back()
                .is_some_and(|old| *old >= segment)
            {
                return Err(decode_error(
                    ArtifactKind::Assembly,
                    "complement ids are not strictly ascending",
                ));
            }
            let complement = reader
                .raw()
                .ok_or_else(|| decode_error(ArtifactKind::Assembly, "truncated complement"))
                .and_then(|bytes| AnyComplementSegment::from_canonical_bytes(&bytes))?;
            if complement.shape() != instance.shape() {
                return Err(SegmentError::ShapeMismatch {
                    expected: instance.shape(),
                    found: complement.shape(),
                });
            }
            complements.insert(segment, complement);
        }
        if !reader.done() {
            return Err(decode_error(
                ArtifactKind::Assembly,
                "trailing assembly bytes",
            ));
        }
        Ok(SegmentOperationResult {
            instance,
            complements,
        })
    }

    fn emit_bundle_bytes(
        segment: &AnySegment,
        complement: Option<&AnyComplementSegment>,
        schema: Digest,
        p: &Partition,
    ) -> Result<Vec<u8>, SegmentError> {
        let mut bytes = Canonical::default();
        bytes.bytes(&segment.canonical_bytes(&schema, p)?);
        bytes.bool(complement.is_some());
        if let Some(complement) = complement {
            if complement.shape() != segment.shape() {
                return Err(SegmentError::ShapeMismatch {
                    expected: segment.shape(),
                    found: complement.shape(),
                });
            }
            bytes.bytes(&complement.canonical_bytes()?);
        }
        Ok(bytes.0)
    }

    fn decode_emit_bundle(
        bytes: &[u8],
        schema: Digest,
        p: &Partition,
    ) -> Result<(AnySegment, Option<AnyComplementSegment>), SegmentError> {
        let mut reader = Reader::new(bytes);
        let (segment, found_schema) = reader
            .raw()
            .ok_or_else(|| decode_error(ArtifactKind::Segment, "truncated emitted segment"))
            .and_then(|bytes| AnySegment::from_canonical_bytes(&bytes, p))?;
        if found_schema != schema {
            return Err(SegmentError::SchemaMismatch {
                expected: schema,
                found: found_schema,
            });
        }
        let complement = if read_bool(&mut reader)
            .ok_or_else(|| decode_error(ArtifactKind::Segment, "malformed complement flag"))?
        {
            let value = reader
                .raw()
                .ok_or_else(|| {
                    decode_error(ArtifactKind::ComplementSegment, "truncated complement")
                })
                .and_then(|bytes| AnyComplementSegment::from_canonical_bytes(&bytes))?;
            if value.shape() != segment.shape() {
                return Err(SegmentError::ShapeMismatch {
                    expected: segment.shape(),
                    found: value.shape(),
                });
            }
            Some(value)
        } else {
            None
        };
        if !reader.done() {
            return Err(decode_error(ArtifactKind::Segment, "trailing emit bytes"));
        }
        Ok((segment, complement))
    }

    /// Digest exact whole-instance canonical bytes under the shape-specific
    /// instance context.
    ///
    /// # Errors
    ///
    /// Returns a decode error when an instance contains a value outside the
    /// canonical encoding domain.
    pub fn whole_instance_digest(instance: &AnyInstance) -> Result<Digest, SegmentError> {
        let context = match instance.shape() {
            InstanceShapeKind::Tree => "panproto 2026-09 tree instance v1",
            InstanceShapeKind::Table => "panproto 2026-09 table instance v1",
            InstanceShapeKind::Graph => "panproto 2026-09 graph instance v1",
        };
        Ok(Digest::derive(context, &instance_bytes(instance)?))
    }

    type CrossRecords = BTreeMap<SegmentId, BTreeMap<ViewRef, Vec<u8>>>;
    type CrossDigests = BTreeMap<SegmentId, Digest>;

    fn cross_maps(segments: &[AnySegment]) -> (Vec<CrossDigests>, Vec<CrossDigests>) {
        let mut outgoing: Vec<CrossRecords> = vec![BTreeMap::new(); segments.len()];
        let mut incoming: Vec<CrossRecords> = vec![BTreeMap::new(); segments.len()];
        let all_anchors: BTreeMap<_, _> = segments.iter().flat_map(anchors).collect();
        for segment in segments {
            for edge in owned_edges(segment) {
                for (position, candidate) in segments.iter().enumerate() {
                    let target = SegmentId(u32::try_from(position).unwrap_or(u32::MAX));
                    if target == edge.holder || !anchors(candidate).contains_key(&edge.tgt) {
                        continue;
                    }
                    let mut w = Canonical::default();
                    write_ref(&mut w, &edge.key);
                    w.u32(edge.src);
                    w.string(&all_anchors[&edge.src]);
                    w.u32(edge.tgt);
                    w.string(&all_anchors[&edge.tgt]);
                    write_edge(&mut w, &edge.label);
                    outgoing[index(edge.holder)]
                        .entry(target)
                        .or_default()
                        .insert(edge.key.clone(), w.0.clone());
                    incoming[index(target)]
                        .entry(edge.holder)
                        .or_default()
                        .insert(edge.key.clone(), w.0);
                }
            }
        }
        let finish = |all: Vec<CrossRecords>| {
            all.into_iter()
                .map(|map| {
                    map.into_iter()
                        .map(|(segment, records)| {
                            let bytes = records.into_values().flatten().collect::<Vec<_>>();
                            (segment, Digest::derive(SEGMENT_CROSS_CONTEXT, &bytes))
                        })
                        .collect()
                })
                .collect()
        };
        (finish(outgoing), finish(incoming))
    }

    /// Seal checked segments into a canonical manifest.
    ///
    /// # Errors
    ///
    /// Returns a segment or partition defect, an allocation-floor refusal, or
    /// an unencodable whole instance.
    pub fn seal_manifest(
        segments: &[AnySegment],
        p: &Partition,
        schema: Digest,
        role: ManifestRole,
        binding: Option<Digest>,
    ) -> Result<SegmentManifest, SegmentError> {
        let ordered: Vec<_> = arrange(segments, p)?.into_iter().cloned().collect();
        let whole = glue(&ordered, p)?;
        let global_max_edge = ordered
            .iter()
            .flat_map(owned_edges)
            .filter_map(|edge| edge.id)
            .max();
        if let Some(maximum) = global_max_edge {
            for spec in &p.specs {
                if spec.alloc_edges.is_some_and(|range| range.lo <= maximum) {
                    return Err(SegmentError::AllocationBelowFloor {
                        segment: spec.id,
                        kind: IdKind::Edge,
                    });
                }
            }
        }
        let (cross_out, cross_in) = cross_maps(&ordered);
        let mut records = Vec::with_capacity(ordered.len());
        for segment in &ordered {
            let spec =
                p.spec(segment.segment())
                    .ok_or_else(|| SegmentError::MalformedPartition {
                        segment: segment.segment(),
                        defect: PartitionDefect::UnknownSegment,
                    })?;
            let summary = segment_summary(segment, p, schema)?;
            let bytes = canonical_segment_bytes(segment, schema, p)?;
            let mut ids: Vec<_> = owned_edges(segment)
                .into_iter()
                .filter_map(|edge| edge.id)
                .collect();
            ids.sort_unstable();
            let mut id_bytes = Canonical::default();
            for id in ids {
                id_bytes.u32(id);
            }
            records.push(SegmentRecord {
                id: segment.segment(),
                nodes: spec.nodes.clone(),
                alloc_nodes: spec.alloc_nodes,
                alloc_edges: spec.alloc_edges,
                stats: summary.stats,
                max_node: summary.max_node,
                max_edge: summary.max_edge,
                edge_ids: Digest::derive(SEGMENT_EDGE_IDS_CONTEXT, &id_bytes.0),
                content: Digest::derive(SEGMENT_CONTEXT, &bytes),
                cross_out: cross_out[index(segment.segment())].clone(),
                cross_in: cross_in[index(segment.segment())].clone(),
            });
        }
        let root = merkle(&records);
        Ok(SegmentManifest {
            version: SEGMENT_MANIFEST_VERSION,
            shape: p.shape,
            schema,
            role,
            binding,
            segments: records,
            root,
            instance_digest: Some(whole_instance_digest(&whole)?),
        })
    }

    /// Verify manifest shape, cross maps, and Merkle root without segments.
    ///
    /// # Errors
    ///
    /// Returns a version, density, cross-map, or root-digest mismatch.
    pub fn verify_manifest(manifest: &SegmentManifest) -> Result<(), SegmentError> {
        if manifest.version != SEGMENT_MANIFEST_VERSION {
            return Err(decode_error(
                ArtifactKind::Manifest,
                "unsupported manifest version",
            ));
        }
        manifest_partition(manifest).validate()?;
        for (position, record) in manifest.segments.iter().enumerate() {
            let expected = SegmentId(u32::try_from(position).unwrap_or(u32::MAX));
            if record.id != expected || record.stats.segment != expected {
                return Err(SegmentError::MalformedPartition {
                    segment: expected,
                    defect: PartitionDefect::NotDense { found: record.id },
                });
            }
            for (target, digest) in &record.cross_out {
                if manifest
                    .segments
                    .get(index(*target))
                    .and_then(|peer| peer.cross_in.get(&record.id))
                    != Some(digest)
                {
                    return Err(SegmentError::CrossMismatch {
                        from: record.id,
                        to: *target,
                    });
                }
            }
            for (source, digest) in &record.cross_in {
                if manifest
                    .segments
                    .get(index(*source))
                    .and_then(|peer| peer.cross_out.get(&record.id))
                    != Some(digest)
                {
                    return Err(SegmentError::CrossMismatch {
                        from: *source,
                        to: record.id,
                    });
                }
            }
        }
        let found = merkle(&manifest.segments);
        if found == manifest.root {
            Ok(())
        } else {
            Err(SegmentError::DigestMismatch {
                segment: None,
                expected: manifest.root,
                found,
            })
        }
    }

    fn manifest_partition(manifest: &SegmentManifest) -> Partition {
        Partition {
            shape: manifest.shape,
            specs: manifest
                .segments
                .iter()
                .map(|record| SegmentSpec {
                    id: record.id,
                    nodes: record.nodes.clone(),
                    alloc_nodes: record.alloc_nodes,
                    alloc_edges: record.alloc_edges,
                })
                .collect(),
        }
    }

    fn edge_id_digest(segment: &AnySegment) -> Digest {
        let mut ids: Vec<_> = owned_edges(segment)
            .into_iter()
            .filter_map(|edge| edge.id)
            .collect();
        ids.sort_unstable();
        let mut bytes = Canonical::default();
        for id in ids {
            bytes.u32(id);
        }
        Digest::derive(SEGMENT_EDGE_IDS_CONTEXT, &bytes.0)
    }

    /// Verify one materialized segment against its manifest record.
    ///
    /// # Errors
    ///
    /// Returns the first partition, content, summary, or edge-run mismatch.
    pub fn verify_segment(
        manifest: &SegmentManifest,
        segment: &AnySegment,
    ) -> Result<(), SegmentError> {
        verify_manifest(manifest)?;
        if segment.shape() != manifest.shape {
            return Err(SegmentError::ShapeMismatch {
                expected: manifest.shape,
                found: segment.shape(),
            });
        }
        let p = manifest_partition(manifest);
        let record = manifest
            .segments
            .get(index(segment.segment()))
            .filter(|record| record.id == segment.segment())
            .ok_or_else(|| SegmentError::MalformedPartition {
                segment: segment.segment(),
                defect: PartitionDefect::UnknownSegment,
            })?;
        let summary = segment_summary(segment, &p, manifest.schema)?;
        if summary.stats != record.stats
            || summary.max_node != record.max_node
            || summary.max_edge != record.max_edge
        {
            return Err(SegmentError::DigestMismatch {
                segment: Some(segment.segment()),
                expected: record.content,
                found: segment.digest(&manifest.schema, &p)?,
            });
        }
        let edge_ids = edge_id_digest(segment);
        if edge_ids != record.edge_ids {
            return Err(SegmentError::DigestMismatch {
                segment: Some(segment.segment()),
                expected: record.edge_ids,
                found: edge_ids,
            });
        }
        let found = segment.digest(&manifest.schema, &p)?;
        if found != record.content {
            return Err(SegmentError::DigestMismatch {
                segment: Some(segment.segment()),
                expected: record.content,
                found,
            });
        }
        Ok(())
    }

    /// Decode and verify one canonical segment artifact against a manifest.
    ///
    /// # Errors
    ///
    /// Returns a typed schema mismatch before content verification, or the
    /// first decoder/segment verification error.
    pub fn verify_segment_bytes(
        manifest: &SegmentManifest,
        bytes: &[u8],
    ) -> Result<AnySegment, SegmentError> {
        let p = manifest_partition(manifest);
        let (segment, schema) = AnySegment::from_canonical_bytes(bytes, &p)?;
        if schema != manifest.schema {
            return Err(SegmentError::SchemaMismatch {
                expected: manifest.schema,
                found: schema,
            });
        }
        verify_segment(manifest, &segment)?;
        Ok(segment)
    }

    fn collect_audit_path(records: &[SegmentRecord], index: usize, path: &mut Vec<Digest>) -> bool {
        match records.len() {
            0 => false,
            1 => index == 0,
            n => {
                let split = n.next_power_of_two() / 2;
                if index < split {
                    if !collect_audit_path(&records[..split], index, path) {
                        return false;
                    }
                    path.push(merkle(&records[split..]));
                } else {
                    if !collect_audit_path(&records[split..], index - split, path) {
                        return false;
                    }
                    path.push(merkle(&records[..split]));
                }
                true
            }
        }
    }

    /// Return the bottom-up Merkle audit path for one dense segment id.
    #[must_use]
    pub fn audit_path(manifest: &SegmentManifest, segment: SegmentId) -> Vec<Digest> {
        let mut path = Vec::new();
        if !collect_audit_path(&manifest.segments, index(segment), &mut path) {
            path.clear();
        }
        path
    }

    fn rebuild_audit_root(
        leaf: Digest,
        index: usize,
        count: usize,
        path: &[Digest],
        at: &mut usize,
    ) -> Option<Digest> {
        if count == 1 {
            return (index == 0).then_some(leaf);
        }
        let split = count.next_power_of_two() / 2;
        let child = if index < split {
            rebuild_audit_root(leaf, index, split, path, at)?
        } else {
            rebuild_audit_root(leaf, index - split, count - split, path, at)?
        };
        let sibling = *path.get(*at)?;
        *at += 1;
        let mut bytes = Vec::with_capacity(64);
        if index < split {
            bytes.extend(child.0);
            bytes.extend(sibling.0);
        } else {
            bytes.extend(sibling.0);
            bytes.extend(child.0);
        }
        Some(Digest::derive(MANIFEST_NODE_CONTEXT, &bytes))
    }

    /// Verify a manifest-record leaf against a Merkle root and audit path.
    #[must_use]
    pub fn verify_audit_path(
        root: Digest,
        leaf: Digest,
        index: usize,
        count: usize,
        path: &[Digest],
    ) -> bool {
        if count == 0 || index >= count {
            return false;
        }
        let mut at = 0;
        rebuild_audit_root(leaf, index, count, path, &mut at)
            .is_some_and(|found| found == root && at == path.len())
    }

    /// Digest the canonical leaf bytes of one manifest record.
    #[must_use]
    pub fn manifest_leaf(record: &SegmentRecord) -> Digest {
        Digest::derive(MANIFEST_LEAF_CONTEXT, &record_bytes(record))
    }

    /// Budgeted resource.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Resource {
        /// Peak bytes needed by one materialized segment.
        WorkingSet,
        /// Bytes retained by a bidirectional operation.
        Complement,
        /// Bytes needed by the segment summaries.
        Summary,
        /// Number of segments touched by an operation.
        AffectedSegments,
    }
    /// Segmented operation.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SegmentOp {
        /// Validate the segmented value.
        Validate,
        /// Restrict the segmented value.
        Restrict,
        /// Extend the segmented value.
        Extend,
        /// Run a lens get.
        Get,
        /// Run a lens put.
        Put,
        /// Check the domain of a lens put.
        CheckPutDomain,
        /// Transport an edit forward.
        GetEdit,
        /// Transport an edit backward.
        PutEdit,
        /// Compute a digest.
        Digest,
    }
    /// Summary combination.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SummaryKind {
        /// Reachability summary.
        Reach,
        /// Sorted-stream merge summary.
        SortedMerge,
        /// Associative fold summary.
        Fold,
    }
    /// Global reason.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum GlobalReason {
        /// A traversal builtin crosses arbitrary segment boundaries.
        TraversalBuiltin {
            /// Theory vertex requiring traversal.
            vertex: Name,
        },
        /// A structural field demands a remote child.
        StructuralChildDemand {
            /// Theory vertex owning the field.
            vertex: Name,
            /// Field demanding the child.
            field: String,
        },
        /// A dependent product needs the whole carrier.
        PiProduct,
        /// Closure witnesses require global ordering.
        ClosureWitnessOrder,
        /// Query evaluation is global.
        Query,
        /// Sequential composition has no local exchange plan.
        SequentialComposite,
    }
    /// Locality class.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Locality {
        /// Each segment can run independently.
        Local,
        /// Segments require only declared boundary exchange.
        Exchange,
        /// Segments combine a bounded summary of the given kind.
        Summary(SummaryKind),
        /// The operation cannot be segmented for the stated reason.
        Global(GlobalReason),
    }
    /// Operation-wise locality.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct LocalityReport {
        /// Shape classified by the report.
        pub shape: InstanceShapeKind,
        /// Locality of lens get.
        pub get: Locality,
        /// Locality of lens put.
        pub put: Locality,
        /// Locality of forward edit transport.
        pub get_edit: Locality,
        /// Locality of backward edit transport.
        pub put_edit: Locality,
        /// Locality of validation.
        pub validate: Locality,
        /// Machine-readable explanations for non-local operations.
        pub reasons: Vec<(SegmentOp, Locality, String)>,
    }
    impl LocalityReport {
        const fn identity(shape: InstanceShapeKind) -> Self {
            Self {
                shape,
                get: Locality::Local,
                put: Locality::Local,
                get_edit: Locality::Local,
                put_edit: Locality::Local,
                validate: Locality::Summary(SummaryKind::Fold),
                reasons: Vec::new(),
            }
        }
    }
    /// A typed residual, not an invalid-input error.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Residual {
        /// The operation has a global dependency.
        Global {
            /// Operation that cannot be segmented.
            op: SegmentOp,
            /// Reason that the operation is global.
            reason: GlobalReason,
        },
        /// A deterministic resource preflight exceeded its budget.
        BudgetExceeded {
            /// Resource whose budget was exceeded.
            resource: Resource,
            /// Phase in which the resource is required.
            phase: Phase,
            /// Segment responsible for the demand, if local.
            segment: Option<SegmentId>,
            /// Required capacity.
            needed: u64,
            /// Available capacity.
            budget: u64,
        },
    }
    /// Deterministic budget.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Budget {
        /// Maximum estimated bytes for one materialized segment.
        pub working_set: u64,
        /// Optional maximum complement bytes.
        pub complement: Option<u64>,
        /// Maximum aggregate summary bytes.
        pub summary: u64,
        /// Maximum number of affected segments.
        pub affected_segments: u32,
    }
    impl Budget {
        /// A budget that cannot bind materialized reference fixtures.
        #[must_use]
        pub const fn unlimited() -> Self {
            Self {
                working_set: u64::MAX,
                complement: None,
                summary: u64::MAX,
                affected_segments: u32::MAX,
            }
        }
    }
    /// Deterministic memory estimator.
    pub struct MemoryModel;
    impl MemoryModel {
        /// Estimate the bytes required to materialize one segment as
        /// `160 * nodes + 128 * edges + 192 * fans + 96 * ghosts +
        /// 128 * in_stubs + payload_bytes`, using saturating `u64` arithmetic.
        #[must_use]
        pub const fn estimate(stats: &SegmentStats) -> u64 {
            stats
                .nodes
                .saturating_mul(160)
                .saturating_add(stats.edges.saturating_mul(128))
                .saturating_add(stats.fans.saturating_mul(192))
                .saturating_add(stats.ghosts.saturating_mul(96))
                .saturating_add(stats.in_stubs.saturating_mul(128))
                .saturating_add(stats.payload_bytes)
        }
    }

    /// One task key.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TaskKey {
        /// Phase containing the task.
        pub phase: Phase,
        /// Segment for a local task, or none for a global task.
        pub segment: Option<SegmentId>,
    }
    /// One committed task.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct TaskRecord {
        /// Digest of the task inputs.
        pub inputs: Digest,
        /// Digest of the committed output.
        pub output: Digest,
        /// Length of the committed output.
        pub bytes: u64,
    }
    /// One run journal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RunJournal {
        /// Run digest naming this journal.
        pub run: Digest,
        /// Committed task records.
        pub tasks: BTreeMap<TaskKey, TaskRecord>,
    }

    /// Canonical run-journal bytes in task-key order.
    #[must_use]
    pub fn canonical_journal_bytes(journal: &RunJournal) -> Vec<u8> {
        let mut bytes = Canonical::default();
        bytes.digest(journal.run);
        bytes.len(journal.tasks.len());
        for (task, record) in &journal.tasks {
            bytes.tag(task.phase as u8);
            bytes.bool(task.segment.is_some());
            if let Some(segment) = task.segment {
                bytes.u32(segment.0);
            }
            bytes.digest(record.inputs);
            bytes.digest(record.output);
            bytes.u64(record.bytes);
        }
        bytes.0
    }

    /// Decode canonical run-journal bytes, refusing duplicate/out-of-order
    /// task keys and trailing bytes.
    #[must_use]
    pub fn decode_journal_bytes(bytes: &[u8]) -> Option<RunJournal> {
        let mut reader = Reader::new(bytes);
        let run = read_digest(&mut reader)?;
        let mut tasks = BTreeMap::new();
        for _ in 0..reader.count()? {
            let phase = match reader.tag()? {
                0 => Phase::Validate,
                1 => Phase::Export,
                2 => Phase::Summarize,
                3 => Phase::Assemble,
                4 => Phase::Emit,
                5 => Phase::Seal,
                _ => return None,
            };
            let segment = if read_bool(&mut reader)? {
                Some(SegmentId(reader.u32()?))
            } else {
                None
            };
            let key = TaskKey { phase, segment };
            if tasks.keys().next_back().is_some_and(|old| *old >= key) {
                return None;
            }
            tasks.insert(
                key,
                TaskRecord {
                    inputs: read_digest(&mut reader)?,
                    output: read_digest(&mut reader)?,
                    bytes: reader.u64()?,
                },
            );
        }
        reader.done().then_some(RunJournal { run, tasks })
    }

    /// Digest canonical run-journal bytes under [`RUN_JOURNAL_CONTEXT`].
    #[must_use]
    pub fn journal_digest(journal: &RunJournal) -> Digest {
        Digest::derive(RUN_JOURNAL_CONTEXT, &canonical_journal_bytes(journal))
    }

    /// Digest already canonical complement-segment bytes under the reserved
    /// complement domain. Typed complement encoders feed this function.
    #[must_use]
    pub fn complement_segment_digest(bytes: &[u8]) -> Digest {
        Digest::derive(SEGMENT_COMPLEMENT_CONTEXT, bytes)
    }
    /// One artifact key.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ArtifactKey {
        /// Run containing the artifact.
        pub run: Digest,
        /// Task that produced the artifact.
        pub task: TaskKey,
        /// Artifact representation.
        pub kind: ArtifactKind,
    }
    /// Caller-supplied staged/committed storage.
    pub trait SegmentStore {
        /// Storage failure.
        type Error: Error + Send + Sync + 'static;
        /// Read a committed artifact.
        ///
        /// # Errors
        ///
        /// Returns a storage-specific read failure.
        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error>;
        /// Stage an artifact before committing its journal record.
        ///
        /// # Errors
        ///
        /// Returns a storage-specific staging failure.
        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error>;
        /// Atomically expose a staged artifact and its journal record.
        ///
        /// # Errors
        ///
        /// Returns a storage-specific commit failure.
        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error>;
        /// Read the committed journal for a run.
        ///
        /// # Errors
        ///
        /// Returns a storage-specific journal failure.
        fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error>;
        /// List artifacts staged but not committed for a run.
        ///
        /// # Errors
        ///
        /// Returns a storage-specific enumeration failure.
        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error>;
    }
    /// In-memory store with atomic stage/commit visibility.
    #[derive(Clone, Debug, Default)]
    pub struct MemoryStore {
        staged: BTreeMap<ArtifactKey, Vec<u8>>,
        committed: BTreeMap<ArtifactKey, Vec<u8>>,
        journals: BTreeMap<Digest, RunJournal>,
    }
    impl SegmentStore for MemoryStore {
        type Error = Infallible;
        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self.committed.get(key).cloned())
        }
        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
            self.staged.insert(*key, bytes);
            Ok(())
        }
        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
            if let Some(bytes) = self.staged.remove(key) {
                self.committed.insert(*key, bytes);
                self.journals
                    .entry(key.run)
                    .or_insert_with(|| RunJournal {
                        run: key.run,
                        tasks: BTreeMap::new(),
                    })
                    .tasks
                    .insert(key.task, *record);
            }
            Ok(())
        }
        fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
            Ok(self.journals.get(run).cloned().unwrap_or(RunJournal {
                run: *run,
                tasks: BTreeMap::new(),
            }))
        }
        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
            Ok(self
                .staged
                .keys()
                .filter(|key| key.run == *run)
                .copied()
                .collect())
        }
    }

    /// Progress after each task.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Progress {
        /// Most recently completed phase.
        pub phase: Phase,
        /// Number of completed tasks.
        pub tasks_done: u32,
        /// Total number of tasks.
        pub tasks_total: u32,
        /// Number of completed logical elements.
        pub elements_done: u64,
        /// Total number of logical elements.
        pub elements_total: u64,
    }
    /// Complement accounting (zero for this observation runner).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ComplementSize {
        /// Total complement elements.
        pub elements: u64,
        /// Total complement bytes.
        pub bytes: u64,
        /// Per-segment element and byte counts.
        pub per_segment: BTreeMap<SegmentId, (u64, u64)>,
    }
    /// Per-phase accounting.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PhaseReport {
        /// Accounted phase.
        pub phase: Phase,
        /// Tasks executed during this invocation.
        pub tasks: u32,
        /// Tasks reused from the committed journal.
        pub reused: u32,
        /// Peak deterministic memory estimate.
        pub peak_estimated_bytes: u64,
        /// Optional measured wall time; absent in the deterministic reference.
        pub wall_ms: Option<u64>,
    }
    /// Resumable incomplete reason.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RunIncompleteReason {
        /// The caller-requested cancellation boundary was reached.
        Cancelled,
        /// Storage failed without invalidating the input value.
        Store {
            /// Stable printable storage error.
            message: String,
        },
    }
    /// Run outcome.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RunStatus {
        /// Every task committed successfully.
        Complete {
            /// Digest of the committed manifest bytes.
            manifest: Digest,
        },
        /// The run can be resumed from its committed journal.
        Incomplete {
            /// Reason the run stopped.
            reason: RunIncompleteReason,
        },
        /// A valid input could not run within the declared contract.
        Residual(Residual),
        /// The input or segmented representation was invalid.
        Failed(SegmentError),
    }
    /// Deterministic run report.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentRunReport {
        /// Run outcome.
        pub status: RunStatus,
        /// Declared operation locality.
        pub locality: LocalityReport,
        /// Applied resource budget.
        pub budget: Budget,
        /// Deterministic progress counters.
        pub progress: Progress,
        /// Peak deterministic memory estimate.
        pub peak_estimated_bytes: u64,
        /// Optional implementation-specific measured peak.
        pub observed_peak_bytes: Option<u64>,
        /// Complement accounting.
        pub complement: ComplementSize,
        /// Tasks executed during this invocation.
        pub tasks_run: u32,
        /// Tasks reused from the committed journal.
        pub tasks_reused: u32,
        /// Staged artifacts not represented by committed records.
        pub orphans: Vec<ArtifactKey>,
        /// Per-phase accounting.
        pub phases: Vec<PhaseReport>,
    }
    /// Inputs determining a reference run.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RunSpec {
        /// Operation identity included in task keys.
        pub operation: Digest,
        /// Schema digest bound into canonical artifacts.
        pub schema: Digest,
        /// Optional endpoint binding digest.
        pub binding: Option<Digest>,
        /// Optional cancellation boundary measured in committed or reused tasks.
        pub cancel_after_tasks: Option<u32>,
    }
    /// A report and optional completed observation/manifest.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SegmentedRun<T> {
        /// Observation of the completed assembled value.
        pub value: Option<T>,
        /// Completed manifest.
        pub manifest: Option<SegmentManifest>,
        /// Deterministic execution report.
        pub report: SegmentRunReport,
    }

    fn base_report(shape: InstanceShapeKind, budget: Budget, total: u32) -> SegmentRunReport {
        SegmentRunReport {
            status: RunStatus::Incomplete {
                reason: RunIncompleteReason::Cancelled,
            },
            locality: LocalityReport::identity(shape),
            budget,
            progress: Progress {
                phase: Phase::Validate,
                tasks_done: 0,
                tasks_total: total,
                elements_done: 0,
                elements_total: u64::from(total),
            },
            peak_estimated_bytes: 0,
            observed_peak_bytes: None,
            complement: ComplementSize {
                elements: 0,
                bytes: 0,
                per_segment: BTreeMap::new(),
            },
            tasks_run: 0,
            tasks_reused: 0,
            orphans: Vec::new(),
            phases: Vec::new(),
        }
    }
    fn failed<T>(mut report: SegmentRunReport, error: SegmentError) -> SegmentedRun<T> {
        report.status = RunStatus::Failed(error);
        SegmentedRun {
            value: None,
            manifest: None,
            report,
        }
    }
    fn incomplete<T, S: SegmentStore>(
        mut report: SegmentRunReport,
        store: &S,
        run: Digest,
        reason: RunIncompleteReason,
    ) -> SegmentedRun<T> {
        match store.staged_uncommitted(&run) {
            Ok(orphans) => {
                report.orphans = orphans;
                report.status = RunStatus::Incomplete { reason };
            }
            Err(error) => {
                let message = match reason {
                    RunIncompleteReason::Store { message } => {
                        format!("{message}; orphan scan failed: {error}")
                    }
                    RunIncompleteReason::Cancelled => {
                        format!("orphan scan failed after cancellation: {error}")
                    }
                };
                report.status = RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store { message },
                };
            }
        }
        SegmentedRun {
            value: None,
            manifest: None,
            report,
        }
    }
    fn task_input(
        operation: Digest,
        phase: Phase,
        segment: Option<SegmentId>,
        inputs: &[u8],
    ) -> Digest {
        let mut w = Canonical::default();
        w.digest(operation);
        w.tag(phase as u8);
        w.bool(segment.is_some());
        if let Some(segment) = segment {
            w.u32(segment.0);
        }
        w.bytes(inputs);
        Digest::derive(SEGMENT_TASK_CONTEXT, &w.0)
    }
    #[derive(Clone, Copy)]
    enum TaskOutcome {
        Ran,
        Reused,
    }

    struct TaskRequest<'a> {
        run: Digest,
        operation: Digest,
        phase: Phase,
        segment: Option<SegmentId>,
        kind: ArtifactKind,
        inputs: &'a [u8],
    }

    struct TaskExecution {
        bytes: Vec<u8>,
        outcome: TaskOutcome,
    }

    enum TaskFailure {
        Store(String),
        Segment(SegmentError),
    }

    fn execute_task<S, F>(
        store: &mut S,
        request: &TaskRequest<'_>,
        compute: F,
    ) -> Result<TaskExecution, TaskFailure>
    where
        S: SegmentStore,
        F: FnOnce() -> Result<Vec<u8>, SegmentError>,
    {
        let task = TaskKey {
            phase: request.phase,
            segment: request.segment,
        };
        let key = ArtifactKey {
            run: request.run,
            task,
            kind: request.kind,
        };
        let inputs = task_input(
            request.operation,
            request.phase,
            request.segment,
            request.inputs,
        );
        let journal = store
            .journal(&request.run)
            .map_err(|error| TaskFailure::Store(error.to_string()))?;
        if let Some(record) = journal.tasks.get(&task)
            && record.inputs == inputs
        {
            let stored = store
                .read(&key)
                .map_err(|error| TaskFailure::Store(error.to_string()))?;
            if let Some(bytes) = stored
                && u64::try_from(bytes.len()).unwrap_or(u64::MAX) == record.bytes
                && Digest::derive(SEGMENT_TASK_CONTEXT, &bytes) == record.output
            {
                return Ok(TaskExecution {
                    bytes,
                    outcome: TaskOutcome::Reused,
                });
            }
        }
        let output = compute().map_err(TaskFailure::Segment)?;
        let output_digest = Digest::derive(SEGMENT_TASK_CONTEXT, &output);
        store
            .stage(&key, output.clone())
            .map_err(|error| TaskFailure::Store(error.to_string()))?;
        let record = TaskRecord {
            inputs,
            output: output_digest,
            bytes: u64::try_from(output.len()).unwrap_or(u64::MAX),
        };
        store
            .commit(&key, &record)
            .map_err(|error| TaskFailure::Store(error.to_string()))?;
        Ok(TaskExecution {
            bytes: output,
            outcome: TaskOutcome::Ran,
        })
    }
    fn run_digest(
        spec: RunSpec,
        p: &Partition,
        segments: &[&AnySegment],
    ) -> Result<Digest, SegmentError> {
        let mut w = Canonical::default();
        w.digest(spec.operation);
        w.digest(spec.schema);
        w.bool(spec.binding.is_some());
        if let Some(binding) = spec.binding {
            w.digest(binding);
        }
        w.tag(shape_tag(p.shape));
        w.len(p.specs.len());
        for item in &p.specs {
            write_spec(&mut w, item);
        }
        w.len(segments.len());
        for segment in segments {
            w.bytes(&canonical_segment_bytes(segment, spec.schema, p)?);
        }
        Ok(Digest::derive(RUN_JOURNAL_CONTEXT, &w.0))
    }
    fn summary_bytes(summary: &SegmentSummary) -> Vec<u8> {
        let mut w = Canonical::default();
        write_stats(&mut w, summary.stats);
        write_opt_u32(&mut w, summary.max_node);
        write_opt_u32(&mut w, summary.max_edge);
        w.len(summary.range_max.len());
        for (segment, maximum) in &summary.range_max {
            w.u32(segment.0);
            w.u32(*maximum);
        }
        w.0
    }

    fn decode_summary_bytes(bytes: &[u8]) -> Option<SegmentSummary> {
        let mut reader = Reader::new(bytes);
        let stats = read_stats(&mut reader)?;
        let max_node = read_opt_u32(&mut reader).ok()?;
        let max_edge = read_opt_u32(&mut reader).ok()?;
        let mut range_max = BTreeMap::new();
        for _ in 0..reader.count()? {
            let segment = SegmentId(reader.u32()?);
            if range_max
                .keys()
                .next_back()
                .is_some_and(|old| *old >= segment)
            {
                return None;
            }
            range_max.insert(segment, reader.u32()?);
        }
        reader.done().then_some(SegmentSummary {
            stats,
            max_node,
            max_edge,
            range_max,
        })
    }

    fn account_task(
        report: &mut SegmentRunReport,
        outcome: TaskOutcome,
        phase: Phase,
        progress: &mut dyn FnMut(&Progress),
    ) {
        match outcome {
            TaskOutcome::Ran => report.tasks_run += 1,
            TaskOutcome::Reused => report.tasks_reused += 1,
        }
        report.progress.tasks_done = report.progress.tasks_done.saturating_add(1);
        report.progress.elements_done = report.progress.elements_done.saturating_add(1);
        report.progress.phase = phase;
        progress(&report.progress);
    }

    fn run_task<S, F>(
        report: &mut SegmentRunReport,
        store: &mut S,
        request: &TaskRequest<'_>,
        progress: &mut dyn FnMut(&Progress),
        compute: F,
    ) -> Result<Vec<u8>, TaskFailure>
    where
        S: SegmentStore,
        F: FnOnce() -> Result<Vec<u8>, SegmentError>,
    {
        let execution = execute_task(store, request, compute)?;
        account_task(report, execution.outcome, request.phase, progress);
        Ok(execution.bytes)
    }

    fn finish_phase(
        report: &mut SegmentRunReport,
        phase: Phase,
        run_before: u32,
        reuse_before: u32,
        peak: u64,
    ) {
        report.phases.push(PhaseReport {
            phase,
            tasks: report.tasks_run - run_before,
            reused: report.tasks_reused - reuse_before,
            peak_estimated_bytes: peak,
            wall_ms: None,
        });
    }

    enum PipelineStop {
        Cancelled,
        Store(String),
        Failed(SegmentError),
        Residual(Residual),
    }

    struct PreparedRun {
        segment_inputs: Vec<Vec<u8>>,
        summaries: Vec<SegmentSummary>,
        peak: u64,
    }

    fn prepare_run(
        ordered: &[&AnySegment],
        p: &Partition,
        spec: RunSpec,
        budget: Budget,
    ) -> Result<PreparedRun, PipelineStop> {
        let affected = u64::try_from(ordered.len()).unwrap_or(u64::MAX);
        if affected > u64::from(budget.affected_segments) {
            return Err(PipelineStop::Residual(Residual::BudgetExceeded {
                resource: Resource::AffectedSegments,
                phase: Phase::Validate,
                segment: None,
                needed: affected,
                budget: u64::from(budget.affected_segments),
            }));
        }
        let mut segment_inputs = Vec::with_capacity(ordered.len());
        let mut summaries = Vec::with_capacity(ordered.len());
        let mut peak = 0;
        for segment in ordered {
            let bytes = segment
                .canonical_bytes(&spec.schema, p)
                .map_err(PipelineStop::Failed)?;
            let summary = segment_summary(segment, p, spec.schema).map_err(PipelineStop::Failed)?;
            let estimate = MemoryModel::estimate(&summary.stats);
            peak = peak.max(estimate);
            if estimate > budget.working_set {
                return Err(PipelineStop::Residual(Residual::BudgetExceeded {
                    resource: Resource::WorkingSet,
                    phase: Phase::Validate,
                    segment: Some(segment.segment()),
                    needed: estimate,
                    budget: budget.working_set,
                }));
            }
            segment_inputs.push(bytes);
            summaries.push(summary);
        }
        let needed = summaries.iter().fold(0_u64, |total, value| {
            total.saturating_add(u64::try_from(summary_bytes(value).len()).unwrap_or(u64::MAX))
        });
        if needed > budget.summary {
            return Err(PipelineStop::Residual(Residual::BudgetExceeded {
                resource: Resource::Summary,
                phase: Phase::Assemble,
                segment: None,
                needed,
                budget: budget.summary,
            }));
        }
        Ok(PreparedRun {
            segment_inputs,
            summaries,
            peak,
        })
    }

    struct RunnerState<'a, 'b, S> {
        p: &'a Partition,
        spec: RunSpec,
        budget: Budget,
        store: &'b mut S,
        progress: &'b mut dyn FnMut(&Progress),
        run: Digest,
        peak: u64,
        segment_inputs: Vec<Vec<u8>>,
        preflight_summaries: Vec<SegmentSummary>,
        report: SegmentRunReport,
    }

    impl<S: SegmentStore> RunnerState<'_, '_, S> {
        fn cancelled(&self) -> bool {
            self.spec
                .cancel_after_tasks
                .is_some_and(|limit| self.report.progress.tasks_done >= limit)
        }

        fn task<F>(
            &mut self,
            phase: Phase,
            segment: Option<SegmentId>,
            kind: ArtifactKind,
            inputs: &[u8],
            compute: F,
        ) -> Result<Vec<u8>, PipelineStop>
        where
            F: FnOnce() -> Result<Vec<u8>, SegmentError>,
        {
            let request = TaskRequest {
                run: self.run,
                operation: self.spec.operation,
                phase,
                segment,
                kind,
                inputs,
            };
            run_task(
                &mut self.report,
                self.store,
                &request,
                self.progress,
                compute,
            )
            .map_err(|failure| match failure {
                TaskFailure::Store(message) => PipelineStop::Store(message),
                TaskFailure::Segment(error) => PipelineStop::Failed(error),
            })
        }

        fn local_phases(
            &mut self,
            ordered: &[&AnySegment],
        ) -> Result<Vec<SegmentSummary>, PipelineStop> {
            let mut summaries = Vec::with_capacity(ordered.len());
            for phase in [Phase::Validate, Phase::Export, Phase::Summarize] {
                let (run_before, reuse_before) = (self.report.tasks_run, self.report.tasks_reused);
                for (position, segment) in ordered.iter().enumerate() {
                    if self.cancelled() {
                        return Err(PipelineStop::Cancelled);
                    }
                    let input = self.segment_inputs[position].clone();
                    let expected_summary = self.preflight_summaries[position].clone();
                    let kind = match phase {
                        Phase::Validate => ArtifactKind::Segment,
                        Phase::Export => ArtifactKind::Export,
                        Phase::Summarize => ArtifactKind::Summary,
                        _ => unreachable!("per-segment phase"),
                    };
                    let output =
                        self.task(
                            phase,
                            Some(segment.segment()),
                            kind,
                            &input,
                            || match phase {
                                Phase::Validate => Ok(input.clone()),
                                Phase::Export => {
                                    let mut bytes = Canonical::default();
                                    write_boundary(&mut bytes, segment.boundary());
                                    Ok(bytes.0)
                                }
                                Phase::Summarize => Ok(summary_bytes(&expected_summary)),
                                _ => unreachable!("per-segment phase"),
                            },
                        )?;
                    match phase {
                        Phase::Validate => self.check_validate_output(&output, segment)?,
                        Phase::Export => check_export_output(&output)?,
                        Phase::Summarize => {
                            let summary = decode_summary_bytes(&output).ok_or_else(|| {
                                PipelineStop::Failed(decode_error(
                                    ArtifactKind::Summary,
                                    "malformed summary",
                                ))
                            })?;
                            if summary != expected_summary {
                                return Err(PipelineStop::Failed(decode_error(
                                    ArtifactKind::Summary,
                                    "summary changed its input",
                                )));
                            }
                            summaries.push(summary);
                        }
                        _ => unreachable!("per-segment phase"),
                    }
                }
                finish_phase(&mut self.report, phase, run_before, reuse_before, self.peak);
            }
            Ok(summaries)
        }

        fn check_validate_output(
            &self,
            output: &[u8],
            expected: &AnySegment,
        ) -> Result<(), PipelineStop> {
            match AnySegment::from_canonical_bytes(output, self.p) {
                Ok((value, schema)) if value == *expected && schema == self.spec.schema => Ok(()),
                Ok(_) => Err(PipelineStop::Failed(decode_error(
                    ArtifactKind::Segment,
                    "validated artifact changed its input",
                ))),
                Err(error) => Err(PipelineStop::Failed(error)),
            }
        }

        fn assemble(
            &mut self,
            whole: &AnyInstance,
            summaries: &[SegmentSummary],
            operation: &dyn Fn(&AnyInstance) -> Result<SegmentOperationResult, SegmentError>,
        ) -> Result<(Vec<u8>, SegmentOperationResult), PipelineStop> {
            if self.cancelled() {
                return Err(PipelineStop::Cancelled);
            }
            let mut inputs = Canonical::default();
            for summary in summaries {
                inputs.bytes(&summary_bytes(summary));
            }
            let (run_before, reuse_before) = (self.report.tasks_run, self.report.tasks_reused);
            let p = self.p;
            let bytes = self.task(
                Phase::Assemble,
                None,
                ArtifactKind::Assembly,
                &inputs.0,
                || {
                    let result = operation(whole)?;
                    validate_operation_result(&result, p)?;
                    operation_result_bytes(&result)
                },
            )?;
            finish_phase(
                &mut self.report,
                Phase::Assemble,
                run_before,
                reuse_before,
                self.peak,
            );
            let result = decode_operation_result(&bytes).map_err(PipelineStop::Failed)?;
            self.check_operation_result(&result)?;
            Ok((bytes, result))
        }

        fn check_operation_result(
            &self,
            result: &SegmentOperationResult,
        ) -> Result<(), PipelineStop> {
            validate_operation_result(result, self.p)
                .map(|_| ())
                .map_err(PipelineStop::Failed)
        }

        fn emit(
            &mut self,
            result: &SegmentOperationResult,
            assembly_bytes: &[u8],
        ) -> Result<Vec<AnySegment>, PipelineStop> {
            let cache = OnceCell::new();
            let mut emitted = Vec::with_capacity(self.p.specs.len());
            let (run_before, reuse_before) = (self.report.tasks_run, self.report.tasks_reused);
            let specs = self.p.specs.clone();
            for spec_item in &specs {
                if self.cancelled() {
                    return Err(PipelineStop::Cancelled);
                }
                let complement = result.complements.get(&spec_item.id);
                let (elements, bytes) = complement_size(complement)?;
                let needed = self.report.complement.bytes.saturating_add(bytes);
                if self.budget.complement.is_some_and(|limit| needed > limit) {
                    return Err(PipelineStop::Residual(Residual::BudgetExceeded {
                        resource: Resource::Complement,
                        phase: Phase::Emit,
                        segment: Some(spec_item.id),
                        needed,
                        budget: self.budget.complement.unwrap_or(u64::MAX),
                    }));
                }
                let kind = if complement.is_some() {
                    ArtifactKind::ComplementSegment
                } else {
                    ArtifactKind::Segment
                };
                let p = self.p;
                let schema = self.spec.schema;
                let output = self.task(
                    Phase::Emit,
                    Some(spec_item.id),
                    kind,
                    assembly_bytes,
                    || {
                        let all = cache.get_or_init(|| partition(&result.instance, p));
                        let all = all.as_ref().map_err(Clone::clone)?;
                        let segment = all.get(index(spec_item.id)).ok_or_else(|| {
                            decode_error(ArtifactKind::Segment, "missing emitted segment")
                        })?;
                        if let Some(complement) = complement {
                            validate_complement_ownership(
                                complement,
                                segment,
                                &result.instance,
                                p,
                            )?;
                        }
                        emit_bundle_bytes(segment, complement, schema, p)
                    },
                )?;
                let (segment, stored_complement) =
                    decode_emit_bundle(&output, self.spec.schema, self.p)
                        .map_err(PipelineStop::Failed)?;
                if segment.segment() != spec_item.id || stored_complement.as_ref() != complement {
                    return Err(PipelineStop::Failed(decode_error(
                        ArtifactKind::Segment,
                        "emit artifact changed its input",
                    )));
                }
                if let Some(complement) = &stored_complement {
                    validate_complement_ownership(complement, &segment, &result.instance, self.p)
                        .map_err(PipelineStop::Failed)?;
                }
                self.account_complement(spec_item.id, complement, elements, bytes, needed);
                emitted.push(segment);
            }
            finish_phase(
                &mut self.report,
                Phase::Emit,
                run_before,
                reuse_before,
                self.peak,
            );
            Ok(emitted)
        }

        fn account_complement(
            &mut self,
            segment: SegmentId,
            complement: Option<&AnyComplementSegment>,
            elements: u64,
            bytes: u64,
            total_bytes: u64,
        ) {
            self.report.complement.elements =
                self.report.complement.elements.saturating_add(elements);
            self.report.complement.bytes = total_bytes;
            if complement.is_some() {
                self.report
                    .complement
                    .per_segment
                    .insert(segment, (elements, bytes));
            }
        }

        fn seal(
            &mut self,
            emitted: &[AnySegment],
        ) -> Result<(Vec<u8>, SegmentManifest), PipelineStop> {
            if self.cancelled() {
                return Err(PipelineStop::Cancelled);
            }
            let mut inputs = Canonical::default();
            for segment in emitted {
                let bytes = segment
                    .canonical_bytes(&self.spec.schema, self.p)
                    .map_err(PipelineStop::Failed)?;
                inputs.bytes(&bytes);
            }
            let (run_before, reuse_before) = (self.report.tasks_run, self.report.tasks_reused);
            let p = self.p;
            let schema = self.spec.schema;
            let binding = self.spec.binding;
            let bytes = self.task(Phase::Seal, None, ArtifactKind::Manifest, &inputs.0, || {
                let manifest = seal_manifest(emitted, p, schema, ManifestRole::Source, binding)?;
                Ok(canonical_manifest_bytes(&manifest))
            })?;
            finish_phase(
                &mut self.report,
                Phase::Seal,
                run_before,
                reuse_before,
                self.peak,
            );
            let manifest = decode_manifest_bytes(&bytes).map_err(PipelineStop::Failed)?;
            Ok((bytes, manifest))
        }
    }

    fn check_export_output(bytes: &[u8]) -> Result<(), PipelineStop> {
        let mut reader = Reader::new(bytes);
        if read_boundary(&mut reader).is_some() && reader.done() {
            Ok(())
        } else {
            Err(PipelineStop::Failed(decode_error(
                ArtifactKind::Export,
                "malformed boundary export",
            )))
        }
    }

    fn complement_size(
        complement: Option<&AnyComplementSegment>,
    ) -> Result<(u64, u64), PipelineStop> {
        let elements = complement.map_or(0, AnyComplementSegment::element_count);
        let bytes = match complement {
            Some(value) => value
                .canonical_bytes()
                .map_err(PipelineStop::Failed)
                .map(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX))?,
            None => 0,
        };
        Ok((elements, bytes))
    }

    fn stop_run<T, S: SegmentStore>(
        mut state: RunnerState<'_, '_, S>,
        stop: PipelineStop,
    ) -> SegmentedRun<T> {
        state.report.peak_estimated_bytes = state.peak;
        match stop {
            PipelineStop::Cancelled => incomplete(
                state.report,
                state.store,
                state.run,
                RunIncompleteReason::Cancelled,
            ),
            PipelineStop::Store(message) => incomplete(
                state.report,
                state.store,
                state.run,
                RunIncompleteReason::Store { message },
            ),
            PipelineStop::Failed(error) => failed(state.report, error),
            PipelineStop::Residual(residual) => {
                state.report.status = RunStatus::Residual(residual);
                SegmentedRun {
                    value: None,
                    manifest: None,
                    report: state.report,
                }
            }
        }
    }

    /// Run a whole-state reference operation through the six deterministic
    /// segmented phases.
    ///
    /// A valid committed artifact is decoded and consumed without invoking
    /// its phase computation. `progress` is called after every committed or
    /// reused task.
    pub fn segmented_operation_with_progress<S: SegmentStore>(
        segments: &[AnySegment],
        p: &Partition,
        spec: RunSpec,
        budget: Budget,
        store: &mut S,
        operation: &dyn Fn(&AnyInstance) -> Result<SegmentOperationResult, SegmentError>,
        progress: &mut dyn FnMut(&Progress),
    ) -> SegmentedRun<AnyInstance> {
        let per = u32::try_from(segments.len()).unwrap_or(u32::MAX);
        let total = per.saturating_mul(4).saturating_add(2);
        let report = base_report(p.shape, budget, total);
        let ordered = match arrange(segments, p) {
            Ok(value) => value,
            Err(error) => return failed(report, error),
        };
        let whole = match glue(segments, p) {
            Ok(value) => value,
            Err(error) => return failed(report, error),
        };
        let run = match run_digest(spec, p, &ordered) {
            Ok(value) => value,
            Err(error) => return failed(report, error),
        };
        let prepared = match prepare_run(&ordered, p, spec, budget) {
            Ok(value) => value,
            Err(stop) => {
                let state = RunnerState {
                    p,
                    spec,
                    budget,
                    store,
                    progress,
                    run,
                    peak: 0,
                    segment_inputs: Vec::new(),
                    preflight_summaries: Vec::new(),
                    report,
                };
                return stop_run(state, stop);
            }
        };
        let mut state = RunnerState {
            p,
            spec,
            budget,
            store,
            progress,
            run,
            peak: prepared.peak,
            segment_inputs: prepared.segment_inputs,
            preflight_summaries: prepared.summaries,
            report,
        };
        let summaries = match state.local_phases(&ordered) {
            Ok(value) => value,
            Err(stop) => return stop_run(state, stop),
        };
        let (assembly_bytes, result) = match state.assemble(&whole, &summaries, operation) {
            Ok(value) => value,
            Err(stop) => return stop_run(state, stop),
        };

        let emitted = match state.emit(&result, &assembly_bytes) {
            Ok(value) => value,
            Err(stop) => return stop_run(state, stop),
        };
        let (manifest_bytes, manifest) = match state.seal(&emitted) {
            Ok(value) => value,
            Err(stop) => return stop_run(state, stop),
        };
        state.report.peak_estimated_bytes = state.peak;
        match state.store.staged_uncommitted(&state.run) {
            Ok(orphans) => state.report.orphans = orphans,
            Err(error) => {
                state.report.status = RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store {
                        message: error.to_string(),
                    },
                };
                return SegmentedRun {
                    value: None,
                    manifest: None,
                    report: state.report,
                };
            }
        }
        state.report.status = RunStatus::Complete {
            manifest: Digest::derive(MANIFEST_CONTEXT, &manifest_bytes),
        };
        SegmentedRun {
            value: Some(result.instance),
            manifest: Some(manifest),
            report: state.report,
        }
    }

    /// Run validate/export/summarize/assemble/emit/seal, with deterministic
    /// artifacts and restart reuse, then observe the assembled whole value.
    pub fn segmented_with<T, S: SegmentStore>(
        segments: &[AnySegment],
        p: &Partition,
        spec: RunSpec,
        budget: Budget,
        store: &mut S,
        observation: &dyn Fn(&AnyInstance) -> T,
    ) -> SegmentedRun<T> {
        let mut ignore_progress = |_: &Progress| {};
        let run = segmented_operation_with_progress(
            segments,
            p,
            spec,
            budget,
            store,
            &|instance| Ok(SegmentOperationResult::identity(instance)),
            &mut ignore_progress,
        );
        SegmentedRun {
            value: run.value.as_ref().map(observation),
            manifest: run.manifest,
            report: run.report,
        }
    }

    /// Run the complete pipeline in memory with unlimited budgets.
    ///
    /// # Errors
    ///
    /// Returns a segment failure or a decode error if the unlimited in-memory
    /// run does not complete.
    pub fn segmented<T>(
        segments: &[AnySegment],
        p: &Partition,
        observation: &dyn Fn(&AnyInstance) -> T,
    ) -> Result<T, SegmentError> {
        let mut store = MemoryStore::default();
        let result = segmented_with(
            segments,
            p,
            RunSpec {
                operation: Digest::derive(SEGMENT_TASK_CONTEXT, b"reference observation"),
                schema: Digest::default(),
                binding: None,
                cancel_after_tasks: None,
            },
            Budget::unlimited(),
            &mut store,
            observation,
        );
        match (result.value, result.report.status) {
            (Some(value), RunStatus::Complete { .. }) => Ok(value),
            (_, RunStatus::Failed(error)) => Err(error),
            (_, status) => Err(decode_error(
                ArtifactKind::Manifest,
                &format!("unlimited run did not complete: {status:?}"),
            )),
        }
    }
}

pub use self::segment::{fresh, glue, partition, segmented};
