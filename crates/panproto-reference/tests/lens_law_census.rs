//! Independent five-gate and fusion classifications on the closed carrier.

use std::collections::BTreeMap;

use panproto_reference::encode::TreeEmbedding;
use panproto_reference::enumerate::lenses::{
    PARTIAL_LENS_SOURCES, PATH_CONTRACT_ENDPOINTS, PathContractKind, path_contract_law_class,
    path_contract_law_count_certificate, path_contract_put_domain,
};
use panproto_reference::model::{GEdge, Graph, Node, SEdge, Tree};
use panproto_reference::pages::lenses::{
    BasisCase, CongruenceCase, Divergence, DivergenceKind, DomainExpectation, EqualityClass,
    FusionObservation, GraphShape, LawFailure, LawGate, LawGateStatus, LawShape, LensShape,
    Mutation, OutcomeClass, PutGetMode, SynthIndex, TreeLawComplement, TreeShape,
    check_laws_report, check_locality_report, compare_fusion,
};
use panproto_reference::restrict::{Complement, SynthKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    OutsideDomain,
    Different,
    DesignatedAndExtra,
}

fn success<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("expected success, got {error:?}"))
}

#[derive(Clone, Copy)]
struct BooleanShape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BooleanInstance {
    value: usize,
    untouched: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum NegativeBasis {
    #[default]
    Omit,
    Include,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LocalityBehavior {
    #[default]
    Preserve,
    Corrupt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AdmittedPut {
    #[default]
    Accept,
    Refuse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum NegativeRefusal {
    #[default]
    Designated,
    Extra,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BooleanContext {
    negative_basis: NegativeBasis,
    locality: LocalityBehavior,
    admitted_put: AdmittedPut,
    negative_refusal: NegativeRefusal,
}

impl LensShape for BooleanShape {
    type Instance = BooleanInstance;
    type Complement = Vec<usize>;

    fn identical(left: &BooleanInstance, right: &BooleanInstance) -> bool {
        left == right
    }

    fn complements_identical(left: &Vec<usize>, right: &Vec<usize>) -> bool {
        left == right
    }

    fn synth_index(_: &BooleanInstance, _: &Vec<usize>) -> SynthIndex {
        SynthIndex::default()
    }

    fn equivalent(
        left: &BooleanInstance,
        _: &SynthIndex,
        right: &BooleanInstance,
        _: &SynthIndex,
    ) -> bool {
        left == right
    }

    fn divergence(
        left: &BooleanInstance,
        _: &SynthIndex,
        right: &BooleanInstance,
        _: &SynthIndex,
    ) -> Option<Divergence> {
        if left.value != right.value {
            Some(Divergence {
                kind: DivergenceKind::Instance,
                detail: "two-state values differ",
            })
        } else if left.untouched != right.untouched {
            Some(Divergence {
                kind: DivergenceKind::Locality,
                detail: "untouched coordinate differs",
            })
        } else {
            None
        }
    }

    fn complement_divergence(left: &Vec<usize>, right: &Vec<usize>) -> Option<Divergence> {
        (left != right).then_some(Divergence {
            kind: DivergenceKind::Complement,
            detail: "complement words differ",
        })
    }
}

impl LawShape<PathContractKind> for BooleanShape {
    type Refusal = Refusal;
    type Context = BooleanContext;

    fn get(
        lens: &PathContractKind,
        source: &BooleanInstance,
        _: &BooleanContext,
    ) -> Result<(BooleanInstance, Vec<usize>), Refusal> {
        let (value, complement) = lens.get(source.value);
        Ok((
            BooleanInstance {
                value,
                untouched: source.untouched,
            },
            vec![complement],
        ))
    }

    fn put(
        lens: &PathContractKind,
        view: &BooleanInstance,
        complement: &Vec<usize>,
        context: &BooleanContext,
    ) -> Result<BooleanInstance, Refusal> {
        if context.admitted_put == AdmittedPut::Refuse && view.value == 1 {
            return Err(Refusal::Different);
        }
        if context.negative_refusal == NegativeRefusal::Extra && view.value == 2 {
            return Err(Refusal::DesignatedAndExtra);
        }
        let [complement] = complement.as_slice() else {
            return Err(Refusal::OutsideDomain);
        };
        let value = lens
            .put(view.value, *complement)
            .ok_or(Refusal::OutsideDomain)?;
        Ok(BooleanInstance {
            value,
            untouched: if context.locality == LocalityBehavior::Corrupt {
                view.untouched ^ 1
            } else {
                view.untouched
            },
        })
    }

    fn check_put_domain(
        lens: &PathContractKind,
        view: &BooleanInstance,
        complement: &Vec<usize>,
        context: &BooleanContext,
    ) -> Result<(), Refusal> {
        if context.admitted_put == AdmittedPut::Refuse && view.value == 1 {
            return Err(Refusal::Different);
        }
        if context.negative_refusal == NegativeRefusal::Extra && view.value == 2 {
            return Err(Refusal::DesignatedAndExtra);
        }
        let [complement] = complement.as_slice() else {
            return Err(Refusal::OutsideDomain);
        };
        path_contract_put_domain(*lens, view.value, *complement)
            .then_some(())
            .ok_or(Refusal::OutsideDomain)
    }

    fn single_step_basis(
        _lens: &PathContractKind,
        _source: &BooleanInstance,
        view: &BooleanInstance,
        _complement: &Vec<usize>,
        context: &BooleanContext,
    ) -> Vec<BasisCase<BooleanInstance, Refusal>> {
        let mut basis = vec![BasisCase {
            ordinal: 0,
            mutation: Mutation::ValueEdit,
            view: BooleanInstance {
                value: 1 - view.value,
                untouched: view.untouched,
            },
            domain: DomainExpectation::Admitted,
        }];
        if context.negative_basis == NegativeBasis::Include {
            basis.push(BasisCase {
                ordinal: 1,
                mutation: Mutation::EditSynthesized,
                view: BooleanInstance {
                    value: 2,
                    untouched: view.untouched,
                },
                domain: DomainExpectation::Refused(Refusal::OutsideDomain),
            });
        }
        basis
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &Vec<usize>,
        _observed_view: &BooleanInstance,
        _edited_view: &BooleanInstance,
        _producing_complement: &Vec<usize>,
    ) -> Vec<usize> {
        complement.clone()
    }

    fn congruence_cases(
        _: &PathContractKind,
        view: &BooleanInstance,
        complement: &Vec<usize>,
        _: &BooleanContext,
    ) -> Vec<CongruenceCase<BooleanInstance, Vec<usize>>> {
        vec![CongruenceCase {
            ordinal: 0,
            view: *view,
            complement: complement.clone(),
        }]
    }

    fn locality_divergence(
        _lens: &PathContractKind,
        source: &BooleanInstance,
        _original_view: &BooleanInstance,
        _edited_view: &BooleanInstance,
        restored: &BooleanInstance,
        _complement: &Vec<usize>,
        _: &BooleanContext,
    ) -> Option<Divergence> {
        (source.untouched != restored.untouched).then_some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "put changed the independent untouched coordinate",
        })
    }
}

#[derive(Clone)]
struct GraphCongruenceLens {
    source: Graph,
    view: Graph,
    complement: Complement,
    witness: Graph,
    witness_complement: Complement,
}

#[derive(Clone, Copy)]
struct GraphGateShape;

impl LensShape for GraphGateShape {
    type Instance = Graph;
    type Complement = Complement;

    fn identical(left: &Graph, right: &Graph) -> bool {
        GraphShape::identical(left, right)
    }

    fn complements_identical(left: &Complement, right: &Complement) -> bool {
        GraphShape::complements_identical(left, right)
    }

    fn synth_index(instance: &Graph, complement: &Complement) -> SynthIndex {
        GraphShape::synth_index(instance, complement)
    }

    fn equivalent(
        left: &Graph,
        left_index: &SynthIndex,
        right: &Graph,
        right_index: &SynthIndex,
    ) -> bool {
        GraphShape::equivalent(left, left_index, right, right_index)
    }

    fn divergence(
        left: &Graph,
        left_index: &SynthIndex,
        right: &Graph,
        right_index: &SynthIndex,
    ) -> Option<Divergence> {
        GraphShape::divergence(left, left_index, right, right_index)
    }

    fn complement_divergence(left: &Complement, right: &Complement) -> Option<Divergence> {
        GraphShape::complement_divergence(left, right)
    }
}

impl LawShape<GraphCongruenceLens> for GraphGateShape {
    type Refusal = Refusal;
    type Context = ();

    fn get(lens: &GraphCongruenceLens, _: &Graph, (): &()) -> Result<(Graph, Complement), Refusal> {
        Ok((lens.view.clone(), lens.complement.clone()))
    }

    fn put(
        lens: &GraphCongruenceLens,
        _: &Graph,
        _: &Complement,
        (): &(),
    ) -> Result<Graph, Refusal> {
        Ok(lens.source.clone())
    }

    fn check_put_domain(
        _: &GraphCongruenceLens,
        _: &Graph,
        _: &Complement,
        (): &(),
    ) -> Result<(), Refusal> {
        Ok(())
    }

    fn single_step_basis(
        _: &GraphCongruenceLens,
        _: &Graph,
        _: &Graph,
        _: &Complement,
        (): &(),
    ) -> Vec<BasisCase<Graph, Refusal>> {
        Vec::new()
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &Complement,
        _: &Graph,
        _: &Graph,
        _: &Complement,
    ) -> Complement {
        complement.clone()
    }

    fn congruence_cases(
        lens: &GraphCongruenceLens,
        _: &Graph,
        _: &Complement,
        (): &(),
    ) -> Vec<CongruenceCase<Graph, Complement>> {
        vec![CongruenceCase {
            ordinal: 0,
            view: lens.witness.clone(),
            complement: lens.witness_complement.clone(),
        }]
    }

    fn locality_divergence(
        _: &GraphCongruenceLens,
        _: &Graph,
        _: &Graph,
        _: &Graph,
        _: &Graph,
        _: &Complement,
        (): &(),
    ) -> Option<Divergence> {
        None
    }
}

#[derive(Clone)]
struct TreeCongruenceLens {
    source: Tree,
    view: Tree,
    complement: TreeLawComplement,
    witness: Tree,
    witness_complement: TreeLawComplement,
}

#[derive(Clone, Copy)]
struct TreeGateShape;

impl LensShape for TreeGateShape {
    type Instance = Tree;
    type Complement = TreeLawComplement;

    fn identical(left: &Tree, right: &Tree) -> bool {
        TreeShape::identical(left, right)
    }

    fn complements_identical(left: &TreeLawComplement, right: &TreeLawComplement) -> bool {
        TreeShape::complements_identical(left, right)
    }

    fn synth_index(instance: &Tree, complement: &TreeLawComplement) -> SynthIndex {
        TreeShape::synth_index(instance, complement)
    }

    fn equivalent(
        left: &Tree,
        left_index: &SynthIndex,
        right: &Tree,
        right_index: &SynthIndex,
    ) -> bool {
        TreeShape::equivalent(left, left_index, right, right_index)
    }

    fn divergence(
        left: &Tree,
        left_index: &SynthIndex,
        right: &Tree,
        right_index: &SynthIndex,
    ) -> Option<Divergence> {
        TreeShape::divergence(left, left_index, right, right_index)
    }

    fn complement_divergence(
        left: &TreeLawComplement,
        right: &TreeLawComplement,
    ) -> Option<Divergence> {
        TreeShape::complement_divergence(left, right)
    }
}

impl LawShape<TreeCongruenceLens> for TreeGateShape {
    type Refusal = Refusal;
    type Context = ();

    fn get(
        lens: &TreeCongruenceLens,
        _: &Tree,
        (): &(),
    ) -> Result<(Tree, TreeLawComplement), Refusal> {
        Ok((lens.view.clone(), lens.complement.clone()))
    }

    fn put(
        lens: &TreeCongruenceLens,
        _: &Tree,
        _: &TreeLawComplement,
        (): &(),
    ) -> Result<Tree, Refusal> {
        Ok(lens.source.clone())
    }

    fn check_put_domain(
        _: &TreeCongruenceLens,
        _: &Tree,
        _: &TreeLawComplement,
        (): &(),
    ) -> Result<(), Refusal> {
        Ok(())
    }

    fn single_step_basis(
        _: &TreeCongruenceLens,
        _: &Tree,
        _: &Tree,
        _: &TreeLawComplement,
        (): &(),
    ) -> Vec<BasisCase<Tree, Refusal>> {
        Vec::new()
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &TreeLawComplement,
        _: &Tree,
        _: &Tree,
        _: &TreeLawComplement,
    ) -> TreeLawComplement {
        complement.clone()
    }

    fn congruence_cases(
        lens: &TreeCongruenceLens,
        _: &Tree,
        _: &TreeLawComplement,
        (): &(),
    ) -> Vec<CongruenceCase<Tree, TreeLawComplement>> {
        vec![CongruenceCase {
            ordinal: 0,
            view: lens.witness.clone(),
            complement: lens.witness_complement.clone(),
        }]
    }

    fn locality_divergence(
        _: &TreeCongruenceLens,
        _: &Tree,
        _: &Tree,
        _: &Tree,
        _: &Tree,
        _: &TreeLawComplement,
        (): &(),
    ) -> Option<Divergence> {
        None
    }
}

const fn is_passed<R>(status: &LawGateStatus<R>) -> bool {
    matches!(status, LawGateStatus::Passed { .. })
}

#[test]
fn five_gate_reports_reconcile_the_independent_closed_form_census() {
    let certificate = path_contract_law_count_certificate();
    let mut observed = [0; 5];
    let mut all_five = 0;
    let mut stages = 0;
    let mut source_cases = 0;
    let mut basis_cases = 0;

    for _source_endpoint in 0..PATH_CONTRACT_ENDPOINTS {
        for _target_endpoint in 0..PATH_CONTRACT_ENDPOINTS {
            for kind in PathContractKind::ALL {
                stages += 1;
                for source in 0..PARTIAL_LENS_SOURCES {
                    source_cases += 1;
                    let instance = BooleanInstance {
                        value: source,
                        untouched: 7,
                    };
                    let report = check_laws_report::<BooleanShape, _>(
                        &kind,
                        &instance,
                        &BooleanContext::default(),
                    );
                    basis_cases += report.basis_cases;
                    assert_eq!(report.gates.len(), 5);
                    let passes: Vec<_> = report
                        .gates
                        .iter()
                        .map(|gate| is_passed(&gate.status))
                        .collect();
                    let direct = path_contract_law_class(kind, source);
                    assert_eq!(passes, direct.gates.to_vec());
                    for (index, passed) in passes.iter().copied().enumerate() {
                        observed[index] += usize::from(passed);
                    }
                    all_five += usize::from(passes.into_iter().all(|passed| passed));
                }
            }
        }
    }

    assert_eq!(stages, certificate.stages);
    assert_eq!(source_cases, certificate.source_cases);
    assert_eq!(basis_cases, certificate.basis_cases);
    assert_eq!(observed, certificate.gates);
    assert_eq!(all_five, certificate.all_five);
    assert_eq!(certificate.gates, [24, 16, 24, 24, 24]);
    eprintln!(
        "path law census: stages={stages} source_cases={source_cases} ssb={basis_cases} gates={observed:?} all_five={all_five}"
    );
}

#[test]
fn totality_checks_an_independently_classified_refusal() {
    let source = BooleanInstance {
        value: 0,
        untouched: 7,
    };
    let report = check_laws_report::<BooleanShape, _>(
        &PathContractKind::Identity,
        &source,
        &BooleanContext {
            negative_basis: NegativeBasis::Include,
            ..BooleanContext::default()
        },
    );
    assert_eq!(report.basis_cases, 2);
    let Some(domain) = report
        .gates
        .iter()
        .find(|gate| gate.gate == LawGate::Totality)
    else {
        panic!("domain gate expected")
    };
    assert_eq!(domain.status, LawGateStatus::Passed { cases: 2 });

    let (_, complement) = success(BooleanShape::get(
        &PathContractKind::Identity,
        &source,
        &BooleanContext::default(),
    ));
    let invalid = BooleanInstance {
        value: 2,
        untouched: 7,
    };
    assert_eq!(
        BooleanShape::check_put_domain(
            &PathContractKind::Identity,
            &invalid,
            &complement,
            &BooleanContext::default(),
        ),
        Err(Refusal::OutsideDomain)
    );
    assert_eq!(
        BooleanShape::put(
            &PathContractKind::Identity,
            &invalid,
            &complement,
            &BooleanContext::default(),
        ),
        Err(Refusal::OutsideDomain)
    );
}

#[test]
fn totality_excludes_refused_positive_candidates_and_retains_negative_ones() {
    let source = BooleanInstance {
        value: 0,
        untouched: 7,
    };
    let refused_admitted = check_laws_report::<BooleanShape, _>(
        &PathContractKind::Identity,
        &source,
        &BooleanContext {
            admitted_put: AdmittedPut::Refuse,
            ..BooleanContext::default()
        },
    );
    assert_eq!(refused_admitted.basis_sequence, vec![Mutation::ValueEdit]);
    let Some(refused_gate) = refused_admitted
        .gates
        .iter()
        .find(|gate| gate.gate == LawGate::Totality)
    else {
        panic!("totality gate expected")
    };
    // A positive candidate the put-domain predicate refuses is out of the
    // domain: Totality ranges over no case, and nothing is admitted.
    assert_eq!(refused_gate.status, LawGateStatus::Passed { cases: 0 });
    assert_eq!(refused_admitted.admitted, 0);

    let extra_negative = check_laws_report::<BooleanShape, _>(
        &PathContractKind::Identity,
        &source,
        &BooleanContext {
            negative_basis: NegativeBasis::Include,
            negative_refusal: NegativeRefusal::Extra,
            ..BooleanContext::default()
        },
    );
    assert_eq!(
        extra_negative.basis_sequence,
        vec![Mutation::ValueEdit, Mutation::EditSynthesized]
    );
    let Some(extra_gate) = extra_negative
        .gates
        .iter()
        .find(|gate| gate.gate == LawGate::Totality)
    else {
        panic!("totality gate expected")
    };
    assert!(matches!(
        extra_gate.status,
        LawGateStatus::Failed(LawFailure::DomainMismatch {
            case: 1,
            expected: Some(Refusal::OutsideDomain),
            observed: Some(Refusal::DesignatedAndExtra),
        })
    ));
}

#[test]
fn locality_oracle_detects_an_independent_coordinate_change() {
    let report = check_locality_report::<BooleanShape, _>(
        &PathContractKind::Identity,
        &BooleanInstance {
            value: 0,
            untouched: 7,
        },
        &BooleanContext {
            locality: LocalityBehavior::Corrupt,
            ..BooleanContext::default()
        },
    );
    assert_eq!(
        report.status,
        LawGateStatus::Failed(LawFailure::Divergence {
            case: Some(0),
            divergence: Divergence {
                kind: DivergenceKind::Locality,
                detail: "put changed the independent untouched coordinate",
            },
        })
    );
}

#[test]
fn fusion_comparison_is_independent_and_complement_divergence_is_non_gating() {
    let instance = BooleanInstance {
        value: 1,
        untouched: 7,
    };
    let sequential_get = Ok::<_, Refusal>((instance, vec![2, 0]));
    let fused_get = Ok::<_, Refusal>((instance, vec![0]));
    let sequential_put = Ok::<_, Refusal>(instance);
    let fused_put = Ok::<_, Refusal>(instance);
    let report: FusionObservation<Refusal> =
        compare_fusion::<BooleanShape, _>(&sequential_get, &fused_get, &sequential_put, &fused_put);

    assert_eq!(report.get, OutcomeClass::Success(EqualityClass::Exact));
    assert!(
        matches!(
            report.complement,
            Some(EqualityClass::Divergent(Divergence {
                kind: DivergenceKind::Complement,
                detail: "complement words differ"
            }))
        ),
        "complement divergence is classified separately"
    );
    assert_eq!(report.put, OutcomeClass::Success(EqualityClass::Exact));
}

#[test]
fn fusion_comparison_preserves_equal_refusals_and_detects_typed_mismatches() {
    let equal: FusionObservation<Refusal> = compare_fusion::<BooleanShape, _>(
        &Err(Refusal::OutsideDomain),
        &Err(Refusal::OutsideDomain),
        &Err(Refusal::OutsideDomain),
        &Err(Refusal::OutsideDomain),
    );
    assert_eq!(
        equal.get,
        OutcomeClass::EqualRefusal(Refusal::OutsideDomain)
    );
    assert_eq!(
        equal.put,
        OutcomeClass::EqualRefusal(Refusal::OutsideDomain)
    );

    let mismatch: FusionObservation<Refusal> = compare_fusion::<BooleanShape, _>(
        &Err(Refusal::OutsideDomain),
        &Err(Refusal::Different),
        &Ok(BooleanInstance {
            value: 0,
            untouched: 7,
        }),
        &Err(Refusal::Different),
    );
    assert_eq!(
        mismatch.get,
        OutcomeClass::RefusalMismatch {
            sequential: Some(Refusal::OutsideDomain),
            fused: Some(Refusal::Different),
        }
    );
    assert_eq!(
        mismatch.put,
        OutcomeClass::RefusalMismatch {
            sequential: None,
            fused: Some(Refusal::Different),
        }
    );
}

struct GraphSynthFixture {
    label: SEdge,
    node_key: SynthKey,
    left: Graph,
    right: Graph,
    left_complement: Complement,
    right_complement: Complement,
}

fn graph_synth_fixture() -> GraphSynthFixture {
    let label = SEdge::new("root", "leaf", "child", Some("leaf"));
    let node_key = SynthKey::Contract {
        src: 1,
        tgt: 2,
        edge: label.clone(),
    };
    let edge_key = SynthKey::Expand {
        source_edge: 3,
        hop: 0,
    };
    let mut left_graph = Graph::default();
    left_graph.nodes.insert(1, Node::new(1, "root"));
    left_graph.nodes.insert(7, Node::new(7, "leaf"));
    left_graph.edges.insert(
        8,
        GEdge {
            src: 1,
            tgt: 7,
            label: label.clone(),
            position: None,
        },
    );
    let mut right_graph = Graph::default();
    right_graph.nodes.insert(1, Node::new(1, "root"));
    right_graph.nodes.insert(99, Node::new(99, "leaf"));
    right_graph.edges.insert(
        100,
        GEdge {
            src: 1,
            tgt: 99,
            label: label.clone(),
            position: None,
        },
    );
    let left_complement = Complement {
        synthesized_nodes: BTreeMap::from([(7, node_key.clone())]),
        synthesized_edges: BTreeMap::from([(8, edge_key.clone())]),
        ..Complement::default()
    };
    let right_complement = Complement {
        synthesized_nodes: BTreeMap::from([(99, node_key.clone())]),
        synthesized_edges: BTreeMap::from([(100, edge_key)]),
        ..Complement::default()
    };
    GraphSynthFixture {
        label,
        node_key,
        left: left_graph,
        right: right_graph,
        left_complement,
        right_complement,
    }
}

fn assert_graph_synth_equivalence(fixture: &GraphSynthFixture) {
    let left_index = GraphShape::synth_index(&fixture.left, &fixture.left_complement);
    let right_index = GraphShape::synth_index(&fixture.right, &fixture.right_complement);
    assert!(!GraphShape::identical(&fixture.left, &fixture.right));
    assert!(GraphShape::equivalent(
        &fixture.left,
        &left_index,
        &fixture.right,
        &right_index,
    ));
}

struct TreeSynthFixture {
    left: Tree,
    right: Tree,
    left_complement: TreeLawComplement,
    right_complement: TreeLawComplement,
}

fn tree_synth_fixture(graph: &GraphSynthFixture) -> TreeSynthFixture {
    let left = Tree {
        nodes: graph.left.nodes.clone(),
        arcs: vec![(1, 7, graph.label.clone())],
        fans: Vec::new(),
        root: 1,
    };
    let right = Tree {
        nodes: graph.right.nodes.clone(),
        arcs: vec![(1, 99, graph.label.clone())],
        fans: Vec::new(),
        root: 1,
    };
    let left_complement = TreeLawComplement {
        complement: Complement {
            synthesized_nodes: BTreeMap::from([(7, graph.node_key.clone())]),
            ..Complement::default()
        },
        embedding: TreeEmbedding::default(),
        source_arcs: BTreeMap::new(),
    };
    let right_complement = TreeLawComplement {
        complement: Complement {
            synthesized_nodes: BTreeMap::from([(99, graph.node_key.clone())]),
            ..Complement::default()
        },
        embedding: TreeEmbedding::default(),
        source_arcs: BTreeMap::new(),
    };
    TreeSynthFixture {
        left,
        right,
        left_complement,
        right_complement,
    }
}

fn assert_tree_synth_equivalence(fixture: &TreeSynthFixture) {
    let left_index = TreeShape::synth_index(&fixture.left, &fixture.left_complement);
    let right_index = TreeShape::synth_index(&fixture.right, &fixture.right_complement);
    assert!(!TreeShape::identical(&fixture.left, &fixture.right));
    assert!(TreeShape::equivalent(
        &fixture.left,
        &left_index,
        &fixture.right,
        &right_index,
    ));
}

fn assert_graph_congruence_passes(graph_lens: &GraphCongruenceLens) {
    let graph_report = check_laws_report::<GraphGateShape, _>(graph_lens, &Graph::default(), &());
    let graph_congruence = graph_report
        .gates
        .iter()
        .find(|report| report.gate == LawGate::Congruence)
        .unwrap_or_else(|| panic!("graph Congruence gate missing"));
    assert_eq!(graph_congruence.status, LawGateStatus::Passed { cases: 1 });
}

fn assert_tree_congruence_passes(tree_lens: &TreeCongruenceLens) {
    let tree_report = check_laws_report::<TreeGateShape, _>(tree_lens, &tree_lens.source, &());
    let tree_congruence = tree_report
        .gates
        .iter()
        .find(|report| report.gate == LawGate::Congruence)
        .unwrap_or_else(|| panic!("tree Congruence gate missing"));
    assert_eq!(tree_congruence.status, LawGateStatus::Passed { cases: 1 });
}

fn assert_changed_synth_anchor_fails(mut graph_lens: GraphCongruenceLens) {
    let Some(node) = graph_lens.witness.nodes.get_mut(&99) else {
        panic!("synthesized witness node missing")
    };
    "wrong".clone_into(&mut node.anchor);
    let failed = check_laws_report::<GraphGateShape, _>(&graph_lens, &Graph::default(), &());
    let congruence = failed
        .gates
        .iter()
        .find(|report| report.gate == LawGate::Congruence)
        .unwrap_or_else(|| panic!("negative Congruence gate missing"));
    assert_eq!(
        congruence.status,
        LawGateStatus::Failed(LawFailure::CongruenceMismatch {
            case: 0,
            detail: "declared presentations are not SynthIndex-equivalent",
        })
    );
}

#[test]
fn graph_and_tree_equivalence_uses_synth_keys_instead_of_concrete_ids() {
    let graph = graph_synth_fixture();
    assert_graph_synth_equivalence(&graph);
    let tree = tree_synth_fixture(&graph);
    assert_tree_synth_equivalence(&tree);

    let graph_lens = GraphCongruenceLens {
        source: Graph::default(),
        view: graph.left,
        complement: graph.left_complement,
        witness: graph.right,
        witness_complement: graph.right_complement,
    };
    assert_graph_congruence_passes(&graph_lens);

    let tree_lens = TreeCongruenceLens {
        source: Tree {
            nodes: BTreeMap::from([(1, Node::new(1, "root"))]),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: 1,
        },
        view: tree.left,
        complement: tree.left_complement,
        witness: tree.right,
        witness_complement: tree.right_complement,
    };
    assert_tree_congruence_passes(&tree_lens);
    assert_changed_synth_anchor_fails(graph_lens);
}
