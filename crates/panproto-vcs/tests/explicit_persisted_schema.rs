//! Data can be staged against an exact persisted schema without changing
//! the schema that the commit itself carries.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use panproto_schema::{Schema, SchemaBuilder};
use panproto_vcs::store::{self, Store};
use panproto_vcs::{AddDataOptions, ObjectId, Repository, VcsError};

fn schema(field: &str, kind: &str) -> Schema {
    let protocol = panproto_protocols::atproto::protocol();
    SchemaBuilder::new(&protocol)
        .vertex("rec", "object", None)
        .unwrap()
        .vertex(field, kind, None)
        .unwrap()
        .edge("rec", field, "prop", Some(field))
        .unwrap()
        .entry("rec")
        .build()
        .unwrap()
}

fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).unwrap();
    path
}

struct SeededRepo {
    dir: tempfile::TempDir,
    repo: Repository,
    config_schema_id: ObjectId,
    record_schema_id: ObjectId,
    first_commit_id: ObjectId,
}

fn seeded_repo() -> SeededRepo {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let config_index = repo.add(&schema("configuration", "string")).unwrap();
    let config_schema_id = config_index.staged.unwrap().schema_id;
    let config = write(&dir, "p1-config.json", r#"[{"configuration":"one"}]"#);
    repo.add_data(&config, Some("p1:configuration")).unwrap();

    let record_index = repo.add(&schema("value", "integer")).unwrap();
    let record_schema_id = record_index.staged.unwrap().schema_id;
    let record = write(&dir, "p1-record.json", r#"[{"value":1}]"#);
    repo.add_data(&record, Some("p1")).unwrap();
    let first_commit_id = repo.commit("p1", "test").unwrap();

    SeededRepo {
        dir,
        repo,
        config_schema_id,
        record_schema_id,
        first_commit_id,
    }
}

#[test]
fn a_persisted_schema_can_be_reused_without_moving_head_or_staging_it() {
    let SeededRepo {
        dir,
        mut repo,
        config_schema_id,
        record_schema_id,
        first_commit_id,
    } = seeded_repo();

    let head_before = store::resolve_head(repo.store()).unwrap();
    let config = write(&dir, "p2-config.json", r#"[{"configuration":"two"}]"#);
    let index = repo
        .add_data_with_options(
            &config,
            Some("p2:configuration"),
            &AddDataOptions {
                schema_id: Some(config_schema_id),
                skip_verify: false,
            },
        )
        .unwrap();

    assert_eq!(store::resolve_head(repo.store()).unwrap(), head_before);
    assert!(
        index.staged.is_none(),
        "selecting data's schema must not stage it"
    );
    assert_eq!(index.staged_data[0].schema_id, config_schema_id);

    let record = write(&dir, "p2-record.json", r#"[{"value":2}]"#);
    let index = repo.add_data(&record, Some("p2")).unwrap();
    assert_eq!(index.staged_data[1].schema_id, record_schema_id);
    let second_commit_id = repo.commit("p2", "test").unwrap();

    let second_data = repo.data_at(&second_commit_id.to_string()).unwrap();
    assert_eq!(second_data.len(), 2);
    assert_eq!(second_data[0].schema_id, config_schema_id);
    assert_eq!(second_data[0].key.as_deref(), Some("p2:configuration"));
    assert_eq!(second_data[1].schema_id, record_schema_id);
    assert_eq!(second_data[1].key.as_deref(), Some("p2"));

    let config = write(&dir, "p3-config.json", r#"[{"configuration":"three"}]"#);
    repo.add_data_with_options(
        &config,
        Some("p3:configuration"),
        &AddDataOptions {
            schema_id: Some(config_schema_id),
            skip_verify: false,
        },
    )
    .unwrap();
    let record = write(&dir, "p3-record.json", r#"[{"value":3}]"#);
    repo.add_data(&record, Some("p3")).unwrap();
    let third_commit_id = repo.commit("p3", "test").unwrap();

    let blame = panproto_vcs::blame::blame_vertex(repo.store(), third_commit_id, "value").unwrap();
    assert_eq!(blame.commit_id, first_commit_id);

    repo.gc().unwrap();
    assert!(repo.store().has(&config_schema_id));
    assert_eq!(
        repo.decoded_data_at(&third_commit_id.to_string())
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn explicit_selection_uses_the_selected_schema_for_lifting_and_validation() {
    let SeededRepo {
        dir,
        mut repo,
        config_schema_id,
        ..
    } = seeded_repo();

    let config = write(&dir, "config.json", r#"[{"configuration":"current"}]"#);
    assert!(
        repo.add_data(&config, None).is_err(),
        "HEAD is the record schema"
    );
    repo.add_data_with_options(
        &config,
        None,
        &AddDataOptions {
            schema_id: Some(config_schema_id),
            skip_verify: false,
        },
    )
    .unwrap();

    let record = write(&dir, "record.json", r#"[{"value":4}]"#);
    let result = repo.add_data_with_options(
        &record,
        None,
        &AddDataOptions {
            schema_id: Some(config_schema_id),
            skip_verify: false,
        },
    );
    assert!(
        matches!(
            result,
            Err(VcsError::DataParseFailed { .. } | VcsError::DataValidationFailed { .. })
        ),
        "data valid under HEAD must still fail under the selected schema: {result:?}",
    );
}

#[test]
fn missing_and_wrong_kind_schema_ids_are_typed_errors_and_stage_nothing() {
    let SeededRepo {
        dir,
        mut repo,
        first_commit_id,
        ..
    } = seeded_repo();
    let data = write(&dir, "config.json", r#"[{"configuration":"current"}]"#);
    let before = repo.read_index().unwrap();

    let missing = repo.add_data_with_options(
        &data,
        None,
        &AddDataOptions {
            schema_id: Some(ObjectId::from_bytes([0; 32])),
            skip_verify: false,
        },
    );
    assert!(matches!(missing, Err(VcsError::ObjectNotFound { id: _ })));

    let wrong_kind = repo.add_data_with_options(
        &data,
        None,
        &AddDataOptions {
            schema_id: Some(first_commit_id),
            skip_verify: false,
        },
    );
    assert!(matches!(wrong_kind, Err(VcsError::WrongObjectType { .. })));

    let after = repo.read_index().unwrap();
    assert_eq!(after.staged_data.len(), before.staged_data.len());
    assert!(after.staged.is_none());
}
