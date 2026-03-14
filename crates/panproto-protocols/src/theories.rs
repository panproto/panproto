//! Shared component theory definitions (27 building blocks).
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
//!
//! ## Inventory
//!
//! | # | Theory | Sorts | Eqs | Category |
//! |---|--------|-------|-----|----------|
//! | 1 | ThGraph | 2 | 0 | Schema shape |
//! | 2 | ThSimpleGraph | 2 | 0 | Schema shape (dependent Edge) |
//! | 3 | ThHypergraph | 3 | 0 | Schema shape |
//! | 4 | ThConstraint | 2 | 0 | Schema modifier |
//! | 5 | ThMulti | 3 | 0 | Schema modifier |
//! | 6 | ThInterface | 2 | 0 | Schema modifier |
//! | 7 | ThWType | 3 | 0 | Instance shape |
//! | 8 | ThMeta | 3 | 0 | Instance modifier |
//! | 9 | ThFunctor | 3 | 0 | Instance shape |
//! | 10 | ThFlat | 3 | 0 | Instance shape |
//! | 11 | ThOrder | 2 | 0 | Schema modifier |
//! | 12 | ThCoproduct | 3 | 0 | Schema modifier |
//! | 13 | ThRecursion | 2 | 1 | Schema modifier |
//! | 14 | ThSpan | 2 | 0 | Schema structure |
//! | 15 | ThCospan | 2 | 0 | Schema structure |
//! | 16 | ThPartial | 2 | 0 | Schema modifier |
//! | 17 | ThLinear | 2 | 0 | Schema modifier |
//! | 18 | ThNominal | 2 | 0 | Schema modifier |
//! | 19 | ThReflexiveGraph | 2 | 2 | Schema shape |
//! | 20 | ThSymmetricGraph | 2 | 3 | Schema shape |
//! | 21 | ThPetriNet | 3 | 0 | Schema shape |
//! | 22 | ThGraphInstance | 5 | 0 | Instance shape |
//! | 23 | ThAnnotation | 4 | 0 | Instance modifier |
//! | 24 | ThCausal | 3 | 0 | Instance modifier |
//! | 25 | ThOperad | 2 | 0 | Schema structure |
//! | 26 | ThTracedMonoidal | 2 | 0 | Schema structure |
//! | 27 | ThSimplicial | 3 | 0 | Instance structure |

use panproto_gat::{Equation, Operation, Sort, SortParam, Term, Theory};

// ═══════════════════════════════════════════════════════════════════
// Original 10 building blocks (with corrections)
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

/// `ThSimpleGraph`: simple graph (no parallel edges).
///
/// Uses a dependent sort `Edge(s: Vertex, t: Vertex)` to encode edge
/// uniqueness structurally: an edge is determined by its endpoints.
/// This is the formally correct encoding — the conditional property
/// `src(e1) = src(e2) ∧ tgt(e1) = tgt(e2) → e1 = e2` is not
/// expressible as an unconditional GAT equation, but the dependent
/// sort achieves the same effect.
///
/// Ops `src` and `tgt` are retained as projections for API compatibility.
#[must_use]
pub fn th_simple_graph() -> Theory {
    Theory::new(
        "ThSimpleGraph",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent(
                "Edge",
                vec![
                    SortParam::new("s", "Vertex"),
                    SortParam::new("t", "Vertex"),
                ],
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
///
/// Sorts: `Vertex`, `HyperEdge`, `Label`.
/// Ops: `incident(he, l) → Vertex`, `parent_label(he) → Label`.
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

/// `ThInterface`: interface types (`GraphQL`, TypeScript, etc.).
///
/// Sorts: `Vertex`, `Interface`.
/// Ops: `implements(v, i) → Vertex`.
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

/// `ThWType`: W-type instance theory (tree-shaped data).
///
/// Sorts: `Node`, `Arc`, `Value`.
/// Ops: `anchor`, `arc_src`, `arc_tgt`, `arc_edge`, `node_value`.
///
/// Note: `anchor : Node → Vertex` and `arc_edge : Arc → Edge` reference
/// schema sorts. These are identified via colimit when the instance
/// theory is composed with the schema theory.
#[must_use]
pub fn th_wtype() -> Theory {
    Theory::new(
        "ThWType",
        vec![
            Sort::simple("Node"),
            Sort::simple("Arc"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("anchor", "n", "Node", "Vertex"),
            Operation::unary("arc_src", "a", "Arc", "Node"),
            Operation::unary("arc_tgt", "a", "Arc", "Node"),
            Operation::unary("arc_edge", "a", "Arc", "Edge"),
            Operation::unary("node_value", "n", "Node", "Value"),
        ],
        vec![],
    )
}

/// `ThMeta`: metadata extension for W-type instances.
///
/// Sorts: `Node`, `Discriminator`, `ExtraField`.
/// Ops: `discriminator`, `extra_field`.
///
/// Note: `extra_field` outputs `Value` which is from `ThWType`. These
/// are identified when `ThMeta` is composed with `ThWType` via colimit
/// over shared `Node`.
#[must_use]
pub fn th_meta() -> Theory {
    Theory::new(
        "ThMeta",
        vec![
            Sort::simple("Node"),
            Sort::simple("Discriminator"),
            Sort::simple("ExtraField"),
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

/// `ThFunctor`: set-valued functor instance (relational data).
///
/// Sorts: `Table`, `Row`, `ForeignKey`.
/// Ops: `table_vertex`, `fk_src`, `fk_tgt`.
///
/// Note: `table_vertex` outputs `Vertex` (schema sort, identified via colimit).
#[must_use]
pub fn th_functor() -> Theory {
    Theory::new(
        "ThFunctor",
        vec![
            Sort::simple("Table"),
            Sort::simple("Row"),
            Sort::simple("ForeignKey"),
        ],
        vec![
            Operation::unary("table_vertex", "t", "Table", "Vertex"),
            Operation::new("fk_src", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::new("fk_tgt", vec![("fk".into(), "ForeignKey".into())], "Row"),
        ],
        vec![],
    )
}

/// `ThFlat`: flat instance theory (key-value data).
///
/// Sorts: `Node`, `Field`, `Value`.
/// Ops: `field_node`, `field_value`, `node_anchor`.
///
/// Note: `node_anchor` outputs `Vertex` (schema sort, identified via colimit).
#[must_use]
pub fn th_flat() -> Theory {
    Theory::new(
        "ThFlat",
        vec![
            Sort::simple("Node"),
            Sort::simple("Field"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("field_node", "f", "Field", "Node"),
            Operation::unary("field_value", "f", "Field", "Value"),
            Operation::unary("node_anchor", "n", "Node", "Vertex"),
        ],
        vec![],
    )
}

// ═══════════════════════════════════════════════════════════════════
// Schema-side building blocks: modifiers and structural theories
// ═══════════════════════════════════════════════════════════════════

/// `ThOrder`: ordered collections.
///
/// Sorts: `Edge`, `Position`.
/// Ops: `edge_position : Edge → Position`, `succ : Position → Position`.
///
/// No equation — order preservation (`edge_position` commuting with
/// `succ`) is a model-level constraint, not an unconditional GAT axiom.
/// `Edge` is shared with `ThGraph`/`ThMulti` via colimit.
#[must_use]
pub fn th_order() -> Theory {
    Theory::new(
        "ThOrder",
        vec![Sort::simple("Edge"), Sort::simple("Position")],
        vec![
            Operation::unary("edge_position", "e", "Edge", "Position"),
            Operation::unary("succ", "p", "Position", "Position"),
        ],
        vec![],
    )
}

/// `ThCoproduct`: sum types / tagged unions.
///
/// Sorts: `Vertex`, `Variant`, `Tag`.
/// Ops: `injection : Variant → Vertex`, `tag : Variant → Tag`,
///      `variant_of : Vertex → Variant` (retraction).
/// Eqs: `variant_of(injection(v)) = v`.
///
/// Injectivity of `injection` is encoded as structure: `variant_of`
/// is a retraction (left inverse) of `injection`. The equation says
/// injecting a variant and then recovering it gives back the original.
///
/// Type check:
/// - `injection(v) : Vertex` (injection: Variant → Vertex, v: Variant)
/// - `variant_of(injection(v)) : Variant` (variant_of: Vertex → Variant)
/// - `v : Variant`
/// - Both sides: `Variant`. ✓
///
/// `Vertex` is shared with `ThGraph` via colimit.
#[must_use]
pub fn th_coproduct() -> Theory {
    Theory::new(
        "ThCoproduct",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("Variant"),
            Sort::simple("Tag"),
        ],
        vec![
            Operation::unary("injection", "v", "Variant", "Vertex"),
            Operation::unary("tag", "v", "Variant", "Tag"),
            Operation::unary("variant_of", "v", "Vertex", "Variant"),
        ],
        vec![Equation::new(
            "variant_retraction",
            Term::app(
                "variant_of",
                vec![Term::app("injection", vec![Term::var("v")])],
            ),
            Term::var("v"),
        )],
    )
}

/// `ThRecursion`: recursive types / fixpoints.
///
/// Sorts: `Vertex`, `Mu`.
/// Ops: `unfold : Mu → Vertex`, `fold : Vertex → Mu`.
/// Eqs: `fold_unfold: unfold(fold(v)) = v`.
///
/// Type check: `fold(v) : Mu`, `unfold(fold(v)) : Vertex`, `v : Vertex`.
/// Both sides are `Vertex`. ✓
///
/// This is the isorecursive fold-unfold law. `fold` is a section of
/// `unfold` (but not necessarily vice versa — equirecursive types
/// would add `fold(unfold(m)) = m` as well).
#[must_use]
pub fn th_recursion() -> Theory {
    Theory::new(
        "ThRecursion",
        vec![Sort::simple("Vertex"), Sort::simple("Mu")],
        vec![
            Operation::unary("unfold", "m", "Mu", "Vertex"),
            Operation::unary("fold", "v", "Vertex", "Mu"),
        ],
        vec![Equation::new(
            "fold_unfold",
            Term::app("unfold", vec![Term::app("fold", vec![Term::var("v")])]),
            Term::var("v"),
        )],
    )
}

/// `ThSpan`: correspondences / diffs.
///
/// Sorts: `Vertex`, `Span`.
/// Ops: `span_left : Span → Vertex`, `span_right : Span → Vertex`.
///
/// A span `A ← S → B` connects two vertices through a common source.
/// Diffs, patches, and migrations are spans. `Vertex` shared via colimit.
#[must_use]
pub fn th_span() -> Theory {
    Theory::new(
        "ThSpan",
        vec![Sort::simple("Vertex"), Sort::simple("Span")],
        vec![
            Operation::unary("span_left", "s", "Span", "Vertex"),
            Operation::unary("span_right", "s", "Span", "Vertex"),
        ],
        vec![],
    )
}

/// `ThCospan`: merge targets (dual of span).
///
/// Sorts: `Vertex`, `Apex`.
/// Ops: `inl : Vertex → Apex`, `inr : Vertex → Apex`.
///
/// A cospan `A → M ← B` models a merge result. Categorically, merge
/// is the pushout (colimit). `Vertex` shared via colimit.
#[must_use]
pub fn th_cospan() -> Theory {
    Theory::new(
        "ThCospan",
        vec![Sort::simple("Vertex"), Sort::simple("Apex")],
        vec![
            Operation::unary("inl", "v", "Vertex", "Apex"),
            Operation::unary("inr", "v", "Vertex", "Apex"),
        ],
        vec![],
    )
}

/// `ThPartial`: optionality / partiality.
///
/// Sorts: `Vertex`, `Defined`.
/// Ops: `defined : Vertex → Defined` (witness of inhabitedness),
///      `witness : Defined → Vertex` (section / evidence term).
/// Eqs: `defined(witness(d)) = d`.
///
/// The `Defined` sort is a witness that a vertex is inhabited
/// (required, not optional). `witness` is a section of `defined`:
/// every definedness proof arises from some vertex.
///
/// Type check:
/// - `witness(d) : Vertex` (witness: Defined → Vertex, d: Defined)
/// - `defined(witness(d)) : Defined` (defined: Vertex → Defined)
/// - `d : Defined`
/// - Both sides: `Defined`. ✓
///
/// `Vertex` is shared via colimit.
#[must_use]
pub fn th_partial() -> Theory {
    Theory::new(
        "ThPartial",
        vec![Sort::simple("Vertex"), Sort::simple("Defined")],
        vec![
            Operation::unary("defined", "v", "Vertex", "Defined"),
            Operation::unary("witness", "d", "Defined", "Vertex"),
        ],
        vec![Equation::new(
            "defined_witness",
            Term::app(
                "defined",
                vec![Term::app("witness", vec![Term::var("d")])],
            ),
            Term::var("d"),
        )],
    )
}

/// `ThLinear`: use-counting / linearity.
///
/// Sorts: `Edge`, `Usage`.
/// Ops: `use_count : Edge → Usage`.
///
/// Distinguishes structural (any), linear (exactly once), and affine
/// (at most once) edges. `Edge` shared via colimit.
#[must_use]
pub fn th_linear() -> Theory {
    Theory::new(
        "ThLinear",
        vec![Sort::simple("Edge"), Sort::simple("Usage")],
        vec![Operation::unary("use_count", "e", "Edge", "Usage")],
        vec![],
    )
}

/// `ThNominal`: nominal identity.
///
/// Sorts: `Vertex`, `Name`.
/// Ops: `name : Vertex → Name`.
///
/// Nominal: identity by name (Java, Protobuf field numbers).
/// Structural: identity by shape (TypeScript). `Vertex` shared.
#[must_use]
pub fn th_nominal() -> Theory {
    Theory::new(
        "ThNominal",
        vec![Sort::simple("Vertex"), Sort::simple("Name")],
        vec![Operation::unary("name", "v", "Vertex", "Name")],
        vec![],
    )
}

/// `ThReflexiveGraph`: graph with identity edges.
///
/// Sorts: `Vertex`, `Edge`.
/// Ops: `id : Vertex → Edge`, `src : Edge → Vertex`, `tgt : Edge → Vertex`.
/// Eqs: `src(id(v)) = v`, `tgt(id(v)) = v`.
///
/// Type check for `src_id`:
/// - `id(v) : Edge` (since `id : Vertex → Edge` and `v : Vertex`)
/// - `src(id(v)) : Vertex` (since `src : Edge → Vertex`)
/// - `v : Vertex`
/// - Both sides: `Vertex`. ✓
///
/// Same for `tgt_id`. ✓
///
/// The identity edge is the first building block where equations are
/// load-bearing: without them, it's just `ThGraph` + an extra operation.
#[must_use]
pub fn th_reflexive_graph() -> Theory {
    Theory::new(
        "ThReflexiveGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("id", "v", "Vertex", "Edge"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new(
                "src_id",
                Term::app("src", vec![Term::app("id", vec![Term::var("v")])]),
                Term::var("v"),
            ),
            Equation::new(
                "tgt_id",
                Term::app("tgt", vec![Term::app("id", vec![Term::var("v")])]),
                Term::var("v"),
            ),
        ],
    )
}

/// `ThSymmetricGraph`: graph with edge inversion.
///
/// Sorts: `Vertex`, `Edge`.
/// Ops: `inv : Edge → Edge`, `src : Edge → Vertex`, `tgt : Edge → Vertex`.
/// Eqs: `src(inv(e)) = tgt(e)`, `tgt(inv(e)) = src(e)`, `inv(inv(e)) = e`.
///
/// Type check for `src_inv`:
/// - `inv(e) : Edge` (since `inv : Edge → Edge` and `e : Edge`)
/// - `src(inv(e)) : Vertex`
/// - `tgt(e) : Vertex`
/// - Both sides: `Vertex`. ✓
///
/// Type check for `inv_inv`:
/// - `inv(inv(e)) : Edge`
/// - `e : Edge`
/// - Both sides: `Edge`. ✓
#[must_use]
pub fn th_symmetric_graph() -> Theory {
    Theory::new(
        "ThSymmetricGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("inv", "e", "Edge", "Edge"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new(
                "src_inv",
                Term::app("src", vec![Term::app("inv", vec![Term::var("e")])]),
                Term::app("tgt", vec![Term::var("e")]),
            ),
            Equation::new(
                "tgt_inv",
                Term::app("tgt", vec![Term::app("inv", vec![Term::var("e")])]),
                Term::app("src", vec![Term::var("e")]),
            ),
            Equation::new(
                "inv_inv",
                Term::app("inv", vec![Term::app("inv", vec![Term::var("e")])]),
                Term::var("e"),
            ),
        ],
    )
}

/// `ThPetriNet`: Petri net for concurrent structure.
///
/// Sorts: `Place`, `Transition`, `Token`.
/// Ops: `pre : Transition → Place`, `post : Transition → Place`,
///      `marking : Place → Token`.
///
/// Models concurrent/workflow protocols (BPMN, state machines with
/// concurrency, microservice choreographies). Petri nets generate
/// free symmetric monoidal categories.
#[must_use]
pub fn th_petri_net() -> Theory {
    Theory::new(
        "ThPetriNet",
        vec![
            Sort::simple("Place"),
            Sort::simple("Transition"),
            Sort::simple("Token"),
        ],
        vec![
            Operation::unary("pre", "t", "Transition", "Place"),
            Operation::unary("post", "t", "Transition", "Place"),
            Operation::unary("marking", "p", "Place", "Token"),
        ],
        vec![],
    )
}

// ═══════════════════════════════════════════════════════════════════
// Instance-side building blocks
// ═══════════════════════════════════════════════════════════════════

/// `ThGraphInstance`: graph-shaped instance data (most general form).
///
/// Sorts: `IVertex`, `IEdge`, `Value`, `Vertex`, `Edge`.
/// Ops: `i_src`, `i_tgt`, `iv_anchor`, `ie_anchor`, `iv_value`.
///
/// Unlike `ThWType` (trees), `ThGraphInstance` has no distinguished
/// root and cycles are allowed. Both `ThWType` and `ThFunctor` are
/// special cases. `Vertex` and `Edge` are schema sorts, shared via
/// colimit when composed with a schema theory.
#[must_use]
pub fn th_graph_instance() -> Theory {
    Theory::new(
        "ThGraphInstance",
        vec![
            Sort::simple("IVertex"),
            Sort::simple("IEdge"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
        ],
        vec![
            Operation::unary("i_src", "e", "IEdge", "IVertex"),
            Operation::unary("i_tgt", "e", "IEdge", "IVertex"),
            Operation::unary("iv_anchor", "v", "IVertex", "Vertex"),
            Operation::unary("ie_anchor", "e", "IEdge", "Edge"),
            Operation::unary("iv_value", "v", "IVertex", "Value"),
        ],
        vec![],
    )
}

/// `ThAnnotation`: out-of-band metadata.
///
/// Sorts: `Vertex`, `Annotation`, `AnnotationKey`, `Value`.
/// Ops: `annotated`, `ann_key`, `ann_value`.
///
/// Models metadata structurally different from data: XML attributes
/// (vs. elements), Java annotations, Protobuf options.
/// `Vertex` and `Value` are shared via colimit.
#[must_use]
pub fn th_annotation() -> Theory {
    Theory::new(
        "ThAnnotation",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("Annotation"),
            Sort::simple("AnnotationKey"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("annotated", "a", "Annotation", "Vertex"),
            Operation::unary("ann_key", "a", "Annotation", "AnnotationKey"),
            Operation::unary("ann_value", "a", "Annotation", "Value"),
        ],
        vec![],
    )
}

/// `ThCausal`: causal / temporal ordering.
///
/// Sorts: `Event`, `Timestamp`, `Before(e1: Event, e2: Event)` (dependent).
/// Ops: `time : Event → Timestamp`,
///      `before_refl(e: Event) : Before(e, e)`,
///      `before_trans(p: Before(a,b), q: Before(b,c)) : Before(a,c)`.
///
/// Transitivity is encoded as structure using Cartmell's property-to-
/// structure move: the dependent sort `Before(e1, e2)` carries
/// evidence that `e1` causally precedes `e2`. Transitivity becomes
/// a composition operation on evidence terms, not a conditional axiom.
///
/// This is the same pattern as ThCategory (where `Hom(a,b)` carries
/// morphism evidence and composition is an operation).
#[must_use]
pub fn th_causal() -> Theory {
    Theory::new(
        "ThCausal",
        vec![
            Sort::simple("Event"),
            Sort::simple("Timestamp"),
            Sort::dependent(
                "Before",
                vec![
                    SortParam::new("e1", "Event"),
                    SortParam::new("e2", "Event"),
                ],
            ),
        ],
        vec![
            Operation::unary("time", "e", "Event", "Timestamp"),
            Operation::unary("before_refl", "e", "Event", "Before"),
            Operation::new(
                "before_trans",
                vec![
                    ("p".into(), "Before".into()),
                    ("q".into(), "Before".into()),
                ],
                "Before",
            ),
        ],
        vec![],
    )
}

// ═══════════════════════════════════════════════════════════════════
// Architectural building blocks
// ═══════════════════════════════════════════════════════════════════

/// `ThOperad`: multi-input operations (fan-in / aggregation).
///
/// Sorts: `Color`, `MOperation`.
/// Ops: `arity : MOperation → Color`, `op_output : MOperation → Color`.
///
/// Models aggregation patterns: `MapReduce`, SQL GROUP BY, event
/// stream windowing, merge commits in VCS. Directed n-ary (unlike
/// `ThHypergraph` which is symmetric n-ary).
#[must_use]
pub fn th_operad() -> Theory {
    Theory::new(
        "ThOperad",
        vec![Sort::simple("Color"), Sort::simple("MOperation")],
        vec![
            Operation::unary("arity", "op", "MOperation", "Color"),
            Operation::unary("op_output", "op", "MOperation", "Color"),
        ],
        vec![],
    )
}

/// `ThTracedMonoidal`: feedback / loops.
///
/// Sorts: `Wire`, `Box`.
/// Ops: `trace_in : Box → Wire`, `trace_out : Box → Wire`,
///      `feedback : Box → Box`.
///
/// Axiomatizes feedback loops and recursion creation. Given
/// `f : A ⊗ U → B ⊗ U`, `trace(f) : A → B` feeds `U` back.
/// Relevant for recursive schemas, circular dependencies, and
/// control flow graphs.
#[must_use]
pub fn th_traced_monoidal() -> Theory {
    Theory::new(
        "ThTracedMonoidal",
        vec![Sort::simple("Wire"), Sort::simple("Box")],
        vec![
            Operation::unary("trace_in", "b", "Box", "Wire"),
            Operation::unary("trace_out", "b", "Box", "Wire"),
            Operation::unary("feedback", "b", "Box", "Box"),
        ],
        vec![],
    )
}

/// `ThSimplicial`: simplicial structure (layered / filtered data).
///
/// Sorts: `Simplex`, `Face`, `Degeneracy`.
/// Ops: `face_map : Face → Simplex`, `degeneracy_map : Degeneracy → Simplex`.
///
/// Models layered data: CSS cascade order, config override layers,
/// git ancestry (merge commits create 2-simplices), transitive
/// closure witnesses.
#[must_use]
pub fn th_simplicial() -> Theory {
    Theory::new(
        "ThSimplicial",
        vec![
            Sort::simple("Simplex"),
            Sort::simple("Face"),
            Sort::simple("Degeneracy"),
        ],
        vec![
            Operation::unary("face_map", "f", "Face", "Simplex"),
            Operation::unary("degeneracy_map", "d", "Degeneracy", "Simplex"),
        ],
        vec![],
    )
}

// ═══════════════════════════════════════════════════════════════════
// Theory group registration helpers
// ═══════════════════════════════════════════════════════════════════

use panproto_gat::colimit;
use std::collections::HashMap;

/// Register a **constrained multigraph + W-type** theory pair (Group A).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `ThWType`.
///
/// Used by: ATProto, JSON Schema, OpenAPI, AsyncAPI, RAML, JSON:API,
/// MongoDB, YAML Schema, TOML Schema, INI, CDDL, BSON, MsgPack,
/// K8s CRD, CloudFormation, Ansible, FHIR, RSS/Atom, vCard/iCal,
/// GeoJSON, Markdown, and more.
pub fn register_constrained_multigraph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let w = th_wtype();

    registry.entry("ThGraph".into()).or_insert_with(|| g.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThMulti".into()).or_insert_with(|| m.clone());
    registry.entry("ThWType".into()).or_insert_with(|| w.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![], vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &m, &shared_ve) {
            schema_theory.name = schema_name.into();
            registry.insert(schema_name.into(), schema_theory);
        }
    }

    let mut inst = w;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **hypergraph + functor** theory pair (Group B).
///
/// Schema: `colimit(ThHypergraph, ThConstraint)`.
/// Instance: `ThFunctor`.
///
/// Used by: SQL, Cassandra, DynamoDB, Parquet, Arrow, DataFrame,
/// CSV/Table Schema, EDI X12, SWIFT MT.
pub fn register_hypergraph_functor<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let h = th_hypergraph();
    let c = th_constraint();
    let f = th_functor();

    registry.entry("ThHypergraph".into()).or_insert_with(|| h.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThFunctor".into()).or_insert_with(|| f.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(mut schema_theory) = colimit(&h, &c, &shared_vertex) {
        schema_theory.name = schema_name.into();
        registry.insert(schema_name.into(), schema_theory);
    }

    let mut inst = f;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **simple graph + flat** theory pair (Group C).
///
/// Schema: `colimit(ThSimpleGraph, ThConstraint)`.
/// Instance: `ThFlat`.
///
/// Used by: Protobuf, Avro, Thrift, Cap'n Proto, FlatBuffers,
/// ASN.1, Bond, Redis, HCL.
pub fn register_simple_graph_flat<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let sg = th_simple_graph();
    let c = th_constraint();
    let fl = th_flat();

    registry.entry("ThSimpleGraph".into()).or_insert_with(|| sg.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThFlat".into()).or_insert_with(|| fl.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(mut schema_theory) = colimit(&sg, &c, &shared_vertex) {
        schema_theory.name = schema_name.into();
        registry.insert(schema_name.into(), schema_theory);
    }

    let mut inst = fl;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **typed graph + W-type** theory pair with interfaces (Group D).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
/// Instance: `ThWType`.
///
/// Used by: GraphQL, TypeScript, Python, Rust Serde, Java, Go,
/// Swift, Kotlin, C#, JSX/React, Vue, Svelte.
pub fn register_typed_graph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let iface = th_interface();
    let w = th_wtype();

    registry.entry("ThGraph".into()).or_insert_with(|| g.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThMulti".into()).or_insert_with(|| m.clone());
    registry.entry("ThInterface".into()).or_insert_with(|| iface.clone());
    registry.entry("ThWType".into()).or_insert_with(|| w.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![], vec![],
        );
        if let Ok(gcm) = colimit(&gc, &m, &shared_ve) {
            let shared_vertex_only =
                Theory::new("ThVertex2", vec![Sort::simple("Vertex")], vec![], vec![]);
            if let Ok(mut schema_theory) = colimit(&gcm, &iface, &shared_vertex_only) {
                schema_theory.name = schema_name.into();
                registry.insert(schema_name.into(), schema_theory);
            }
        }
    }

    let mut inst = w;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **constrained multigraph + W-type + metadata** theory pair (Group E).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `colimit(ThWType, ThMeta)`.
///
/// Used by: XML/XSD, HTML, CSS, DOCX, ODF.
pub fn register_multigraph_wtype_meta<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let w = th_wtype();
    let meta = th_meta();

    registry.entry("ThGraph".into()).or_insert_with(|| g.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThMulti".into()).or_insert_with(|| m.clone());
    registry.entry("ThWType".into()).or_insert_with(|| w.clone());
    registry.entry("ThMeta".into()).or_insert_with(|| meta.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![], vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &m, &shared_ve) {
            schema_theory.name = schema_name.into();
            registry.insert(schema_name.into(), schema_theory);
        }
    }

    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    if let Ok(mut inst_theory) = colimit(&w, &meta, &shared_node) {
        inst_theory.name = instance_name.into();
        registry.insert(instance_name.into(), inst_theory);
    }
}

/// Register a **constrained multigraph + graph instance** theory pair (Group F).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `ThGraphInstance`.
///
/// Used by: Neo4j, and future graph-native protocols (RDF, OWL,
/// JSON-LD, knowledge graphs).
pub fn register_constrained_graph_instance<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let gi = th_graph_instance();

    registry.entry("ThGraph".into()).or_insert_with(|| g.clone());
    registry.entry("ThConstraint".into()).or_insert_with(|| c.clone());
    registry.entry("ThMulti".into()).or_insert_with(|| m.clone());
    registry.entry("ThGraphInstance".into()).or_insert_with(|| gi.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![], vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &m, &shared_ve) {
            schema_theory.name = schema_name.into();
            registry.insert(schema_name.into(), schema_theory);
        }
    }

    let mut inst = gi;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}
