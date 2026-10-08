//! Operations assigned to keys and registration.

pub use crate::keys::{
    annotated_format, buckets, index_and_null_match, key_equivalence, key_order, key_value,
    lexical_space, nearest_ancestor, path_value, representation_refines, sort_order, target_set,
    xml_noncdata,
};
pub use crate::schema::{
    Key, KeyPart, KeyRole, KeyScope, Reference, Schema, canonical_bytes, classify_change, register,
};
