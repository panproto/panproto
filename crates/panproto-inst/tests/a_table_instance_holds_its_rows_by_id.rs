#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A table instance holds its rows by an instance-wide row id: decoding
//! checks every invariant, rows enumerate in id order, fresh ids start above
//! the largest in use, and the canonical bytes are record-sequential.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::identity::{Digest, GRAPH_INSTANCE_CONTEXT, TABLE_INSTANCE_CONTEXT};
use panproto_inst::{
    CanonicalSink, CompiledMigration, DigestHasher, FInstance, InstError, KeyLookupError, Row,
    RowId, Value, canonical_finstance_bytes, functor_extend, write_finstance, write_pair_record,
    write_row_record,
};
use panproto_schema::{CanonicalWriter, Edge};

fn fk(src: &str, tgt: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: None,
    }
}

fn named(value: &str) -> Row {
    HashMap::from([("name".to_owned(), Value::Str(value.into()))])
}

/// Users 5 and 2, posts 9 and 7; post 9 by user 5, post 7 by users 2 and 5.
fn sample() -> FInstance {
    FInstance::new()
        .with_rows("user", [(RowId(5), named("ada")), (RowId(2), named("bob"))])
        .and_then(|f| f.with_rows("post", [(RowId(9), named("p")), (RowId(7), named("q"))]))
        .and_then(|f| {
            f.with_foreign_key(
                fk("post", "user"),
                [
                    (RowId(9), RowId(5)),
                    (RowId(7), RowId(5)),
                    (RowId(7), RowId(2)),
                ],
            )
        })
        .unwrap()
}

#[test]
fn a_row_id_in_two_tables_is_refused_on_decode() {
    let json = r#"{"tables":{"post":{"3":{}},"user":{"3":{}}},"foreign_keys":[]}"#;
    let err = serde_json::from_str::<FInstance>(json).expect_err("row 3 is in two tables");
    assert!(err.to_string().contains("row 3 appears in tables"), "{err}");
    let err = FInstance::from_parts(
        [
            (Name::from("user"), vec![(RowId(3), Row::new())]),
            (Name::from("post"), vec![(RowId(3), Row::new())]),
        ],
        [],
    )
    .expect_err("row 3 is in two tables");
    assert!(matches!(
        err,
        InstError::DuplicateRowId { row: RowId(3), ref tables }
            if tables == &[Name::from("post"), Name::from("user")]
    ));
    let mut f = sample();
    assert!(matches!(
        f.insert_row("post", RowId(5), Row::new()),
        Err(InstError::DuplicateRowId { row: RowId(5), .. })
    ));
}

#[test]
fn a_foreign_key_to_a_missing_row_is_refused_on_decode() {
    let json = r#"{"tables":{"post":{"1":{}},"user":{"0":{}}},
        "foreign_keys":[[{"src":"post","tgt":"user","kind":"fk","name":null},[[1,4]]]]}"#;
    let err = serde_json::from_str::<FInstance>(json).expect_err("row 4 does not exist");
    assert!(err.to_string().contains("names row 4"), "{err}");
    let mut f = sample();
    // Row 9 exists but is a post, not a user.
    assert!(matches!(
        f.insert_foreign_key(fk("post", "user"), RowId(7), RowId(9)),
        Err(InstError::DanglingForeignKey { row: RowId(9), .. })
    ));
}

#[test]
fn a_duplicate_foreign_key_pair_is_refused() {
    let mut f = sample();
    assert!(matches!(
        f.insert_foreign_key(fk("post", "user"), RowId(9), RowId(5)),
        Err(InstError::DuplicateForeignKey {
            src: RowId(9),
            tgt: RowId(5),
            ..
        })
    ));
    let json = r#"{"tables":{"post":{"1":{}},"user":{"0":{}}},
        "foreign_keys":[[{"src":"post","tgt":"user","kind":"fk","name":null},[[1,0],[1,0]]]]}"#;
    let err = serde_json::from_str::<FInstance>(json).expect_err("the pair is listed twice");
    assert!(err.to_string().contains("twice"), "{err}");
}

#[test]
fn foreign_key_pairs_are_kept_sorted() {
    let f = sample();
    assert_eq!(
        f.pairs(&fk("post", "user")),
        &[
            (RowId(7), RowId(2)),
            (RowId(7), RowId(5)),
            (RowId(9), RowId(5))
        ]
    );
    assert_eq!(f.incident_pairs(RowId(5)).len(), 2);
}

#[test]
fn a_decoded_table_instance_rebuilds_its_row_index() {
    let f = sample();
    let json = serde_json::to_string(&f).unwrap();
    let back: FInstance = serde_json::from_str(&json).unwrap();
    assert_eq!(back, f);
    assert_eq!(back.table_of(RowId(7)).map(AsRef::as_ref), Some("post"));
    assert_eq!(back.table_of(RowId(2)).map(AsRef::as_ref), Some("user"));
    assert_eq!(back.row(RowId(2)), Some(&named("bob")));
    let packed = rmp_serde::to_vec(&f).unwrap();
    let back: FInstance = rmp_serde::from_slice(&packed).unwrap();
    assert_eq!(back, f);
    assert_eq!(back.table_of(RowId(9)).map(AsRef::as_ref), Some("post"));
}

#[test]
fn rows_iterate_in_row_id_order() {
    let f = sample();
    let ids: Vec<RowId> = f.rows("user").into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![RowId(2), RowId(5)]);
    assert_eq!(f.table("post").unwrap().ids(), vec![RowId(7), RowId(9)]);
    assert_eq!(f.row_ids(), vec![RowId(2), RowId(5), RowId(7), RowId(9)]);
}

#[test]
fn fresh_row_ids_start_above_the_largest_input_row() {
    let mut f = sample();
    assert_eq!(f.fresh_row_id().unwrap(), RowId(10));
    assert_eq!(f.push_row("user", named("cy")).unwrap(), RowId(10));
    assert_eq!(f.push_row("user", named("di")).unwrap(), RowId(11));
}

#[test]
fn an_empty_input_allocates_row_zero() {
    assert_eq!(FInstance::new().fresh_row_id().unwrap(), RowId(0));
    let f = FInstance::new()
        .with_table("t", [Row::new(), Row::new()])
        .unwrap();
    assert_eq!(f.row_ids(), vec![RowId(0), RowId(1)]);
}

#[test]
fn row_id_exhaustion_is_an_error() {
    let mut f = FInstance::new()
        .with_rows("t", [(RowId(u32::MAX), Row::new())])
        .unwrap();
    assert!(matches!(f.fresh_row_id(), Err(InstError::NodeIdsExhausted)));
    assert!(matches!(
        f.push_row("t", Row::new()),
        Err(InstError::NodeIdsExhausted)
    ));
    assert_eq!(f.total_row_count(), 1, "a failed allocation adds no row");
}

#[test]
fn key_lookup_reports_a_missing_key() {
    let f = sample();
    let ada = Value::Str("ada".into());
    assert_eq!(f.find_row_by_key("user", &[("name", &ada)]), Ok(RowId(5)));
    let zed = Value::Str("zed".into());
    assert_eq!(
        f.find_row_by_key("user", &[("name", &zed)]),
        Err(KeyLookupError::Missing {
            table: Name::from("user")
        })
    );
    assert_eq!(
        f.find_row_by_key("group", &[("name", &zed)]),
        Err(KeyLookupError::Missing {
            table: Name::from("group")
        })
    );
}

#[test]
fn key_lookup_reports_a_duplicate_key() {
    let mut f = sample();
    f.insert_row("user", RowId(30), named("ada")).unwrap();
    let ada = Value::Str("ada".into());
    assert_eq!(
        f.find_row_by_key("user", &[("name", &ada)]),
        Err(KeyLookupError::Duplicate {
            table: Name::from("user"),
            rows: vec![RowId(5), RowId(30)],
        })
    );
}

/// The bytes the layout documents, written field by field.
fn expected_bytes(f: &FInstance) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    w.len(4);
    w.len(3);
    for (id, table, name) in [
        (2, "user", "bob"),
        (5, "user", "ada"),
        (7, "post", "q"),
        (9, "post", "p"),
    ] {
        w.u32(id);
        w.name(&Name::from(table));
        w.len(1);
        w.str("name");
        w.value(&Value::Str(name.into()));
    }
    for (s, t) in [(7, 2), (7, 5), (9, 5)] {
        w.edge(&fk("post", "user"));
        w.u32(s);
        w.u32(t);
    }
    assert_eq!(f.total_row_count(), 4);
    w.into_bytes()
}

#[test]
fn canonical_finstance_bytes_follow_the_record_layout() {
    let f = sample();
    assert_eq!(canonical_finstance_bytes(&f), expected_bytes(&f));
}

#[test]
fn finstance_bytes_are_record_sequential() {
    // The bytes are the header followed by each record written on its
    // own, rows ascending by id and then pairs ascending by (edge, src,
    // tgt): no record depends on another.
    let f = sample();
    let mut w = CanonicalWriter::new();
    w.len(f.total_row_count());
    w.len(f.pair_count());
    for id in [RowId(2), RowId(5), RowId(7), RowId(9)] {
        let mut one = CanonicalWriter::new();
        write_row_record(&mut one, id, f.table_of(id).unwrap(), f.row(id).unwrap());
        w.write(&one.into_bytes());
    }
    for (edge, s, t) in f.all_pairs() {
        let mut one = CanonicalWriter::new();
        write_pair_record(&mut one, &edge, s, t);
        w.write(&one.into_bytes());
    }
    assert_eq!(canonical_finstance_bytes(&f), w.into_bytes());
    // Built in another order, the instance has the same bytes.
    let reordered = FInstance::new()
        .with_rows("post", [(RowId(7), named("q")), (RowId(9), named("p"))])
        .and_then(|g| g.with_rows("user", [(RowId(2), named("bob")), (RowId(5), named("ada"))]))
        .and_then(|g| {
            g.with_foreign_key(
                fk("post", "user"),
                [
                    (RowId(9), RowId(5)),
                    (RowId(7), RowId(2)),
                    (RowId(7), RowId(5)),
                ],
            )
        })
        .unwrap();
    assert_eq!(
        canonical_finstance_bytes(&reordered),
        canonical_finstance_bytes(&f)
    );
}

#[test]
fn a_streamed_table_digest_equals_the_digest_of_its_bytes() {
    let f = sample();
    let mut hasher: DigestHasher = Digest::hasher(TABLE_INSTANCE_CONTEXT);
    write_finstance(&mut hasher, &f);
    assert_eq!(hasher.finalize(), f.digest());
    assert_eq!(
        f.digest(),
        Digest::derive(TABLE_INSTANCE_CONTEXT, &canonical_finstance_bytes(&f))
    );
}

#[test]
fn table_digest_uses_the_table_instance_context() {
    let f = sample();
    let bytes = canonical_finstance_bytes(&f);
    assert_eq!(f.digest(), Digest::derive(TABLE_INSTANCE_CONTEXT, &bytes));
    assert_ne!(f.digest(), Digest::derive(GRAPH_INSTANCE_CONTEXT, &bytes));
}

#[test]
fn functor_extend_keeps_row_ids_and_creates_no_rows() {
    let f = sample();
    let mut migration = CompiledMigration::default();
    migration.surviving_verts.insert("member".into());
    migration
        .vertex_remap
        .insert("user".into(), "member".into());
    let extended = functor_extend(&f, &migration).unwrap();
    assert_eq!(extended.row_ids(), f.row_ids());
    assert_eq!(
        extended.table("member").unwrap().ids(),
        vec![RowId(2), RowId(5)]
    );
    assert_eq!(
        extended.table("post").unwrap().ids(),
        vec![RowId(7), RowId(9)]
    );
}

/// Extension fills each target table from its fibre by padding cells,
/// never by adding rows: over a merge of two tables with different
/// columns, the merged table holds exactly the source rows under their
/// own ids, each padded with `Null` to the union of the columns, and a
/// surviving target no row reaches is the empty set, so it has no table.
#[test]
fn functor_extend_allocates_fills_by_target_table() {
    let f = FInstance::new()
        .with_rows("b", [(RowId(4), named("x"))])
        .and_then(|f| {
            f.with_rows(
                "a",
                [(RowId(1), HashMap::from([("n".to_owned(), Value::Int(1))]))],
            )
        })
        .unwrap();
    let mut migration = CompiledMigration::default();
    migration.surviving_verts.insert("t".into());
    migration.surviving_verts.insert("empty".into());
    migration.vertex_remap.insert("a".into(), "t".into());
    migration.vertex_remap.insert("b".into(), "t".into());
    let extended = functor_extend(&f, &migration).unwrap();
    assert_eq!(extended.row_ids(), vec![RowId(1), RowId(4)]);
    assert_eq!(extended.table("t").unwrap().ids(), vec![RowId(1), RowId(4)]);
    assert_eq!(
        extended.row(RowId(1)).unwrap().get("name"),
        Some(&Value::Null)
    );
    assert_eq!(extended.row(RowId(4)).unwrap().get("n"), Some(&Value::Null));
    assert!(!extended.has_table("empty"));
    assert_eq!(extended.row_count("empty"), 0);
    assert_eq!(
        extended.fresh_row_id().unwrap(),
        f.fresh_row_id().unwrap(),
        "no id was drawn"
    );
}
/// Allocating a fresh id reads a maintained maximum instead of scanning
/// every row, so building a table row by row is linear: fifty thousand
/// rows take well under the bound even in a debug build.
#[test]
fn with_table_is_linear_in_row_count() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let f = FInstance::new().with_table("t", (0..50_000).map(|_| Row::new()))?;
    let elapsed = start.elapsed();
    assert_eq!(f.total_row_count(), 50_000);
    assert_eq!(f.fresh_row_id()?, RowId(50_000));
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "building 50000 rows took {elapsed:?}"
    );
    Ok(())
}

/// A set-valued functor sends a vertex with no rows to the empty set, so
/// an empty table and an absent one are the same instance: equal, with
/// the same wire form and the same digest.
#[test]
fn an_empty_table_is_no_table() -> Result<(), Box<dyn std::error::Error>> {
    let bare = FInstance::new();
    let emptied = FInstance::new().with_table("user", [])?;
    assert_eq!(emptied, bare);
    assert_eq!(
        serde_json::to_string(&emptied)?,
        serde_json::to_string(&bare)?
    );
    assert_eq!(emptied.digest(), bare.digest());
    assert!(!emptied.has_table("user"));

    let mut f = sample();
    let before = f.clone();
    let id = f.push_row("tag", named("t"))?;
    f.remove_row(id);
    assert_eq!(f, before, "removing a table's last row removes the table");
    f.insert_row("tag", id, named("t"))?;
    f.move_row(id, "label")?;
    f.remove_row(id);
    assert_eq!(
        f, before,
        "moving a table's last row away removes the table"
    );

    let decoded: FInstance = serde_json::from_str(r#"{"tables":{"user":{}},"foreign_keys":[]}"#)?;
    assert_eq!(decoded, bare);
    Ok(())
}

#[test]
fn a_row_id_repeated_within_one_table_is_refused_on_decode() {
    let json = r#"{"tables":{"u":{"0":{},"0":{}}},"foreign_keys":[]}"#;
    let decoded = serde_json::from_str::<FInstance>(json);
    assert!(
        decoded
            .as_ref()
            .is_err_and(|e| e.to_string().contains("row 0 appears in tables")),
        "{decoded:?}"
    );
    let json = r#"{"tables":{"u":{"0":{"a":{"Int":1},"a":{"Int":2}}}},"foreign_keys":[]}"#;
    let decoded = serde_json::from_str::<FInstance>(json);
    assert!(
        decoded
            .as_ref()
            .is_err_and(|e| e.to_string().contains("column `a`")),
        "{decoded:?}"
    );
    let json = r#"{"tables":{"u":{"0":{}},"u":{"1":{}}},"foreign_keys":[]}"#;
    let decoded = serde_json::from_str::<FInstance>(json);
    assert!(
        decoded
            .as_ref()
            .is_err_and(|e| e.to_string().contains("table `u`")),
        "{decoded:?}"
    );
}
