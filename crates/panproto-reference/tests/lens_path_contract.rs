//! Exact path-word and fusion contract witnesses.

use std::cell::RefCell;
use std::collections::BTreeSet;

use panproto_reference::enumerate::lenses::{
    PATH_CONTRACT_MAX_LENGTH, PathContractFusionObstruction, PathContractKind,
    for_each_path_contract_word, path_contract_count_certificate, path_contract_fusion_coverage,
    path_contract_fusion_semantics,
};
use panproto_reference::path::{
    FusionCandidate, FusionCondition, FusionDecision, FusionNormalizationError, FusionPredicate,
    LensPath, NormalGroup, NormalStageError, PathError, PathStage, PathStageError, StageIndex,
    get_normal_form, get_path, put_normal_form, put_path,
};
use panproto_reference::schema::Schema;

fn success<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("expected success, got {error:?}"))
}

fn failure<T: std::fmt::Debug, E>(result: Result<T, E>) -> E {
    match result {
        Ok(value) => panic!("expected failure, got {value:?}"),
        Err(error) => error,
    }
}

fn schema(name: &str) -> Schema {
    Schema::default().vertex(name, "obj")
}

fn stage(kind: PathContractKind, source: &Schema, target: &Schema) -> PathStage<PathContractKind> {
    PathStage::new(
        kind,
        source,
        target,
        vec![kind as u8],
        vec![b'e', kind as u8],
    )
}

fn derive_candidate(
    left: &NormalGroup<PathContractKind>,
    right: &NormalGroup<PathContractKind>,
    fused: PathContractKind,
) -> Result<FusionCandidate<PathContractKind>, FusionCondition> {
    FusionCandidate::derive(
        left,
        right,
        fused,
        vec![fused as u8],
        vec![b'f', fused as u8],
        None,
        &path_contract_fusion_semantics(),
    )
}

#[derive(Clone, Copy)]
struct ContractFusion;

impl FusionPredicate<PathContractKind> for ContractFusion {
    type Obstruction = PathContractFusionObstruction;

    fn context(&self) -> Option<&[u8]> {
        None
    }

    fn decide(
        &self,
        left: &NormalGroup<PathContractKind>,
        right: &NormalGroup<PathContractKind>,
    ) -> FusionDecision<PathContractKind, Self::Obstruction> {
        let first = *left.stage().payload();
        let second = *right.stage().payload();
        match first.fuse(second) {
            Ok(kind) => {
                let candidate = derive_candidate(left, right, kind);
                FusionDecision::Fused(Box::new(success(candidate)))
            }
            Err(obstruction) => FusionDecision::Sequential(obstruction),
        }
    }
}

#[test]
fn path_contract_words_reconcile_formula_ordinals_and_canonical_bytes() {
    let certificate = path_contract_count_certificate();
    assert_eq!(certificate.by_length, [2, 12, 72, 432]);
    assert_eq!(certificate.total, 518);

    let mut seen = BTreeSet::new();
    let mut streamed = [0; PATH_CONTRACT_MAX_LENGTH + 1];
    for_each_path_contract_word(|length, ordinal, word| {
        assert_eq!(word.stages.len(), length);
        assert_eq!(word.ordinal(), ordinal);
        assert!(word.source < 2, "source endpoint is in the alphabet");
        assert!(word.target() < 2, "target endpoint is in the alphabet");
        assert!(
            word.stages
                .windows(2)
                .all(|pair| pair[0].target == pair[1].source),
            "adjacent endpoints compose"
        );
        assert!(
            seen.insert(word.canonical_bytes()),
            "canonical path bytes are unique"
        );
        streamed[length] += 1;
    });

    assert_eq!(streamed, certificate.by_length);
    assert_eq!(seen.len(), certificate.total);
    eprintln!(
        "path contract: endpoints=2 behaviours=3 max_length=3; by_length={:?} total={}",
        certificate.by_length, certificate.total
    );
}

#[test]
fn both_bracketings_have_one_atomic_word_and_one_leftmost_normal_form() {
    let source_schema = schema("a");
    let first_boundary = schema("b");
    let second_boundary = schema("c");
    let target_schema = schema("d");
    let first_path = LensPath::single(stage(
        PathContractKind::Clear,
        &source_schema,
        &first_boundary,
    ));
    let second_path = LensPath::single(stage(
        PathContractKind::Toggle,
        &first_boundary,
        &second_boundary,
    ));
    let third_path = LensPath::single(stage(
        PathContractKind::Identity,
        &second_boundary,
        &target_schema,
    ));
    let left = success(success(first_path.then(&second_path)).then(&third_path));
    let right = success(first_path.then(&success(second_path.then(&third_path))));

    assert_eq!(left, right);
    let left_normal = success(left.normal_form(&ContractFusion));
    let right_normal = success(right.normal_form(&ContractFusion));
    assert_eq!(left_normal, right_normal);
    assert_eq!(
        left_normal
            .groups()
            .iter()
            .map(|group| group.stages().clone())
            .collect::<Vec<_>>(),
        vec![0..1, 1..3]
    );
    assert_eq!(left_normal.obstructions().len(), 1);
    assert_eq!(left_normal.obstructions()[0].left, 0..1);
    assert_eq!(left_normal.obstructions()[0].right, 1..3);
    assert_eq!(
        left_normal.obstructions()[0].obstruction,
        PathContractFusionObstruction::ConstantOneOutsideAlphabet
    );
}

#[derive(Clone, Copy)]
struct NeverFuse;

impl FusionPredicate<PathContractKind> for NeverFuse {
    type Obstruction = PathContractFusionObstruction;

    fn context(&self) -> Option<&[u8]> {
        None
    }

    fn decide(
        &self,
        _left: &NormalGroup<PathContractKind>,
        _right: &NormalGroup<PathContractKind>,
    ) -> FusionDecision<PathContractKind, Self::Obstruction> {
        FusionDecision::Sequential(PathContractFusionObstruction::ConstantOneOutsideAlphabet)
    }
}

#[derive(Default)]
struct PairSwappingFusion {
    saved: RefCell<Option<FusionCandidate<PathContractKind>>>,
}

impl FusionPredicate<PathContractKind> for PairSwappingFusion {
    type Obstruction = PathContractFusionObstruction;

    fn context(&self) -> Option<&[u8]> {
        None
    }

    fn decide(
        &self,
        left: &NormalGroup<PathContractKind>,
        right: &NormalGroup<PathContractKind>,
    ) -> FusionDecision<PathContractKind, Self::Obstruction> {
        if let Some(candidate) = self.saved.borrow_mut().take() {
            return FusionDecision::Fused(Box::new(candidate));
        }
        let first = *left.stage().payload();
        let second = *right.stage().payload();
        let fused = success(first.fuse(second));
        let candidate = success(derive_candidate(left, right, fused));
        self.saved.replace(Some(candidate));
        FusionDecision::Sequential(PathContractFusionObstruction::ConstantOneOutsideAlphabet)
    }
}

#[test]
fn an_admitted_candidate_cannot_be_reused_for_another_adjacent_pair() {
    let a = schema("a");
    let path = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Identity, &a, &a),
    ]));

    assert_eq!(
        path.normal_form(&PairSwappingFusion::default()),
        Err(FusionNormalizationError::CandidateBindingMismatch)
    );
}

struct ReplayedCandidate(RefCell<Option<FusionCandidate<PathContractKind>>>);

impl FusionPredicate<PathContractKind> for ReplayedCandidate {
    type Obstruction = PathContractFusionObstruction;

    fn context(&self) -> Option<&[u8]> {
        None
    }

    fn decide(
        &self,
        _: &NormalGroup<PathContractKind>,
        _: &NormalGroup<PathContractKind>,
    ) -> FusionDecision<PathContractKind, Self::Obstruction> {
        let Some(candidate) = self.0.borrow_mut().take() else {
            return FusionDecision::Sequential(
                PathContractFusionObstruction::ConstantOneOutsideAlphabet,
            );
        };
        FusionDecision::Fused(Box::new(candidate))
    }
}

#[test]
fn identity_evidence_cannot_authorize_clear_toggle_as_identity() {
    let a = schema("a");
    let identity_pair = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Identity, &a, &a),
    ]));
    let identity_groups = success(identity_pair.normal_form(&NeverFuse));
    let [left, right] = identity_groups.groups() else {
        panic!("two identity groups expected")
    };
    let replayed = success(derive_candidate(left, right, PathContractKind::Identity));

    let clear_toggle = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Clear, &a, &a),
        stage(PathContractKind::Toggle, &a, &a),
    ]));
    assert_eq!(
        clear_toggle.normal_form(&ReplayedCandidate(RefCell::new(Some(replayed)))),
        Err(FusionNormalizationError::CandidateBindingMismatch)
    );
}

struct CrossContextReplay {
    candidate: RefCell<Option<FusionCandidate<PathContractKind>>>,
    context: Vec<u8>,
}

impl FusionPredicate<PathContractKind> for CrossContextReplay {
    type Obstruction = PathContractFusionObstruction;

    fn context(&self) -> Option<&[u8]> {
        Some(&self.context)
    }

    fn decide(
        &self,
        _: &NormalGroup<PathContractKind>,
        _: &NormalGroup<PathContractKind>,
    ) -> FusionDecision<PathContractKind, Self::Obstruction> {
        FusionDecision::Fused(Box::new(success(
            self.candidate
                .borrow_mut()
                .take()
                .ok_or("candidate missing"),
        )))
    }
}

#[test]
fn an_admitted_candidate_cannot_cross_boundary_contexts() {
    let a = schema("a");
    let path = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Toggle, &a, &a),
    ]));
    let groups = success(path.normal_form(&NeverFuse));
    let [left, right] = groups.groups() else {
        panic!("two groups expected")
    };
    let candidate = success(FusionCandidate::derive(
        left,
        right,
        PathContractKind::Toggle,
        vec![1],
        vec![2],
        Some(vec![1]),
        &path_contract_fusion_semantics(),
    ));
    let replay = CrossContextReplay {
        candidate: RefCell::new(Some(candidate)),
        context: vec![0],
    };
    assert_eq!(
        path.normal_form(&replay),
        Err(FusionNormalizationError::CandidateContextMismatch)
    );
}

#[test]
fn trusted_semantics_rejects_a_wrong_payload_and_false_context() {
    let a = schema("a");
    let path = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Toggle, &a, &a),
    ]));
    let normal = success(path.normal_form(&NeverFuse));
    let [left, right] = normal.groups() else {
        panic!("two atomic groups expected")
    };
    let rejected = FusionCandidate::derive(
        left,
        right,
        PathContractKind::Identity,
        vec![1],
        vec![2],
        None,
        &path_contract_fusion_semantics(),
    );
    assert_eq!(rejected.err(), Some(FusionCondition::MigrationComposes));

    let rejected_context = FusionCandidate::derive(
        left,
        right,
        PathContractKind::Toggle,
        vec![1],
        vec![2],
        Some(vec![0]),
        &path_contract_fusion_semantics(),
    );
    assert_eq!(
        rejected_context.err(),
        Some(FusionCondition::ContextCompiles)
    );
}

#[test]
fn scalar_fusion_scope_marks_five_structural_premises_as_vacuous() {
    let coverage = path_contract_fusion_coverage();
    assert_eq!(coverage.observations, [0, 2, 0, 0, 4, 0, 0, 0]);
    assert_eq!(
        (0..8)
            .map(|index| coverage.nonvacuous(index))
            .collect::<Vec<_>>(),
        vec![false, true, false, false, true, false, false, false]
    );
}

#[test]
fn every_admitted_atomic_fusion_matches_sequential_get_and_put() {
    let a = schema("a");
    let mut admitted = 0;
    let mut obstructed = 0;
    for first in PathContractKind::ALL {
        for second in PathContractKind::ALL {
            let path = success(LensPath::try_from_stages(vec![
                stage(first, &a, &a),
                stage(second, &a, &a),
            ]));
            let normal = success(path.normal_form(&ContractFusion));
            if normal.groups().len() != 1 {
                obstructed += 1;
                assert_eq!(
                    (first, second),
                    (PathContractKind::Clear, PathContractKind::Toggle)
                );
                continue;
            }
            admitted += 1;
            for source in 0..2 {
                let (sequential_view, sequential_complement) = success(get_path(
                    &path,
                    |_, kind, value| {
                        let (view, complement) = kind.get(*value);
                        Ok::<_, &'static str>((view, complement))
                    },
                    &source,
                ));
                let (fused_view, fused_complement) = success(get_normal_form(
                    &path,
                    &normal,
                    |_, group, value| {
                        let (view, complement) = group.stage().payload().get(*value);
                        Ok::<_, NormalStageError<&'static str>>((view, complement))
                    },
                    &source,
                ));
                assert_eq!(sequential_view, fused_view);
                assert_ne!(
                    sequential_complement.entries().len(),
                    fused_complement.entries().len(),
                    "the complement comparison is non-gating and non-vacuous"
                );
                for edited_view in 0..2 {
                    let sequential_source = put_path(
                        &path,
                        &|_, kind, view, complement| kind.put(*view, *complement).ok_or("domain"),
                        &edited_view,
                        &sequential_complement,
                    );
                    let fused_source = put_normal_form(
                        &path,
                        &normal,
                        |_, group, view, complement| {
                            group
                                .stage()
                                .payload()
                                .put(*view, *complement)
                                .ok_or_else(|| {
                                    NormalStageError::new(group.stages().end - 1, "domain")
                                })
                        },
                        &edited_view,
                        &fused_complement,
                    );
                    assert_eq!(sequential_source, fused_source);
                }
            }
        }
    }
    assert_eq!((admitted, obstructed), (8, 1));
}

#[test]
fn a_normal_form_is_bound_to_its_exact_atomic_path_before_execution() {
    let a = schema("a");
    let first = LensPath::single(stage(PathContractKind::Identity, &a, &a));
    let cloned_first = first.clone();
    let second = LensPath::single(PathStage::new(
        PathContractKind::Identity,
        &a,
        &a,
        vec![99],
        vec![b'e', PathContractKind::Identity as u8],
    ));
    let normal = success(first.normal_form(&ContractFusion));
    let calls = RefCell::new(0);

    let get_error = failure(get_normal_form(
        &second,
        &normal,
        |_, _, value: &usize| {
            *calls.borrow_mut() += 1;
            Ok::<_, NormalStageError<&'static str>>((*value, 2))
        },
        &0,
    ));
    assert_eq!(get_error, PathError::NormalFormMismatch);
    assert_eq!(*calls.borrow(), 0);

    let (_, complement) = success(get_normal_form(
        &cloned_first,
        &normal,
        |_, group, value: &usize| {
            let (view, complement) = group.stage().payload().get(*value);
            Ok::<_, NormalStageError<&'static str>>((view, complement))
        },
        &0,
    ));
    let put_error = failure(put_normal_form(
        &second,
        &normal,
        |_, _, value: &usize, _| {
            *calls.borrow_mut() += 1;
            Ok::<_, NormalStageError<&'static str>>(*value)
        },
        &0,
        &complement,
    ));
    assert_eq!(put_error, PathError::NormalFormMismatch);
    assert_eq!(*calls.borrow(), 0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContractRefusal {
    OutsideDomain,
}

const fn contract_get(
    _: usize,
    kind: PathContractKind,
    source: usize,
) -> Result<(usize, usize), ContractRefusal> {
    if source < 2 {
        Ok(kind.get(source))
    } else {
        Err(ContractRefusal::OutsideDomain)
    }
}

fn kind_from_digest(digest: &[u8]) -> PathContractKind {
    match digest {
        [0] => PathContractKind::Identity,
        [1] => PathContractKind::Toggle,
        [2] => PathContractKind::Clear,
        _ => panic!("unknown path-contract digest"),
    }
}

fn contract_group_get(
    _: usize,
    group: &NormalGroup<PathContractKind>,
    source: usize,
) -> Result<(usize, usize), NormalStageError<ContractRefusal>> {
    let mut current = source;
    for (stage, digest) in group.stages().clone().zip(group.atomic_stage_digests()) {
        let (next, _) = contract_get(stage, kind_from_digest(digest), current)
            .map_err(|error| NormalStageError::new(stage, error))?;
        current = next;
    }
    let (_, complement) = group.stage().payload().get(source);
    Ok((current, complement))
}

fn contract_group_put(
    _: usize,
    group: &NormalGroup<PathContractKind>,
    view: usize,
    complement: usize,
) -> Result<usize, NormalStageError<ContractRefusal>> {
    let mut current = view;
    for (stage, digest) in group
        .stages()
        .clone()
        .zip(group.atomic_stage_digests())
        .rev()
    {
        current = kind_from_digest(digest)
            .put(current, complement)
            .ok_or_else(|| NormalStageError::new(stage, ContractRefusal::OutsideDomain))?;
    }
    Ok(current)
}

#[test]
fn an_admitted_fusion_preserves_typed_get_and_put_refusals() {
    let a = schema("a");
    let path = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Identity, &a, &a),
        stage(PathContractKind::Toggle, &a, &a),
    ]));
    let normal = success(path.normal_form(&ContractFusion));
    assert_eq!(normal.groups().len(), 1);

    let sequential_get = failure(get_path(
        &path,
        |stage, kind, source| contract_get(stage, *kind, *source),
        &2,
    ));
    let fused_get = failure(get_normal_form(
        &path,
        &normal,
        |group, normal, source| contract_group_get(group, normal, *source),
        &2,
    ));
    assert_eq!(sequential_get, fused_get);

    let (_, sequential_complement) = success(get_path(
        &path,
        |stage, kind, source| contract_get(stage, *kind, *source),
        &0,
    ));
    let (_, fused_complement) = success(get_normal_form(
        &path,
        &normal,
        |group, normal, source| contract_group_get(group, normal, *source),
        &0,
    ));
    let sequential_put = failure(put_path(
        &path,
        &|_, kind, view, complement| {
            kind.put(*view, *complement)
                .ok_or(ContractRefusal::OutsideDomain)
        },
        &2,
        &sequential_complement,
    ));
    let fused_put = failure(put_normal_form(
        &path,
        &normal,
        |group, normal, view, complement| contract_group_put(group, normal, *view, *complement),
        &2,
        &fused_complement,
    ));
    assert_eq!(sequential_put, fused_put);
    assert!(matches!(
        fused_put,
        PathError::AtStage {
            at: StageIndex { group: 1, stages },
            error,
        } if stages == (1..2)
            && matches!(*error, PathStageError::Operation(ContractRefusal::OutsideDomain))
    ));
}

#[test]
fn equal_refusals_retain_the_same_atomic_location_in_a_sequential_group() {
    let a = schema("a");
    let path = success(LensPath::try_from_stages(vec![
        stage(PathContractKind::Clear, &a, &a),
        stage(PathContractKind::Toggle, &a, &a),
    ]));
    let normal = success(path.normal_form(&ContractFusion));
    assert_eq!(normal.groups().len(), 2);

    let sequential = failure(get_path(
        &path,
        |index, _, value: &usize| {
            if index == 1 {
                Err("typed refusal")
            } else {
                Ok((*value, 0))
            }
        },
        &0,
    ));
    let normalized = failure(get_normal_form(
        &path,
        &normal,
        |index, _, value: &usize| {
            if index == 1 {
                Err(NormalStageError::new(1, "typed refusal"))
            } else {
                Ok((*value, 0))
            }
        },
        &0,
    ));

    assert_eq!(sequential, normalized);
    assert!(
        matches!(
            normalized,
            PathError::AtStage {
                at: StageIndex { group: 1, stages },
                error,
            } if stages == (1..2)
                && matches!(*error, PathStageError::Operation("typed refusal"))
        ),
        "the refusal retains its atomic stage"
    );
}

#[test]
fn distinguished_and_atomic_identity_are_behaviorally_equal_but_structurally_distinct() {
    let a = schema("a");
    let identity = LensPath::<PathContractKind>::identity(&a);
    let atomic = LensPath::single(stage(PathContractKind::Identity, &a, &a));
    let (identity_view, identity_complement) = success(get_path(
        &identity,
        |_, _, value: &usize| Ok::<_, &'static str>((*value, 2)),
        &1,
    ));
    let (atomic_view, atomic_complement) = success(get_path(
        &atomic,
        |_, kind, value| {
            let (view, complement) = kind.get(*value);
            Ok::<_, &'static str>((view, complement))
        },
        &1,
    ));

    assert_eq!(identity_view, atomic_view);
    assert!(identity_complement.is_identity_complement());
    assert!(!atomic_complement.is_identity_complement());
    assert_ne!(identity, atomic);
}
