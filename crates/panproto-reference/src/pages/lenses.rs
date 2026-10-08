//! Operations assigned to graph, table and tree lenses.

pub use crate::lens::{
    GraphLens, graph_square, protolens_instance, protolens_schema, put, table_square, tree_get,
    tree_put,
};
pub use crate::path::{LensPath, get_path, put_path};
pub use crate::table::{TableLens, get};
