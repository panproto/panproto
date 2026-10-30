//! What each failing entry point leaves in the last-error slot.
//!
//! The slot is process-global in the library as released, and this test
//! binary links that library, so every check here reads the slot a host
//! reads. The checks share it, so each one holds [`SLOT`] from its first
//! call to its last drain: under the default parallel runner another
//! check's failing call could otherwise overwrite, or its drain consume,
//! the envelope under test. Nothing else in this process touches the slot.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use panproto_c::api::data::{
    pp_data_migrate_backward, pp_data_migrate_forward, pp_data_store_dataset,
};
use panproto_c::api::helpers::default_protocol;
use panproto_c::api::lens::{
    pp_lens_compile_document, pp_lens_compile_document_with_refs, pp_lens_compose,
};
use panproto_c::api::protocol::pp_protocol_serialize;
use panproto_c::api::schema::pp_schema_to_cbor;
use panproto_c::api::vcs::{pp_vcs_branch, pp_vcs_commit, pp_vcs_init};
use panproto_c::api::{pp_buf_free, pp_handle_free, pp_last_error_take, pp_protocol_define};
use panproto_c::canonical::{decode, encode};
use panproto_c::error::{ErrorEnvelope, FfiError, PpStatus, set_last_error, take_last_error};
use panproto_c::handle::{self, Resource};
use panproto_c::panic::guard;
use panproto_core::lens::Complement;
use panproto_core::schema::{Protocol, Schema, SchemaBuilder};
use safer_ffi::prelude::*;

/// Held by every check for as long as it uses the slot.
static SLOT: Mutex<()> = Mutex::new(());

/// Take the slot for one check, starting it empty.
///
/// A check that fails while holding the lock poisons it; the slot itself
/// is still sound, so the next check recovers the guard.
fn exclusive_slot() -> MutexGuard<'static, ()> {
    let guard = SLOT.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = take_last_error();
    guard
}

/// Drain the slot through the C entry point, as a host does.
fn drain() -> Option<ErrorEnvelope> {
    let mut buf: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_last_error_take(&mut buf), PpStatus::Ok as i32);
    let envelope = if buf.is_empty() {
        None
    } else {
        Some(decode::<ErrorEnvelope>(&buf).expect("envelope decodes"))
    };
    pp_buf_free(buf);
    envelope
}

fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
    bytes.to_vec().into_boxed_slice().into()
}

fn empty_schema(protocol: &str) -> Schema {
    Schema {
        protocol: protocol.into(),
        vertices: HashMap::new(),
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: vec![],
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
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
        ..Default::default()
    }
}

fn alloc_schema(schema: Schema) -> u32 {
    handle::alloc(Resource::Schema(Arc::new(schema)))
}

fn define_protocol() -> u32 {
    let protocol = Protocol {
        name: "envelope-test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into()],
        constraint_sorts: vec![],
        ..Default::default()
    };
    let bytes = slice(&encode(&protocol).unwrap());
    let mut handle: u32 = u32::MAX;
    assert_eq!(
        pp_protocol_define(bytes.as_ref(), &mut handle),
        PpStatus::Ok as i32
    );
    handle
}

const SYMMETRIC_LENS_DOCUMENT: &[u8] = br#"{
    "id": "symmetric-demo",
    "source": "left",
    "target": "right",
    "symmetric": {
        "left": [{ "remove_field": "left-only" }],
        "right": [{ "remove_field": "right-only" }]
    }
}"#;

const DIRECTIONAL_COMPOSE_DOCUMENT: &[u8] = br#"{
    "id": "directional-root",
    "source": "left",
    "target": "right",
    "compose": {
        "mode": "vertical",
        "lenses": [{ "ref": "symmetric-demo" }]
    }
}"#;

// ---------------------------------------------------------------------
// The slot and the boundary guard
// ---------------------------------------------------------------------

#[test]
fn an_empty_slot_drains_to_an_empty_buffer() {
    let _slot = exclusive_slot();
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_last_error_take(&mut out), PpStatus::Ok as i32);
    assert!(out.is_empty());
    pp_buf_free(out);
}

#[test]
fn a_stashed_error_round_trips_through_the_drain() {
    let _slot = exclusive_slot();
    set_last_error(&FfiError::InvalidHandle { handle: 99 });
    let env = drain().expect("envelope present");
    assert_eq!(env.tag, "invalid_handle");
    assert!(env.message.contains("99"));
    assert!(drain().is_none(), "a drain clears the slot");
}

#[test]
fn a_successful_guarded_call_leaves_the_slot_empty() {
    let _slot = exclusive_slot();
    assert_eq!(guard(|| Ok(PpStatus::Ok)), PpStatus::Ok as i32);
    assert!(take_last_error().is_none());
}

#[test]
fn a_guarded_error_is_stashed() {
    let _slot = exclusive_slot();
    let status = guard(|| Err(FfiError::InvalidHandle { handle: 42 }));
    assert_eq!(status, PpStatus::InvalidHandle as i32);
    let env = take_last_error().expect("error stashed");
    assert_eq!(env.tag, "invalid_handle");
    assert!(env.message.contains("42"));
}

#[test]
fn a_guarded_str_panic_is_stashed() {
    let _slot = exclusive_slot();
    let status = guard(|| -> Result<PpStatus, FfiError> { panic!("boom") });
    assert_eq!(status, PpStatus::Panic as i32);
    let env = take_last_error().expect("panic stashed");
    assert_eq!(env.tag, "panic");
    assert!(env.message.contains("boom"));
}

#[test]
fn a_guarded_string_panic_is_stashed() {
    let _slot = exclusive_slot();
    let status = guard(|| -> Result<PpStatus, FfiError> {
        panic!("formatted {}", 7);
    });
    assert_eq!(status, PpStatus::Panic as i32);
    let env = take_last_error().expect("panic stashed");
    assert!(env.message.contains("formatted 7"));
}

// ---------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------

#[test]
fn an_invalid_handle_is_reported() {
    let _slot = exclusive_slot();
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_protocol_serialize(u32::MAX - 1, &mut out);
    assert_eq!(status, PpStatus::InvalidHandle as i32);
    pp_buf_free(out);

    let env = drain().expect("envelope present");
    assert_eq!(env.tag, "invalid_handle");
}

#[test]
fn a_schema_handle_serialized_as_a_protocol_is_a_type_mismatch() {
    let _slot = exclusive_slot();
    let h = alloc_schema(empty_schema("x"));

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_protocol_serialize(h, &mut out);
    assert_eq!(status, PpStatus::TypeMismatch as i32);
    pp_buf_free(out);

    let env = drain().expect("envelope present");
    assert_eq!(env.tag, "type_mismatch");
    assert!(env.message.contains("Protocol"));
    assert!(env.message.contains("Schema"));

    assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
}

#[test]
fn a_protocol_handle_encoded_as_a_schema_is_a_type_mismatch() {
    let _slot = exclusive_slot();
    let proto_h = define_protocol();

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_schema_to_cbor(proto_h, &mut out);
    assert_eq!(status, PpStatus::TypeMismatch as i32);
    pp_buf_free(out);

    let env = drain().expect("envelope present");
    assert_eq!(env.tag, "type_mismatch");
    assert!(env.message.contains("Schema"));
    assert!(env.message.contains("Protocol"));

    assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
}

// ---------------------------------------------------------------------
// Directional lens-document boundary
// ---------------------------------------------------------------------

fn assert_symmetric_document_error(envelope: &ErrorEnvelope) {
    assert_eq!(envelope.tag, "operation");
    assert!(
        envelope.message.contains("symmetric lens documents")
            && envelope.message.contains("pp_lens_symmetric_from_schemas")
            && envelope.message.contains("pp_lens_symmetric_sync"),
        "the error directs callers to the symmetric boundary: {}",
        envelope.message
    );
}

fn assert_symmetric_reference_error(envelope: &ErrorEnvelope) {
    assert_eq!(envelope.tag, "operation");
    assert!(
        envelope.message.contains("symmetric-demo")
            && envelope.message.contains("symmetric body")
            && envelope.message.contains("directed compose"),
        "the error identifies the symmetric reference rejected by directed compose: {}",
        envelope.message
    );
}

#[test]
fn directional_document_compile_rejects_a_symmetric_body() {
    let _slot = exclusive_slot();
    let mut out = u32::MAX;
    let status = pp_lens_compile_document(
        slice(SYMMETRIC_LENS_DOCUMENT).as_ref(),
        slice(b"json").as_ref(),
        slice(b"record").as_ref(),
        &mut out,
    );

    assert_eq!(status, PpStatus::Operation as i32);
    assert_eq!(out, u32::MAX, "no handle is allocated on rejection");
    assert_symmetric_document_error(&drain().expect("an error envelope is stashed"));
}

#[test]
fn directional_document_compile_with_refs_rejects_a_symmetric_reference() {
    let _slot = exclusive_slot();
    let refs = encode(&HashMap::from([(
        "symmetric-demo".to_owned(),
        std::str::from_utf8(SYMMETRIC_LENS_DOCUMENT)
            .expect("symmetric fixture is UTF-8")
            .to_owned(),
    )]))
    .expect("symmetric reference bundle encodes");
    let mut out = u32::MAX;
    let status = pp_lens_compile_document_with_refs(
        slice(DIRECTIONAL_COMPOSE_DOCUMENT).as_ref(),
        slice(b"json").as_ref(),
        slice(b"record").as_ref(),
        slice(&refs).as_ref(),
        &mut out,
    );

    assert_eq!(status, PpStatus::Operation as i32);
    assert_eq!(out, u32::MAX, "no handle is allocated on rejection");
    assert_symmetric_reference_error(&drain().expect("an error envelope is stashed"));
}

// ---------------------------------------------------------------------
// Data migration
// ---------------------------------------------------------------------

/// A `post` record with a `text` string property, under the record
/// vertex name `record`.
fn post_schema(record: &str) -> Schema {
    let proto = default_protocol("test");
    SchemaBuilder::new(&proto)
        .vertex(record, "record", None)
        .unwrap()
        .vertex("text", "string", None)
        .unwrap()
        .edge(record, "text", "prop", Some("text"))
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn a_short_complement_list_names_both_lengths() {
    let _slot = exclusive_slot();
    let src_h = alloc_schema(post_schema("post"));
    let tgt_h = alloc_schema(post_schema("note"));

    let json = slice(br#"[{"text": "a"}, {"text": "b"}, {"text": "c"}]"#);
    let mut ds_h: u32 = u32::MAX;
    assert_eq!(
        pp_data_store_dataset(src_h, json.as_ref(), &mut ds_h),
        PpStatus::Ok as i32
    );

    let mut data_h: u32 = u32::MAX;
    let mut comp_h: u32 = u32::MAX;
    assert_eq!(
        pp_data_migrate_forward(ds_h, src_h, tgt_h, &mut data_h, &mut comp_h),
        PpStatus::Ok as i32
    );

    // Drop the last complement, leaving 3 records against 2.
    let comp_bytes = handle::with_resource(comp_h, |r| Ok(r.as_dataset()?.data.clone())).unwrap();
    let mut complements: Vec<Complement> = decode(&comp_bytes).unwrap();
    assert_eq!(complements.len(), 3);
    complements.pop();
    let short = slice(&encode(&complements).unwrap());

    let mut restored_h: u32 = u32::MAX;
    let status = pp_data_migrate_backward(data_h, short.as_ref(), src_h, tgt_h, &mut restored_h);
    assert_eq!(status, PpStatus::Operation as i32);
    assert_eq!(restored_h, u32::MAX, "no handle is allocated on failure");

    let env = drain().expect("an envelope is stashed");
    assert!(
        env.message.contains('3') && env.message.contains('2'),
        "the error must name both lengths, got: {}",
        env.message
    );

    for h in [ds_h, data_h, comp_h, src_h, tgt_h] {
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }
}

// ---------------------------------------------------------------------
// Lens composition
// ---------------------------------------------------------------------

/// A lens handle over `src` keeping exactly the vertices and edges of `tgt`.
fn alloc_lens(src: &Schema, tgt: &Schema) -> u32 {
    let compiled = panproto_core::inst::CompiledMigration {
        surviving_verts: tgt.vertices.keys().cloned().collect(),
        surviving_edges: tgt.edges.keys().cloned().collect(),
        ..Default::default()
    };
    handle::alloc(Resource::MigrationWithSchemas {
        compiled: Box::new(compiled),
        src_schema: Arc::new(src.clone()),
        tgt_schema: Arc::new(tgt.clone()),
    })
}

#[test]
fn composing_lenses_that_stay_two_groups_is_not_single_stage() {
    let _slot = exclusive_slot();
    let source = post_schema("post");
    let mut middle = source.clone();
    middle.edges.clear();
    middle.vertices.retain(|id, _| id.as_ref() == "post");
    middle.outgoing.clear();
    middle.incoming.clear();
    middle.between.clear();
    // A lossy first stage keeps the pair from fusing into one lens.
    let first = alloc_lens(&source, &middle);
    let second = alloc_lens(&middle, &middle);

    let mut composed: u32 = u32::MAX;
    let status = pp_lens_compose(first, second, &mut composed);
    assert_eq!(status, PpStatus::NotSingleStage as i32);
    assert_eq!(composed, u32::MAX, "no handle is allocated on failure");

    let env = drain().expect("an envelope is stashed");
    assert_eq!(env.status, PpStatus::NotSingleStage as i32);
    assert_eq!(env.tag, "not_single_stage");
    assert!(
        env.message.contains("compose") && env.message.contains('2'),
        "the message names the operation and the group count, got: {}",
        env.message
    );

    for h in [first, second] {
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }
}

// ---------------------------------------------------------------------
// Schema version control
// ---------------------------------------------------------------------

fn init_repo(dir: &std::path::Path) -> u32 {
    let path = slice(dir.to_str().unwrap().as_bytes());
    let mut h: u32 = u32::MAX;
    assert_eq!(pp_vcs_init(path.as_ref(), &mut h), PpStatus::Ok as i32);
    assert_ne!(h, u32::MAX);
    h
}

#[test]
fn a_commit_with_nothing_staged_is_an_operation_error() {
    let _slot = exclusive_slot();
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_vcs_commit(
        repo,
        slice(b"empty").as_ref(),
        slice(b"alice").as_ref(),
        &mut out,
    );
    assert_eq!(status, PpStatus::Operation as i32);
    pp_buf_free(out);

    let env = drain().expect("error stashed");
    assert_eq!(env.tag, "operation");
    assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
}

#[test]
fn a_branch_before_any_commit_is_an_operation_error() {
    let _slot = exclusive_slot();
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_vcs_branch(repo, slice(b"feature").as_ref(), &mut out);
    assert_eq!(status, PpStatus::Operation as i32);
    pp_buf_free(out);

    let env = drain().expect("error stashed");
    assert_eq!(env.tag, "operation");
    assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
}

// ---------------------------------------------------------------------
// Git import
// ---------------------------------------------------------------------

#[cfg(feature = "git")]
mod git_import {
    use super::*;
    use panproto_c::api::git::pp_git_import;

    #[test]
    fn a_directory_that_is_not_a_repository_is_an_operation_error() {
        let _slot = exclusive_slot();
        let dir = tempfile::tempdir().unwrap();
        let path = slice(dir.path().to_str().unwrap().as_bytes());

        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_git_import(path.as_ref(), slice(b"HEAD").as_ref(), &mut h, &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        assert_eq!(h, u32::MAX);
        pp_buf_free(out);

        let env = drain().expect("error stashed");
        assert_eq!(env.tag, "operation");
        assert!(
            env.message.contains("open git repo"),
            "unexpected message: {}",
            env.message
        );
    }

    #[test]
    fn an_invalid_utf8_repository_path_is_an_operation_error() {
        let _slot = exclusive_slot();
        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_git_import(
            slice(&[0xFF, 0xFE]).as_ref(),
            slice(b"HEAD").as_ref(),
            &mut h,
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);

        let env = drain().expect("error stashed");
        assert!(
            env.message.contains("invalid repo path"),
            "unexpected message: {}",
            env.message
        );
    }
}

// ---------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------

#[cfg(feature = "project")]
mod projects {
    use super::*;
    use panproto_c::api::project::{pp_project_add_file, pp_project_build, pp_project_builder_new};

    #[test]
    fn building_an_empty_project_is_an_operation_error() {
        let _slot = exclusive_slot();
        let mut builder_h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut builder_h), PpStatus::Ok as i32);

        let mut project_h: u32 = u32::MAX;
        let status = pp_project_build(builder_h, &mut project_h);
        assert_eq!(status, PpStatus::Operation as i32);
        assert_eq!(project_h, u32::MAX);

        let env = drain().expect("error stashed");
        assert_eq!(env.tag, "operation");
        assert_eq!(pp_handle_free(builder_h), PpStatus::Ok as i32);
    }

    #[test]
    fn an_invalid_utf8_file_path_is_an_operation_error() {
        let _slot = exclusive_slot();
        let mut builder_h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut builder_h), PpStatus::Ok as i32);

        let status = pp_project_add_file(
            builder_h,
            slice(&[0xFF, 0xFE]).as_ref(),
            slice(b"x").as_ref(),
        );
        assert_eq!(status, PpStatus::Operation as i32);

        let env = drain().expect("error stashed");
        assert!(
            env.message.contains("invalid file path"),
            "unexpected message: {}",
            env.message
        );
        assert_eq!(pp_handle_free(builder_h), PpStatus::Ok as i32);
    }
}

// ---------------------------------------------------------------------
// Full-AST parsing
// ---------------------------------------------------------------------

#[cfg(feature = "full-parse")]
mod parsing {
    use super::*;
    use panproto_c::api::parse::{pp_parse_file, pp_parse_registry_new};

    #[test]
    fn an_invalid_utf8_source_path_is_an_operation_error() {
        let _slot = exclusive_slot();
        let mut reg: u32 = u32::MAX;
        assert_eq!(pp_parse_registry_new(&mut reg), PpStatus::Ok as i32);

        let mut schema_h: u32 = u32::MAX;
        let status = pp_parse_file(
            reg,
            slice(&[0xFF, 0xFE]).as_ref(),
            slice(b"package main\n").as_ref(),
            &mut schema_h,
        );
        assert_eq!(status, PpStatus::Operation as i32);

        let env = drain().expect("error stashed");
        assert!(
            env.message.contains("invalid path"),
            "unexpected message: {}",
            env.message
        );
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }
}
