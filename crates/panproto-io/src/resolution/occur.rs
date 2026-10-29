//! Reading reference occurrences from a syntactic source and resolving
//! them.
//!
//! [`read`] evaluates every reference at every holder into a sorted list of
//! [`Occurrence`]s, each with its literal, the digest of the external state
//! it used and what it is looked up by; [`resolve`] then gives each its
//! [`Outcome`]. Both are deterministic functions of the source, the compiled
//! references, the provider and the context.

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_inst::identity::{
    resolution_context_digest, resolution_external_text_digest, resolution_token_boundaries_digest,
};
use panproto_inst::key::{KeyingError, canonical_key_bytes};
use panproto_inst::resolution_complement::{ResolutionSpan, ResolutionTextId, ResolutionValueSite};
use panproto_inst::{
    AnyInstance, Digest, FidelityReason, ResolutionProviderId, Value, ValueFidelityError, ViewRef,
};
use panproto_schema::{
    Cardinality, Edge, NullMatch, OffsetUnit, OnUnresolved, RefTextSource, Reference, ScalarKind,
    SpanEnd, VariableBinding,
};

use super::compile::{CompiledRef, RefForm};
use super::error::{KeyEqualityReason, ResolutionFeature, ResolveError};
use super::source::{KeyIndex, ScopeOf, Source};
use super::standoff::{self, UnitFailure};
use super::{ResolutionProvider, ResolveContext};
use panproto_schema::Key;

/// The `(holder, reference)` pairs a run leaves unread.
pub type Skipped = std::collections::BTreeSet<(ViewRef, Name)>;

/// What one occurrence is looked up by.
#[derive(Clone, Debug)]
pub enum Lookup {
    /// A key tuple in the index of `key` over the target vertex, within a
    /// scope.
    Key {
        /// The target vertex.
        vertex: Name,
        /// The key.
        key: Key,
        /// The holder's scope under the key.
        scope: ScopeOf,
        /// The key tuple.
        values: Vec<Value>,
    },
    /// A self-binding variable name, within a scope.
    Binding {
        /// The holder's scope under the binder key.
        scope: ScopeOf,
        /// The canonical bytes of the name.
        bytes: Vec<u8>,
        /// Which matching preceding definition binds this occurrence.
        binding: VariableBinding,
    },
    /// Candidates a provider resolved a path to, ascending.
    Candidates(Vec<ViewRef>),
    /// A materialized span.
    Span(ResolutionSpan),
    /// A guarded declaration group for which no guard was selected.
    Unselected(Value),
}

/// One occurrence of a reference at a holder.
#[derive(Clone, Debug)]
pub struct Occurrence<'r> {
    /// The holder.
    pub holder: ViewRef,
    /// The reference, compiled.
    pub compiled: &'r CompiledRef,
    /// Its position among the reference's occurrences at the holder.
    pub index: u32,
    /// The exact syntactic literal.
    pub literal: Value,
    /// The digest of the external state it used.
    pub context: Option<Digest>,
    /// What it is looked up by.
    pub lookup: Lookup,
}

/// How one occurrence resolved.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// It names exactly this element (or span key).
    Resolved(ViewRef),
    /// It names the span it carries.
    Span,
    /// It is the definition of a self-binding variable at its own holder.
    Definition,
    /// It names no element.
    Dangling,
    /// It names several elements, ascending.
    Ambiguous(Vec<ViewRef>),
    /// No declaration in a guarded group matched this exact value.
    Unselected(Value),
}

enum GuardAction {
    Read,
    Skip,
    Unselected(Value),
}

/// The stable identity of a local or externally loaded resolution text.
///
/// The resource key and loaded bytes are context, not identity: two loads at
/// the same field keep one identity and are distinguished by their recorded
/// context digest.
#[must_use]
pub const fn text_identity(site: ResolutionValueSite, external: bool) -> ResolutionTextId {
    if external {
        ResolutionTextId::ExternalKey(site)
    } else {
        ResolutionTextId::Local(site)
    }
}

/// The literal of a key tuple: the value itself for one part, a list of
/// the values otherwise.
#[must_use]
pub fn tuple_literal(values: &[Value]) -> Value {
    match values {
        [value] => value.clone(),
        values => Value::List(values.to_vec()),
    }
}

/// The values of one reference part read as a sequence: a list value
/// contributes its items, any other value itself.
fn sequence(values: Vec<Value>) -> Vec<Value> {
    values
        .into_iter()
        .flat_map(|value| match value {
            Value::List(items) => items,
            other => vec![other],
        })
        .collect()
}

fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

/// The tuples one holder's parts produce: one tuple for a `One` reference
/// whose parts each hold exactly one value, the positional zip of equal
/// length sequences for a `Many` reference, nothing for a nullable
/// reference that holds no value.
fn tuples(
    reference: &Reference,
    holder: &ViewRef,
    parts: Vec<Vec<Value>>,
) -> Result<Vec<Vec<Value>>, ResolveError> {
    let mismatch = |lengths: Vec<u64>| ResolveError::ReferenceCardinalityMismatch {
        reference: reference.id.clone(),
        at: Box::new(holder.clone()),
        lengths,
    };
    match reference.cardinality {
        Cardinality::One => {
            let lengths: Vec<u64> = parts.iter().map(|part| len_u64(part.len())).collect();
            if reference.nullable && lengths.iter().all(|length| *length == 0) && !parts.is_empty()
            {
                return Ok(Vec::new());
            }
            if lengths.iter().any(|length| *length != 1) {
                return Err(mismatch(lengths));
            }
            Ok(vec![parts.into_iter().flatten().collect()])
        }
        Cardinality::Many => {
            let sequences: Vec<Vec<Value>> = parts.into_iter().map(sequence).collect();
            let lengths: Vec<u64> = sequences.iter().map(|part| len_u64(part.len())).collect();
            let length = sequences.first().map_or(0, Vec::len);
            if sequences.iter().any(|part| part.len() != length)
                || (length == 0 && !reference.nullable)
            {
                return Err(mismatch(lengths));
            }
            Ok((0..length)
                .map(|index| sequences.iter().map(|part| part[index].clone()).collect())
                .collect())
        }
    }
}

/// One occurrence as read: its literal, its context digest and its lookup.
type Read = (Value, Option<Digest>, Lookup);

/// A standoff declaration's paths and offset convention.
struct Declared<'d> {
    text: &'d RefTextSource,
    start: &'d [Edge],
    end: &'d [Edge],
    unit: &'d OffsetUnit,
    end_inclusive: bool,
    end_form: SpanEnd,
    origin: u64,
}

/// The context every occurrence evaluation reads.
pub struct Reader<'s, 'a> {
    /// The source.
    pub source: &'s Source<'a>,
    /// The same source as the provider receives it.
    pub any: &'s AnyInstance,
    /// The provider.
    pub provider: &'s dyn ResolutionProvider,
    /// The provider identity named by provider failures.
    pub provider_id: &'s ResolutionProviderId,
    /// The run's services.
    pub ctx: &'s ResolveContext<'s>,
}

impl Reader<'_, '_> {
    /// Every occurrence of every reference, sorted by `(holder, reference,
    /// index)`, except those of the `(holder, reference)` pairs in `skip`.
    ///
    /// # Errors
    ///
    /// The first failure in that order: a cardinality mismatch, a provider
    /// or context failure, an offset outside its text or off a unit
    /// boundary, or a value that is not an offset or a text.
    pub fn read<'r>(
        &self,
        references: &'r BTreeMap<Name, CompiledRef>,
        skip: &Skipped,
    ) -> Result<Vec<Occurrence<'r>>, ResolveError> {
        let mut occurrences = Vec::new();
        for (holder, compiled) in self.holders(references) {
            if skip.contains(&(holder.clone(), compiled.reference.id.clone())) {
                continue;
            }
            match self.guard_action(references, skip, compiled, &holder) {
                GuardAction::Read => occurrences.extend(self.read_at(compiled, &holder)?),
                GuardAction::Skip => {}
                GuardAction::Unselected(value) => occurrences.push(Occurrence {
                    holder,
                    compiled,
                    index: 0,
                    literal: value.clone(),
                    context: None,
                    lookup: Lookup::Unselected(value),
                }),
            }
        }
        Ok(occurrences)
    }

    /// Every holder and reference whose occurrences cannot be read, with
    /// the failure, in `(holder, reference)` order.
    pub fn failures<'r>(
        &self,
        references: &'r BTreeMap<Name, CompiledRef>,
    ) -> Vec<(ViewRef, &'r CompiledRef, ResolveError)> {
        let mut failures = Vec::new();
        let skip = Skipped::new();
        for (holder, compiled) in self.holders(references) {
            if matches!(
                self.guard_action(references, &skip, compiled, &holder),
                GuardAction::Read
            ) && let Err(error) = self.read_at(compiled, &holder)
            {
                failures.push((holder, compiled, error));
            }
        }
        failures
    }

    fn guard_action(
        &self,
        references: &BTreeMap<Name, CompiledRef>,
        skip: &Skipped,
        compiled: &CompiledRef,
        holder: &ViewRef,
    ) -> GuardAction {
        let Some(guard) = &compiled.reference.guard else {
            return GuardAction::Read;
        };
        let Some(actual) = self
            .source
            .path_sites(holder, &guard.field)
            .into_iter()
            .next()
            .map(|(_, value)| value.clone())
        else {
            return GuardAction::Skip;
        };
        let in_group = |candidate: &CompiledRef| {
            candidate.reference.holder == compiled.reference.holder
                && candidate.reference.source == compiled.reference.source
                && candidate
                    .reference
                    .guard
                    .as_ref()
                    .is_some_and(|candidate| candidate.field == guard.field)
        };
        if references
            .values()
            .filter(|candidate| in_group(candidate))
            .any(|candidate| {
                candidate
                    .reference
                    .guard
                    .as_ref()
                    .is_some_and(|candidate| candidate.equals.canonical_cmp(&actual).is_eq())
            })
        {
            return if guard.equals.canonical_cmp(&actual).is_eq() {
                GuardAction::Read
            } else {
                GuardAction::Skip
            };
        }
        let representative = references
            .values()
            .filter(|candidate| in_group(candidate))
            .find(|candidate| !skip.contains(&(holder.clone(), candidate.reference.id.clone())));
        if representative.is_some_and(|candidate| candidate.reference.id == compiled.reference.id) {
            GuardAction::Unselected(actual)
        } else {
            GuardAction::Skip
        }
    }

    /// Every holder of every reference, in `(holder, reference)` order.
    fn holders<'r>(
        &self,
        references: &'r BTreeMap<Name, CompiledRef>,
    ) -> Vec<(ViewRef, &'r CompiledRef)> {
        let mut holders: Vec<(ViewRef, &CompiledRef)> = Vec::new();
        for compiled in references.values() {
            for holder in self.source.elements(&compiled.reference.holder) {
                holders.push((holder, compiled));
            }
        }
        holders.sort_by(|a, b| (&a.0, &a.1.reference.id).cmp(&(&b.0, &b.1.reference.id)));
        holders
    }

    fn read_at<'r>(
        &self,
        compiled: &'r CompiledRef,
        holder: &ViewRef,
    ) -> Result<Vec<Occurrence<'r>>, ResolveError> {
        let reads = match &compiled.form {
            RefForm::KeyValue { key, parts } => {
                let paths: Vec<&[Edge]> = parts.iter().map(Vec::as_slice).collect();
                self.keyed(compiled, holder, key, &paths, None)?
            }
            RefForm::Variable {
                key,
                field,
                self_binding,
                binding,
            } => self.keyed(
                compiled,
                holder,
                key,
                &[field.as_slice()],
                self_binding.then_some(*binding),
            )?,
            RefForm::Path { field } => self.paths(compiled, holder, field)?,
            RefForm::Standoff {
                key,
                text,
                start,
                end,
                unit,
                end_inclusive,
                end_form,
                origin,
            } => {
                let declaration = Declared {
                    text,
                    start,
                    end,
                    unit,
                    end_inclusive: *end_inclusive,
                    end_form: *end_form,
                    origin: *origin,
                };
                self.standoff(compiled, holder, key.as_ref(), &declaration)?
            }
        };
        reads
            .into_iter()
            .enumerate()
            .map(|(index, (literal, context, lookup))| {
                Ok(Occurrence {
                    holder: holder.clone(),
                    compiled,
                    index: u32::try_from(index).map_err(|_| ResolveError::IdExhausted {
                        kind: panproto_inst::segment::IdKind::Edge,
                    })?,
                    literal,
                    context,
                    lookup,
                })
            })
            .collect()
    }

    /// The values reached from `holder` along `path`.
    fn values(&self, holder: &ViewRef, path: &[Edge]) -> Vec<&Value> {
        self.source
            .path_sites(holder, path)
            .into_iter()
            .map(|(_, value)| value)
            .collect()
    }

    /// Values reached along one foreign-key part, retaining explicit nulls
    /// so the declaration's null-matching mode can decide tuple presence.
    fn key_values(&self, holder: &ViewRef, path: &[Edge]) -> Vec<Value> {
        self.source.path_values_or_null(holder, path)
    }

    /// The occurrences of a reference looked up by a key: key values, or a
    /// variable's name.
    fn keyed(
        &self,
        compiled: &CompiledRef,
        holder: &ViewRef,
        key: &Key,
        paths: &[&[Edge]],
        binding: Option<VariableBinding>,
    ) -> Result<Vec<Read>, ResolveError> {
        let reference = &compiled.reference;
        let scope = self.source.scope_of(holder, &key.scope);
        let parts = paths
            .iter()
            .map(|path| match reference.cardinality {
                Cardinality::One => self.key_values(holder, path),
                Cardinality::Many => self.values(holder, path).into_iter().cloned().collect(),
            })
            .collect();
        tuples(reference, holder, parts)?
            .into_iter()
            .filter(|values| match reference.null_match {
                NullMatch::Simple => values.iter().all(|value| *value != Value::Null),
                NullMatch::Full => values.iter().any(|value| *value != Value::Null),
            })
            .map(|values| {
                let lookup = match binding {
                    None => Lookup::Key {
                        vertex: reference.target_vertex.clone(),
                        key: key.clone(),
                        scope: scope.clone(),
                        values: values.clone(),
                    },
                    // A binding partitions definitions and occurrences by the
                    // binder key's declared equality, as the key lookup does.
                    Some(binding) => Lookup::Binding {
                        scope: scope.clone(),
                        bytes: canonical_key_bytes(key, &values)
                            .map_err(|error| key_equality_error(key, error))?,
                        binding,
                    },
                };
                Ok((tuple_literal(&values), None, lookup))
            })
            .collect()
    }

    /// The occurrences of a path reference, each resolved by the provider.
    fn paths(
        &self,
        compiled: &CompiledRef,
        holder: &ViewRef,
        field: &[Edge],
    ) -> Result<Vec<Read>, ResolveError> {
        let reference = &compiled.reference;
        tuples(
            reference,
            holder,
            vec![self.values(holder, field).into_iter().cloned().collect()],
        )?
        .into_iter()
        .map(|values| {
            let literal = tuple_literal(&values);
            let resolved = self.provider.resolve_path(
                reference,
                holder.clone(),
                &literal,
                self.any,
                self.ctx,
            )?;
            let candidates = self.typed_candidates(reference, holder, resolved.candidates)?;
            Ok((literal, resolved.context, Lookup::Candidates(candidates)))
        })
        .collect()
    }

    /// The occurrences of a standoff reference: spans over the holder's one
    /// text site, materialized or looked up by their declared offsets.
    fn standoff(
        &self,
        compiled: &CompiledRef,
        holder: &ViewRef,
        key: Option<&Key>,
        declared: &Declared<'_>,
    ) -> Result<Vec<Read>, ResolveError> {
        let reference = &compiled.reference;
        let pairs = tuples(
            reference,
            holder,
            vec![
                self.values(holder, declared.start)
                    .into_iter()
                    .cloned()
                    .collect(),
                self.values(holder, declared.end)
                    .into_iter()
                    .cloned()
                    .collect(),
            ],
        )?;
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let text = self.text(reference, holder, declared.text, declared.unit)?;
        let keyed = key.map(|key| (key, self.source.scope_of(holder, &key.scope)));
        pairs
            .into_iter()
            .map(|values| {
                let (start, end) = text.interval(reference, holder, declared, &values)?;
                let lookup = match &keyed {
                    None => Lookup::Span(ResolutionSpan {
                        text: text.id.clone(),
                        start,
                        end,
                    }),
                    Some((key, scope)) => Lookup::Key {
                        vertex: reference.target_vertex.clone(),
                        key: (*key).clone(),
                        scope: scope.clone(),
                        values: values.clone(),
                    },
                };
                Ok((tuple_literal(&values), text.context, lookup))
            })
            .collect()
    }

    /// The provider's candidates that are elements of the reference's
    /// target vertex.
    ///
    /// Target typing is exact, so a candidate of another vertex is not a
    /// target and an occurrence whose candidates are all of other vertices
    /// dangles. Candidates that are not ascending and distinct, or that are
    /// not elements of the source, are a provider failure.
    fn typed_candidates(
        &self,
        reference: &Reference,
        holder: &ViewRef,
        candidates: Vec<ViewRef>,
    ) -> Result<Vec<ViewRef>, ResolveError> {
        let ascending = candidates.windows(2).all(|pair| pair[0] < pair[1]);
        let elements = candidates
            .iter()
            .all(|candidate| self.source.anchor(candidate).is_some());
        if !(ascending && elements) {
            return Err(ResolveError::ProviderFailure {
                provider: self.provider_id.clone(),
                code: "path_candidates".into(),
                at: Some(Box::new(holder.clone())),
            });
        }
        Ok(candidates
            .into_iter()
            .filter(|candidate| {
                self.source
                    .anchor(candidate)
                    .is_some_and(|anchor| *anchor == reference.target_vertex)
            })
            .collect())
    }

    /// The text of a standoff reference at a holder: its field-qualified
    /// identity, its exact bytes, the unit boundaries the declared unit
    /// names and the digest of the external state it used.
    ///
    /// The text path must reach exactly one value site. A site that is the
    /// own value of a node with children is a string range over a non-leaf
    /// node, which standoff does not address: it is refused as the `XPath`
    /// path syntax a provider resolves.
    fn text(
        &self,
        reference: &Reference,
        holder: &ViewRef,
        source: &RefTextSource,
        unit: &OffsetUnit,
    ) -> Result<Text, ResolveError> {
        let (path, local) = match source {
            RefTextSource::Local(path) => (path, true),
            RefTextSource::External(path) => (path, false),
            RefTextSource::Referenced { .. } => {
                return Err(ResolveError::UnsupportedFeature {
                    provider: Some(self.provider_id.clone()),
                    feature: ResolutionFeature::ReferencedText,
                });
            }
            RefTextSource::Span { .. } => {
                return Err(ResolveError::UnsupportedFeature {
                    provider: Some(self.provider_id.clone()),
                    feature: ResolutionFeature::SpanText,
                });
            }
            RefTextSource::ExternalUri { .. } => {
                return Err(ResolveError::UnsupportedFeature {
                    provider: Some(self.provider_id.clone()),
                    feature: ResolutionFeature::ExternalUriText,
                });
            }
        };
        let sites = self.source.path_sites(holder, path);
        let [(site, value)] = sites.as_slice() else {
            return Err(ResolveError::ReferenceCardinalityMismatch {
                reference: reference.id.clone(),
                at: Box::new(holder.clone()),
                lengths: vec![len_u64(sites.len())],
            });
        };
        if let (true, panproto_inst::resolution_complement::ResolutionValueSite::Own(node)) =
            (local, site)
        {
            if !self.source.out_steps(node).is_empty() {
                return Err(ResolveError::UnsupportedFeature {
                    provider: Some(self.provider_id.clone()),
                    feature: ResolutionFeature::Path(panproto_schema::PathSyntax::XPath),
                });
            }
        }
        let (id, bytes, external) = if local {
            (
                text_identity(site.clone(), false),
                text_bytes(value, reference)?,
                None,
            )
        } else {
            let store = self
                .ctx
                .external_text
                .ok_or_else(|| ResolveError::UnsupportedFeature {
                    provider: Some(self.provider_id.clone()),
                    feature: ResolutionFeature::ExternalText,
                })?;
            let bytes = store.load(value)?;
            let digest = resolution_external_text_digest(value, &bytes);
            (text_identity(site.clone(), true), bytes, Some(digest))
        };
        let (tokens, token_digest) = if matches!(unit, OffsetUnit::Token) {
            let provider =
                self.ctx
                    .token_boundaries
                    .ok_or_else(|| ResolveError::UnsupportedFeature {
                        provider: Some(self.provider_id.clone()),
                        feature: ResolutionFeature::TokenBoundaries,
                    })?;
            let tokens = provider.boundaries(id.clone(), &bytes)?;
            if !standoff::valid_token_boundaries(&tokens, len_u64(bytes.len())) {
                return Err(ResolveError::ProviderFailure {
                    provider: self.provider_id.clone(),
                    code: "token_boundaries".into(),
                    at: Some(Box::new(holder.clone())),
                });
            }
            let digest = resolution_token_boundaries_digest(&id, &tokens);
            (Some(tokens), Some(digest))
        } else {
            (None, None)
        };
        let boundaries = standoff::unit_boundaries(&bytes, unit, tokens.as_deref());
        Ok(Text {
            id,
            boundaries,
            context: resolution_context_digest(external.as_ref(), token_digest.as_ref()),
        })
    }
}

/// The resolved text of one standoff holder.
struct Text {
    id: ResolutionTextId,
    boundaries: Result<Vec<Option<u64>>, super::error::OffsetUnitReason>,
    context: Option<Digest>,
}

impl Text {
    /// The byte interval a declared `(start, end)` pair names.
    fn interval(
        &self,
        reference: &Reference,
        holder: &ViewRef,
        declared: &Declared<'_>,
        values: &[Value],
    ) -> Result<(u64, u64), ResolveError> {
        let [start, end] = values else {
            return Err(ResolveError::ReferenceCardinalityMismatch {
                reference: reference.id.clone(),
                at: Box::new(holder.clone()),
                lengths: vec![len_u64(values.len())],
            });
        };
        let start = offset(start, reference)?;
        let end = offset(end, reference)?;
        let out_of_range = || ResolveError::OffsetOutOfRange {
            reference: reference.id.clone(),
            at: Box::new(holder.clone()),
            start,
            end,
            len: self
                .boundaries
                .as_ref()
                .map_or(0, |boundaries| standoff::unit_len(boundaries)),
        };
        let (declared_start, declared_end) =
            standoff::form_to_declared(start, end, declared.origin, declared.end_form)
                .map_err(|_| out_of_range())?;
        // A reversed pair is out of range. An inclusive end one before its
        // start is the empty inclusive span, which the unit conversion
        // refuses with its own reason.
        if declared_start > declared_end.saturating_add(u64::from(declared.end_inclusive)) {
            return Err(out_of_range());
        }
        let boundaries =
            self.boundaries
                .as_ref()
                .map_err(|reason| ResolveError::OffsetUnitViolation {
                    reference: reference.id.clone(),
                    at: Box::new(holder.clone()),
                    unit: declared.unit.clone(),
                    offset: start,
                    reason: *reason,
                })?;
        standoff::declared_to_bytes(
            boundaries,
            declared_start,
            declared_end,
            declared.end_inclusive,
        )
        .map_err(|(failure, offset)| match failure {
            UnitFailure::OutOfRange => ResolveError::OffsetOutOfRange {
                reference: reference.id.clone(),
                at: Box::new(holder.clone()),
                start,
                end,
                len: standoff::unit_len(boundaries),
            },
            UnitFailure::Violation(reason) => ResolveError::OffsetUnitViolation {
                reference: reference.id.clone(),
                at: Box::new(holder.clone()),
                unit: declared.unit.clone(),
                offset,
                reason,
            },
        })
    }
}

/// A non-negative integer offset value.
fn offset(value: &Value, reference: &Reference) -> Result<u64, ResolveError> {
    match value {
        Value::Int(offset) => u64::try_from(*offset).map_err(|_| {
            fidelity(
                reference,
                value,
                unsigned_offset(),
                FidelityReason::Overflow,
            )
        }),
        _ => Err(fidelity(
            reference,
            value,
            unsigned_offset(),
            FidelityReason::NotANumber,
        )),
    }
}

const fn unsigned_offset() -> ScalarKind {
    ScalarKind::Int {
        bits: 64,
        signed: false,
    }
}

/// The exact bytes of a local text value: a string's UTF-8 bytes or a byte
/// string's bytes.
fn text_bytes(value: &Value, reference: &Reference) -> Result<Vec<u8>, ResolveError> {
    match value {
        Value::Str(text) => Ok(text.as_bytes().to_vec()),
        Value::Bytes(bytes) => Ok(bytes.clone()),
        other => Err(fidelity(
            reference,
            other,
            ScalarKind::Str,
            FidelityReason::InvalidEncoding,
        )),
    }
}

/// A value of a holder of `reference` that its form cannot read.
fn fidelity(
    reference: &Reference,
    value: &Value,
    target: ScalarKind,
    reason: FidelityReason,
) -> ResolveError {
    ResolveError::Fidelity(ValueFidelityError {
        site: panproto_inst::ValueSite::new(reference.holder.clone(), None),
        lexeme: lexeme(value),
        target,
        reason,
    })
}

/// A deterministic spelling of a value for an error report: a scalar's
/// text, or the kind of a composite value.
#[must_use]
pub fn lexeme(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Str(value) | Value::Token(value) | Value::CidLink(value) => value.clone(),
        Value::Null => "null".to_owned(),
        Value::Decimal(value) => value.to_string(),
        Value::BigInt(value) => value.to_string(),
        Value::Temporal(value) => value.to_string(),
        Value::LabeledNull(label) => format!("labeled null {label}"),
        Value::Bytes(_) => "bytes".to_owned(),
        Value::Blob { .. } => "blob".to_owned(),
        Value::Opaque { .. } => "opaque".to_owned(),
        Value::Unknown(_) => "record".to_owned(),
        Value::List(_) => "list".to_owned(),
    }
}

/// The outcome of every occurrence, in the order of `occurrences`.
///
/// A key lookup takes the bucket of the holder's scope and key bytes in the
/// target key's index; a holder outside a `Within` key's scope has no
/// bucket and dangles. A self-binding variable's first occurrence of a name
/// in its scope, in document order, is the name's definition, and every
/// later one resolves to that definition's holder. Path candidates come
/// from the provider. A materialized span always resolves to its span.
pub fn resolve(
    source: &Source<'_>,
    occurrences: &[Occurrence<'_>],
) -> Result<Vec<Outcome>, ResolveError> {
    let mut indexes: BTreeMap<(Name, Name), KeyIndex> = BTreeMap::new();
    let mut outcomes: Vec<Outcome> = Vec::with_capacity(occurrences.len());
    let mut bindings: Vec<usize> = Vec::new();
    for (position, occurrence) in occurrences.iter().enumerate() {
        let outcome = match &occurrence.lookup {
            Lookup::Key {
                scope: ScopeOf::Outside,
                ..
            } => Outcome::Dangling,
            Lookup::Key {
                vertex,
                key,
                scope,
                values,
            } => {
                let scope = match scope {
                    ScopeOf::Within(scope) => Some(scope),
                    ScopeOf::Whole | ScopeOf::Outside => None,
                };
                let index = match indexes.entry((vertex.clone(), key.id.clone())) {
                    std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                        KeyIndex::build(source, vertex, key)
                            .map_err(|error| key_equality_error(key, error))?,
                    ),
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                };
                let bytes = canonical_key_bytes(key, values)
                    .map_err(|error| key_equality_error(key, error))?;
                of_candidates(index.lookup(scope, &bytes))
            }
            Lookup::Candidates(candidates) => of_candidates(candidates.clone()),
            Lookup::Span(_) => Outcome::Span,
            Lookup::Unselected(value) => Outcome::Unselected(value.clone()),
            Lookup::Binding { .. } => {
                // Decided below, in document order; every binding position
                // is overwritten there.
                bindings.push(position);
                Outcome::Definition
            }
        };
        outcomes.push(outcome);
    }
    if bindings.is_empty() {
        return Ok(outcomes);
    }
    let order: BTreeMap<ViewRef, usize> = source
        .document_order()
        .into_iter()
        .enumerate()
        .map(|(rank, element)| (element, rank))
        .collect();
    bindings.sort_by_key(|&position| {
        let occurrence = &occurrences[position];
        (
            occurrence.compiled.reference.id.clone(),
            order.get(&occurrence.holder).copied(),
            occurrence.index,
        )
    });
    let mut definitions: BTreeMap<(Name, Option<ViewRef>, Vec<u8>), ViewRef> = BTreeMap::new();
    for position in bindings {
        let occurrence = &occurrences[position];
        let Lookup::Binding {
            scope,
            bytes,
            binding,
        } = &occurrence.lookup
        else {
            continue;
        };
        let scope = match scope {
            ScopeOf::Outside => {
                outcomes[position] = Outcome::Dangling;
                continue;
            }
            ScopeOf::Whole => None,
            ScopeOf::Within(scope) => Some(scope.clone()),
        };
        let bucket = (
            occurrence.compiled.reference.id.clone(),
            scope,
            bytes.clone(),
        );
        if let Some(defined) = definitions.get(&bucket).cloned() {
            outcomes[position] = Outcome::Resolved(defined);
            if matches!(binding, VariableBinding::NearestPreceding) {
                definitions.insert(bucket, occurrence.holder.clone());
            }
        } else {
            definitions.insert(bucket, occurrence.holder.clone());
            outcomes[position] = Outcome::Definition;
        }
    }
    Ok(outcomes)
}

fn key_equality_error(key: &Key, error: KeyingError) -> ResolveError {
    let reason = match error {
        KeyingError::UnsupportedCollation(collation) => {
            KeyEqualityReason::UnsupportedCollation(collation)
        }
        KeyingError::EqualityArity { expected, found } => {
            KeyEqualityReason::EqualityArity { expected, found }
        }
        KeyingError::InvalidNumericRepresentation { kind } => {
            KeyEqualityReason::InvalidNumericRepresentation {
                kind: kind.to_owned(),
            }
        }
    };
    ResolveError::KeyEquality {
        key: key.id.clone(),
        reason,
    }
}

fn of_candidates(candidates: Vec<ViewRef>) -> Outcome {
    match candidates.as_slice() {
        [] => Outcome::Dangling,
        [target] => Outcome::Resolved(target.clone()),
        _ => Outcome::Ambiguous(candidates),
    }
}

/// The key an unresolved occurrence is reported by: its key tuple, or its
/// literal.
#[must_use]
pub fn reported_key(occurrence: &Occurrence<'_>) -> Vec<Value> {
    match &occurrence.lookup {
        Lookup::Key { values, .. } => values.clone(),
        Lookup::Binding { .. }
        | Lookup::Candidates(_)
        | Lookup::Span(_)
        | Lookup::Unselected(_) => {
            vec![occurrence.literal.clone()]
        }
    }
}

/// Apply the unresolved precedence to every occurrence.
///
/// An occurrence that dangles or is ambiguous is kept, to be recorded as
/// unresolved, when its reference says `Complement`, or says `Error` under
/// `lenient`; under `Error` without `lenient` the first such occurrence in
/// `(holder, reference, index)` order is returned as
/// [`ResolveError::DanglingReference`] or
/// [`ResolveError::AmbiguousReference`].
///
/// # Errors
///
/// As above.
pub fn admit(
    occurrences: Vec<Occurrence<'_>>,
    outcomes: Vec<Outcome>,
    lenient: bool,
) -> Result<Vec<(Occurrence<'_>, Outcome)>, ResolveError> {
    let mut admitted = Vec::with_capacity(occurrences.len());
    for (occurrence, outcome) in occurrences.into_iter().zip(outcomes) {
        let reference = &occurrence.compiled.reference;
        let strict = reference.on_unresolved == OnUnresolved::Error && !lenient;
        match &outcome {
            Outcome::Dangling if strict => {
                return Err(ResolveError::DanglingReference {
                    reference: reference.id.clone(),
                    at: Box::new(occurrence.holder.clone()),
                    key: reported_key(&occurrence),
                });
            }
            Outcome::Ambiguous(candidates) if strict => {
                return Err(ResolveError::AmbiguousReference {
                    reference: reference.id.clone(),
                    at: Box::new(occurrence.holder.clone()),
                    key: reported_key(&occurrence),
                    candidates: candidates.clone(),
                });
            }
            _ => admitted.push((occurrence, outcome)),
        }
    }
    Ok(admitted)
}
