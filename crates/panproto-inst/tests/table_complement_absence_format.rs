//! Table format 2 preserves source absence and keeps version-1 decoding explicit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::identity::{
    AnyComplement, Encoding, IdentityField, decode_complement, decode_legacy_table_complement,
    decode_legacy_table_complements,
};
use panproto_inst::{
    CompiledMigration, ComplementShape, DroppedRow, IdentityEnvelope, IdentityError, RowId,
    TableComplement, Value,
};
use panproto_schema::{Edge, Protocol, SchemaBuilder};
use serde::Serialize;

/// The nine-field version-1 layout, independent of the current serializer.
#[derive(Serialize)]
struct LegacyTableWire<'a> {
    dropped_rows: &'a BTreeMap<RowId, DroppedRow>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    dropped_cells: &'a BTreeMap<(RowId, String), Value>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    dropped_fk_pairs: &'a BTreeMap<Edge, BTreeSet<(RowId, RowId)>>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    original_fk_edges: &'a BTreeMap<(Edge, RowId, RowId), BTreeSet<Edge>>,
    original_tables: &'a BTreeMap<RowId, Name>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    original_cells: &'a BTreeMap<(RowId, String), Value>,
    synthesized_fk_pairs: &'a BTreeSet<(Edge, RowId, RowId)>,
    id_floor: u32,
    identity: &'a Option<IdentityEnvelope>,
}

impl<'a> From<&'a TableComplement> for LegacyTableWire<'a> {
    fn from(c: &'a TableComplement) -> Self {
        Self {
            dropped_rows: &c.dropped_rows,
            dropped_cells: &c.dropped_cells,
            dropped_fk_pairs: &c.dropped_fk_pairs,
            original_fk_edges: &c.original_fk_edges,
            original_tables: &c.original_tables,
            original_cells: &c.original_cells,
            synthesized_fk_pairs: &c.synthesized_fk_pairs,
            id_floor: c.id_floor,
            identity: &c.identity,
        }
    }
}

const ENCODINGS: [Encoding; 4] = [
    Encoding::Json,
    Encoding::MsgpackNamed,
    Encoding::MsgpackPositional,
    Encoding::Cbor,
];

fn encode(value: &impl Serialize, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Json => serde_json::to_vec(value).unwrap(),
        Encoding::MsgpackNamed => rmp_serde::to_vec_named(value).unwrap(),
        Encoding::MsgpackPositional => rmp_serde::to_vec(value).unwrap(),
        Encoding::Cbor => {
            let mut bytes = Vec::new();
            ciborium::ser::into_writer(value, &mut bytes).unwrap();
            bytes
        }
    }
}

fn complement(version: u16) -> TableComplement {
    let schema = SchemaBuilder::new(&Protocol::default())
        .vertex("t", "object", None)
        .unwrap()
        .build()
        .unwrap();
    let mut identity = IdentityEnvelope::for_parts(
        &schema,
        &schema,
        &CompiledMigration::default(),
        ComplementShape::Functor,
    );
    identity.complement_format.version = version;
    let mut c = TableComplement::empty();
    c.identity = Some(identity);
    c.id_floor = 4;
    c.original_cells
        .insert((RowId(3), "g".to_owned()), Value::Int(9));
    c
}

#[test]
fn old_table_payloads_decode_explicitly_without_restamping_or_changing_bytes() {
    for stamped in [false, true] {
        let mut original = complement(1);
        if !stamped {
            original.identity = None;
        }
        for encoding in ENCODINGS {
            let bytes = encode(&LegacyTableWire::from(&original), encoding);
            let decoded = decode_legacy_table_complement(&bytes, encoding).unwrap();
            assert_eq!(decoded, original);
            assert!(decoded.original_absent_cells.is_empty());
            assert_eq!(encode(&decoded, encoding), bytes);
            if stamped {
                assert!(matches!(
                    decode_complement(&bytes, encoding, ComplementShape::Functor),
                    Err(IdentityError::Mismatch {
                        field: IdentityField::Format,
                        ..
                    })
                ));
            }
        }
    }
    assert_eq!(
        encode(
            &LegacyTableWire::from(&complement(1)),
            Encoding::MsgpackPositional
        )[0],
        0x99
    );
}

#[test]
fn table_absence_residue_round_trips_in_format_two() {
    let mut original = complement(2);
    original
        .original_absent_cells
        .insert((RowId(3), "f".to_owned()));
    for encoding in ENCODINGS {
        let bytes = encode(&original, encoding);
        let AnyComplement::Functor(decoded) =
            decode_complement(&bytes, encoding, ComplementShape::Functor).unwrap()
        else {
            panic!("a table complement must decode as a table");
        };
        assert_eq!(*decoded, original);
        assert!(matches!(
            decode_legacy_table_complement(&bytes, encoding),
            Err(IdentityError::Mismatch {
                field: IdentityField::Format,
                ..
            })
        ));
        // Version-1 verification refuses the envelope even if an old named
        // deserializer were to ignore the new field.
        assert!(matches!(
            complement(1)
                .identity
                .unwrap()
                .mismatch(original.identity.as_ref().unwrap()),
            Some(IdentityError::Mismatch {
                field: IdentityField::Format,
                ..
            })
        ));
    }
    assert_eq!(encode(&original, Encoding::MsgpackPositional)[0], 0x9a);
}

#[test]
fn legacy_table_decode_refuses_absence_residue_and_trailing_bytes() {
    let mut mislabeled = complement(1);
    mislabeled
        .original_absent_cells
        .insert((RowId(3), "f".to_owned()));
    for encoding in ENCODINGS {
        assert!(matches!(
            decode_legacy_table_complement(&encode(&mislabeled, encoding), encoding),
            Err(IdentityError::Undecodable(_))
        ));
        let mut trailing = encode(&LegacyTableWire::from(&complement(1)), encoding);
        trailing.push(0);
        assert!(matches!(
            decode_legacy_table_complement(&trailing, encoding),
            Err(IdentityError::Undecodable(_))
        ));
    }
}

#[test]
fn legacy_table_sequences_preserve_each_envelope_and_check_every_element() {
    let original = [complement(1), TableComplement::empty()];
    let old_layouts = original.each_ref().map(LegacyTableWire::from);
    for encoding in ENCODINGS {
        let bytes = encode(&old_layouts, encoding);
        let decoded = decode_legacy_table_complements(&bytes, encoding).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(encode(&decoded, encoding), bytes);
        let mixed = [complement(1), complement(2)];
        assert!(matches!(
            decode_legacy_table_complements(&encode(&mixed, encoding), encoding),
            Err(IdentityError::Mismatch {
                field: IdentityField::Format,
                ..
            })
        ));
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            decode_legacy_table_complements(&trailing, encoding),
            Err(IdentityError::Undecodable(_))
        ));
    }
}
