//! Homomorphism search and the theory -> schema -> data cascade.
//!
//! These entry points mirror the Python-only `panproto_py::hom` surface
//! (morphism search has no WASM analogue). The `PyO3` wrapper passes
//! `SearchOptions` and `FoundMorphism` across the boundary as `PyO3`
//! classes; the C ABI passes them as CBOR. Because
//! `panproto_core::mig::hom_search::{SearchOptions, FoundMorphism}` do
//! not derive `serde`, the CBOR payload types are the serializable
//! shadow structs `SearchOptionsWire` and `FoundMorphismWire`
//! defined here (mirroring the shadow-struct idiom in
//! [`crate::api::helpers`]), converted to and from the engine types at
//! the boundary. `SchemaMorphism`, `TheoryMorphism`, and `Migration`
//! already derive `serde` and cross as themselves.
//!
//! The WASM `WasmError`/`JsError` pair becomes [`FfiError`], `rmp_serde`
//! becomes [`crate::canonical`] (CBOR via ciborium), and handle outputs
//! land in the slab as [`Resource::Migration`](crate::handle::Resource)
//! (for `morphism_to_migration`) or
//! [`Resource::MigrationWithSchemas`](crate::handle::Resource) (for the
//! cascade's `induce_migration_from_theory`).

use std::collections::HashMap;
use std::sync::Arc;

use panproto_core::gat::{Name, TheoryMorphism};
use panproto_core::mig::{
    self, Migration, cascade,
    hom_search::{self, FoundMorphism, SearchOptions},
};
use panproto_core::schema::Edge;
use safer_ffi::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

// ---------------------------------------------------------------------------
// Serializable shadow structs (CBOR payload types)
// ---------------------------------------------------------------------------

/// Serializable mirror of
/// `panproto_core::mig::hom_search::SearchOptions`.
///
/// The engine type derives only `Clone`/`Debug`/`Default`, so it cannot
/// cross the FFI boundary directly. This shadow struct carries the same
/// fields with the same `snake_case` names the Haskell encoder pins
/// (`monic`, `epic`, `iso`, `max_results`, `initial`,
/// `relax_edge_name_pruning`). `serde(default)` lets a producer omit any
/// field, matching the engine's `Default`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct SearchOptionsWire {
    /// Require an injective vertex map.
    #[serde(default)]
    monic: bool,
    /// Require a surjective vertex map.
    #[serde(default)]
    epic: bool,
    /// Require a bijective vertex map (an isomorphism).
    #[serde(default)]
    iso: bool,
    /// Stop after this many morphisms; `0` means unlimited.
    #[serde(default)]
    max_results: usize,
    /// Pre-assigned vertex mappings (the Python `anchors`).
    #[serde(default)]
    initial: HashMap<String, String>,
    /// Relax the CSP's edge-name overlap pruning.
    #[serde(default)]
    relax_edge_name_pruning: bool,
}

impl SearchOptionsWire {
    /// Build the engine [`SearchOptions`], lifting the string-keyed
    /// `initial` map into the `Name`-keyed form the search expects.
    fn into_engine(self) -> SearchOptions {
        let initial = self
            .initial
            .into_iter()
            .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
            .collect();
        SearchOptions {
            monic: self.monic,
            epic: self.epic,
            iso: self.iso,
            max_results: self.max_results,
            initial,
            relax_edge_name_pruning: self.relax_edge_name_pruning,
        }
    }
}

/// Serializable mirror of
/// `panproto_core::mig::hom_search::FoundMorphism`.
///
/// The engine type derives only `Clone`/`Debug`, so the C ABI exchanges
/// this shadow struct instead. The wire shape matches the Haskell
/// `FoundMorphism` codec exactly: a `vertex_map` (string-keyed, since a
/// `Name` is transparent over a string), an `edge_map` in the
/// `map_as_vec` array-of-pairs shape (the `Edge` key cannot be a CBOR
/// map key), and a `quality` double. `edge_map` is carried (unlike the
/// `PyO3` `to_dict`, which drops it) because `morphism_to_migration`
/// reconstructs the migration's edge mapping from it.
#[derive(Debug, Serialize, Deserialize)]
struct FoundMorphismWire {
    /// Vertex mapping: source vertex ID to target vertex ID.
    vertex_map: HashMap<Name, Name>,
    /// Edge mapping: source edge to target edge, as an array of pairs.
    #[serde(with = "panproto_core::schema::serde_helpers::map_as_vec")]
    edge_map: HashMap<Edge, Edge>,
    /// Quality score in `[0, 1]`.
    quality: f64,
}

impl From<FoundMorphism> for FoundMorphismWire {
    fn from(m: FoundMorphism) -> Self {
        Self {
            vertex_map: m.vertex_map,
            edge_map: m.edge_map,
            quality: m.quality,
        }
    }
}

impl From<FoundMorphismWire> for FoundMorphism {
    fn from(w: FoundMorphismWire) -> Self {
        Self {
            vertex_map: w.vertex_map,
            edge_map: w.edge_map,
            quality: w.quality,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Find structure-preserving morphisms between two schemas.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `opts` is a CBOR-encoded `SearchOptionsWire` mirroring
/// `panproto_core::mig::hom_search::SearchOptions`. On success, `out`
/// receives a CBOR-encoded `Vec<FoundMorphismWire>` (each with
/// `vertex_map`, `edge_map`, and `quality`), already ranked by
/// descending quality. Calls `hom_search::find_morphisms`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_morphisms(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let wire: SearchOptionsWire = crate::canonical::decode(opts.as_slice())?;
        let options = wire.into_engine();

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let found = hom_search::find_morphisms(&src_schema, &tgt_schema, &options);
        let wire_results: Vec<FoundMorphismWire> =
            found.into_iter().map(FoundMorphismWire::from).collect();

        let bytes = crate::canonical::encode(&wire_results)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Find the single best-quality morphism between two schemas.
///
/// Arguments match [`pp_hom_find_morphisms`]. On success, `out`
/// receives a CBOR-encoded `Option<FoundMorphismWire>`. Calls
/// `hom_search::find_best_morphism`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_best_morphism(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let wire: SearchOptionsWire = crate::canonical::decode(opts.as_slice())?;
        let options = wire.into_engine();

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let best = hom_search::find_best_morphism(&src_schema, &tgt_schema, &options)
            .map(FoundMorphismWire::from);

        let bytes = crate::canonical::encode(&best)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Convert a found morphism into a compiled migration.
///
/// `morphism` is a CBOR-encoded `FoundMorphismWire`. The found
/// morphism is lowered to a `mig::Migration` via
/// `hom_search::morphism_to_migration`, then compiled against the
/// minimal schemas implied by its surviving vertex and edge sets. On
/// success, `out_handle` receives a fresh
/// [`Resource::Migration`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_morphism_to_migration(morphism: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let wire: FoundMorphismWire = crate::canonical::decode(morphism.as_slice())?;
        let found: FoundMorphism = wire.into();

        let migration: Migration = hom_search::morphism_to_migration(&found);

        // The found morphism is total on the matched sub-schema, so the
        // surviving vertex/edge sets recover the source and target.
        // Build minimal anchoring schemas from those sets, then compile
        // (the same `mig::compile` the `mig` domain uses) so the handle
        // is an applyable `CompiledMigration`.
        let (src_schema, tgt_schema) = minimal_schemas_for_migration(&migration);

        let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
            .map_err(|e| FfiError::Operation(format!("compile: {e}")))?;

        *out_handle = handle::alloc(Resource::Migration(Box::new(compiled)));
        Ok(PpStatus::Ok)
    })
}

/// Induce a schema morphism from a theory morphism and a source schema.
///
/// `theory_morphism` is a CBOR-encoded
/// `panproto_core::gat::TheoryMorphism`. `src` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded `panproto_core::schema::SchemaMorphism`.
/// Calls `mig::cascade::induce_schema_morphism`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_schema_morphism(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let theory_morph: TheoryMorphism = crate::canonical::decode(theory_morphism.as_slice())?;

        let src_schema = handle::with_resource(src, |r| Ok(r.as_schema()?.clone()))?;

        let schema_morph = cascade::induce_schema_morphism(&theory_morph, &src_schema);

        let bytes = crate::canonical::encode(&schema_morph)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Induce a migration from a theory morphism and source/target schemas.
///
/// `theory_morphism` is a CBOR-encoded `gat::TheoryMorphism`; `src` and
/// `tgt` are [`Resource::Schema`](crate::handle::Resource) handles. On
/// success, `out` receives the CBOR-encoded induced `SchemaMorphism`
/// and `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle
/// (the compiled `Delta_F` pullback bundled with its anchoring schemas).
/// Calls `mig::cascade::induce_migration_from_theory`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_migration_from_theory(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    tgt: u32,
    out: &mut repr_c::Vec<u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let theory_morph: TheoryMorphism = crate::canonical::decode(theory_morphism.as_slice())?;

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let (schema_morph, compiled) =
            cascade::induce_migration_from_theory(&theory_morph, &src_schema, &tgt_schema);

        *out_handle = handle::alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(compiled),
            src_schema: Arc::new(src_schema),
            tgt_schema: Arc::new(tgt_schema),
        });

        let bytes = crate::canonical::encode(&schema_morph)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build minimal source and target schemas that anchor `migration`.
///
/// A migration produced by `morphism_to_migration` carries no resolver
/// tables, only the vertex and edge maps. Its domain keys describe the
/// source structure and its range values the target structure; compiling
/// against schemas reconstructed from those keys (source) and values
/// (target) yields an applyable `CompiledMigration` without the caller
/// having to supply the original schemas (which the C ABI does not
/// receive for this entry point). Vertex kinds are unknown here, so a
/// uniform `unknown` kind is used; the compile path keys off the vertex
/// and edge maps, not the kinds.
fn minimal_schemas_for_migration(
    migration: &Migration,
) -> (panproto_core::schema::Schema, panproto_core::schema::Schema) {
    use panproto_core::schema::{Schema, Vertex};
    use smallvec::SmallVec;

    fn schema_from(
        vertices: impl Iterator<Item = Name>,
        edges: impl Iterator<Item = Edge>,
    ) -> Schema {
        let mut vmap = HashMap::new();
        for v in vertices {
            vmap.entry(v.clone()).or_insert_with(|| Vertex {
                id: v.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
        }

        let mut emap = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for e in edges {
            // Endpoints of every edge must exist as vertices.
            vmap.entry(e.src.clone()).or_insert_with(|| Vertex {
                id: e.src.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
            vmap.entry(e.tgt.clone()).or_insert_with(|| Vertex {
                id: e.tgt.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
            emap.insert(e.clone(), e.kind.clone());
            outgoing.entry(e.src.clone()).or_default().push(e.clone());
            incoming.entry(e.tgt.clone()).or_default().push(e.clone());
            between
                .entry((e.src.clone(), e.tgt.clone()))
                .or_default()
                .push(e.clone());
        }

        Schema {
            protocol: String::new(),
            vertices: vmap,
            edges: emap,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    let src = schema_from(
        migration.vertex_map.keys().cloned(),
        migration.edge_map.keys().cloned(),
    );
    let tgt = schema_from(
        migration.vertex_map.values().cloned(),
        migration.edge_map.values().cloned(),
    );
    (src, tgt)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::gat::{Name, TheoryMorphism};
    use panproto_core::mig::Migration;
    use panproto_core::schema::{Schema, SchemaBuilder, SchemaMorphism};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::Resource;

    /// A two-vertex source schema: a `post` record with a `text` string
    /// property.
    fn source_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("text", "string", None::<&str>)
            .unwrap()
            .edge("post", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A target schema isomorphic to the source but with the record
    /// vertex renamed to `note` (the property keeps its `text` label).
    fn target_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("note", "record", None::<&str>)
            .unwrap()
            .vertex("text", "string", None::<&str>)
            .unwrap()
            .edge("note", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn alloc_schema(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// The all-default search options, CBOR-encoded.
    fn default_opts_bytes() -> Vec<u8> {
        encode(&SearchOptionsWire::default()).unwrap()
    }

    /// A theory morphism that renames the `prop` operation to `field`.
    fn rename_prop_morphism() -> TheoryMorphism {
        TheoryMorphism::new(
            "rename_prop",
            "test",
            "test",
            HashMap::new(),
            HashMap::from([(std::sync::Arc::from("prop"), std::sync::Arc::from("field"))]),
        )
    }

    #[test]
    fn search_options_wire_round_trips() {
        let wire = SearchOptionsWire {
            monic: true,
            epic: false,
            iso: true,
            max_results: 3,
            initial: HashMap::from([("a".to_string(), "b".to_string())]),
            relax_edge_name_pruning: true,
        };
        let bytes = encode(&wire).unwrap();
        let back: SearchOptionsWire = decode(&bytes).unwrap();
        let engine = back.into_engine();
        assert!(engine.monic);
        assert!(engine.iso);
        assert_eq!(engine.max_results, 3);
        assert!(engine.relax_edge_name_pruning);
        assert_eq!(engine.initial.get(&Name::from("a")), Some(&Name::from("b")));
    }

    #[test]
    fn search_options_wire_tolerates_missing_fields() {
        // An empty CBOR map decodes to all-default options.
        let bytes = encode(&HashMap::<String, bool>::new()).unwrap();
        let wire: SearchOptionsWire = decode(&bytes).unwrap();
        let engine = wire.into_engine();
        assert!(!engine.monic);
        assert_eq!(engine.max_results, 0);
        assert!(engine.initial.is_empty());
    }

    #[test]
    fn find_morphisms_finds_the_rename() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(src_h, tgt_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let results: Vec<FoundMorphismWire> = decode(&out).unwrap();
        assert!(!results.is_empty(), "expected at least one morphism");
        // The post -> note rename should appear in some result's vertex map.
        let maps_post_to_note = results
            .iter()
            .any(|m| m.vertex_map.get(&Name::from("post")) == Some(&Name::from("note")));
        assert!(maps_post_to_note, "expected a post -> note mapping");
        // Results are ranked by descending quality.
        for pair in results.windows(2) {
            assert!(pair[0].quality >= pair[1].quality);
        }
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_best_morphism_returns_some() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let best: Option<FoundMorphismWire> = decode(&out).unwrap();
        let best = best.expect("expected a best morphism");
        assert!((0.0..=1.0).contains(&best.quality));
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn morphism_to_migration_yields_a_migration_handle() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        // First find a best morphism, then convert it to a migration handle.
        let opts = slice(&default_opts_bytes());
        let mut best_out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut best_out);
        assert_eq!(status, PpStatus::Ok as i32);
        let best_bytes = best_out.to_vec();
        pp_buf_free(best_out);

        // `best_bytes` encodes Option<FoundMorphismWire>; unwrap to the
        // FoundMorphismWire payload for the migration call.
        let best: Option<FoundMorphismWire> = decode(&best_bytes).unwrap();
        let best = best.expect("expected a morphism");
        let morphism_bytes = encode(&best).unwrap();

        let morphism = slice(&morphism_bytes);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_morphism_to_migration(morphism.as_ref(), &mut mig_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);

        // The handle is an applyable Migration resource.
        let is_migration = handle::with_resource(mig_h, |r| Ok(r.as_migration().is_ok())).unwrap();
        assert!(is_migration);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn morphism_to_migration_handles_empty_morphism() {
        // A FoundMorphism with empty maps lowers to an empty migration;
        // compiling against empty schemas must still succeed.
        let wire = FoundMorphismWire {
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            quality: 1.0,
        };
        let bytes = encode(&wire).unwrap();
        let morphism = slice(&bytes);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_morphism_to_migration(morphism.as_ref(), &mut mig_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);
        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
    }

    #[test]
    fn induce_schema_morphism_round_trips() {
        let src = source_schema();
        let src_h = alloc_schema(&src);

        let theory = rename_prop_morphism();
        let theory_bytes = encode(&theory).unwrap();
        let theory_slice = slice(&theory_bytes);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_induce_schema_morphism(theory_slice.as_ref(), src_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let schema_morph: SchemaMorphism = decode(&out).unwrap();
        // Vertices are preserved with identical IDs.
        for (s, t) in &schema_morph.vertex_map {
            assert_eq!(s, t);
        }
        // The `prop` edge kind is renamed to `field`.
        let renamed = schema_morph
            .edge_map
            .iter()
            .any(|(s, t)| s.kind.as_ref() == "prop" && t.kind.as_ref() == "field");
        assert!(renamed, "expected prop -> field edge kind rename");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn induce_migration_from_theory_yields_morphism_and_handle() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let theory = rename_prop_morphism();
        let theory_bytes = encode(&theory).unwrap();
        let theory_slice = slice(&theory_bytes);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_induce_migration_from_theory(
            theory_slice.as_ref(),
            src_h,
            tgt_h,
            &mut out,
            &mut mig_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);

        let schema_morph: SchemaMorphism = decode(&out).unwrap();
        assert_eq!(schema_morph.vertex_map.len(), src.vertices.len());
        pp_buf_free(out);

        // The handle is a MigrationWithSchemas resource (accepts both
        // migration projection and carries the bundled schemas).
        let kind = handle::with_resource(mig_h, |r| Ok(r.type_name())).unwrap();
        assert_eq!(kind, "MigrationWithSchemas");
        assert!(handle::with_resource(mig_h, |r| Ok(r.as_migration().is_ok())).unwrap());

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_morphisms_rejects_non_schema_handle() {
        let proto = crate::api::helpers::default_protocol("p");
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));
        let src = source_schema();
        let src_h = alloc_schema(&src);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(proto_h, src_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_morphisms_rejects_invalid_handle() {
        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(u32::MAX - 1, u32::MAX - 2, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
    }

    #[test]
    fn find_morphisms_rejects_garbage_opts() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(src_h, tgt_h, bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn morphism_to_migration_lowers_maps() {
        // Sanity-check the lowering: morphism_to_migration carries the
        // vertex/edge maps straight across, which our minimal schema
        // builder then anchors.
        let mut vertex_map = HashMap::new();
        vertex_map.insert(Name::from("post"), Name::from("note"));
        let found = FoundMorphism {
            vertex_map,
            edge_map: HashMap::new(),
            quality: 0.9,
        };
        let migration: Migration = hom_search::morphism_to_migration(&found);
        let (src, tgt) = minimal_schemas_for_migration(&migration);
        assert!(src.vertices.contains_key(&Name::from("post")));
        assert!(tgt.vertices.contains_key(&Name::from("note")));
    }
}
