//! Executable reference operations assigned to `validation`.

pub mod implementation {
    //! Whole-instance validation as predicates over an instance, the status
    //! aggregation, the budget law and the admission policies.
    //!
    //! Each shared check kind is a predicate read off its definition over a
    //! shape's graph presentation (a table's rows are nodes with their cells as
    //! fields and its pairs edges). Tree-only checks are evaluated before that
    //! presentation. The executable constraint carrier includes scalar length
    //! (characters, bytes and a stated grapheme subset), numeric bounds,
    //! enum/const, anchored-literal patterns, the closed format names,
    //! required values, degree, acyclicity, external verdicts, annotations and
    //! value parameters. A construct outside a stated subset produces typed
    //! incompleteness; it never makes the report disappear.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::exact::DecimalValue;
    use crate::fan::{FanViolation, candidates as fan_candidates, violations_at};
    use crate::keys;
    use crate::lens::ViewRef;
    use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Table, Tree};
    use crate::pages::representation;
    use crate::pages::representation::encode::{
        TableEmbedding, TableEncodeError, TreeEmbedding, TreeEncodeError,
    };
    use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
    use crate::schema::{KeyRole, KeyScope, OnUnresolved, RefSource, Reference, Schema};
    use crate::value::{Presence, Value};

    /// Definition: Two present indices match exactly when they are equal; two
    /// absent indices match exactly when `null_matches` is set; mixed presence
    /// never matches.
    ///
    /// Book anchors: `L-validation-16`.
    #[must_use]
    pub const fn index_and_null_match(
        index: Option<u64>,
        target: Option<u64>,
        null_matches: bool,
    ) -> bool {
        match (index, target) {
            (Some(left), Some(right)) => left == right,
            (None, None) => null_matches,
            _ => false,
        }
    }

    /// Definition: Lexical-space validation applies the declared recognizer to
    /// the literal source text exactly once.
    ///
    /// Book anchors: `L-validation-17`.
    #[must_use]
    pub fn lexical_space(text: &str, accepts: impl FnOnce(&str) -> bool) -> bool {
        accepts(text)
    }

    /// The semantics a protocol gives a constraint sort.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Semantics {
        /// No check.
        Annotation,
        /// A maximum length in characters (`true`) or UTF-8 bytes.
        MaxLength(bool),
        /// A minimum length in characters (`true`) or UTF-8 bytes.
        MinLength(bool),
        /// A maximum length in extended grapheme clusters.
        MaxGraphemes,
        /// A minimum length in extended grapheme clusters.
        MinGraphemes,
        /// An inclusive numeric lower bound.
        Minimum,
        /// An inclusive numeric upper bound.
        Maximum,
        /// A `|`-separated set of permitted canonical scalar spellings.
        Enum,
        /// One permitted canonical scalar spelling.
        Const,
        /// A settled anchored-literal pattern (`^literal$`). Other regex
        /// syntax is preserved as typed incompleteness.
        Pattern,
        /// One of the closed lexical formats named by the bound, under the
        /// format family named here.
        Format(Name),
        /// A lower bound on out-degree.
        MinOutDegree,
        /// An upper bound on out-degree.
        MaxOutDegree,
        /// A value is required.
        RequiredValue,
        /// The self-edge named by the value is acyclic.
        Acyclic,
        /// An application-supplied predicate, named by this constraint sort.
        External,
        /// A value-coercion parameter; it compiles to no validation check.
        ValueParameter,
        /// A semantics outside the reference's domain.
        Other,
    }

    /// Trusted verdicts and locality available to external/global checks.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ValidationContext {
        /// `(external check name, subject)` to pass/fail verdict.
        pub external: BTreeMap<(Name, ViewRef), bool>,
        /// Whether maintained-global checks see the whole instance.
        pub complete_segment: bool,
    }

    impl Default for ValidationContext {
        fn default() -> Self {
            Self {
                external: BTreeMap::new(),
                complete_segment: true,
            }
        }
    }

    /// The native carrier validated.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum InstanceShape {
        /// A graph.
        Graph,
        /// A table.
        Table,
        /// A rooted tree.
        Tree,
    }

    /// How much of an instance one check needs.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum CheckClass {
        /// Decided from declarations alone.
        Static,
        /// Decided from one element and its incident elements.
        GraphLocal,
        /// Decided from a whole-instance index or traversal.
        MaintainedGlobal,
        /// Decided by evidence outside the reference model.
        BoundedExternal,
    }

    /// A check kind, in its declaration order (which leads the evaluation and
    /// report order).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum CheckKind {
        /// The schema is well formed.
        SchemaWellFormed,
        /// The schema's declarations have supported meanings.
        DeclarationInterpretable,
        /// Every anchor is a schema vertex.
        NodeAnchor,
        /// A tree carries no reserved fan representation outside a fan.
        ReservedVocabulary,
        /// Every non-fan edge's label is a schema edge.
        EdgeDeclared,
        /// Every non-fan edge's endpoints are anchored at its label's ends.
        EndpointCoherence,
        /// Every fan is well formed.
        FanWellFormed,
        /// Required edges are present.
        RequiredEdge,
        /// Out-degrees lie in their bounds.
        OutDegree,
        /// Positions of an ordered edge are distinct per node.
        PositionDistinct,
        /// Values satisfy their constraints.
        ValueConstraint,
        /// Required values are present.
        RequiredValue,
        /// Attributes are declared and well typed.
        Attribute,
        /// A tree's root exists.
        TreeRoot,
        /// A tree node has the parent its arcs record.
        TreeParentMap,
        /// Keys are unique.
        KeyUnique,
        /// Sort-key targets are non-decreasing.
        KeyOrder,
        /// References resolve.
        ReferenceResolves,
        /// Declared self-edges are acyclic.
        Acyclic,
        /// Every tree node is reachable from its root.
        TreeReachable,
        /// A judgment supplied by an application.
        External,
    }

    impl CheckKind {
        /// The class fixed by V.2 for this kind.
        #[must_use]
        pub const fn class(self) -> CheckClass {
            match self {
                Self::SchemaWellFormed | Self::DeclarationInterpretable => CheckClass::Static,
                Self::NodeAnchor
                | Self::ReservedVocabulary
                | Self::EdgeDeclared
                | Self::EndpointCoherence
                | Self::FanWellFormed
                | Self::RequiredEdge
                | Self::OutDegree
                | Self::PositionDistinct
                | Self::ValueConstraint
                | Self::RequiredValue
                | Self::Attribute
                | Self::TreeRoot
                | Self::TreeParentMap => CheckClass::GraphLocal,
                Self::KeyUnique
                | Self::KeyOrder
                | Self::ReferenceResolves
                | Self::Acyclic
                | Self::TreeReachable => CheckClass::MaintainedGlobal,
                Self::External => CheckClass::BoundedExternal,
            }
        }

        /// Whether this kind applies to a native shape.
        #[must_use]
        pub const fn applies_to(self, shape: InstanceShape) -> bool {
            match self {
                Self::ReservedVocabulary
                | Self::TreeRoot
                | Self::TreeParentMap
                | Self::TreeReachable => matches!(shape, InstanceShape::Tree),
                Self::PositionDistinct | Self::Acyclic => matches!(shape, InstanceShape::Graph),
                _ => true,
            }
        }
    }

    /// What a check is about.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum SchemaRef {
        /// The whole schema.
        Schema,
        /// A vertex.
        Vertex(Name),
        /// An edge.
        Edge(SEdge),
        /// A required edge of a vertex.
        Required(Name, SEdge),
        /// An ordered edge.
        Ordering(SEdge),
        /// A constraint `(vertex, sort, value)`.
        Constraint(Name, Name, String),
        /// A key `(vertex, key)`.
        Key(Name, Name),
        /// A cross-vertex target set.
        TargetSet(Name),
        /// A reference.
        Reference(Name),
    }

    /// A compiled check.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct CheckKey {
        /// Kind.
        pub kind: CheckKind,
        /// Declaration.
        pub decl: SchemaRef,
    }

    /// Whether a check decides the status.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Requirement {
        /// It does.
        Required,
        /// It is reported only.
        Advisory,
    }

    /// A finding.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Violation {
        /// The anchor is not a vertex.
        UnknownAnchor(Name),
        /// The label is not an edge.
        UndeclaredEdge(SEdge),
        /// An endpoint is anchored elsewhere: `(source side?, expected, found)`.
        IncoherentEndpoint(bool, Name, Name),
        /// An endpoint is no node of the instance: `(source side?, node)`. An
        /// instance of the production shapes cannot hold such an edge; the
        /// reference model can, and reports it here rather than failing.
        DanglingEndpoint(bool, u32),
        /// A fan violation.
        MalformedFan(FanViolation),
        /// A native tree fan names no present parent node.
        MissingFanParent,
        /// A native tree fan child label names no present node.
        MissingFanChild(Name),
        /// A required edge is missing.
        MissingRequiredEdge(SEdge),
        /// An out-degree out of bounds: `(edge, min, max, found, edges)`.
        OutDegree(Option<SEdge>, Option<u32>, Option<u32>, u32, Vec<ViewRef>),
        /// Edges sharing a position.
        DuplicatePosition(u32, Vec<u32>),
        /// A value outside its bound: `(sort, bound, value)`.
        ValueConstraint(Name, String, Value),
        /// A missing value (`None` the own value, else the field).
        MissingValue(Option<String>),
        /// A duplicated key: its columns and values.
        DuplicateKey(Vec<(String, Value)>),
        /// An adjacent sort-key descent.
        KeyOrder(Name, Vec<ViewRef>),
        /// An element a `Within` key leaves outside its scope (advisory).
        OutsideKeyScope(Name, Name),
        /// A reference with no target.
        DanglingReference(Name, Vec<Value>),
        /// A reference with several targets.
        AmbiguousReference(Name, Vec<Value>, Vec<ViewRef>),
        /// A present guard value selected no declaration in its group.
        UnselectedReference(Name, Value),
        /// A cycle: its strongly connected component.
        Cycle(Vec<ViewRef>),
        /// A field has no declared property edge at its owner.
        UndeclaredAttribute(Name),
        /// A declared field has the wrong value kind: `(field, expected, found)`.
        MistypedAttribute(Name, Name, Name),
        /// An application-supplied predicate rejected the subject.
        ExternalRejected(Name),
        /// A tree's root is not one of its nodes.
        MissingRoot,
        /// A tree node is unreachable from the root.
        Unreachable,
        /// A node has another parent than its first parent arc.
        ParentMismatch(Option<u32>, Option<u32>),
        /// Reserved fan vocabulary occurs outside its representation.
        ReservedVocabulary,
    }

    /// A diagnostic.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Diagnostic {
        /// The check.
        pub check: CheckKey,
        /// How much of the instance this check needs.
        pub class: CheckClass,
        /// Its requirement.
        pub requirement: Requirement,
        /// The subject.
        pub subject: Option<ViewRef>,
        /// The elements named, ascending.
        pub elements: Vec<ViewRef>,
        /// The finding.
        pub violation: Violation,
    }

    /// A three-valued status.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum CheckStatus {
        /// Every required check passed.
        Passed,
        /// A required check failed.
        Failed,
        /// No required failure, but a required check is undecided.
        Incomplete,
    }

    /// Why a check could not decide its predicate.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum IncompleteReason {
        /// Evaluation stopped before this check instance.
        Budget {
            /// The read limit.
            limit: u64,
            /// Reads charged before the cut.
            spent: u64,
            /// The first skipped subject for this check.
            from: Option<ViewRef>,
        },
        /// The protocol gives no supported meaning to a sort.
        UnsupportedDeclaration {
            /// The unsupported sort.
            sort: Name,
        },
        /// A declaration is malformed.
        IllFormedDeclaration {
            /// The declaration error.
            detail: String,
        },
        /// The simplified reference carrier cannot evaluate this construct.
        Unsupported {
            /// The unsupported construct.
            detail: String,
        },
        /// A maintained-global delta-local check has no trusted baseline.
        NoValidatedBaseline,
        /// No trusted verdict was supplied for an external predicate.
        MissingExternalWitness {
            /// External constraint sort.
            name: Name,
        },
        /// A maintained-global check cannot be decided from one segment.
        OutsideSegment {
            /// The required check locality.
            class: CheckClass,
        },
        /// A native carrier could not be presented to the shared checks.
        NativeShapeRefusal {
            /// The native carrier shape.
            shape: InstanceShape,
            /// The exact refusal rendered deterministically.
            detail: String,
        },
    }

    /// One undecided check instance.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct IncompleteCheck {
        /// The check.
        pub check: CheckKey,
        /// Its class.
        pub class: CheckClass,
        /// Whether it decides status.
        pub requirement: Requirement,
        /// Its subject, if any.
        pub subject: Option<ViewRef>,
        /// Why it is undecided.
        pub reason: IncompleteReason,
    }

    impl CheckStatus {
        /// Kleene strong conjunction: `Failed` if either is, else `Incomplete`
        /// if either is, else `Passed`.
        #[must_use]
        pub const fn and(self, other: Self) -> Self {
            match (self, other) {
                (Self::Failed, _) | (_, Self::Failed) => Self::Failed,
                (Self::Incomplete, _) | (_, Self::Incomplete) => Self::Incomplete,
                _ => Self::Passed,
            }
        }

        /// Definition: Status aggregation is the fold of [`Self::and`] with unit `Passed`.
        ///
        /// Book anchors: `L-validation-1`, `L-validation-2`, `L-validation-3`,
        /// `L-validation-4`, `L-validation-5`, `L-validation-6`,
        /// `L-validation-7`, `L-validation-8`, `L-validation-9`,
        /// `L-validation-10`, `L-validation-11`.
        #[must_use]
        pub fn all(items: impl IntoIterator<Item = Self>) -> Self {
            items.into_iter().fold(Self::Passed, Self::and)
        }
    }

    /// Definition: The status of a report is `Failed` iff a required diagnostic exists, else
    /// `Incomplete` iff a required undecided check exists, else `Passed`.
    ///
    /// Book anchors: `L-validation-1`, `L-validation-2`, `L-validation-3`,
    /// `L-validation-4`, `L-validation-5`, `L-validation-6`,
    /// `L-validation-7`, `L-validation-8`, `L-validation-9`,
    /// `L-validation-10`, `L-validation-11`.
    #[must_use]
    pub fn status(diagnostics: &[Diagnostic], incomplete: &[IncompleteCheck]) -> CheckStatus {
        let failed = diagnostics
            .iter()
            .filter(|d| d.requirement == Requirement::Required)
            .map(|_| CheckStatus::Failed);
        let undecided = incomplete
            .iter()
            .filter(|entry| entry.requirement == Requirement::Required)
            .map(|_| CheckStatus::Incomplete);
        CheckStatus::all(failed.chain(undecided))
    }

    /// A validation report: diagnostics and undecided checks in check order,
    /// and the status.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Report {
        /// Native shape of the validated instance.
        pub shape: InstanceShape,
        /// Diagnostics, sorted by check, subject and elements.
        pub diagnostics: Vec<Diagnostic>,
        /// Undecided (ill-formed) check instances with their detail.
        pub incomplete: Vec<IncompleteCheck>,
        /// Interpretation of presentation identities in the native shape.
        pub identity: BTreeMap<ViewRef, NativeRef>,
        /// The status.
        pub status: CheckStatus,
    }

    /// Native identity named by a presentation reference.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum NativeRef {
        /// Graph node.
        GraphNode(u32),
        /// Graph edge.
        GraphEdge(u32),
        /// Table row.
        TableRow(u32),
        /// Table pair.
        TablePair(Box<SEdge>, u32, u32),
        /// Tree node.
        TreeNode(u32),
        /// Tree arc by native list index.
        TreeArc(usize),
        /// Tree fan by native list index.
        TreeFan(usize),
        /// The parent incidence of a native fan.
        TreeFanParent(usize),
        /// A labelled child incidence of a native fan.
        TreeFanChild(usize, Name),
    }

    fn non_fan(g: &Graph, n: u32) -> bool {
        g.nodes
            .get(&n)
            .is_some_and(|x| x.shape != NodeShape::HyperEdge)
    }

    fn fan_violation_elements(node: u32, violation: &FanViolation) -> Vec<ViewRef> {
        let mut elements = vec![ViewRef::Node(node)];
        match violation {
            FanViolation::MultipleParents { edges }
            | FanViolation::DuplicateLabel { edges, .. } => {
                elements.extend(edges.iter().copied().map(ViewRef::Edge));
            }
            FanViolation::ReservedKindOutsideFan { edge }
            | FanViolation::ForeignIncidentEdge { edge } => {
                elements.push(ViewRef::Edge(*edge));
            }
            FanViolation::MissingParent
            | FanViolation::UnknownLabel { .. }
            | FanViolation::UnknownHyperEdge { .. } => {}
        }
        elements
    }

    /// The value sites of vertex `v` at node `n`.
    ///
    /// Its own value when it is anchored at v, and for each `prop` edge e named c into v with `e.src`
    /// its anchor and no out-edge labelled e, its field c. `(None, ..)` is the own value; a site is
    /// `None` when absent or null-absent.
    #[must_use]
    pub fn value_sites(
        schema: &Schema,
        g: &Graph,
        v: &str,
        n: u32,
    ) -> Vec<(Option<String>, Option<Value>)> {
        let Some(node) = g.nodes.get(&n) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if node.anchor == v {
            let own = match &node.value {
                Some(Presence::Present(x)) if *x != Value::Null => Some(x.clone()),
                _ => None,
            };
            out.push((None, own));
        }
        for e in schema
            .edges
            .iter()
            .filter(|e| e.kind == "prop" && e.tgt == v && e.src == node.anchor)
        {
            let Some(c) = e.name.as_ref() else { continue };
            if g.edges.values().any(|x| x.src == n && x.label == *e) {
                continue;
            }
            out.push((
                Some(c.clone()),
                node.fields.get(c).filter(|x| **x != Value::Null).cloned(),
            ));
        }
        out
    }

    fn semantics_of<'a>(sem: &'a BTreeMap<Name, Semantics>, sort: &str) -> Option<&'a Semantics> {
        sem.get(sort)
    }

    fn diag(
        kind: CheckKind,
        decl: SchemaRef,
        subject: Option<ViewRef>,
        mut elements: Vec<ViewRef>,
        violation: Violation,
    ) -> Diagnostic {
        elements.sort();
        elements.dedup();
        Diagnostic {
            check: CheckKey { kind, decl },
            class: kind.class(),
            requirement: Requirement::Required,
            subject,
            elements,
            violation,
        }
    }

    const fn undecided(
        kind: CheckKind,
        decl: SchemaRef,
        requirement: Requirement,
        subject: Option<ViewRef>,
        reason: IncompleteReason,
    ) -> IncompleteCheck {
        IncompleteCheck {
            check: CheckKey { kind, decl },
            class: kind.class(),
            requirement,
            subject,
            reason,
        }
    }

    /// The structural kinds: anchors, edges, endpoints, fans, required edges
    /// and positions.
    fn structural(
        schema: &Schema,
        g: &Graph,
        shape: InstanceShape,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        structural_nodes(schema, g, out, incomplete);
        structural_edges(schema, g, shape, out);
        structural_fans(schema, g, shape, out);
        structural_required_and_positions(schema, g, shape, out);
    }

    // A tree's graph presentation also contains every native arc verbatim.
    // Reserved vocabulary does not turn such an Arc into a graph fan carrier:
    // only carriers sourced at the synthesized HyperEdge node belong to the
    // presentation fan. Original HyperEdge-shaped tree nodes were flattened
    // before that node was allocated, so the source shape separates the two.
    fn is_presentation_fan_edge(shape: InstanceShape, graph: &Graph, edge: &GEdge) -> bool {
        is_fan_kind(&edge.label.kind)
            && (shape != InstanceShape::Tree
                || graph
                    .nodes
                    .get(&edge.src)
                    .is_some_and(|node| node.shape == NodeShape::HyperEdge))
    }

    fn is_presentation_fan_node(shape: InstanceShape, graph: &Graph, node: u32) -> bool {
        shape != InstanceShape::Tree
            || graph
                .nodes
                .get(&node)
                .is_some_and(|item| item.shape == NodeShape::HyperEdge)
    }

    fn structural_nodes(
        schema: &Schema,
        g: &Graph,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        for (id, n) in g.nodes.iter().filter(|(i, _)| non_fan(g, **i)) {
            if !schema.vertices.contains_key(&n.anchor) {
                out.push(diag(
                    CheckKind::NodeAnchor,
                    SchemaRef::Vertex(n.anchor.clone()),
                    Some(ViewRef::Node(*id)),
                    vec![ViewRef::Node(*id)],
                    Violation::UnknownAnchor(n.anchor.clone()),
                ));
                // An unknown anchor has no declared attribute or constraint
                // interpretation.  Reporting those checks as well would guess
                // a declaration after NodeAnchor has already refused it.
                continue;
            }
            for (field, value) in &n.fields {
                let declared = schema.edges.iter().find(|edge| {
                    edge.src == n.anchor
                        && edge.kind == "prop"
                        && edge.name.as_deref() == Some(field)
                });
                let Some(declared) = declared else {
                    out.push(diag(
                        CheckKind::Attribute,
                        SchemaRef::Vertex(n.anchor.clone()),
                        Some(ViewRef::Node(*id)),
                        vec![ViewRef::Node(*id)],
                        Violation::UndeclaredAttribute(field.clone()),
                    ));
                    continue;
                };
                let Some(expected) = schema.vertices.get(&declared.tgt) else {
                    incomplete.push(undecided(
                        CheckKind::Attribute,
                        SchemaRef::Vertex(n.anchor.clone()),
                        Requirement::Required,
                        Some(ViewRef::Node(*id)),
                        IncompleteReason::Unsupported {
                            detail: format!(
                                "attribute {field:?} targets undeclared vertex {:?}",
                                declared.tgt
                            ),
                        },
                    ));
                    continue;
                };
                match attribute_kind_accepts(expected, value) {
                    Some(true) => {}
                    Some(false) => out.push(diag(
                        CheckKind::Attribute,
                        SchemaRef::Vertex(n.anchor.clone()),
                        Some(ViewRef::Node(*id)),
                        vec![ViewRef::Node(*id)],
                        Violation::MistypedAttribute(
                            field.clone(),
                            expected.clone(),
                            value_kind_name(value).to_owned(),
                        ),
                    )),
                    None => incomplete.push(undecided(
                        CheckKind::Attribute,
                        SchemaRef::Vertex(n.anchor.clone()),
                        Requirement::Required,
                        Some(ViewRef::Node(*id)),
                        IncompleteReason::Unsupported {
                            detail: format!(
                                "attribute {field:?} has unclassified declared kind {expected:?}"
                            ),
                        },
                    )),
                }
            }
        }
    }

    fn structural_edges(
        schema: &Schema,
        g: &Graph,
        shape: InstanceShape,
        out: &mut Vec<Diagnostic>,
    ) {
        for (id, e) in g
            .edges
            .iter()
            .filter(|(_, edge)| !is_presentation_fan_edge(shape, g, edge))
        {
            let me = ViewRef::Edge(*id);
            if !schema.edges.contains(&e.label) {
                out.push(diag(
                    CheckKind::EdgeDeclared,
                    SchemaRef::Edge(e.label.clone()),
                    Some(me.clone()),
                    vec![me.clone()],
                    Violation::UndeclaredEdge(e.label.clone()),
                ));
            }
            for (src_side, node, expected) in
                [(true, e.src, &e.label.src), (false, e.tgt, &e.label.tgt)]
            {
                let Some(found) = g.nodes.get(&node).map(|n| &n.anchor) else {
                    out.push(diag(
                        CheckKind::EndpointCoherence,
                        SchemaRef::Edge(e.label.clone()),
                        Some(me.clone()),
                        vec![me.clone()],
                        Violation::DanglingEndpoint(src_side, node),
                    ));
                    continue;
                };
                if found != expected {
                    let v =
                        Violation::IncoherentEndpoint(src_side, expected.clone(), found.clone());
                    out.push(diag(
                        CheckKind::EndpointCoherence,
                        SchemaRef::Edge(e.label.clone()),
                        Some(me.clone()),
                        vec![me.clone(), ViewRef::Node(node)],
                        v,
                    ));
                }
            }
        }
    }

    fn structural_fans(
        schema: &Schema,
        g: &Graph,
        shape: InstanceShape,
        out: &mut Vec<Diagnostic>,
    ) {
        for f in fan_candidates(g)
            .into_iter()
            .filter(|node| is_presentation_fan_node(shape, g, *node))
        {
            let he = g.nodes[&f].anchor.clone();
            for v in violations_at(g, f, Some(schema)) {
                let elements = fan_violation_elements(f, &v);
                out.push(diag(
                    CheckKind::FanWellFormed,
                    SchemaRef::Vertex(he.clone()),
                    Some(ViewRef::Node(f)),
                    elements,
                    Violation::MalformedFan(v),
                ));
            }
        }
    }

    const fn value_kind_name(value: &Value) -> &'static str {
        match value {
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::CidLink(_) => "cid",
            Value::Blob { .. } => "blob",
            Value::Token(_) => "token",
            Value::Null => "null",
            Value::Opaque { .. } => "opaque",
            Value::Unknown(_) => "object",
            Value::List(_) => "list",
            Value::LabeledNull(_) => "labeled-null",
            Value::Decimal(_) => "decimal",
            Value::BigInt(_) => "bigint",
            Value::Temporal(_) => "temporal",
        }
    }

    fn attribute_kind_accepts(kind: &str, value: &Value) -> Option<bool> {
        if matches!(value, Value::Null) {
            return Some(true);
        }
        let accepted = match kind {
            "string" | "str" | "text" => {
                matches!(value, Value::Str(_) | Value::Token(_) | Value::CidLink(_))
            }
            "token" | "enum" => matches!(value, Value::Token(_) | Value::Str(_)),
            "int" | "integer" | "long" | "short" | "i64" => {
                matches!(value, Value::Int(_))
            }
            "bigint" => matches!(value, Value::Int(_) | Value::BigInt(_)),
            "uint" | "unsigned" | "u64" => matches!(value, Value::Int(number) if *number >= 0),
            "float" | "double" | "number" => {
                matches!(value, Value::Float(_) | Value::Int(_))
            }
            "decimal" | "numeric" => {
                matches!(value, Value::Decimal(_) | Value::Int(_) | Value::BigInt(_))
            }
            "bool" | "boolean" => matches!(value, Value::Bool(_)),
            "bytes" | "binary" => matches!(value, Value::Bytes(_)),
            "blob" => matches!(value, Value::Blob { .. } | Value::Bytes(_)),
            "cid" | "cid-link" => matches!(value, Value::CidLink(_) | Value::Str(_)),
            "date" | "time" | "datetime" | "date-time" | "temporal" => {
                matches!(
                    value,
                    Value::Temporal(_) | Value::Str(_) | Value::Token(_) | Value::Int(_)
                )
            }
            "list" | "array" => matches!(value, Value::List(_)),
            "object" | "record" | "map" => {
                matches!(value, Value::Unknown(_) | Value::Opaque { .. })
            }
            "null" => matches!(value, Value::Null),
            _ => return None,
        };
        Some(accepted)
    }

    fn structural_required_and_positions(
        schema: &Schema,
        g: &Graph,
        shape: InstanceShape,
        out: &mut Vec<Diagnostic>,
    ) {
        for (id, n) in g
            .nodes
            .iter()
            .filter(|(i, n)| non_fan(g, **i) && schema.vertices.contains_key(&n.anchor))
        {
            for req in schema.required.get(&n.anchor).into_iter().flatten() {
                if !g.edges.values().any(|e| e.src == *id && e.label == *req) {
                    out.push(diag(
                        CheckKind::RequiredEdge,
                        SchemaRef::Required(n.anchor.clone(), req.clone()),
                        Some(ViewRef::Node(*id)),
                        vec![ViewRef::Node(*id)],
                        Violation::MissingRequiredEdge(req.clone()),
                    ));
                }
            }
            for ord in schema
                .orderings
                .keys()
                .filter(|o| shape == InstanceShape::Graph && o.src == n.anchor)
            {
                let mut by_pos: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
                for (eid, e) in g
                    .edges
                    .iter()
                    .filter(|(_, e)| e.src == *id && e.label == *ord)
                {
                    if let Some(p) = e.position {
                        by_pos.entry(p).or_default().push(*eid);
                    }
                }
                for (p, edges) in by_pos.into_iter().filter(|(_, v)| v.len() > 1) {
                    let mut elements: Vec<ViewRef> =
                        edges.iter().map(|e| ViewRef::Edge(*e)).collect();
                    elements.push(ViewRef::Node(*id));
                    out.push(diag(
                        CheckKind::PositionDistinct,
                        SchemaRef::Ordering(ord.clone()),
                        Some(ViewRef::Node(*id)),
                        elements,
                        Violation::DuplicatePosition(p, edges),
                    ));
                }
            }
        }
    }

    /// A value as an exact number (an integer, a big integer, a decimal, or a
    /// finite float at its shortest decimal).
    fn as_number(v: &Value) -> Option<DecimalValue> {
        match v {
            Value::Int(i) => DecimalValue::parse(&i.to_string()),
            Value::BigInt(b) => Some(crate::exact::ExactInteger::Big(b.clone()).as_decimal()),
            Value::Decimal(crate::exact::Decimal::Finite(d)) => Some(d.clone()),
            Value::Float(bits) => {
                let d = crate::float::shortest(crate::float::Format::B64, *bits)?;
                Some(DecimalValue {
                    negative: d.negative,
                    digits: d.coefficient,
                    exponent: crate::num::Int::from_i64(d.exponent),
                })
            }
            _ => None,
        }
    }

    fn length(v: &Value, chars: bool) -> Option<u64> {
        let n = match (v, chars) {
            (Value::Str(s) | Value::Token(s), true) => s.chars().count(),
            (Value::Str(s) | Value::Token(s), false) => s.len(),
            (Value::Bytes(b), false) => b.len(),
            (Value::List(items), _) => items.len(),
            _ => return None,
        };
        u64::try_from(n).ok()
    }

    fn grapheme_length(v: &Value) -> Option<u64> {
        let (Value::Str(text) | Value::Token(text)) = v else {
            return None;
        };
        let mut count = 0u64;
        for character in text.chars() {
            if ('\u{0300}'..='\u{036f}').contains(&character) {
                if count == 0 {
                    return None;
                }
            } else if character.is_ascii() && !character.is_control() {
                count = count.checked_add(1)?;
            } else {
                return None;
            }
        }
        Some(count)
    }

    fn scalar_text(value: &Value) -> Option<String> {
        match value {
            Value::Bool(value) => Some(value.to_string()),
            Value::Int(value) => Some(value.to_string()),
            Value::Str(value) | Value::Token(value) => Some(value.clone()),
            Value::Null => Some("null".to_owned()),
            _ => None,
        }
    }

    fn anchored_literal(pattern: &str) -> Option<&str> {
        let literal = pattern.strip_prefix('^')?.strip_suffix('$')?;
        (!literal
            .chars()
            .any(|character| ".*+?()[]{}|\\".contains(character)))
        .then_some(literal)
    }

    fn format_matches(family: &str, name: &str, value: &Value) -> Option<bool> {
        let text = match value {
            Value::Str(text) | Value::Token(text) => text.as_str(),
            _ => return None,
        };
        match name {
            "date" => exact_date(text),
            "time" => exact_time(text),
            "date-time" | "datetime" => {
                let (date, time) = text.split_once('T')?;
                Some(exact_date(date)? && exact_time(time)?)
            }
            "uri" => {
                let (scheme, rest) = text.split_once(':')?;
                Some(
                    !scheme.is_empty()
                        && scheme.chars().enumerate().all(|(index, character)| {
                            character.is_ascii_alphabetic()
                                || (index > 0
                                    && (character.is_ascii_digit()
                                        || matches!(character, '+' | '-' | '.')))
                        })
                        && !rest.chars().any(char::is_whitespace),
                )
            }
            "uuid" => {
                let groups: Vec<_> = text.split('-').collect();
                Some(
                    groups.iter().map(|group| group.len()).collect::<Vec<_>>() == [8, 4, 4, 4, 12]
                        && groups.iter().all(|group| {
                            group.chars().all(|character| character.is_ascii_hexdigit())
                        }),
                )
            }
            "ipv4" => Some(text.parse::<std::net::Ipv4Addr>().is_ok()),
            "ipv6" => Some(text.parse::<std::net::Ipv6Addr>().is_ok()),
            _ => super::super::lexical_formats::matches(family, name, text),
        }
    }

    fn exact_date(text: &str) -> Option<bool> {
        let bytes = text.as_bytes();
        if !text.is_ascii()
            || bytes.len() != 10
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
        {
            return None;
        }
        let year = text[0..4].parse::<u32>().ok()?;
        let month = text[5..7].parse::<u8>().ok()?;
        let day = text[8..10].parse::<u8>().ok()?;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let last = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        Some((1..=last).contains(&day))
    }

    fn exact_time(text: &str) -> Option<bool> {
        if !text.is_ascii() {
            return None;
        }
        let (clock, offset) = if let Some(clock) = text.strip_suffix('Z') {
            (clock, None)
        } else if text.len() == 14 && matches!(text.as_bytes()[8], b'+' | b'-') {
            (&text[..8], Some(&text[8..]))
        } else {
            return None;
        };
        let bytes = clock.as_bytes();
        if bytes.len() != 8
            || bytes[2] != b':'
            || bytes[5] != b':'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| !matches!(index, 2 | 5) && !byte.is_ascii_digit())
        {
            return None;
        }
        let clock_valid = clock[0..2].parse::<u8>().ok()? <= 23
            && clock[3..5].parse::<u8>().ok()? <= 59
            && clock[6..8].parse::<u8>().ok()? <= 59;
        let offset_valid = offset.is_none_or(|offset| {
            let bytes = offset.as_bytes();
            bytes.len() == 6
                && matches!(bytes[0], b'+' | b'-')
                && bytes[3] == b':'
                && bytes[1..3].iter().all(u8::is_ascii_digit)
                && bytes[4..6].iter().all(u8::is_ascii_digit)
                && offset[1..3].parse::<u8>().is_ok_and(|hour| hour <= 23)
                && offset[4..6].parse::<u8>().is_ok_and(|minute| minute <= 59)
        });
        Some(clock_valid && offset_valid)
    }

    /// One value constraint at one value: `Some(true)` passes, `Some(false)`
    /// fails, `None` undecidable for the value's kind.
    fn passes(sem: &Semantics, bound: &str, v: &Value) -> Option<bool> {
        match sem {
            Semantics::MaxLength(chars) => Some(length(v, *chars)? <= bound.parse::<u64>().ok()?),
            Semantics::MinLength(chars) => Some(length(v, *chars)? >= bound.parse::<u64>().ok()?),
            Semantics::MaxGraphemes => Some(grapheme_length(v)? <= bound.parse::<u64>().ok()?),
            Semantics::MinGraphemes => Some(grapheme_length(v)? >= bound.parse::<u64>().ok()?),
            Semantics::Minimum => Some(
                as_number(v)?
                    .cmp_numeric(&DecimalValue::parse(bound)?)
                    .is_ge(),
            ),
            Semantics::Maximum => Some(
                as_number(v)?
                    .cmp_numeric(&DecimalValue::parse(bound)?)
                    .is_le(),
            ),
            Semantics::Enum => {
                let scalar = scalar_text(v)?;
                Some(bound.split('|').any(|member| scalar == member))
            }
            Semantics::Const => Some(scalar_text(v)?.as_str() == bound),
            Semantics::Pattern => Some(anchored_literal(bound)? == scalar_text(v)?.as_str()),
            Semantics::Format(family) => format_matches(family, bound, v),
            _ => Some(true),
        }
    }

    /// The constraint kinds: value bounds, required values and out-degrees.
    struct ConstraintSite<'a> {
        schema: &'a Schema,
        graph: &'a Graph,
        shape: InstanceShape,
        vertex: &'a str,
        node: u32,
        declaration: &'a SchemaRef,
        subject: &'a ViewRef,
    }

    fn constraints(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        g: &Graph,
        shape: InstanceShape,
        out: &mut Vec<Diagnostic>,
        inc: &mut Vec<IncompleteCheck>,
        context: &ValidationContext,
    ) {
        for (v, cs) in &schema.constraints {
            for (sort, bound) in cs {
                let Some(s) = semantics_of(sem, sort) else {
                    continue;
                };
                let decl = SchemaRef::Constraint(v.clone(), sort.clone(), bound.clone());
                for (id, n) in g
                    .nodes
                    .iter()
                    .filter(|(i, n)| non_fan(g, **i) && schema.vertices.contains_key(&n.anchor))
                {
                    let me = ViewRef::Node(*id);
                    let site = ConstraintSite {
                        schema,
                        graph: g,
                        shape,
                        vertex: v,
                        node: *id,
                        declaration: &decl,
                        subject: &me,
                    };
                    match s {
                        Semantics::MinOutDegree | Semantics::MaxOutDegree if n.anchor == *v => {
                            degree_constraint(&site, s, bound, out, inc);
                        }
                        Semantics::RequiredValue => {
                            required_value_constraint(&site, out);
                        }
                        Semantics::MaxLength(_)
                        | Semantics::MinLength(_)
                        | Semantics::MaxGraphemes
                        | Semantics::MinGraphemes
                        | Semantics::Minimum
                        | Semantics::Maximum
                        | Semantics::Enum
                        | Semantics::Const
                        | Semantics::Pattern
                        | Semantics::Format(_) => {
                            value_constraint(&site, sort, bound, s, out, inc);
                        }
                        Semantics::External if !value_sites(schema, g, v, *id).is_empty() => {
                            external_constraint(context, sort, &decl, &me, out, inc);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn degree_constraint(
        site: &ConstraintSite<'_>,
        semantics: &Semantics,
        bound: &str,
        diagnostics: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        let Ok(limit) = bound.parse::<u32>() else {
            incomplete.push(undecided(
                CheckKind::OutDegree,
                site.declaration.clone(),
                Requirement::Required,
                Some(site.subject.clone()),
                IncompleteReason::IllFormedDeclaration {
                    detail: "ill-formed out-degree bound".into(),
                },
            ));
            return;
        };
        let counted: Vec<ViewRef> = site
            .graph
            .edges
            .iter()
            .filter(|(_, edge)| {
                edge.src == site.node && !is_presentation_fan_edge(site.shape, site.graph, edge)
            })
            .map(|(id, _)| ViewRef::Edge(*id))
            .collect();
        let found = u32::try_from(counted.len()).unwrap_or(u32::MAX);
        let (min, max) = if *semantics == Semantics::MinOutDegree {
            (Some(limit), None)
        } else {
            (None, Some(limit))
        };
        if min.is_some_and(|minimum| found < minimum) || max.is_some_and(|maximum| found > maximum)
        {
            let mut elements = counted.clone();
            elements.push(site.subject.clone());
            diagnostics.push(diag(
                CheckKind::OutDegree,
                site.declaration.clone(),
                Some(site.subject.clone()),
                elements,
                Violation::OutDegree(None, min, max, found, counted),
            ));
        }
    }

    fn required_value_constraint(site: &ConstraintSite<'_>, diagnostics: &mut Vec<Diagnostic>) {
        for (field, value) in value_sites(site.schema, site.graph, site.vertex, site.node) {
            if value.is_none() {
                diagnostics.push(diag(
                    CheckKind::RequiredValue,
                    site.declaration.clone(),
                    Some(site.subject.clone()),
                    vec![site.subject.clone()],
                    Violation::MissingValue(field),
                ));
            }
        }
    }

    fn value_constraint(
        site: &ConstraintSite<'_>,
        sort: &str,
        bound: &str,
        semantics: &Semantics,
        diagnostics: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        for (_, value) in value_sites(site.schema, site.graph, site.vertex, site.node) {
            let Some(value) = value else { continue };
            match passes(semantics, bound, &value) {
                Some(true) => {}
                Some(false) => diagnostics.push(diag(
                    CheckKind::ValueConstraint,
                    site.declaration.clone(),
                    Some(site.subject.clone()),
                    vec![site.subject.clone()],
                    Violation::ValueConstraint(sort.to_owned(), bound.to_owned(), value),
                )),
                None => incomplete.push(undecided(
                    CheckKind::ValueConstraint,
                    site.declaration.clone(),
                    Requirement::Required,
                    Some(site.subject.clone()),
                    IncompleteReason::Unsupported {
                        detail: format!(
                            "{sort} cannot decide value kind {} in the settled subset",
                            value_kind_name(&value)
                        ),
                    },
                )),
            }
        }
    }

    fn external_constraint(
        context: &ValidationContext,
        sort: &str,
        declaration: &SchemaRef,
        subject: &ViewRef,
        diagnostics: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        match context.external.get(&(sort.to_owned(), subject.clone())) {
            Some(true) => {}
            Some(false) => diagnostics.push(diag(
                CheckKind::External,
                declaration.clone(),
                Some(subject.clone()),
                vec![subject.clone()],
                Violation::ExternalRejected(sort.to_owned()),
            )),
            None => incomplete.push(undecided(
                CheckKind::External,
                declaration.clone(),
                Requirement::Required,
                Some(subject.clone()),
                IncompleteReason::MissingExternalWitness {
                    name: sort.to_owned(),
                },
            )),
        }
    }

    /// Key uniqueness and primary-key required parts.
    fn nearest_preceding_binders(schema: &Schema) -> BTreeSet<(&str, &str)> {
        schema
            .references
            .values()
            .filter_map(|reference| match &reference.source {
                RefSource::Variable {
                    binder_key,
                    binding: crate::schema::VariableBinding::NearestPreceding,
                    ..
                } => Some((reference.target_vertex.as_str(), binder_key.as_str())),
                _ => None,
            })
            .collect()
    }

    fn keyed(
        schema: &Schema,
        g: &Graph,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        let nearest_preceding_binders = nearest_preceding_binders(schema);
        for (v, ks) in &schema.keys {
            for key in ks {
                let decl = SchemaRef::Key(v.clone(), key.id.clone());
                if key.role == KeyRole::Index
                    || nearest_preceding_binders.contains(&(v.as_str(), key.id.as_str()))
                {
                    continue;
                }
                if key.role == KeyRole::SortKey {
                    validate_sort_key(schema, g, v, key, out, incomplete);
                    continue;
                }
                let (buckets, outside) = match keys::buckets(g, v, key) {
                    Ok(result) => result,
                    Err(error) => {
                        incomplete.push(undecided(
                            CheckKind::KeyUnique,
                            decl.clone(),
                            Requirement::Required,
                            None,
                            IncompleteReason::Unsupported {
                                detail: format!("{error:?}"),
                            },
                        ));
                        continue;
                    }
                };
                for (values, elems) in buckets.into_values().filter(|(_, elems)| elems.len() > 1) {
                    let cols: Vec<(String, Value)> = key
                        .parts
                        .iter()
                        .map(|p| match p {
                            crate::schema::KeyPart::Path(path) => path
                                .last()
                                .map(|e| e.column().to_owned())
                                .unwrap_or_default(),
                            crate::schema::KeyPart::Position { within } => {
                                within.column().to_owned()
                            }
                        })
                        .zip(values)
                        .collect();
                    out.push(diag(
                        CheckKind::KeyUnique,
                        decl.clone(),
                        None,
                        elems.into_iter().map(ViewRef::Node).collect(),
                        Violation::DuplicateKey(cols),
                    ));
                }
                if let KeyScope::Within(scope) = &key.scope {
                    for n in outside {
                        let mut d = diag(
                            CheckKind::KeyUnique,
                            decl.clone(),
                            Some(ViewRef::Node(n)),
                            vec![ViewRef::Node(n)],
                            Violation::OutsideKeyScope(key.id.clone(), scope.clone()),
                        );
                        d.requirement = Requirement::Advisory;
                        out.push(d);
                    }
                }
                if key.role == KeyRole::Primary {
                    for (id, _) in g
                        .nodes
                        .iter()
                        .filter(|(i, n)| n.anchor == *v && non_fan(g, **i))
                    {
                        for p in &key.parts {
                            let mut one_part = key.clone();
                            one_part.parts = vec![p.clone()];
                            one_part.equality.clear();
                            if keys::key_value(g, *id, &one_part).is_none() {
                                let field = match p {
                                    crate::schema::KeyPart::Path(path) => {
                                        path.last().map(|e| e.column().to_owned())
                                    }
                                    crate::schema::KeyPart::Position { within } => {
                                        Some(within.column().to_owned())
                                    }
                                };
                                let mut finding = diag(
                                    CheckKind::RequiredValue,
                                    decl.clone(),
                                    Some(ViewRef::Node(*id)),
                                    vec![ViewRef::Node(*id)],
                                    Violation::MissingValue(field),
                                );
                                finding.class = primary_required_class(key);
                                out.push(finding);
                            }
                        }
                    }
                }
            }
        }
        validate_target_sets(schema, g, out, incomplete);
    }

    fn tuple_order(left: &[Value], right: &[Value]) -> Option<std::cmp::Ordering> {
        for (left, right) in left.iter().zip(right) {
            let order = keys::sort_order(left, right)?;
            if !order.is_eq() {
                return Some(order);
            }
        }
        (left.len() == right.len()).then_some(std::cmp::Ordering::Equal)
    }

    fn validate_sort_key(
        schema: &Schema,
        graph: &Graph,
        vertex: &str,
        key: &crate::schema::Key,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        let KeyScope::Within(container) = &key.scope else {
            return;
        };
        let carriers: Vec<_> = schema
            .edges
            .iter()
            .filter(|edge| edge.src == *container && edge.tgt == vertex)
            .collect();
        let [carrier] = carriers.as_slice() else {
            return;
        };
        let decl = SchemaRef::Key(vertex.to_owned(), key.id.clone());
        for (parent, _) in graph
            .nodes
            .iter()
            .filter(|(id, node)| node.anchor == *container && non_fan(graph, **id))
        {
            let mut children: Vec<(Option<u32>, u32, u32)> = graph
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == *parent && edge.label == **carrier)
                .map(|(id, edge)| (edge.position, *id, edge.tgt))
                .collect();
            children.sort();
            for pair in children.windows(2) {
                let (Some(left), Some(right)) = (
                    keys::key_value(graph, pair[0].2, key),
                    keys::key_value(graph, pair[1].2, key),
                ) else {
                    incomplete.push(undecided(
                        CheckKind::KeyOrder,
                        decl.clone(),
                        Requirement::Advisory,
                        Some(ViewRef::Node(*parent)),
                        IncompleteReason::Unsupported {
                            detail: format!("OutsideSortOrder({})", key.id),
                        },
                    ));
                    continue;
                };
                match tuple_order(&left, &right) {
                    Some(order) if order.is_gt() => {
                        let mut diagnostic = diag(
                            CheckKind::KeyOrder,
                            decl.clone(),
                            Some(ViewRef::Node(*parent)),
                            vec![ViewRef::Node(pair[0].2), ViewRef::Node(pair[1].2)],
                            Violation::KeyOrder(
                                key.id.clone(),
                                vec![ViewRef::Node(pair[0].2), ViewRef::Node(pair[1].2)],
                            ),
                        );
                        diagnostic.requirement = Requirement::Advisory;
                        out.push(diagnostic);
                    }
                    Some(_) => {}
                    None => incomplete.push(undecided(
                        CheckKind::KeyOrder,
                        decl.clone(),
                        Requirement::Advisory,
                        Some(ViewRef::Node(*parent)),
                        IncompleteReason::Unsupported {
                            detail: format!("OutsideSortOrder({})", key.id),
                        },
                    )),
                }
            }
        }
    }

    type TargetBucket = (Vec<Value>, Vec<u32>);

    fn validate_target_sets(
        schema: &Schema,
        graph: &Graph,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        for (set_id, set) in &schema.target_sets {
            let Some((first_vertex, first_key_id)) = set.members.first() else {
                continue;
            };
            let Some(first_key) = schema
                .keys
                .get(first_vertex)
                .and_then(|keys| keys.iter().find(|key| key.id == *first_key_id))
            else {
                continue;
            };
            let decl = SchemaRef::TargetSet(set_id.clone());
            let mut buckets: BTreeMap<(Option<u32>, Vec<u8>), TargetBucket> = BTreeMap::new();
            for (vertex, key_id) in &set.members {
                let Some(key) = schema
                    .keys
                    .get(vertex)
                    .and_then(|keys| keys.iter().find(|key| key.id == *key_id))
                else {
                    continue;
                };
                for (node, _) in graph
                    .nodes
                    .iter()
                    .filter(|(id, candidate)| candidate.anchor == *vertex && non_fan(graph, **id))
                {
                    let Some(tuple) = keys::key_value(graph, *node, key) else {
                        continue;
                    };
                    let scope = match &set.scope {
                        KeyScope::Within(owner) => {
                            match keys::nearest_ancestor(graph, *node, owner) {
                                Some(scope) => Some(scope),
                                None => continue,
                            }
                        }
                        KeyScope::Document | KeyScope::Global => None,
                    };
                    let key_bytes = match keys::tuple_key(first_key, &tuple) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            incomplete.push(undecided(
                                CheckKind::KeyUnique,
                                decl.clone(),
                                Requirement::Required,
                                Some(ViewRef::Node(*node)),
                                IncompleteReason::Unsupported {
                                    detail: format!("{error:?}"),
                                },
                            ));
                            continue;
                        }
                    };
                    buckets
                        .entry((scope, key_bytes))
                        .or_insert_with(|| (tuple, Vec::new()))
                        .1
                        .push(*node);
                }
            }
            for (tuple, nodes) in buckets.into_values().filter(|(_, nodes)| nodes.len() > 1) {
                let columns = tuple
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| (format!("part-{index}"), value))
                    .collect();
                out.push(diag(
                    CheckKind::KeyUnique,
                    decl.clone(),
                    None,
                    nodes.into_iter().map(ViewRef::Node).collect(),
                    Violation::DuplicateKey(columns),
                ));
            }
        }
    }

    /// Reference resolution on the finite graph carrier. Unsupported route
    /// services are reported as required incomplete checks rather than making
    /// the entire validation call disappear.
    fn references(
        schema: &Schema,
        g: &Graph,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        for reference in schema.references.values() {
            for (h, _) in g
                .nodes
                .iter()
                .filter(|(i, n)| n.anchor == reference.holder && non_fan(g, **i))
            {
                validate_reference_holder(schema, g, reference, *h, out, incomplete);
            }
        }
        unselected_guards(schema, g, out);
    }

    fn reference_requirement(reference: &Reference) -> Requirement {
        if reference.on_unresolved == OnUnresolved::Error {
            Requirement::Required
        } else {
            Requirement::Advisory
        }
    }

    fn incomplete_reference(
        reference: &Reference,
        holder: u32,
        detail: impl std::fmt::Debug,
    ) -> IncompleteCheck {
        undecided(
            CheckKind::ReferenceResolves,
            SchemaRef::Reference(reference.id.clone()),
            reference_requirement(reference),
            Some(ViewRef::Node(holder)),
            IncompleteReason::Unsupported {
                detail: format!("{detail:?}"),
            },
        )
    }

    fn push_reference_diag(
        out: &mut Vec<Diagnostic>,
        reference: &Reference,
        holder: u32,
        elements: Vec<ViewRef>,
        violation: Violation,
    ) {
        let mut diagnostic = diag(
            CheckKind::ReferenceResolves,
            SchemaRef::Reference(reference.id.clone()),
            Some(ViewRef::Node(holder)),
            elements,
            violation,
        );
        diagnostic.requirement = reference_requirement(reference);
        out.push(diagnostic);
    }

    fn validate_reference_holder(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        holder: u32,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        if let Some(guard) = &reference.guard
            && keys::path_value(graph, holder, &guard.field)
                .as_ref()
                .is_none_or(|actual| {
                    crate::value::representation(actual, &guard.equals) != Some(true)
                })
        {
            return;
        }
        let tuples = match crate::resolution::reference_tuples(schema, graph, holder, reference) {
            Ok(tuples) => tuples,
            Err(detail) => {
                incomplete.push(incomplete_reference(reference, holder, detail));
                return;
            }
        };
        if tuples.is_empty() {
            if !reference.nullable {
                let field = missing_reference_field(reference);
                push_reference_diag(
                    out,
                    reference,
                    holder,
                    vec![ViewRef::Node(holder)],
                    Violation::MissingValue(field),
                );
            }
            return;
        }
        for values in tuples {
            validate_reference_tuple(schema, graph, reference, holder, values, out, incomplete);
        }
    }

    fn missing_reference_field(reference: &Reference) -> Option<String> {
        match &reference.source {
            RefSource::KeyValue { parts } => parts.first().map(Vec::as_slice),
            RefSource::Path { field, .. }
            | RefSource::Variable { field, .. }
            | RefSource::Inline { field } => Some(field.as_slice()),
            RefSource::Standoff { start, .. } => Some(start.as_slice()),
        }
        .and_then(|path| path.last())
        .map(|edge| edge.column().to_owned())
    }

    fn validate_reference_tuple(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        holder: u32,
        values: Vec<Value>,
        out: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        match crate::resolution::reference_is_null(schema, graph, reference, &values) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                incomplete.push(incomplete_reference(reference, holder, error));
                return;
            }
        }
        let candidates = match crate::resolution::reference_candidates(
            schema, graph, reference, holder, &values,
        ) {
            Ok(candidates) => candidates
                .into_iter()
                .map(ViewRef::Node)
                .collect::<Vec<_>>(),
            Err(error) => {
                incomplete.push(incomplete_reference(reference, holder, error));
                return;
            }
        };
        match candidates.len() {
            1 => {}
            0 => push_reference_diag(
                out,
                reference,
                holder,
                vec![ViewRef::Node(holder)],
                Violation::DanglingReference(reference.id.clone(), values),
            ),
            _ => {
                let mut elements = candidates.clone();
                elements.push(ViewRef::Node(holder));
                push_reference_diag(
                    out,
                    reference,
                    holder,
                    elements,
                    Violation::AmbiguousReference(reference.id.clone(), values, candidates),
                );
            }
        }
    }

    fn unselected_guards(schema: &Schema, g: &Graph, out: &mut Vec<Diagnostic>) {
        let guarded: Vec<_> = schema
            .references
            .values()
            .filter_map(|reference| {
                reference.guard.as_ref().map(|guard| {
                    (
                        reference.holder.as_str(),
                        &reference.source,
                        guard.field.as_slice(),
                        reference.id.as_str(),
                        &guard.equals,
                    )
                })
            })
            .collect();
        for (index, (holder_anchor, source, field, _, _)) in guarded.iter().enumerate() {
            if guarded[..index]
                .iter()
                .any(|(other_holder, other_source, other_field, _, _)| {
                    other_holder == holder_anchor && other_source == source && other_field == field
                })
            {
                continue;
            }
            let mut declarations: Vec<_> = guarded
                .iter()
                .filter(|(other_holder, other_source, other_field, _, _)| {
                    other_holder == holder_anchor && other_source == source && other_field == field
                })
                .map(|(_, _, _, reference, equals)| (*reference, *equals))
                .collect();
            declarations.sort_by_key(|(reference, _)| *reference);
            let reference = declarations[0].0.to_owned();
            for holder in g.nodes.iter().filter_map(|(id, node)| {
                (node.anchor == *holder_anchor && non_fan(g, *id)).then_some(*id)
            }) {
                let Some(value) = keys::path_value(g, holder, field) else {
                    continue;
                };
                if declarations
                    .iter()
                    .any(|(_, guard)| crate::value::representation(&value, guard) == Some(true))
                {
                    continue;
                }
                let check = CheckKey {
                    kind: CheckKind::ReferenceResolves,
                    decl: SchemaRef::Reference(reference.clone()),
                };
                out.push(Diagnostic {
                    check,
                    class: CheckKind::ReferenceResolves.class(),
                    requirement: Requirement::Advisory,
                    subject: Some(ViewRef::Node(holder)),
                    elements: vec![ViewRef::Node(holder)],
                    violation: Violation::UnselectedReference(reference.clone(), value),
                });
            }
        }
    }

    /// Acyclicity of declared self-edges: every strongly connected component
    /// of the subgraph of edges labelled `(v, v, name)` with an edge inside it
    /// (a self-loop counts) is a cycle.
    fn acyclic(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        g: &Graph,
        out: &mut Vec<Diagnostic>,
    ) {
        for (v, cs) in &schema.constraints {
            for (sort, name) in cs {
                if semantics_of(sem, sort) != Some(&Semantics::Acyclic) {
                    continue;
                }
                let label = SEdge {
                    src: v.clone(),
                    tgt: v.clone(),
                    kind: schema
                        .edges
                        .iter()
                        .find(|e| {
                            e.src == *v && e.tgt == *v && e.name.as_deref() == Some(name.as_str())
                        })
                        .map_or_else(String::new, |e| e.kind.clone()),
                    name: Some(name.clone()),
                };
                let edges: Vec<(u32, u32)> = g
                    .edges
                    .values()
                    .filter(|e| e.label == label)
                    .map(|e| (e.src, e.tgt))
                    .collect();
                let reach = |a: u32, b: u32| {
                    let mut seen = BTreeSet::new();
                    let mut stack = vec![a];
                    while let Some(x) = stack.pop() {
                        for (s, t) in &edges {
                            if *s == x && seen.insert(*t) {
                                stack.push(*t);
                            }
                        }
                    }
                    seen.contains(&b)
                };
                let nodes: BTreeSet<u32> = edges.iter().flat_map(|(s, t)| [*s, *t]).collect();
                let mut done = BTreeSet::new();
                for n in &nodes {
                    if done.contains(n) || !reach(*n, *n) {
                        continue;
                    }
                    let scc: Vec<ViewRef> = nodes
                        .iter()
                        .filter(|m| reach(*n, **m) && reach(**m, *n))
                        .map(|m| ViewRef::Node(*m))
                        .collect();
                    for m in &scc {
                        if let ViewRef::Node(x) = m {
                            done.insert(*x);
                        }
                    }
                    out.push(diag(
                        CheckKind::Acyclic,
                        SchemaRef::Constraint(v.clone(), sort.clone(), name.clone()),
                        None,
                        scc.clone(),
                        Violation::Cycle(scc),
                    ));
                }
            }
        }
    }

    const fn dependent_constraint_checks(semantics: Option<&Semantics>) -> &'static [CheckKind] {
        match semantics {
            Some(
                Semantics::MaxLength(_)
                | Semantics::MinLength(_)
                | Semantics::MaxGraphemes
                | Semantics::MinGraphemes
                | Semantics::Minimum
                | Semantics::Maximum
                | Semantics::Enum
                | Semantics::Const
                | Semantics::Pattern
                | Semantics::Format(_),
            ) => &[CheckKind::ValueConstraint],
            Some(Semantics::MinOutDegree | Semantics::MaxOutDegree) => &[CheckKind::OutDegree],
            Some(Semantics::RequiredValue) => &[CheckKind::RequiredValue],
            Some(Semantics::Acyclic) => &[CheckKind::Acyclic],
            Some(Semantics::External) => &[CheckKind::External],
            None | Some(Semantics::Annotation | Semantics::ValueParameter | Semantics::Other) => {
                &[]
            }
        }
    }

    const fn dependent_key_checks(role: KeyRole) -> &'static [CheckKind] {
        match role {
            KeyRole::Primary => &[CheckKind::KeyUnique, CheckKind::RequiredValue],
            KeyRole::Unique | KeyRole::Identifier => &[CheckKind::KeyUnique],
            KeyRole::SortKey => &[CheckKind::KeyOrder],
            KeyRole::Index => &[],
        }
    }

    fn key_dependents(schema: &Schema, vertex: &str, key: &str) -> BTreeSet<CheckKey> {
        schema
            .keys
            .get(vertex)
            .into_iter()
            .flatten()
            .filter(|declaration| declaration.id == key)
            .flat_map(|declaration| dependent_key_checks(declaration.role))
            .map(|kind| CheckKey {
                kind: *kind,
                decl: SchemaRef::Key(vertex.to_owned(), key.to_owned()),
            })
            .collect()
    }

    fn requirement_for_check(schema: &Schema, check: &CheckKey) -> Requirement {
        match (&check.kind, &check.decl) {
            (CheckKind::KeyOrder, _) => Requirement::Advisory,
            (CheckKind::ReferenceResolves, SchemaRef::Reference(id)) => schema
                .references
                .get(id)
                .map_or(Requirement::Required, reference_requirement),
            _ => Requirement::Required,
        }
    }

    fn class_for_check(schema: &Schema, check: &CheckKey) -> CheckClass {
        let SchemaRef::Key(vertex, key_id) = &check.decl else {
            return check.kind.class();
        };
        if check.kind != CheckKind::RequiredValue {
            return check.kind.class();
        }
        schema
            .keys
            .get(vertex)
            .into_iter()
            .flatten()
            .filter(|key| key.id == *key_id && key.role == KeyRole::Primary)
            .map(primary_required_class)
            .max()
            .unwrap_or_else(|| check.kind.class())
    }

    fn declaration_error_dependents(
        schema: &Schema,
        error: &crate::schema::DeclarationError,
    ) -> BTreeSet<CheckKey> {
        use crate::schema::DeclarationError as Error;

        let reference = |id: &str| {
            BTreeSet::from([CheckKey {
                kind: CheckKind::ReferenceResolves,
                decl: SchemaRef::Reference(id.to_owned()),
            }])
        };
        let target_set = |set: &str| {
            let mut out = BTreeSet::from([CheckKey {
                kind: CheckKind::KeyUnique,
                decl: SchemaRef::TargetSet(set.to_owned()),
            }]);
            out.extend(
                schema
                    .references
                    .values()
                    .filter(|declaration| declaration.target_vertex == set)
                    .map(|declaration| CheckKey {
                        kind: CheckKind::ReferenceResolves,
                        decl: SchemaRef::Reference(declaration.id.clone()),
                    }),
            );
            out
        };
        match error {
            Error::EqualityArity { vertex, key, .. }
            | Error::DuplicateKeyId { vertex, key }
            | Error::SortKeyNotOrdered { vertex, key } => key_dependents(schema, vertex, key),
            Error::TargetSetNameClash { set }
            | Error::EmptyTargetSet { set }
            | Error::TargetSetMemberMismatch { set, .. } => target_set(set),
            Error::DuplicateGuard { references } | Error::BinderModeConflict { references, .. } => {
                references.iter().flat_map(|id| reference(id)).collect()
            }
            Error::KeyNotResolvable { reference: id, .. }
            | Error::UnknownTarget { reference: id }
            | Error::ReferenceArity { reference: id, .. }
            | Error::InlineRequiresRedirect { reference: id }
            | Error::InlineRequiresTargetSet { reference: id }
            | Error::LengthInclusive { reference: id }
            | Error::InvalidFragments { reference: id }
            | Error::EdgeMismatch { reference: id }
            | Error::TargetSetHasKey { reference: id }
            | Error::VariableBinderInvalid { reference: id, .. }
            | Error::TextReferenceInvalid { reference: id, .. }
            | Error::ScopeViaInvalid { reference: id } => reference(id),
        }
    }

    fn schema_dependent_checks(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
    ) -> BTreeSet<CheckKey> {
        let mut checks = BTreeSet::new();
        for (vertex, required) in &schema.required {
            for edge in required {
                checks.insert(CheckKey {
                    kind: CheckKind::RequiredEdge,
                    decl: SchemaRef::Required(vertex.clone(), edge.clone()),
                });
            }
        }
        for edge in schema.orderings.keys() {
            checks.insert(CheckKey {
                kind: CheckKind::PositionDistinct,
                decl: SchemaRef::Ordering(edge.clone()),
            });
        }
        for (vertex, constraints) in &schema.constraints {
            for (sort, bound) in constraints {
                let decl = SchemaRef::Constraint(vertex.clone(), sort.clone(), bound.clone());
                for kind in dependent_constraint_checks(sem.get(sort)) {
                    checks.insert(CheckKey {
                        kind: *kind,
                        decl: decl.clone(),
                    });
                }
            }
        }
        for (vertex, keys) in &schema.keys {
            for key in keys {
                let decl = SchemaRef::Key(vertex.clone(), key.id.clone());
                for kind in dependent_key_checks(key.role) {
                    checks.insert(CheckKey {
                        kind: *kind,
                        decl: decl.clone(),
                    });
                }
            }
        }
        for reference in schema.references.keys() {
            checks.insert(CheckKey {
                kind: CheckKind::ReferenceResolves,
                decl: SchemaRef::Reference(reference.clone()),
            });
        }
        for hyper_edge in schema.hyper_edges.keys() {
            checks.insert(CheckKey {
                kind: CheckKind::FanWellFormed,
                decl: SchemaRef::Vertex(hyper_edge.clone()),
            });
        }
        for target_set in schema.target_sets.keys() {
            checks.insert(CheckKey {
                kind: CheckKind::KeyUnique,
                decl: SchemaRef::TargetSet(target_set.clone()),
            });
        }
        checks
    }

    fn static_findings(schema: &Schema, sem: &BTreeMap<Name, Semantics>) -> Vec<IncompleteCheck> {
        let mut incomplete = Vec::new();
        if let Err(errors) = crate::schema::validate_declarations(schema) {
            for error in errors {
                let dependents = declaration_error_dependents(schema, &error);
                let reason = IncompleteReason::IllFormedDeclaration {
                    detail: format!("{error:?}"),
                };
                incomplete.push(undecided(
                    CheckKind::SchemaWellFormed,
                    SchemaRef::Schema,
                    Requirement::Required,
                    None,
                    reason.clone(),
                ));
                incomplete.extend(dependents.iter().map(|check| IncompleteCheck {
                    check: check.clone(),
                    class: class_for_check(schema, check),
                    requirement: requirement_for_check(schema, check),
                    subject: None,
                    reason: reason.clone(),
                }));
            }
        }
        static_structure_findings(schema, &mut incomplete);
        for (vertex, declarations) in &schema.constraints {
            for (sort, bound) in declarations {
                let reason = constraint_declaration_reason(schema, sem, vertex, sort, bound);
                if let Some(reason) = reason {
                    let decl = SchemaRef::Constraint(vertex.clone(), sort.clone(), bound.clone());
                    incomplete.push(undecided(
                        CheckKind::DeclarationInterpretable,
                        decl.clone(),
                        Requirement::Required,
                        None,
                        reason.clone(),
                    ));
                    incomplete.extend(
                        dependent_constraint_checks(sem.get(sort))
                            .iter()
                            .copied()
                            .map(|kind| {
                                undecided(
                                    kind,
                                    decl.clone(),
                                    Requirement::Required,
                                    None,
                                    reason.clone(),
                                )
                            }),
                    );
                }
            }
        }
        incomplete
    }

    fn static_declaration_failure(
        incomplete: &mut Vec<IncompleteCheck>,
        declaration: SchemaRef,
        dependent: CheckKind,
        detail: String,
    ) {
        let reason = IncompleteReason::IllFormedDeclaration { detail };
        incomplete.push(undecided(
            CheckKind::SchemaWellFormed,
            SchemaRef::Schema,
            Requirement::Required,
            None,
            reason.clone(),
        ));
        incomplete.push(undecided(
            CheckKind::DeclarationInterpretable,
            declaration.clone(),
            Requirement::Required,
            None,
            reason.clone(),
        ));
        incomplete.push(undecided(
            dependent,
            declaration,
            Requirement::Required,
            None,
            reason,
        ));
    }

    fn static_structure_findings(schema: &Schema, incomplete: &mut Vec<IncompleteCheck>) {
        for (vertex, required) in &schema.required {
            for edge in required {
                if schema.vertices.contains_key(vertex)
                    && edge.src == *vertex
                    && schema.edges.contains(edge)
                {
                    continue;
                }
                static_declaration_failure(
                    incomplete,
                    SchemaRef::Required(vertex.clone(), edge.clone()),
                    CheckKind::RequiredEdge,
                    format!("required edge {edge:?} is not a declared edge from {vertex:?}"),
                );
            }
        }
        for edge in schema.orderings.keys() {
            if schema.edges.contains(edge) {
                continue;
            }
            static_declaration_failure(
                incomplete,
                SchemaRef::Ordering(edge.clone()),
                CheckKind::PositionDistinct,
                format!("ordered edge {edge:?} is not declared"),
            );
        }
        for (name, hyper_edge) in &schema.hyper_edges {
            let missing_parent = !hyper_edge.signature.contains_key(&hyper_edge.parent_label);
            let unknown_vertices: Vec<_> = hyper_edge
                .signature
                .iter()
                .filter(|(_, vertex)| !schema.vertices.contains_key(*vertex))
                .map(|(label, vertex)| (label.clone(), vertex.clone()))
                .collect();
            if !missing_parent && unknown_vertices.is_empty() {
                continue;
            }
            static_declaration_failure(
                incomplete,
                SchemaRef::Vertex(name.clone()),
                CheckKind::FanWellFormed,
                format!(
                    "hyper-edge {name:?} has missing parent label {missing_parent} and unknown signature vertices {unknown_vertices:?}"
                ),
            );
        }
    }

    fn constraint_declaration_reason(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        vertex: &str,
        sort: &str,
        bound: &str,
    ) -> Option<IncompleteReason> {
        match sem.get(sort) {
            None | Some(Semantics::Other) => Some(IncompleteReason::UnsupportedDeclaration {
                sort: sort.to_owned(),
            }),
            Some(
                Semantics::MaxLength(_)
                | Semantics::MinLength(_)
                | Semantics::MaxGraphemes
                | Semantics::MinGraphemes,
            ) if bound.parse::<u64>().is_err() => Some(IncompleteReason::IllFormedDeclaration {
                detail: format!("{sort} bound {bound:?} is not an unsigned integer"),
            }),
            Some(Semantics::MinOutDegree | Semantics::MaxOutDegree)
                if bound.parse::<u32>().is_err() =>
            {
                Some(IncompleteReason::IllFormedDeclaration {
                    detail: format!("{sort} bound {bound:?} is not a u32"),
                })
            }
            Some(Semantics::Minimum | Semantics::Maximum)
                if DecimalValue::parse(bound).is_none() =>
            {
                Some(IncompleteReason::IllFormedDeclaration {
                    detail: format!("{sort} bound {bound:?} is not an exact number"),
                })
            }
            Some(Semantics::Acyclic)
                if !schema.edges.iter().any(|edge| {
                    edge.src == vertex && edge.tgt == vertex && edge.name.as_deref() == Some(bound)
                }) =>
            {
                Some(IncompleteReason::IllFormedDeclaration {
                    detail: format!("acyclic edge {bound:?} is not a self-edge of {vertex}"),
                })
            }
            Some(Semantics::Pattern) if anchored_literal(bound).is_none() => {
                Some(IncompleteReason::Unsupported {
                    detail: format!(
                        "pattern {bound:?} is outside the settled anchored-literal subset"
                    ),
                })
            }
            Some(Semantics::Format(_)) if !supported_format(bound) => {
                Some(IncompleteReason::UnsupportedDeclaration {
                    sort: sort.to_owned(),
                })
            }
            Some(Semantics::ValueParameter) if !value_parameter_is_well_formed(sort, bound) => {
                Some(IncompleteReason::IllFormedDeclaration {
                    detail: format!(
                        "value parameter {sort}={bound:?} is not in its declared scalar domain"
                    ),
                })
            }
            _ => None,
        }
    }

    fn supported_format(bound: &str) -> bool {
        matches!(
            bound,
            "date-time"
                | "datetime"
                | "date"
                | "time"
                | "uri"
                | "uuid"
                | "ipv4"
                | "ipv6"
                | "did"
                | "at-identifier"
                | "at-uri"
                | "cid"
                | "handle"
                | "nsid"
                | "tid"
                | "record-key"
                | "language"
        )
    }

    fn value_parameter_is_well_formed(sort: &str, bound: &str) -> bool {
        match sort {
            "precision" | "scale" | "max_length" => bound.parse::<u32>().is_ok(),
            "int_bits" => bound.parse::<u8>().is_ok(),
            "with_time_zone" | "unsigned" => matches!(bound, "true" | "false"),
            _ => false,
        }
    }

    fn write_view(writer: &mut crate::canonical::Writer, reference: &ViewRef) {
        // These are compatibility tags, independent of `ViewRef` declaration
        // order. Arc was added after the four established graph/table tags.
        match reference {
            ViewRef::Node(id) => {
                writer.tag(0);
                writer.u32(*id);
            }
            ViewRef::Edge(id) => {
                writer.tag(1);
                writer.u32(*id);
            }
            ViewRef::Row(id) => {
                writer.tag(2);
                writer.u32(*id);
            }
            ViewRef::Pair(edge, source, target) => {
                writer.tag(3);
                writer.edge(edge);
                writer.u32(*source);
                writer.u32(*target);
            }
            ViewRef::Arc { parent, child } => {
                writer.tag(4);
                writer.u32(*parent);
                writer.u32(*child);
            }
        }
    }

    #[cfg(test)]
    mod view_encoding_tests {
        use std::collections::{BTreeMap, BTreeSet};

        use super::{
            NativeRef, Node, SEdge, Schema, Tree, TreeEmbedding, ValidationInput, ViewRef,
            Violation, changed_refs, tree_identity, tree_violation, write_view,
        };

        fn bytes(reference: &ViewRef) -> Vec<u8> {
            let mut writer = crate::canonical::Writer::default();
            write_view(&mut writer, reference);
            writer.bytes
        }

        #[test]
        fn canonical_view_tags_preserve_old_tags_and_append_arc() {
            assert_eq!(bytes(&ViewRef::Node(7))[0], 0);
            assert_eq!(bytes(&ViewRef::Edge(7))[0], 1);
            assert_eq!(bytes(&ViewRef::Row(7))[0], 2);
            assert_eq!(
                bytes(&ViewRef::Pair(
                    Box::new(SEdge::new("a", "b", "r", None)),
                    7,
                    8,
                ))[0],
                3
            );
            assert_eq!(
                bytes(&ViewRef::Arc {
                    parent: 7,
                    child: 8,
                }),
                [
                    vec![4],
                    7_u32.to_le_bytes().to_vec(),
                    8_u32.to_le_bytes().to_vec()
                ]
                .concat()
            );
        }

        fn tree_input(tree: Tree) -> ValidationInput {
            ValidationInput::Tree {
                schema: Schema::default(),
                semantics: BTreeMap::new(),
                instance: tree,
            }
        }

        fn two_arc_tree() -> Tree {
            Tree {
                nodes: [
                    (0, Node::new(0, "v")),
                    (1, Node::new(1, "v")),
                    (2, Node::new(2, "v")),
                ]
                .into(),
                arcs: vec![
                    (0, 1, SEdge::new("v", "v", "child", Some("a"))),
                    (0, 2, SEdge::new("v", "v", "child", Some("b"))),
                ],
                fans: Vec::new(),
                root: 0,
            }
        }

        #[test]
        fn changed_tree_slots_name_old_and_new_native_arcs() {
            let prior = two_arc_tree();

            let mut relabelled = prior.clone();
            relabelled.arcs[0].2.name = Some("changed".to_owned());
            assert_eq!(
                changed_refs(&tree_input(prior.clone()), &tree_input(relabelled)),
                Some(vec![ViewRef::Arc {
                    parent: 0,
                    child: 1,
                }])
            );

            let mut reconnected = prior.clone();
            reconnected.arcs[0].0 = 2;
            assert_eq!(
                changed_refs(&tree_input(prior.clone()), &tree_input(reconnected)),
                Some(vec![
                    ViewRef::Arc {
                        parent: 0,
                        child: 1,
                    },
                    ViewRef::Arc {
                        parent: 2,
                        child: 1,
                    },
                ])
            );

            let mut reordered = prior.clone();
            reordered.arcs.swap(0, 1);
            assert_eq!(
                changed_refs(&tree_input(prior), &tree_input(reordered)),
                Some(vec![
                    ViewRef::Arc {
                        parent: 0,
                        child: 1,
                    },
                    ViewRef::Arc {
                        parent: 0,
                        child: 2,
                    },
                ])
            );
        }

        #[test]
        fn nested_tree_violation_refs_preserve_physical_arc_multiplicity() {
            let arcs = BTreeMap::from([(0, (0, 1)), (1, (0, 1))]);
            assert_eq!(
                tree_violation(
                    &Violation::OutDegree(
                        None,
                        None,
                        None,
                        2,
                        vec![ViewRef::Edge(0), ViewRef::Edge(1), ViewRef::Edge(2)],
                    ),
                    &arcs,
                ),
                Violation::OutDegree(
                    None,
                    None,
                    None,
                    2,
                    vec![
                        ViewRef::Arc {
                            parent: 0,
                            child: 1,
                        },
                        ViewRef::Arc {
                            parent: 0,
                            child: 1,
                        },
                        ViewRef::Edge(2),
                    ],
                )
            );
        }

        #[test]
        fn duplicate_endpoint_identity_uses_the_first_physical_arc() {
            let label = SEdge::new("v", "v", "child", None);
            let tree = Tree {
                nodes: [(0, Node::new(0, "v")), (1, Node::new(1, "v"))].into(),
                arcs: vec![(0, 1, label.clone()), (0, 1, label)],
                fans: Vec::new(),
                root: 0,
            };
            let identity = tree_identity(&tree, &TreeEmbedding::default());
            assert_eq!(
                identity.get(&ViewRef::Arc {
                    parent: 0,
                    child: 1,
                }),
                Some(&NativeRef::TreeArc(0))
            );
            assert_eq!(
                identity
                    .iter()
                    .filter(|(reference, _)| matches!(reference, ViewRef::Arc { .. }))
                    .map(|(reference, _)| reference.clone())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([ViewRef::Arc {
                    parent: 0,
                    child: 1,
                }])
            );
        }
    }

    fn write_views(writer: &mut crate::canonical::Writer, references: &[ViewRef]) {
        writer.len(references.len());
        for reference in references {
            write_view(writer, reference);
        }
    }

    fn write_value(writer: &mut crate::canonical::Writer, value: &Value) {
        match crate::canonical::value_bytes(value) {
            Some(bytes) => {
                writer.tag(1);
                writer.raw(&bytes);
            }
            None => writer.tag(0),
        }
    }

    fn write_optional_u32(writer: &mut crate::canonical::Writer, value: Option<u32>) {
        writer.opt_u32(value);
    }

    fn write_optional_edge(writer: &mut crate::canonical::Writer, edge: Option<&SEdge>) {
        match edge {
            None => writer.tag(0),
            Some(edge) => {
                writer.tag(1);
                writer.edge(edge);
            }
        }
    }

    fn write_fan_violation(writer: &mut crate::canonical::Writer, violation: &FanViolation) {
        match violation {
            FanViolation::MissingParent => writer.tag(0),
            FanViolation::MultipleParents { edges } => {
                writer.tag(1);
                writer.len(edges.len());
                for edge in edges {
                    writer.u32(*edge);
                }
            }
            FanViolation::DuplicateLabel { label, edges } => {
                writer.tag(2);
                writer.str(label);
                writer.len(edges.len());
                for edge in edges {
                    writer.u32(*edge);
                }
            }
            FanViolation::UnknownLabel { label } => {
                writer.tag(3);
                writer.str(label);
            }
            FanViolation::ReservedKindOutsideFan { edge } => {
                writer.tag(4);
                writer.u32(*edge);
            }
            FanViolation::ForeignIncidentEdge { edge } => {
                writer.tag(5);
                writer.u32(*edge);
            }
            FanViolation::UnknownHyperEdge { hyper_edge } => {
                writer.tag(6);
                writer.str(hyper_edge);
            }
        }
    }

    fn violation_bytes(violation: &Violation) -> Vec<u8> {
        let mut writer = crate::canonical::Writer::default();
        match violation {
            Violation::UnknownAnchor(anchor) => {
                writer.tag(0);
                writer.str(anchor);
            }
            Violation::UndeclaredEdge(edge) => {
                writer.tag(1);
                writer.edge(edge);
            }
            Violation::IncoherentEndpoint(source, expected, found) => {
                writer.tag(2);
                writer.tag(u8::from(*source));
                writer.str(expected);
                writer.str(found);
            }
            Violation::DanglingEndpoint(source, node) => {
                writer.tag(3);
                writer.tag(u8::from(*source));
                writer.u32(*node);
            }
            Violation::MalformedFan(reason) => {
                writer.tag(4);
                write_fan_violation(&mut writer, reason);
            }
            Violation::MissingFanParent => writer.tag(5),
            Violation::MissingFanChild(label) => {
                writer.tag(6);
                writer.str(label);
            }
            Violation::MissingRequiredEdge(edge) => {
                writer.tag(7);
                writer.edge(edge);
            }
            Violation::OutDegree(edge, min, max, found, edges) => {
                writer.tag(8);
                write_optional_edge(&mut writer, edge.as_ref());
                write_optional_u32(&mut writer, *min);
                write_optional_u32(&mut writer, *max);
                writer.u32(*found);
                write_views(&mut writer, edges);
            }
            Violation::DuplicatePosition(position, edges) => {
                writer.tag(9);
                writer.u32(*position);
                writer.len(edges.len());
                for edge in edges {
                    writer.u32(*edge);
                }
            }
            Violation::ValueConstraint(sort, bound, value) => {
                writer.tag(10);
                writer.str(sort);
                writer.str(bound);
                write_value(&mut writer, value);
            }
            Violation::MissingValue(field) => {
                writer.tag(11);
                writer.opt_name(field.as_deref());
            }
            Violation::DuplicateKey(parts) => {
                writer.tag(12);
                writer.len(parts.len());
                for (column, value) in parts {
                    writer.str(column);
                    write_value(&mut writer, value);
                }
            }
            _ => write_violation_tail(&mut writer, violation),
        }
        writer.bytes
    }

    fn write_violation_tail(writer: &mut crate::canonical::Writer, violation: &Violation) {
        match violation {
            Violation::KeyOrder(key, elements) => {
                writer.tag(13);
                writer.str(key);
                write_views(writer, elements);
            }
            Violation::OutsideKeyScope(key, scope) => {
                writer.tag(14);
                writer.str(key);
                writer.str(scope);
            }
            Violation::DanglingReference(reference, values) => {
                writer.tag(15);
                writer.str(reference);
                writer.len(values.len());
                for value in values {
                    write_value(writer, value);
                }
            }
            Violation::AmbiguousReference(reference, values, elements) => {
                writer.tag(16);
                writer.str(reference);
                writer.len(values.len());
                for value in values {
                    write_value(writer, value);
                }
                write_views(writer, elements);
            }
            Violation::UnselectedReference(reference, value) => {
                writer.tag(17);
                writer.str(reference);
                write_value(writer, value);
            }
            Violation::Cycle(elements) => {
                writer.tag(18);
                write_views(writer, elements);
            }
            Violation::UndeclaredAttribute(field) => {
                writer.tag(19);
                writer.str(field);
            }
            Violation::MistypedAttribute(field, expected, found) => {
                writer.tag(20);
                writer.str(field);
                writer.str(expected);
                writer.str(found);
            }
            Violation::ExternalRejected(name) => {
                writer.tag(21);
                writer.str(name);
            }
            Violation::MissingRoot => writer.tag(22),
            Violation::Unreachable => writer.tag(23),
            Violation::ParentMismatch(previous, found) => {
                writer.tag(24);
                write_optional_u32(writer, *previous);
                write_optional_u32(writer, *found);
            }
            Violation::ReservedVocabulary => writer.tag(25),
            _ => unreachable!("head violation routed to tail encoder"),
        }
    }

    fn incomplete_reason_bytes(reason: &IncompleteReason) -> Vec<u8> {
        let mut writer = crate::canonical::Writer::default();
        match reason {
            IncompleteReason::Budget { limit, spent, from } => {
                writer.tag(0);
                writer.u64(*limit);
                writer.u64(*spent);
                match from {
                    None => writer.tag(0),
                    Some(reference) => {
                        writer.tag(1);
                        write_view(&mut writer, reference);
                    }
                }
            }
            IncompleteReason::UnsupportedDeclaration { sort } => {
                writer.tag(1);
                writer.str(sort);
            }
            IncompleteReason::IllFormedDeclaration { detail } => {
                writer.tag(2);
                writer.str(detail);
            }
            IncompleteReason::Unsupported { detail } => {
                writer.tag(3);
                writer.str(detail);
            }
            IncompleteReason::NoValidatedBaseline => writer.tag(4),
            IncompleteReason::MissingExternalWitness { name } => {
                writer.tag(5);
                writer.str(name);
            }
            IncompleteReason::OutsideSegment { class } => {
                writer.tag(6);
                writer.tag(match class {
                    CheckClass::Static => 0,
                    CheckClass::GraphLocal => 1,
                    CheckClass::MaintainedGlobal => 2,
                    CheckClass::BoundedExternal => 3,
                });
            }
            IncompleteReason::NativeShapeRefusal { shape, detail } => {
                writer.tag(7);
                writer.tag(match shape {
                    InstanceShape::Graph => 0,
                    InstanceShape::Table => 1,
                    InstanceShape::Tree => 2,
                });
                writer.str(detail);
            }
        }
        writer.bytes
    }

    fn diagnostic_order(left: &Diagnostic, right: &Diagnostic) -> std::cmp::Ordering {
        let tail = |diagnostic: &Diagnostic| {
            let mut bytes = vec![
                match diagnostic.class {
                    CheckClass::Static => 0,
                    CheckClass::GraphLocal => 1,
                    CheckClass::MaintainedGlobal => 2,
                    CheckClass::BoundedExternal => 3,
                },
                match diagnostic.requirement {
                    Requirement::Required => 0,
                    Requirement::Advisory => 1,
                },
            ];
            bytes.extend(violation_bytes(&diagnostic.violation));
            bytes
        };
        (&left.check, &left.subject, &left.elements, tail(left)).cmp(&(
            &right.check,
            &right.subject,
            &right.elements,
            tail(right),
        ))
    }

    fn incomplete_order(left: &IncompleteCheck, right: &IncompleteCheck) -> std::cmp::Ordering {
        let tail = |entry: &IncompleteCheck| {
            let mut bytes = vec![
                match entry.class {
                    CheckClass::Static => 0,
                    CheckClass::GraphLocal => 1,
                    CheckClass::MaintainedGlobal => 2,
                    CheckClass::BoundedExternal => 3,
                },
                match entry.requirement {
                    Requirement::Required => 0,
                    Requirement::Advisory => 1,
                },
            ];
            bytes.extend(incomplete_reason_bytes(&entry.reason));
            bytes
        };
        (&left.check, &left.subject, tail(left)).cmp(&(&right.check, &right.subject, tail(right)))
    }

    fn finish_report(
        shape: InstanceShape,
        mut diagnostics: Vec<Diagnostic>,
        mut incomplete: Vec<IncompleteCheck>,
    ) -> Report {
        diagnostics.sort_by(diagnostic_order);
        incomplete.sort_by(incomplete_order);
        let report_status = status(&diagnostics, &incomplete);
        Report {
            shape,
            diagnostics,
            incomplete,
            identity: BTreeMap::new(),
            status: report_status,
        }
    }

    fn validate_graph_as(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        graph: &Graph,
        shape: InstanceShape,
        context: &ValidationContext,
    ) -> Report {
        let mut diagnostics = Vec::new();
        let mut incomplete = static_findings(schema, sem);
        structural(schema, graph, shape, &mut diagnostics, &mut incomplete);
        constraints(
            schema,
            sem,
            graph,
            shape,
            &mut diagnostics,
            &mut incomplete,
            context,
        );
        if context.complete_segment {
            keyed(schema, graph, &mut diagnostics, &mut incomplete);
            references(schema, graph, &mut diagnostics, &mut incomplete);
            if shape == InstanceShape::Graph {
                acyclic(schema, sem, graph, &mut diagnostics);
            }
        } else {
            incomplete.extend(schema_dependent_checks(schema, sem).into_iter().filter_map(
                |check| {
                    (class_for_check(schema, &check) == CheckClass::MaintainedGlobal).then(|| {
                        IncompleteCheck {
                            class: class_for_check(schema, &check),
                            requirement: requirement_for_check(schema, &check),
                            check,
                            subject: None,
                            reason: IncompleteReason::OutsideSegment {
                                class: CheckClass::MaintainedGlobal,
                            },
                        }
                    })
                },
            ));
        }
        let mut report = finish_report(shape, diagnostics, incomplete);
        report.identity.extend(
            graph
                .nodes
                .keys()
                .map(|id| (ViewRef::Node(*id), NativeRef::GraphNode(*id))),
        );
        report.identity.extend(
            graph
                .edges
                .keys()
                .map(|id| (ViewRef::Edge(*id), NativeRef::GraphEdge(*id))),
        );
        report
    }

    /// Definition: Validate a graph without a finite read budget or external checks.
    ///
    /// Ill-formed and unsupported declarations are preserved as typed static
    /// incomplete entries. Thus every call returns a report.
    ///
    /// Book anchors: `L-validation-1`, `L-validation-2`, `L-validation-3`, `L-validation-4`, `L-validation-5`, `L-validation-6`, `L-validation-7`, `L-validation-8`, `L-validation-9`, `L-validation-10`, `L-validation-11`.
    #[must_use]
    pub fn validate_graph(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        graph: &Graph,
    ) -> Report {
        validate_graph_with_context(schema, sem, graph, &ValidationContext::default())
    }

    /// Validate a graph with trusted external verdicts and segment scope.
    #[must_use]
    pub fn validate_graph_with_context(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        graph: &Graph,
        context: &ValidationContext,
    ) -> Report {
        validate_graph_as(schema, sem, graph, InstanceShape::Graph, context)
    }

    /// Backward-compatible name for [`validate_graph`].
    ///
    /// Book anchors: `L-validation-1`, `L-validation-2`, `L-validation-3`, `L-validation-4`, `L-validation-5`, `L-validation-6`, `L-validation-7`, `L-validation-8`, `L-validation-9`, `L-validation-10`, `L-validation-11`.
    #[must_use]
    pub fn validate(schema: &Schema, sem: &BTreeMap<Name, Semantics>, graph: &Graph) -> Report {
        validate_graph(schema, sem, graph)
    }

    /// A native shape cannot be presented to the shared predicates.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ShapeRefusal {
        /// The table contains an unrepresentable pair or exhausts edge ids.
        Table(TableEncodeError),
        /// The tree contains an unrepresentable arc or fan carrier.
        Tree(TreeEncodeError),
    }

    fn table_ref(reference: &ViewRef, edge_pairs: &BTreeMap<u32, (SEdge, u32, u32)>) -> ViewRef {
        match reference {
            ViewRef::Node(row) => ViewRef::Row(*row),
            ViewRef::Edge(edge) => edge_pairs.get(edge).map_or_else(
                || ViewRef::Edge(*edge),
                |(label, src, tgt)| ViewRef::Pair(Box::new(label.clone()), *src, *tgt),
            ),
            other => other.clone(),
        }
    }

    fn table_violation(
        violation: &Violation,
        edge_pairs: &BTreeMap<u32, (SEdge, u32, u32)>,
    ) -> Violation {
        let refs = |items: &[ViewRef]| {
            items
                .iter()
                .map(|item| table_ref(item, edge_pairs))
                .collect()
        };
        match violation {
            Violation::OutDegree(edge, min, max, found, items) => {
                Violation::OutDegree(edge.clone(), *min, *max, *found, refs(items))
            }
            Violation::KeyOrder(key, items) => Violation::KeyOrder(key.clone(), refs(items)),
            Violation::AmbiguousReference(reference, key, candidates) => {
                Violation::AmbiguousReference(reference.clone(), key.clone(), refs(candidates))
            }
            Violation::Cycle(items) => Violation::Cycle(refs(items)),
            other => other.clone(),
        }
    }

    fn table_report(mut report: Report, embedding: &TableEmbedding) -> Report {
        let edge_pairs: BTreeMap<_, _> = embedding
            .iter()
            .map(|(pair, edge)| (*edge, pair.clone()))
            .collect();
        for diagnostic in &mut report.diagnostics {
            diagnostic.subject = diagnostic
                .subject
                .as_ref()
                .map(|subject| table_ref(subject, &edge_pairs));
            diagnostic.elements = diagnostic
                .elements
                .iter()
                .map(|element| table_ref(element, &edge_pairs))
                .collect();
            diagnostic.violation = table_violation(&diagnostic.violation, &edge_pairs);
        }
        for incomplete in &mut report.incomplete {
            incomplete.subject = incomplete
                .subject
                .as_ref()
                .map(|subject| table_ref(subject, &edge_pairs));
        }
        let identity = report
            .identity
            .keys()
            .filter_map(|reference| match reference {
                ViewRef::Node(row) => Some((ViewRef::Row(*row), NativeRef::TableRow(*row))),
                ViewRef::Edge(edge) => edge_pairs.get(edge).map(|(label, source, target)| {
                    (
                        ViewRef::Pair(Box::new(label.clone()), *source, *target),
                        NativeRef::TablePair(Box::new(label.clone()), *source, *target),
                    )
                }),
                ViewRef::Row(_) | ViewRef::Pair(_, _, _) | ViewRef::Arc { .. } => None,
            })
            .collect();
        let mut native = finish_report(report.shape, report.diagnostics, report.incomplete);
        native.identity = identity;
        native
    }

    fn tree_ref(reference: &ViewRef, arcs: &BTreeMap<u32, (u32, u32)>) -> ViewRef {
        match reference {
            ViewRef::Edge(edge) => arcs.get(edge).map_or_else(
                || ViewRef::Edge(*edge),
                |(parent, child)| ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                },
            ),
            other => other.clone(),
        }
    }

    fn tree_violation(violation: &Violation, arcs: &BTreeMap<u32, (u32, u32)>) -> Violation {
        let refs = |items: &[ViewRef]| items.iter().map(|item| tree_ref(item, arcs)).collect();
        match violation {
            Violation::OutDegree(edge, min, max, found, items) => {
                Violation::OutDegree(edge.clone(), *min, *max, *found, refs(items))
            }
            Violation::KeyOrder(key, items) => Violation::KeyOrder(key.clone(), refs(items)),
            Violation::AmbiguousReference(reference, key, candidates) => {
                Violation::AmbiguousReference(reference.clone(), key.clone(), refs(candidates))
            }
            Violation::Cycle(items) => Violation::Cycle(refs(items)),
            other => other.clone(),
        }
    }

    fn tree_report(mut report: Report, tree: &Tree, embedding: &TreeEmbedding) -> Report {
        let arcs: BTreeMap<_, _> = tree
            .arcs
            .iter()
            .enumerate()
            .map(|(index, (parent, child, _))| {
                (u32::try_from(index).unwrap_or(u32::MAX), (*parent, *child))
            })
            .collect();
        for diagnostic in &mut report.diagnostics {
            diagnostic.subject = diagnostic
                .subject
                .as_ref()
                .map(|subject| tree_ref(subject, &arcs));
            diagnostic.elements = diagnostic
                .elements
                .iter()
                .map(|element| tree_ref(element, &arcs))
                .collect();
            diagnostic.elements.sort();
            diagnostic.elements.dedup();
            diagnostic.violation = tree_violation(&diagnostic.violation, &arcs);
        }
        for incomplete in &mut report.incomplete {
            incomplete.subject = incomplete
                .subject
                .as_ref()
                .map(|subject| tree_ref(subject, &arcs));
        }
        let mut native = finish_report(report.shape, report.diagnostics, report.incomplete);
        native.identity = tree_identity(tree, embedding);
        native
    }

    /// Validate a table in its native row/pair identity.
    ///
    /// # Errors
    ///
    /// Returns the exact table-presentation refusal before validation.
    pub fn validate_table(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        table: &Table,
    ) -> Result<Report, ShapeRefusal> {
        validate_table_with_context(schema, sem, table, &ValidationContext::default())
    }

    /// Validate a table with trusted external verdicts and segment scope.
    ///
    /// # Errors
    ///
    /// Returns the exact table-presentation refusal before validation.
    pub fn validate_table_with_context(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        table: &Table,
        context: &ValidationContext,
    ) -> Result<Report, ShapeRefusal> {
        let (graph, embedding) =
            representation::table_to_graph(table).map_err(ShapeRefusal::Table)?;
        Ok(table_report(
            validate_graph_as(schema, sem, &graph, InstanceShape::Table, context),
            &embedding,
        ))
    }

    fn tree_checks(
        tree: &Tree,
        complete_segment: bool,
        diagnostics: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        if !tree.nodes.contains_key(&tree.root) {
            diagnostics.push(diag(
                CheckKind::TreeRoot,
                SchemaRef::Schema,
                None,
                Vec::new(),
                Violation::MissingRoot,
            ));
        }
        for (id, node) in &tree.nodes {
            if node.shape == NodeShape::HyperEdge {
                diagnostics.push(diag(
                    CheckKind::ReservedVocabulary,
                    SchemaRef::Vertex(node.anchor.clone()),
                    Some(ViewRef::Node(*id)),
                    vec![ViewRef::Node(*id)],
                    Violation::ReservedVocabulary,
                ));
            }
        }
        let mut parents = BTreeMap::new();
        let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (parent, child, label) in &tree.arcs {
            children.entry(*parent).or_default().push(*child);
            if is_fan_kind(&label.kind) {
                let arc = ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                };
                diagnostics.push(diag(
                    CheckKind::ReservedVocabulary,
                    SchemaRef::Edge(label.clone()),
                    Some(arc.clone()),
                    vec![arc],
                    Violation::ReservedVocabulary,
                ));
            }
            if let Some(previous) = parents.insert(*child, *parent) {
                diagnostics.push(diag(
                    CheckKind::TreeParentMap,
                    SchemaRef::Vertex(
                        tree.nodes
                            .get(child)
                            .map_or_else(String::new, |node| node.anchor.clone()),
                    ),
                    Some(ViewRef::Node(*child)),
                    vec![ViewRef::Node(*child)],
                    Violation::ParentMismatch(Some(previous), Some(*parent)),
                ));
            }
        }
        tree_reachability(tree, complete_segment, &children, diagnostics, incomplete);
        for fan in &tree.fans {
            if !tree.nodes.contains_key(&fan.parent) {
                diagnostics.push(diag(
                    CheckKind::FanWellFormed,
                    SchemaRef::Vertex(fan.hyper_edge.clone()),
                    Some(ViewRef::Node(fan.parent)),
                    vec![ViewRef::Node(fan.parent)],
                    Violation::MissingFanParent,
                ));
            }
            for (label, child) in &fan.children {
                if !tree.nodes.contains_key(child) {
                    diagnostics.push(diag(
                        CheckKind::FanWellFormed,
                        SchemaRef::Vertex(fan.hyper_edge.clone()),
                        Some(ViewRef::Node(fan.parent)),
                        vec![ViewRef::Node(fan.parent), ViewRef::Node(*child)],
                        Violation::MissingFanChild(label.clone()),
                    ));
                }
            }
        }
    }

    fn tree_reachability(
        tree: &Tree,
        complete_segment: bool,
        children: &BTreeMap<u32, Vec<u32>>,
        diagnostics: &mut Vec<Diagnostic>,
        incomplete: &mut Vec<IncompleteCheck>,
    ) {
        if complete_segment && tree.nodes.contains_key(&tree.root) {
            let mut reached = BTreeSet::from([tree.root]);
            let mut stack = vec![tree.root];
            while let Some(parent) = stack.pop() {
                for child in children.get(&parent).into_iter().flatten() {
                    if reached.insert(*child) {
                        stack.push(*child);
                    }
                }
            }
            for (id, node) in &tree.nodes {
                if !reached.contains(id) {
                    diagnostics.push(diag(
                        CheckKind::TreeReachable,
                        SchemaRef::Vertex(node.anchor.clone()),
                        Some(ViewRef::Node(*id)),
                        vec![ViewRef::Node(*id)],
                        Violation::Unreachable,
                    ));
                }
            }
        } else if !complete_segment {
            incomplete.extend(tree.nodes.iter().map(|(id, node)| {
                undecided(
                    CheckKind::TreeReachable,
                    SchemaRef::Vertex(node.anchor.clone()),
                    Requirement::Required,
                    Some(ViewRef::Node(*id)),
                    IncompleteReason::OutsideSegment {
                        class: CheckClass::MaintainedGlobal,
                    },
                )
            }));
        }
    }

    fn take_tree_id(counter: &mut u32, error: TreeEncodeError) -> Result<u32, TreeEncodeError> {
        let id = *counter;
        *counter = counter.checked_add(1).ok_or(error)?;
        Ok(id)
    }

    fn tree_validation_graph(
        tree: &Tree,
        schema: &Schema,
    ) -> Result<(Graph, TreeEmbedding), TreeEncodeError> {
        let mut graph = Graph {
            nodes: tree.nodes.clone(),
            edges: BTreeMap::new(),
        };
        // A native tree node shaped `HyperEdge` is reserved vocabulary, not a
        // graph-presentation fan carrier. Keep it in every shared check by
        // erasing only the presentation marker on the local graph copy.
        for node in graph.nodes.values_mut() {
            if node.shape == NodeShape::HyperEdge {
                node.shape = NodeShape::Plain;
            }
        }
        let mut embedding = TreeEmbedding::default();
        let mut next_edge = 0;
        for (parent, child, label) in &tree.arcs {
            if !tree.nodes.contains_key(parent) || !tree.nodes.contains_key(child) {
                return Err(TreeEncodeError::DanglingArc);
            }
            let edge = take_tree_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
            graph.edges.insert(
                edge,
                GEdge {
                    src: *parent,
                    tgt: *child,
                    label: label.clone(),
                    position: None,
                },
            );
            embedding.arcs.entry(*child).or_insert(edge);
        }
        let mut next_node = crate::restrict::floor_of(tree.nodes.keys().copied());
        for fan in &tree.fans {
            let node = take_tree_id(&mut next_node, TreeEncodeError::NodeIdsExhausted)?;
            let parent_anchor = tree
                .nodes
                .get(&fan.parent)
                .map_or_else(String::new, |item| item.anchor.clone());
            let parent_label = SEdge {
                src: fan.hyper_edge.clone(),
                tgt: parent_anchor,
                kind: HYPER_PARENT.to_owned(),
                name: schema
                    .hyper_edges
                    .get(&fan.hyper_edge)
                    .map(|declaration| declaration.parent_label.clone()),
            };
            let parent_edge = take_tree_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
            graph.edges.insert(
                parent_edge,
                GEdge {
                    src: node,
                    tgt: fan.parent,
                    label: parent_label,
                    position: None,
                },
            );
            let mut children = BTreeMap::new();
            for (label, child) in &fan.children {
                let edge = take_tree_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
                graph.edges.insert(
                    edge,
                    GEdge {
                        src: node,
                        tgt: *child,
                        label: SEdge {
                            src: fan.hyper_edge.clone(),
                            tgt: tree.nodes.get(child).map_or_else(
                                || {
                                    schema
                                        .hyper_edges
                                        .get(&fan.hyper_edge)
                                        .and_then(|hyper| hyper.signature.get(label))
                                        .cloned()
                                        .unwrap_or_default()
                                },
                                |item| item.anchor.clone(),
                            ),
                            kind: HYPER_LABEL.to_owned(),
                            name: Some(label.clone()),
                        },
                        position: None,
                    },
                );
                children.insert(label.clone(), edge);
            }
            let mut fan_node = Node::new(node, &fan.hyper_edge);
            fan_node.shape = NodeShape::HyperEdge;
            graph.nodes.insert(node, fan_node);
            embedding.fans.push((node, parent_edge, children));
        }
        embedding.next_node = next_node;
        embedding.next_edge = next_edge;
        Ok((graph, embedding))
    }

    fn tree_identity(tree: &Tree, embedding: &TreeEmbedding) -> BTreeMap<ViewRef, NativeRef> {
        let mut identity: BTreeMap<_, _> = tree
            .nodes
            .keys()
            .map(|node| (ViewRef::Node(*node), NativeRef::TreeNode(*node)))
            .collect();
        for (index, (parent, child, _)) in tree.arcs.iter().enumerate() {
            identity
                .entry(ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                })
                .or_insert(NativeRef::TreeArc(index));
        }
        for (index, (node, parent, children)) in embedding.fans.iter().enumerate() {
            identity.insert(ViewRef::Node(*node), NativeRef::TreeFan(index));
            identity.insert(ViewRef::Edge(*parent), NativeRef::TreeFanParent(index));
            identity.extend(children.iter().map(|(label, edge)| {
                (
                    ViewRef::Edge(*edge),
                    NativeRef::TreeFanChild(index, label.clone()),
                )
            }));
        }
        identity
    }

    /// Validate a tree, retaining tree-only root, parent, reachability and
    /// reserved-vocabulary checks.
    ///
    /// # Errors
    ///
    /// Returns an exact tree-presentation refusal when the shared predicates
    /// have no graph presentation. Reserved vocabulary is a validation
    /// diagnostic, not such a refusal.
    pub fn validate_tree(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        tree: &Tree,
    ) -> Result<Report, ShapeRefusal> {
        validate_tree_with_context(schema, sem, tree, &ValidationContext::default())
    }

    /// Validate a tree with trusted external verdicts and segment scope.
    ///
    /// # Errors
    ///
    /// Returns an exact tree-presentation refusal when an arc cannot be
    /// presented or identifier allocation is exhausted. Missing fan members
    /// are retained as typed diagnostics.
    pub fn validate_tree_with_context(
        schema: &Schema,
        sem: &BTreeMap<Name, Semantics>,
        tree: &Tree,
        context: &ValidationContext,
    ) -> Result<Report, ShapeRefusal> {
        let mut tree_diagnostics = Vec::new();
        let mut tree_incomplete = Vec::new();
        tree_checks(
            tree,
            context.complete_segment,
            &mut tree_diagnostics,
            &mut tree_incomplete,
        );
        match tree_validation_graph(tree, schema) {
            Ok((graph, embedding)) => {
                let mut report =
                    validate_graph_as(schema, sem, &graph, InstanceShape::Tree, context);
                report.diagnostics.extend(tree_diagnostics);
                report.incomplete.extend(tree_incomplete);
                Ok(tree_report(report, tree, &embedding))
            }
            Err(error) => Err(ShapeRefusal::Tree(error)),
        }
    }

    /// The identity of one check evaluation.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct CheckInstance {
        /// The compiled check.
        pub check: CheckKey,
        /// Its focus, or `None` for a whole-instance check.
        pub subject: Option<ViewRef>,
    }

    /// One independently supplied predicate run.
    ///
    /// `reads` is the exact ascending, duplicate-free dependency trace. The
    /// two outcome vectors retain multiplicity; a run can contain both.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CheckRun {
        /// Run identity.
        pub instance: CheckInstance,
        /// Locality of this compiled instance.  Most kinds have one fixed
        /// class, while a primary-key required-value check becomes global
        /// when it follows a nonempty path or computes a sibling position.
        pub class: CheckClass,
        /// Whether this run decides admission.
        pub requirement: Requirement,
        /// Exact read trace.
        pub reads: Vec<ViewRef>,
        /// Every diagnostic from this instance.
        pub diagnostics: Vec<Diagnostic>,
        /// Every undecided outcome from this instance.
        pub incomplete: Vec<IncompleteCheck>,
    }

    /// A deterministic read budget. `None` is unlimited.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ValidationBudget {
        /// Maximum charged reads.
        pub max_reads: Option<u64>,
    }

    /// Coverage of one compiled check in an explicit run bundle.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct CheckCoverage {
        /// Check locality/class.
        pub class: CheckClass,
        /// Whether the check decides admission.
        pub requirement: Requirement,
        /// Scheduled instances.
        pub instances: u64,
        /// Evaluated instances with no outcome.
        pub passed: u64,
        /// Evaluated instances with at least one diagnostic.
        pub failed: u64,
        /// Evaluated-undecided and budget-skipped instances.
        pub incomplete: u64,
    }

    /// Why an explicit run bundle is not a valid validation schedule.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RunBundleError {
        /// Two runs have one identity.
        DuplicateInstance(Box<CheckInstance>),
        /// A read trace is not strictly increasing.
        NonCanonicalReads(Box<CheckInstance>),
        /// An outcome names another check instance.
        OutcomeInstanceMismatch(Box<CheckInstance>),
        /// Runs are not in strict canonical instance order.
        NonCanonicalSchedule,
        /// Outcomes inside a run are not in canonical order.
        NonCanonicalOutcomes(Box<CheckInstance>),
        /// A diagnostic's element list is not strict canonical order.
        NonCanonicalElements(Box<CheckInstance>),
        /// Outcomes disagree with the run's requirement.
        OutcomeRequirementMismatch(Box<CheckInstance>),
        /// Outcomes disagree with the compiled instance's locality.
        OutcomeClassMismatch(Box<CheckInstance>),
        /// Instances of one check disagree about its requirement.
        InconsistentRequirement(Box<CheckKey>),
        /// Instances of one check disagree about its locality.
        InconsistentClass(Box<CheckKey>),
        /// A supplied schedule differs from the trusted plan-derived list.
        ScheduleMismatch,
        /// Evaluation produced an outcome absent from the independently
        /// compiled plan, or with another admission requirement.
        PlanMismatch(Box<CheckInstance>),
        /// The read count does not fit the public `u64` counter.
        ReadCountOverflow(Box<CheckInstance>),
    }

    /// Result of the explicit V.7 budget schedule.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct BudgetEvaluation {
        /// Report obtained from evaluated runs and the skipped suffix.
        pub report: Report,
        /// Complete ordered schedule.
        pub scheduled: Vec<CheckInstance>,
        /// Exact budget-skipped suffix.
        pub skipped: Vec<CheckInstance>,
        /// Reads charged before the cut.
        pub spent: u64,
        /// Coverage by check.
        pub coverage: BTreeMap<CheckKey, CheckCoverage>,
    }

    /// Replayable evidence for the exact finite-budget cut of a retained
    /// baseline.
    ///
    /// The vectors and counter are commitments only. Witness construction and
    /// verification recompile the baseline schedule from `prior_input` and
    /// accept this evidence only when evaluating that trusted schedule under
    /// `budget` reproduces all three fields exactly.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct BaselineBudgetEvidence {
        /// Budget used to evaluate the retained baseline.
        pub budget: ValidationBudget,
        /// Complete canonical baseline schedule.
        pub scheduled: Vec<CheckInstance>,
        /// Exact skipped suffix of `scheduled`.
        pub skipped: Vec<CheckInstance>,
        /// Reads charged before the cut.
        pub spent: u64,
    }

    fn run_is_canonical(run: &CheckRun) -> Result<(), RunBundleError> {
        if run.reads.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(RunBundleError::NonCanonicalReads(Box::new(
                run.instance.clone(),
            )));
        }
        let agrees = run.diagnostics.iter().all(|diagnostic| {
            diagnostic.check == run.instance.check && diagnostic.subject == run.instance.subject
        }) && run.incomplete.iter().all(|entry| {
            entry.check == run.instance.check && entry.subject == run.instance.subject
        });
        if !agrees {
            return Err(RunBundleError::OutcomeInstanceMismatch(Box::new(
                run.instance.clone(),
            )));
        }
        if run.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .elements
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        }) {
            return Err(RunBundleError::NonCanonicalElements(Box::new(
                run.instance.clone(),
            )));
        }
        let requirement_agrees = run
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.requirement == run.requirement)
            && run
                .incomplete
                .iter()
                .all(|entry| entry.requirement == run.requirement);
        if !requirement_agrees {
            return Err(RunBundleError::OutcomeRequirementMismatch(Box::new(
                run.instance.clone(),
            )));
        }
        let class_agrees = run
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.class == run.class)
            && run.incomplete.iter().all(|entry| entry.class == run.class);
        if !class_agrees {
            return Err(RunBundleError::OutcomeClassMismatch(Box::new(
                run.instance.clone(),
            )));
        }
        if run
            .diagnostics
            .windows(2)
            .any(|pair| diagnostic_order(&pair[0], &pair[1]).is_gt())
            || run
                .incomplete
                .windows(2)
                .any(|pair| incomplete_order(&pair[0], &pair[1]).is_gt())
        {
            return Err(RunBundleError::NonCanonicalOutcomes(Box::new(
                run.instance.clone(),
            )));
        }
        Ok(())
    }

    fn validate_run_bundle(runs: &[CheckRun]) -> Result<(), RunBundleError> {
        for pair in runs.windows(2) {
            if pair[0].instance == pair[1].instance {
                return Err(RunBundleError::DuplicateInstance(Box::new(
                    pair[0].instance.clone(),
                )));
            }
            if pair[0].instance > pair[1].instance {
                return Err(RunBundleError::NonCanonicalSchedule);
            }
        }
        let mut requirements = BTreeMap::new();
        let mut classes = BTreeMap::new();
        for run in runs {
            run_is_canonical(run)?;
            if requirements
                .insert(run.instance.check.clone(), run.requirement)
                .is_some_and(|previous| previous != run.requirement)
            {
                return Err(RunBundleError::InconsistentRequirement(Box::new(
                    run.instance.check.clone(),
                )));
            }
            if classes
                .insert(run.instance.check.clone(), run.class)
                .is_some_and(|previous| previous != run.class)
            {
                return Err(RunBundleError::InconsistentClass(Box::new(
                    run.instance.check.clone(),
                )));
            }
        }
        Ok(())
    }

    const fn coverage_for(run: &CheckRun) -> CheckCoverage {
        CheckCoverage {
            class: run.class,
            requirement: run.requirement,
            instances: 0,
            passed: 0,
            failed: 0,
            incomplete: 0,
        }
    }

    /// Evaluate a caller-supplied, canonically ordered check bundle under a
    /// deterministic read budget.
    ///
    /// This low-level operation validates order and accounting but makes no
    /// completeness claim. [`trusted_schedule`] derives the complete bounded
    /// reference schedule from an input before witness replay.
    /// runs are sorted by `CheckInstance`, each evaluated run charges the
    /// cardinality of its trace, and the first run that would exceed the
    /// limit starts the skipped suffix. The suffix produces one `Budget`
    /// incomplete entry per exact skipped check instance. Every entry for one
    /// check key names that key's first skipped subject in `from`.
    ///
    /// # Errors
    ///
    /// Refuses duplicate instances, noncanonical read traces, outcome/run
    /// identity disagreement, and a read count outside `u64`.
    pub fn evaluate_runs(
        shape: InstanceShape,
        runs: &[CheckRun],
        static_incomplete: &[IncompleteCheck],
        budget: ValidationBudget,
    ) -> Result<BudgetEvaluation, RunBundleError> {
        validate_run_bundle(runs)?;
        let scheduled: Vec<_> = runs.iter().map(|run| run.instance.clone()).collect();
        let mut diagnostics = Vec::new();
        let mut incomplete = static_incomplete.to_vec();
        let mut coverage: BTreeMap<CheckKey, CheckCoverage> = BTreeMap::new();
        let mut spent = 0u64;
        let mut cut = runs.len();
        for (index, run) in runs.iter().enumerate() {
            let reads = u64::try_from(run.reads.len())
                .map_err(|_| RunBundleError::ReadCountOverflow(Box::new(run.instance.clone())))?;
            if budget
                .max_reads
                .is_some_and(|limit| spent.saturating_add(reads) > limit)
            {
                cut = index;
                break;
            }
            spent = spent
                .checked_add(reads)
                .ok_or_else(|| RunBundleError::ReadCountOverflow(Box::new(run.instance.clone())))?;
            diagnostics.extend(run.diagnostics.clone());
            incomplete.extend(run.incomplete.clone());
            let entry = coverage
                .entry(run.instance.check.clone())
                .or_insert_with(|| coverage_for(run));
            entry.instances += 1;
            if !run.diagnostics.is_empty() {
                entry.failed += 1;
            } else if !run.incomplete.is_empty() {
                entry.incomplete += 1;
            } else {
                entry.passed += 1;
            }
        }
        let skipped: Vec<_> = runs[cut..].iter().map(|run| run.instance.clone()).collect();
        if let Some(limit) = budget.max_reads {
            let mut first_by_check = BTreeMap::new();
            for run in &runs[cut..] {
                match first_by_check.entry(run.instance.check.clone()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert((run.instance.subject.clone(), run.requirement, run.class));
                    }
                    std::collections::btree_map::Entry::Occupied(entry)
                        if entry.get().1 != run.requirement || entry.get().2 != run.class =>
                    {
                        return Err(if entry.get().1 == run.requirement {
                            RunBundleError::InconsistentClass(Box::new(run.instance.check.clone()))
                        } else {
                            RunBundleError::InconsistentRequirement(Box::new(
                                run.instance.check.clone(),
                            ))
                        });
                    }
                    std::collections::btree_map::Entry::Occupied(_) => {}
                }
                let entry = coverage
                    .entry(run.instance.check.clone())
                    .or_insert_with(|| coverage_for(run));
                entry.instances += 1;
                entry.incomplete += 1;
            }
            for run in &runs[cut..] {
                let (from, _, _) = &first_by_check[&run.instance.check];
                incomplete.push(IncompleteCheck {
                    class: run.class,
                    check: run.instance.check.clone(),
                    requirement: run.requirement,
                    subject: run.instance.subject.clone(),
                    reason: IncompleteReason::Budget {
                        limit,
                        spent,
                        from: from.clone(),
                    },
                });
            }
        }
        Ok(BudgetEvaluation {
            report: finish_report(shape, diagnostics, incomplete),
            scheduled,
            skipped,
            spent,
            coverage,
        })
    }

    /// Trusted native input from which a witness is recomputed.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ValidationInput {
        /// Graph input.
        Graph {
            /// Schema.
            schema: Schema,
            /// Constraint-sort meanings.
            semantics: BTreeMap<Name, Semantics>,
            /// Instance.
            instance: Graph,
        },
        /// Table input.
        Table {
            /// Schema.
            schema: Schema,
            /// Constraint-sort meanings.
            semantics: BTreeMap<Name, Semantics>,
            /// Instance.
            instance: Table,
        },
        /// Tree input.
        Tree {
            /// Schema.
            schema: Schema,
            /// Constraint-sort meanings.
            semantics: BTreeMap<Name, Semantics>,
            /// Instance.
            instance: Tree,
        },
    }

    impl ValidationInput {
        const fn shape(&self) -> InstanceShape {
            match self {
                Self::Graph { .. } => InstanceShape::Graph,
                Self::Table { .. } => InstanceShape::Table,
                Self::Tree { .. } => InstanceShape::Tree,
            }
        }
    }

    /// Policy under which a retained prior validation was admitted.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum PriorAdmissionPolicy {
        /// The trusted prior must have passed every required check.
        RequirePassed,
        /// The trusted prior was retained under exact no-new-failures admission.
        NoNewFailures,
    }

    /// Admission and incremental evidence bound into a validation witness.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ValidationTrust {
        /// Trusted external verdicts and segment completeness.
        pub context: ValidationContext,
        /// Trusted external verdicts and segment completeness under which the
        /// retained baseline outcome and budget schedule were evaluated.
        pub baseline_context: Option<ValidationContext>,
        /// Optional validated baseline outcome.
        pub baseline: Option<ValidationOutcome>,
        /// Optional replayable evidence for the baseline's finite-budget cut.
        /// Callers cannot supply outcome bytes: virtual incomplete bundles are
        /// derived from the re-evaluated exact skipped suffix.
        pub baseline_budget: Option<BaselineBudgetEvidence>,
        /// Optional prior admitted outcome.
        pub prior: Option<ValidationOutcome>,
        /// Native input whose validation must equal `prior` when incremental
        /// evidence is supplied.
        pub prior_input: Option<Box<ValidationInput>>,
        /// Explicit policy under which the retained prior was admitted.
        pub prior_policy: Option<PriorAdmissionPolicy>,
        /// Canonically ordered touched elements for an incremental run.
        pub delta: Vec<ViewRef>,
        /// `true` means admission requires `Passed`; `false` means no new
        /// required failures/incompleteness relative to the baseline.
        pub require_passed: bool,
        /// Deterministic read budget.
        pub budget: ValidationBudget,
    }

    impl Default for ValidationTrust {
        fn default() -> Self {
            Self {
                context: ValidationContext::default(),
                baseline_context: None,
                baseline: None,
                baseline_budget: None,
                prior: None,
                prior_input: None,
                prior_policy: None,
                delta: Vec::new(),
                require_passed: true,
                budget: ValidationBudget::default(),
            }
        }
    }

    /// Complete outcome recomputed from a [`ValidationInput`].
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ValidationOutcome {
        /// A complete report.
        Report(Report),
        /// Native presentation refused before shared checks.
        Refused(ShapeRefusal),
    }

    fn evaluate_input(input: &ValidationInput, context: &ValidationContext) -> ValidationOutcome {
        match input {
            ValidationInput::Graph {
                schema,
                semantics,
                instance,
            } => ValidationOutcome::Report(validate_graph_with_context(
                schema, semantics, instance, context,
            )),
            ValidationInput::Table {
                schema,
                semantics,
                instance,
            } => validate_table_with_context(schema, semantics, instance, context)
                .map_or_else(ValidationOutcome::Refused, ValidationOutcome::Report),
            ValidationInput::Tree {
                schema,
                semantics,
                instance,
            } => validate_tree_with_context(schema, semantics, instance, context)
                .map_or_else(ValidationOutcome::Refused, ValidationOutcome::Report),
        }
    }

    /// A plan-derived canonical schedule. Its runs are private: callers can
    /// inspect identities but cannot substitute a shorter list.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TrustedSchedule {
        shape: InstanceShape,
        runs: Vec<CheckRun>,
        plan_error: Option<RunBundleError>,
    }

    impl TrustedSchedule {
        /// Complete ordered check identities.
        #[must_use]
        pub fn instances(&self) -> Vec<CheckInstance> {
            self.runs.iter().map(|run| run.instance.clone()).collect()
        }

        /// Complete canonical runs, including passing dependency traces.
        #[must_use]
        pub fn runs(&self) -> &[CheckRun] {
            &self.runs
        }

        /// Check an externally retained schedule against the trusted complete
        /// list, rejecting omission, insertion and reordering alike.
        ///
        /// # Errors
        ///
        /// Returns [`RunBundleError::PlanMismatch`] when evaluation exposed a
        /// compiler omission, or [`RunBundleError::ScheduleMismatch`] unless
        /// every identity occurs once in exact canonical order.
        pub fn verify_instances(&self, supplied: &[CheckInstance]) -> Result<(), RunBundleError> {
            if let Some(error) = &self.plan_error {
                return Err(error.clone());
            }
            (supplied == self.instances())
                .then_some(())
                .ok_or(RunBundleError::ScheduleMismatch)
        }
    }

    const fn input_schema_and_semantics(
        input: &ValidationInput,
    ) -> (&Schema, &BTreeMap<Name, Semantics>) {
        match input {
            ValidationInput::Graph {
                schema, semantics, ..
            }
            | ValidationInput::Table {
                schema, semantics, ..
            }
            | ValidationInput::Tree {
                schema, semantics, ..
            } => (schema, semantics),
        }
    }

    fn plan_nodes(input: &ValidationInput) -> Vec<(ViewRef, Name)> {
        match input {
            ValidationInput::Graph { instance, .. } => instance
                .nodes
                .iter()
                .filter(|(_, node)| node.shape != NodeShape::HyperEdge)
                .map(|(id, node)| (ViewRef::Node(*id), node.anchor.clone()))
                .collect(),
            ValidationInput::Table { instance, .. } => instance
                .rows
                .iter()
                .map(|(id, (table, _))| (ViewRef::Row(*id), table.clone()))
                .collect(),
            ValidationInput::Tree { instance, .. } => instance
                .nodes
                .iter()
                .map(|(id, node)| (ViewRef::Node(*id), node.anchor.clone()))
                .collect(),
        }
    }

    fn plan_edges(input: &ValidationInput) -> Vec<(ViewRef, SEdge, ViewRef, ViewRef)> {
        match input {
            ValidationInput::Graph { instance, .. } => instance
                .edges
                .iter()
                .filter(|(_, edge)| !is_fan_kind(&edge.label.kind))
                .map(|(id, edge)| {
                    (
                        ViewRef::Edge(*id),
                        edge.label.clone(),
                        ViewRef::Node(edge.src),
                        ViewRef::Node(edge.tgt),
                    )
                })
                .collect(),
            ValidationInput::Table { instance, .. } => instance
                .pairs
                .iter()
                .filter(|(edge, _, _)| !is_fan_kind(&edge.kind))
                .map(|(edge, source, target)| {
                    (
                        ViewRef::Pair(Box::new(edge.clone()), *source, *target),
                        edge.clone(),
                        ViewRef::Row(*source),
                        ViewRef::Row(*target),
                    )
                })
                .collect(),
            ValidationInput::Tree { instance, .. } => instance
                .arcs
                .iter()
                .map(|(parent, child, edge)| {
                    (
                        ViewRef::Arc {
                            parent: *parent,
                            child: *child,
                        },
                        edge.clone(),
                        ViewRef::Node(*parent),
                        ViewRef::Node(*child),
                    )
                })
                .collect(),
        }
    }

    fn plan_incidence_edges(input: &ValidationInput) -> Vec<(ViewRef, SEdge, ViewRef, ViewRef)> {
        match input {
            ValidationInput::Graph { instance, .. } => instance
                .edges
                .iter()
                .map(|(id, edge)| {
                    (
                        ViewRef::Edge(*id),
                        edge.label.clone(),
                        ViewRef::Node(edge.src),
                        ViewRef::Node(edge.tgt),
                    )
                })
                .collect(),
            ValidationInput::Tree {
                schema, instance, ..
            } => tree_validation_graph(instance, schema).map_or_else(
                |_| plan_edges(input),
                |(graph, _)| {
                    graph
                        .edges
                        .into_iter()
                        .map(|(id, edge)| {
                            let subject = usize::try_from(id)
                                .ok()
                                .and_then(|index| instance.arcs.get(index))
                                .map_or_else(
                                    || ViewRef::Edge(id),
                                    |(parent, child, _)| ViewRef::Arc {
                                        parent: *parent,
                                        child: *child,
                                    },
                                );
                            (
                                subject,
                                edge.label,
                                ViewRef::Node(edge.src),
                                ViewRef::Node(edge.tgt),
                            )
                        })
                        .collect()
                },
            ),
            ValidationInput::Table { .. } => plan_edges(input),
        }
    }

    fn plan_all_refs(input: &ValidationInput) -> Vec<ViewRef> {
        let mut refs: Vec<_> = plan_nodes(input)
            .into_iter()
            .map(|(reference, _)| reference)
            .collect();
        refs.extend(
            plan_incidence_edges(input)
                .into_iter()
                .map(|(reference, ..)| reference),
        );
        if let ValidationInput::Tree {
            schema, instance, ..
        } = input
        {
            if let Ok((graph, _)) = tree_validation_graph(instance, schema) {
                refs.extend(graph.nodes.into_keys().map(ViewRef::Node));
            }
        }
        refs.sort();
        refs.dedup();
        refs
    }

    fn insert_plan_run(
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
        check: CheckKey,
        subject: Option<ViewRef>,
        requirement: Requirement,
        reads: Vec<ViewRef>,
    ) {
        let class = check.kind.class();
        insert_plan_run_with_class(runs, check, subject, class, requirement, reads);
    }

    fn insert_plan_run_with_class(
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
        check: CheckKey,
        subject: Option<ViewRef>,
        class: CheckClass,
        requirement: Requirement,
        mut reads: Vec<ViewRef>,
    ) {
        reads.sort();
        reads.dedup();
        let instance = CheckInstance { check, subject };
        runs.entry(instance.clone()).or_insert(CheckRun {
            instance,
            class,
            requirement,
            reads,
            diagnostics: Vec::new(),
            incomplete: Vec::new(),
        });
    }

    fn local_reads(
        subject: &ViewRef,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
    ) -> Vec<ViewRef> {
        let mut reads = vec![subject.clone()];
        reads.extend(edges.iter().filter_map(|(edge, _, source, target)| {
            (source == subject || target == subject).then_some(edge.clone())
        }));
        reads
    }

    fn tree_parent_reads(input: &ValidationInput, subject: &ViewRef) -> Vec<ViewRef> {
        let mut reads = vec![subject.clone()];
        let ValidationInput::Tree { instance, .. } = input else {
            return reads;
        };
        let ViewRef::Node(subject_id) = subject else {
            return reads;
        };
        reads.extend(
            instance
                .arcs
                .iter()
                .filter(|(_, child, _)| *child == *subject_id)
                .map(|(parent, child, _)| ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                }),
        );
        reads
    }

    fn tree_reachability_reads(input: &ValidationInput, subject: &ViewRef) -> Vec<ViewRef> {
        let mut reads = vec![subject.clone()];
        let ValidationInput::Tree { instance, .. } = input else {
            return reads;
        };
        let ViewRef::Node(subject_id) = subject else {
            return reads;
        };
        let mut frontier = vec![*subject_id];
        let mut visited = BTreeSet::from([*subject_id]);
        while let Some(child) = frontier.pop() {
            for (parent, found_child, _) in &instance.arcs {
                if *found_child != child {
                    continue;
                }
                reads.push(ViewRef::Arc {
                    parent: *parent,
                    child: *found_child,
                });
                reads.push(ViewRef::Node(*parent));
                if visited.insert(*parent) {
                    frontier.push(*parent);
                }
            }
        }
        reads
    }

    fn outgoing_reads(
        subject: &ViewRef,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
        label: Option<&SEdge>,
    ) -> Vec<ViewRef> {
        let mut reads = vec![subject.clone()];
        reads.extend(edges.iter().filter_map(|(edge, found, source, _)| {
            (source == subject && label.is_none_or(|expected| found == expected))
                .then_some(edge.clone())
        }));
        reads
    }

    fn value_site_reads(
        schema: &Schema,
        vertex: &str,
        anchor: &str,
        subject: &ViewRef,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
    ) -> Vec<ViewRef> {
        let relevant: BTreeSet<_> = schema
            .edges
            .iter()
            .filter(|edge| edge.kind == "prop" && edge.src == anchor && edge.tgt == vertex)
            .cloned()
            .collect();
        let mut reads = vec![subject.clone()];
        reads.extend(edges.iter().filter_map(|(reference, edge, source, _)| {
            (source == subject && relevant.contains(edge)).then_some(reference.clone())
        }));
        reads
    }

    fn primary_required_class(key: &crate::schema::Key) -> CheckClass {
        if key.parts.iter().any(|part| match part {
            crate::schema::KeyPart::Path(path) => !path.is_empty(),
            crate::schema::KeyPart::Position { .. } => true,
        }) {
            CheckClass::MaintainedGlobal
        } else {
            CheckClass::GraphLocal
        }
    }

    fn primary_required_reads(
        subject: &ViewRef,
        key: &crate::schema::Key,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
    ) -> Vec<ViewRef> {
        let mut reads = vec![subject.clone()];
        for part in &key.parts {
            match part {
                crate::schema::KeyPart::Path(path) => {
                    let mut frontier = BTreeSet::from([subject.clone()]);
                    for label in path {
                        let mut next = BTreeSet::new();
                        for (edge, found, source, target) in edges {
                            if found == label && frontier.contains(source) {
                                reads.push(edge.clone());
                                reads.push(target.clone());
                                next.insert(target.clone());
                            }
                        }
                        frontier = next;
                    }
                }
                crate::schema::KeyPart::Position { within } => {
                    let carriers: Vec<_> = edges
                        .iter()
                        .filter(|(_, found, _, target)| found == within && target == subject)
                        .collect();
                    for (carrier, _, parent, _) in carriers {
                        reads.push(carrier.clone());
                        reads.push(parent.clone());
                        for (edge, found, source, target) in edges {
                            if found == within && source == parent {
                                reads.push(edge.clone());
                                reads.push(target.clone());
                            }
                        }
                    }
                }
            }
        }
        reads
    }

    fn compile_structural_plan(
        input: &ValidationInput,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let nodes = plan_nodes(input);
        let edges = plan_edges(input);
        let incidence_edges = plan_incidence_edges(input);
        insert_plan_run(
            runs,
            CheckKey {
                kind: CheckKind::SchemaWellFormed,
                decl: SchemaRef::Schema,
            },
            None,
            Requirement::Required,
            Vec::new(),
        );
        compile_structural_node_plan(input, &nodes, runs);
        compile_structural_edge_plan(input, &edges, runs);
        compile_fan_and_root_plan(input, &incidence_edges, runs);
    }

    fn compile_structural_node_plan(
        input: &ValidationInput,
        nodes: &[(ViewRef, Name)],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let schema = input_schema_and_semantics(input).0;
        for (subject, anchor) in nodes {
            insert_plan_run(
                runs,
                CheckKey {
                    kind: CheckKind::NodeAnchor,
                    decl: SchemaRef::Vertex(anchor.clone()),
                },
                Some(subject.clone()),
                Requirement::Required,
                vec![subject.clone()],
            );
            if schema.vertices.contains_key(anchor) {
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::Attribute,
                        decl: SchemaRef::Vertex(anchor.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    vec![subject.clone()],
                );
            }
            if input.shape() == InstanceShape::Tree {
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::ReservedVocabulary,
                        decl: SchemaRef::Vertex(anchor.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    vec![subject.clone()],
                );
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::TreeParentMap,
                        decl: SchemaRef::Vertex(anchor.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    tree_parent_reads(input, subject),
                );
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::TreeReachable,
                        decl: SchemaRef::Vertex(anchor.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    tree_reachability_reads(input, subject),
                );
            }
        }
    }

    fn compile_structural_edge_plan(
        input: &ValidationInput,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        for (subject, edge, source, target) in edges {
            insert_plan_run(
                runs,
                CheckKey {
                    kind: CheckKind::EdgeDeclared,
                    decl: SchemaRef::Edge(edge.clone()),
                },
                Some(subject.clone()),
                Requirement::Required,
                vec![subject.clone()],
            );
            insert_plan_run(
                runs,
                CheckKey {
                    kind: CheckKind::EndpointCoherence,
                    decl: SchemaRef::Edge(edge.clone()),
                },
                Some(subject.clone()),
                Requirement::Required,
                vec![subject.clone(), source.clone(), target.clone()],
            );
        }
        if let ValidationInput::Tree { instance, .. } = input {
            for (parent, child, edge) in &instance.arcs {
                let subject = ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                };
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::ReservedVocabulary,
                        decl: SchemaRef::Edge(edge.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    vec![subject],
                );
            }
        }
    }

    fn compile_fan_and_root_plan(
        input: &ValidationInput,
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        if let Some(graph) = planning_graph(input) {
            for id in fan_candidates(&graph)
                .into_iter()
                .filter(|node| is_presentation_fan_node(input.shape(), &graph, *node))
            {
                let subject = match input {
                    ValidationInput::Table { .. } => ViewRef::Row(id),
                    ValidationInput::Graph { .. } | ValidationInput::Tree { .. } => {
                        ViewRef::Node(id)
                    }
                };
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::FanWellFormed,
                        decl: SchemaRef::Vertex(graph.nodes[&id].anchor.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    local_reads(&subject, edges),
                );
            }
        }

        if let ValidationInput::Tree {
            schema, instance, ..
        } = input
        {
            let embedding = tree_validation_graph(instance, schema)
                .ok()
                .map(|(_, embedding)| embedding);
            for (index, fan) in instance.fans.iter().enumerate() {
                let mut reads = vec![ViewRef::Node(fan.parent)];
                reads.extend(fan.children.values().copied().map(ViewRef::Node));
                if let Some((node, parent, children)) =
                    embedding.as_ref().and_then(|value| value.fans.get(index))
                {
                    reads.push(ViewRef::Node(*node));
                    reads.push(ViewRef::Edge(*parent));
                    reads.extend(children.values().copied().map(ViewRef::Edge));
                }
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::FanWellFormed,
                        decl: SchemaRef::Vertex(fan.hyper_edge.clone()),
                    },
                    Some(ViewRef::Node(fan.parent)),
                    Requirement::Required,
                    reads,
                );
            }
            insert_plan_run(
                runs,
                CheckKey {
                    kind: CheckKind::TreeRoot,
                    decl: SchemaRef::Schema,
                },
                None,
                Requirement::Required,
                vec![ViewRef::Node(instance.root)],
            );
        }
    }

    fn compile_declaration_plan(
        input: &ValidationInput,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let (schema, semantics) = input_schema_and_semantics(input);
        let nodes = plan_nodes(input);
        let edges = plan_edges(input);
        let all_reads = plan_all_refs(input);
        compile_constraint_plan(input, schema, semantics, &nodes, &edges, &all_reads, runs);
        compile_required_and_ordering_plan(input.shape(), schema, &nodes, &edges, runs);
        compile_key_and_reference_plan(input, schema, &nodes, &all_reads, runs);
    }

    fn planning_graph(input: &ValidationInput) -> Option<Graph> {
        match input {
            ValidationInput::Graph { instance, .. } => Some(instance.clone()),
            ValidationInput::Table { instance, .. } => representation::table_to_graph(instance)
                .ok()
                .map(|(graph, _)| graph),
            ValidationInput::Tree {
                schema, instance, ..
            } => tree_validation_graph(instance, schema)
                .ok()
                .map(|(graph, _)| graph),
        }
    }

    const fn subject_id(subject: &ViewRef) -> Option<u32> {
        match subject {
            ViewRef::Node(id) | ViewRef::Row(id) => Some(*id),
            ViewRef::Edge(_) | ViewRef::Pair(_, _, _) | ViewRef::Arc { .. } => None,
        }
    }

    fn compile_constraint_plan(
        input: &ValidationInput,
        schema: &Schema,
        semantics: &BTreeMap<Name, Semantics>,
        nodes: &[(ViewRef, Name)],
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
        all_reads: &[ViewRef],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let graph = planning_graph(input);
        for (vertex, constraints) in &schema.constraints {
            for (sort, bound) in constraints {
                let decl = SchemaRef::Constraint(vertex.clone(), sort.clone(), bound.clone());
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::DeclarationInterpretable,
                        decl: decl.clone(),
                    },
                    None,
                    Requirement::Required,
                    Vec::new(),
                );
                for kind in dependent_constraint_checks(semantics.get(sort)) {
                    if *kind == CheckKind::Acyclic {
                        if input.shape() != InstanceShape::Graph {
                            continue;
                        }
                        insert_plan_run(
                            runs,
                            CheckKey {
                                kind: *kind,
                                decl: decl.clone(),
                            },
                            None,
                            Requirement::Required,
                            all_reads.to_vec(),
                        );
                    } else {
                        for (subject, anchor) in nodes
                            .iter()
                            .filter(|(_, anchor)| schema.vertices.contains_key(anchor))
                        {
                            let applies = match *kind {
                                CheckKind::OutDegree => anchor == vertex,
                                CheckKind::External
                                | CheckKind::RequiredValue
                                | CheckKind::ValueConstraint => graph
                                    .as_ref()
                                    .and_then(|value| subject_id(subject).map(|id| (value, id)))
                                    .is_some_and(|(value, id)| {
                                        !value_sites(schema, value, vertex, id).is_empty()
                                    }),
                                _ => false,
                            };
                            if !applies {
                                continue;
                            }
                            insert_plan_run(
                                runs,
                                CheckKey {
                                    kind: *kind,
                                    decl: decl.clone(),
                                },
                                Some(subject.clone()),
                                Requirement::Required,
                                match *kind {
                                    CheckKind::OutDegree => outgoing_reads(subject, edges, None),
                                    CheckKind::External => local_reads(subject, edges),
                                    CheckKind::RequiredValue | CheckKind::ValueConstraint => {
                                        value_site_reads(schema, vertex, anchor, subject, edges)
                                    }
                                    _ => Vec::new(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn compile_required_and_ordering_plan(
        shape: InstanceShape,
        schema: &Schema,
        nodes: &[(ViewRef, Name)],
        edges: &[(ViewRef, SEdge, ViewRef, ViewRef)],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        for (vertex, required) in &schema.required {
            for edge in required {
                for (subject, _) in nodes.iter().filter(|(_, anchor)| anchor == vertex) {
                    insert_plan_run(
                        runs,
                        CheckKey {
                            kind: CheckKind::RequiredEdge,
                            decl: SchemaRef::Required(vertex.clone(), edge.clone()),
                        },
                        Some(subject.clone()),
                        Requirement::Required,
                        outgoing_reads(subject, edges, Some(edge)),
                    );
                }
            }
        }
        if shape != InstanceShape::Graph {
            return;
        }
        for edge in schema.orderings.keys() {
            for (subject, _) in nodes.iter().filter(|(_, anchor)| *anchor == edge.src) {
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::PositionDistinct,
                        decl: SchemaRef::Ordering(edge.clone()),
                    },
                    Some(subject.clone()),
                    Requirement::Required,
                    outgoing_reads(subject, edges, Some(edge)),
                );
            }
        }
    }

    fn compile_key_and_reference_plan(
        input: &ValidationInput,
        schema: &Schema,
        nodes: &[(ViewRef, Name)],
        all_reads: &[ViewRef],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        compile_key_plan(input, schema, nodes, all_reads, runs);
        compile_reference_plan(input, schema, nodes, all_reads, runs);
    }

    fn compile_key_plan(
        input: &ValidationInput,
        schema: &Schema,
        nodes: &[(ViewRef, Name)],
        all_reads: &[ViewRef],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let edges = plan_edges(input);
        for (vertex, keys) in &schema.keys {
            for key in keys {
                let decl = SchemaRef::Key(vertex.clone(), key.id.clone());
                for kind in dependent_key_checks(key.role) {
                    let check = CheckKey {
                        kind: *kind,
                        decl: decl.clone(),
                    };
                    match *kind {
                        CheckKind::RequiredValue => {
                            for (subject, _) in nodes.iter().filter(|(_, anchor)| anchor == vertex)
                            {
                                insert_plan_run_with_class(
                                    runs,
                                    check.clone(),
                                    Some(subject.clone()),
                                    primary_required_class(key),
                                    Requirement::Required,
                                    primary_required_reads(subject, key, &edges),
                                );
                            }
                        }
                        CheckKind::KeyOrder => {
                            if let KeyScope::Within(container) = &key.scope {
                                for (subject, _) in
                                    nodes.iter().filter(|(_, anchor)| anchor == container)
                                {
                                    insert_plan_run(
                                        runs,
                                        check.clone(),
                                        Some(subject.clone()),
                                        Requirement::Advisory,
                                        all_reads.to_vec(),
                                    );
                                }
                            }
                        }
                        _ => insert_plan_run(
                            runs,
                            check.clone(),
                            None,
                            Requirement::Required,
                            all_reads.to_vec(),
                        ),
                    }
                    if *kind == CheckKind::KeyUnique && matches!(&key.scope, KeyScope::Within(_)) {
                        for (subject, _) in nodes.iter().filter(|(_, anchor)| anchor == vertex) {
                            insert_plan_run(
                                runs,
                                check.clone(),
                                Some(subject.clone()),
                                Requirement::Advisory,
                                vec![subject.clone()],
                            );
                        }
                    }
                }
            }
        }
    }

    fn compile_reference_plan(
        input: &ValidationInput,
        schema: &Schema,
        nodes: &[(ViewRef, Name)],
        all_reads: &[ViewRef],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let graph = planning_graph(input);
        for reference in schema.references.values() {
            for (subject, _) in nodes
                .iter()
                .filter(|(_, anchor)| *anchor == reference.holder)
            {
                let selected = reference.guard.as_ref().is_none_or(|guard| {
                    graph
                        .as_ref()
                        .and_then(|value| subject_id(subject).map(|id| (value, id)))
                        .and_then(|(value, id)| keys::path_value(value, id, &guard.field))
                        .is_some_and(|actual| {
                            crate::value::representation(&actual, &guard.equals) == Some(true)
                        })
                });
                if !selected {
                    continue;
                }
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::ReferenceResolves,
                        decl: SchemaRef::Reference(reference.id.clone()),
                    },
                    Some(subject.clone()),
                    reference_requirement(reference),
                    all_reads.to_vec(),
                );
            }
        }
        compile_unselected_guard_plan(schema, nodes, graph.as_ref(), all_reads, runs);
        for (target_set, declaration) in &schema.target_sets {
            let check = CheckKey {
                kind: CheckKind::KeyUnique,
                decl: SchemaRef::TargetSet(target_set.clone()),
            };
            insert_plan_run(
                runs,
                check.clone(),
                None,
                Requirement::Required,
                all_reads.to_vec(),
            );
            for (subject, _) in nodes.iter().filter(|(_, anchor)| {
                declaration
                    .members
                    .iter()
                    .any(|(vertex, _)| vertex == anchor)
            }) {
                insert_plan_run(
                    runs,
                    check.clone(),
                    Some(subject.clone()),
                    Requirement::Required,
                    all_reads.to_vec(),
                );
            }
        }
    }

    fn compile_unselected_guard_plan(
        schema: &Schema,
        nodes: &[(ViewRef, Name)],
        graph: Option<&Graph>,
        all_reads: &[ViewRef],
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        let guarded: Vec<_> = schema
            .references
            .values()
            .filter_map(|reference| {
                reference.guard.as_ref().map(|guard| {
                    (
                        reference.holder.as_str(),
                        &reference.source,
                        guard.field.as_slice(),
                        reference.id.as_str(),
                        &guard.equals,
                    )
                })
            })
            .collect();
        for (index, (holder, source, field, _, _)) in guarded.iter().enumerate() {
            if guarded[..index]
                .iter()
                .any(|(other_holder, other_source, other_field, _, _)| {
                    other_holder == holder && other_source == source && other_field == field
                })
            {
                continue;
            }
            let mut group: Vec<_> = guarded
                .iter()
                .filter(|(other_holder, other_source, other_field, _, _)| {
                    other_holder == holder && other_source == source && other_field == field
                })
                .map(|(_, _, _, reference, equals)| (*reference, *equals))
                .collect();
            group.sort_by_key(|(reference, _)| *reference);
            let Some((first_reference, _)) = group.first() else {
                continue;
            };
            for (subject, _) in nodes
                .iter()
                .filter(|(_, anchor)| anchor.as_str() == *holder)
            {
                let Some(value) = graph
                    .and_then(|value| subject_id(subject).map(|id| (value, id)))
                    .and_then(|(value, id)| keys::path_value(value, id, field))
                else {
                    continue;
                };
                if group.iter().any(|(_, expected)| {
                    crate::value::representation(&value, expected) == Some(true)
                }) {
                    continue;
                }
                insert_plan_run(
                    runs,
                    CheckKey {
                        kind: CheckKind::ReferenceResolves,
                        decl: SchemaRef::Reference((*first_reference).to_owned()),
                    },
                    Some(subject.clone()),
                    Requirement::Advisory,
                    all_reads.to_vec(),
                );
            }
        }
    }

    fn compile_static_plan(
        schema: &Schema,
        semantics: &BTreeMap<Name, Semantics>,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        for incomplete in static_findings(schema, semantics) {
            insert_plan_run_with_class(
                runs,
                incomplete.check,
                incomplete.subject,
                incomplete.class,
                incomplete.requirement,
                Vec::new(),
            );
        }
    }

    fn compile_segment_plan(
        schema: &Schema,
        semantics: &BTreeMap<Name, Semantics>,
        context: &ValidationContext,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) {
        if context.complete_segment {
            return;
        }
        for check in schema_dependent_checks(schema, semantics)
            .into_iter()
            .filter(|check| class_for_check(schema, check) == CheckClass::MaintainedGlobal)
        {
            let requirement = requirement_for_check(schema, &check);
            let class = class_for_check(schema, &check);
            insert_plan_run_with_class(runs, check, None, class, requirement, Vec::new());
        }
    }

    fn attach_report_outcomes(
        report: &Report,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) -> Result<(), RunBundleError> {
        for diagnostic in &report.diagnostics {
            let instance = CheckInstance {
                check: diagnostic.check.clone(),
                subject: diagnostic.subject.clone(),
            };
            let Some(run) = runs.get_mut(&instance) else {
                return Err(RunBundleError::PlanMismatch(Box::new(instance)));
            };
            if run.requirement != diagnostic.requirement || run.class != diagnostic.class {
                return Err(RunBundleError::PlanMismatch(Box::new(instance)));
            }
            run.diagnostics.push(diagnostic.clone());
        }
        for incomplete in &report.incomplete {
            let instance = CheckInstance {
                check: incomplete.check.clone(),
                subject: incomplete.subject.clone(),
            };
            let Some(run) = runs.get_mut(&instance) else {
                return Err(RunBundleError::PlanMismatch(Box::new(instance)));
            };
            if run.requirement != incomplete.requirement || run.class != incomplete.class {
                return Err(RunBundleError::PlanMismatch(Box::new(instance)));
            }
            run.incomplete.push(incomplete.clone());
        }
        Ok(())
    }

    fn attach_shape_refusal(
        shape: InstanceShape,
        refusal: &ShapeRefusal,
        runs: &mut BTreeMap<CheckInstance, CheckRun>,
    ) -> Result<(), RunBundleError> {
        let instance = CheckInstance {
            check: CheckKey {
                kind: CheckKind::SchemaWellFormed,
                decl: SchemaRef::Schema,
            },
            subject: None,
        };
        let Some(run) = runs.get_mut(&instance) else {
            return Err(RunBundleError::PlanMismatch(Box::new(instance)));
        };
        run.incomplete.push(IncompleteCheck {
            check: instance.check,
            class: CheckClass::Static,
            requirement: Requirement::Required,
            subject: None,
            reason: IncompleteReason::NativeShapeRefusal {
                shape,
                detail: format!("{refusal:?}"),
            },
        });
        Ok(())
    }

    /// Derive the bounded reference's complete canonical schedule from the
    /// native input and trusted validation context.
    #[must_use]
    pub fn trusted_schedule(input: &ValidationInput, trust: &ValidationTrust) -> TrustedSchedule {
        let mut runs: BTreeMap<CheckInstance, CheckRun> = BTreeMap::new();
        let (schema, semantics) = input_schema_and_semantics(input);
        compile_structural_plan(input, &mut runs);
        compile_static_plan(schema, semantics, &mut runs);
        compile_declaration_plan(input, &mut runs);
        compile_segment_plan(schema, semantics, &trust.context, &mut runs);
        let outcome = evaluate_input(input, &trust.context);
        let plan_error = match &outcome {
            ValidationOutcome::Report(report) => attach_report_outcomes(report, &mut runs),
            ValidationOutcome::Refused(refusal) => {
                attach_shape_refusal(input.shape(), refusal, &mut runs)
            }
        }
        .err();
        for run in runs.values_mut() {
            run.reads.sort();
            run.reads.dedup();
            run.diagnostics.sort_by(diagnostic_order);
            run.incomplete.sort_by(incomplete_order);
        }
        TrustedSchedule {
            shape: input.shape(),
            runs: runs.into_values().collect(),
            plan_error,
        }
    }

    fn evaluate_schedule(
        schedule: &TrustedSchedule,
        budget: ValidationBudget,
    ) -> Result<BudgetEvaluation, RunBundleError> {
        if let Some(error) = &schedule.plan_error {
            return Err(error.clone());
        }
        evaluate_runs(schedule.shape, &schedule.runs, &[], budget)
    }

    fn failed_plan_evaluation(
        schedule: &TrustedSchedule,
        error: &RunBundleError,
    ) -> BudgetEvaluation {
        let instance = match error {
            RunBundleError::PlanMismatch(instance) => (**instance).clone(),
            _ => CheckInstance {
                check: CheckKey {
                    kind: CheckKind::SchemaWellFormed,
                    decl: SchemaRef::Schema,
                },
                subject: None,
            },
        };
        let CheckInstance { check, subject } = instance;
        let incomplete = IncompleteCheck {
            class: check.kind.class(),
            check,
            requirement: Requirement::Required,
            subject,
            reason: IncompleteReason::Unsupported {
                detail: format!("ValidationPlanMismatch({error:?})"),
            },
        };
        BudgetEvaluation {
            report: finish_report(schedule.shape, Vec::new(), vec![incomplete]),
            scheduled: schedule.instances(),
            skipped: schedule.instances(),
            spent: 0,
            coverage: BTreeMap::new(),
        }
    }

    /// A replayable witness bound to its complete native input.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ValidationWitness {
        /// Trusted input retained by the witness.
        pub input: ValidationInput,
        /// Trusted baseline, prior, delta, policy, external verdicts and budget.
        pub trust: ValidationTrust,
        /// Plan-derived complete canonical schedule.
        pub schedule: TrustedSchedule,
        /// Canonically ordered committed outcome.
        pub outcome: ValidationOutcome,
        /// Budgeted evaluation of the trusted schedule.
        pub evaluation: BudgetEvaluation,
        /// Admission decision under the retained policy and baseline.
        pub admitted: bool,
        sealed: Box<WitnessSeal>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct WitnessSeal {
        input: ValidationInput,
        trust: ValidationTrust,
        schedule: TrustedSchedule,
        outcome: ValidationOutcome,
        evaluation: BudgetEvaluation,
        admitted: bool,
    }

    impl ValidationWitness {
        /// Recompute native validation from the retained input and compare the
        /// complete canonically ordered outcome.
        ///
        /// # Errors
        ///
        /// Returns `WitnessMismatch` when any finding, incomplete entry,
        /// native identity or shape refusal differs from the commitment.
        pub fn verify(&self) -> Result<(), WitnessError> {
            let schedule = trusted_schedule(&self.input, &self.trust);
            let outcome = evaluate_input(&self.input, &self.trust.context);
            let evaluation = evaluate_schedule(&schedule, self.trust.budget)
                .map_err(|_| WitnessError::WitnessMismatch)?;
            let admitted = witness_admitted(&outcome, &evaluation.report, &self.trust);
            let incremental = trusted_incremental_is_valid(&self.input, &self.trust);
            let current = WitnessSeal {
                input: self.input.clone(),
                trust: self.trust.clone(),
                schedule: schedule.clone(),
                outcome: outcome.clone(),
                evaluation: evaluation.clone(),
                admitted,
            };
            if incremental
                && current == *self.sealed
                && self.schedule == schedule
                && self.outcome == outcome
                && self.evaluation == evaluation
                && self.admitted == admitted
            {
                Ok(())
            } else {
                Err(WitnessError::WitnessMismatch)
            }
        }
    }

    /// Why validation-witness replay failed.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum WitnessError {
        /// Recomputed canonical validation disagrees with the commitment.
        WitnessMismatch,
        /// Incremental delta elements are not in strict canonical order.
        NonCanonicalDelta,
        /// Prior input, prior outcome and exact changed-element delta disagree.
        IncrementalMismatch,
    }

    /// Construct a checked validation witness.
    ///
    /// # Errors
    ///
    /// This constructor is total because native refusals are committed as
    /// [`ValidationOutcome::Refused`] and verified exactly.
    #[must_use]
    pub fn witness(input: ValidationInput) -> ValidationWitness {
        build_witness(input, ValidationTrust::default())
    }

    /// Construct a checked validation witness with explicit trusted evidence.
    ///
    /// # Errors
    ///
    /// Returns [`WitnessError::NonCanonicalDelta`] for a reordered or
    /// duplicate incremental delta.
    pub fn witness_with_trust(
        input: ValidationInput,
        trust: ValidationTrust,
    ) -> Result<ValidationWitness, WitnessError> {
        if trust.delta.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(WitnessError::NonCanonicalDelta);
        }
        if !trusted_incremental_is_valid(&input, &trust) {
            return Err(WitnessError::IncrementalMismatch);
        }
        Ok(build_witness(input, trust))
    }

    fn build_witness(input: ValidationInput, trust: ValidationTrust) -> ValidationWitness {
        let schedule = trusted_schedule(&input, &trust);
        let outcome = evaluate_input(&input, &trust.context);
        let evaluation = evaluate_schedule(&schedule, trust.budget)
            .unwrap_or_else(|error| failed_plan_evaluation(&schedule, &error));
        let admitted = witness_admitted(&outcome, &evaluation.report, &trust);
        let sealed = Box::new(WitnessSeal {
            input: input.clone(),
            trust: trust.clone(),
            schedule: schedule.clone(),
            outcome: outcome.clone(),
            evaluation: evaluation.clone(),
            admitted,
        });
        ValidationWitness {
            input,
            trust,
            schedule,
            outcome,
            evaluation,
            admitted,
            sealed,
        }
    }

    fn witness_admitted(
        outcome: &ValidationOutcome,
        report: &Report,
        trust: &ValidationTrust,
    ) -> bool {
        if !matches!(outcome, ValidationOutcome::Report(_)) {
            return false;
        }
        let result = required_outcome_bundles(report);
        if trust.require_passed {
            return result.is_empty();
        }
        let Some(baseline_evaluation) = baseline_evaluation(trust) else {
            return false;
        };
        let baseline =
            required_outcome_bundles_with(&baseline_evaluation.report, BundleSelection::Evaluated);
        let baseline_cut =
            required_outcome_bundles_with(&baseline_evaluation.report, BundleSelection::Budget);
        admits(false, &baseline, &baseline_cut, &result)
    }

    #[derive(Default)]
    struct RequiredOutcomeBundle<'a> {
        diagnostics: Vec<&'a Diagnostic>,
        incomplete: Vec<&'a IncompleteCheck>,
    }

    fn required_outcome_bundles(report: &Report) -> BTreeMap<CheckInstance, Vec<u8>> {
        required_outcome_bundles_with(report, BundleSelection::All)
    }

    #[derive(Clone, Copy)]
    enum BundleSelection {
        All,
        Evaluated,
        Budget,
    }

    fn required_outcome_bundles_with(
        report: &Report,
        selection: BundleSelection,
    ) -> BTreeMap<CheckInstance, Vec<u8>> {
        let mut grouped: BTreeMap<CheckInstance, RequiredOutcomeBundle<'_>> = BTreeMap::new();
        for diagnostic in report.diagnostics.iter().filter(|entry| {
            entry.requirement == Requirement::Required
                && !matches!(selection, BundleSelection::Budget)
        }) {
            grouped
                .entry(CheckInstance {
                    check: diagnostic.check.clone(),
                    subject: diagnostic.subject.clone(),
                })
                .or_default()
                .diagnostics
                .push(diagnostic);
        }
        for incomplete in report.incomplete.iter().filter(|entry| {
            entry.requirement == Requirement::Required
                && match selection {
                    BundleSelection::All => true,
                    BundleSelection::Evaluated => {
                        !matches!(entry.reason, IncompleteReason::Budget { .. })
                    }
                    BundleSelection::Budget => {
                        matches!(entry.reason, IncompleteReason::Budget { .. })
                    }
                }
        }) {
            grouped
                .entry(CheckInstance {
                    check: incomplete.check.clone(),
                    subject: incomplete.subject.clone(),
                })
                .or_default()
                .incomplete
                .push(incomplete);
        }
        grouped
            .into_iter()
            .map(|(instance, bundle)| {
                let mut writer = crate::canonical::Writer::default();
                writer.len(bundle.diagnostics.len());
                for diagnostic in bundle.diagnostics {
                    writer.raw(&diagnostic_outcome_bytes(diagnostic));
                }
                writer.len(bundle.incomplete.len());
                for entry in bundle.incomplete {
                    writer.raw(&incomplete_outcome_bytes(entry));
                }
                (instance, writer.bytes)
            })
            .collect()
    }

    fn diagnostic_outcome_bytes(diagnostic: &Diagnostic) -> Vec<u8> {
        let mut writer = crate::canonical::Writer::default();
        writer.tag(check_class_tag(diagnostic.class));
        writer.tag(requirement_tag(diagnostic.requirement));
        write_views(&mut writer, &diagnostic.elements);
        writer.raw(&violation_bytes(&diagnostic.violation));
        writer.bytes
    }

    fn incomplete_outcome_bytes(incomplete: &IncompleteCheck) -> Vec<u8> {
        let mut writer = crate::canonical::Writer::default();
        writer.tag(check_class_tag(incomplete.class));
        writer.tag(requirement_tag(incomplete.requirement));
        writer.raw(&incomplete_reason_bytes(&incomplete.reason));
        writer.bytes
    }

    const fn check_class_tag(class: CheckClass) -> u8 {
        match class {
            CheckClass::Static => 0,
            CheckClass::GraphLocal => 1,
            CheckClass::MaintainedGlobal => 2,
            CheckClass::BoundedExternal => 3,
        }
    }

    const fn requirement_tag(requirement: Requirement) -> u8 {
        match requirement {
            Requirement::Required => 0,
            Requirement::Advisory => 1,
        }
    }

    fn changed_map_keys<T: PartialEq>(
        left: &BTreeMap<u32, T>,
        right: &BTreeMap<u32, T>,
        wrap: impl Fn(u32) -> ViewRef,
        out: &mut BTreeSet<ViewRef>,
    ) {
        for key in left.keys().chain(right.keys()) {
            if left.get(key) != right.get(key) {
                out.insert(wrap(*key));
            }
        }
    }

    fn changed_refs(prior: &ValidationInput, current: &ValidationInput) -> Option<Vec<ViewRef>> {
        let mut changed = BTreeSet::new();
        match (prior, current) {
            (
                ValidationInput::Graph {
                    schema: left_schema,
                    semantics: left_semantics,
                    instance: left,
                },
                ValidationInput::Graph {
                    schema: right_schema,
                    semantics: right_semantics,
                    instance: right,
                },
            ) if left_schema == right_schema && left_semantics == right_semantics => {
                changed_map_keys(&left.nodes, &right.nodes, ViewRef::Node, &mut changed);
                changed_map_keys(&left.edges, &right.edges, ViewRef::Edge, &mut changed);
            }
            (
                ValidationInput::Table {
                    schema: left_schema,
                    semantics: left_semantics,
                    instance: left,
                },
                ValidationInput::Table {
                    schema: right_schema,
                    semantics: right_semantics,
                    instance: right,
                },
            ) if left_schema == right_schema && left_semantics == right_semantics => {
                changed_map_keys(&left.rows, &right.rows, ViewRef::Row, &mut changed);
                for (edge, source, target) in left.pairs.symmetric_difference(&right.pairs) {
                    changed.insert(ViewRef::Pair(Box::new(edge.clone()), *source, *target));
                }
            }
            (
                ValidationInput::Tree {
                    schema: left_schema,
                    semantics: left_semantics,
                    instance: left,
                },
                ValidationInput::Tree {
                    schema: right_schema,
                    semantics: right_semantics,
                    instance: right,
                },
            ) if left_schema == right_schema && left_semantics == right_semantics => {
                changed_map_keys(&left.nodes, &right.nodes, ViewRef::Node, &mut changed);
                let arc_count = left.arcs.len().max(right.arcs.len());
                for index in 0..arc_count {
                    if left.arcs.get(index) != right.arcs.get(index) {
                        for (parent, child, _) in [left.arcs.get(index), right.arcs.get(index)]
                            .into_iter()
                            .flatten()
                        {
                            changed.insert(ViewRef::Arc {
                                parent: *parent,
                                child: *child,
                            });
                        }
                    }
                }
                let fan_count = left.fans.len().max(right.fans.len());
                for index in 0..fan_count {
                    if left.fans.get(index) != right.fans.get(index) {
                        for fan in [left.fans.get(index), right.fans.get(index)]
                            .into_iter()
                            .flatten()
                        {
                            changed.insert(ViewRef::Node(fan.parent));
                            changed.extend(fan.children.values().copied().map(ViewRef::Node));
                        }
                    }
                }
                if left.root != right.root {
                    changed.insert(ViewRef::Node(left.root));
                    changed.insert(ViewRef::Node(right.root));
                }
            }
            _ => return None,
        }
        Some(changed.into_iter().collect())
    }

    fn baseline_evaluation(trust: &ValidationTrust) -> Option<BudgetEvaluation> {
        let prior_input = trust.prior_input.as_deref()?;
        let baseline_context = trust.baseline_context.as_ref()?;
        let baseline_trust = ValidationTrust {
            context: baseline_context.clone(),
            ..ValidationTrust::default()
        };
        let schedule = trusted_schedule(prior_input, &baseline_trust);
        let budget = trust
            .baseline_budget
            .as_ref()
            .map_or_else(ValidationBudget::default, |evidence| evidence.budget);
        let evaluation = evaluate_schedule(&schedule, budget).ok()?;
        if let Some(evidence) = &trust.baseline_budget
            && (evidence.scheduled != evaluation.scheduled
                || evidence.skipped != evaluation.skipped
                || evidence.spent != evaluation.spent)
        {
            return None;
        }
        Some(evaluation)
    }

    fn trusted_incremental_is_valid(input: &ValidationInput, trust: &ValidationTrust) -> bool {
        match (
            &trust.prior_input,
            &trust.prior,
            trust.prior_policy,
            trust.delta.is_empty(),
        ) {
            (None, None, None, true) => {
                trust.baseline_context.is_none()
                    && trust.baseline.is_none()
                    && trust.baseline_budget.is_none()
                    && trust.require_passed
            }
            (Some(prior_input), Some(prior @ ValidationOutcome::Report(_)), Some(policy), _) => {
                let Some(baseline_context) = &trust.baseline_context else {
                    return false;
                };
                let Some(baseline_evaluation) = baseline_evaluation(trust) else {
                    return false;
                };
                (policy == PriorAdmissionPolicy::NoNewFailures
                    || baseline_evaluation.report.status == CheckStatus::Passed)
                    && &evaluate_input(prior_input, baseline_context) == prior
                    && trust.baseline.as_ref() == Some(prior)
                    && changed_refs(prior_input, input).as_deref() == Some(trust.delta.as_slice())
            }
            _ => false,
        }
    }

    /// Definition: The admission policies over outcome bundles of required check instances.
    ///
    /// `RequirePassed` admits exactly a result with no failed or incomplete bundle; `NoNewFailures`
    /// admits iff every failed or incomplete bundle of the result was already open in the baseline with
    /// equal bytes, where each instance of a baseline budget cut counts as open with the given virtual
    /// incomplete bundle.
    ///
    /// Book anchors: `L-validation-12`.
    #[must_use]
    pub fn admits<K: Ord>(
        require_passed: bool,
        baseline: &BTreeMap<K, Vec<u8>>,
        baseline_cut: &BTreeMap<K, Vec<u8>>,
        result: &BTreeMap<K, Vec<u8>>,
    ) -> bool {
        if require_passed {
            return result.is_empty();
        }
        result
            .iter()
            .all(|(k, bytes)| baseline.get(k).or_else(|| baseline_cut.get(k)) == Some(bytes))
    }

    /// Definition: The validation-side declared key equality check.
    ///
    /// Book anchors: `L-validation-13`.
    #[must_use]
    pub fn key_equality(equality: &keys::KeyEquality, left: &Value, right: &Value) -> Option<bool> {
        keys::key_equivalence(equality, left, right)
    }

    /// Definition: Select a variable definition under first-defines or
    /// nearest-preceding binding.
    ///
    /// Book anchors: `L-validation-14`.
    #[must_use]
    pub fn variable_binding(
        definitions: &[(usize, String, u32)],
        occurrence_position: usize,
        name: &str,
        nearest_preceding: bool,
    ) -> Option<u32> {
        let matching = definitions.iter().filter(|(position, candidate, _)| {
            *position < occurrence_position && candidate == name
        });
        if nearest_preceding {
            matching
                .max_by_key(|(position, _, _)| *position)
                .map(|x| x.2)
        } else {
            matching
                .min_by_key(|(position, _, _)| *position)
                .map(|x| x.2)
        }
    }

    /// Definition: The declaration-side guard, target-set and sort-key forms are valid
    /// exactly when all independently stated predicates pass.
    ///
    /// Book anchors: `L-validation-15`.
    ///
    /// # Errors
    ///
    /// Returns every declaration error in canonical declaration order.
    pub fn key_forms(schema: &Schema) -> Result<(), Vec<crate::schema::DeclarationError>> {
        crate::schema::validate_declarations(schema)
    }

    /// Definition: Parsing with an advisory format annotation has the same result as
    /// parsing without the annotation.
    ///
    /// Book anchors: `L-validation-18`.
    #[must_use]
    pub fn annotated_format_parse<T: Eq>(without: &Option<T>, with: &Option<T>) -> bool {
        without == with
    }

    /// Definition: Advisory format validation is exact: it reports precisely the
    /// complement of the declared recognizer.
    ///
    /// Book anchors: `L-validation-19`.
    #[must_use]
    pub fn annotated_format_advisory(
        text: &str,
        accepts: impl FnOnce(&str) -> bool,
        reports: bool,
    ) -> bool {
        reports != accepts(text)
    }

    /// Definition: An asserting format accepts exactly the values admitted by its
    /// recognizer.
    ///
    /// Book anchors: `L-validation-20`.
    #[must_use]
    pub fn format_assertion_acceptance(
        text: &str,
        accepts: impl FnOnce(&str) -> bool,
        accepted: bool,
    ) -> bool {
        accepted == accepts(text)
    }
}

pub use self::implementation::{
    BaselineBudgetEvidence, BudgetEvaluation, CheckClass, CheckCoverage, CheckInstance, CheckKind,
    CheckRun, CheckStatus, Diagnostic, IncompleteCheck, IncompleteReason, InstanceShape, NativeRef,
    PriorAdmissionPolicy, Report, Requirement, RunBundleError, SchemaRef, Semantics, ShapeRefusal,
    TrustedSchedule, ValidationBudget, ValidationContext, ValidationInput, ValidationOutcome,
    ValidationTrust, ValidationWitness, Violation, WitnessError, admits, annotated_format_advisory,
    annotated_format_parse, evaluate_runs, format_assertion_acceptance, index_and_null_match,
    key_equality, key_forms, lexical_space, status, trusted_schedule, validate, validate_graph,
    validate_graph_with_context, validate_table, validate_table_with_context, validate_tree,
    validate_tree_with_context, value_sites, variable_binding, witness, witness_with_trust,
};
