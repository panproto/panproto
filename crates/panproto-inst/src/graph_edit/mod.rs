//! Edits of graph instances: the canonical [`GraphDelta`] and the
//! [`GraphEdit`] surface that lowers to it.
//!
//! A [`GraphDelta`] reads as a span `S <- K -> S'` at the identity match on
//! ids (Hermann et al. 2011, stated: model updates as spans of inclusions).
//! Its `del_*` parts are `S \ K` with full payloads, its `add_*` parts are
//! `S' \ K`, and its `attrs` change attribution entries of elements kept in
//! `K`. An id in both a delete part and an add part is a *replacement*: a new
//! element under a reused name, whose predecessor's residue (its incident
//! edges) does not transfer.
//!
//! [`GraphDelta::apply`] is double-pushout rewriting at that match: it checks
//! the delta once against the input, then applies it in full, and leaves the
//! input unchanged on any error. The algebra ([`GraphDelta::invert`],
//! [`GraphDelta::compose`], [`GraphDelta::identity`]) is total and state free
//! on the representation, [`GraphDelta::independence`] names the ways two
//! deltas of one state can interfere, and [`GraphDelta::diff`] computes the
//! delta between two states.
//!
//! [`GraphEdit`] is the surface a caller writes. Every variant lowers to a
//! delta with [`GraphEdit::to_delta`] (or [`GraphEdit::to_delta_in`] when a
//! step needs the instance, as [`GraphEdit::Reconnect`] does), and the
//! state-reading constructors ([`GraphEdit::remove_node_cascade`],
//! [`GraphEdit::set_field`], [`GraphEdit::reconnect`], ...) read the incident
//! set or old value from an instance, lowering a delete-with-cascade intent
//! to a reversible double-pushout step (Harmer and Oshurko 2020, Def 2.1 and
//! Rem 2.2, stated).
//!
//! [`canonical_graph_delta_bytes`](crate::graph_edit::canonical_graph_delta_bytes) writes the normalized delta through a
//! [`CanonicalSink`](crate::CanonicalSink); its digest under
//! [`GRAPH_DELTA_CONTEXT`](crate::identity::GRAPH_DELTA_CONTEXT) is
//! [`GraphDelta::digest`].

mod apply;
mod canonical;
mod compose;
mod delta;
mod diff;
mod independence;
mod surface;

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;

pub use canonical::{
    canonical_graph_delta_bytes, write_attr_change, write_attr_key, write_attr_val,
    write_graph_delta,
};
pub use compose::{ComposeError, ElemRef};
pub use delta::{
    AttrChange, AttrKey, AttrVal, GraphDelta, GraphDeltaParts, MalformedDelta, NodeMeta,
};
pub use independence::EditConflict;
pub use surface::GraphEdit;
