//! A complement's kind: its wire names, its format versions, the instance
//! shape it belongs to, and how decoding treats a kind this build has no
//! complement type for.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use panproto_inst::identity::{
    AnyComplement, ComplementFormat, Encoding, IdentityField, decode_complement,
};
use panproto_inst::{Complement, ComplementShape, IdentityError, InstanceShapeKind};

#[test]
fn complement_shape_maps_to_instance_shape_kind() {
    assert_eq!(
        InstanceShapeKind::try_from(ComplementShape::WType),
        Ok(InstanceShapeKind::Tree)
    );
    assert_eq!(
        InstanceShapeKind::try_from(ComplementShape::Functor),
        Ok(InstanceShapeKind::Table)
    );
    assert_eq!(
        InstanceShapeKind::try_from(ComplementShape::Graph),
        Ok(InstanceShapeKind::Graph)
    );
    // A resolution relates two instances, each of any shape, so it belongs
    // to none; the conversion hands the kind back.
    assert_eq!(
        InstanceShapeKind::try_from(ComplementShape::Resolution),
        Err(ComplementShape::Resolution)
    );
    for shape in ComplementShape::ALL {
        assert_eq!(
            shape.instance_shape(),
            InstanceShapeKind::try_from(shape).ok(),
            "{shape}"
        );
    }
}

#[test]
fn complement_shape_keeps_its_serde_names() {
    for (shape, name) in [
        (ComplementShape::WType, "wtype"),
        (ComplementShape::Graph, "graph"),
        (ComplementShape::Functor, "functor"),
        (ComplementShape::Resolution, "resolution"),
    ] {
        assert_eq!(
            serde_json::to_string(&shape).unwrap(),
            format!("\"{name}\"")
        );
        assert_eq!(
            serde_json::from_str::<ComplementShape>(&format!("\"{name}\"")).unwrap(),
            shape
        );
        assert_eq!(shape.to_string(), name);
        let msgpack = rmp_serde::to_vec(&shape).unwrap();
        assert_eq!(
            rmp_serde::from_slice::<ComplementShape>(&msgpack).unwrap(),
            shape
        );
    }
    assert_eq!(ComplementShape::ALL.len(), 4);
}

#[test]
fn complement_shape_functor_serializes_as_functor() {
    assert_eq!(
        serde_json::to_string(&ComplementShape::Functor).unwrap(),
        r#""functor""#
    );
    let format = ComplementShape::Functor.current_format();
    assert_eq!(format.to_string(), "functor/1");
    assert_eq!(
        serde_json::to_string(&format).unwrap(),
        r#"{"shape":"functor","version":1}"#
    );
    assert_eq!(
        serde_json::from_str::<ComplementFormat>(r#"{"shape":"functor","version":1}"#).unwrap(),
        format
    );
}

#[test]
fn complement_shape_resolution_serializes_as_resolution() {
    assert_eq!(
        serde_json::to_string(&ComplementShape::Resolution).unwrap(),
        r#""resolution""#
    );
    let format = ComplementShape::Resolution.current_format();
    assert_eq!(format.to_string(), "resolution/1");
    assert_eq!(
        serde_json::to_string(&format).unwrap(),
        r#"{"shape":"resolution","version":1}"#
    );
}

/// Tree complement bytes asked for as a table or resolution complement are a
/// format mismatch naming both formats, in every encoding.
#[test]
fn a_tree_complement_is_not_a_table_or_resolution_complement() {
    let c = Complement::empty();
    for (encoding, bytes) in [
        (Encoding::Json, serde_json::to_vec(&c).unwrap()),
        (Encoding::MsgpackNamed, rmp_serde::to_vec_named(&c).unwrap()),
        (Encoding::MsgpackPositional, rmp_serde::to_vec(&c).unwrap()),
    ] {
        for (expected, name) in [
            (ComplementShape::Functor, "functor/1"),
            (ComplementShape::Resolution, "resolution/1"),
        ] {
            assert_eq!(
                decode_complement(&bytes, encoding, expected).unwrap_err(),
                IdentityError::Mismatch {
                    field: IdentityField::Format,
                    expected: name.into(),
                    found: "wtype/1".into(),
                },
                "{encoding:?} as {expected}"
            );
        }
    }
}

/// Bytes that are no complement at all are undecodable, and the message
/// names every kind it tried, including unsupported complement types.
#[test]
fn undecodable_bytes_name_every_kind_tried() {
    let err = decode_complement(
        b"not a complement",
        Encoding::Json,
        ComplementShape::Functor,
    )
    .unwrap_err();
    let IdentityError::Undecodable(message) = err else {
        panic!("expected Undecodable, got {err:?}");
    };
    for shape in ComplementShape::ALL {
        assert!(message.contains(&format!("as {shape}:")), "{message}");
    }
    assert!(
        message.starts_with("as functor:"),
        "the expected kind comes first: {message}"
    );
}

fn complement_upgrade_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/complement_upgrade")
}

/// Every frozen 0.74.4 complement, from both producers and in every
/// encoding, still decodes as a tree complement.
#[test]
fn every_frozen_complement_still_decodes_as_a_tree_complement() {
    let mut decoded = 0;
    for case in ["contract", "drop_field", "ordered_list", "rename_vertex"] {
        for producer in ["native", "wasm32"] {
            for (extension, encoding) in [
                ("pos.msgpack", Encoding::MsgpackPositional),
                ("named.msgpack", Encoding::MsgpackNamed),
                ("cbor", Encoding::Cbor),
                ("json", Encoding::Json),
            ] {
                let path = complement_upgrade_dir()
                    .join(case)
                    .join(format!("{producer}.{extension}"));
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                match decode_complement(&bytes, encoding, ComplementShape::WType) {
                    Ok(AnyComplement::WType(c)) => {
                        assert!(c.identity.is_none(), "{} has no envelope", path.display());
                        assert_ne!(c.source_fingerprint, 0, "{}", path.display());
                        decoded += 1;
                    }
                    Ok(AnyComplement::Functor(_)) => {
                        panic!("{} decoded as a table complement", path.display())
                    }
                    Err(e) => panic!("{}: {e}", path.display()),
                }
            }
        }
    }
    assert_eq!(decoded, 32);
}
