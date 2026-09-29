//! Per-protocol semantic declarations, attached to each
//! [`Protocol`](panproto_schema::Protocol) as it is built.
//!
//! Every `protocol()` constructor in this crate passes its
//! [`Protocol`](panproto_schema::Protocol) to
//! `attach` below before returning it, so the
//! declarations of a protocol are kept in tables here rather than in its
//! constructor.

use panproto_schema::Protocol;

/// Attaches the semantic declarations of `protocol` (its syntactic and
/// semantic shapes, the semantics of its constraint sorts and the scalar
/// kinds of its vertex kinds) to it.
///
/// No protocol has declarations in these tables yet, so `protocol` is left
/// unchanged and keeps the defaults of its constructor.
pub const fn attach(protocol: &mut Protocol) {
    let _ = protocol;
}
