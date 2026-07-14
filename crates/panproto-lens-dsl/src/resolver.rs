//! Named-reference resolution for `compose` bodies.
//!
//! A `compose` body may reference sibling lenses by `id`. The compiler
//! takes a resolver callback (`Fn(&str) -> Option<CompiledLens>`); this
//! module builds resolvers over a set of already-parsed documents so
//! that named references actually resolve, rather than always failing
//! against the null resolver.
//!
//! Two builders are provided:
//!
//! - [`compile_with_refs`] compiles a document, resolving `ref` entries
//!   against an in-memory `id → document` map. Bindings that hold a
//!   bundle of DSL sources use this.
//! - [`compile_in_dir`] loads every sibling document from a directory
//!   (via [`load_dir`](crate::load_dir)) and resolves against those.
//!   [`load_and_compile`](crate::load_and_compile) uses this.
//!
//! Resolution is one level deep: a referenced document is compiled with
//! the null resolver, so a referenced document may not itself contain a
//! `compose` body of further named references. This keeps resolution
//! total and cycle-free; deeper composition should be flattened at
//! authoring time or expressed inline.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;

use crate::compile::{self, CompiledLens};
use crate::document::LensDocument;
use crate::error::LensDslError;

/// Compile `doc`, resolving `compose` named references against
/// `docs_by_id`.
///
/// Each referenced document is compiled with the same `body_vertex` and
/// the null resolver (one-level resolution). A reference whose `id` is
/// absent from `docs_by_id`, or whose target fails to compile, surfaces
/// as [`LensDslError::UnresolvedRef`] naming the missing `id`.
///
/// # Errors
///
/// Propagates compilation errors from `doc`, including
/// [`LensDslError::UnresolvedRef`] for unresolved references.
pub fn compile_with_refs<S: BuildHasher>(
    doc: &LensDocument,
    body_vertex: &str,
    docs_by_id: &HashMap<String, LensDocument, S>,
) -> Result<CompiledLens, LensDslError> {
    let resolver = |id: &str| -> Option<CompiledLens> {
        docs_by_id
            .get(id)
            .and_then(|referenced| compile::compile(referenced, body_vertex, &null_resolver).ok())
    };
    compile::compile(doc, body_vertex, &resolver)
}

/// Compile `doc`, resolving `compose` named references against the other
/// lens documents found in `dir`.
///
/// Sibling documents are loaded via [`load_dir`](crate::load_dir). The
/// document's own `id` is excluded from the resolution set so a
/// self-reference cannot recurse.
///
/// # Errors
///
/// Returns [`LensDslError::Io`] if the directory cannot be read, and
/// propagates compilation errors from `doc`.
pub fn compile_in_dir(
    doc: &LensDocument,
    dir: &Path,
    body_vertex: &str,
) -> Result<CompiledLens, LensDslError> {
    let loaded = crate::load_dir(dir)?;
    let docs_by_id: HashMap<String, LensDocument> = loaded
        .documents
        .into_iter()
        .filter(|sibling| sibling.id != doc.id)
        .map(|sibling| (sibling.id.clone(), sibling))
        .collect();
    compile_with_refs(doc, body_vertex, &docs_by_id)
}

/// The null resolver: resolves nothing. Used for one-level resolution of
/// referenced documents.
const fn null_resolver(_id: &str) -> Option<CompiledLens> {
    None
}
