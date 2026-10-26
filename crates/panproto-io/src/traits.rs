//! Core traits for instance-level presentation functors.
//!
//! Each protocol implements [`InstanceParser`] and [`InstanceEmitter`] to
//! connect raw format bytes to panproto's instance models. [`NativeRepr`]
//! names the shape the protocol's syntax has, which is the shape `parse`
//! returns:
//!
//! - a tree ([`NativeRepr::Tree`], [`panproto_inst::WInstance`]) for nested
//!   documents such as JSON and XML;
//! - a table ([`NativeRepr::Table`], [`panproto_inst::FInstance`]) for
//!   delimited, tabular and columnar formats;
//! - a graph ([`NativeRepr::Graph`], [`panproto_inst::GInstance`]) for
//!   native graph formats such as triples and edge lists, read through
//!   [`GraphInstanceParser`](crate::traits::GraphInstanceParser) and written through [`GraphInstanceEmitter`](crate::traits::GraphInstanceEmitter).
//!
//! [`TableInstanceParser`](crate::traits::TableInstanceParser) and [`TableInstanceEmitter`](crate::traits::TableInstanceEmitter) are the table
//! counterparts of the graph traits. The
//! [`ProtocolRegistry`](crate::ProtocolRegistry) keeps graph and table
//! codecs in their own tables beside the tree and table codecs of
//! [`InstanceParser`].

use panproto_inst::{FInstance, GInstance, WInstance};
use panproto_schema::{InstanceShapeKind, Schema};

use crate::error::{EmitInstanceError, ParseInstanceError};

#[cfg(feature = "tree-sitter")]
use crate::cst_extract::CstComplement;

/// Which instance shape a protocol's syntax has.
///
/// The variants follow the order of [`InstanceShapeKind`], and the
/// conversion `InstanceShapeKind::from` maps each to the kind of the same
/// name, so a codec's representation can be compared with the
/// `syntactic_shape` its protocol declares.
///
/// ```
/// use panproto_io::NativeRepr;
/// use panproto_schema::InstanceShapeKind;
///
/// assert_eq!(InstanceShapeKind::from(NativeRepr::Table), InstanceShapeKind::Table);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeRepr {
    /// W-type trees (wellfounded trees per Gambino & Hyland 2004).
    /// Used by document, API, annotation, and most web protocols.
    Tree,
    /// Set-valued functors (Spivak 2012).
    /// Used by relational, columnar, and tabular protocols.
    Table,
    /// Graph instances: nodes and labelled edges, with no distinguished
    /// root. Used by native graph formats.
    Graph,
}

impl From<NativeRepr> for InstanceShapeKind {
    fn from(repr: NativeRepr) -> Self {
        match repr {
            NativeRepr::Tree => Self::Tree,
            NativeRepr::Table => Self::Table,
            NativeRepr::Graph => Self::Graph,
        }
    }
}

/// Parse raw format bytes into a panproto instance.
///
/// Each implementation is a presentation functor: it witnesses that the raw
/// format syntax is a faithful encoding of the algebraic model defined by
/// the protocol's instance theory.
pub trait InstanceParser: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Which representation is native for this protocol.
    fn native_repr(&self) -> NativeRepr;

    /// Parse bytes into a W-type instance (tree-shaped).
    ///
    /// The `schema` guides parsing: outgoing edges determine child structure,
    /// vertex kinds determine node types, constraints validate values.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support W-type instances.
    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError>;

    /// Parse bytes into a functor instance (tabular).
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support functor instances.
    fn parse_functor(&self, schema: &Schema, input: &[u8])
    -> Result<FInstance, ParseInstanceError>;
}

/// Emit a panproto instance to raw format bytes.
///
/// The reverse of [`InstanceParser`]: converts abstract instance models
/// back to concrete format syntax.
pub trait InstanceEmitter: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Emit a W-type instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support W-type instances.
    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;

    /// Emit a functor instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::UnsupportedRepresentation`] if this
    /// protocol's instance theory does not support functor instances.
    fn emit_functor(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;
}

/// Parse raw format bytes into a graph instance.
///
/// Implemented by a protocol whose syntax is a graph (triples, edge lists,
/// node-link documents), so the bytes are read directly into a
/// [`GInstance`] with no tree or table in between. A codec registers itself
/// for a protocol through
/// [`ProtocolRegistry::register_graph`](crate::ProtocolRegistry::register_graph).
pub trait GraphInstanceParser: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Parse bytes into a graph instance.
    ///
    /// The `schema` guides parsing: node kinds are its vertices and edge
    /// labels are its edges.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] when the input is not well-formed in
    /// the format or does not fit `schema`.
    fn parse_graph(&self, schema: &Schema, input: &[u8]) -> Result<GInstance, ParseInstanceError>;
}

/// Emit a graph instance to raw format bytes.
///
/// The reverse of [`GraphInstanceParser`].
pub trait GraphInstanceEmitter: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Emit a graph instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] when the instance has no encoding in
    /// the format.
    fn emit_graph(
        &self,
        schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;
}

/// Parse raw format bytes into a table instance.
///
/// The table counterpart of [`GraphInstanceParser`]: a codec that reads its
/// format straight into an [`FInstance`] and registers through
/// [`ProtocolRegistry::register_table`](crate::ProtocolRegistry::register_table).
pub trait TableInstanceParser: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Parse bytes into a table instance.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] when the input is not well-formed in
    /// the format or does not fit `schema`.
    fn parse_table(&self, schema: &Schema, input: &[u8]) -> Result<FInstance, ParseInstanceError>;
}

/// Emit a table instance to raw format bytes.
///
/// The reverse of [`TableInstanceParser`].
pub trait TableInstanceEmitter: Send + Sync {
    /// The protocol name (must match `Protocol::name`).
    fn protocol_name(&self) -> &str;

    /// Emit a table instance to raw format bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] when the instance has no encoding in
    /// the format.
    fn emit_table(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError>;
}

/// Extended codec trait for format-preserving round-trips.
///
/// Codecs that implement this trait can capture formatting information
/// during parsing (as a [`CstComplement`]) and restore it during emission,
/// producing byte-identical output for unmodified data.
///
/// This trait is only available when the `tree-sitter` feature is enabled.
#[cfg(feature = "tree-sitter")]
pub trait FormatPreservingCodec: InstanceParser + InstanceEmitter {
    /// Parse bytes into a `WInstance`, capturing the CST complement for
    /// format-preserving emission.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] if parsing or extraction fails.
    fn parse_wtype_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, CstComplement), ParseInstanceError>;

    /// Emit bytes from a `WInstance` using the CST complement to preserve
    /// original formatting.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] if emission fails.
    fn emit_wtype_preserving(
        &self,
        schema: &Schema,
        instance: &WInstance,
        complement: &CstComplement,
    ) -> Result<Vec<u8>, EmitInstanceError>;
}

#[cfg(test)]
mod tests {
    use panproto_schema::InstanceShapeKind;

    use super::NativeRepr;

    #[test]
    fn native_repr_variants_follow_instance_shape_kind_order() {
        let reprs = [NativeRepr::Tree, NativeRepr::Table, NativeRepr::Graph];
        let kinds: Vec<InstanceShapeKind> =
            reprs.iter().copied().map(InstanceShapeKind::from).collect();
        assert_eq!(
            kinds,
            [
                InstanceShapeKind::Tree,
                InstanceShapeKind::Table,
                InstanceShapeKind::Graph,
            ]
        );
        assert!(kinds.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn native_repr_converts_to_the_same_named_shape_kind() {
        for (repr, kind) in [
            (NativeRepr::Tree, InstanceShapeKind::Tree),
            (NativeRepr::Table, InstanceShapeKind::Table),
            (NativeRepr::Graph, InstanceShapeKind::Graph),
        ] {
            assert_eq!(InstanceShapeKind::from(repr), kind);
            assert_eq!(format!("{repr:?}"), format!("{kind:?}"));
        }
    }
}
