//! Property tests for the VCS double category.
//!
//! Objects pair a schema with its data sets; commits are the horizontal arrows
//! and data migrations the vertical arrows. A square commutes when a data set
//! migrated forward through a schema change and back through its complement
//! reconstructs the original. These properties exercise that condition over
//! generated schemas and data.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{Node, WInstance};
use panproto_lens::instances_equivalent;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};
use panproto_vcs::MemStore;
use panproto_vcs::ObjectId;
use panproto_vcs::data_mig::{Fibre, TransportedData, migrate_backward, migrate_forward};
use panproto_vcs::hash::hash_schema;
use panproto_vcs::object::{DataSetObject, Object};
use panproto_vcs::square::verify_square;
use panproto_vcs::store::Store;
use proptest::prelude::*;

fn proto() -> Protocol {
    Protocol {
        name: "dc".into(),
        schema_theory: "ThDc".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec!["object".into(), "string".into()],
        constraint_sorts: vec![],
        ..Default::default()
    }
}

/// The fibre over `schema` under `protocol`.
fn fibre<'a>(schema: &'a Schema, protocol: &'a Protocol) -> Fibre<'a> {
    Fibre {
        schema_id: hash_schema(schema).unwrap(),
        schema,
        protocol_id: None,
        protocol,
    }
}

/// Transport `base` from `src` to `tgt`.
fn transport(
    store: &mut MemStore,
    base: ObjectId,
    src: Fibre<'_>,
    tgt: Fibre<'_>,
) -> TransportedData {
    let (image, complement) = migrate_forward(store, base, src, tgt).unwrap();
    TransportedData {
        base,
        image,
        complement,
    }
}

/// A `Root` object with one string field per name in `fields`.
fn schema_with_fields(fields: &[String]) -> Schema {
    let p = proto();
    let mut b = SchemaBuilder::new(&p)
        .vertex("Root", "object", None::<&str>)
        .unwrap();
    for f in fields {
        let vid = format!("Root.{f}");
        b = b.vertex(vid.as_str(), "string", None::<&str>).unwrap();
        b = b
            .edge("Root", vid.as_str(), "prop", Some(f.as_str()))
            .unwrap();
    }
    b.build().unwrap()
}

/// A single-record data set: one `Root` node conforming to `schema`.
fn root_dataset(store: &mut MemStore, schema: &Schema) -> ObjectId {
    let mut nodes = HashMap::new();
    nodes.insert(0_u32, Node::new(0, "Root"));
    let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("Root"));
    let ds = DataSetObject {
        schema_id: hash_schema(schema).unwrap(),
        data: rmp_serde::to_vec(&vec![inst]).unwrap(),
        record_count: 1,
        key: Some("rec".to_owned()),
        protocol_id: None,
    };
    store.put(&Object::DataSet(ds)).unwrap()
}

fn load_instances(store: &MemStore, id: ObjectId) -> Vec<WInstance> {
    match store.get(&id).unwrap() {
        Object::DataSet(ds) => rmp_serde::from_slice(&ds.data).unwrap(),
        other => panic!("expected data set, got {}", other.type_name()),
    }
}

/// `n` distinct field names sharing `prefix`.
fn fields(n: usize, prefix: &str) -> Vec<String> {
    (0..n).map(|i| format!("{prefix}{i}")).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]

    /// The square around a single forward migration commutes: the data
    /// reconstructs under the round-trip through its complement.
    #[test]
    fn square_commutes(base in 0usize..=2, add in 1usize..=3) {
        let src = schema_with_fields(&fields(base, "b"));
        let mut all = fields(base, "b");
        all.extend(fields(add, "a"));
        let tgt = schema_with_fields(&all);

        let mut store = MemStore::new();
        let data = root_dataset(&mut store, &src);
        let protocol = proto();
        let (src, tgt) = (fibre(&src, &protocol), fibre(&tgt, &protocol));
        let transported = transport(&mut store, data, src, tgt);
        prop_assert!(verify_square(&mut store, transported, src, tgt).is_ok());
    }

    /// Replay preserves data: migrating a data set forward and back through the
    /// replay's schema change reconstructs it, so the lift carries data rather
    /// than dropping it. Removing the `.data_ids()` lift in `replay_one` makes
    /// the rebased commit carry no data set, which this property would then
    /// observe as loss.
    #[test]
    fn replay_preserves_data(base in 0usize..=2, add in 1usize..=3) {
        let src = schema_with_fields(&fields(base, "b"));
        let mut all = fields(base, "b");
        all.extend(fields(add, "a"));
        let tgt = schema_with_fields(&all);

        let mut store = MemStore::new();
        let data = root_dataset(&mut store, &src);
        let protocol = proto();
        let (src, tgt) = (fibre(&src, &protocol), fibre(&tgt, &protocol));
        let TransportedData {
            image, complement, ..
        } = transport(&mut store, data, src, tgt);
        let restored = migrate_backward(&mut store, image, complement, src, tgt).unwrap();

        let before = load_instances(&store, data);
        let after = load_instances(&store, restored);
        prop_assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            prop_assert!(instances_equivalent(b, a));
        }
    }

    /// Squares paste horizontally: two consecutive migrations each commute, and
    /// the composite migration commutes too.
    #[test]
    fn squares_paste_horizontally(base in 0usize..=2, add1 in 1usize..=2, add2 in 1usize..=2) {
        let s0 = schema_with_fields(&fields(base, "b"));
        let mut mid_f = fields(base, "b");
        mid_f.extend(fields(add1, "m"));
        let s1 = schema_with_fields(&mid_f);
        let mut tgt_f = mid_f.clone();
        tgt_f.extend(fields(add2, "t"));
        let s2 = schema_with_fields(&tgt_f);

        let mut store = MemStore::new();
        let d0 = root_dataset(&mut store, &s0);
        let protocol = proto();
        let [f0, f1, f2] = [&s0, &s1, &s2].map(|schema| fibre(schema, &protocol));

        // First square: s0 -> s1.
        let first = transport(&mut store, d0, f0, f1);
        prop_assert!(verify_square(&mut store, first, f0, f1).is_ok());

        // Second square: s1 -> s2.
        let second = transport(&mut store, first.image, f1, f2);
        prop_assert!(verify_square(&mut store, second, f1, f2).is_ok());

        // Pasted square: s0 -> s2 directly commutes as well.
        let pasted = transport(&mut store, d0, f0, f2);
        prop_assert!(verify_square(&mut store, pasted, f0, f2).is_ok());
    }
}
