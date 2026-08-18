//! What of the span's certificate crosses the C ABI, and what a host can do
//! without it.
//!
//! `SchemaSpanWire` carries four of the certificate's ten fields, and two of
//! those four are what a host needs to *identify* a span: `apex_digest`, which
//! with the two leg maps is the span's identity, and `legs_are_functorial`.
//! The first test pins that both cross, and that the digest a host reads back
//! is the one `canonical_digest` computes over the apex.
//!
//! The two tests in the middle are why the digest has to cross rather than be
//! computed host-side. There is no schema-digest entry point among the ABI's
//! exports, the CBOR `pp_schema_to_cbor` hands out is not the digest's
//! pre-image, and the one schema digest a C host can reach, the VCS
//! `schema_id`, is a different number by design: `panproto-vcs` hashes a subset
//! of the schema's fields while `canonical_bytes` covers all of them.
//!
//! The last test is why both fields carry `serde(default)`. `SchemaSpanWire` is
//! bidirectional, and `pp_hom_span_to_overlap` takes a span a *host* encoded, so
//! a host that writes only the other nine keys must still be decoded rather than
//! answered with `PpStatus::Serialization`. That payload is fed back through the
//! real entry point here, so the compatibility is measured rather than asserted
//! in a comment.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_c::api::{
    pp_buf_free, pp_handle_free, pp_hom_find_span, pp_hom_span_to_overlap, pp_protocol_define,
    pp_registry_get_builtin, pp_schema_from_cbor, pp_schema_to_cbor, pp_vcs_add, pp_vcs_commit,
    pp_vcs_init, pp_vcs_log,
};
use panproto_c::canonical::{decode, encode};
use panproto_c::error::PpStatus;
use panproto_core::mig::hom_search::{self, DomainConstraints, SearchOptions};
use panproto_core::schema::{Protocol, Schema, SchemaBuilder, canonical_bytes, canonical_digest};
use safer_ffi::prelude::*;

/// Lower-case hex, which is the rendering every object id on this surface uses.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Fetch atproto the way a C host does, through the ABI rather than the crate.
fn atproto() -> Protocol {
    let name = b"atproto".as_slice().into();
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_registry_get_builtin(name, &mut out), PpStatus::Ok as i32);
    let protocol: Protocol = decode(&out).unwrap();
    pp_buf_free(out);
    protocol
}

/// A record with two properties.
fn wide(protocol: &Protocol) -> Schema {
    SchemaBuilder::new(protocol)
        .vertex("r", "record", Some("local.x"))
        .unwrap()
        .vertex("r:b", "object", None)
        .unwrap()
        .vertex("r:b:s", "string", None)
        .unwrap()
        .vertex("r:b:i", "integer", None)
        .unwrap()
        .edge("r", "r:b", "record-schema", None)
        .unwrap()
        .edge("r:b", "r:b:s", "prop", Some("s"))
        .unwrap()
        .edge("r:b", "r:b:i", "prop", Some("i"))
        .unwrap()
        .build()
        .unwrap()
}

/// The same record with one property, so the span is partial and its
/// certificate has something to say.
fn narrow(protocol: &Protocol) -> Schema {
    SchemaBuilder::new(protocol)
        .vertex("r", "record", Some("local.x"))
        .unwrap()
        .vertex("r:b", "object", None)
        .unwrap()
        .vertex("r:b:s", "string", None)
        .unwrap()
        .edge("r", "r:b", "record-schema", None)
        .unwrap()
        .edge("r:b", "r:b:s", "prop", Some("s"))
        .unwrap()
        .build()
        .unwrap()
}

fn load_schema(schema: &Schema) -> u32 {
    let bytes = encode(schema).unwrap();
    let slice = bytes.as_slice().into();
    let mut handle = 0u32;
    assert_eq!(pp_schema_from_cbor(slice, &mut handle), PpStatus::Ok as i32);
    handle
}

fn load_protocol(protocol: &Protocol) -> u32 {
    let bytes = encode(protocol).unwrap();
    let slice = bytes.as_slice().into();
    let mut handle = 0u32;
    assert_eq!(pp_protocol_define(slice, &mut handle), PpStatus::Ok as i32);
    handle
}

/// The span wire as bytes, decoded as a raw CBOR value so the key list is what
/// crossed rather than what the struct declares.
fn span_wire(src: &Schema, tgt: &Schema, protocol: &Protocol) -> ciborium::Value {
    let src_h = load_schema(src);
    let tgt_h = load_schema(tgt);
    let proto_h = load_protocol(protocol);

    let empty = encode(&ciborium::Value::Map(Vec::new())).unwrap();
    let opts = empty.as_slice().into();
    let cons = empty.as_slice().into();
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(
        pp_hom_find_span(src_h, tgt_h, proto_h, opts, cons, &mut out),
        PpStatus::Ok as i32,
        "the span search is total, so this call answers"
    );
    let wire: ciborium::Value = decode(&out).unwrap();
    pp_buf_free(out);

    assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    wire
}

/// The digest and the functoriality flag cross, and the digest is the engine's.
#[test]
fn the_span_wire_carries_the_digest_the_engine_computed() {
    let protocol = atproto();
    let src = wide(&protocol);
    let tgt = narrow(&protocol);

    let wire = span_wire(&src, &tgt, &protocol);
    let map = wire.as_map().expect("a span encodes as a CBOR map");
    let field = |name: &str| {
        map.iter()
            .find(|(key, _)| key.as_text() == Some(name))
            .map(|(_, value)| value.clone())
    };

    // The engine's own answer to the same question, from the same call.
    let engine = hom_search::find_span_constrained(
        &src,
        &tgt,
        &protocol,
        &SearchOptions::default(),
        &DomainConstraints::default(),
    )
    .expect("the span search is total");

    assert_eq!(
        field("apex_digest").and_then(|v| v.as_text().map(str::to_owned)),
        Some(engine.apex_digest_hex()),
        "the wire digest must be the engine's digest, lower-case hex, or a host \
         caching on it caches under a name nothing else uses"
    );
    assert_eq!(
        field("legs_are_functorial").and_then(|v| v.as_bool()),
        Some(engine.certificate.legs_are_functorial)
    );
    assert_eq!(
        field("proven_optimal").and_then(|v| v.as_bool()),
        Some(engine.certificate.proven_optimal)
    );
}

/// A host cannot recompute the digest from the bytes it holds.
///
/// This is the reason the field is necessary rather than convenient: the CBOR
/// `pp_schema_to_cbor` returns is serde's rendering of `Schema`, while the
/// digest hashes `canonical_bytes`, which sorts and normalises. Hashing the
/// former yields a different number.
#[test]
fn the_cbor_a_host_holds_is_not_the_digest_pre_image() {
    let protocol = atproto();
    let schema = narrow(&protocol);
    let handle = load_schema(&schema);

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_schema_to_cbor(handle, &mut out), PpStatus::Ok as i32);
    let cbor = out.to_vec();
    pp_buf_free(out);
    assert_eq!(pp_handle_free(handle), PpStatus::Ok as i32);

    assert_ne!(
        cbor,
        canonical_bytes(&schema),
        "if these were equal a host could hash what it already has, and the \
         wire field would be redundant"
    );
}

/// Nor from the one schema digest the ABI does expose.
///
/// `pp_vcs_log` reports a `schema_id`, which is the closest thing a C host has
/// to a content address for a schema. It is a different hash: `panproto-vcs`
/// covers a subset of the schema's fields deliberately, so substituting it for
/// the apex digest would silently identify spans over apexes that differ.
#[test]
fn the_vcs_schema_id_is_not_the_canonical_digest() {
    let protocol = atproto();
    let schema = narrow(&protocol);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().into_owned();

    let mut repo = 0u32;
    assert_eq!(
        pp_vcs_init(path.as_bytes().into(), &mut repo),
        PpStatus::Ok as i32
    );

    let handle = load_schema(&schema);
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_vcs_add(repo, handle, &mut out), PpStatus::Ok as i32);
    pp_buf_free(out);

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(
        pp_vcs_commit(
            repo,
            b"m".as_slice().into(),
            b"a".as_slice().into(),
            &mut out
        ),
        PpStatus::Ok as i32
    );
    pp_buf_free(out);

    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_vcs_log(repo, 1, &mut out), PpStatus::Ok as i32);
    let log: ciborium::Value = decode(&out).unwrap();
    pp_buf_free(out);

    let entries = log
        .as_map()
        .unwrap()
        .iter()
        .find(|(key, _)| key.as_text() == Some("entries"))
        .map(|(_, value)| value.as_array().unwrap().clone())
        .unwrap();
    let schema_id = entries[0]
        .as_map()
        .unwrap()
        .iter()
        .find(|(key, _)| key.as_text() == Some("schema_id"))
        .map(|(_, value)| value.as_text().unwrap().to_owned())
        .unwrap();

    assert_ne!(
        schema_id,
        hex(&canonical_digest(&schema)),
        "if these were equal a host could read the apex digest off the VCS log"
    );

    assert_eq!(pp_handle_free(handle), PpStatus::Ok as i32);
    assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
}

/// The nine-key payload a host encoder writes still converts to an overlap.
///
/// A host encoder is free to write only the nine keys it has values for, since
/// `apex_digest` and `legs_are_functorial` are measurements the engine makes
/// rather than inputs it needs. Without `serde(default)` on those two this call
/// returns `PpStatus::Serialization` and `pp_hom_span_to_overlap` is unusable
/// from any such host.
#[test]
fn a_nine_key_span_from_a_host_encoder_still_converts() {
    const HOST_KEYS: [&str; 9] = [
        "apex",
        "left",
        "right",
        "quality",
        "quality_lo",
        "quality_hi",
        "apex_coverage",
        "proven_optimal",
        "is_total",
    ];

    let protocol = atproto();
    let wire = span_wire(&wide(&protocol), &narrow(&protocol), &protocol);

    let legacy: Vec<(ciborium::Value, ciborium::Value)> = wire
        .as_map()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.as_text().is_some_and(|text| HOST_KEYS.contains(&text)))
        .cloned()
        .collect();
    assert_eq!(
        legacy.len(),
        HOST_KEYS.len(),
        "the nine fields a host encoder writes must all still be produced, or \
         this fixture is testing a payload nobody sends"
    );

    let bytes = encode(&ciborium::Value::Map(legacy)).unwrap();
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_hom_span_to_overlap(bytes.as_slice().into(), &mut out);
    assert_eq!(
        status,
        PpStatus::Ok as i32,
        "a span missing the two new fields must still decode; adding them as \
         required fields is what breaks every host written against the \
         nine-key form"
    );
    let overlap: Result<ciborium::Value, _> = decode(&out);
    assert!(
        overlap.is_ok(),
        "and the overlap it produces must be readable"
    );
    pp_buf_free(out);
}
