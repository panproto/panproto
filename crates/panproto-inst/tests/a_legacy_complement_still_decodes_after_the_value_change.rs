//! The value variants appended for exact values, and the byte-string form
//! of `Value::Bytes`, change no complement already written: every fixture
//! in `tests/fixtures/complement_upgrade` (complements written before
//! envelopes existed, on a 64-bit host and through wasm32, in every
//! encoding) still decodes, as does each case's source instance, and the
//! complement format versions stay 1.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use panproto_inst::WInstance;
use panproto_inst::identity::{
    AnyComplement, ComplementShape, Encoding, GRAPH_COMPLEMENT_FORMAT_VERSION,
    TREE_COMPLEMENT_FORMAT_VERSION, decode_complement,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/complement_upgrade")
}

#[test]
fn every_legacy_complement_fixture_decodes_after_the_value_change() {
    assert_eq!(TREE_COMPLEMENT_FORMAT_VERSION, 1);
    assert_eq!(GRAPH_COMPLEMENT_FORMAT_VERSION, 1);
    let mut decoded = 0;
    for case in ["contract", "drop_field", "ordered_list", "rename_vertex"] {
        let dir = fixtures().join(case);
        for producer in ["native", "wasm32"] {
            for (extension, encoding) in [
                ("pos.msgpack", Encoding::MsgpackPositional),
                ("named.msgpack", Encoding::MsgpackNamed),
                ("cbor", Encoding::Cbor),
                ("json", Encoding::Json),
            ] {
                let path = dir.join(format!("{producer}.{extension}"));
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                match decode_complement(&bytes, encoding, ComplementShape::WType) {
                    Ok(AnyComplement::WType(complement)) => {
                        assert!(
                            complement.identity.is_none(),
                            "{} is legacy",
                            path.display()
                        );
                        decoded += 1;
                    }
                    Err(e) => panic!("{} decodes: {e}", path.display()),
                }
            }
        }
        let json = std::fs::read_to_string(dir.join("instance.json")).unwrap();
        let from_json: WInstance = serde_json::from_str(&json).unwrap();
        let packed = std::fs::read(dir.join("instance.msgpack")).unwrap();
        let from_msgpack: WInstance = rmp_serde::from_slice(&packed).unwrap();
        assert_eq!(
            serde_json::to_value(&from_json).unwrap(),
            serde_json::to_value(&from_msgpack).unwrap(),
            "{case}: the two instance fixtures agree"
        );
    }
    assert_eq!(decoded, 32);
}
