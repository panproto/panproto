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
//! Resolution is transitive: a referenced document's own references
//! resolve against the same set. A reference to a document already being
//! compiled along the current chain of references is refused with
//! [`LensDslError::RefCycle`], and a referenced document's refusal is
//! reported as [`LensDslError::InRef`], located in that document, rather
//! than as an unresolved reference.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;

use crate::compile::{self, CompiledLens};
use crate::document::LensDocument;
use crate::error::LensDslError;

/// Compile `doc`, resolving `compose` named references against
/// `docs_by_id`.
///
/// Each referenced document is compiled with the same `body_vertex`,
/// resolving its own references against the same set.
///
/// # Errors
///
/// Propagates compilation errors from `doc`. At a reference
/// (`/compose/lenses/i/ref`): [`LensDslError::UnresolvedRef`] for an `id`
/// absent from `docs_by_id`, [`LensDslError::RefCycle`] for a reference
/// back into the chain being compiled (`doc` itself included),
/// [`LensDslError::InRef`] carrying a referenced document's own located
/// refusal, and [`LensDslError::SymmetricRefInDirectionalCompose`] for a
/// referenced document with a `symmetric` body.
pub fn compile_with_refs<S: BuildHasher>(
    doc: &LensDocument,
    body_vertex: &str,
    docs_by_id: &HashMap<String, LensDocument, S>,
) -> Result<CompiledLens, LensDslError> {
    let chain = RefCell::new(vec![doc.id.clone()]);
    compile_in_chain(doc, body_vertex, docs_by_id, &chain)
}

fn compile_in_chain<S: BuildHasher>(
    doc: &LensDocument,
    body_vertex: &str,
    docs_by_id: &HashMap<String, LensDocument, S>,
    chain: &RefCell<Vec<String>>,
) -> Result<CompiledLens, LensDslError> {
    let resolver = |id: &str| -> Option<Result<CompiledLens, LensDslError>> {
        if chain.borrow().iter().any(|open| open == id) {
            let mut cycle = chain.borrow().clone();
            cycle.push(id.to_owned());
            return Some(Err(LensDslError::RefCycle { cycle }));
        }
        let referenced = docs_by_id.get(id)?;
        chain.borrow_mut().push(id.to_owned());
        let compiled = compile_in_chain(referenced, body_vertex, docs_by_id, chain);
        chain.borrow_mut().pop();
        Some(compiled.map_err(|error| LensDslError::InRef {
            lens_ref: id.to_owned(),
            error: Box::new(error),
        }))
    };
    compile::compile_with(doc, body_vertex, &resolver)
}

/// Compile `doc`, resolving `compose` named references against the other
/// lens documents found in `dir`.
///
/// Sibling documents are loaded via [`load_dir`](crate::load_dir); a
/// sibling with `doc`'s own `id` is left out of the resolution set, and a
/// reference to that `id` is a cycle.
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
