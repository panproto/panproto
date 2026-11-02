//! Published breaking-change variants retain their serde indices when new kinds are added.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::fmt;

use panproto_check::BreakingChange;
use serde::Serialize;
use serde::ser::{self, Impossible};

/// Stops serialization once the variant index has been recorded.
#[derive(Debug)]
struct Stop;

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("stop")
    }
}

impl std::error::Error for Stop {}

impl ser::Error for Stop {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

/// Records `(variant index, variant name)` of the outermost enum value.
struct VariantIndex<'a>(&'a Cell<Option<(u32, &'static str)>>);

macro_rules! refuse {
    ($($name:ident($($ty:ty),*)),* $(,)?) => {
        $(fn $name(self, $(_: $ty),*) -> Result<(), Stop> { Err(Stop) })*
    };
}

impl ser::Serializer for VariantIndex<'_> {
    type Ok = ();
    type Error = Stop;
    type SerializeSeq = Impossible<(), Stop>;
    type SerializeTuple = Impossible<(), Stop>;
    type SerializeTupleStruct = Impossible<(), Stop>;
    type SerializeTupleVariant = Impossible<(), Stop>;
    type SerializeMap = Impossible<(), Stop>;
    type SerializeStruct = Impossible<(), Stop>;
    type SerializeStructVariant = Impossible<(), Stop>;

    refuse!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
    );

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<(), Stop> {
        Err(Stop)
    }

    fn serialize_unit_variant(self, _: &'static str, i: u32, v: &'static str) -> Result<(), Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<(), Stop> {
        Err(Stop)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: &T,
    ) -> Result<(), Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Stop> {
        Err(Stop)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Stop> {
        Err(Stop)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Stop> {
        Err(Stop)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Stop> {
        Err(Stop)
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Stop> {
        Err(Stop)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }
}

/// The serde variant index and name of an enum value.
fn variant_of<T: Serialize>(value: &T) -> (u32, &'static str) {
    let cell = Cell::new(None);
    let _stopped = value.serialize(VariantIndex(&cell));
    cell.get().expect("an enum value")
}

fn boundary_variants() -> [(u32, &'static str, BreakingChange); 8] {
    [
        (
            27,
            "RenamedVertex",
            BreakingChange::RenamedVertex {
                old_id: "old".into(),
                new_id: "new".into(),
            },
        ),
        (
            28,
            "UnclassifiedChange",
            BreakingChange::UnclassifiedChange {
                category: "future".into(),
                count: 2,
            },
        ),
        (
            29,
            "KeyAdded",
            BreakingChange::KeyAdded {
                vertex_id: "item".into(),
                key_id: "id".into(),
            },
        ),
        (
            30,
            "KeyTightened",
            BreakingChange::KeyTightened {
                vertex_id: "item".into(),
                key_id: "id".into(),
            },
        ),
        (
            31,
            "ReferenceAdded",
            BreakingChange::ReferenceAdded {
                reference_id: "owner".into(),
            },
        ),
        (
            32,
            "ReferenceTightened",
            BreakingChange::ReferenceTightened {
                reference_id: "owner".into(),
            },
        ),
        (
            33,
            "TargetSetAdded",
            BreakingChange::TargetSetAdded {
                set_id: "items".into(),
            },
        ),
        (
            34,
            "TargetSetTightened",
            BreakingChange::TargetSetTightened {
                set_id: "items".into(),
            },
        ),
    ]
}

#[test]
fn new_breaking_changes_follow_the_published_fallback_variant() {
    for (index, name, change) in boundary_variants() {
        assert_eq!(variant_of(&change), (index, name), "serde index for {name}");
    }
}

#[test]
fn breaking_change_names_and_payloads_round_trip() {
    let fixtures = [
        serde_json::json!({"RenamedVertex": {"old_id": "old", "new_id": "new"}}),
        serde_json::json!({"UnclassifiedChange": {"category": "future", "count": 2}}),
        serde_json::json!({"KeyAdded": {"vertex_id": "item", "key_id": "id"}}),
        serde_json::json!({"KeyTightened": {"vertex_id": "item", "key_id": "id"}}),
        serde_json::json!({"ReferenceAdded": {"reference_id": "owner"}}),
        serde_json::json!({"ReferenceTightened": {"reference_id": "owner"}}),
        serde_json::json!({"TargetSetAdded": {"set_id": "items"}}),
        serde_json::json!({"TargetSetTightened": {"set_id": "items"}}),
    ];
    for ((_, name, change), fixture) in boundary_variants().into_iter().zip(fixtures) {
        assert_eq!(
            serde_json::to_value(&change).unwrap(),
            fixture,
            "encode {name}"
        );
        assert_eq!(
            serde_json::from_value::<BreakingChange>(fixture).unwrap(),
            change,
            "decode {name}"
        );
    }
}
