//! Production schema merge, migration selection and repository format against
//! the schema-merge reference.
//!
//! Every triple of schemas of the scope is merged, and the merged schema, the
//! conflicts in the order they are listed and the identified additions are
//! compared with the reference's per-element merge. The same merges are
//! rendered canonically and compared across insertion orders, across
//! processes and with a pinned digest. Every ordered pair of the scope has its
//! derived migration compared with the identity on what the pair shares, and
//! repositories are initialized, opened, written and corrupted on disk to
//! check the format marker and the identity checks.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use panproto_reference::enumerate::schema_merge::{SCHEMAS_COUNT, format_markers, schemas};
use panproto_reference::pages::migration_search::digest;
use panproto_reference::pages::schema_merge::{
    self as reference, Conflict, Identified, Merge, Schema, Side,
};
use panproto_schema::{Protocol, SchemaBuilder};
use panproto_vcs::merge::{
    ConflictResolution, IdentifiedAddition, MergeConflict, MergeResult, PushoutError,
    ResolutionStrategy, apply_resolutions, three_way_merge,
};
use panproto_vcs::{Object, Repository, Store, VcsError};
use proptest::prelude::*;

fn protocol() -> Protocol {
    Protocol {
        name: "merge".into(),
        schema_theory: "ThMerge".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Default::default()
    }
}

/// The production schema of a reference schema, its vertices and edges added
/// in ascending order, or in descending order when `reversed` holds. The
/// empty schema, which the builder refuses, is the default schema of the
/// protocol.
fn production(schema: &Schema, reversed: bool) -> panproto_schema::Schema {
    if schema.vertices.is_empty() {
        return panproto_schema::Schema {
            protocol: protocol().name,
            ..Default::default()
        };
    }
    let proto = protocol();
    let mut vertices: Vec<(&String, &String)> = schema.vertices.iter().collect();
    let mut edges: Vec<&(String, String)> = schema.edges.iter().collect();
    if reversed {
        vertices.reverse();
        edges.reverse();
    }
    let mut builder = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        builder = match builder.vertex(id, kind, None::<&str>) {
            Ok(next) => next,
            Err(error) => panic!("vertex {id} of the scope is refused: {error}"),
        };
    }
    for (src, tgt) in edges {
        builder = match builder.edge(src, tgt, "prop", None::<&str>) {
            Ok(next) => next,
            Err(error) => panic!("edge {src}->{tgt} of the scope is refused: {error}"),
        };
    }
    match builder.build() {
        Ok(schema) => schema,
        Err(error) => panic!("a schema of the scope is refused: {error}"),
    }
}

fn reference_schema(schema: &panproto_schema::Schema) -> Schema {
    Schema {
        vertices: schema
            .vertices
            .iter()
            .map(|(id, vertex)| (id.to_string(), vertex.kind.to_string()))
            .collect(),
        edges: schema
            .edges
            .keys()
            .map(|edge| {
                assert_eq!(edge.kind.as_str(), "prop");
                assert_eq!(edge.name, None);
                (edge.src.to_string(), edge.tgt.to_string())
            })
            .collect(),
    }
}

const fn reference_side(side: &panproto_vcs::merge::Side) -> Side {
    match side {
        panproto_vcs::merge::Side::Ours => Side::Ours,
        panproto_vcs::merge::Side::Theirs => Side::Theirs,
    }
}

fn reference_conflict(conflict: &MergeConflict) -> Conflict {
    match conflict {
        MergeConflict::BothModifiedVertex {
            vertex_id,
            ours_kind,
            theirs_kind,
        } => Conflict::BothModifiedVertex {
            vertex: vertex_id.clone(),
            ours: ours_kind.clone(),
            theirs: theirs_kind.clone(),
        },
        MergeConflict::BothAddedVertexDifferently {
            vertex_id,
            ours_kind,
            theirs_kind,
        } => Conflict::BothAddedVertexDifferently {
            vertex: vertex_id.clone(),
            ours: ours_kind.clone(),
            theirs: theirs_kind.clone(),
        },
        MergeConflict::DeleteModifyVertex {
            vertex_id,
            deleted_by,
        } => Conflict::DeleteModifyVertex {
            vertex: vertex_id.clone(),
            deleted_by: reference_side(deleted_by),
        },
        other => panic!("a conflict outside the scope's classes: {other:?}"),
    }
}

fn reference_identified(addition: &IdentifiedAddition) -> Identified {
    match addition {
        IdentifiedAddition::Vertex { vertex_id } => Identified::Vertex(vertex_id.clone()),
        IdentifiedAddition::Edge { edge } => {
            Identified::Edge((edge.src.to_string(), edge.tgt.to_string()))
        }
        other => panic!("an identification outside the scope: {other:?}"),
    }
}

/// A production merge as the reference states one.
fn observed(result: &MergeResult) -> Merge {
    Merge {
        merged: reference_schema(&result.merged_schema),
        conflicts: result.conflicts.iter().map(reference_conflict).collect(),
        identified: result
            .identified_additions
            .iter()
            .map(reference_identified)
            .collect(),
    }
}

fn family() -> Vec<Schema> {
    let family = schemas();
    assert_eq!(family.len(), SCHEMAS_COUNT);
    assert_eq!(family.len(), 73);
    family
}

#[test]
fn merge_agrees_with_the_reference_on_every_triple_of_the_scope() {
    let family = family();
    let built: Vec<panproto_schema::Schema> = family
        .iter()
        .map(|schema| production(schema, false))
        .collect();
    let mut compared = 0_usize;
    let mut conflicted = 0_usize;
    let mut delete_use = 0_usize;
    for (b, base) in family.iter().enumerate() {
        for (o, ours) in family.iter().enumerate() {
            for (t, theirs) in family.iter().enumerate() {
                let result = three_way_merge(&built[b], &built[o], &built[t]);
                let expected = reference::merge(base, ours, theirs);
                assert_eq!(
                    observed(&result),
                    expected,
                    "{base:?} / {ours:?} / {theirs:?}"
                );
                if expected.conflicts.is_empty() {
                    assert!(
                        expected.merged.well_formed(),
                        "{base:?} / {ours:?} / {theirs:?}"
                    );
                } else {
                    conflicted += 1;
                }
                delete_use += expected
                    .conflicts
                    .iter()
                    .filter(|conflict| {
                        matches!(conflict, Conflict::DeleteModifyVertex { vertex, .. }
                            if ours.vertices.get(vertex) == base.vertices.get(vertex)
                                || theirs.vertices.get(vertex) == base.vertices.get(vertex))
                    })
                    .count();
                compared += 1;
            }
        }
    }
    println!(
        "bound: ids=2, kinds=2; merges compared: {compared}; with conflicts: {conflicted}; \
         delete-use conflicts: {delete_use}"
    );
    assert_eq!(compared, 73 * 73 * 73);
    assert!(delete_use > 0, "the scope reaches a delete-use conflict");
}

#[test]
fn swapping_the_branches_swaps_the_sides_on_every_triple_of_the_scope() {
    let family = family();
    let built: Vec<panproto_schema::Schema> = family
        .iter()
        .map(|schema| production(schema, false))
        .collect();
    for (b, base) in family.iter().enumerate() {
        for o in 0..family.len() {
            for t in 0..o {
                let forward = observed(&three_way_merge(&built[b], &built[o], &built[t]));
                let backward = observed(&three_way_merge(&built[b], &built[t], &built[o]));
                assert_eq!(forward.merged, backward.merged, "{base:?}");
                assert_eq!(forward.identified, backward.identified, "{base:?}");
                let swapped: Vec<Conflict> =
                    backward.conflicts.iter().map(Conflict::swapped).collect();
                assert_eq!(forward.conflicts, swapped, "{base:?}");
            }
        }
    }
}

#[test]
fn conflicts_are_listed_in_their_stable_order() {
    let ids: Vec<String> = (0..8).map(|index| format!("v{index}")).collect();
    let with_kind = |kind: &str| Schema {
        vertices: ids
            .iter()
            .map(|id| (id.clone(), kind.to_owned()))
            .chain([("w".to_owned(), "object".to_owned())])
            .collect(),
        edges: BTreeSet::new(),
    };
    let base = with_kind("object");
    let mut ours = with_kind("string");
    let mut theirs = with_kind("object");
    theirs.vertices.remove("w");
    ours.edges.insert(("w".to_owned(), "v0".to_owned()));
    for id in &ids {
        theirs.vertices.insert(id.clone(), "boolean".to_owned());
    }
    let proto = Protocol {
        obj_kinds: vec!["object".into(), "string".into(), "boolean".into()],
        ..protocol()
    };
    let build = |schema: &Schema, reversed: bool| {
        let mut vertices: Vec<_> = schema.vertices.iter().collect();
        if reversed {
            vertices.reverse();
        }
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = match builder.vertex(id, kind, None::<&str>) {
                Ok(next) => next,
                Err(error) => panic!("{error}"),
            };
        }
        for (src, tgt) in &schema.edges {
            builder = match builder.edge(src, tgt, "prop", None::<&str>) {
                Ok(next) => next,
                Err(error) => panic!("{error}"),
            };
        }
        match builder.build() {
            Ok(built) => built,
            Err(error) => panic!("{error}"),
        }
    };
    let expected = reference::merge(&base, &ours, &theirs);
    assert_eq!(expected.conflicts.len(), 9);
    for reversed in [false, true, false, true] {
        let result = three_way_merge(
            &build(&base, reversed),
            &build(&ours, !reversed),
            &build(&theirs, reversed),
        );
        let mut sorted = result.conflicts.clone();
        sorted.sort();
        assert_eq!(result.conflicts, sorted, "listed in the derived order");
        assert_eq!(observed(&result).conflicts, expected.conflicts);
    }
}

/// The determinism scope: every triple whose base has at most one vertex.
fn determinism_scope() -> Vec<(Schema, Schema, Schema)> {
    let family = family();
    let mut triples = Vec::new();
    for base in family.iter().filter(|schema| schema.vertices.len() <= 1) {
        for ours in &family {
            for theirs in &family {
                triples.push((base.clone(), ours.clone(), theirs.clone()));
            }
        }
    }
    assert_eq!(triples.len(), 9 * 73 * 73);
    triples
}

/// The canonical rendering of every production merge of the determinism
/// scope, schemas built in the order `reversed` selects.
fn production_rendering(reversed: bool) -> String {
    determinism_scope()
        .iter()
        .map(|(base, ours, theirs)| {
            reference::render(&observed(&three_way_merge(
                &production(base, reversed),
                &production(ours, reversed),
                &production(theirs, reversed),
            )))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The digest of the rendering of every merge of the determinism scope.
const MERGES_DIGEST: u64 = 0x9c42_6e90_6b8c_aa43;

/// How many separate processes the merges are compared across.
const PROCESSES: usize = 4;

/// The child half: one process's digest, printed.
#[test]
fn one_process_merge_digest() {
    if std::env::var_os("PP_MERGE_DIGEST").is_none() {
        return;
    }
    print!(
        "<<<{:016x}>>>",
        digest(production_rendering(false).as_bytes())
    );
}

fn child_digest(exe: &std::path::Path) -> String {
    let output = match Command::new(exe)
        .args(["one_process_merge_digest", "--exact", "--nocapture"])
        .env("PP_MERGE_DIGEST", "1")
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("the test binary does not re-run: {error}"),
    };
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (Some(start), Some(end)) = (text.find("<<<"), text.find(">>>")) else {
        panic!("the child printed no digest: {text}");
    };
    text[start + 3..end].to_owned()
}

#[test]
fn merges_are_the_same_in_every_process_and_insertion_order() {
    if std::env::var_os("PP_MERGE_DIGEST").is_some() {
        return;
    }
    let forward = production_rendering(false);
    assert_eq!(
        forward,
        production_rendering(true),
        "insertion order changed a merge"
    );
    let expected = determinism_scope()
        .iter()
        .map(|(base, ours, theirs)| reference::render(&reference::merge(base, ours, theirs)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(forward, expected, "the rendering is the reference's");
    let here = format!("{:016x}", digest(forward.as_bytes()));
    let Ok(exe) = std::env::current_exe() else {
        panic!("the test binary does not know its own path");
    };
    for _ in 0..PROCESSES {
        assert_eq!(
            child_digest(&exe),
            here,
            "a child process merged differently"
        );
    }
    assert_eq!(
        here,
        format!("{MERGES_DIGEST:016x}"),
        "the merges moved from the pinned digest"
    );
}

#[test]
fn derived_migrations_select_the_identity_on_what_the_pair_shares() {
    let family = family();
    let mut renames = 0_usize;
    for old in &family {
        for new in &family {
            let (old_built, new_built) = (production(old, false), production(new, false));
            let diff = panproto_check::diff::diff(&old_built, &new_built);
            let migration = panproto_vcs::auto_mig::derive_migration(&old_built, &new_built, &diff);
            let vertices: BTreeMap<String, String> = migration
                .vertex_map
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect();
            let edges: BTreeMap<(String, String), (String, String)> = migration
                .edge_map
                .iter()
                .map(|(from, to)| {
                    (
                        (from.src.to_string(), from.tgt.to_string()),
                        (to.src.to_string(), to.tgt.to_string()),
                    )
                })
                .collect();
            let (identity, shared) = reference::surviving_identity(old, new);
            let context = format!("{old:?} -> {new:?}");
            if reference::may_rename(old, new) {
                renames += 1;
                for (from, to) in &identity {
                    if old.vertices.get(from) == new.vertices.get(from) {
                        assert_eq!(vertices.get(from), Some(to), "{context}: a survivor moved");
                    }
                }
                let images: BTreeSet<&String> = vertices.values().collect();
                assert_eq!(images.len(), vertices.len(), "{context}: not injective");
            } else {
                assert_eq!(vertices, identity, "{context}: vertex map");
                let expected: BTreeMap<_, _> = shared
                    .into_iter()
                    .map(|edge| (edge.clone(), edge))
                    .collect();
                assert_eq!(edges, expected, "{context}: edge map");
            }
        }
    }
    println!("pairs: {}; pairs that may rename: {renames}", 73 * 73);
    assert_eq!(renames, 32);
}

/// The scope's schemas with at least one vertex, which a repository stages.
fn stageable() -> Vec<Schema> {
    family()
        .into_iter()
        .filter(|schema| !schema.vertices.is_empty())
        .collect()
}

fn stage_and_commit(repo: &mut Repository, schema: &panproto_schema::Schema) {
    if let Err(error) = repo.add(schema) {
        panic!("staging refused: {error}");
    }
    if let Err(error) = repo.commit("change", "tester") {
        panic!("commit refused: {error}");
    }
}

/// Stage `old`, commit, stage `new`, and check what is recorded.
fn assert_staging_selects(old: &Schema, new: &Schema) {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("no temporary directory");
    };
    let mut repo = match Repository::init(dir.path()) {
        Ok(repo) => repo,
        Err(error) => panic!("init refused: {error}"),
    };
    let (old_built, new_built) = (production(old, false), production(new, false));
    stage_and_commit(&mut repo, &old_built);
    let head = match repo.log(None) {
        Ok(log) => log[0].clone(),
        Err(error) => panic!("log refused: {error}"),
    };
    assert_eq!(
        head.migration_id, None,
        "a first commit records no migration"
    );
    let staged = repo.add(&new_built);
    if old == new {
        assert!(
            matches!(&staged, Err(VcsError::ValidationFailed { reasons })
                if reasons == &vec!["no changes detected".to_owned()]),
            "{old:?}: an unchanged schema is refused: {staged:?}"
        );
        return;
    }
    let index = match staged {
        Ok(index) => index,
        Err(error) => panic!("{old:?} -> {new:?}: staging refused: {error}"),
    };
    let Some(migration_id) = index.staged.and_then(|staged| staged.migration_id) else {
        panic!("{old:?} -> {new:?}: no migration selected");
    };
    let store = repo.store();
    let Ok(Object::Migration { src, tgt, mapping }) = store.get(&migration_id) else {
        panic!("{old:?} -> {new:?}: the migration object does not read back");
    };
    let flat = |schema: &panproto_schema::Schema| match panproto_vcs::hash::object_id(
        &Object::FlatSchema(Box::new(schema.clone())),
    ) {
        Ok(id) => id,
        Err(error) => panic!("{error}"),
    };
    assert_eq!(src, flat(&old_built), "the migration names the head schema");
    assert_eq!(
        tgt,
        flat(&new_built),
        "the migration names the staged schema"
    );
    let diff = panproto_check::diff::diff(&old_built, &new_built);
    let derived = panproto_vcs::auto_mig::derive_migration(&old_built, &new_built, &diff);
    assert_eq!(mapping.vertex_map, derived.vertex_map, "{old:?} -> {new:?}");
    assert_eq!(mapping.edge_map, derived.edge_map, "{old:?} -> {new:?}");
}

#[test]
fn staging_records_the_selected_migration_between_the_two_schema_ids() {
    let family = stageable();
    let picks = [0, 3, 17, 40, 71];
    for &old in &picks {
        for &new in &picks {
            assert_staging_selects(&family[old], &family[new]);
        }
    }
}

/// Open the repository at `path` with `content` as its format marker and
/// compare the outcome with the reference; whether it opened.
fn assert_marker_read(path: &std::path::Path, content: &[u8]) -> bool {
    match (
        Repository::open(path).map(|_| ()),
        reference::repository_format(Some(content)),
    ) {
        (Ok(()), Some(1)) => true,
        (Err(VcsError::UnsupportedRepositoryFormat { found, supported }), None) => {
            assert_eq!(found, String::from_utf8_lossy(content));
            assert_eq!(supported, 1);
            false
        }
        (outcome, expected) => panic!("{content:?}: {outcome:?} against {expected:?}"),
    }
}

#[test]
fn a_repository_records_and_checks_its_format_version() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("no temporary directory");
    };
    let marker = dir.path().join(".panproto/format");
    let read_marker = || std::fs::read(&marker).ok();

    let mut repo = match Repository::init(dir.path()) {
        Ok(repo) => repo,
        Err(error) => panic!("init refused: {error}"),
    };
    assert_eq!(read_marker(), Some(b"1\n".to_vec()));
    assert_eq!(
        repo.store().format_version(),
        panproto_vcs::REPOSITORY_FORMAT_VERSION
    );
    assert!(matches!(
        Repository::init(dir.path()),
        Err(VcsError::AlreadyARepository)
    ));
    stage_and_commit(&mut repo, &production(&stageable()[0], false));

    let markers = format_markers(3);
    assert_eq!(markers.len(), 260);
    let mut accepted = 0_usize;
    for content in markers.iter().flatten() {
        if let Err(error) = std::fs::write(&marker, content) {
            panic!("{error}");
        }
        accepted += usize::from(assert_marker_read(dir.path(), content));
    }
    assert_eq!(accepted, 1, "one present marker is accepted");

    // A repository written before the marker existed reads as version 1,
    // a read leaves it unmarked, and the first write marks it.
    if let Err(error) = std::fs::remove_file(&marker) {
        panic!("{error}");
    }
    assert_eq!(reference::repository_format(None), Some(1));
    let mut legacy = match Repository::open(dir.path()) {
        Ok(repo) => repo,
        Err(error) => panic!("an unmarked repository is refused: {error}"),
    };
    if let Err(error) = legacy.log(None) {
        panic!("{error}");
    }
    assert_eq!(read_marker(), None, "a read writes no marker");
    stage_and_commit(&mut legacy, &production(&stageable()[5], false));
    assert_eq!(
        read_marker(),
        Some(b"1\n".to_vec()),
        "a write marks the repository"
    );
}

#[test]
fn a_stored_object_that_does_not_hash_to_its_id_is_refused() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("no temporary directory");
    };
    let mut repo = match Repository::init(dir.path()) {
        Ok(repo) => repo,
        Err(error) => panic!("init refused: {error}"),
    };
    let (first, second) = (
        Object::FlatSchema(Box::new(production(&stageable()[0], false))),
        Object::FlatSchema(Box::new(production(&stageable()[9], false))),
    );
    let (Ok(first_id), Ok(second_id)) =
        (repo.store_mut().put(&first), repo.store_mut().put(&second))
    else {
        panic!("put refused");
    };
    let path = |id: &panproto_vcs::ObjectId| {
        let hex = id.to_string();
        dir.path()
            .join(".panproto/objects")
            .join(&hex[..2])
            .join(&hex[2..])
    };
    let Ok(bytes) = std::fs::read(path(&second_id)) else {
        panic!("the object file does not read");
    };
    if let Err(error) = std::fs::write(path(&first_id), bytes) {
        panic!("{error}");
    }
    match repo.store().get(&first_id) {
        Err(VcsError::ObjectCorrupted { id, actual }) => {
            assert_eq!(id, first_id);
            assert_eq!(actual, second_id);
        }
        other => panic!("a substituted object read back: {other:?}"),
    }
}

#[test]
fn resolving_every_conflict_of_the_scope_leaves_a_well_formed_schema() {
    let family = family();
    let built: Vec<panproto_schema::Schema> = family
        .iter()
        .map(|schema| production(schema, false))
        .collect();
    let mut resolved = 0_usize;
    let mut refused = BTreeMap::new();
    for b in 0..family.len() {
        for o in 0..family.len() {
            for t in 0..family.len() {
                let result = three_way_merge(&built[b], &built[o], &built[t]);
                if result.conflicts.is_empty() {
                    continue;
                }
                for choice in [
                    ConflictResolution::ChooseOurs,
                    ConflictResolution::ChooseTheirs,
                ] {
                    let strategy = ResolutionStrategy {
                        resolutions: (0..result.conflicts.len())
                            .map(|index| (index, choice.clone()))
                            .collect(),
                    };
                    match apply_resolutions(&built[b], &built[o], &built[t], &result, &strategy) {
                        Ok(merge) => {
                            let schema = reference_schema(&merge.schema);
                            assert!(schema.well_formed(), "{schema:?}");
                            let chosen = match choice {
                                ConflictResolution::ChooseOurs => &family[o],
                                ConflictResolution::ChooseTheirs => &family[t],
                            };
                            for conflict in &result.conflicts {
                                let MergeConflict::DeleteModifyVertex { vertex_id, .. } = conflict
                                else {
                                    continue;
                                };
                                assert_eq!(
                                    schema.vertices.get(vertex_id),
                                    chosen.vertices.get(vertex_id),
                                    "the chosen side's vertex"
                                );
                            }
                            resolved += 1;
                        }
                        // Choosing a side that deleted a vertex the other side
                        // keeps leaves the other side's migration partial,
                        // which the cocone check refuses by name.
                        Err(PushoutError::MigrationNotTotal { side, .. }) => {
                            *refused.entry(side).or_insert(0_usize) += 1;
                        }
                        Err(error) => panic!("an unexpected refusal: {error}"),
                    }
                }
            }
        }
    }
    println!("resolved: {resolved}; refused: {refused:?}");
    assert!(resolved > 0);
}

fn schema_strategy() -> impl Strategy<Value = Schema> {
    let ids = ["a", "b", "c", "d"];
    prop::collection::btree_map(
        prop::sample::select(ids.to_vec()),
        prop::sample::select(vec!["object", "string"]),
        1..=4,
    )
    .prop_flat_map(|vertices| {
        let names: Vec<String> = vertices.keys().map(|id| (*id).to_owned()).collect();
        let kinds: BTreeMap<String, String> = vertices
            .iter()
            .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
            .collect();
        prop::collection::btree_set(
            (
                prop::sample::select(names.clone()),
                prop::sample::select(names),
            ),
            0..=5,
        )
        .prop_map(move |edges| Schema {
            vertices: kinds.clone(),
            edges,
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn randomized_merges_agree_with_the_reference(
        base in schema_strategy(),
        ours in schema_strategy(),
        theirs in schema_strategy(),
    ) {
        let result = three_way_merge(
            &production(&base, false),
            &production(&ours, true),
            &production(&theirs, false),
        );
        let expected = reference::merge(&base, &ours, &theirs);
        prop_assert_eq!(&observed(&result), &expected);
        if expected.conflicts.is_empty() {
            prop_assert!(expected.merged.well_formed());
        }
    }

    #[test]
    fn randomized_insertion_orders_and_swapped_branches_give_the_same_merge(
        base in schema_strategy(),
        ours in schema_strategy(),
        theirs in schema_strategy(),
    ) {
        let forward = observed(&three_way_merge(
            &production(&base, false),
            &production(&ours, false),
            &production(&theirs, false),
        ));
        let reordered = observed(&three_way_merge(
            &production(&base, true),
            &production(&ours, true),
            &production(&theirs, true),
        ));
        prop_assert_eq!(&forward, &reordered);
        let swapped = observed(&three_way_merge(
            &production(&base, true),
            &production(&theirs, false),
            &production(&ours, true),
        ));
        prop_assert_eq!(&forward.merged, &swapped.merged);
        let back: Vec<Conflict> = swapped.conflicts.iter().map(Conflict::swapped).collect();
        prop_assert_eq!(&forward.conflicts, &back);
    }

    #[test]
    fn randomized_derived_migrations_keep_every_survivor(
        old in schema_strategy(),
        new in schema_strategy(),
    ) {
        let (old_built, new_built) = (production(&old, false), production(&new, true));
        let diff = panproto_check::diff::diff(&old_built, &new_built);
        let migration = panproto_vcs::auto_mig::derive_migration(&old_built, &new_built, &diff);
        let vertices: BTreeMap<String, String> = migration
            .vertex_map
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();
        let (identity, _) = reference::surviving_identity(&old, &new);
        if reference::may_rename(&old, &new) {
            for (from, to) in &identity {
                if old.vertices.get(from) == new.vertices.get(from) {
                    prop_assert_eq!(vertices.get(from), Some(to));
                }
            }
            let images: BTreeSet<&String> = vertices.values().collect();
            prop_assert_eq!(images.len(), vertices.len());
        } else {
            prop_assert_eq!(vertices, identity);
        }
    }

    #[test]
    fn randomized_format_markers_are_read_as_the_reference_reads_them(
        content in prop::collection::vec(any::<u8>(), 0..=4),
    ) {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temporary directory");
        };
        if let Err(error) = Repository::init(dir.path()) {
            panic!("init refused: {error}");
        }
        if let Err(error) = std::fs::write(dir.path().join(".panproto/format"), &content) {
            panic!("{error}");
        }
        assert_marker_read(dir.path(), &content);
    }
}
