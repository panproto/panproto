#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Structural laws of the reference lens-path free category.

use std::cell::RefCell;

use panproto_reference::path::{
    LensPath, PathComplement, PathError, PathIdentityField, PathStage, PathStageError,
    SchemaEndpoint, StageIndex, get_path, put_path,
};
use panproto_reference::schema::Schema;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stage(&'static str);

fn schema(name: &str) -> Schema {
    Schema::default().vertex(name, "obj")
}

fn stage(name: &'static str, src: &Schema, tgt: &Schema) -> PathStage<Stage> {
    PathStage::new(
        Stage(name),
        src,
        tgt,
        name.as_bytes().to_vec(),
        format!("envelope:{name}").into_bytes(),
    )
}

fn stage_with_protocols(
    name: &'static str,
    src: &Schema,
    src_protocol: &str,
    tgt: &Schema,
    tgt_protocol: &str,
) -> PathStage<Stage> {
    PathStage::new_with_protocols(
        Stage(name),
        src,
        src_protocol,
        tgt,
        tgt_protocol,
        name.as_bytes().to_vec(),
        format!("envelope:{name}").into_bytes(),
    )
}

#[test]
fn identities_at_different_schemas_do_not_compose() {
    let a = schema("a");
    let b = schema("b");

    assert!(matches!(
        LensPath::<Stage>::identity(&a).then(&LensPath::identity(&b)),
        Err(PathError::CompositionMismatch)
    ));
}

#[test]
fn shared_boundaries_quotient_validation_annotations() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let mut annotated_b = b.clone();
    annotated_b.constraints.insert(
        "b".to_owned(),
        vec![("format".to_owned(), "tagged".to_owned())],
    );
    annotated_b.entries.push("b".to_owned());

    let left = LensPath::single(stage("ab", &a, &b));
    let right = LensPath::single(stage("bc", &annotated_b, &c));
    assert!(left.then(&right).is_ok());
}

#[test]
fn shared_boundaries_retain_protocol_identity() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let left = LensPath::single(stage_with_protocols("ab", &a, "p", &b, "p"));
    let right = LensPath::single(stage_with_protocols("bc", &b, "q", &c, "q"));

    assert!(matches!(
        left.then(&right),
        Err(PathError::CompositionMismatch)
    ));
}

#[test]
fn an_empty_nonidentity_path_is_refused() {
    assert!(matches!(
        LensPath::<Stage>::try_from_stages(vec![]),
        Err(PathError::EmptyNonIdentity)
    ));
}

#[test]
fn then_refuses_an_incompatible_shared_boundary() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let d = schema("d");
    let left = LensPath::single(stage("ab", &a, &b));
    let right = LensPath::single(stage("cd", &c, &d));

    assert!(matches!(
        left.then(&right),
        Err(PathError::CompositionMismatch)
    ));
    assert!(matches!(
        LensPath::identity(&c).then(&left),
        Err(PathError::CompositionMismatch)
    ));
    assert!(matches!(
        left.then(&LensPath::identity(&c)),
        Err(PathError::CompositionMismatch)
    ));
    assert!(matches!(
        LensPath::try_from_stages(vec![stage("ab", &a, &b), stage("cd", &c, &d),]),
        Err(PathError::CompositionMismatch)
    ));
}

#[test]
fn path_put_refuses_truncated_and_extra_complements_before_running_a_stage() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let path = LensPath::single(stage("ab", &a, &b))
        .then(&LensPath::single(stage("bc", &b, &c)))
        .unwrap();
    let (view, complement) = get_path(
        &path,
        |index, _, value| Ok::<_, &'static str>((value + 1, index)),
        &0,
    )
    .unwrap();
    let calls = RefCell::new(Vec::new());
    let put = |index, _: &Stage, value: &i32, _: &usize| {
        calls.borrow_mut().push(index);
        Ok::<_, &'static str>(value - 1)
    };

    let truncated = PathComplement::new(complement.entries()[..1].to_vec());
    assert!(matches!(
        put_path(&path, &put, &view, &truncated),
        Err(PathError::PathComplementMismatch {
            expected: 2,
            found: 1
        })
    ));
    let mut extra = complement.entries().to_vec();
    extra.push(extra[1].clone());
    assert!(matches!(
        put_path(&path, &put, &view, &PathComplement::new(extra)),
        Err(PathError::PathComplementMismatch {
            expected: 2,
            found: 3
        })
    ));
    assert!(calls.borrow().is_empty());
}

#[test]
fn path_put_refuses_a_misstamped_complement_at_its_rightmost_bad_stage() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let path = LensPath::single(stage("ab", &a, &b))
        .then(&LensPath::single(stage("bc", &b, &c)))
        .unwrap();
    let (view, complement) = get_path(
        &path,
        |index, _, value| Ok::<_, &'static str>((value + 1, index)),
        &0,
    )
    .unwrap();
    let mut entries = complement.entries().to_vec();
    entries[0].stage_digests.clear();
    entries[1].envelope = b"wrong".to_vec();
    entries[1].stage_digests.clear();

    assert!(matches!(
        put_path(
            &path,
            &|_, _, value: &i32, _: &usize| Ok::<_, &'static str>(*value),
            &view,
            &PathComplement::new(entries)
        ),
        Err(PathError::AtStage {
            at: StageIndex { group: 1, stages },
            error,
        }) if stages == (1..2)
            && matches!(*error, PathStageError::IdentityMismatch {
                field: PathIdentityField::Envelope
            })
    ));

    let mut entries = complement.entries().to_vec();
    entries[1].stage_digests.clear();
    assert!(matches!(
        put_path(
            &path,
            &|_, _, value: &i32, _: &usize| Ok::<_, &'static str>(*value),
            &view,
            &PathComplement::new(entries)
        ),
        Err(PathError::AtStage {
            at: StageIndex { group: 1, stages },
            error,
        }) if stages == (1..2)
            && matches!(*error, PathStageError::IdentityMismatch {
                field: PathIdentityField::StageDigests
            })
    ));

    let mut wrong_source = complement.entries().to_vec();
    wrong_source[1].source = SchemaEndpoint::of(&a);
    let mut wrong_target = complement.entries().to_vec();
    wrong_target[1].target = SchemaEndpoint::of(&a);
    let mut wrong_digest = complement.entries().to_vec();
    wrong_digest[1].stage_digests[0] = b"wrong".to_vec();
    for (entries, field) in [
        (wrong_source, PathIdentityField::SourceSchema),
        (wrong_target, PathIdentityField::TargetSchema),
        (wrong_digest, PathIdentityField::StageDigests),
    ] {
        let error = put_path(
            &path,
            &|_, _, value: &i32, _: &usize| Ok::<_, &'static str>(*value),
            &view,
            &PathComplement::new(entries),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PathError::AtStage {
                at: StageIndex { group: 1, stages },
                error,
            } if stages == (1..2)
                && *error == PathStageError::IdentityMismatch { field }
        ));
    }
}

#[test]
fn path_get_locates_the_first_failing_stage_and_stops() {
    let source_a = schema("a");
    let source_b = schema("b");
    let source_c = schema("c");
    let source_d = schema("d");
    let path = LensPath::try_from_stages(vec![
        stage("ab", &source_a, &source_b),
        stage("bc", &source_b, &source_c),
        stage("cd", &source_c, &source_d),
    ])
    .unwrap();
    let calls = RefCell::new(Vec::new());

    assert!(matches!(
        get_path(
            &path,
            |index, _, value: &i32| {
                calls.borrow_mut().push(index);
                if index == 1 {
                    Err("refused")
                } else {
                    Ok((*value, index))
                }
            },
            &0,
        ),
        Err(PathError::AtStage {
            at: StageIndex { group: 1, stages },
            error,
        }) if stages == (1..2)
            && matches!(*error, PathStageError::Operation("refused"))
    ));
    assert_eq!(*calls.borrow(), vec![0, 1]);
}

#[test]
fn path_put_reports_the_rightmost_failing_stage() {
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let d = schema("d");
    let path = LensPath::try_from_stages(vec![
        stage("ab", &a, &b),
        stage("bc", &b, &c),
        stage("cd", &c, &d),
    ])
    .unwrap();
    let (view, complement) = get_path(
        &path,
        |index, _, value| Ok::<_, &'static str>((value + 1, index)),
        &0,
    )
    .unwrap();

    assert!(matches!(
        put_path(
            &path,
            &|index, _, value: &i32, _: &usize| if index == 0 || index == 2 {
                Err("refused")
            } else {
                Ok(*value)
            },
            &view,
            &complement,
        ),
        Err(PathError::AtStage {
            at: StageIndex { group: 2, stages },
            error,
        }) if stages == (2..3)
            && matches!(*error, PathStageError::Operation("refused"))
    ));
}

#[test]
fn identity_and_single_have_distinct_exact_invariants() {
    let a = schema("a");
    let identity = LensPath::<Stage>::identity(&a);
    assert!(identity.is_identity());
    assert!(identity.is_empty());
    assert_eq!(identity.len(), 0);
    assert_eq!(identity.src_schema(), &SchemaEndpoint::of(&a));
    assert_eq!(identity.tgt_schema(), &SchemaEndpoint::of(&a));

    let calls = RefCell::new(0);
    let (view, complement) = get_path(
        &identity,
        |_, _, _: &i32| {
            *calls.borrow_mut() += 1;
            Ok::<_, &'static str>((0, ()))
        },
        &7,
    )
    .unwrap();
    assert_eq!(view, 7);
    assert!(complement.is_empty());
    assert!(complement.is_identity_complement());
    assert_eq!(
        put_path(
            &identity,
            &|_, _, _: &i32, (): &()| Err::<i32, _>("must not run"),
            &view,
            &complement,
        ),
        Ok(7)
    );
    assert_eq!(*calls.borrow(), 0);
    let shape_calls = RefCell::new(0);
    assert!(matches!(
        put_path(
            &identity,
            &|_, _, value: &i32, (): &()| {
                *shape_calls.borrow_mut() += 1;
                Ok::<_, &'static str>(*value)
            },
            &view,
            &PathComplement::new(vec![]),
        ),
        Err(PathError::PathComplementShapeMismatch)
    ));
    assert_eq!(*shape_calls.borrow(), 0);

    let single = LensPath::single(stage("aa", &a, &a));
    assert!(!single.is_identity());
    assert!(!single.is_empty());
    assert_eq!(single.len(), 1);
}

#[test]
fn identity_complements_are_bound_to_their_exact_schema_endpoint() {
    let a = schema("a");
    let b = schema("b");
    let identity_a = LensPath::<Stage>::identity(&a);
    let identity_b = LensPath::<Stage>::identity(&b);
    let get_calls = RefCell::new(0);
    let (_, complement_a) = get_path(
        &identity_a,
        |_, _, value: &i32| {
            *get_calls.borrow_mut() += 1;
            Ok::<_, &'static str>((*value, ()))
        },
        &7,
    )
    .unwrap();
    let put_calls = RefCell::new(0);

    assert!(matches!(
        put_path(
            &identity_b,
            &|_, _, value: &i32, (): &()| {
                *put_calls.borrow_mut() += 1;
                Ok::<_, &'static str>(*value)
            },
            &7,
            &complement_a,
        ),
        Err(PathError::IdentityEndpointMismatch { expected, found })
            if expected == SchemaEndpoint::of(&b) && found == SchemaEndpoint::of(&a)
    ));
    assert_eq!((*get_calls.borrow(), *put_calls.borrow()), (0, 0));
}

#[test]
fn typed_identity_is_a_strict_two_sided_unit() {
    let a = schema("a");
    let b = schema("b");
    let path = LensPath::single(stage("ab", &a, &b));

    assert_eq!(LensPath::identity(&a).then(&path), Ok(path.clone()));
    assert_eq!(path.then(&LensPath::identity(&b)), Ok(path));
}

#[test]
fn then_is_strictly_associative_and_preserves_atomic_execution_order() {
    let source_a = schema("a");
    let source_b = schema("b");
    let source_c = schema("c");
    let source_d = schema("d");
    let path_p = LensPath::single(stage("ab", &source_a, &source_b));
    let path_q = LensPath::single(stage("bc", &source_b, &source_c));
    let path_r = LensPath::single(stage("cd", &source_c, &source_d));
    let left = path_p.then(&path_q).unwrap().then(&path_r).unwrap();
    let right = path_p.then(&path_q.then(&path_r).unwrap()).unwrap();

    assert_eq!(left, right);
    assert_eq!(left.src_schema(), &SchemaEndpoint::of(&source_a));
    assert_eq!(left.tgt_schema(), &SchemaEndpoint::of(&source_d));
    let order = RefCell::new(Vec::new());
    let (view, complement) = get_path(
        &left,
        |index, stage, value| {
            order.borrow_mut().push(("get", index, stage.0));
            let delta = match index {
                0 => 1,
                1 => 2,
                _ => 3,
            };
            Ok::<_, &'static str>((value + delta, delta))
        },
        &0,
    )
    .unwrap();
    let restored = put_path(
        &left,
        &|index, stage, value: &i32, delta: &i32| {
            order.borrow_mut().push(("put", index, stage.0));
            Ok::<_, &'static str>(value - delta)
        },
        &view,
        &complement,
    )
    .unwrap();

    assert_eq!(restored, 0);
    assert_eq!(
        *order.borrow(),
        vec![
            ("get", 0, "ab"),
            ("get", 1, "bc"),
            ("get", 2, "cd"),
            ("put", 2, "cd"),
            ("put", 1, "bc"),
            ("put", 0, "ab"),
        ]
    );
}
