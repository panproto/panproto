//! Complements written before envelopes existed still decode, are
//! recognised as legacy at the pointer width that wrote them, and still
//! put, under the policy that allows them.
//!
//! The fixtures in `tests/fixtures/complement_upgrade` were written by the
//! last release without envelopes, on a 64-bit host and through its wasm32
//! build, in every encoding the workspace reads a complement in. See the
//! README there for the table this test checks.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not load is a defect in the fixture"
)]

mod identity_corpus;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use identity_corpus::{case_lens, repo_root};
use panproto_inst::identity::{
    AnyComplement, ComplementShape, Encoding, IdentityError, IdentityPolicy, IdentityStatus,
    LegacyRejectReason, LegacyWidth, decode_complement, legacy_schema_fingerprint,
};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Complement, Fan, Node, WInstance};
use panproto_lens::{
    ComplementIdentity, Lens, LensError, get, instances_equivalent, put, put_checked,
};
use panproto_schema::Edge;
use serde::Deserialize;

/// The fingerprints each case's complements carry, as the fixture README
/// lists them: `(case, 64-bit, wasm32)`.
const FINGERPRINTS: [(&str, u64, u64); 4] = [
    (
        "contract",
        8_537_667_589_647_975_768,
        14_067_926_949_814_075_051,
    ),
    (
        "drop_field",
        2_770_883_639_970_332_002,
        16_649_305_309_498_269_837,
    ),
    (
        "ordered_list",
        5_568_901_458_156_004_314,
        8_140_304_790_023_999_500,
    ),
    (
        "rename_vertex",
        5_545_373_909_098_406_705,
        9_223_972_107_605_509_029,
    ),
];

const ENCODINGS: [(&str, Encoding); 4] = [
    ("pos.msgpack", Encoding::MsgpackPositional),
    ("named.msgpack", Encoding::MsgpackNamed),
    ("cbor", Encoding::Cbor),
    ("json", Encoding::Json),
];

fn case_dir(case: &str) -> PathBuf {
    repo_root()
        .join("tests/fixtures/complement_upgrade")
        .join(case)
}

fn case_instance(case: &str) -> WInstance {
    let text = std::fs::read_to_string(case_dir(case).join("instance.json"))
        .expect("the case instance exists");
    serde_json::from_str(&text).expect("the case instance parses")
}

fn fixture(case: &str, producer: &str, extension: &str) -> Vec<u8> {
    let path = case_dir(case).join(format!("{producer}.{extension}"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn decode(bytes: &[u8], encoding: Encoding) -> Complement {
    match decode_complement(bytes, encoding, ComplementShape::WType) {
        Ok(AnyComplement::WType(c)) => *c,
        Ok(AnyComplement::Functor(_)) => {
            panic!("a legacy tree complement decoded as a table complement")
        }
        Ok(AnyComplement::Resolution(_)) => {
            panic!("a legacy tree complement decoded as a resolution complement")
        }
        Err(e) => panic!("a legacy complement decodes: {e}"),
    }
}

#[test]
fn the_reimplemented_fingerprint_is_the_one_the_fixtures_carry() {
    for (case, bits64, bits32) in FINGERPRINTS {
        let lens = case_lens(&case_dir(case));
        assert_eq!(
            legacy_schema_fingerprint(&lens.src_schema, LegacyWidth::Bits64),
            bits64,
            "{case} at 64 bits"
        );
        assert_eq!(
            legacy_schema_fingerprint(&lens.src_schema, LegacyWidth::Bits32),
            bits32,
            "{case} at 32 bits"
        );
    }
}

#[test]
fn every_legacy_complement_decodes_as_legacy_at_its_width_and_puts() {
    for (case, bits64, bits32) in FINGERPRINTS {
        let lens = case_lens(&case_dir(case));
        let source = case_instance(case);
        let (view, _) = get(&lens, &source).expect("get");

        for (producer, fingerprint, width) in [
            ("native", bits64, LegacyWidth::Bits64),
            ("wasm32", bits32, LegacyWidth::Bits32),
        ] {
            for (extension, encoding) in ENCODINGS {
                let label = format!("{case}/{producer}.{extension}");
                let legacy = decode(&fixture(case, producer, extension), encoding);
                assert!(legacy.identity.is_none(), "{label} has no envelope");
                assert_eq!(legacy.source_fingerprint, fingerprint, "{label}");
                assert_eq!(
                    legacy.status(&lens),
                    Ok(IdentityStatus::Legacy { fingerprint, width }),
                    "{label}"
                );

                let restored = put(&lens, &view, &legacy)
                    .unwrap_or_else(|e| panic!("{label} puts under AllowLegacy: {e}"));
                assert!(
                    instances_equivalent(&restored, &source),
                    "{label} restores the source"
                );

                match put_checked(&lens, &view, &legacy, IdentityPolicy::Strict) {
                    Err(LensError::Identity(IdentityError::LegacyComplementRejected {
                        reason: LegacyRejectReason::PolicyStrict,
                    })) => {}
                    other => panic!("{label} under Strict: {other:?}"),
                }

                let mut upgraded = legacy.clone();
                upgraded
                    .upgrade_identity(&lens)
                    .expect("a matching legacy complement upgrades");
                assert_eq!(
                    upgraded.identity,
                    Some(lens.identity(ComplementShape::WType))
                );
                let (again, status) = put_checked(&lens, &view, &upgraded, IdentityPolicy::Strict)
                    .expect("an upgraded complement puts under Strict");
                assert_eq!(status, IdentityStatus::Verified);
                assert!(
                    instances_equivalent(&again, &source),
                    "{label} after upgrade"
                );
            }
        }
    }
}

#[test]
fn a_legacy_complement_of_another_schema_is_refused() {
    let lens = case_lens(&case_dir("rename_vertex"));
    let (view, _) = get(&lens, &case_instance("rename_vertex")).expect("get");
    let foreign = decode(&fixture("drop_field", "native", "json"), Encoding::Json);
    match put(&lens, &view, &foreign) {
        Err(LensError::Identity(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::FingerprintMismatch,
        })) => {}
        other => panic!("expected a fingerprint mismatch, got {other:?}"),
    }
    let mut upgraded = foreign;
    assert!(upgraded.upgrade_identity(&lens).is_err());
}

#[test]
fn the_two_producers_differ_only_in_the_fingerprint() {
    for (case, _, _) in FINGERPRINTS {
        let native: serde_json::Value =
            serde_json::from_slice(&fixture(case, "native", "json")).expect("json");
        let wasm: serde_json::Value =
            serde_json::from_slice(&fixture(case, "wasm32", "json")).expect("json");
        let strip = |v: &serde_json::Value| {
            let mut v = v.clone();
            v.as_object_mut()
                .expect("an object")
                .remove("source_fingerprint");
            v
        };
        assert_eq!(strip(&native), strip(&wasm), "{case}");
        assert_ne!(native, wasm, "{case}");
    }
}

/// The complement exactly as a reader that predates envelopes declares it.
#[derive(Deserialize)]
struct PreEnvelopeComplement {
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    dropped_nodes: HashMap<u32, Node>,
    dropped_arcs: Vec<(u32, u32, Edge)>,
    dropped_fans: Vec<Fan>,
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    contraction_choices: HashMap<(u32, u32), Edge>,
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    original_parent: HashMap<u32, u32>,
    #[serde(default)]
    source_fingerprint: u64,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_nested_map")]
    original_extra_fields: HashMap<u32, HashMap<String, Value>>,
    #[serde(default, with = "panproto_schema::serde_helpers::map_as_vec_default")]
    arc_edges: HashMap<(u32, u32), Edge>,
    #[serde(default)]
    arc_order: Vec<(u32, u32)>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    original_values: HashMap<u32, Option<FieldPresence>>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_set")]
    synthesized_nodes: HashSet<u32>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    contracted_into: HashMap<u32, u32>,
}

impl PreEnvelopeComplement {
    /// Assert that this reader saw every field it declares exactly as the
    /// current complement holds it.
    fn assert_reads(self, current: &Complement, label: &str) {
        let Self {
            dropped_nodes,
            dropped_arcs,
            dropped_fans,
            contraction_choices,
            original_parent,
            source_fingerprint,
            original_extra_fields,
            arc_edges,
            arc_order,
            original_values,
            synthesized_nodes,
            contracted_into,
        } = self;
        // `Node` has no `PartialEq`; its serialized form stands in for it.
        let as_json = |nodes: &HashMap<u32, Node>| {
            serde_json::to_value(nodes).expect("dropped nodes encode as JSON")
        };
        assert_eq!(
            as_json(&dropped_nodes),
            as_json(&current.dropped_nodes),
            "{label}: dropped_nodes"
        );
        assert_eq!(dropped_arcs, current.dropped_arcs, "{label}: dropped_arcs");
        assert_eq!(dropped_fans, current.dropped_fans, "{label}: dropped_fans");
        assert_eq!(
            contraction_choices, current.contraction_choices,
            "{label}: contraction_choices"
        );
        assert_eq!(
            original_parent, current.original_parent,
            "{label}: original_parent"
        );
        assert_eq!(
            source_fingerprint, current.source_fingerprint,
            "{label}: source_fingerprint"
        );
        assert_eq!(
            original_extra_fields, current.original_extra_fields,
            "{label}: original_extra_fields"
        );
        assert_eq!(arc_edges, current.arc_edges, "{label}: arc_edges");
        assert_eq!(arc_order, current.arc_order, "{label}: arc_order");
        assert_eq!(
            original_values, current.original_values,
            "{label}: original_values"
        );
        assert_eq!(
            synthesized_nodes, current.synthesized_nodes,
            "{label}: synthesized_nodes"
        );
        assert_eq!(
            contracted_into, current.contracted_into,
            "{label}: contracted_into"
        );
    }
}

/// What a reader that predates envelopes makes of a complement written
/// now: positional msgpack has fields it cannot place and is unreadable;
/// every self-describing encoding reads, with the envelope ignored and the
/// 64-bit fingerprint still there for that reader's own check.
#[test]
fn a_reader_that_predates_envelopes_reads_the_self_describing_encodings() {
    for (case, bits64, _) in FINGERPRINTS {
        let lens: Lens = case_lens(&case_dir(case));
        let (_, complement) = get(&lens, &case_instance(case)).expect("get");
        assert!(complement.identity.is_some(), "every complement is stamped");

        let positional = rmp_serde::to_vec(&complement).expect("encodes");
        assert!(rmp_serde::from_slice::<PreEnvelopeComplement>(&positional).is_err());

        let named = rmp_serde::to_vec_named(&complement).expect("encodes");
        let mut cbor = Vec::new();
        ciborium::ser::into_writer(&complement, &mut cbor).expect("encodes");
        let json = serde_json::to_vec(&complement).expect("encodes");
        assert_eq!(complement.source_fingerprint, bits64, "{case}");
        let read = [
            (
                "named msgpack",
                rmp_serde::from_slice::<PreEnvelopeComplement>(&named).expect("named reads"),
            ),
            (
                "CBOR",
                ciborium::de::from_reader::<PreEnvelopeComplement, _>(cbor.as_slice())
                    .expect("CBOR reads"),
            ),
            (
                "JSON",
                serde_json::from_slice::<PreEnvelopeComplement>(&json).expect("JSON reads"),
            ),
        ];
        for (encoding, old) in read {
            old.assert_reads(&complement, &format!("{case} as {encoding}"));
        }
    }
}
