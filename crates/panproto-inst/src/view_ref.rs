//! References to view elements and the keys of synthesized elements, shared by
//! the tree, table and graph shapes.
//!
//! One vocabulary serves all three shapes, so that an error, a synthesized
//! object or a law report produced for a tree or a table corresponds to the
//! one produced for its graph form:
//!
//! - [`ViewRef`] names one element of a view: a node or an edge of a graph, a
//!   node or an arc of a tree, a row or a foreign-key pair of a table.
//! - [`SynthKey`] names an element that `get` creates without a source
//!   counterpart, by the source elements it was built from.
//! - [`IdFloor`] is the least id fresh allocation may hand out for a graph.
//! - [`SynthIndex`] records which elements of a view were synthesized and
//!   under which key.
//! - [`RowId`] identifies a row of a table instance, uniquely across all of
//!   its tables.
//!
//! The shape of an instance itself is named by
//! [`InstanceShapeKind`](crate::InstanceShapeKind), which `panproto-schema`
//! defines and this crate re-exports, so both paths name one type:
//!
//! ```
//! use panproto_inst::InstanceShapeKind;
//! use panproto_schema::InstanceShapeKind as S;
//!
//! let kind: S = InstanceShapeKind::Graph;
//! assert_eq!(kind, S::Graph);
//! assert_eq!(serde_json::to_string(&kind).unwrap(), r#""graph""#);
//! ```

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::ginstance::EdgeId;

/// Identifier of a row in a table instance.
///
/// A row id is unique across the whole instance, not per table, so a row can
/// be named without naming its table. It has the same derives and the same
/// transparent serde form as [`EdgeId`]: a bare `u32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RowId(pub u32);

/// A reference to one element of a view, in any of the three shapes.
///
/// Trees use [`ViewRef::Node`] and [`ViewRef::Arc`]; graphs use
/// [`ViewRef::Node`] and [`ViewRef::Edge`]; tables use [`ViewRef::Row`] and
/// [`ViewRef::Pair`]. Serialized externally tagged in `snake_case`, for
/// instance `{"node": 3}` or `{"arc": {"parent": 0, "child": 3}}`. The
/// derived order compares the variant first, in declaration order, and then
/// the payload.
///
/// ```
/// use panproto_inst::{EdgeId, ViewRef};
///
/// assert!(ViewRef::Node(u32::MAX) < ViewRef::Edge(EdgeId(0)));
/// let json = serde_json::to_string(&ViewRef::Arc { parent: 0, child: 3 }).unwrap();
/// assert_eq!(json, r#"{"arc":{"parent":0,"child":3}}"#);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewRef {
    /// A node of a tree or a graph, by node id.
    Node(u32),
    /// An edge of a graph, by edge id.
    Edge(EdgeId),
    /// An arc of a tree, by its parent and child node ids.
    Arc {
        /// The parent node id.
        parent: u32,
        /// The child node id.
        child: u32,
    },
    /// A row of a table.
    Row(RowId),
    /// A foreign-key pair of a table instance: the pair `(src, tgt)` of the
    /// schema edge `edge`.
    Pair {
        /// The schema edge the foreign key instantiates.
        edge: Edge,
        /// The referencing row.
        src: RowId,
        /// The referenced row.
        tgt: RowId,
    },
}

/// The key of an element that `get` synthesizes without a source counterpart.
///
/// A synthesized key is a Skolem term: an injective, deterministic name
/// built from the source elements the object comes from (Bonifati et al.
/// 2024, stated; Hidaka et al. 2010, stated, for trace ids). The id a view
/// gives a synthesized object is a presentation of its key, never its
/// identity.
///
/// For an expansion with `n` intermediate anchors, the node key with hop `h`
/// (`0 <= h < n`) names the `h`-th intermediate node, and the edge key with
/// hop `h` (`0 <= h <= n`) names the edge entering intermediate `h`, with
/// `h = n` the edge into the final endpoint.
///
/// Serialized externally tagged in `snake_case`. The derived order compares
/// the variant first, in declaration order, and then the fields in order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthKey {
    /// A contracted pair of source nodes resolved to the schema edge `edge`.
    Contract {
        /// The source node the contracted path starts at.
        src: u32,
        /// The source node the contracted path ends at.
        tgt: u32,
        /// The view schema edge the pair resolves to.
        edge: Edge,
    },
    /// The `hop`-th element of the expansion of a surviving source edge.
    Expand {
        /// The source edge being expanded.
        source_edge: EdgeId,
        /// The position along the expansion path.
        hop: u16,
    },
    /// The `hop`-th element of the expansion of a contracted pair whose
    /// resolution is an expansion path.
    ContractExpand {
        /// The source node the contracted path starts at.
        src: u32,
        /// The source node the contracted path ends at.
        tgt: u32,
        /// The position along the expansion path.
        hop: u16,
    },
    /// An edge synthesized for one resolved reference occurrence.
    ResolutionReference {
        /// Source holder of the occurrence.
        holder: ViewRef,
        /// Reference declaration.
        reference: Name,
        /// Occurrence index at the holder.
        index: u32,
    },
    /// A node synthesized for one standoff span.
    ResolutionSpan {
        /// Text carrier indexed by the span.
        text: ViewRef,
        /// Inclusive start byte offset.
        start: u64,
        /// Exclusive end byte offset.
        end: u64,
    },
}

/// The allocation floor of a graph: the least node id and the least edge id
/// that fresh or synthesized allocation may use.
///
/// Each component is the maximum id over the whole source at `get` time,
/// dropped objects included, plus one, and 0 for an empty source. A floor
/// governs allocation only; it is never an admission test for ids a view
/// carries. Tables and trees use a bare `u32` floor (rows, nodes).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct IdFloor {
    /// The node id floor.
    pub node: u32,
    /// The edge id floor.
    pub edge: u32,
}

/// Which elements of a view were synthesized, and under which key.
///
/// A bare view does not record this; it lives in the complement or in the
/// edit lens state that produced the view, which build a `SynthIndex` for
/// every law that compares views. Graphs fill both maps; trees fill `nodes`
/// and key their arcs in `edges` by the graph edge ids of their embedding;
/// tables fill `edges` with the embedded edge ids of synthesized
/// foreign-key pairs.
///
/// Serialized as a struct with named fields; both maps have integer keys, so
/// they serialize as maps in every format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SynthIndex {
    /// Synthesized nodes, by node id.
    pub nodes: BTreeMap<u32, SynthKey>,
    /// Synthesized edges, by edge id.
    pub edges: BTreeMap<EdgeId, SynthKey>,
}

impl SynthIndex {
    /// Whether no element is recorded as synthesized.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Debug;
    use std::hash::Hash;

    use panproto_schema::Edge;
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use super::{IdFloor, RowId, SynthIndex, SynthKey, ViewRef};
    use crate::ginstance::EdgeId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn edge() -> Edge {
        Edge {
            src: "post".into(),
            tgt: "author".into(),
            kind: "prop".into(),
            name: Some("by".into()),
        }
    }

    /// Round-trip through JSON, JSON as a dynamic value (the shape
    /// `pythonize` produces), `MessagePack` and CBOR.
    fn round_trip<T>(value: &T) -> TestResult
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let json = serde_json::to_string(value)?;
        assert_eq!(&serde_json::from_str::<T>(&json)?, value, "json");
        let dynamic = serde_json::to_value(value)?;
        assert_eq!(&serde_json::from_value::<T>(dynamic)?, value, "value");
        let named = rmp_serde::to_vec_named(value)?;
        assert_eq!(&rmp_serde::from_slice::<T>(&named)?, value, "msgpack");
        let compact = rmp_serde::to_vec(value)?;
        assert_eq!(&rmp_serde::from_slice::<T>(&compact)?, value, "msgpack");
        let mut cbor = Vec::new();
        ciborium::into_writer(value, &mut cbor)?;
        assert_eq!(
            &ciborium::from_reader::<T, _>(cbor.as_slice())?,
            value,
            "cbor"
        );
        Ok(())
    }

    fn all_view_refs() -> Vec<ViewRef> {
        vec![
            ViewRef::Node(4),
            ViewRef::Edge(EdgeId(2)),
            ViewRef::Arc {
                parent: 0,
                child: 1,
            },
            ViewRef::Row(RowId(9)),
            ViewRef::Pair {
                edge: edge(),
                src: RowId(1),
                tgt: RowId(2),
            },
        ]
    }

    #[test]
    fn view_ref_variants_order_by_declaration() {
        let refs = vec![
            ViewRef::Node(u32::MAX),
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Arc {
                parent: 0,
                child: 0,
            },
            ViewRef::Row(RowId(0)),
            ViewRef::Pair {
                edge: edge(),
                src: RowId(0),
                tgt: RowId(0),
            },
        ];
        assert!(refs.windows(2).all(|w| w[0] < w[1]), "{refs:?}");
        assert!(ViewRef::Node(1) < ViewRef::Node(2));
        assert!(
            ViewRef::Arc {
                parent: 1,
                child: 9
            } < ViewRef::Arc {
                parent: 2,
                child: 0
            }
        );
    }

    #[test]
    fn view_ref_serde_is_externally_tagged_snake_case() -> TestResult {
        let json: Vec<String> = all_view_refs()
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        assert_eq!(
            json,
            [
                r#"{"node":4}"#,
                r#"{"edge":2}"#,
                r#"{"arc":{"parent":0,"child":1}}"#,
                r#"{"row":9}"#,
                r#"{"pair":{"edge":{"src":"post","tgt":"author","kind":"prop","name":"by"},"src":1,"tgt":2}}"#,
            ]
        );
        for r in all_view_refs() {
            round_trip(&r)?;
        }
        Ok(())
    }

    #[test]
    fn synth_key_variants_and_serde_are_fixed() -> TestResult {
        let keys = vec![
            SynthKey::Contract {
                src: u32::MAX,
                tgt: u32::MAX,
                edge: edge(),
            },
            SynthKey::Expand {
                source_edge: EdgeId(0),
                hop: 0,
            },
            SynthKey::ContractExpand {
                src: 0,
                tgt: 0,
                hop: 0,
            },
            SynthKey::ResolutionReference {
                holder: ViewRef::Node(3),
                reference: "alias".into(),
                index: 2,
            },
            SynthKey::ResolutionSpan {
                text: ViewRef::Row(RowId(4)),
                start: 5,
                end: 8,
            },
        ];
        assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
        let json: Vec<String> = keys
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        assert_eq!(
            json,
            [
                format!(
                    r#"{{"contract":{{"src":{m},"tgt":{m},"edge":{{"src":"post","tgt":"author","kind":"prop","name":"by"}}}}}}"#,
                    m = u32::MAX
                ),
                r#"{"expand":{"source_edge":0,"hop":0}}"#.to_owned(),
                r#"{"contract_expand":{"src":0,"tgt":0,"hop":0}}"#.to_owned(),
                r#"{"resolution_reference":{"holder":{"node":3},"reference":"alias","index":2}}"#
                    .to_owned(),
                r#"{"resolution_span":{"text":{"row":4},"start":5,"end":8}}"#.to_owned(),
            ]
        );
        for k in &keys {
            round_trip(k)?;
        }
        Ok(())
    }

    #[test]
    fn id_floor_serde_round_trips() -> TestResult {
        let floor = IdFloor { node: 7, edge: 3 };
        assert_eq!(serde_json::to_string(&floor)?, r#"{"node":7,"edge":3}"#);
        round_trip(&floor)?;
        assert_eq!(IdFloor::default(), IdFloor { node: 0, edge: 0 });
        Ok(())
    }

    #[test]
    fn synth_index_serde_round_trips() -> TestResult {
        let index = SynthIndex {
            nodes: BTreeMap::from([(
                12,
                SynthKey::Expand {
                    source_edge: EdgeId(4),
                    hop: 0,
                },
            )]),
            edges: BTreeMap::from([
                (
                    EdgeId(20),
                    SynthKey::Expand {
                        source_edge: EdgeId(4),
                        hop: 1,
                    },
                ),
                (
                    EdgeId(21),
                    SynthKey::Contract {
                        src: 1,
                        tgt: 5,
                        edge: edge(),
                    },
                ),
            ]),
        };
        let json = serde_json::to_string(&index)?;
        assert!(json.starts_with(r#"{"nodes":{"12":{"expand":"#), "{json}");
        round_trip(&index)?;
        round_trip(&SynthIndex::default())?;
        assert!(SynthIndex::default().is_empty());
        assert!(!index.is_empty());
        Ok(())
    }

    fn assert_edge_id_derives<T>()
    where
        T: Clone
            + Copy
            + Debug
            + PartialEq
            + Eq
            + PartialOrd
            + Ord
            + Hash
            + Serialize
            + for<'de> Deserialize<'de>,
    {
    }

    #[test]
    fn row_id_has_the_derives_of_edge_id() -> TestResult {
        assert_edge_id_derives::<EdgeId>();
        assert_edge_id_derives::<RowId>();
        // Both are transparent: a bare integer on the wire.
        assert_eq!(serde_json::to_string(&RowId(5))?, "5");
        assert_eq!(
            serde_json::to_string(&RowId(5))?,
            serde_json::to_string(&EdgeId(5))?
        );
        assert_eq!(serde_json::from_str::<RowId>("5")?, RowId(5));
        round_trip(&RowId(u32::MAX))?;
        assert!(RowId(1) < RowId(2));
        Ok(())
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct ByViewRef {
        #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
        map: BTreeMap<ViewRef, String>,
    }

    #[test]
    fn view_ref_keyed_maps_round_trip_through_btree_map_as_vec() -> TestResult {
        let map: BTreeMap<ViewRef, String> = all_view_refs()
            .into_iter()
            .enumerate()
            .map(|(i, r)| (r, i.to_string()))
            .collect();
        // A plain map keyed by ViewRef is rejected by JSON.
        assert!(serde_json::to_string(&map).is_err());
        round_trip(&ByViewRef { map })?;
        Ok(())
    }
}
