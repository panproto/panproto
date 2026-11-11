//! The three instance shapes a protocol's data can take.
//!
//! A tree is a W-type instance, a table is a set-valued functor instance and
//! a graph is a graph instance. A [`Protocol`](crate::Protocol) declares the
//! shape its codec produces (`syntactic_shape`) and the shape its data means
//! once references are resolved (`semantic_shape`).
//!
//! The enum lives in this crate because [`Protocol`](crate::Protocol) carries
//! it and this crate does not depend on `panproto-inst`, which re-exports it
//! as `panproto_inst::InstanceShapeKind`.
//!
//! ```
//! use panproto_schema::InstanceShapeKind;
//!
//! assert_eq!(InstanceShapeKind::default(), InstanceShapeKind::Tree);
//! assert!(InstanceShapeKind::Tree < InstanceShapeKind::Graph);
//! ```

use serde::{Deserialize, Serialize};

/// The shape of an instance: tree, table or graph.
///
/// Serialized in `snake_case` (`"tree"`, `"table"`, `"graph"`). The derived
/// order is the declaration order. The default is [`InstanceShapeKind::Tree`].
///
/// ```
/// use panproto_schema::InstanceShapeKind;
///
/// let json = serde_json::to_string(&InstanceShapeKind::Table).unwrap();
/// assert_eq!(json, r#""table""#);
/// let back: InstanceShapeKind = serde_json::from_str(r#""graph""#).unwrap();
/// assert_eq!(back, InstanceShapeKind::Graph);
/// assert!(serde_json::from_str::<InstanceShapeKind>(r#""Graph""#).is_err());
/// ```
///
/// `panproto-inst` re-exports this type, so `panproto_inst::InstanceShapeKind`
/// and `panproto_schema::InstanceShapeKind` name one type.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum InstanceShapeKind {
    /// A W-type instance: nodes with arcs to their children.
    #[default]
    Tree,
    /// A set-valued functor instance: rows per table and foreign-key pairs.
    Table,
    /// A graph instance: nodes and labelled edges.
    Graph,
}

#[cfg(test)]
mod tests {
    use super::InstanceShapeKind;

    #[test]
    fn instance_shape_kind_serde_names_are_snake_case() -> Result<(), serde_json::Error> {
        let all = [
            InstanceShapeKind::Tree,
            InstanceShapeKind::Table,
            InstanceShapeKind::Graph,
        ];
        assert_eq!(serde_json::to_string(&all)?, r#"["tree","table","graph"]"#);
        let back: Vec<InstanceShapeKind> = serde_json::from_str(r#"["tree","table","graph"]"#)?;
        assert_eq!(back, all);
        assert!(all.windows(2).all(|w| w[0] < w[1]));
        Ok(())
    }

    #[test]
    fn instance_shape_kind_round_trips_msgpack() -> Result<(), Box<dyn std::error::Error>> {
        for kind in [
            InstanceShapeKind::Tree,
            InstanceShapeKind::Table,
            InstanceShapeKind::Graph,
        ] {
            let bytes = rmp_serde::to_vec(&kind)?;
            assert_eq!(rmp_serde::from_slice::<InstanceShapeKind>(&bytes)?, kind);
        }
        Ok(())
    }

    #[test]
    fn instance_shape_kind_defaults_to_tree() {
        assert_eq!(InstanceShapeKind::default(), InstanceShapeKind::Tree);
    }
}
