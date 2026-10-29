//! Shared component theory definitions (11 building blocks).
//!
//! These are the building-block GATs that protocols compose via colimit
//! to form their schema and instance theories. Each function returns
//! a standalone [`panproto_gat::Theory`] that can be registered and
//! referenced by name.
//!
//! ## Formal invariants
//!
//! Every theory satisfies:
//! 1. **Sort closure**: all sort names referenced in operation inputs/outputs
//!    are declared in the theory's sorts list.
//! 2. **Equation well-typedness**: both sides of every equation have the
//!    same output sort under the declared operations.
//! 3. **No tautological equations**: `lhs ≠ rhs` syntactically for all equations.
//! 4. **Rewrite-system soundness**: the directed rewrite system is locally
//!    confluent and LPO-terminating, checked at registration.
//!
//! ## Inventory
//!
//! | #  | Theory | Category |
//! |----|--------|----------|
//! | 1  | `ThGraph` | Schema shape |
//! | 2  | `ThConstraint` | Schema modifier |
//! | 3  | `ThMulti` | Schema modifier |
//! | 4  | `ThWType` | Instance shape |
//! | 5  | `ThMeta` | Instance modifier |
//! | 6  | `ThSimpleGraph` | Schema shape |
//! | 7  | `ThHypergraph` | Schema shape |
//! | 8  | `ThInterface` | Schema shape |
//! | 9  | `ThFunctor` | Instance shape |
//! | 10 | `ThFlat` | Instance shape |
//! | 11 | `ThGraphInstance` | Instance shape |
//!
//! The registration helpers in the middle of this module compose these into
//! the schema/instance theory pair each protocol group uses, and the
//! composition specs below them are the declarative recipe for the same
//! colimits.

use panproto_gat::{
    CompositionMode, CompositionSpec, CompositionStep, Equation, Operation, Sort, SortParam, Term,
    Theory,
};

// ═══════════════════════════════════════════════════════════════════
// Building blocks
// ═══════════════════════════════════════════════════════════════════

/// `ThGraph`: directed graph.
///
/// Sorts: `Vertex`, `Edge`.
/// Ops: `src : Edge → Vertex`, `tgt : Edge → Vertex`.
#[must_use]
pub fn th_graph() -> Theory {
    Theory::new(
        "ThGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// `ThConstraint`: vertex-attached constraints.
///
/// Sorts: `Vertex`, `Constraint(v: Vertex)` (dependent).
/// Ops: `target : Constraint → Vertex`.
#[must_use]
pub fn th_constraint() -> Theory {
    Theory::new(
        "ThConstraint",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
        ],
        vec![Operation::unary("target", "c", "Constraint", "Vertex")],
        vec![],
    )
}

/// `ThMulti`: multigraph (parallel edges).
///
/// Sorts: `Vertex`, `Edge`, `EdgeLabel`.
/// Ops: `edge_label : Edge → EdgeLabel`.
#[must_use]
pub fn th_multi() -> Theory {
    Theory::new(
        "ThMulti",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
            Sort::simple("EdgeLabel"),
        ],
        vec![Operation::unary("edge_label", "e", "Edge", "EdgeLabel")],
        vec![],
    )
}

/// `ThWType`: W-type instance theory (tree-shaped data).
///
/// Sorts: `Node`, `Arc`, `Value`, `Vertex`, `Edge`.
/// Ops: `anchor`, `arc_src`, `arc_tgt`, `arc_edge`, `node_value`, `src`, `tgt`.
///
/// `anchor : Node → Vertex` and `arc_edge : Arc → Edge` land in the schema
/// sorts, and the schema's `src`/`tgt` projections are declared here so the
/// two coherence axioms can be stated: an arc's endpoints are anchored at the
/// endpoints of the schema edge it is an instance of.
///
/// ```text
/// arc_src_anchor : anchor(arc_src(a)) = src(arc_edge(a))
/// arc_tgt_anchor : anchor(arc_tgt(a)) = tgt(arc_edge(a))
/// ```
///
/// The shared sorts and projections are identified with the schema theory's
/// when the instance theory is composed with it via colimit.
#[must_use]
pub fn th_wtype() -> Theory {
    Theory::new(
        "ThWType",
        vec![
            Sort::simple("Node"),
            Sort::simple("Arc"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
        ],
        vec![
            Operation::unary("anchor", "n", "Node", "Vertex"),
            Operation::unary("arc_src", "a", "Arc", "Node"),
            Operation::unary("arc_tgt", "a", "Arc", "Node"),
            Operation::unary("arc_edge", "a", "Arc", "Edge"),
            Operation::unary("node_value", "n", "Node", "Value"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new(
                "arc_src_anchor",
                Term::app("anchor", vec![Term::app("arc_src", vec![Term::var("a")])]),
                Term::app("src", vec![Term::app("arc_edge", vec![Term::var("a")])]),
            ),
            Equation::new(
                "arc_tgt_anchor",
                Term::app("anchor", vec![Term::app("arc_tgt", vec![Term::var("a")])]),
                Term::app("tgt", vec![Term::app("arc_edge", vec![Term::var("a")])]),
            ),
        ],
    )
}

/// `ThMeta`: metadata extension for W-type instances.
///
/// Sorts: `Node`, `Discriminator`, `ExtraField`.
/// Ops: `discriminator`, `extra_field`.
///
/// Note: `extra_field` outputs `Value`, which is also declared in `ThWType`.
/// These sorts are identified when `ThMeta` is composed with `ThWType` via
/// colimit over shared `{Node, Value}`.
#[must_use]
pub fn th_meta() -> Theory {
    Theory::new(
        "ThMeta",
        vec![
            Sort::simple("Node"),
            Sort::simple("Discriminator"),
            Sort::simple("ExtraField"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("discriminator", "n", "Node", "Discriminator"),
            Operation::new(
                "extra_field",
                vec![
                    ("n".into(), "Node".into()),
                    ("key".into(), "ExtraField".into()),
                ],
                "Value",
            ),
        ],
        vec![],
    )
}

// ═══════════════════════════════════════════════════════════════════
// Registration diagrams
// ═══════════════════════════════════════════════════════════════════

use panproto_gat::{
    ClashPolicy, ComposeOptions, Composition, CompositionError, DEFAULT_VERIFY_BUDGET, DiagramEdge,
    DiagramNode, OpAssignment, RecipeEdge, RecipeOpImage, TheoryDiagram, TheoryMorphism, compose,
    validate_rewrite_system,
};
use panproto_schema::{Protocol, canonical_composition_bytes};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// One registration diagram: building blocks and the shared theories whose
/// identity-on-names legs say which of their symbols are one symbol.
///
/// Registration composes a group as one diagram with
/// [`CompositionMode::ExplicitIdentity`] and [`ClashPolicy::Reject`]: only
/// what a shared theory lists is identified, and any other name two blocks
/// both declare is refused rather than glued. The shared theories are built
/// here and never inserted into a registry, so the recipe carries them
/// inline.
#[derive(Clone, Debug)]
pub struct RegistrationDiagram {
    /// The building blocks, each labelled by its theory name.
    pub blocks: Vec<Theory>,
    /// Each shared theory with the labels of the blocks it maps into; the
    /// leg into a block sends every sort and operation to the same-name
    /// one.
    pub shared: Vec<(Theory, Vec<Arc<str>>)>,
}

impl RegistrationDiagram {
    /// A diagram of one building block and nothing shared.
    #[must_use]
    pub fn single(block: Theory) -> Self {
        Self {
            blocks: vec![block],
            shared: Vec::new(),
        }
    }

    /// The node labels in diagram order: the blocks, then the shared
    /// theories.
    fn labels(&self) -> Vec<Arc<str>> {
        self.blocks
            .iter()
            .chain(self.shared.iter().map(|(apex, _)| apex))
            .map(|t| Arc::clone(&t.name))
            .collect()
    }

    /// The edges, each an identity-on-names leg from a shared theory into
    /// a block, named as [`panproto_gat::recompose`] names the legs it
    /// replays.
    fn edges(&self) -> Vec<DiagramEdge> {
        let mut edges = Vec::new();
        for (apex, targets) in &self.shared {
            for target in targets {
                let codomain = self
                    .blocks
                    .iter()
                    .find(|b| b.name == *target)
                    .map_or_else(|| Arc::clone(target), |b| Arc::clone(&b.name));
                edges.push(DiagramEdge {
                    src: Arc::clone(&apex.name),
                    tgt: Arc::clone(target),
                    morphism: TheoryMorphism::new(
                        format!("{}_to_{target}", apex.name),
                        Arc::clone(&apex.name),
                        codomain,
                        apex.sorts
                            .iter()
                            .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
                            .collect::<HashMap<_, _>>(),
                        apex.ops
                            .iter()
                            .map(|o| (Arc::clone(&o.name), OpAssignment::Op(Arc::clone(&o.name))))
                            .collect::<HashMap<_, _>>(),
                    ),
                });
            }
        }
        edges
    }

    /// The diagram [`compose()`] runs.
    #[must_use]
    pub fn diagram(&self) -> TheoryDiagram {
        TheoryDiagram {
            nodes: self
                .blocks
                .iter()
                .chain(self.shared.iter().map(|(apex, _)| apex))
                .map(|t| DiagramNode {
                    label: Arc::clone(&t.name),
                    theory: t.clone(),
                })
                .collect(),
            edges: self.edges(),
        }
    }

    /// The options registration composes with.
    #[must_use]
    pub fn options(result_name: &str) -> ComposeOptions {
        ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: Arc::from(result_name),
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        }
    }

    /// The recipe that replays this diagram through
    /// [`panproto_gat::recompose`] against a registry holding the blocks.
    #[must_use]
    pub fn recipe(&self, result_name: &str) -> CompositionSpec {
        let edges = self
            .edges()
            .into_iter()
            .map(|edge| {
                let mut sorts: Vec<(String, String)> = edge
                    .morphism
                    .sort_map
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect();
                sorts.sort();
                let mut ops: Vec<(String, RecipeOpImage)> = edge
                    .morphism
                    .op_map
                    .iter()
                    .map(|(a, image)| {
                        let image = match image {
                            OpAssignment::Op(b) => RecipeOpImage::Op(b.to_string()),
                            OpAssignment::Term(t) => RecipeOpImage::Term(t.clone()),
                        };
                        (a.to_string(), image)
                    })
                    .collect();
                ops.sort_by(|a, b| a.0.cmp(&b.0));
                RecipeEdge {
                    src: edge.src.to_string(),
                    tgt: edge.tgt.to_string(),
                    sorts,
                    ops,
                }
            })
            .collect();
        let options = Self::options(result_name);
        CompositionSpec::new(
            result_name,
            vec![CompositionStep::Diagram {
                nodes: self.labels().iter().map(ToString::to_string).collect(),
                edges,
                mode: options.mode,
                clash: options.clash,
                rewrite_budget: options.rewrite_budget,
                inline: self.shared.iter().map(|(apex, _)| apex.clone()).collect(),
            }],
        )
    }

    /// Compose the diagram under the registration options, naming the
    /// result `result_name`.
    ///
    /// # Errors
    ///
    /// The [`CompositionError`] of [`compose()`].
    pub fn compose(&self, result_name: &str) -> Result<Composition, CompositionError> {
        compose(&self.diagram(), &Self::options(result_name))
    }

    /// Record the composition of every building block as a one-node
    /// diagram, and insert the block into `registry` under its name,
    /// keeping or replacing a theory the registry already holds under that
    /// name as `existing` says.
    ///
    /// # Panics
    ///
    /// Panics if a block does not compose on its own, which is a defect in
    /// the building blocks of this module.
    pub(crate) fn register_blocks<S: ::std::hash::BuildHasher>(
        &self,
        registry: &mut HashMap<String, Theory, S>,
        existing: ExistingBlock,
    ) {
        for block in &self.blocks {
            let name = block.name.to_string();
            let single = Self::single(block.clone());
            let composition = single
                .compose(&name)
                .unwrap_or_else(|e| panic!("composing the building block `{name}` failed: {e}"));
            record(&name, &composition, single.recipe(&name));
            match existing {
                ExistingBlock::Keep => {
                    registry.entry(name).or_insert_with(|| block.clone());
                }
                ExistingBlock::Replace => {
                    registry.insert(name, block.clone());
                }
            }
        }
    }
}

/// What registering a building block does with a theory the registry
/// already holds under the block's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExistingBlock {
    /// Keep the registry's theory, as the group helpers of this module do.
    Keep,
    /// Replace it with the block, as [`crate::atproto::register_theories`]
    /// does.
    Replace,
}

/// A shared theory named `name` with the given simple sorts and no
/// operations.
fn shared_sorts(name: &str, sorts: &[&str]) -> Theory {
    Theory::new(
        name,
        sorts.iter().map(|s| Sort::simple(*s)).collect(),
        vec![],
        vec![],
    )
}

fn labels(names: &[&str]) -> Vec<Arc<str>> {
    names.iter().map(|n| Arc::from(*n)).collect()
}

/// The constrained multigraph schema: `ThGraph`, `ThConstraint` and
/// `ThMulti`, with `Vertex` shared by all three (through `ThVertex` and
/// `ThVertexEdge`) and `Edge` shared by `ThGraph` and `ThMulti`.
#[must_use]
pub fn constrained_multigraph_diagram() -> RegistrationDiagram {
    RegistrationDiagram {
        blocks: vec![th_graph(), th_constraint(), th_multi()],
        shared: vec![
            (
                shared_sorts("ThVertex", &["Vertex"]),
                labels(&["ThGraph", "ThConstraint"]),
            ),
            (
                shared_sorts("ThVertexEdge", &["Vertex", "Edge"]),
                labels(&["ThGraph", "ThMulti"]),
            ),
        ],
    }
}

/// The typed graph schema: the constrained multigraph with `ThInterface`,
/// whose `Vertex` is shared through `ThVertex2`.
#[must_use]
pub fn typed_graph_diagram() -> RegistrationDiagram {
    let mut diagram = constrained_multigraph_diagram();
    diagram.blocks.push(th_interface());
    diagram.shared.push((
        shared_sorts("ThVertex2", &["Vertex"]),
        labels(&["ThGraph", "ThInterface"]),
    ));
    diagram
}

/// The constrained hypergraph schema: `ThHypergraph` and `ThConstraint`
/// sharing `Vertex`.
#[must_use]
pub fn constrained_hypergraph_diagram() -> RegistrationDiagram {
    RegistrationDiagram {
        blocks: vec![th_hypergraph(), th_constraint()],
        shared: vec![(
            shared_sorts("ThVertex", &["Vertex"]),
            labels(&["ThHypergraph", "ThConstraint"]),
        )],
    }
}

/// The constrained simple graph schema: `ThSimpleGraph` and `ThConstraint`
/// sharing `Vertex`.
#[must_use]
pub fn constrained_simple_graph_diagram() -> RegistrationDiagram {
    RegistrationDiagram {
        blocks: vec![th_simple_graph(), th_constraint()],
        shared: vec![(
            shared_sorts("ThVertex", &["Vertex"]),
            labels(&["ThSimpleGraph", "ThConstraint"]),
        )],
    }
}

/// The W-type instance with metadata: `ThWType` and `ThMeta` sharing
/// `Node` and `Value` through a shared theory named `shared_name`.
#[must_use]
pub fn wtype_meta_diagram(shared_name: &str) -> RegistrationDiagram {
    RegistrationDiagram {
        blocks: vec![th_wtype(), th_meta()],
        shared: vec![(
            shared_sorts(shared_name, &["Node", "Value"]),
            labels(&["ThWType", "ThMeta"]),
        )],
    }
}

// ═══════════════════════════════════════════════════════════════════
// Registered compositions
// ═══════════════════════════════════════════════════════════════════

/// A registered theory's composition and the recipe that replays it.
#[derive(Clone)]
struct Registered {
    composition: Composition,
    recipe: CompositionSpec,
}

/// Every composition the registration helpers have built in this process,
/// by the name it was registered under.
fn registered() -> &'static Mutex<BTreeMap<String, Registered>> {
    static REGISTERED: OnceLock<Mutex<BTreeMap<String, Registered>>> = OnceLock::new();
    REGISTERED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// The [`Composition`] a registration helper built for the theory it
/// registered as `theory_name`.
///
/// The composition holds the composed theory, the inclusion of each
/// building block, the provenance of every symbol and the composition
/// report.
///
/// This covers every theory the helpers of this module and
/// [`crate::atproto::register_theories`] have registered in this process:
/// the schema and instance theory of each protocol, and each building
/// block, whose composition is the one-node diagram of the block. The
/// shared theories of a diagram are never inserted into a registry and
/// have none. The registry itself stays a map from names to theories; this
/// is kept beside it, by name, for every registry.
///
/// A group helper of this module keeps a theory a registry already holds
/// under a building block's name, and [`crate::atproto::register_theories`]
/// replaces it, both as in 0.74. So the composition of a block is the
/// built-in block's, and its theory differs from the registry's entry when
/// a group helper kept a caller's own theory under that name.
#[must_use]
pub fn registered_composition(theory_name: &str) -> Option<Composition> {
    registered()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(theory_name)
        .map(|r| r.composition.clone())
}

/// The recipe that replays the composition registered as `theory_name`:
/// one [`CompositionStep::Diagram`] step whose nodes name the building
/// blocks and carry the shared theories inline.
#[must_use]
pub fn registered_recipe(theory_name: &str) -> Option<CompositionSpec> {
    registered()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(theory_name)
        .map(|r| r.recipe.clone())
}

/// Fill `protocol`'s [`Protocol::schema_composition`] and
/// [`Protocol::instance_composition`] with the recipes that registration
/// records for its schema and instance theories.
///
/// Only a protocol this crate builds gets recipes: one whose name is a
/// built-in descriptor's (canonical or alias), or the raw file protocol.
/// Any other protocol keeps the fields it was given, even when its theory
/// names have recipes recorded in this process.
///
/// The recipes do not depend on what this process registered before: when
/// a theory's recipe is not recorded yet, the protocol's own theories are
/// registered into a scratch registry first, and registration is
/// deterministic, so the recipes are the same either way.
///
/// # Panics
///
/// Panics if `protocol` is one this crate builds and its theories fail to
/// register, or their registration records no recipe for its schema or
/// instance theory: both are defects in the static registration
/// configuration, like a failing registration helper.
pub fn attach_recipes(protocol: &mut Protocol) {
    let Some(register) = built_in_registration(&protocol.name) else {
        return;
    };
    let lookup = |protocol: &Protocol| {
        registered_recipe(&protocol.schema_theory).zip(registered_recipe(&protocol.instance_theory))
    };
    let recipes = if let Some(recipes) = lookup(protocol) {
        recipes
    } else {
        let mut scratch = HashMap::new();
        if let Err(err) = register(&mut scratch) {
            panic!(
                "the theories of built-in protocol {} fail to register: {err}",
                protocol.name
            );
        }
        lookup(protocol).unwrap_or_else(|| {
            panic!(
                "registering built-in protocol {} records no recipe for {} and {}",
                protocol.name, protocol.schema_theory, protocol.instance_theory
            )
        })
    };
    protocol.schema_composition = Some(recipes.0);
    protocol.instance_composition = Some(recipes.1);
}

/// Registers a protocol's theories into a registry.
type RegisterTheories = fn(&mut HashMap<String, Theory>) -> Result<(), crate::error::ProtocolError>;

/// The theory registration of the protocol this crate builds under `name`:
/// a built-in descriptor's, or the raw file protocol's, which no descriptor
/// lists.
fn built_in_registration(name: &str) -> Option<RegisterTheories> {
    if let Some(descriptor) = crate::registry::descriptor(name) {
        return Some(descriptor.register_theories);
    }
    (name == "raw_file").then_some(|registry: &mut HashMap<String, Theory>| {
        crate::raw_file::register_theories(registry);
        Ok(())
    })
}

/// Record the composition registered as `name`. The first registration of
/// a name is kept.
///
/// # Panics
///
/// Panics if `name` was registered before with a composition whose
/// canonical bytes differ: one theory name stands for one composition.
pub(crate) fn record(name: &str, composition: &Composition, recipe: CompositionSpec) {
    let mut map = registered().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = map.get(name) {
        assert!(
            canonical_composition_bytes(&existing.composition)
                == canonical_composition_bytes(composition),
            "theory `{name}` is registered twice with different compositions"
        );
        return;
    }
    map.insert(
        name.to_owned(),
        Registered {
            composition: composition.clone(),
            recipe,
        },
    );
}

/// Gate a to-be-registered theory on the soundness of its directed rewrite
/// system (local confluence and LPO termination under operation-declaration
/// order). Theories with no directed equations (every built-in composed
/// here) pass trivially.
///
/// The theories this gate sees are assembled from the building blocks in
/// this module, not supplied by a caller, so a failure is a defect in
/// those blocks rather than bad input.
///
/// # Panics
///
/// Panics if the soundness analysis cannot be completed or reports a
/// non-joining critical pair or an LPO violation. Normalization decides a
/// theory's equality judgment by running its rewrite system, so
/// registering one that is not provably sound would leave every schema
/// built on the theory resting on an unsound judgment.
pub(crate) fn gate_rewrite_system(theory: &Theory) {
    let report = validate_rewrite_system(theory).unwrap_or_else(|e| {
        panic!(
            "theory `{}`: rewrite-system soundness could not be decided: {e}",
            theory.name
        )
    });
    let warnings = report.warnings();
    assert!(
        warnings.is_empty(),
        "theory `{}` has an unsound rewrite system: {}",
        theory.name,
        warnings.join("; ")
    );
}

/// Compose `diagram` as the theory `name`, gate it, record it, and insert
/// it and its building blocks into `registry`.
///
/// # Panics
///
/// Panics if the composition fails, if the result's rewrite system is not
/// sound, or if `name` was recorded before with another composition. The
/// diagrams are static configuration, so a panic here is a defect in the
/// building blocks, not a runtime input error.
fn register_diagram<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    name: &str,
    diagram: &RegistrationDiagram,
) {
    diagram.register_blocks(registry, ExistingBlock::Keep);
    let composition = diagram
        .compose(name)
        .unwrap_or_else(|e| panic!("composing `{name}` failed: {e}"));
    gate_rewrite_system(&composition.theory);
    record(name, &composition, diagram.recipe(name));
    registry.insert(name.to_owned(), composition.theory);
}

// ═══════════════════════════════════════════════════════════════════
// Theory group registration helpers
// ═══════════════════════════════════════════════════════════════════

/// Register a **constrained multigraph + W-type** theory pair (Group A).
///
/// Schema: [`constrained_multigraph_diagram`].
/// Instance: `ThWType`.
///
/// Used by: `ATProto`, `JSON Schema`, `OpenAPI`, `AsyncAPI`, `RAML`, `JSON:API`,
/// `MongoDB`, `YAML Schema`, `TOML Schema`, `INI`, `CDDL`, `BSON`, `MsgPack`,
/// `K8s CRD`, `CloudFormation`, `Ansible`, `FHIR`, `RSS/Atom`, `vCard/iCal`,
/// `GeoJSON`, `Markdown`, and more.
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails. This is
/// a static, build-time configuration; a panic here indicates a
/// panproto-internal bug in the building-block theories, not a runtime
/// input error.
pub fn register_constrained_multigraph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &constrained_multigraph_diagram());
    register_diagram(
        registry,
        instance_name,
        &RegistrationDiagram::single(th_wtype()),
    );
}

/// Register a **hypergraph + functor** theory pair (Group B).
///
/// Schema: [`constrained_hypergraph_diagram`].
/// Instance: `ThFunctor`.
///
/// Used by: SQL, Cassandra, `DynamoDB`, Parquet, Arrow, `DataFrame`,
/// CSV/Table Schema, EDI X12, SWIFT MT.
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails (a
/// panproto-internal bug, not a runtime input error).
pub fn register_hypergraph_functor<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &constrained_hypergraph_diagram());
    register_diagram(
        registry,
        instance_name,
        &RegistrationDiagram::single(th_functor()),
    );
}

/// Register a **simple graph + flat** theory pair (Group C).
///
/// Schema: [`constrained_simple_graph_diagram`].
/// Instance: `ThFlat`.
///
/// Used by: `Protobuf`, `Avro`, `FlatBuffers`, `ASN.1`, `Bond`, `Redis`.
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails (a
/// panproto-internal bug, not a runtime input error).
pub fn register_simple_graph_flat<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &constrained_simple_graph_diagram());
    register_diagram(
        registry,
        instance_name,
        &RegistrationDiagram::single(th_flat()),
    );
}

/// Register a **typed graph + W-type** theory pair with interfaces (Group D).
///
/// Schema: [`typed_graph_diagram`].
/// Instance: `ThWType`.
///
/// Used by: GraphQL, TypeScript, Python, Rust Serde, Java, Go,
/// Swift, Kotlin, C#, JSX/React, Vue, Svelte.
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails (a
/// panproto-internal bug, not a runtime input error).
pub fn register_typed_graph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &typed_graph_diagram());
    register_diagram(
        registry,
        instance_name,
        &RegistrationDiagram::single(th_wtype()),
    );
}

/// Register a **constrained multigraph + W-type + metadata** theory pair (Group E).
///
/// Schema: [`constrained_multigraph_diagram`].
/// Instance: [`wtype_meta_diagram`] over `ThNodeValue`.
///
/// Used by: XML/XSD, HTML, CSS, DOCX, ODF.
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails (a
/// panproto-internal bug, not a runtime input error).
pub fn register_multigraph_wtype_meta<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &constrained_multigraph_diagram());
    register_diagram(registry, instance_name, &wtype_meta_diagram("ThNodeValue"));
}

/// Register a **constrained multigraph + graph instance** theory pair (Group F).
///
/// Schema: [`constrained_multigraph_diagram`].
/// Instance: `ThGraphInstance`.
///
/// Used by: Neo4j, and future graph-native protocols (RDF, OWL,
/// JSON-LD, knowledge graphs).
///
/// # Panics
///
/// Panics if a composition over the building-block theories fails (a
/// panproto-internal bug, not a runtime input error).
pub fn register_constrained_graph_instance<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    register_diagram(registry, schema_name, &constrained_multigraph_diagram());
    register_diagram(
        registry,
        instance_name,
        &RegistrationDiagram::single(th_graph_instance()),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Composition specs (declarative recipes for each group)
// ═══════════════════════════════════════════════════════════════════

/// Composition specs for Group A (constrained multigraph + W-type): the
/// recipes [`register_constrained_multigraph_wtype`] records.
#[must_use]
pub fn constrained_multigraph_wtype_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        constrained_multigraph_diagram().recipe(schema_name),
        RegistrationDiagram::single(th_wtype()).recipe(instance_name),
    )
}

/// Composition specs for Group B (hypergraph + functor): the recipes
/// [`register_hypergraph_functor`] records.
#[must_use]
pub fn hypergraph_functor_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        constrained_hypergraph_diagram().recipe(schema_name),
        RegistrationDiagram::single(th_functor()).recipe(instance_name),
    )
}

/// Composition specs for Group C (simple graph + flat): the recipes
/// [`register_simple_graph_flat`] records.
#[must_use]
pub fn simple_graph_flat_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        constrained_simple_graph_diagram().recipe(schema_name),
        RegistrationDiagram::single(th_flat()).recipe(instance_name),
    )
}

/// Composition specs for Group D (typed graph + W-type with interfaces):
/// the recipes [`register_typed_graph_wtype`] records.
#[must_use]
pub fn typed_graph_wtype_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        typed_graph_diagram().recipe(schema_name),
        RegistrationDiagram::single(th_wtype()).recipe(instance_name),
    )
}

/// Composition specs for Group E (constrained multigraph + W-type +
/// metadata): the recipes [`register_multigraph_wtype_meta`] records.
#[must_use]
pub fn multigraph_wtype_meta_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        constrained_multigraph_diagram().recipe(schema_name),
        wtype_meta_diagram("ThNodeValue").recipe(instance_name),
    )
}

/// Composition specs for Group F (constrained multigraph + graph
/// instance): the recipes [`register_constrained_graph_instance`] records.
#[must_use]
pub fn constrained_graph_instance_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    (
        constrained_multigraph_diagram().recipe(schema_name),
        RegistrationDiagram::single(th_graph_instance()).recipe(instance_name),
    )
}

// ═══════════════════════════════════════════════════════════════════
// Building blocks the registration helpers above compose
// ═══════════════════════════════════════════════════════════════════

/// `ThSimpleGraph`: simple graph (no parallel edges).
///
/// Uses a dependent sort `Edge(s: Vertex, t: Vertex)` to encode edge
/// uniqueness structurally.
#[must_use]
pub fn th_simple_graph() -> Theory {
    Theory::new(
        "ThSimpleGraph",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent(
                "Edge",
                vec![SortParam::new("s", "Vertex"), SortParam::new("t", "Vertex")],
            ),
        ],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// `ThHypergraph`: hypergraph with labeled incidence.
#[must_use]
pub fn th_hypergraph() -> Theory {
    Theory::new(
        "ThHypergraph",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("HyperEdge"),
            Sort::simple("Label"),
        ],
        vec![
            Operation::new(
                "incident",
                vec![
                    ("he".into(), "HyperEdge".into()),
                    ("l".into(), "Label".into()),
                ],
                "Vertex",
            ),
            Operation::unary("parent_label", "he", "HyperEdge", "Label"),
        ],
        vec![],
    )
}

/// `ThInterface`: interface types (GraphQL, TypeScript, etc.).
#[must_use]
pub fn th_interface() -> Theory {
    Theory::new(
        "ThInterface",
        vec![Sort::simple("Vertex"), Sort::simple("Interface")],
        vec![Operation::new(
            "implements",
            vec![
                ("v".into(), "Vertex".into()),
                ("i".into(), "Interface".into()),
            ],
            "Vertex",
        )],
        vec![],
    )
}

/// `ThFunctor`: set-valued functor instance (relational data).
///
/// Sorts: `Table`, `Row`, `ForeignKey`, and the schema sorts `Vertex`,
/// `Edge`.
/// Ops: `table_vertex`, `row_table`, `fk_src`, `fk_tgt`, `fk_edge`, and the
/// schema projections `src`, `tgt`.
///
/// A table is anchored at a schema vertex and a foreign key at the schema
/// edge it instantiates; the schema sorts and projections are declared here
/// so the two coherence axioms can be stated: a foreign key runs between
/// rows of the tables anchored at its schema edge's endpoints.
///
/// ```text
/// fk_src_anchor : src(fk_edge(k)) = table_vertex(row_table(fk_src(k)))
/// fk_tgt_anchor : tgt(fk_edge(k)) = table_vertex(row_table(fk_tgt(k)))
/// ```
///
/// Composed with a table schema theory, which has vertices but no edges,
/// only `Vertex` is shared; `Edge`, `src` and `tgt` stay the instance
/// theory's own.
#[must_use]
pub fn th_functor() -> Theory {
    let anchored = |proj: &str, end: &str| {
        (
            Term::app(proj, vec![Term::app("fk_edge", vec![Term::var("k")])]),
            Term::app(
                "table_vertex",
                vec![Term::app(
                    "row_table",
                    vec![Term::app(end, vec![Term::var("k")])],
                )],
            ),
        )
    };
    let (src_lhs, src_rhs) = anchored("src", "fk_src");
    let (tgt_lhs, tgt_rhs) = anchored("tgt", "fk_tgt");
    Theory::new(
        "ThFunctor",
        vec![
            Sort::simple("Table"),
            Sort::simple("Row"),
            Sort::simple("ForeignKey"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
        ],
        vec![
            Operation::unary("table_vertex", "t", "Table", "Vertex"),
            Operation::new("fk_src", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::new("fk_tgt", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::unary("row_table", "r", "Row", "Table"),
            Operation::unary("fk_edge", "fk", "ForeignKey", "Edge"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new("fk_src_anchor", src_lhs, src_rhs),
            Equation::new("fk_tgt_anchor", tgt_lhs, tgt_rhs),
        ],
    )
}

/// `ThFlat`: flat instance theory (key-value data).
///
/// Sorts: `Node`, `Field`, `Value`, and the schema sort `Vertex` that a
/// node is anchored at.
#[must_use]
pub fn th_flat() -> Theory {
    Theory::new(
        "ThFlat",
        vec![
            Sort::simple("Node"),
            Sort::simple("Field"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
        ],
        vec![
            Operation::unary("field_node", "f", "Field", "Node"),
            Operation::unary("field_value", "f", "Field", "Value"),
            Operation::unary("node_anchor", "n", "Node", "Vertex"),
        ],
        vec![],
    )
}

/// `ThGraphInstance`: graph-shaped instance data (most general form).
///
/// Sorts: `IVertex`, `IEdge`, `Value`, the schema sorts `Vertex`, `Edge`,
/// and the fan incidence sorts `FanNode`, `FanPort`, `HyperEdge`, `Label`.
/// Ops: `i_src`, `i_tgt`, `iv_anchor`, `ie_anchor`, `iv_value`, the schema
/// projections `src`, `tgt`, and the fan incidence operations
/// `fan_anchor`, `fan_parent`, `port_fan`, `port_label`, `port_target`.
///
/// An instance edge is anchored at the schema edge it instantiates, and its
/// endpoints at that schema edge's endpoints:
///
/// ```text
/// ie_src_anchor : src(ie_anchor(e)) = iv_anchor(i_src(e))
/// ie_tgt_anchor : tgt(ie_anchor(e)) = iv_anchor(i_tgt(e))
/// ```
///
/// An n-ary relation (a fan) is not an instance edge. A fan node is
/// anchored at a hyper-edge name and has a parent instance vertex; each of
/// its ports carries a signature label and reaches one participant. No
/// equation relates a hyper-edge name or a label to a schema vertex, so
/// fan incidence is never read as an edge between vertices.
#[must_use]
pub fn th_graph_instance() -> Theory {
    let anchored = |proj: &str, end: &str| {
        (
            Term::app(proj, vec![Term::app("ie_anchor", vec![Term::var("e")])]),
            Term::app("iv_anchor", vec![Term::app(end, vec![Term::var("e")])]),
        )
    };
    let (src_lhs, src_rhs) = anchored("src", "i_src");
    let (tgt_lhs, tgt_rhs) = anchored("tgt", "i_tgt");
    Theory::new(
        "ThGraphInstance",
        vec![
            Sort::simple("IVertex"),
            Sort::simple("IEdge"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
            Sort::simple("FanNode"),
            Sort::simple("FanPort"),
            Sort::simple("HyperEdge"),
            Sort::simple("Label"),
        ],
        vec![
            Operation::unary("i_src", "e", "IEdge", "IVertex"),
            Operation::unary("i_tgt", "e", "IEdge", "IVertex"),
            Operation::unary("iv_anchor", "v", "IVertex", "Vertex"),
            Operation::unary("ie_anchor", "e", "IEdge", "Edge"),
            Operation::unary("iv_value", "v", "IVertex", "Value"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
            Operation::unary("fan_anchor", "f", "FanNode", "HyperEdge"),
            Operation::unary("fan_parent", "f", "FanNode", "IVertex"),
            Operation::unary("port_fan", "p", "FanPort", "FanNode"),
            Operation::unary("port_label", "p", "FanPort", "Label"),
            Operation::unary("port_target", "p", "FanPort", "IVertex"),
        ],
        vec![
            Equation::new("ie_src_anchor", src_lhs, src_rhs),
            Equation::new("ie_tgt_anchor", tgt_lhs, tgt_rhs),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::{
        CheckOutcome, ClashPolicy, ComposeOptions, Composition, CompositionCheckKind,
        CompositionMode, DEFAULT_VERIFY_BUDGET, Term, TheoryDiagram, compose, typecheck_signature,
        typecheck_theory,
    };

    /// The interface a tree or graph instance theory shares with its schema
    /// theory: the schema's vertices, edges and edge endpoints.
    fn vertex_edge_interface() -> Theory {
        Theory::new(
            "Interface",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![],
        )
    }

    /// The interface a table or flat instance theory shares with its schema
    /// theory: the schema's vertices only.
    fn vertex_interface() -> Theory {
        Theory::new("Interface", vec![Sort::simple("Vertex")], vec![], vec![])
    }

    type Register = fn(&mut HashMap<String, Theory>, &str, &str);

    /// The schema theory a registration helper registers under
    /// `{tag}Schema`. Each helper gets its own tag, since a theory name
    /// stands for one composition in a process.
    fn schema_of(register: Register, tag: &str) -> Theory {
        let mut registry = HashMap::new();
        let name = format!("{tag}Schema");
        register(&mut registry, &name, &format!("{tag}Instance"));
        registry
            .remove(&name)
            .unwrap_or_else(|| panic!("the helper registers its schema theory"))
    }

    /// The instance theory a registration helper registers under
    /// `{tag}Instance`.
    fn instance_of(register: Register, tag: &str) -> Theory {
        let mut registry = HashMap::new();
        let name = format!("{tag}Instance");
        register(&mut registry, &format!("{tag}Schema"), &name);
        registry
            .remove(&name)
            .unwrap_or_else(|| panic!("the helper registers its instance theory"))
    }

    /// `compose` of `instance` with `schema` over `interface`, identifying
    /// exactly the interface and refusing any other shared name.
    fn compose_over(interface: Theory, schema: &Theory, instance: &Theory) -> Composition {
        let diagram = TheoryDiagram::by_name(
            ("Interface", interface),
            ("Schema", schema.clone()),
            ("Instance", instance.clone()),
        )
        .unwrap_or_else(|e| {
            panic!(
                "{} over the interface into {}: {e}",
                instance.name, schema.name
            )
        });
        let opts = ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: "SchemaAndInstance".into(),
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        };
        compose(&diagram, &opts)
            .unwrap_or_else(|e| panic!("{} with {}: {e}", instance.name, schema.name))
    }

    /// Every inclusion of `composition` is a morphism and the identity is
    /// its universal mediator, each recorded as `Passed`; the report passes
    /// and every record of every check kind passes, so none is failed,
    /// incomplete or inapplicable.
    fn assert_total(composition: &Composition, what: &str) {
        for check in [
            CompositionCheckKind::InclusionIsMorphism,
            CompositionCheckKind::UniversalIdentity,
        ] {
            let records: Vec<_> = composition.report.records(check).collect();
            assert!(!records.is_empty(), "{what}: no {check:?} record");
            for record in records {
                assert_eq!(
                    record.outcome,
                    CheckOutcome::Passed,
                    "{what}: {check:?} on {:?}",
                    record.subject
                );
            }
        }
        assert_eq!(
            composition.report.status,
            panproto_gat::CheckStatus::Passed,
            "{what}"
        );
        for record in &composition.report.checks {
            assert!(
                matches!(
                    record.outcome,
                    CheckOutcome::Passed | CheckOutcome::PassedByEquation { .. }
                ),
                "{what}: {record:?}"
            );
        }
    }

    /// A building block typechecks on its own, signature and body.
    fn assert_typechecks(theory: &Theory) {
        assert_eq!(
            typecheck_signature(theory),
            Ok(()),
            "{}: signature",
            theory.name
        );
        assert!(
            typecheck_theory(theory).is_ok(),
            "{}: {:?}",
            theory.name,
            typecheck_theory(theory)
        );
    }

    #[test]
    fn every_real_building_block_typechecks_standalone() {
        for theory in [
            th_graph(),
            th_constraint(),
            th_multi(),
            th_wtype(),
            th_meta(),
            th_simple_graph(),
            th_hypergraph(),
            th_interface(),
            th_functor(),
            th_flat(),
            th_graph_instance(),
        ] {
            assert_typechecks(&theory);
        }
    }

    #[test]
    fn th_graph_instance_typechecks_and_composes_with_every_graph_schema_theory() {
        let theory = th_graph_instance();
        assert_typechecks(&theory);
        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(names, vec!["ie_src_anchor", "ie_tgt_anchor"]);
        // src(ie_anchor(e)) = iv_anchor(i_src(e)), and likewise for tgt.
        let anchored = |proj: &str, end: &str| {
            (
                Term::app(proj, vec![Term::app("ie_anchor", vec![Term::var("e")])]),
                Term::app("iv_anchor", vec![Term::app(end, vec![Term::var("e")])]),
            )
        };
        assert_eq!(
            (theory.eqs[0].lhs.clone(), theory.eqs[0].rhs.clone()),
            anchored("src", "i_src")
        );
        assert_eq!(
            (theory.eqs[1].lhs.clone(), theory.eqs[1].rhs.clone()),
            anchored("tgt", "i_tgt")
        );
        let schema = schema_of(register_constrained_graph_instance, "Q6Graph");
        assert_total(
            &compose_over(vertex_edge_interface(), &schema, &theory),
            "ThGraphInstance with the constrained multigraph",
        );
    }

    #[test]
    fn fan_ports_are_not_related_to_vertices() {
        let theory = th_graph_instance();
        assert_typechecks(&theory);
        // Fan incidence is its own sort pair, anchored at hyper-edge names
        // and signature labels.
        for sort in ["FanNode", "FanPort", "HyperEdge", "Label"] {
            assert!(theory.has_sort(sort), "{sort} is declared");
        }
        let output = |op: &str| {
            theory.find_op(op).map_or_else(
                || panic!("{op} is declared"),
                |o| o.output.head().to_string(),
            )
        };
        assert_eq!(output("fan_anchor"), "HyperEdge");
        assert_eq!(output("port_label"), "Label");
        assert_eq!(output("port_fan"), "FanNode");
        // No operation reaches a schema vertex or edge from the fan sorts,
        // and no equation mentions a fan operation, so nothing equates a
        // hyper-edge name with a vertex.
        let fan_sorts = ["FanNode", "FanPort", "HyperEdge", "Label"];
        for op in &theory.ops {
            let from_fan = op
                .inputs
                .iter()
                .any(|(_, sort, _)| fan_sorts.contains(&sort.head().as_ref()));
            let to_schema = ["Vertex", "Edge"].contains(&op.output.head().as_ref());
            assert!(
                !(from_fan && to_schema),
                "{} relates a fan to the schema",
                op.name
            );
        }
        let fan_ops: Vec<&str> = theory
            .ops
            .iter()
            .filter(|op| {
                op.inputs
                    .iter()
                    .any(|(_, sort, _)| fan_sorts.contains(&sort.head().as_ref()))
            })
            .map(|op| &*op.name)
            .collect();
        assert!(!fan_ops.is_empty());
        for eq in &theory.eqs {
            let text = format!("{:?} {:?}", eq.lhs, eq.rhs);
            for op in &fan_ops {
                assert!(
                    !text.contains(&format!("\"{op}\"")),
                    "{} mentions {op}",
                    eq.name
                );
            }
        }
    }

    #[test]
    fn th_functor_typechecks_and_composes_with_every_table_schema_theory() {
        let theory = th_functor();
        assert_typechecks(&theory);
        for sort in ["Table", "Row", "ForeignKey", "Vertex", "Edge"] {
            assert!(theory.has_sort(sort), "{sort} is declared");
        }
        for op in [
            "table_vertex",
            "fk_src",
            "fk_tgt",
            "src",
            "tgt",
            "row_table",
            "fk_edge",
        ] {
            assert!(theory.has_op(op), "{op} is declared");
        }
        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(names, vec!["fk_src_anchor", "fk_tgt_anchor"]);
        let anchored = |proj: &str, end: &str| {
            (
                Term::app(proj, vec![Term::app("fk_edge", vec![Term::var("k")])]),
                Term::app(
                    "table_vertex",
                    vec![Term::app(
                        "row_table",
                        vec![Term::app(end, vec![Term::var("k")])],
                    )],
                ),
            )
        };
        assert_eq!(
            (theory.eqs[0].lhs.clone(), theory.eqs[0].rhs.clone()),
            anchored("src", "fk_src")
        );
        assert_eq!(
            (theory.eqs[1].lhs.clone(), theory.eqs[1].rhs.clone()),
            anchored("tgt", "fk_tgt")
        );
        let schema = schema_of(register_hypergraph_functor, "Q6Table");
        assert_total(
            &compose_over(vertex_interface(), &schema, &theory),
            "ThFunctor with the constrained hypergraph",
        );
    }

    #[test]
    fn th_flat_typechecks_and_composes_with_its_schema_theory() {
        let theory = th_flat();
        assert_typechecks(&theory);
        assert!(theory.has_sort("Vertex"));
        let schema = schema_of(register_simple_graph_flat, "Q6Flat");
        assert_total(
            &compose_over(vertex_interface(), &schema, &theory),
            "ThFlat with the constrained simple graph",
        );
    }

    #[test]
    fn standalone_total_theories_pass_inclusion_and_universal_checks() {
        let tree: [(&str, Register); 3] = [
            ("TotalA", register_constrained_multigraph_wtype),
            ("TotalD", register_typed_graph_wtype),
            ("TotalE", register_multigraph_wtype_meta),
        ];
        for (tag, register) in tree {
            assert_total(
                &compose_over(
                    vertex_edge_interface(),
                    &schema_of(register, tag),
                    &instance_of(register, tag),
                ),
                tag,
            );
        }
        let graph: Register = register_constrained_graph_instance;
        assert_total(
            &compose_over(
                vertex_edge_interface(),
                &schema_of(graph, "TotalF"),
                &instance_of(graph, "TotalF"),
            ),
            "TotalF",
        );
        let table: Register = register_hypergraph_functor;
        assert_total(
            &compose_over(
                vertex_interface(),
                &schema_of(table, "TotalB"),
                &instance_of(table, "TotalB"),
            ),
            "TotalB",
        );
        let flat: Register = register_simple_graph_flat;
        assert_total(
            &compose_over(
                vertex_interface(),
                &schema_of(flat, "TotalC"),
                &instance_of(flat, "TotalC"),
            ),
            "TotalC",
        );
    }

    /// The W-type instance theory states the arc/anchor coherence axioms and
    /// declares every sort and projection they mention, so it typechecks on
    /// its own rather than deferring to a later colimit.
    #[test]
    fn wtype_theory_states_the_arc_anchor_coherence_axioms() {
        let theory = th_wtype();
        assert!(
            panproto_gat::typecheck_theory(&theory).is_ok(),
            "ThWType must typecheck standalone: {:?}",
            panproto_gat::typecheck_theory(&theory),
        );

        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(
            names,
            vec!["arc_src_anchor", "arc_tgt_anchor"],
            "both endpoint coherence axioms must be stated",
        );
    }

    /// One theory name stands for one composition: registering a name again
    /// with a different composition is a defect in the static registration
    /// configuration.
    #[test]
    #[should_panic(
        expected = "theory `RecordedTwiceSchema` is registered twice with different compositions"
    )]
    fn a_theory_name_registered_again_with_another_composition_panics() {
        let mut first: HashMap<String, Theory> = HashMap::new();
        register_hypergraph_functor(&mut first, "RecordedTwiceSchema", "RecordedTwiceInstance");
        let mut second: HashMap<String, Theory> = HashMap::new();
        register_simple_graph_flat(&mut second, "RecordedTwiceSchema", "RecordedTwiceFlat");
    }

    /// Registering a name again with the same composition keeps the first
    /// record and its recipe.
    #[test]
    fn a_theory_name_registered_again_with_the_same_composition_is_kept() {
        let mut first: HashMap<String, Theory> = HashMap::new();
        register_hypergraph_functor(&mut first, "RecordedAgainSchema", "RecordedAgainInstance");
        let composition = registered_composition("RecordedAgainSchema")
            .unwrap_or_else(|| panic!("the first registration is recorded"));
        let recipe = registered_recipe("RecordedAgainSchema")
            .unwrap_or_else(|| panic!("the first registration's recipe is recorded"));
        let mut second: HashMap<String, Theory> = HashMap::new();
        register_hypergraph_functor(&mut second, "RecordedAgainSchema", "RecordedAgainInstance");
        assert_eq!(first, second);
        let again = registered_composition("RecordedAgainSchema")
            .unwrap_or_else(|| panic!("the record is kept"));
        assert_eq!(
            canonical_composition_bytes(&again),
            canonical_composition_bytes(&composition)
        );
        assert_eq!(registered_recipe("RecordedAgainSchema"), Some(recipe));
    }
}
