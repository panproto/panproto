//! Provider-indexed reference-resolution lenses.

mod error;
mod laws;

use std::collections::BTreeMap;
use std::sync::Arc;

use panproto_inst::identity::{
    IdentityCarrier, IdentityPolicy, identity_status, resolution_identity,
};
use panproto_inst::{
    Digest, EdgeId, FInstance, FieldPresence, GInstance, InstError, InstanceShapeKind,
    ResolutionUnresolvedReason, RowId, Value, ViewRef,
};
use panproto_schema::{
    Cardinality, Edge, Key, KeyPart, KeyRole, KeyScope, OffsetUnit, OnUnresolved, Protocol,
    RefSource, RefTextSource, Reference, ResolveMode, Schema,
};

pub use self::error::{
    AssemblyDomainReason, ResolutionFeature, ResolutionInstanceError, ResolveDomainError,
    ResolveError,
};
pub use self::laws::{ResolutionLawReport, ResolutionLawViolation, check_resolution_laws};
pub use panproto_inst::{
    AnyInstance, AssemblyPlanId, AssemblyResidue, AssemblyTrace, DefinitionKey, RedirectedArc,
    RedirectedAttachment, RedirectedFragment, RefOccurrence, ResolutionComplement,
    ResolutionContextKind, ResolutionKind, ResolutionProviderId, ResolutionSpan, ResolutionSpec,
    ResolutionUnresolvedReason as UnresolvedReason, UnresolvedRef,
};

/// Runtime options that change unresolved-reference admission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    /// Record otherwise-fatal dangling and ambiguous references.
    pub lenient: bool,
}

/// Result of resolving a provider-specific path literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathResolution {
    /// Matching carriers, in ascending order.
    pub candidates: Vec<ViewRef>,
    /// Digest of exact external state consulted by the provider.
    pub context: Option<Digest>,
}

/// Loads bytes named by a value in the syntactic document.
pub trait ExternalTextStore: Send + Sync {
    /// Load the exact byte sequence named by `key`.
    ///
    /// # Errors
    ///
    /// Returns a typed resolution error when the key cannot be loaded.
    fn load(&self, key: &panproto_inst::Value) -> Result<Vec<u8>, ResolveError>;
}

/// Supplies token boundaries for one UTF-8 text carrier.
pub trait TokenBoundaryProvider: Send + Sync {
    /// Return byte offsets beginning at zero, strictly increasing, and ending
    /// at `utf8.len()`.
    ///
    /// # Errors
    ///
    /// Returns a typed resolution error when tokenization fails.
    fn boundaries(&self, text: ViewRef, utf8: &[u8]) -> Result<Vec<u64>, ResolveError>;
}

/// Fixed document-varying services for one resolution run.
#[derive(Clone, Copy, Default)]
pub struct ResolveContext<'a> {
    /// External text service.
    pub external_text: Option<&'a dyn ExternalTextStore>,
    /// Token-boundary service.
    pub token_boundaries: Option<&'a dyn TokenBoundaryProvider>,
}

impl ResolveContext<'_> {
    /// A context providing no external services.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            external_text: None,
            token_boundaries: None,
        }
    }
}

/// Format semantics needed by the generic resolution core.
pub trait ResolutionProvider: Send + Sync {
    /// Versioned provider identity.
    fn id(&self) -> ResolutionProviderId;

    /// Shape change implemented by this provider.
    fn kind(&self) -> ResolutionKind;

    /// Complete protocol descriptor, including custom codecs.
    fn protocol(&self) -> &Protocol;

    /// Compile an immutable identity-bearing plan.
    ///
    /// # Errors
    ///
    /// Returns a typed declaration or provider error.
    fn spec(&self, protocol: &Protocol, syntactic: &Schema)
    -> Result<ResolutionSpec, ResolveError>;

    /// Resolve a provider-specific path.
    ///
    /// # Errors
    ///
    /// The default reports an unsupported path syntax.
    fn resolve_path(
        &self,
        reference: &Reference,
        _holder: ViewRef,
        _literal: &panproto_inst::Value,
        _source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<PathResolution, ResolveError> {
        let syntax = match &reference.source {
            RefSource::Path { syntax, .. } => syntax.clone(),
            _ => panproto_schema::PathSyntax::IdRef,
        };
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::Path(syntax),
        })
    }

    /// Render a provider-specific canonical path.
    ///
    /// # Errors
    ///
    /// The default reports an unsupported path syntax.
    fn render_path(
        &self,
        reference: &Reference,
        _holder: ViewRef,
        _target: ViewRef,
        _view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<panproto_inst::Value, ResolveError> {
        let syntax = match &reference.source {
            RefSource::Path { syntax, .. } => syntax.clone(),
            _ => panproto_schema::PathSyntax::IdRef,
        };
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::Path(syntax),
        })
    }

    /// Assemble a source through a declared grammar.
    fn assembly_get(
        &self,
        _plan: &AssemblyPlanId,
        _source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, AssemblyResidue), ResolveError> {
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::Assembly(self.kind()),
        })
    }

    /// Put an assembled view using its exact residue.
    fn assembly_put(
        &self,
        _plan: &AssemblyPlanId,
        _view: &AnyInstance,
        _residue: &AssemblyResidue,
        _ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::Assembly(self.kind()),
        })
    }

    /// Collect every assembly-domain violation.
    fn assembly_check_put_domain(
        &self,
        _plan: &AssemblyPlanId,
        _view: &AnyInstance,
        _residue: &AssemblyResidue,
        _ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        vec![ResolveDomainError::Assembly {
            at: None,
            production: None,
            reason: AssemblyDomainReason::MissingProduction,
        }]
    }

    /// Construct a source and residue without an earlier get.
    fn assembly_put_canonical(
        &self,
        _plan: &AssemblyPlanId,
        _view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, AssemblyResidue), ResolveError> {
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::Assembly(self.kind()),
        })
    }
}

/// A validated immutable resolution lens.
pub struct Resolution {
    protocol_name: String,
    syntactic_schema: Schema,
    spec: ResolutionSpec,
    references: BTreeMap<panproto_gat::Name, Reference>,
    identity: panproto_inst::IdentityEnvelope,
    provider: Arc<dyn ResolutionProvider>,
}

impl std::fmt::Debug for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolution")
            .field("protocol_name", &self.protocol_name)
            .field("kind", &self.spec.kind)
            .field("provider", &self.spec.provider)
            .finish_non_exhaustive()
    }
}

impl Resolution {
    /// Compile and validate a provider's resolution specification.
    ///
    /// # Errors
    ///
    /// Returns a typed mismatch, declaration, or provider error.
    pub fn for_protocol(
        protocol: &Protocol,
        syntactic: &Schema,
        provider: Arc<dyn ResolutionProvider>,
    ) -> Result<Self, ResolveError> {
        if provider.protocol() != protocol {
            return Err(ResolveError::ProviderUnavailable {
                protocol: protocol.name.clone(),
            });
        }
        let spec = provider.spec(protocol, syntactic)?;
        if spec.provider != provider.id() || spec.kind != provider.kind() {
            return Err(ResolveError::ProviderUnavailable {
                protocol: protocol.name.clone(),
            });
        }
        if protocol.syntactic_shape != spec.kind.syntactic_shape()
            || protocol.semantic_shape != spec.kind.semantic_shape()
        {
            return Err(ResolveError::UnsupportedKind {
                syntactic: protocol.syntactic_shape,
                semantic: protocol.semantic_shape,
            });
        }
        let assembly_kind = matches!(
            spec.kind,
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree
        );
        if assembly_kind != spec.assembly.is_some() {
            return Err(ResolveError::UnsupportedFeature {
                provider: Some(spec.provider.clone()),
                feature: ResolutionFeature::Assembly(spec.kind),
            });
        }

        let mut references = syntactic.declarations.references.clone();
        for (id, reference) in &spec.protocol_references {
            if let Some(schema_reference) = references.get(id) {
                if schema_reference != reference {
                    return Err(ResolveError::ReferenceConflict {
                        reference: id.clone(),
                    });
                }
            } else {
                references.insert(id.clone(), reference.clone());
            }
        }
        validate_reference_plan(&spec, &references)?;
        let protocol_name = panproto_protocols::semantics::canonical_name(&protocol.name);
        let identity = resolution_identity(&protocol_name, syntactic, &spec);
        Ok(Self {
            protocol_name,
            syntactic_schema: syntactic.clone(),
            spec,
            references,
            identity,
            provider,
        })
    }

    /// Validated resolution specification.
    #[must_use]
    pub const fn spec(&self) -> &ResolutionSpec {
        &self.spec
    }

    /// Semantic target schema declared by the provider.
    #[must_use]
    pub const fn semantic_schema(&self) -> &Schema {
        &self.spec.semantic_schema
    }

    /// Syntactic source schema used to compile the resolution.
    #[must_use]
    pub const fn syntactic_schema(&self) -> &Schema {
        &self.syntactic_schema
    }

    /// Merged schema and provider reference declarations.
    #[must_use]
    pub const fn references(&self) -> &BTreeMap<panproto_gat::Name, Reference> {
        &self.references
    }

    /// Provider used by this resolution.
    #[must_use]
    pub fn provider(&self) -> &dyn ResolutionProvider {
        self.provider.as_ref()
    }

    /// Resolve a syntactic source into its semantic view and stamped state.
    pub fn get(
        &self,
        source: &AnyInstance,
        _options: &ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        expect_shape(source, self.spec.kind.syntactic_shape())?;
        let (view, mut complement) = match self.spec.kind {
            ResolutionKind::TreeToGraph => self.get_tree_graph(source, _options, ctx)?,
            ResolutionKind::TableToGraph => self.get_table_graph(source, _options, ctx)?,
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let plan = self.spec.assembly.as_ref().ok_or_else(|| {
                    ResolveError::UnsupportedFeature {
                        provider: Some(self.spec.provider.clone()),
                        feature: ResolutionFeature::Assembly(self.spec.kind),
                    }
                })?;
                let (view, residue) = self.provider.assembly_get(plan, source, ctx)?;
                expect_shape(&view, self.spec.kind.semantic_shape())?;
                let complement = ResolutionComplement {
                    assembly: Some(residue),
                    ..ResolutionComplement::default()
                };
                (view, complement)
            }
        };
        complement.stamp(self.identity.clone(), &self.syntactic_schema);
        Ok((view, complement))
    }

    /// Reconstruct a syntactic source from a semantic view and complement.
    pub fn put(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        expect_shape(view, self.spec.kind.semantic_shape())?;
        let status = identity_status(complement, &self.identity, &self.syntactic_schema)?;
        IdentityPolicy::Strict.admit(&status, complement.records_nothing())?;
        if let Err(errors) = self.check_put_domain(view, complement, ctx) {
            if let Some(first) = errors.into_iter().next() {
                return Err(ResolveError::PutDomain(first));
            }
        }
        match self.spec.kind {
            ResolutionKind::TreeToGraph => self.put_tree_graph(view, complement, ctx),
            ResolutionKind::TableToGraph => self.put_table_graph(view, complement, ctx),
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let plan = self.spec.assembly.as_ref().ok_or_else(|| {
                    ResolveError::UnsupportedFeature {
                        provider: Some(self.spec.provider.clone()),
                        feature: ResolutionFeature::Assembly(self.spec.kind),
                    }
                })?;
                let residue = complement.assembly.as_ref().ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::Assembly {
                        at: None,
                        production: None,
                        reason: AssemblyDomainReason::ResidueMismatch,
                    })
                })?;
                self.provider.assembly_put(plan, view, residue, ctx)
            }
        }
    }

    /// Return every put-domain violation in canonical order.
    pub fn check_put_domain(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), Vec<ResolveDomainError>> {
        let mut errors = Vec::new();
        if let Some(plan) = &self.spec.assembly {
            match &complement.assembly {
                Some(residue) => errors.extend(
                    self.provider
                        .assembly_check_put_domain(plan, view, residue, ctx),
                ),
                None => errors.push(ResolveDomainError::Assembly {
                    at: None,
                    production: None,
                    reason: AssemblyDomainReason::ResidueMismatch,
                }),
            }
        }
        if let Some(graph) = view.as_graph() {
            for (&node, _) in &complement.span_nodes {
                let surviving: Vec<_> = complement
                    .occurrences
                    .iter()
                    .filter_map(|(carrier, occurrences)| {
                        let ViewRef::Edge(edge_id) = carrier else {
                            return None;
                        };
                        let edge = graph.edge(*edge_id)?;
                        (edge.tgt == node).then_some((*edge_id, occurrences.first()))
                    })
                    .collect();
                let Some(record) = graph.node(node) else {
                    if surviving.is_empty() {
                        continue;
                    }
                    errors.push(ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Node(node),
                    });
                    continue;
                };
                let expected_anchor = surviving.iter().find_map(|(_, occurrence)| {
                    occurrence
                        .and_then(|occurrence| self.references.get(&occurrence.reference))
                        .map(|reference| &reference.target_vertex)
                });
                if surviving.is_empty()
                    || expected_anchor.is_some_and(|anchor| record.anchor != *anchor)
                    || record.value.is_some()
                    || record.discriminator.is_some()
                    || !record.extra_fields.is_empty()
                    || record.position.is_some()
                    || record.shape != panproto_inst::NodeShape::Plain
                    || !record.annotations.is_empty()
                {
                    errors.push(ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Node(node),
                    });
                }
            }
            let occurrence_edges: std::collections::BTreeSet<EdgeId> = complement
                .occurrences
                .keys()
                .filter_map(|carrier| match carrier {
                    ViewRef::Edge(id) => Some(*id),
                    _ => None,
                })
                .collect();
            let spanning_edges: std::collections::BTreeSet<EdgeId> = complement
                .spanning
                .values()
                .filter_map(|carrier| match carrier {
                    ViewRef::Edge(id) => Some(*id),
                    _ => None,
                })
                .collect();
            for carrier in &occurrence_edges {
                let Some(edge) = graph.edge(*carrier) else {
                    continue;
                };
                if edge.position.is_some() {
                    errors.push(ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Edge(*carrier),
                    });
                }
                let occurrence = complement
                    .occurrences
                    .get(&ViewRef::Edge(*carrier))
                    .and_then(|items| items.first());
                let historical = occurrence.and_then(|item| self.references.get(&item.reference));
                let writable = historical
                    .is_some_and(|reference| reference.edge.as_ref() == Some(&edge.edge))
                    || self.spec.reference_writers.contains_key(&edge.edge);
                if !writable {
                    errors.push(ResolveDomainError::UnrepresentableReference {
                        carrier: ViewRef::Edge(*carrier),
                        reference: occurrence
                            .map_or_else(String::new, |item| item.reference.to_string())
                            .into(),
                    });
                }
                if historical.is_some_and(|reference| reference.mode == ResolveMode::Redirect)
                    && occurrence.is_some_and(|item| {
                        holder_node(&item.holder)
                            .ok()
                            .is_none_or(|root| !complement.redirected.contains_key(&root))
                    })
                {
                    errors.push(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Edge(*carrier),
                    });
                }
            }
            for (id, edge) in graph.edges_sorted() {
                if occurrence_edges.contains(&id)
                    || spanning_edges.contains(&id)
                    || is_fan_edge(edge)
                {
                    continue;
                }
                if !self.spec.reference_writers.contains_key(&edge.edge) {
                    let mut references: Vec<_> = self
                        .references
                        .values()
                        .filter(|reference| reference.edge.as_ref() == Some(&edge.edge))
                        .map(|reference| reference.id.clone())
                        .collect();
                    references.sort_unstable();
                    if references.len() > 1 {
                        errors.push(ResolveDomainError::AmbiguousPlacement {
                            carrier: ViewRef::Edge(id),
                            references,
                        });
                    } else {
                        errors.push(ResolveDomainError::UnrepresentableReference {
                            carrier: ViewRef::Edge(id),
                            reference: references.into_iter().next().unwrap_or_default(),
                        });
                    }
                }
            }
            let reference_edges = self.reference_edges(graph, complement);
            let mut placements: BTreeMap<(u32, panproto_gat::Name), Vec<EdgeId>> = BTreeMap::new();
            for (id, reference_name) in reference_edges {
                let Some(edge) = graph.edge(id) else {
                    continue;
                };
                placements
                    .entry((edge.src, reference_name))
                    .or_default()
                    .push(id);
            }
            for ((_, reference_name), carriers) in placements {
                if self
                    .references
                    .get(&reference_name)
                    .is_some_and(|reference| {
                        reference.cardinality == Cardinality::One && carriers.len() > 1
                    })
                {
                    for carrier in carriers.into_iter().skip(1) {
                        errors.push(ResolveDomainError::AmbiguousPlacement {
                            carrier: ViewRef::Edge(carrier),
                            references: vec![reference_name.clone()],
                        });
                    }
                }
            }
        }
        errors.sort_by_key(domain_sort_key);
        errors.dedup();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Construct a canonical source and its stamped complement.
    pub fn put_canonical(
        &self,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        expect_shape(view, self.spec.kind.semantic_shape())?;
        match self.spec.kind {
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let plan = self.spec.assembly.as_ref().ok_or_else(|| {
                    ResolveError::UnsupportedFeature {
                        provider: Some(self.spec.provider.clone()),
                        feature: ResolutionFeature::Assembly(self.spec.kind),
                    }
                })?;
                let (source, residue) = self.provider.assembly_put_canonical(plan, view, ctx)?;
                let (regotten, mut produced) =
                    self.get(&source, &ResolveOptions::default(), ctx)?;
                if !instances_equivalent(view, &regotten) {
                    return Err(ResolveError::PutDomain(ResolveDomainError::Assembly {
                        at: None,
                        production: None,
                        reason: AssemblyDomainReason::ResidueMismatch,
                    }));
                }
                produced.assembly = Some(residue);
                produced.stamp(self.identity.clone(), &self.syntactic_schema);
                Ok((source, produced))
            }
            ResolutionKind::TreeToGraph => {
                let state = self.canonical_graph_complement(view)?;
                let source = self.put_tree_graph(view, &state, ctx)?;
                let (produced, complement) = self.get(&source, &ResolveOptions::default(), ctx)?;
                let complement = canonical_graph_result(view, &produced, complement)?;
                Ok((source, complement))
            }
            ResolutionKind::TableToGraph => {
                let state = self.canonical_graph_complement(view)?;
                let source = self.put_table_graph(view, &state, ctx)?;
                let (produced, complement) = self.get(&source, &ResolveOptions::default(), ctx)?;
                let complement = canonical_graph_result(view, &produced, complement)?;
                Ok((source, complement))
            }
        }
    }

    fn get_tree_graph(
        &self,
        source: &AnyInstance,
        options: &ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        let tree = source.as_tree().ok_or(ResolveError::ShapeMismatch {
            expected: InstanceShapeKind::Tree,
            found: source.shape(),
        })?;
        let (mut graph, embedding) =
            GInstance::from_wtype_embedded(tree, Some(&self.syntactic_schema))
                .map_err(|_| ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
        let mut complement = ResolutionComplement::default();
        for (&parent, children) in &tree.children_map {
            complement.order.insert(
                parent,
                children.iter().copied().map(ViewRef::Node).collect(),
            );
        }
        for (&child, &edge) in &embedding.arcs {
            complement.spanning.insert(child, ViewRef::Edge(edge));
        }
        let span_nodes =
            self.preallocate_tree_spans(tree, source, ctx, &mut graph, &mut complement)?;
        for reference in self.references.values() {
            let holders: Vec<u32> = tree_document_order(tree)
                .into_iter()
                .filter(|id| {
                    tree.node(*id)
                        .is_some_and(|node| node.anchor == reference.holder)
                })
                .collect();
            let key = target_key(&self.syntactic_schema, reference)?;
            let index = tree_key_index(tree, key, reference.target_vertex.as_ref());
            let mut variable_targets: BTreeMap<(Option<u32>, Vec<Vec<u8>>), u32> = BTreeMap::new();
            for holder in holders {
                let at = ViewRef::Node(holder);
                let occurrences =
                    self.tree_reference_values(tree, reference, holder, source, ctx)?;
                for (position, occurrence) in occurrences.into_iter().enumerate() {
                    let occurrence_index =
                        u32::try_from(position).map_err(|_| ResolveError::IdExhausted {
                            kind: panproto_inst::IdKind::Edge,
                        })?;
                    let scope_key = (
                        tree_scope(tree, holder, &key.scope),
                        canonical_tuple(&occurrence.key),
                    );
                    if matches!(reference.source, RefSource::Variable { .. })
                        && !variable_targets.contains_key(&scope_key)
                    {
                        let target = if tree
                            .node(holder)
                            .is_some_and(|node| node.anchor == reference.target_vertex)
                            && one_tree_key(tree, holder, key)
                                .is_some_and(|tuple| canonical_tuple(&tuple) == scope_key.1)
                        {
                            holder
                        } else {
                            let candidates = index.get(&scope_key).cloned().unwrap_or_default();
                            let [target] = candidates.as_slice() else {
                                let candidate_refs = candidates
                                    .into_iter()
                                    .map(ViewRef::Node)
                                    .collect::<Vec<_>>();
                                let _ = resolved_or_record(
                                    reference,
                                    at.clone(),
                                    occurrence_index,
                                    occurrence.literal.clone(),
                                    occurrence.context,
                                    candidate_refs,
                                    options,
                                    &mut complement,
                                )?;
                                continue;
                            };
                            *target
                        };
                        variable_targets.insert(scope_key, target);
                        complement.definitions.insert(
                            DefinitionKey {
                                reference: reference.id.clone(),
                                target,
                            },
                            at.clone(),
                        );
                        continue;
                    }
                    let candidates = if let Some(target) = occurrence
                        .span
                        .as_ref()
                        .and_then(|span| span_nodes.get(span))
                    {
                        vec![ViewRef::Node(*target)]
                    } else if matches!(reference.source, RefSource::Variable { .. }) {
                        variable_targets
                            .get(&scope_key)
                            .copied()
                            .map(ViewRef::Node)
                            .into_iter()
                            .collect()
                    } else {
                        match occurrence.candidates {
                            Some(candidates) => candidates,
                            None => index
                                .get(&scope_key)
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .map(ViewRef::Node)
                                .collect(),
                        }
                    };
                    let Some(target) = resolved_or_record(
                        reference,
                        at.clone(),
                        occurrence_index,
                        occurrence.literal.clone(),
                        occurrence.context,
                        candidates,
                        options,
                        &mut complement,
                    )?
                    else {
                        continue;
                    };
                    let target_id = carrier_node(&target)?;
                    let edge_source = match reference.mode {
                        ResolveMode::AddEdge => holder,
                        ResolveMode::Redirect => {
                            if graph.node(holder).is_none() {
                                return Err(ResolveError::PutDomain(
                                    ResolveDomainError::NoPlacement {
                                        carrier: ViewRef::Node(holder),
                                    },
                                ));
                            }
                            let (fragment, fan_indices) =
                                capture_redirected_fragment(tree, holder)?;
                            let parent = fragment
                                .attachment
                                .as_ref()
                                .map(|attachment| attachment.parent)
                                .ok_or_else(|| {
                                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                                        carrier: ViewRef::Node(holder),
                                    })
                                })?;
                            if fragment.nodes.contains_key(&target_id) {
                                return Err(ResolveError::Instance(
                                    ResolutionInstanceError::DanglingCarrier { carrier: target },
                                ));
                            }
                            remove_redirected_graph(
                                &mut graph,
                                &embedding,
                                &fragment,
                                &fan_indices,
                            )?;
                            for node in fragment.nodes.keys() {
                                complement.order.remove(node);
                                complement.spanning.remove(node);
                            }
                            complement.redirected.insert(holder, fragment);
                            parent
                        }
                    };
                    let edge =
                        reference
                            .edge
                            .clone()
                            .ok_or_else(|| ResolveError::ReferenceConflict {
                                reference: reference.id.clone(),
                            })?;
                    let edge_id = graph
                        .add_edge(edge_source, target_id, edge)
                        .map_err(map_graph_allocation)?;
                    let carrier = ViewRef::Edge(edge_id);
                    complement.occurrences.insert(
                        carrier,
                        vec![RefOccurrence {
                            holder: at.clone(),
                            reference: reference.id.clone(),
                            index: occurrence_index,
                            literal: occurrence.literal,
                            form: occurrence.form,
                            context: occurrence.context,
                        }],
                    );
                }
            }
        }
        Ok((AnyInstance::Graph(graph), complement))
    }

    fn preallocate_tree_spans(
        &self,
        tree: &panproto_inst::WInstance,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
        graph: &mut GInstance,
        complement: &mut ResolutionComplement,
    ) -> Result<BTreeMap<ResolutionSpan, u32>, ResolveError> {
        let mut pending: BTreeMap<ResolutionSpan, panproto_gat::Name> = BTreeMap::new();
        for reference in self.references.values() {
            if !matches!(reference.source, RefSource::Standoff { .. }) {
                continue;
            }
            let key = target_key(&self.syntactic_schema, reference)?;
            let index = tree_key_index(tree, key, reference.target_vertex.as_ref());
            for holder in tree_document_order(tree).into_iter().filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| node.anchor == reference.holder)
            }) {
                for occurrence in
                    self.tree_reference_values(tree, reference, holder, source, ctx)?
                {
                    let Some(span) = occurrence.span else {
                        continue;
                    };
                    let bucket = (
                        tree_scope(tree, holder, &key.scope),
                        canonical_tuple(&occurrence.key),
                    );
                    if index
                        .get(&bucket)
                        .is_some_and(|candidates| !candidates.is_empty())
                    {
                        continue;
                    }
                    if let Some(anchor) = pending.get(&span) {
                        if *anchor != reference.target_vertex {
                            return Err(ResolveError::ReferenceConflict {
                                reference: reference.id.clone(),
                            });
                        }
                    } else {
                        pending.insert(span, reference.target_vertex.clone());
                    }
                }
            }
        }
        let mut allocated = BTreeMap::new();
        let mut next = graph.max_node_id().map_or(Some(0), |id| id.checked_add(1));
        for (span, anchor) in pending {
            let id = next.ok_or(ResolveError::IdExhausted {
                kind: panproto_inst::IdKind::Node,
            })?;
            graph
                .insert_node(panproto_inst::Node::new(id, anchor))
                .map_err(map_graph_allocation)?;
            complement.span_nodes.insert(id, span.clone());
            allocated.insert(span, id);
            next = id.checked_add(1);
        }
        Ok(allocated)
    }

    fn get_table_graph(
        &self,
        source: &AnyInstance,
        options: &ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        let table = source.as_table().ok_or(ResolveError::ShapeMismatch {
            expected: InstanceShapeKind::Table,
            found: source.shape(),
        })?;
        let (mut graph, _) = GInstance::from_finstance(table)
            .map_err(|_| ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
        let mut complement = ResolutionComplement::default();
        let span_nodes =
            self.preallocate_table_spans(table, source, ctx, &mut graph, &mut complement)?;
        for reference in self.references.values() {
            if reference.mode == ResolveMode::Redirect {
                return Err(ResolveError::UnsupportedKind {
                    syntactic: InstanceShapeKind::Table,
                    semantic: InstanceShapeKind::Graph,
                });
            }
            let key = target_key(&self.syntactic_schema, reference)?;
            let index = table_key_index(table, key, reference.target_vertex.as_ref());
            let mut variable_targets: BTreeMap<Vec<Vec<u8>>, u32> = BTreeMap::new();
            for (holder, _) in table.rows(reference.holder.as_ref()) {
                let at = ViewRef::Row(holder);
                let occurrences =
                    self.table_reference_values(table, reference, holder, source, ctx)?;
                for (position, occurrence) in occurrences.into_iter().enumerate() {
                    let occurrence_index =
                        u32::try_from(position).map_err(|_| ResolveError::IdExhausted {
                            kind: panproto_inst::IdKind::Edge,
                        })?;
                    let tuple_key = canonical_tuple(&occurrence.key);
                    if matches!(reference.source, RefSource::Variable { .. })
                        && !variable_targets.contains_key(&tuple_key)
                    {
                        let target = if table.table_of(holder).is_some_and(|vertex| {
                            vertex.as_ref() == reference.target_vertex.as_ref()
                        }) && one_table_key(table, holder, key)
                            .is_some_and(|tuple| canonical_tuple(&tuple) == tuple_key)
                        {
                            holder.0
                        } else {
                            let candidates = index.get(&tuple_key).cloned().unwrap_or_default();
                            let [target] = candidates.as_slice() else {
                                let candidate_refs =
                                    candidates.into_iter().map(ViewRef::Row).collect::<Vec<_>>();
                                let _ = resolved_or_record(
                                    reference,
                                    at.clone(),
                                    occurrence_index,
                                    occurrence.literal.clone(),
                                    occurrence.context,
                                    candidate_refs,
                                    options,
                                    &mut complement,
                                )?;
                                continue;
                            };
                            target.0
                        };
                        variable_targets.insert(tuple_key, target);
                        complement.definitions.insert(
                            DefinitionKey {
                                reference: reference.id.clone(),
                                target,
                            },
                            at.clone(),
                        );
                        continue;
                    }
                    let candidates = if let Some(target) = occurrence
                        .span
                        .as_ref()
                        .and_then(|span| span_nodes.get(span))
                    {
                        vec![ViewRef::Row(RowId(*target))]
                    } else if matches!(reference.source, RefSource::Variable { .. }) {
                        variable_targets
                            .get(&tuple_key)
                            .copied()
                            .map(|target| ViewRef::Row(RowId(target)))
                            .into_iter()
                            .collect()
                    } else {
                        match occurrence.candidates {
                            Some(candidates) => candidates,
                            None => index
                                .get(&tuple_key)
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .map(ViewRef::Row)
                                .collect(),
                        }
                    };
                    let Some(target) = resolved_or_record(
                        reference,
                        at.clone(),
                        occurrence_index,
                        occurrence.literal.clone(),
                        occurrence.context,
                        candidates,
                        options,
                        &mut complement,
                    )?
                    else {
                        continue;
                    };
                    let target_id = carrier_node(&target)?;
                    let edge =
                        reference
                            .edge
                            .clone()
                            .ok_or_else(|| ResolveError::ReferenceConflict {
                                reference: reference.id.clone(),
                            })?;
                    let edge_id = graph
                        .add_edge(holder.0, target_id, edge)
                        .map_err(map_graph_allocation)?;
                    complement.occurrences.insert(
                        ViewRef::Edge(edge_id),
                        vec![RefOccurrence {
                            holder: at.clone(),
                            reference: reference.id.clone(),
                            index: occurrence_index,
                            literal: occurrence.literal,
                            form: occurrence.form,
                            context: occurrence.context,
                        }],
                    );
                }
            }
        }
        Ok((AnyInstance::Graph(graph), complement))
    }

    fn preallocate_table_spans(
        &self,
        table: &FInstance,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
        graph: &mut GInstance,
        complement: &mut ResolutionComplement,
    ) -> Result<BTreeMap<ResolutionSpan, u32>, ResolveError> {
        let mut pending: BTreeMap<ResolutionSpan, panproto_gat::Name> = BTreeMap::new();
        for reference in self.references.values() {
            if !matches!(reference.source, RefSource::Standoff { .. }) {
                continue;
            }
            let key = target_key(&self.syntactic_schema, reference)?;
            let index = table_key_index(table, key, reference.target_vertex.as_ref());
            for (holder, _) in table.rows(reference.holder.as_ref()) {
                for occurrence in
                    self.table_reference_values(table, reference, holder, source, ctx)?
                {
                    let Some(span) = occurrence.span else {
                        continue;
                    };
                    if index
                        .get(&canonical_tuple(&occurrence.key))
                        .is_some_and(|candidates| !candidates.is_empty())
                    {
                        continue;
                    }
                    if let Some(anchor) = pending.get(&span) {
                        if *anchor != reference.target_vertex {
                            return Err(ResolveError::ReferenceConflict {
                                reference: reference.id.clone(),
                            });
                        }
                    } else {
                        pending.insert(span, reference.target_vertex.clone());
                    }
                }
            }
        }
        let mut allocated = BTreeMap::new();
        let mut next = graph.max_node_id().map_or(Some(0), |id| id.checked_add(1));
        for (span, anchor) in pending {
            let id = next.ok_or(ResolveError::IdExhausted {
                kind: panproto_inst::IdKind::Node,
            })?;
            graph
                .insert_node(panproto_inst::Node::new(id, anchor))
                .map_err(map_graph_allocation)?;
            complement.span_nodes.insert(id, span.clone());
            allocated.insert(span, id);
            next = id.checked_add(1);
        }
        Ok(allocated)
    }

    fn tree_reference_values(
        &self,
        tree: &panproto_inst::WInstance,
        reference: &Reference,
        holder: u32,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<OccurrenceInput>, ResolveError> {
        match &reference.source {
            RefSource::KeyValue { parts } => occurrence_tuples(
                reference,
                ViewRef::Node(holder),
                parts
                    .iter()
                    .map(|path| tree_path_values(tree, holder, path))
                    .collect(),
            ),
            RefSource::Variable { field, .. } => occurrence_tuples(
                reference,
                ViewRef::Node(holder),
                vec![tree_path_values(tree, holder, field)],
            ),
            RefSource::Path { field, .. } => {
                let values = tree_path_values(tree, holder, field);
                let mut out = Vec::new();
                for literal in flatten_occurrence_values(reference, values) {
                    let resolved = self.provider.resolve_path(
                        reference,
                        ViewRef::Node(holder),
                        &literal,
                        source,
                        ctx,
                    )?;
                    out.push(OccurrenceInput {
                        key: vec![literal.clone()],
                        form: literal_text(&literal),
                        literal,
                        candidates: Some(resolved.candidates),
                        context: resolved.context,
                        span: None,
                    });
                }
                Ok(out)
            }
            RefSource::Standoff {
                text,
                start,
                end,
                unit,
                end_inclusive,
            } => standoff_tree_values(
                tree,
                reference,
                holder,
                text,
                start,
                end,
                unit,
                *end_inclusive,
                ctx,
            ),
        }
    }

    fn table_reference_values(
        &self,
        table: &FInstance,
        reference: &Reference,
        holder: RowId,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<OccurrenceInput>, ResolveError> {
        match &reference.source {
            RefSource::KeyValue { parts } => occurrence_tuples(
                reference,
                ViewRef::Row(holder),
                parts
                    .iter()
                    .map(|path| table_path_values(table, holder, path))
                    .collect(),
            ),
            RefSource::Variable { field, .. } => occurrence_tuples(
                reference,
                ViewRef::Row(holder),
                vec![table_path_values(table, holder, field)],
            ),
            RefSource::Path { field, .. } => {
                let values = table_path_values(table, holder, field);
                let mut out = Vec::new();
                for literal in flatten_occurrence_values(reference, values) {
                    let resolved = self.provider.resolve_path(
                        reference,
                        ViewRef::Row(holder),
                        &literal,
                        source,
                        ctx,
                    )?;
                    out.push(OccurrenceInput {
                        key: vec![literal.clone()],
                        form: literal_text(&literal),
                        literal,
                        candidates: Some(resolved.candidates),
                        context: resolved.context,
                        span: None,
                    });
                }
                Ok(out)
            }
            RefSource::Standoff {
                text,
                start,
                end,
                unit,
                end_inclusive,
            } => standoff_table_values(
                table,
                reference,
                holder,
                text,
                start,
                end,
                unit,
                *end_inclusive,
                ctx,
            ),
        }
    }

    fn canonical_graph_complement(
        &self,
        view: &AnyInstance,
    ) -> Result<ResolutionComplement, ResolveError> {
        let graph = view
            .as_graph()
            .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
        let mut complement = ResolutionComplement::default();

        if self.spec.kind == ResolutionKind::TreeToGraph {
            let root = self
                .syntactic_schema
                .entry_vertices()
                .iter()
                .find_map(|entry| {
                    graph
                        .nodes()
                        .iter()
                        .filter_map(|(&id, node)| (node.anchor == *entry).then_some(id))
                        .min()
                })
                .ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Node(graph.nodes().keys().copied().min().unwrap_or(0)),
                    })
                })?;

            let mut reached = std::collections::BTreeSet::from([root]);
            let mut queue = std::collections::VecDeque::from([root]);
            while let Some(parent) = queue.pop_front() {
                let mut outgoing: Vec<_> = graph
                    .out_edges(parent)
                    .iter()
                    .filter_map(|&id| graph.edge(id).map(|edge| (id, edge)))
                    .filter(|(_, edge)| self.spec.structural_edges.contains(&edge.edge))
                    .collect();
                outgoing.sort_by_key(|(id, edge)| (edge.edge.clone(), edge.tgt, id.0));
                for (id, edge) in outgoing {
                    if reached.insert(edge.tgt) {
                        complement.spanning.insert(edge.tgt, ViewRef::Edge(id));
                        complement
                            .order
                            .entry(parent)
                            .or_default()
                            .push(ViewRef::Node(edge.tgt));
                        queue.push_back(edge.tgt);
                    }
                }
            }
            let fan_nodes: std::collections::BTreeSet<u32> = graph
                .edges_sorted()
                .filter(|(_, edge)| is_fan_edge(edge))
                .map(|(_, edge)| edge.src)
                .collect();
            if let Some(unreached) = graph
                .nodes()
                .keys()
                .copied()
                .filter(|id| !fan_nodes.contains(id))
                .find(|id| !reached.contains(id))
            {
                return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Node(unreached),
                }));
            }
        }

        let spanning: std::collections::BTreeSet<EdgeId> = complement
            .spanning
            .values()
            .filter_map(|carrier| match carrier {
                ViewRef::Edge(id) => Some(*id),
                _ => None,
            })
            .collect();
        let mut next_index: BTreeMap<(u32, panproto_gat::Name), u32> = BTreeMap::new();
        for (id, edge) in graph.edges_sorted() {
            if spanning.contains(&id) || is_fan_edge(edge) {
                continue;
            }
            let reference = self.spec.reference_writers.get(&edge.edge).ok_or_else(|| {
                ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: "".into(),
                })
            })?;
            let index = next_index.entry((edge.src, reference.clone())).or_default();
            let occurrence_index = *index;
            *index = index.checked_add(1).ok_or(ResolveError::IdExhausted {
                kind: panproto_inst::IdKind::Edge,
            })?;
            complement.occurrences.insert(
                ViewRef::Edge(id),
                vec![RefOccurrence {
                    holder: match self.spec.kind {
                        ResolutionKind::TreeToGraph => ViewRef::Node(edge.src),
                        ResolutionKind::TableToGraph => ViewRef::Row(RowId(edge.src)),
                        ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                            unreachable!("assembly kinds do not construct graph complements")
                        }
                    },
                    reference: reference.clone(),
                    index: occurrence_index,
                    literal: Value::Null,
                    form: None,
                    context: None,
                }],
            );
        }
        Ok(complement)
    }

    fn put_tree_graph(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        let graph = view
            .as_graph()
            .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
        let reference_edges = self.reference_edges(graph, complement);
        let structural = stripped_graph(view, complement, reference_edges.keys().copied())?;
        let mut tree = structural
            .try_to_wtype(&self.syntactic_schema)
            .map_err(|_| ResolveError::Instance(ResolutionInstanceError::NotTree))?;
        self.restore_redirected_fragments(&mut tree, graph, complement, &reference_edges)?;
        apply_tree_order(&mut tree, &complement.order);
        self.write_tree_references(&mut tree, graph, view, complement, &reference_edges, ctx)?;
        self.verify_tree_contexts(&tree, graph, complement, ctx)?;
        Ok(AnyInstance::Tree(tree))
    }

    fn put_table_graph(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        let graph = view
            .as_graph()
            .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
        let reference_edges = self.reference_edges(graph, complement);
        let structural = stripped_graph(view, complement, reference_edges.keys().copied())?;
        let mut table = graph_to_table(&structural)?;
        self.write_table_references(&mut table, graph, view, complement, &reference_edges, ctx)?;
        self.verify_table_contexts(&table, graph, complement, ctx)?;
        Ok(AnyInstance::Table(table))
    }

    fn reference_edges(
        &self,
        graph: &GInstance,
        complement: &ResolutionComplement,
    ) -> BTreeMap<EdgeId, panproto_gat::Name> {
        let mut edges = BTreeMap::new();
        for (carrier, occurrences) in &complement.occurrences {
            let ViewRef::Edge(id) = carrier else {
                continue;
            };
            if let Some(graph_edge) = graph.edge(*id) {
                if let Some(occurrence) = occurrences.first() {
                    let edge = &graph_edge.edge;
                    let historical = self.references.get(&occurrence.reference);
                    let writer = historical
                        .filter(|reference| reference.edge.as_ref() == Some(edge))
                        .map_or_else(
                            || self.spec.reference_writers.get(edge),
                            |_| Some(&occurrence.reference),
                        );
                    if let Some(writer) = writer {
                        edges.insert(*id, writer.clone());
                    }
                }
            }
        }
        let spanning: std::collections::BTreeSet<EdgeId> = complement
            .spanning
            .values()
            .filter_map(|carrier| match carrier {
                ViewRef::Edge(id) => Some(*id),
                _ => None,
            })
            .collect();
        for (id, edge) in graph.edges_sorted() {
            if edges.contains_key(&id) || spanning.contains(&id) || is_fan_edge(edge) {
                continue;
            }
            if let Some(writer) = self.spec.reference_writers.get(&edge.edge) {
                edges.insert(id, writer.clone());
            }
        }
        edges
    }

    fn write_tree_references(
        &self,
        tree: &mut panproto_inst::WInstance,
        graph: &GInstance,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        reference_edges: &BTreeMap<EdgeId, panproto_gat::Name>,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), ResolveError> {
        let original = original_occurrences(complement);
        let mut next_indices = next_occurrence_indices(&original);
        for (id, occurrence) in &original {
            if graph.edge(*id).is_none() {
                let reference = self.reference(&occurrence.reference)?;
                if reference.mode == ResolveMode::AddEdge {
                    remove_tree_occurrence(tree, reference, &occurrence.holder, occurrence.index)?;
                }
            }
        }
        for (id, reference_name) in reference_edges {
            let edge = graph.edge(*id).ok_or_else(|| {
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Edge(*id),
                })
            })?;
            let reference = self.reference(reference_name)?;
            let occurrence = original.get(id).copied();
            let holder = occurrence.map_or(ViewRef::Node(edge.src), |item| item.holder.clone());
            let index = match occurrence {
                Some(item) => item.index,
                None => take_occurrence_index(&mut next_indices, &holder, reference_name)?,
            };
            let literal = self
                .render_tree_reference(tree, reference, &holder, edge.tgt, view, complement, ctx)?;
            set_tree_occurrence(tree, reference, &holder, index, &literal)?;
        }
        Ok(())
    }

    fn restore_redirected_fragments(
        &self,
        tree: &mut panproto_inst::WInstance,
        graph: &GInstance,
        complement: &ResolutionComplement,
        reference_edges: &BTreeMap<EdgeId, panproto_gat::Name>,
    ) -> Result<(), ResolveError> {
        let original = original_occurrences(complement);
        for (id, reference_name) in reference_edges {
            let reference = self.reference(reference_name)?;
            if reference.mode != ResolveMode::Redirect {
                continue;
            }
            let occurrence = original.get(id).copied().ok_or_else(|| {
                ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(*id),
                })
            })?;
            let root = holder_node(&occurrence.holder)?;
            let fragment = complement.redirected.get(&root).ok_or_else(|| {
                ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(*id),
                })
            })?;
            let edge = graph.edge(*id).ok_or_else(|| {
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Edge(*id),
                })
            })?;
            if fragment
                .attachment
                .as_ref()
                .is_none_or(|attachment| attachment.parent != edge.src)
            {
                return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(*id),
                }));
            }
            restore_redirected_fragment(tree, fragment)?;
        }
        Ok(())
    }

    fn verify_tree_contexts(
        &self,
        tree: &panproto_inst::WInstance,
        graph: &GInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), ResolveError> {
        let source = AnyInstance::Tree(tree.clone());
        for (carrier, occurrences) in &complement.occurrences {
            if matches!(carrier, ViewRef::Edge(id) if graph.edge(*id).is_none()) {
                continue;
            }
            for occurrence in occurrences {
                let Some(expected) = occurrence.context else {
                    continue;
                };
                let holder = holder_node(&occurrence.holder)?;
                let reference = self.reference(&occurrence.reference)?;
                let found = self
                    .tree_reference_values(tree, reference, holder, &source, ctx)?
                    .get(usize::try_from(occurrence.index).map_err(|_| {
                        ResolveError::IdExhausted {
                            kind: panproto_inst::IdKind::Edge,
                        }
                    })?)
                    .and_then(|item| item.context);
                verify_context(occurrence.holder.clone(), expected, found)?;
            }
        }
        for unresolved in &complement.unresolved {
            let Some(expected) = unresolved.context else {
                continue;
            };
            let holder = holder_node(&unresolved.holder)?;
            let reference = self.reference(&unresolved.reference)?;
            let found = self
                .tree_reference_values(tree, reference, holder, &source, ctx)?
                .get(
                    usize::try_from(unresolved.index).map_err(|_| ResolveError::IdExhausted {
                        kind: panproto_inst::IdKind::Edge,
                    })?,
                )
                .and_then(|item| item.context);
            verify_context(unresolved.holder.clone(), expected, found)?;
        }
        Ok(())
    }

    fn verify_table_contexts(
        &self,
        table: &FInstance,
        graph: &GInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), ResolveError> {
        let source = AnyInstance::Table(table.clone());
        for (carrier, occurrences) in &complement.occurrences {
            if matches!(carrier, ViewRef::Edge(id) if graph.edge(*id).is_none()) {
                continue;
            }
            for occurrence in occurrences {
                let Some(expected) = occurrence.context else {
                    continue;
                };
                let holder = holder_row(&occurrence.holder)?;
                let reference = self.reference(&occurrence.reference)?;
                let found = self
                    .table_reference_values(table, reference, holder, &source, ctx)?
                    .get(usize::try_from(occurrence.index).map_err(|_| {
                        ResolveError::IdExhausted {
                            kind: panproto_inst::IdKind::Edge,
                        }
                    })?)
                    .and_then(|item| item.context);
                verify_context(occurrence.holder.clone(), expected, found)?;
            }
        }
        for unresolved in &complement.unresolved {
            let Some(expected) = unresolved.context else {
                continue;
            };
            let holder = holder_row(&unresolved.holder)?;
            let reference = self.reference(&unresolved.reference)?;
            let found = self
                .table_reference_values(table, reference, holder, &source, ctx)?
                .get(
                    usize::try_from(unresolved.index).map_err(|_| ResolveError::IdExhausted {
                        kind: panproto_inst::IdKind::Edge,
                    })?,
                )
                .and_then(|item| item.context);
            verify_context(unresolved.holder.clone(), expected, found)?;
        }
        Ok(())
    }

    fn write_table_references(
        &self,
        table: &mut FInstance,
        graph: &GInstance,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        reference_edges: &BTreeMap<EdgeId, panproto_gat::Name>,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), ResolveError> {
        let original = original_occurrences(complement);
        let mut next_indices = next_occurrence_indices(&original);
        for (id, occurrence) in &original {
            if graph.edge(*id).is_none() {
                let reference = self.reference(&occurrence.reference)?;
                remove_table_occurrence(table, reference, &occurrence.holder, occurrence.index)?;
            }
        }
        for (id, reference_name) in reference_edges {
            let edge = graph.edge(*id).ok_or_else(|| {
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Edge(*id),
                })
            })?;
            let reference = self.reference(reference_name)?;
            let occurrence = original.get(id).copied();
            let holder =
                occurrence.map_or(ViewRef::Row(RowId(edge.src)), |item| item.holder.clone());
            let index = match occurrence {
                Some(item) => item.index,
                None => take_occurrence_index(&mut next_indices, &holder, reference_name)?,
            };
            let literal = self.render_table_reference(
                table, reference, &holder, edge.tgt, view, complement, ctx,
            )?;
            set_table_occurrence(table, reference, &holder, index, &literal)?;
        }
        Ok(())
    }

    fn reference(&self, name: &str) -> Result<&Reference, ResolveError> {
        self.references
            .get(name)
            .ok_or_else(|| ResolveError::ReferenceConflict {
                reference: name.into(),
            })
    }

    fn render_tree_reference(
        &self,
        tree: &panproto_inst::WInstance,
        reference: &Reference,
        holder: &ViewRef,
        target: u32,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<Value>, ResolveError> {
        match &reference.source {
            RefSource::KeyValue { .. } | RefSource::Variable { .. } => {
                let key = target_key(&self.syntactic_schema, reference)?;
                one_tree_key(tree, target, key).ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Node(target),
                    })
                })
            }
            RefSource::Path { .. } => self
                .provider
                .render_path(reference, holder.clone(), ViewRef::Node(target), view, ctx)
                .map(|value| vec![value]),
            RefSource::Standoff {
                unit,
                end_inclusive,
                ..
            } => {
                if let Some(span) = complement.span_nodes.get(&target) {
                    standoff_tree_offsets(tree, reference, holder, span, unit, *end_inclusive, ctx)
                } else {
                    let key = target_key(&self.syntactic_schema, reference)?;
                    one_tree_key(tree, target, key).ok_or_else(|| {
                        ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                            carrier: ViewRef::Node(target),
                        })
                    })
                }
            }
        }
    }

    fn render_table_reference(
        &self,
        table: &FInstance,
        reference: &Reference,
        holder: &ViewRef,
        target: u32,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<Value>, ResolveError> {
        match &reference.source {
            RefSource::KeyValue { .. } | RefSource::Variable { .. } => {
                let key = target_key(&self.syntactic_schema, reference)?;
                one_table_key(table, RowId(target), key).ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Row(RowId(target)),
                    })
                })
            }
            RefSource::Path { .. } => self
                .provider
                .render_path(
                    reference,
                    holder.clone(),
                    ViewRef::Row(RowId(target)),
                    view,
                    ctx,
                )
                .map(|value| vec![value]),
            RefSource::Standoff {
                unit,
                end_inclusive,
                ..
            } => {
                if let Some(span) = complement.span_nodes.get(&target) {
                    standoff_table_offsets(
                        table,
                        reference,
                        holder,
                        span,
                        unit,
                        *end_inclusive,
                        ctx,
                    )
                } else {
                    let key = target_key(&self.syntactic_schema, reference)?;
                    one_table_key(table, RowId(target), key).ok_or_else(|| {
                        ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                            carrier: ViewRef::Row(RowId(target)),
                        })
                    })
                }
            }
        }
    }
}

struct OccurrenceInput {
    key: Vec<Value>,
    literal: Value,
    form: Option<String>,
    candidates: Option<Vec<ViewRef>>,
    context: Option<Digest>,
    span: Option<ResolutionSpan>,
}

fn capture_redirected_fragment(
    tree: &panproto_inst::WInstance,
    root: u32,
) -> Result<(RedirectedFragment, Vec<usize>), ResolveError> {
    let mut node_ids = std::collections::BTreeSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if !node_ids.insert(node) {
            continue;
        }
        stack.extend(tree.children(node).iter().rev().copied());
    }
    let nodes = node_ids
        .iter()
        .map(|id| {
            tree.node(*id)
                .cloned()
                .map(|node| (*id, node))
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Node(*id),
                    })
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let mut attachment = None;
    let mut arcs = Vec::new();
    for (slot, (parent, child, edge)) in tree.arcs.iter().enumerate() {
        let source_slot = u32::try_from(slot).map_err(|_| ResolveError::IdExhausted {
            kind: panproto_inst::IdKind::Edge,
        })?;
        if *child == root && !node_ids.contains(parent) {
            attachment = Some(RedirectedAttachment {
                parent: *parent,
                child: *child,
                edge: edge.clone(),
                source_slot,
            });
        } else if node_ids.contains(parent) && node_ids.contains(child) {
            arcs.push(RedirectedArc {
                parent: *parent,
                child: *child,
                edge: edge.clone(),
                source_slot,
            });
        }
    }
    arcs.sort_by_key(|arc| arc.source_slot);

    let fan_indices: Vec<usize> = tree
        .fans
        .iter()
        .enumerate()
        .filter_map(|(index, fan)| {
            (node_ids.contains(&fan.parent)
                || fan.children.values().any(|child| node_ids.contains(child)))
            .then_some(index)
        })
        .collect();
    let mut fans: Vec<_> = fan_indices
        .iter()
        .map(|&index| tree.fans[index].clone())
        .collect();
    fans.sort_by_key(canonical_fan_key);

    Ok((
        RedirectedFragment {
            root,
            attachment,
            nodes,
            arcs,
            fans,
        },
        fan_indices,
    ))
}

fn canonical_fan_key(fan: &panproto_inst::Fan) -> (String, u32, Vec<(String, u32)>) {
    let mut children: Vec<_> = fan
        .children
        .iter()
        .map(|(label, &node)| (label.clone(), node))
        .collect();
    children.sort_unstable();
    (fan.hyper_edge_id.clone(), fan.parent, children)
}

fn remove_redirected_graph(
    graph: &mut GInstance,
    embedding: &panproto_inst::TreeEmbedding,
    fragment: &RedirectedFragment,
    fan_indices: &[usize],
) -> Result<(), ResolveError> {
    let mut allowed = std::collections::BTreeSet::new();
    for child in fragment.nodes.keys() {
        if let Some(id) = embedding.arcs.get(child) {
            allowed.insert(*id);
        }
    }
    let mut fan_nodes = Vec::new();
    for &index in fan_indices {
        let fan = embedding.fans.get(index).ok_or_else(|| {
            ResolveError::Instance(ResolutionInstanceError::DanglingCarrier {
                carrier: ViewRef::Node(fragment.root),
            })
        })?;
        fan_nodes.push(fan.node);
        allowed.insert(fan.parent_edge);
        allowed.extend(fan.child_edges.values().copied());
    }
    for node in fragment.nodes.keys() {
        for id in graph.out_edges(*node).iter().chain(graph.in_edges(*node)) {
            if !allowed.contains(id) {
                return Err(ResolveError::Instance(
                    ResolutionInstanceError::DanglingCarrier {
                        carrier: ViewRef::Edge(*id),
                    },
                ));
            }
        }
    }
    for id in allowed {
        if graph.edge(id).is_some() {
            graph.remove_edge(id).map_err(map_graph_allocation)?;
        }
    }
    for node in fan_nodes.into_iter().chain(fragment.nodes.keys().copied()) {
        if graph.node(node).is_some() {
            graph.remove_node(node).map_err(map_graph_allocation)?;
        }
    }
    Ok(())
}

fn restore_redirected_fragment(
    tree: &mut panproto_inst::WInstance,
    fragment: &RedirectedFragment,
) -> Result<(), ResolveError> {
    for (&id, node) in &fragment.nodes {
        if tree.nodes.insert(id, node.clone()).is_some() {
            return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(id),
            }));
        }
    }
    let mut removed: BTreeMap<u32, (u32, u32, Edge)> = fragment
        .arcs
        .iter()
        .map(|arc| (arc.source_slot, (arc.parent, arc.child, arc.edge.clone())))
        .collect();
    if let Some(attachment) = &fragment.attachment {
        removed.insert(
            attachment.source_slot,
            (attachment.parent, attachment.child, attachment.edge.clone()),
        );
    }
    let retained = std::mem::take(&mut tree.arcs);
    let total = retained
        .len()
        .checked_add(removed.len())
        .ok_or(ResolveError::IdExhausted {
            kind: panproto_inst::IdKind::Edge,
        })?;
    let mut retained = retained.into_iter();
    let mut arcs = Vec::with_capacity(total);
    for slot in 0..total {
        let slot = u32::try_from(slot).map_err(|_| ResolveError::IdExhausted {
            kind: panproto_inst::IdKind::Edge,
        })?;
        if let Some(arc) = removed.remove(&slot) {
            arcs.push(arc);
        } else if let Some(arc) = retained.next() {
            arcs.push(arc);
        }
    }
    if !removed.is_empty() || retained.next().is_some() {
        return Err(ResolveError::Instance(
            ResolutionInstanceError::DanglingCarrier {
                carrier: ViewRef::Node(fragment.root),
            },
        ));
    }
    tree.fans.extend(fragment.fans.iter().cloned());
    tree.fans.sort_by_key(canonical_fan_key);
    let rebuilt = panproto_inst::WInstance::new(
        std::mem::take(&mut tree.nodes),
        arcs,
        std::mem::take(&mut tree.fans),
        tree.root,
        tree.schema_root.clone(),
    );
    *tree = rebuilt;
    Ok(())
}

fn apply_tree_order(tree: &mut panproto_inst::WInstance, order: &BTreeMap<u32, Vec<ViewRef>>) {
    for (&parent, children) in order {
        let ranks: BTreeMap<u32, usize> = children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| match child {
                ViewRef::Node(id) => Some((*id, index)),
                _ => None,
            })
            .collect();
        let indices: Vec<_> = tree
            .arcs
            .iter()
            .enumerate()
            .filter_map(|(index, (source, _, _))| (*source == parent).then_some(index))
            .collect();
        let mut arcs: Vec<_> = indices
            .iter()
            .map(|&index| tree.arcs[index].clone())
            .collect();
        arcs.sort_by_key(|(_, child, edge)| {
            (
                ranks.get(child).copied().unwrap_or(usize::MAX),
                edge.clone(),
                *child,
            )
        });
        for (index, arc) in indices.into_iter().zip(arcs) {
            tree.arcs[index] = arc;
        }
    }
    let rebuilt = panproto_inst::WInstance::new(
        std::mem::take(&mut tree.nodes),
        std::mem::take(&mut tree.arcs),
        std::mem::take(&mut tree.fans),
        tree.root,
        tree.schema_root.clone(),
    );
    *tree = rebuilt;
}

fn target_key<'a>(schema: &'a Schema, reference: &Reference) -> Result<&'a Key, ResolveError> {
    let declared = match &reference.source {
        RefSource::Variable { binder_key, .. } => Some(binder_key),
        _ => reference.target_key.as_ref(),
    };
    if let Some(id) = declared {
        return schema
            .declarations
            .key(reference.target_vertex.as_ref(), id.as_ref())
            .ok_or_else(|| {
                ResolveError::Schema(panproto_schema::KeyRefError::UnknownTargetKey {
                    reference: reference.id.clone(),
                    key: id.clone(),
                })
            });
    }
    schema
        .declarations
        .keys_of(reference.target_vertex.as_ref())
        .iter()
        .find(|key| key.role == KeyRole::Primary)
        .or_else(|| {
            schema
                .declarations
                .keys_of(reference.target_vertex.as_ref())
                .iter()
                .find(|key| key.role == KeyRole::Identifier)
        })
        .or_else(|| {
            schema
                .declarations
                .keys_of(reference.target_vertex.as_ref())
                .first()
        })
        .ok_or_else(|| {
            ResolveError::Schema(panproto_schema::KeyRefError::UnknownTargetKey {
                reference: reference.id.clone(),
                key: "".into(),
            })
        })
}

fn original_occurrences(complement: &ResolutionComplement) -> BTreeMap<EdgeId, &RefOccurrence> {
    complement
        .occurrences
        .iter()
        .filter_map(|(carrier, occurrences)| match carrier {
            ViewRef::Edge(id) => occurrences.first().map(|occurrence| (*id, occurrence)),
            _ => None,
        })
        .collect()
}

fn next_occurrence_indices(
    occurrences: &BTreeMap<EdgeId, &RefOccurrence>,
) -> BTreeMap<(ViewRef, panproto_gat::Name), u32> {
    let mut next = BTreeMap::new();
    for occurrence in occurrences.values() {
        let slot = next
            .entry((occurrence.holder.clone(), occurrence.reference.clone()))
            .or_insert(0u32);
        *slot = (*slot).max(occurrence.index.saturating_add(1));
    }
    next
}

fn take_occurrence_index(
    indices: &mut BTreeMap<(ViewRef, panproto_gat::Name), u32>,
    holder: &ViewRef,
    reference: &str,
) -> Result<u32, ResolveError> {
    let index = indices
        .entry((holder.clone(), reference.into()))
        .or_insert(0);
    let current = *index;
    *index = index.checked_add(1).ok_or(ResolveError::IdExhausted {
        kind: panproto_inst::IdKind::Edge,
    })?;
    Ok(current)
}

fn reference_paths(reference: &Reference) -> Vec<&[Edge]> {
    match &reference.source {
        RefSource::KeyValue { parts } => parts.iter().map(Vec::as_slice).collect(),
        RefSource::Path { field, .. } | RefSource::Variable { field, .. } => {
            vec![field.as_slice()]
        }
        RefSource::Standoff { start, end, .. } => vec![start.as_slice(), end.as_slice()],
    }
}

fn holder_node(holder: &ViewRef) -> Result<u32, ResolveError> {
    match holder {
        ViewRef::Node(id) => Ok(*id),
        other => Err(ResolveError::Instance(
            ResolutionInstanceError::MissingCarrier {
                carrier: other.clone(),
            },
        )),
    }
}

fn holder_row(holder: &ViewRef) -> Result<RowId, ResolveError> {
    match holder {
        ViewRef::Row(id) => Ok(*id),
        ViewRef::Node(id) => Ok(RowId(*id)),
        other => Err(ResolveError::Instance(
            ResolutionInstanceError::MissingCarrier {
                carrier: other.clone(),
            },
        )),
    }
}

fn remove_tree_occurrence(
    tree: &mut panproto_inst::WInstance,
    reference: &Reference,
    holder: &ViewRef,
    index: u32,
) -> Result<(), ResolveError> {
    let holder = holder_node(holder)?;
    for path in reference_paths(reference) {
        let nodes = tree_path_nodes(tree, holder, path);
        let Some(node) = nodes.first().copied() else {
            return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(holder),
            }));
        };
        let record = tree.node(node).cloned().ok_or_else(|| {
            ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                carrier: ViewRef::Node(node),
            })
        })?;
        let updated = remove_presence(record.value, reference.cardinality.clone(), index);
        if let Some(record) = tree.nodes.get_mut(&node) {
            record.value = updated;
        }
    }
    Ok(())
}

fn set_tree_occurrence(
    tree: &mut panproto_inst::WInstance,
    reference: &Reference,
    holder: &ViewRef,
    index: u32,
    values: &[Value],
) -> Result<(), ResolveError> {
    let holder = holder_node(holder)?;
    let paths = reference_paths(reference);
    if paths.len() != values.len() {
        return Err(ResolveError::ReferenceCardinalityMismatch {
            reference: reference.id.clone(),
            at: ViewRef::Node(holder),
            lengths: vec![
                u64::try_from(paths.len()).unwrap_or(u64::MAX),
                u64::try_from(values.len()).unwrap_or(u64::MAX),
            ],
        });
    }
    for (path, value) in paths.into_iter().zip(values) {
        let nodes = tree_path_nodes(tree, holder, path);
        let Some(node) = nodes.first().copied() else {
            return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(holder),
            }));
        };
        let previous = tree.node(node).and_then(|record| record.value.clone());
        let updated = set_presence(
            previous,
            reference.cardinality.clone(),
            index,
            value.clone(),
        );
        if let Some(record) = tree.nodes.get_mut(&node) {
            record.value = Some(updated);
        }
    }
    Ok(())
}

fn remove_table_occurrence(
    table: &mut FInstance,
    reference: &Reference,
    holder: &ViewRef,
    index: u32,
) -> Result<(), ResolveError> {
    let row = holder_row(holder)?;
    for path in reference_paths(reference) {
        let column = table_column(path).ok_or_else(|| {
            ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Row(row),
            })
        })?;
        let record = table.row_mut(row).ok_or_else(|| {
            ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                carrier: ViewRef::Row(row),
            })
        })?;
        let previous = record.remove(column);
        if let Some(value) = remove_value(previous, reference.cardinality.clone(), index) {
            record.insert(column.to_owned(), value);
        }
    }
    Ok(())
}

fn set_table_occurrence(
    table: &mut FInstance,
    reference: &Reference,
    holder: &ViewRef,
    index: u32,
    values: &[Value],
) -> Result<(), ResolveError> {
    let row = holder_row(holder)?;
    let paths = reference_paths(reference);
    if paths.len() != values.len() {
        return Err(ResolveError::ReferenceCardinalityMismatch {
            reference: reference.id.clone(),
            at: ViewRef::Row(row),
            lengths: vec![
                u64::try_from(paths.len()).unwrap_or(u64::MAX),
                u64::try_from(values.len()).unwrap_or(u64::MAX),
            ],
        });
    }
    for (path, value) in paths.into_iter().zip(values) {
        let column = table_column(path).ok_or_else(|| {
            ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Row(row),
            })
        })?;
        let record = table.row_mut(row).ok_or_else(|| {
            ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                carrier: ViewRef::Row(row),
            })
        })?;
        let previous = record.remove(column);
        let updated = set_value(
            previous,
            reference.cardinality.clone(),
            index,
            value.clone(),
        );
        record.insert(column.to_owned(), updated);
    }
    Ok(())
}

fn remove_presence(
    previous: Option<FieldPresence>,
    cardinality: Cardinality,
    index: u32,
) -> Option<FieldPresence> {
    match previous {
        Some(FieldPresence::Present(value)) => {
            remove_value(Some(value), cardinality, index).map(FieldPresence::Present)
        }
        _ => Some(FieldPresence::Absent),
    }
}

fn set_presence(
    previous: Option<FieldPresence>,
    cardinality: Cardinality,
    index: u32,
    value: Value,
) -> FieldPresence {
    let previous = previous.and_then(|presence| match presence {
        FieldPresence::Present(value) => Some(value),
        FieldPresence::Absent | FieldPresence::Null => None,
    });
    FieldPresence::Present(set_value(previous, cardinality, index, value))
}

fn remove_value(previous: Option<Value>, cardinality: Cardinality, index: u32) -> Option<Value> {
    match cardinality {
        Cardinality::One => None,
        Cardinality::Many => {
            let mut values = previous.map_or_else(Vec::new, |value| match value {
                Value::List(values) => values,
                other => vec![other],
            });
            if let Ok(index) = usize::try_from(index) {
                if index < values.len() {
                    values.remove(index);
                }
            }
            Some(Value::List(values))
        }
    }
}

fn set_value(previous: Option<Value>, cardinality: Cardinality, index: u32, value: Value) -> Value {
    match cardinality {
        Cardinality::One => value,
        Cardinality::Many => {
            let mut values = previous.map_or_else(Vec::new, |previous| match previous {
                Value::List(values) => values,
                other => vec![other],
            });
            let index = usize::try_from(index).unwrap_or(values.len());
            if index < values.len() {
                values[index] = value;
            } else {
                values.push(value);
            }
            Value::List(values)
        }
    }
}

fn canonical_tuple(values: &[Value]) -> Vec<Vec<u8>> {
    values.iter().map(Value::canonical_bytes).collect()
}

fn tree_path_nodes(tree: &panproto_inst::WInstance, start: u32, path: &[Edge]) -> Vec<u32> {
    let mut current = vec![start];
    for wanted in path {
        let mut next = Vec::new();
        for parent in current {
            for (arc_parent, child, edge) in &tree.arcs {
                if *arc_parent == parent && edge == wanted {
                    next.push(*child);
                }
            }
        }
        current = next;
    }
    current
}

fn present_value(value: &Option<FieldPresence>) -> Option<Value> {
    match value {
        Some(FieldPresence::Present(value)) => Some(value.clone()),
        Some(FieldPresence::Null) => Some(Value::Null),
        Some(FieldPresence::Absent) | None => None,
    }
}

fn tree_path_values(tree: &panproto_inst::WInstance, start: u32, path: &[Edge]) -> Vec<Value> {
    tree_path_nodes(tree, start, path)
        .into_iter()
        .filter_map(|id| tree.node(id).and_then(|node| present_value(&node.value)))
        .collect()
}

fn table_column(path: &[Edge]) -> Option<&str> {
    path.last()
        .map(|edge| edge.name.as_ref().unwrap_or(&edge.tgt).as_ref())
}

fn table_path_values(table: &FInstance, row: RowId, path: &[Edge]) -> Vec<Value> {
    let Some(column) = table_column(path) else {
        return Vec::new();
    };
    table
        .row(row)
        .and_then(|row| row.get(column))
        .cloned()
        .into_iter()
        .collect()
}

fn one_tree_key(tree: &panproto_inst::WInstance, node: u32, key: &Key) -> Option<Vec<Value>> {
    let mut tuple = Vec::with_capacity(key.parts.len());
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => {
                let values = tree_path_values(tree, node, path);
                let [value] = values.as_slice() else {
                    return None;
                };
                tuple.push(value.clone());
            }
            KeyPart::Position { within } => {
                let parent = tree.parent(node)?;
                let position = tree
                    .arcs
                    .iter()
                    .filter(|(p, _, edge)| *p == parent && edge == within)
                    .position(|(_, child, _)| *child == node)?;
                tuple.push(Value::Int(i64::try_from(position).ok()?));
            }
        }
    }
    Some(tuple)
}

fn one_table_key(table: &FInstance, row: RowId, key: &Key) -> Option<Vec<Value>> {
    let mut tuple = Vec::with_capacity(key.parts.len());
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => {
                let values = table_path_values(table, row, path);
                let [value] = values.as_slice() else {
                    return None;
                };
                tuple.push(value.clone());
            }
            KeyPart::Position { .. } => tuple.push(Value::Int(i64::from(row.0))),
        }
    }
    Some(tuple)
}

fn tree_scope(tree: &panproto_inst::WInstance, holder: u32, scope: &KeyScope) -> Option<u32> {
    let KeyScope::Within(vertex) = scope else {
        return None;
    };
    let mut current = tree.parent(holder);
    while let Some(node) = current {
        if tree
            .node(node)
            .is_some_and(|record| record.anchor == *vertex)
        {
            return Some(node);
        }
        current = tree.parent(node);
    }
    None
}

fn tree_document_order(tree: &panproto_inst::WInstance) -> Vec<u32> {
    let mut order = Vec::with_capacity(tree.nodes.len());
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![tree.root];
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        order.push(node);
        if let Some(children) = tree.children_map.get(&node) {
            stack.extend(children.iter().rev().copied());
        }
    }
    let mut remaining: Vec<_> = tree
        .nodes
        .keys()
        .copied()
        .filter(|id| !seen.contains(id))
        .collect();
    remaining.sort_unstable();
    order.extend(remaining);
    order
}

type TreeKeyIndex = BTreeMap<(Option<u32>, Vec<Vec<u8>>), Vec<u32>>;

fn tree_key_index(tree: &panproto_inst::WInstance, key: &Key, target_vertex: &str) -> TreeKeyIndex {
    let mut index = BTreeMap::new();
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let Some(node) = tree.node(id) else {
            continue;
        };
        if node.anchor != target_vertex {
            continue;
        }
        if let Some(tuple) = one_tree_key(tree, id, key) {
            index
                .entry((tree_scope(tree, id, &key.scope), canonical_tuple(&tuple)))
                .or_insert_with(Vec::new)
                .push(id);
        }
    }
    index
}

fn table_key_index(
    table: &FInstance,
    key: &Key,
    target_vertex: &str,
) -> BTreeMap<Vec<Vec<u8>>, Vec<RowId>> {
    let mut index = BTreeMap::new();
    for (vertex, rows) in table.tables() {
        if vertex != target_vertex {
            continue;
        }
        for (row, _) in rows.rows() {
            if let Some(tuple) = one_table_key(table, row, key) {
                index
                    .entry(canonical_tuple(&tuple))
                    .or_insert_with(Vec::new)
                    .push(row);
            }
        }
    }
    index
}

fn part_sequence(values: Vec<Value>) -> Vec<Value> {
    values
        .into_iter()
        .flat_map(|value| match value {
            Value::List(items) => items,
            other => vec![other],
        })
        .collect()
}

fn occurrence_tuples(
    reference: &Reference,
    at: ViewRef,
    parts: Vec<Vec<Value>>,
) -> Result<Vec<OccurrenceInput>, ResolveError> {
    match reference.cardinality {
        Cardinality::One => {
            let lengths: Vec<u64> = parts
                .iter()
                .map(|part| u64::try_from(part.len()).unwrap_or(u64::MAX))
                .collect();
            if lengths.iter().all(|length| *length == 0) && reference.nullable {
                return Ok(Vec::new());
            }
            if lengths.iter().any(|length| *length != 1) {
                return Err(ResolveError::ReferenceCardinalityMismatch {
                    reference: reference.id.clone(),
                    at,
                    lengths,
                });
            }
            let tuple: Vec<Value> = parts.into_iter().flatten().collect();
            let literal = tuple_literal(&tuple);
            Ok(vec![OccurrenceInput {
                key: tuple,
                form: literal_text(&literal),
                literal,
                candidates: None,
                context: None,
                span: None,
            }])
        }
        Cardinality::Many => {
            let sequences: Vec<Vec<Value>> = parts.into_iter().map(part_sequence).collect();
            let lengths: Vec<u64> = sequences
                .iter()
                .map(|part| u64::try_from(part.len()).unwrap_or(u64::MAX))
                .collect();
            let length = lengths.first().copied().unwrap_or(0);
            if lengths.iter().any(|other| *other != length) || (length == 0 && !reference.nullable)
            {
                return Err(ResolveError::ReferenceCardinalityMismatch {
                    reference: reference.id.clone(),
                    at,
                    lengths,
                });
            }
            let length = usize::try_from(length).map_err(|_| {
                ResolveError::ReferenceCardinalityMismatch {
                    reference: reference.id.clone(),
                    at: at.clone(),
                    lengths: lengths.clone(),
                }
            })?;
            let mut out = Vec::with_capacity(length);
            for index in 0..length {
                let tuple: Vec<Value> = sequences.iter().map(|part| part[index].clone()).collect();
                let literal = tuple_literal(&tuple);
                out.push(OccurrenceInput {
                    key: tuple,
                    form: literal_text(&literal),
                    literal,
                    candidates: None,
                    context: None,
                    span: None,
                });
            }
            Ok(out)
        }
    }
}

fn flatten_occurrence_values(reference: &Reference, values: Vec<Value>) -> Vec<Value> {
    match reference.cardinality {
        Cardinality::One => values.into_iter().take(1).collect(),
        Cardinality::Many => part_sequence(values),
    }
}

fn tuple_literal(tuple: &[Value]) -> Value {
    match tuple {
        [value] => value.clone(),
        values => Value::List(values.to_vec()),
    }
}

fn literal_text(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) | Value::Token(text) => Some(text.clone()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn standoff_tree_values(
    tree: &panproto_inst::WInstance,
    reference: &Reference,
    holder: u32,
    text_source: &RefTextSource,
    start_path: &[Edge],
    end_path: &[Edge],
    unit: &OffsetUnit,
    end_inclusive: bool,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<OccurrenceInput>, ResolveError> {
    let (text_ref, bytes, mut context) = match text_source {
        RefTextSource::Local(path) => {
            let node = tree_path_nodes(tree, holder, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Node(holder),
                    })
                })?;
            let value = tree
                .node(node)
                .and_then(|record| present_value(&record.value))
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Node(node),
                    })
                })?;
            let bytes = value_bytes(&value);
            (ViewRef::Node(node), bytes, None)
        }
        RefTextSource::External(path) => {
            let key = tree_path_values(tree, holder, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Node(holder),
                    })
                })?;
            let store = ctx
                .external_text
                .ok_or_else(|| ResolveError::UnsupportedFeature {
                    provider: None,
                    feature: ResolutionFeature::ExternalText,
                })?;
            let bytes = store.load(&key)?;
            let mut material = key.canonical_bytes();
            material.extend_from_slice(&bytes);
            (
                ViewRef::Node(holder),
                bytes,
                Some(Digest::derive(
                    "panproto 2026-09 resolution external text v1",
                    &material,
                )),
            )
        }
    };
    let starts = tree_path_values(tree, holder, start_path);
    let ends = tree_path_values(tree, holder, end_path);
    let raw_start = unsigned_offset(starts.first(), reference, holder)?;
    let raw_end = unsigned_offset(ends.first(), reference, holder)?;
    let (start, mut end, unit_context) = offsets_to_bytes(
        &bytes,
        text_ref.clone(),
        raw_start,
        raw_end,
        unit,
        ctx,
        &reference.id,
        ViewRef::Node(holder),
    )?;
    if end_inclusive {
        end = inclusive_end_to_exclusive(
            &bytes,
            text_ref.clone(),
            end,
            raw_end,
            unit,
            ctx,
            &reference.id,
            ViewRef::Node(holder),
        )
        .map_err(|_| ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at: ViewRef::Node(holder),
            start,
            end,
            len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        })?;
    }
    if start > end || end > u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
        return Err(ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at: ViewRef::Node(holder),
            start,
            end,
            len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        });
    }
    if let Some(unit_context) = unit_context {
        context = Some(match context {
            None => unit_context,
            Some(external) => {
                let mut bytes = external.0.to_vec();
                bytes.extend_from_slice(&unit_context.0);
                Digest::derive("panproto 2026-09 resolution context v1", &bytes)
            }
        });
    }
    let start_value = i64::try_from(start).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at: ViewRef::Node(holder),
        start,
        end,
        len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    })?;
    let end_value = i64::try_from(end).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at: ViewRef::Node(holder),
        start,
        end,
        len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    })?;
    let tuple = vec![Value::Int(start_value), Value::Int(end_value)];
    Ok(vec![OccurrenceInput {
        key: tuple.clone(),
        literal: Value::List(tuple),
        form: None,
        candidates: None,
        context,
        span: Some(ResolutionSpan {
            text: text_ref,
            start,
            end,
        }),
    }])
}

#[allow(clippy::too_many_arguments)]
fn standoff_table_values(
    table: &FInstance,
    reference: &Reference,
    holder: RowId,
    text_source: &RefTextSource,
    start_path: &[Edge],
    end_path: &[Edge],
    unit: &OffsetUnit,
    end_inclusive: bool,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<OccurrenceInput>, ResolveError> {
    let (bytes, mut context) = match text_source {
        RefTextSource::Local(path) => {
            let value = table_path_values(table, holder, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Row(holder),
                    })
                })?;
            (value_bytes(&value), None)
        }
        RefTextSource::External(path) => {
            let key = table_path_values(table, holder, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Row(holder),
                    })
                })?;
            let store = ctx
                .external_text
                .ok_or_else(|| ResolveError::UnsupportedFeature {
                    provider: None,
                    feature: ResolutionFeature::ExternalText,
                })?;
            let bytes = store.load(&key)?;
            let mut material = key.canonical_bytes();
            material.extend_from_slice(&bytes);
            (
                bytes,
                Some(Digest::derive(
                    "panproto 2026-09 resolution external text v1",
                    &material,
                )),
            )
        }
    };
    let starts = table_path_values(table, holder, start_path);
    let ends = table_path_values(table, holder, end_path);
    let raw_start = unsigned_offset_at(starts.first(), reference, ViewRef::Row(holder))?;
    let raw_end = unsigned_offset_at(ends.first(), reference, ViewRef::Row(holder))?;
    let text_ref = ViewRef::Row(holder);
    let (start, mut end, unit_context) = offsets_to_bytes(
        &bytes,
        text_ref.clone(),
        raw_start,
        raw_end,
        unit,
        ctx,
        &reference.id,
        ViewRef::Row(holder),
    )?;
    if end_inclusive {
        end = inclusive_end_to_exclusive(
            &bytes,
            text_ref.clone(),
            end,
            raw_end,
            unit,
            ctx,
            &reference.id,
            ViewRef::Row(holder),
        )
        .map_err(|_| ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at: ViewRef::Row(holder),
            start,
            end,
            len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        })?;
    }
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if start > end || end > len {
        return Err(ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at: ViewRef::Row(holder),
            start,
            end,
            len,
        });
    }
    if let Some(unit_context) = unit_context {
        context = Some(match context {
            None => unit_context,
            Some(external) => {
                let mut material = external.0.to_vec();
                material.extend_from_slice(&unit_context.0);
                Digest::derive("panproto 2026-09 resolution context v1", &material)
            }
        });
    }
    let start_value = i64::try_from(start).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at: ViewRef::Row(holder),
        start,
        end,
        len,
    })?;
    let end_value = i64::try_from(end).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at: ViewRef::Row(holder),
        start,
        end,
        len,
    })?;
    let tuple = vec![Value::Int(start_value), Value::Int(end_value)];
    Ok(vec![OccurrenceInput {
        key: tuple.clone(),
        literal: Value::List(tuple),
        form: None,
        candidates: None,
        context,
        span: Some(ResolutionSpan {
            text: text_ref,
            start,
            end,
        }),
    }])
}

fn standoff_tree_offsets(
    tree: &panproto_inst::WInstance,
    reference: &Reference,
    holder: &ViewRef,
    span: &ResolutionSpan,
    unit: &OffsetUnit,
    end_inclusive: bool,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<Value>, ResolveError> {
    let holder_id = holder_node(holder)?;
    let RefSource::Standoff { text, .. } = &reference.source else {
        return Err(ResolveError::ReferenceConflict {
            reference: reference.id.clone(),
        });
    };
    let (text_ref, bytes) = match text {
        RefTextSource::Local(path) => {
            let node = tree_path_nodes(tree, holder_id, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: holder.clone(),
                    })
                })?;
            let value = tree
                .node(node)
                .and_then(|record| present_value(&record.value))
                .ok_or_else(|| {
                    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                        carrier: ViewRef::Node(node),
                    })
                })?;
            (ViewRef::Node(node), value_bytes(&value))
        }
        RefTextSource::External(path) => {
            let key = tree_path_values(tree, holder_id, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: holder.clone(),
                    })
                })?;
            let bytes = ctx
                .external_text
                .ok_or_else(|| ResolveError::UnsupportedFeature {
                    provider: None,
                    feature: ResolutionFeature::ExternalText,
                })?
                .load(&key)?;
            (ViewRef::Node(holder_id), bytes)
        }
    };
    if span.text != text_ref {
        return Err(ResolveError::PutDomain(
            ResolveDomainError::UnrepresentableReference {
                carrier: holder.clone(),
                reference: reference.id.clone(),
            },
        ));
    }
    standoff_offsets_for_bytes(
        reference,
        holder.clone(),
        span,
        &bytes,
        unit,
        end_inclusive,
        ctx,
    )
}

fn standoff_table_offsets(
    table: &FInstance,
    reference: &Reference,
    holder: &ViewRef,
    span: &ResolutionSpan,
    unit: &OffsetUnit,
    end_inclusive: bool,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<Value>, ResolveError> {
    let holder_id = holder_row(holder)?;
    let RefSource::Standoff { text, .. } = &reference.source else {
        return Err(ResolveError::ReferenceConflict {
            reference: reference.id.clone(),
        });
    };
    let bytes = match text {
        RefTextSource::Local(path) => table_path_values(table, holder_id, path)
            .into_iter()
            .next()
            .map(|value| value_bytes(&value))
            .ok_or_else(|| {
                ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                    carrier: holder.clone(),
                })
            })?,
        RefTextSource::External(path) => {
            let key = table_path_values(table, holder_id, path)
                .into_iter()
                .next()
                .ok_or_else(|| {
                    ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                        carrier: holder.clone(),
                    })
                })?;
            ctx.external_text
                .ok_or_else(|| ResolveError::UnsupportedFeature {
                    provider: None,
                    feature: ResolutionFeature::ExternalText,
                })?
                .load(&key)?
        }
    };
    if span.text != ViewRef::Row(holder_id) {
        return Err(ResolveError::PutDomain(
            ResolveDomainError::UnrepresentableReference {
                carrier: holder.clone(),
                reference: reference.id.clone(),
            },
        ));
    }
    standoff_offsets_for_bytes(
        reference,
        holder.clone(),
        span,
        &bytes,
        unit,
        end_inclusive,
        ctx,
    )
}

fn standoff_offsets_for_bytes(
    reference: &Reference,
    at: ViewRef,
    span: &ResolutionSpan,
    bytes: &[u8],
    unit: &OffsetUnit,
    end_inclusive: bool,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<Value>, ResolveError> {
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if span.start > span.end || span.end > len {
        return Err(ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at,
            start: span.start,
            end: span.end,
            len,
        });
    }
    let start = byte_boundary_to_unit(bytes, span.text.clone(), span.start, unit, ctx)?;
    let end = if end_inclusive {
        previous_unit_start(bytes, span.text.clone(), span.end, unit, ctx)?
    } else {
        byte_boundary_to_unit(bytes, span.text.clone(), span.end, unit, ctx)?
    };
    let start = i64::try_from(start).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at: at.clone(),
        start: span.start,
        end: span.end,
        len,
    })?;
    let end = i64::try_from(end).map_err(|_| ResolveError::OffsetOutOfRange {
        reference: reference.id.clone(),
        at,
        start: span.start,
        end: span.end,
        len,
    })?;
    Ok(vec![Value::Int(start), Value::Int(end)])
}

fn byte_boundary_to_unit(
    utf8: &[u8],
    text: ViewRef,
    boundary: u64,
    unit: &OffsetUnit,
    ctx: &ResolveContext<'_>,
) -> Result<u64, ResolveError> {
    let boundary = usize::try_from(boundary).map_err(|_| offset_boundary_error(boundary))?;
    let prefix = utf8
        .get(..boundary)
        .ok_or_else(|| offset_boundary_error(u64::try_from(boundary).unwrap_or(u64::MAX)))?;
    match unit {
        OffsetUnit::Byte => u64::try_from(boundary).map_err(|_| offset_boundary_error(u64::MAX)),
        OffsetUnit::Utf8Char => {
            let prefix =
                std::str::from_utf8(prefix).map_err(|_| ResolveError::ProviderFailure {
                    provider: ResolutionProviderId {
                        name: "utf8".into(),
                        version: 1,
                    },
                    code: "invalid_utf8_boundary".into(),
                    at: Some(text),
                })?;
            u64::try_from(prefix.chars().count()).map_err(|_| offset_boundary_error(u64::MAX))
        }
        OffsetUnit::Utf16 => {
            let prefix =
                std::str::from_utf8(prefix).map_err(|_| ResolveError::ProviderFailure {
                    provider: ResolutionProviderId {
                        name: "utf16".into(),
                        version: 1,
                    },
                    code: "invalid_utf8_boundary".into(),
                    at: Some(text),
                })?;
            u64::try_from(prefix.encode_utf16().count())
                .map_err(|_| offset_boundary_error(u64::MAX))
        }
        OffsetUnit::Token => {
            let provider =
                ctx.token_boundaries
                    .ok_or_else(|| ResolveError::UnsupportedFeature {
                        provider: None,
                        feature: ResolutionFeature::TokenBoundaries,
                    })?;
            let boundaries = provider.boundaries(text.clone(), utf8)?;
            validate_boundaries(&boundaries, utf8.len(), text)?;
            boundaries
                .binary_search(&u64::try_from(boundary).unwrap_or(u64::MAX))
                .ok()
                .and_then(|index| u64::try_from(index).ok())
                .ok_or_else(|| offset_boundary_error(u64::try_from(boundary).unwrap_or(u64::MAX)))
        }
    }
}

fn previous_unit_start(
    utf8: &[u8],
    text: ViewRef,
    exclusive: u64,
    unit: &OffsetUnit,
    ctx: &ResolveContext<'_>,
) -> Result<u64, ResolveError> {
    let exclusive_index =
        usize::try_from(exclusive).map_err(|_| offset_boundary_error(exclusive))?;
    let prefix = utf8
        .get(..exclusive_index)
        .ok_or_else(|| offset_boundary_error(exclusive))?;
    match unit {
        OffsetUnit::Byte => exclusive
            .checked_sub(1)
            .ok_or_else(|| offset_boundary_error(exclusive)),
        OffsetUnit::Utf8Char | OffsetUnit::Utf16 => {
            let text_value =
                std::str::from_utf8(prefix).map_err(|_| ResolveError::ProviderFailure {
                    provider: ResolutionProviderId {
                        name: "utf8".into(),
                        version: 1,
                    },
                    code: "invalid_utf8_boundary".into(),
                    at: Some(text.clone()),
                })?;
            let last = text_value
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .ok_or_else(|| offset_boundary_error(exclusive))?;
            byte_boundary_to_unit(
                utf8,
                text,
                u64::try_from(last).unwrap_or(u64::MAX),
                unit,
                ctx,
            )
        }
        OffsetUnit::Token => byte_boundary_to_unit(utf8, text, exclusive, unit, ctx)?
            .checked_sub(1)
            .ok_or_else(|| offset_boundary_error(exclusive)),
    }
}

fn offset_boundary_error(offset: u64) -> ResolveError {
    ResolveError::OffsetOutOfRange {
        reference: "offset".into(),
        at: ViewRef::Node(0),
        start: offset,
        end: offset,
        len: 0,
    }
}

fn value_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Str(text) | Value::Token(text) => text.as_bytes().to_vec(),
        Value::Bytes(bytes) => bytes.clone(),
        _ => value.canonical_bytes(),
    }
}

fn unsigned_offset(
    value: Option<&Value>,
    reference: &Reference,
    holder: u32,
) -> Result<u64, ResolveError> {
    unsigned_offset_at(value, reference, ViewRef::Node(holder))
}

fn unsigned_offset_at(
    value: Option<&Value>,
    reference: &Reference,
    at: ViewRef,
) -> Result<u64, ResolveError> {
    match value {
        Some(Value::Int(value)) if *value >= 0 => {
            u64::try_from(*value).map_err(|_| ResolveError::OffsetOutOfRange {
                reference: reference.id.clone(),
                at: at.clone(),
                start: 0,
                end: 0,
                len: 0,
            })
        }
        _ => Err(ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at,
            start: 0,
            end: 0,
            len: 0,
        }),
    }
}

fn offsets_to_bytes(
    utf8: &[u8],
    text: ViewRef,
    start: u64,
    end: u64,
    unit: &OffsetUnit,
    ctx: &ResolveContext<'_>,
    reference: &panproto_gat::Name,
    at: ViewRef,
) -> Result<(u64, u64, Option<Digest>), ResolveError> {
    match unit {
        OffsetUnit::Byte => Ok((start, end, None)),
        OffsetUnit::Utf8Char => {
            let text_value =
                std::str::from_utf8(utf8).map_err(|_| ResolveError::ProviderFailure {
                    provider: ResolutionProviderId {
                        name: "utf8".into(),
                        version: 1,
                    },
                    code: "invalid_utf8".into(),
                    at: Some(text.clone()),
                })?;
            let boundaries: Vec<u64> = text_value
                .char_indices()
                .map(|(offset, _)| u64::try_from(offset).unwrap_or(u64::MAX))
                .chain(std::iter::once(
                    u64::try_from(utf8.len()).unwrap_or(u64::MAX),
                ))
                .collect();
            Ok((
                indexed_boundary(&boundaries, start, reference, at.clone())?,
                indexed_boundary(&boundaries, end, reference, at.clone())?,
                None,
            ))
        }
        OffsetUnit::Utf16 => {
            let text_value =
                std::str::from_utf8(utf8).map_err(|_| ResolveError::ProviderFailure {
                    provider: ResolutionProviderId {
                        name: "utf16".into(),
                        version: 1,
                    },
                    code: "invalid_utf8".into(),
                    at: Some(text.clone()),
                })?;
            let mut boundaries = vec![0u64];
            let mut units = 0usize;
            for (byte, character) in text_value.char_indices() {
                let next = byte + character.len_utf8();
                units += character.len_utf16();
                if boundaries.len() <= units {
                    boundaries.resize(units + 1, u64::MAX);
                }
                boundaries[units] = u64::try_from(next).unwrap_or(u64::MAX);
            }
            Ok((
                indexed_boundary(&boundaries, start, reference, at.clone())?,
                indexed_boundary(&boundaries, end, reference, at.clone())?,
                None,
            ))
        }
        OffsetUnit::Token => {
            let provider =
                ctx.token_boundaries
                    .ok_or_else(|| ResolveError::UnsupportedFeature {
                        provider: None,
                        feature: ResolutionFeature::TokenBoundaries,
                    })?;
            let boundaries = provider.boundaries(text.clone(), utf8)?;
            validate_boundaries(&boundaries, utf8.len(), text)?;
            let mut material = Vec::with_capacity(boundaries.len() * 8);
            for boundary in &boundaries {
                material.extend_from_slice(&boundary.to_le_bytes());
            }
            let digest =
                Digest::derive("panproto 2026-09 resolution token boundaries v1", &material);
            Ok((
                indexed_boundary(&boundaries, start, reference, at.clone())?,
                indexed_boundary(&boundaries, end, reference, at)?,
                Some(digest),
            ))
        }
    }
}

fn indexed_boundary(
    boundaries: &[u64],
    index: u64,
    reference: &panproto_gat::Name,
    at: ViewRef,
) -> Result<u64, ResolveError> {
    usize::try_from(index)
        .ok()
        .and_then(|index| boundaries.get(index).copied())
        .filter(|boundary| *boundary != u64::MAX)
        .ok_or_else(|| ResolveError::OffsetOutOfRange {
            reference: reference.clone(),
            at,
            start: index,
            end: index,
            len: u64::try_from(boundaries.len().saturating_sub(1)).unwrap_or(u64::MAX),
        })
}

fn validate_boundaries(boundaries: &[u64], len: usize, text: ViewRef) -> Result<(), ResolveError> {
    let valid = boundaries.first() == Some(&0)
        && boundaries.last() == Some(&u64::try_from(len).unwrap_or(u64::MAX))
        && boundaries.windows(2).all(|window| window[0] < window[1]);
    if valid {
        Ok(())
    } else {
        Err(ResolveError::ProviderFailure {
            provider: ResolutionProviderId {
                name: "token_boundaries".into(),
                version: 1,
            },
            code: "invalid_boundaries".into(),
            at: Some(text),
        })
    }
}

fn next_boundary(utf8: &[u8], offset: u64, unit: &OffsetUnit) -> Option<u64> {
    match unit {
        OffsetUnit::Byte => offset.checked_add(1),
        OffsetUnit::Utf8Char | OffsetUnit::Utf16 | OffsetUnit::Token => {
            let start = usize::try_from(offset).ok()?;
            let text = std::str::from_utf8(utf8.get(start..)?).ok()?;
            text.chars()
                .next()
                .and_then(|character| offset.checked_add(character.len_utf8() as u64))
        }
    }
}

fn inclusive_end_to_exclusive(
    utf8: &[u8],
    text: ViewRef,
    byte_end: u64,
    raw_end: u64,
    unit: &OffsetUnit,
    ctx: &ResolveContext<'_>,
    reference: &panproto_gat::Name,
    at: ViewRef,
) -> Result<u64, ResolveError> {
    if *unit == OffsetUnit::Token {
        let next = raw_end
            .checked_add(1)
            .ok_or_else(|| offset_boundary_error(raw_end))?;
        return offsets_to_bytes(utf8, text, next, next, unit, ctx, reference, at)
            .map(|(_, end, _)| end);
    }
    next_boundary(utf8, byte_end, unit).ok_or_else(|| offset_boundary_error(byte_end))
}

fn resolved_or_record(
    reference: &Reference,
    at: ViewRef,
    index: u32,
    literal: Value,
    context: Option<Digest>,
    mut candidates: Vec<ViewRef>,
    options: &ResolveOptions,
    complement: &mut ResolutionComplement,
) -> Result<Option<ViewRef>, ResolveError> {
    candidates.sort_unstable();
    match candidates.as_slice() {
        [target] => Ok(Some(target.clone())),
        [] => {
            if reference.on_unresolved == OnUnresolved::Error && !options.lenient {
                return Err(ResolveError::DanglingReference {
                    reference: reference.id.clone(),
                    at,
                    key: literal_tuple(&literal),
                });
            }
            complement.unresolved.push(UnresolvedRef {
                holder: at,
                reference: reference.id.clone(),
                index,
                literal,
                reason: ResolutionUnresolvedReason::Dangling,
                context,
            });
            Ok(None)
        }
        _ => {
            if reference.on_unresolved == OnUnresolved::Error && !options.lenient {
                return Err(ResolveError::AmbiguousReference {
                    reference: reference.id.clone(),
                    at,
                    key: literal_tuple(&literal),
                    candidates,
                });
            }
            complement.unresolved.push(UnresolvedRef {
                holder: at,
                reference: reference.id.clone(),
                index,
                literal,
                reason: ResolutionUnresolvedReason::Ambiguous { candidates },
                context,
            });
            Ok(None)
        }
    }
}

fn literal_tuple(literal: &Value) -> Vec<Value> {
    match literal {
        Value::List(values) => values.clone(),
        value => vec![value.clone()],
    }
}

fn verify_context(
    at: ViewRef,
    expected: Digest,
    found: Option<Digest>,
) -> Result<(), ResolveError> {
    if found == Some(expected) {
        Ok(())
    } else {
        Err(ResolveError::ContextMismatch {
            at: Some(at),
            expected,
            found,
        })
    }
}

fn carrier_node(carrier: &ViewRef) -> Result<u32, ResolveError> {
    match carrier {
        ViewRef::Node(id) => Ok(*id),
        ViewRef::Row(id) => Ok(id.0),
        other => Err(ResolveError::Instance(
            ResolutionInstanceError::DanglingCarrier {
                carrier: other.clone(),
            },
        )),
    }
}

fn map_graph_allocation(error: InstError) -> ResolveError {
    match error {
        InstError::NodeIdsExhausted => ResolveError::IdExhausted {
            kind: panproto_inst::IdKind::Node,
        },
        InstError::EdgeIdsExhausted => ResolveError::IdExhausted {
            kind: panproto_inst::IdKind::Edge,
        },
        _ => ResolveError::Instance(ResolutionInstanceError::NotGraph),
    }
}

fn validate_reference_plan(
    spec: &ResolutionSpec,
    references: &BTreeMap<panproto_gat::Name, Reference>,
) -> Result<(), ResolveError> {
    for reference in references.values() {
        if let Some(edge) = &reference.edge {
            if !spec.semantic_schema.edges.contains_key(edge) {
                return Err(ResolveError::ReferenceConflict {
                    reference: reference.id.clone(),
                });
            }
        }
        match &reference.source {
            RefSource::Standoff { text, unit, .. } => {
                if matches!(text, panproto_schema::RefTextSource::External(_))
                    && !spec
                        .context_requirements
                        .contains(&ResolutionContextKind::ExternalText)
                {
                    return Err(ResolveError::UnsupportedFeature {
                        provider: Some(spec.provider.clone()),
                        feature: ResolutionFeature::ExternalText,
                    });
                }
                if matches!(unit, panproto_schema::OffsetUnit::Token)
                    && !spec
                        .context_requirements
                        .contains(&ResolutionContextKind::TokenBoundaries)
                {
                    return Err(ResolveError::UnsupportedFeature {
                        provider: Some(spec.provider.clone()),
                        feature: ResolutionFeature::TokenBoundaries,
                    });
                }
            }
            RefSource::KeyValue { .. } | RefSource::Path { .. } | RefSource::Variable { .. } => {}
        }
    }
    for (edge, writer) in &spec.reference_writers {
        let Some(reference) = references.get(writer) else {
            return Err(ResolveError::ReferenceConflict {
                reference: writer.clone(),
            });
        };
        if reference.edge.as_ref() != Some(edge) {
            return Err(ResolveError::ReferenceConflict {
                reference: writer.clone(),
            });
        }
    }
    Ok(())
}

fn expect_shape(instance: &AnyInstance, expected: InstanceShapeKind) -> Result<(), ResolveError> {
    let found = instance.shape();
    if found == expected {
        Ok(())
    } else {
        Err(ResolveError::ShapeMismatch { expected, found })
    }
}

fn instances_equivalent(left: &AnyInstance, right: &AnyInstance) -> bool {
    match (left, right) {
        (AnyInstance::Tree(left), AnyInstance::Tree(right)) => {
            panproto_lens::laws::instances_equivalent(left, right)
        }
        (AnyInstance::Table(left), AnyInstance::Table(right)) => left == right,
        (AnyInstance::Graph(left), AnyInstance::Graph(right)) => {
            GInstance::equivalent_modulo_edge_ids(left, right)
        }
        _ => false,
    }
}

fn canonical_graph_result(
    requested: &AnyInstance,
    produced: &AnyInstance,
    complement: ResolutionComplement,
) -> Result<ResolutionComplement, ResolveError> {
    let requested = requested
        .as_graph()
        .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
    let produced = produced
        .as_graph()
        .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
    if !GInstance::equivalent_modulo_edge_ids(requested, produced) {
        let carrier = requested
            .nodes()
            .keys()
            .copied()
            .find(|id| produced.node(*id).is_none())
            .map_or_else(
                || {
                    requested
                        .edges_sorted()
                        .next()
                        .map_or(ViewRef::Node(0), |(id, _)| ViewRef::Edge(id))
                },
                ViewRef::Node,
            );
        return Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
            carrier,
        }));
    }

    type EdgeSignature = (u32, u32, Edge, Option<u32>);
    let group = |graph: &GInstance| {
        let mut groups: BTreeMap<EdgeSignature, Vec<EdgeId>> = BTreeMap::new();
        for (id, edge) in graph.edges_sorted() {
            groups
                .entry((edge.src, edge.tgt, edge.edge.clone(), edge.position))
                .or_default()
                .push(id);
        }
        groups
    };
    let requested_groups = group(requested);
    let produced_groups = group(produced);
    let mut edge_ids = BTreeMap::new();
    for (signature, produced_ids) in produced_groups {
        let requested_ids = requested_groups.get(&signature).ok_or_else(|| {
            ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: produced_ids
                    .first()
                    .copied()
                    .map_or(ViewRef::Node(0), ViewRef::Edge),
            })
        })?;
        for (from, to) in produced_ids.into_iter().zip(requested_ids.iter().copied()) {
            edge_ids.insert(from, to);
        }
    }
    Ok(rekey_resolution_complement_edges(complement, &edge_ids))
}

fn rekey_resolution_complement_edges(
    mut complement: ResolutionComplement,
    edge_ids: &BTreeMap<EdgeId, EdgeId>,
) -> ResolutionComplement {
    let map_ref = |carrier: ViewRef| match carrier {
        ViewRef::Edge(id) => ViewRef::Edge(edge_ids.get(&id).copied().unwrap_or(id)),
        other => other,
    };
    complement.occurrences = complement
        .occurrences
        .into_iter()
        .map(|(carrier, occurrences)| (map_ref(carrier), occurrences))
        .collect();
    complement.spanning = complement
        .spanning
        .into_iter()
        .map(|(node, carrier)| (node, map_ref(carrier)))
        .collect();
    complement
}

fn stripped_graph(
    view: &AnyInstance,
    complement: &ResolutionComplement,
    reference_edges: impl IntoIterator<Item = EdgeId>,
) -> Result<GInstance, ResolveError> {
    let mut graph = view
        .as_graph()
        .cloned()
        .ok_or(ResolveError::Instance(ResolutionInstanceError::NotGraph))?;
    for id in reference_edges {
        if graph.edge(id).is_some() {
            graph.remove_edge(id).map_err(|_| {
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Edge(id),
                })
            })?;
        }
    }
    for &node in complement.span_nodes.keys() {
        let incident: Vec<_> = graph
            .out_edges(node)
            .iter()
            .chain(graph.in_edges(node))
            .copied()
            .collect();
        for edge in incident {
            graph.remove_edge(edge).map_err(|_| {
                ResolveError::Instance(ResolutionInstanceError::DanglingCarrier {
                    carrier: ViewRef::Node(node),
                })
            })?;
        }
        if graph.node(node).is_some() {
            graph.remove_node(node).map_err(|_| {
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Node(node),
                })
            })?;
        }
    }
    Ok(graph)
}

fn graph_to_table(graph: &GInstance) -> Result<FInstance, ResolveError> {
    let mut table = FInstance::new();
    let mut nodes: Vec<_> = graph.nodes().iter().collect();
    nodes.sort_unstable_by_key(|(id, _)| **id);
    for (&id, node) in nodes {
        table
            .insert_row(
                node.anchor.clone(),
                panproto_inst::RowId(id),
                node.extra_fields.clone(),
            )
            .map_err(|_| ResolveError::Instance(ResolutionInstanceError::NotTable))?;
    }
    for (_, edge) in graph.edges_sorted() {
        table
            .insert_foreign_key(
                edge.edge.clone(),
                panproto_inst::RowId(edge.src),
                panproto_inst::RowId(edge.tgt),
            )
            .map_err(|_| ResolveError::Instance(ResolutionInstanceError::NotTable))?;
    }
    Ok(table)
}

fn is_fan_edge(edge: &panproto_inst::GEdge) -> bool {
    edge.edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND
        || edge.edge.kind.as_ref() == panproto_inst::HYPER_LABEL_KIND
}

fn domain_sort_key(
    error: &ResolveDomainError,
) -> (Option<ViewRef>, Option<panproto_gat::Name>, u8) {
    match error {
        ResolveDomainError::NoPlacement { carrier } => (Some(carrier.clone()), None, 0),
        ResolveDomainError::AmbiguousPlacement {
            carrier,
            references,
        } => (Some(carrier.clone()), references.first().cloned(), 1),
        ResolveDomainError::UnrepresentableReference { carrier, reference } => {
            (Some(carrier.clone()), Some(reference.clone()), 2)
        }
        ResolveDomainError::SynthesizedEdited { carrier } => (Some(carrier.clone()), None, 3),
        ResolveDomainError::ContextMismatch { at } => (at.clone(), None, 4),
        ResolveDomainError::Assembly { at, production, .. } => (at.clone(), production.clone(), 5),
    }
}
