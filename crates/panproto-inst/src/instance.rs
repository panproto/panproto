//! Unified instance representation (attributed C-set).
//!
//! [`Instance`] is the unified enum wrapping all instance shapes:
//! - [`WInstance`](crate::WInstance) — tree-shaped (W-type)
//! - [`FInstance`](crate::FInstance) — relational (functor)
//! - [`GInstance`](crate::GInstance) — graph-shaped (most general)
//!
//! All three are attributed C-sets over different shape categories.
//! The unified type enables generic code that operates on any instance
//! shape without knowing the concrete representation.

use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::wtype::{CompiledMigration, WInstance};

/// A unified instance wrapping all instance shapes.
///
/// This is the top-level instance type for generic code. Each variant
/// preserves the optimized internal representation of its shape.
///
/// All three shapes are attributed C-sets:
/// - `WType`: C = tree category (rooted, acyclic)
/// - `Functor`: C = relational category (bipartite: tables + foreign keys)
/// - `Graph`: C = graph category (general directed graph)
///
/// Each inner type implements [`AcsetOps`](crate::AcsetOps), which
/// provides `restrict`, `extend`, `element_count`, and `shape_name`
/// through a unified trait interface. The methods on `Instance` below
/// dispatch to those implementations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Instance {
    /// Tree-shaped instance (W-type).
    WType(WInstance),
    /// Relational/tabular instance (set-valued functor).
    Functor(FInstance),
    /// Graph-shaped instance (most general form).
    Graph(GInstance),
}

impl Instance {
    /// Returns the shape name.
    #[must_use]
    pub const fn shape_name(&self) -> &'static str {
        match self {
            Self::WType(_) => "wtype",
            Self::Functor(_) => "functor",
            Self::Graph(_) => "graph",
        }
    }

    /// Restrict this instance along a compiled migration.
    ///
    /// Dispatches to the shape-specific restrict pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if the restrict pipeline fails.
    pub fn restrict(
        &self,
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        match self {
            Self::WType(w) => {
                let restricted = crate::wtype_restrict(w, src_schema, tgt_schema, migration)?;
                Ok(Self::WType(restricted))
            }
            Self::Functor(f) => {
                let restricted = crate::functor_restrict(f, migration)?;
                Ok(Self::Functor(restricted))
            }
            Self::Graph(g) => {
                let restricted = crate::ginstance::graph_restrict(g, migration)?;
                Ok(Self::Graph(restricted))
            }
        }
    }

    /// Extend this instance along a compiled migration (`Sigma_F`).
    ///
    /// Dispatches to the shape-specific extend pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if the extend pipeline fails.
    pub fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        match self {
            Self::WType(w) => {
                let extended = crate::wtype_extend(w, tgt_schema, migration)?;
                Ok(Self::WType(extended))
            }
            Self::Functor(f) => {
                let extended = crate::functor_extend(f, migration)?;
                Ok(Self::Functor(extended))
            }
            Self::Graph(g) => {
                let extended = crate::ginstance::graph_extend(g, migration)?;
                Ok(Self::Graph(extended))
            }
        }
    }

    /// Returns the number of elements (nodes/rows/vertices) in this instance.
    #[must_use]
    pub fn element_count(&self) -> usize {
        match self {
            Self::WType(w) => w.node_count(),
            Self::Functor(f) => f.table_count(),
            Self::Graph(g) => g.node_count(),
        }
    }

    /// Try to get a reference to the inner `WInstance`.
    #[must_use]
    pub const fn as_wtype(&self) -> Option<&WInstance> {
        match self {
            Self::WType(w) => Some(w),
            _ => None,
        }
    }

    /// Try to get a reference to the inner `FInstance`.
    #[must_use]
    pub const fn as_functor(&self) -> Option<&FInstance> {
        match self {
            Self::Functor(f) => Some(f),
            _ => None,
        }
    }

    /// Try to get a reference to the inner `GInstance`.
    #[must_use]
    pub const fn as_graph(&self) -> Option<&GInstance> {
        match self {
            Self::Graph(g) => Some(g),
            _ => None,
        }
    }
}

impl From<WInstance> for Instance {
    fn from(w: WInstance) -> Self {
        Self::WType(w)
    }
}

impl From<FInstance> for Instance {
    fn from(f: FInstance) -> Self {
        Self::Functor(f)
    }
}

impl From<GInstance> for Instance {
    fn from(g: GInstance) -> Self {
        Self::Graph(g)
    }
}
