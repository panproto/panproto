//! Provider-indexed reference resolution between instance shapes.
//!
//! A format's syntax is read in one shape (a tree, a table instance) and
//! its meaning lives in another (usually a graph). A [`Resolution`] is the
//! state lens between them: [`Resolution::get`] resolves the references a
//! schema declares into semantic edges (and materialized standoff spans),
//! and [`Resolution::put`] writes an edited semantic view back to the
//! syntactic source from the [`ResolutionComplement`] get recorded.
//!
//! The lens is indexed by a [`ResolutionProvider`], which fixes the
//! semantic schema, a protocol-level reference overlay, the structural
//! edges, a writer for every reference edge, the document-varying services
//! the run needs and, for the two assembly kinds, a versioned grammar. A
//! [`ResolveContext`] supplies those services (external text and token
//! boundaries) for one run; the complement records a digest of every
//! external state an occurrence used, and put recomputes and compares it.
//!
//! # Resolving kinds
//!
//! For [`ResolutionKind::TreeToGraph`] the view is the tree's graph form
//! with its node ids, plus one semantic edge per resolved occurrence and one
//! node per materialized standoff span. A redirecting reference removes its
//! holder's subtree into the complement and points the holder's incoming
//! arc at the target. [`ResolutionKind::TableToGraph`] is the table's graph
//! form plus one edge per resolved occurrence.
//!
//! # Put domain
//!
//! [`Resolution::check_put_domain`] decides admission alone. It returns
//! every reason a view cannot be put, sorted by carrier, reference and
//! variant: a synthesized span or edge edited apart from its key, a carrier
//! with no syntactic placement or no writer, a reference that its writer
//! cannot write so that it resolves back to its target, a span that a text
//! edit leaves out of range or off a unit boundary, and a changed external
//! context. An admitted view is put to the source whose resolution is the
//! view itself, up to the presentation ids of synthesized carriers, so the
//! five laws of [`check_resolution_laws`] hold on the admitted domain.

mod build;
mod canonical;
mod cases;
mod compile;
mod error;
mod get;
mod laws;
mod occur;
mod put;
mod rekey;
mod source;
mod standoff;
mod verify;

use std::collections::BTreeMap;
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::identity::{
    IdentityCarrier, IdentityPolicy, identity_status, resolution_identity,
};
use panproto_inst::{IdentityEnvelope, InstanceShapeKind};
use panproto_schema::{Protocol, RefSource, Reference, Schema};

pub use self::error::{
    AssemblyDomainReason, OffsetUnitReason, ResolutionFeature, ResolutionInstanceError,
    ResolveDomainError, ResolveError,
};
pub use self::laws::{ResolutionLawReport, ResolutionLawViolation, check_resolution_laws};
pub use panproto_inst::resolution_complement::{
    AnyInstance, AssemblyPlanId, AssemblyResidue, AssemblyTrace, DefinitionKey, RedirectedArc,
    RedirectedAttachment, RedirectedFragment, RefOccurrence, ResolutionComplement,
    ResolutionContextKind, ResolutionKind, ResolutionProviderId, ResolutionSpan, ResolutionSpec,
    ResolutionTextId, ResolutionValueSite, UnresolvedReason, UnresolvedRef,
};

use self::compile::CompiledRef;

/// Runtime options of one resolution run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    /// Record a dangling or ambiguous occurrence of a reference declared
    /// `on_unresolved: Error` instead of failing.
    pub lenient: bool,
}

/// What a provider resolved a path literal to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathResolution {
    /// The matching elements of the reference's target vertex, ascending
    /// and distinct.
    pub candidates: Vec<ViewRef>,
    /// The digest of the exact external state the provider read, if any.
    pub context: Option<panproto_inst::Digest>,
}

/// Loads text named by a key held in the syntactic document.
pub trait ExternalTextStore: Send + Sync {
    /// The exact bytes stored under `key`.
    ///
    /// # Errors
    ///
    /// A typed resolution error when the key names no text.
    fn load(&self, key: &panproto_inst::Value) -> Result<Vec<u8>, ResolveError>;
}

/// Supplies the token boundaries of one text.
pub trait TokenBoundaryProvider: Send + Sync {
    /// The byte offsets at which the tokens of `utf8` begin, followed by
    /// its length: starting at zero, strictly increasing and ending at
    /// `utf8.len()`. `text` is the exact field-qualified identity of the
    /// text, so two texts held at two sites of one holder are told apart.
    ///
    /// # Errors
    ///
    /// A typed resolution error when the text cannot be tokenized.
    fn boundaries(&self, text: ResolutionTextId, utf8: &[u8]) -> Result<Vec<u64>, ResolveError>;
}

/// The document-varying services of one resolution run.
#[derive(Clone, Copy, Default)]
pub struct ResolveContext<'a> {
    /// External text.
    pub external_text: Option<&'a dyn ExternalTextStore>,
    /// Token boundaries.
    pub token_boundaries: Option<&'a dyn TokenBoundaryProvider>,
}

impl std::fmt::Debug for ResolveContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveContext")
            .field("external_text", &self.external_text.is_some())
            .field("token_boundaries", &self.token_boundaries.is_some())
            .finish()
    }
}

impl ResolveContext<'_> {
    /// A context with no services, valid exactly when the compiled spec
    /// requires none.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            external_text: None,
            token_boundaries: None,
        }
    }
}

pub use panproto_inst::ViewRef;

/// The format semantics a resolution relies on.
///
/// `spec` is context-free, since its result is part of the resolution's
/// identity. The path methods resolve and render the reference forms the
/// format owns; the assembly methods implement the grammar of the two
/// assembly kinds. Every default refuses with
/// [`ResolveError::UnsupportedFeature`].
pub trait ResolutionProvider: Send + Sync {
    /// The provider's versioned identity.
    fn id(&self) -> ResolutionProviderId;

    /// The shape change the provider implements.
    fn kind(&self) -> ResolutionKind;

    /// The protocol the provider serves, including one with no descriptor
    /// in `panproto-protocols`.
    fn protocol(&self) -> &Protocol;

    /// The provider's plan for `syntactic`.
    ///
    /// # Errors
    ///
    /// A typed declaration or provider error.
    fn spec(&self, protocol: &Protocol, syntactic: &Schema)
    -> Result<ResolutionSpec, ResolveError>;

    /// Resolve the path `literal` held at `holder`.
    ///
    /// # Errors
    ///
    /// The default reports the path syntax as unsupported.
    fn resolve_path(
        &self,
        reference: &Reference,
        holder: ViewRef,
        literal: &panproto_inst::Value,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<PathResolution, ResolveError> {
        let _ = (holder, literal, source, ctx);
        Err(unsupported_path(self.id(), reference))
    }

    /// The canonical path from `holder` to `target` in `view`, which
    /// [`Self::resolve_path`] must resolve to exactly `[target]`.
    ///
    /// # Errors
    ///
    /// The default reports the path syntax as unsupported.
    fn render_path(
        &self,
        reference: &Reference,
        holder: ViewRef,
        target: ViewRef,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<panproto_inst::Value, ResolveError> {
        let _ = (holder, target, view, ctx);
        Err(unsupported_path(self.id(), reference))
    }

    /// Assemble `source` through the grammar `plan`.
    ///
    /// # Errors
    ///
    /// The default reports assembly as unsupported.
    fn assembly_get(
        &self,
        plan: &AssemblyPlanId,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, AssemblyResidue), ResolveError> {
        let _ = (plan, source, ctx);
        Err(unsupported_assembly(self.id(), self.kind()))
    }

    /// Put an assembled view back through its residue.
    ///
    /// # Errors
    ///
    /// The default reports assembly as unsupported.
    fn assembly_put(
        &self,
        plan: &AssemblyPlanId,
        view: &AnyInstance,
        residue: &AssemblyResidue,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        let _ = (plan, view, residue, ctx);
        Err(unsupported_assembly(self.id(), self.kind()))
    }

    /// Every reason an assembled view cannot be put through its residue.
    ///
    /// The default admits nothing: it reports the missing production.
    fn assembly_check_put_domain(
        &self,
        plan: &AssemblyPlanId,
        view: &AnyInstance,
        residue: &AssemblyResidue,
        ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        let _ = (plan, view, residue, ctx);
        vec![ResolveDomainError::Assembly {
            at: None,
            production: None,
            reason: AssemblyDomainReason::MissingProduction,
        }]
    }

    /// A source and residue for an assembled view, without an earlier get.
    ///
    /// # Errors
    ///
    /// The default reports assembly as unsupported.
    fn assembly_put_canonical(
        &self,
        plan: &AssemblyPlanId,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, AssemblyResidue), ResolveError> {
        let _ = (plan, view, ctx);
        Err(unsupported_assembly(self.id(), self.kind()))
    }
}

fn unsupported_path(provider: ResolutionProviderId, reference: &Reference) -> ResolveError {
    let syntax = match &reference.source {
        RefSource::Path { syntax, .. } => syntax.clone(),
        RefSource::KeyValue { .. } | RefSource::Standoff { .. } | RefSource::Variable { .. } => {
            panproto_schema::PathSyntax::IdRef
        }
    };
    ResolveError::UnsupportedFeature {
        provider: Some(provider),
        feature: ResolutionFeature::Path(syntax),
    }
}

const fn unsupported_assembly(
    provider: ResolutionProviderId,
    kind: ResolutionKind,
) -> ResolveError {
    ResolveError::UnsupportedFeature {
        provider: Some(provider),
        feature: ResolutionFeature::Assembly(kind),
    }
}

/// A compiled, immutable resolution lens.
pub struct Resolution {
    protocol_name: String,
    syntactic_schema: Schema,
    spec: ResolutionSpec,
    references: BTreeMap<Name, Reference>,
    compiled: BTreeMap<Name, CompiledRef>,
    identity: IdentityEnvelope,
    provider: Arc<dyn ResolutionProvider>,
}

impl std::fmt::Debug for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolution")
            .field("protocol", &self.protocol_name)
            .field("kind", &self.spec.kind)
            .field("provider", &self.spec.provider)
            .finish_non_exhaustive()
    }
}

impl Resolution {
    /// Compile `provider`'s resolution of `syntactic` for `protocol`.
    ///
    /// # Errors
    ///
    /// The typed refusals of compilation, in the order the crate-private
    /// compiler checks them: an unequal protocol, the provider's spec
    /// error, a spec naming another provider or kind, a target shape other
    /// than the protocol's semantic shape, a misplaced assembly plan, a
    /// conflicting or malformed declaration, a missing writer and a missing
    /// context requirement.
    pub fn for_protocol(
        protocol: &Protocol,
        syntactic: &Schema,
        provider: Arc<dyn ResolutionProvider>,
    ) -> Result<Self, ResolveError> {
        let compiled = compile::compile(protocol, syntactic, &provider)?;
        let identity = resolution_identity(&compiled.protocol_name, syntactic, &compiled.spec);
        Ok(Self {
            protocol_name: compiled.protocol_name,
            syntactic_schema: syntactic.clone(),
            spec: compiled.spec,
            references: compiled.references,
            compiled: compiled.forms,
            identity,
            provider,
        })
    }

    /// The provider's spec.
    #[must_use]
    pub const fn spec(&self) -> &ResolutionSpec {
        &self.spec
    }

    /// The semantic schema the view is an instance of.
    #[must_use]
    pub const fn semantic_schema(&self) -> &Schema {
        &self.spec.semantic_schema
    }

    /// The syntactic schema the source is an instance of.
    #[must_use]
    pub const fn syntactic_schema(&self) -> &Schema {
        &self.syntactic_schema
    }

    /// The schema's and the provider's references, merged by id.
    #[must_use]
    pub const fn references(&self) -> &BTreeMap<Name, Reference> {
        &self.references
    }

    /// The canonical name of the protocol the resolution serves.
    #[must_use]
    pub fn protocol_name(&self) -> &str {
        &self.protocol_name
    }

    /// The identity every complement of this resolution is stamped with.
    #[must_use]
    pub const fn identity(&self) -> &IdentityEnvelope {
        &self.identity
    }

    /// The provider.
    #[must_use]
    pub fn provider(&self) -> &dyn ResolutionProvider {
        self.provider.as_ref()
    }

    /// Resolve a syntactic source into its semantic view and the stamped
    /// complement that records everything the view does not show.
    ///
    /// # Errors
    ///
    /// [`ResolveError::ShapeMismatch`] for a source of another shape, then
    /// the source, context, fidelity, resolution and allocation errors of
    /// the kind, in the order the source is read.
    pub fn get(
        &self,
        source: &AnyInstance,
        options: &ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        expect_shape(source, self.spec.kind.syntactic_shape())?;
        let (view, mut complement) = match self.spec.kind {
            ResolutionKind::TreeToGraph | ResolutionKind::TableToGraph => {
                let (graph, complement) = self.get_graph(source, *options, ctx)?;
                (AnyInstance::Graph(graph), complement)
            }
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let (view, residue) =
                    self.provider
                        .assembly_get(self.assembly_plan()?, source, ctx)?;
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

    /// Put a semantic view back to its syntactic source.
    ///
    /// Put checks the view's shape, then the complement's identity, then
    /// the put domain, and returns the first domain violation before it
    /// writes anything; operational context, provider and allocation errors
    /// follow.
    ///
    /// # Errors
    ///
    /// [`ResolveError::ShapeMismatch`], [`ResolveError::Identity`],
    /// [`ResolveError::PutDomain`] with the first element of
    /// [`Self::check_put_domain`]'s vector, then operational errors.
    pub fn put(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        expect_shape(view, self.spec.kind.semantic_shape())?;
        let status = identity_status(complement, &self.identity, &self.syntactic_schema)?;
        IdentityPolicy::Strict.admit(&status, complement.records_nothing())?;
        match self.spec.kind {
            ResolutionKind::TreeToGraph | ResolutionKind::TableToGraph => {
                self.put_graph(view, complement, ctx)
            }
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let plan = self.assembly_plan()?;
                let residue = complement.assembly.as_ref().ok_or(ResolveError::PutDomain(
                    ResolveDomainError::Assembly {
                        at: None,
                        production: None,
                        reason: AssemblyDomainReason::ResidueMismatch,
                    },
                ))?;
                let mut errors = self
                    .provider
                    .assembly_check_put_domain(plan, view, residue, ctx);
                sort_domain_errors(&mut errors);
                if let Some(first) = errors.into_iter().next() {
                    return Err(ResolveError::PutDomain(first));
                }
                let source = self.provider.assembly_put(plan, view, residue, ctx)?;
                expect_shape(&source, self.spec.kind.syntactic_shape())?;
                Ok(source)
            }
        }
    }

    /// Every reason `view` cannot be put with `complement`, sorted by
    /// carrier, reference or production, and variant.
    ///
    /// The vector is empty only for a view of another shape, which put
    /// refuses with [`ResolveError::ShapeMismatch`] before it consults the
    /// domain.
    ///
    /// # Errors
    ///
    /// The complete sorted vector of violations.
    pub fn check_put_domain(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<(), Vec<ResolveDomainError>> {
        if view.shape() != self.spec.kind.semantic_shape() {
            return Err(Vec::new());
        }
        let mut errors = match self.spec.kind {
            ResolutionKind::TreeToGraph | ResolutionKind::TableToGraph => {
                self.graph_domain(view, complement, ctx)
            }
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                match (&self.spec.assembly, &complement.assembly) {
                    (Some(plan), Some(residue)) => self
                        .provider
                        .assembly_check_put_domain(plan, view, residue, ctx),
                    _ => vec![ResolveDomainError::Assembly {
                        at: None,
                        production: None,
                        reason: AssemblyDomainReason::ResidueMismatch,
                    }],
                }
            }
        };
        sort_domain_errors(&mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The canonical source of `view` and its stamped complement, for
    /// emission without a complement.
    ///
    /// The candidate source is built by the kind's deterministic placement
    /// rules, then resolved again: its view must be `view` up to the
    /// presentation ids of synthesized carriers, and its complement is
    /// rekeyed to `view`'s ids. A view no source resolves to is refused.
    ///
    /// # Errors
    ///
    /// [`ResolveError::ShapeMismatch`], [`ResolveError::PutDomain`] with a
    /// placement refusal, or the errors of get on the candidate.
    pub fn put_canonical(
        &self,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        expect_shape(view, self.spec.kind.semantic_shape())?;
        match self.spec.kind {
            ResolutionKind::TreeToGraph | ResolutionKind::TableToGraph => {
                self.put_canonical_graph(view, ctx)
            }
            ResolutionKind::TreeToTable | ResolutionKind::TableToTree => {
                let (source, _) =
                    self.provider
                        .assembly_put_canonical(self.assembly_plan()?, view, ctx)?;
                expect_shape(&source, self.spec.kind.syntactic_shape())?;
                let (regotten, complement) = self.get(&source, &ResolveOptions::default(), ctx)?;
                if !verify::same_instance(view, &regotten) {
                    return Err(ResolveError::PutDomain(ResolveDomainError::Assembly {
                        at: None,
                        production: None,
                        reason: AssemblyDomainReason::ResidueMismatch,
                    }));
                }
                Ok((source, complement))
            }
        }
    }

    fn assembly_plan(&self) -> Result<&AssemblyPlanId, ResolveError> {
        self.spec
            .assembly
            .as_ref()
            .ok_or_else(|| unsupported_assembly(self.spec.provider.clone(), self.spec.kind))
    }
}

/// Refuse an instance of a shape other than `expected`.
fn expect_shape(instance: &AnyInstance, expected: InstanceShapeKind) -> Result<(), ResolveError> {
    if instance.shape() == expected {
        Ok(())
    } else {
        Err(ResolveError::ShapeMismatch {
            expected,
            found: instance.shape(),
        })
    }
}

/// Sort a violation vector by carrier, reference or production, and
/// variant, and drop repeats.
fn sort_domain_errors(errors: &mut Vec<ResolveDomainError>) {
    errors.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    errors.dedup();
}
