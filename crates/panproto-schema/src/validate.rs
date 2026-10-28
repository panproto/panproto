//! Schema validation against a protocol's rules.
//!
//! The [`validate`] function checks a finished [`Schema`] against a
//! [`Protocol`]'s edge rules, vertex kinds, and constraint sorts,
//! returning a list of all violations found.
//!
//! This is a schema-level check, not an instance validation boundary: it
//! reads no instance and reports no status. The instance validators of
//! `panproto-inst` (`validate_wtype`, `validate_graph` and `validate_table`)
//! wrap it in the `SchemaWellFormed` check of their validation plan, which
//! reports each [`ValidationError`] it returns as an ill-formed declaration:
//! an incomplete check of every validation report of the plan, never a
//! failed one, since a schema problem says nothing about the instance.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use panproto_gat::Name;

use crate::error::ValidationError;
use crate::keys::{
    Cardinality, Key, KeyEquality, KeyPart, KeyRefError, KeyRole, KeyScope, PathSyntax, RefSource,
    RefTextSource, Reference, ResolveMode, SpanEnd, UriBase, VariableBinding,
};
use crate::protocol::Protocol;
use crate::schema::{Edge, Schema};

/// Validate a schema against a protocol's structural rules.
///
/// Checks:
/// 1. All vertex kinds are recognized by the protocol.
/// 2. All edges satisfy the protocol's edge rules.
/// 3. All constraint sorts are recognized by the protocol.
/// 4. All required edges reference vertices that exist.
/// 5. All recursion points name vertices that exist, at both ends.
/// 6. Every key and reference declaration is well formed, each finding
///    reported as [`ValidationError::KeyRef`]: see [`KeyRefError`] for the
///    conditions. Target typing is exact: a reference resolves only through a
///    key declared on its own `target_vertex`, never through one declared on
///    a vertex related to it.
///
/// Returns an empty vector if the schema is valid.
///
/// This is a report rather than a gate: nothing calls it on the way into a
/// search, so a caller loading a schema from bytes has to run it. What it
/// buys is that a dangling reference is named here instead of surfacing later
/// as a part quietly missing from an induced sub-schema.
///
/// It is schema-level, not an instance validation boundary: the
/// `SchemaWellFormed` check of the instance validators of `panproto-inst`
/// wraps it and reports each finding as an incomplete check of their
/// validation reports, so an instance validator never needs it called
/// first.
#[must_use]
pub fn validate(schema: &Schema, protocol: &Protocol) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // 1. Check vertex kinds.
    if !protocol.obj_kinds.is_empty() || !protocol.edge_rules.is_empty() {
        for (id, vertex) in &schema.vertices {
            if !protocol.is_known_vertex_kind(&vertex.kind) {
                errors.push(ValidationError::InvalidVertexKind {
                    vertex: id.to_string(),
                    kind: vertex.kind.to_string(),
                });
            }
        }
    }

    // 2. Check edge rules.
    for edge in schema.edges.keys() {
        if let Some(rule) = protocol.find_edge_rule(&edge.kind) {
            // Check source kind.
            if let Some(src_vertex) = schema.vertices.get(&edge.src) {
                if !rule.src_kinds.is_empty()
                    && !rule.src_kinds.iter().any(|k| k == src_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "source kind '{}' not in permitted: {:?}",
                            src_vertex.kind, rule.src_kinds
                        ),
                    });
                }
            }
            // Check target kind.
            if let Some(tgt_vertex) = schema.vertices.get(&edge.tgt) {
                if !rule.tgt_kinds.is_empty()
                    && !rule.tgt_kinds.iter().any(|k| k == tgt_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "target kind '{}' not in permitted: {:?}",
                            tgt_vertex.kind, rule.tgt_kinds
                        ),
                    });
                }
            }
        } else if !protocol.edge_rules.is_empty() {
            errors.push(ValidationError::InvalidEdge {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
                kind: edge.kind.to_string(),
                reason: format!("unknown edge kind '{}'", edge.kind),
            });
        }
    }

    // 3. Check constraint sorts.
    if !protocol.constraint_sorts.is_empty() {
        for (vertex_id, constraints) in &schema.constraints {
            for constraint in constraints {
                if !protocol
                    .constraint_sorts
                    .iter()
                    .any(|s| s == constraint.sort.as_ref())
                {
                    errors.push(ValidationError::InvalidConstraintSort {
                        vertex: vertex_id.to_string(),
                        sort: constraint.sort.to_string(),
                    });
                }
            }
        }
    }

    // 4. Check required edges reference existing vertices.
    for (vertex_id, required_edges) in &schema.required {
        for req_edge in required_edges {
            if !schema.vertices.contains_key(&req_edge.src)
                || !schema.vertices.contains_key(&req_edge.tgt)
            {
                errors.push(ValidationError::DanglingRequiredEdge {
                    vertex: vertex_id.to_string(),
                    edge: format!("{} -> {} ({})", req_edge.src, req_edge.tgt, req_edge.kind),
                });
            }
        }
    }

    // 5. Check recursion points name existing vertices at both ends.
    //
    // Inducing a sub-schema keeps a marker only when both ends survive, so a
    // marker naming a vertex that was never there is dropped rather than
    // reported, and the resulting apex looks complete. Reporting it here is
    // what makes the difference visible before a search runs.
    for (mu, point) in &schema.recursion_points {
        if !schema.vertices.contains_key(mu) {
            errors.push(ValidationError::DanglingRecursionPoint {
                mu: mu.to_string(),
                missing: mu.to_string(),
            });
        }
        if !schema.vertices.contains_key(&point.target_vertex) {
            errors.push(ValidationError::DanglingRecursionPoint {
                mu: mu.to_string(),
                missing: point.target_vertex.to_string(),
            });
        }
    }

    // 6. Check key and reference declarations.
    errors.extend(
        declaration_errors(schema, protocol)
            .into_iter()
            .map(ValidationError::KeyRef),
    );

    errors
}

/// Every well-formedness violation of the schema's key and reference
/// declarations: keys, target sets, references, then cross-reference groups.
fn declaration_errors(schema: &Schema, protocol: &Protocol) -> Vec<KeyRefError> {
    let mut errors = Vec::new();
    let ancestry = Ancestry::new(schema);

    for (vertex, keys) in &schema.declarations.keys {
        if !schema.vertices.contains_key(vertex) {
            errors.push(KeyRefError::UnknownVertex {
                vertex: vertex.clone(),
            });
            continue;
        }
        let mut seen: BTreeSet<&Name> = BTreeSet::new();
        let mut repeated: BTreeSet<&Name> = BTreeSet::new();
        for key in keys {
            if !seen.insert(&key.id) && repeated.insert(&key.id) {
                errors.push(KeyRefError::DuplicateKeyId {
                    vertex: vertex.clone(),
                    key: key.id.clone(),
                });
            }
            key_errors(schema, vertex, key, &ancestry, &mut errors);
        }
    }

    target_set_errors(schema, &mut errors);

    let shapes_differ = protocol.syntactic_shape != protocol.semantic_shape;
    let mut guards = Vec::new();
    let mut binder_modes: BTreeMap<(Name, Name), Vec<(Name, VariableBinding)>> = BTreeMap::new();
    for reference in schema.declarations.references.values() {
        reference_errors(
            schema,
            reference,
            shapes_differ,
            &ancestry,
            &mut guards,
            &mut binder_modes,
            &mut errors,
        );
    }
    binder_mode_errors(schema, binder_modes, &mut errors);
    guard_errors(&guards, &mut errors);

    errors
}

/// The violations of one key declared on `vertex`.
fn key_errors(
    schema: &Schema,
    vertex: &Name,
    key: &Key,
    ancestry: &Ancestry<'_>,
    errors: &mut Vec<KeyRefError>,
) {
    if !key.equality.is_empty() && key.equality.len() != key.parts.len() {
        errors.push(KeyRefError::EqualityArity {
            vertex: vertex.clone(),
            key: key.id.clone(),
            expected: key.parts.len(),
            found: key.equality.len(),
        });
    }
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => check_path(schema, vertex, path, errors),
            KeyPart::Position { within } => {
                if !schema.edges.contains_key(within) || within.tgt != *vertex {
                    errors.push(KeyRefError::PathDoesNotResolve {
                        owner: vertex.clone(),
                        path: vec![within.clone()],
                    });
                } else if !schema.orderings.contains_key(within) {
                    errors.push(KeyRefError::PositionNotOrdered {
                        key: key.id.clone(),
                        edge: within.clone(),
                    });
                }
            }
        }
    }
    if let KeyScope::Within(ancestor) = &key.scope {
        if !schema.vertices.contains_key(ancestor) {
            errors.push(KeyRefError::UnknownVertex {
                vertex: ancestor.clone(),
            });
        } else if !ancestry.is_proper_ancestor(ancestor, vertex) {
            errors.push(KeyRefError::WithinNotAncestor {
                key: key.id.clone(),
                vertex: ancestor.clone(),
            });
        }
    }
    if key.role == KeyRole::SortKey {
        let ordered = match &key.scope {
            KeyScope::Within(container) => {
                let carriers: Vec<_> = schema
                    .edges
                    .keys()
                    .filter(|edge| edge.src == *container && edge.tgt == *vertex)
                    .collect();
                carriers.len() == 1 && schema.orderings.contains_key(carriers[0])
            }
            KeyScope::Document | KeyScope::Global => false,
        };
        if !ordered {
            errors.push(KeyRefError::SortKeyNotOrdered {
                key: key.id.clone(),
                vertex: vertex.clone(),
            });
        }
    }
}

fn expanded_equalities(key: &Key) -> Option<Vec<KeyEquality>> {
    if key.equality.is_empty() {
        Some(vec![KeyEquality::Representation; key.parts.len()])
    } else if key.equality.len() == key.parts.len() {
        Some(key.equality.clone())
    } else {
        None
    }
}

fn target_set_errors(schema: &Schema, errors: &mut Vec<KeyRefError>) {
    for (set_id, set) in &schema.declarations.target_sets {
        if schema.vertices.contains_key(set_id) {
            errors.push(KeyRefError::TargetSetNameClash {
                set: set_id.clone(),
            });
        }
        if set.members.is_empty() {
            errors.push(KeyRefError::EmptyTargetSet {
                set: set_id.clone(),
            });
            continue;
        }
        let mut seen = BTreeSet::new();
        let mut signature: Option<(usize, Vec<KeyEquality>)> = None;
        for (vertex, key_id) in &set.members {
            let member = schema.declarations.key(vertex, key_id);
            let this = member.and_then(|key| {
                expanded_equalities(key).map(|equality| (key.parts.len(), equality))
            });
            let base_valid = member.is_some_and(|key| key.scope == set.scope)
                && seen.insert((vertex, key_id))
                && this.is_some();
            let agrees = signature
                .as_ref()
                .is_none_or(|expected| this.as_ref() == Some(expected));
            let valid = base_valid && agrees;
            if base_valid && signature.is_none() {
                signature = this;
            }
            if !valid {
                errors.push(KeyRefError::TargetSetMemberMismatch {
                    set: set_id.clone(),
                    vertex: vertex.clone(),
                    key: key_id.clone(),
                });
            }
        }
    }
}

fn target_members<'a>(schema: &'a Schema, reference: &'a Reference) -> Vec<(&'a Name, &'a Key)> {
    if let Some(set) = schema.declarations.target_set(&reference.target_vertex) {
        return set
            .members
            .iter()
            .filter_map(|(vertex, key_id)| {
                schema
                    .declarations
                    .key(vertex, key_id)
                    .map(|key| (vertex, key))
            })
            .collect();
    }
    let key = match &reference.source {
        RefSource::Variable { binder_key, .. } => schema
            .declarations
            .key(&reference.target_vertex, binder_key),
        _ => reference.target_key.as_ref().map_or_else(
            || {
                schema
                    .declarations
                    .keys_of(&reference.target_vertex)
                    .iter()
                    .find(|key| key.role == KeyRole::Primary)
            },
            |key_id| schema.declarations.key(&reference.target_vertex, key_id),
        ),
    };
    key.map_or_else(Vec::new, |key| vec![(&reference.target_vertex, key)])
}

type GuardDeclaration<'a> = (
    &'a Name,
    &'a RefSource,
    &'a [Edge],
    &'a Name,
    &'a panproto_expr::Value,
);

/// The violations of one reference.
// Keeping the source, edge, text, scope, and group checks together makes the
// full declaration predicate auditable against `KeyRefError`.
#[allow(clippy::too_many_lines)]
fn reference_errors<'a>(
    schema: &'a Schema,
    reference: &'a Reference,
    shapes_differ: bool,
    ancestry: &Ancestry<'a>,
    guards: &mut Vec<GuardDeclaration<'a>>,
    binder_modes: &mut BTreeMap<(Name, Name), Vec<(Name, VariableBinding)>>,
    errors: &mut Vec<KeyRefError>,
) {
    let holder_known = schema.vertices.contains_key(&reference.holder);
    let targets_set = schema
        .declarations
        .target_set(&reference.target_vertex)
        .is_some();
    let target_known = targets_set || schema.vertices.contains_key(&reference.target_vertex);
    if !holder_known {
        errors.push(KeyRefError::UnknownVertex {
            vertex: reference.holder.clone(),
        });
    }
    if !target_known {
        errors.push(KeyRefError::UnknownVertex {
            vertex: reference.target_vertex.clone(),
        });
    }
    if !holder_known || !target_known {
        return;
    }

    if let Some(null_vertex) = &reference.null_vertex
        && !schema.vertices.contains_key(null_vertex)
    {
        errors.push(KeyRefError::UnknownVertex {
            vertex: null_vertex.clone(),
        });
    }

    let declarations = &schema.declarations;
    if targets_set {
        if reference.target_key.is_some() {
            errors.push(KeyRefError::TargetSetHasKey {
                reference: reference.id.clone(),
            });
        }
    } else if let Some(id) = &reference.target_key
        && declarations.key(&reference.target_vertex, id).is_none()
    {
        errors.push(KeyRefError::UnknownTargetKey {
            reference: reference.id.clone(),
            key: id.clone(),
        });
    }

    let targets = target_members(schema, reference);
    if targets.is_empty() {
        match &reference.source {
            RefSource::Variable { binder_key, .. } => {
                errors.push(KeyRefError::UnknownTargetKey {
                    reference: reference.id.clone(),
                    key: binder_key.clone(),
                });
            }
            _ if !targets_set && reference.target_key.is_some() => {
                // The named-key error was appended above.
            }
            _ => errors.push(KeyRefError::UnknownTarget {
                reference: reference.id.clone(),
            }),
        }
        return;
    }
    for (_, key) in targets
        .iter()
        .filter(|(_, key)| matches!(key.role, KeyRole::SortKey | KeyRole::Index))
    {
        errors.push(KeyRefError::KeyNotResolvable {
            reference: reference.id.clone(),
            key: key.id.clone(),
        });
    }

    for path in source_paths(&reference.source) {
        check_path(schema, &reference.holder, path, errors);
    }
    if let Some(guard) = &reference.guard {
        check_path(schema, &reference.holder, &guard.field, errors);
        guards.push((
            &reference.holder,
            &reference.source,
            &guard.field,
            &reference.id,
            &guard.equals,
        ));
    }
    match &reference.source {
        RefSource::KeyValue { parts } => {
            let unknown_named_key = !targets_set
                && reference
                    .target_key
                    .as_ref()
                    .is_some_and(|id| declarations.key(&reference.target_vertex, id).is_none());
            if !unknown_named_key {
                let expected = targets.first().map_or(0, |(_, key)| key.parts.len());
                if expected != parts.len() {
                    errors.push(KeyRefError::ArityMismatch {
                        reference: reference.id.clone(),
                        expected,
                        found: parts.len(),
                    });
                }
            }
        }
        RefSource::Variable {
            binder_key,
            binding,
            ..
        } => {
            match declarations.key(&reference.target_vertex, binder_key) {
                None => errors.push(KeyRefError::UnknownTargetKey {
                    reference: reference.id.clone(),
                    key: binder_key.clone(),
                }),
                Some(key) if key.role != KeyRole::Identifier => {
                    errors.push(KeyRefError::BinderNotIdentifier {
                        reference: reference.id.clone(),
                        key: binder_key.clone(),
                    });
                }
                Some(_) => {}
            }
            binder_modes
                .entry((reference.target_vertex.clone(), binder_key.clone()))
                .or_default()
                .push((reference.id.clone(), *binding));
        }
        RefSource::Inline { .. } => {
            if reference.mode != ResolveMode::Redirect {
                errors.push(KeyRefError::InlineRequiresRedirect {
                    reference: reference.id.clone(),
                });
            }
            if !targets_set {
                errors.push(KeyRefError::InlineRequiresTargetSet {
                    reference: reference.id.clone(),
                });
            }
        }
        RefSource::Standoff {
            end_inclusive,
            end_form,
            fragments,
            ..
        } => {
            if *end_form == SpanEnd::Length && *end_inclusive {
                errors.push(KeyRefError::LengthInclusive {
                    reference: reference.id.clone(),
                });
            }
            if let Some(edge) = fragments
                && (reference.cardinality != Cardinality::Many
                    || edge.src != reference.holder
                    || !schema.orderings.contains_key(edge))
            {
                errors.push(KeyRefError::InvalidFragments {
                    reference: reference.id.clone(),
                });
            }
        }
        RefSource::Path { .. } => {}
    }

    if let Some(edge) = &reference.edge {
        if let Some(set) = declarations.target_set(&reference.target_vertex) {
            let valid = edge.src == reference.holder
                && edge.tgt == reference.target_vertex
                && set.members.iter().all(|(vertex, _)| {
                    let mut member = edge.clone();
                    member.tgt.clone_from(vertex);
                    schema.edges.contains_key(&member)
                });
            if !valid {
                errors.push(KeyRefError::EdgeMismatch {
                    reference: reference.id.clone(),
                    edge: edge.clone(),
                });
            }
        } else if edge.src != reference.holder || edge.tgt != reference.target_vertex {
            errors.push(KeyRefError::EdgeMismatch {
                reference: reference.id.clone(),
                edge: edge.clone(),
            });
        } else if !schema.edges.contains_key(edge) {
            errors.push(KeyRefError::PathDoesNotResolve {
                owner: reference.holder.clone(),
                path: vec![edge.clone()],
            });
        }
    } else if shapes_differ {
        errors.push(KeyRefError::EdgeRequired {
            reference: reference.id.clone(),
        });
    }

    validate_text_reference(schema, reference, errors);
    validate_scope_via(schema, reference, ancestry, errors);
}

fn binder_mode_errors(
    schema: &Schema,
    binder_modes: BTreeMap<(Name, Name), Vec<(Name, VariableBinding)>>,
    errors: &mut Vec<KeyRefError>,
) {
    for ((vertex, key), uses) in binder_modes {
        let has_first = uses
            .iter()
            .any(|(_, mode)| *mode == VariableBinding::FirstDefines);
        let has_nearest = uses
            .iter()
            .any(|(_, mode)| *mode == VariableBinding::NearestPreceding);
        let targeted_elsewhere = schema.declarations.references.values().any(|reference| {
            let directly_targets =
                reference.target_vertex == vertex && reference.target_key.as_ref() == Some(&key);
            let targets_through_set = schema
                .declarations
                .target_set(&reference.target_vertex)
                .is_some_and(|set| {
                    set.members.iter().any(|(member_vertex, member_key)| {
                        member_vertex == &vertex && member_key == &key
                    })
                });
            (directly_targets || targets_through_set)
                && !matches!(
                    &reference.source,
                    RefSource::Variable { binder_key, .. } if binder_key == &key
                )
        });
        if (has_first && has_nearest) || targeted_elsewhere {
            errors.push(KeyRefError::BinderModeConflict {
                key,
                references: uses.into_iter().map(|(id, _)| id).collect(),
            });
        }
    }
}

fn guard_errors(guards: &[GuardDeclaration<'_>], errors: &mut Vec<KeyRefError>) {
    for (index, (holder, source, field, _, _)) in guards.iter().enumerate() {
        if guards[..index].iter().any(
            |(candidate_holder, candidate_source, candidate_field, _, _)| {
                candidate_holder == holder && candidate_source == source && candidate_field == field
            },
        ) {
            continue;
        }
        let group: Vec<_> = guards
            .iter()
            .filter(
                |(candidate_holder, candidate_source, candidate_field, _, _)| {
                    candidate_holder == holder
                        && candidate_source == source
                        && candidate_field == field
                },
            )
            .map(|(_, _, _, id, value)| (*id, *value))
            .collect();
        let duplicates: Vec<Name> = group
            .iter()
            .filter(|(_, value)| {
                group
                    .iter()
                    .filter(|(_, other)| value.canonical_cmp(other).is_eq())
                    .count()
                    > 1
            })
            .map(|(id, _)| (*id).clone())
            .collect();
        if !duplicates.is_empty() {
            errors.push(KeyRefError::DuplicateGuard {
                references: duplicates,
            });
        }
    }
}

fn validate_text_reference(schema: &Schema, reference: &Reference, errors: &mut Vec<KeyRefError>) {
    let RefSource::Standoff { text, .. } = &reference.source else {
        return;
    };
    let named = match text {
        RefTextSource::Referenced {
            reference: named, ..
        }
        | RefTextSource::Span { reference: named } => named,
        RefTextSource::Local(_)
        | RefTextSource::External(_)
        | RefTextSource::ExternalUri { .. } => return,
    };
    let mut valid = schema
        .declarations
        .references
        .get(named)
        .is_some_and(|other| {
            if other.holder != reference.holder || other.cardinality != Cardinality::One {
                return false;
            }
            match text {
                RefTextSource::Referenced { text, external, .. } => {
                    if !matches!(
                        other.source,
                        RefSource::KeyValue { .. }
                            | RefSource::Path { .. }
                            | RefSource::Variable { .. }
                    ) || schema
                        .declarations
                        .target_set(&other.target_vertex)
                        .is_some()
                    {
                        return false;
                    }
                    let mut findings = Vec::new();
                    check_path(schema, &other.target_vertex, text, &mut findings);
                    if let Some(external) = external {
                        check_path(schema, &other.target_vertex, external, &mut findings);
                    }
                    findings.is_empty()
                }
                RefTextSource::Span { .. } => matches!(
                    other.source,
                    RefSource::Standoff {
                        fragments: None,
                        ..
                    }
                ),
                RefTextSource::Local(_)
                | RefTextSource::External(_)
                | RefTextSource::ExternalUri { .. } => true,
            }
        });
    valid &= !text_chain_is_cyclic(schema, named, &mut BTreeSet::from([reference.id.clone()]));
    if !valid {
        errors.push(KeyRefError::TextReferenceInvalid {
            reference: reference.id.clone(),
            text_reference: named.clone(),
        });
    }
}

fn text_chain_is_cyclic(schema: &Schema, at: &Name, active: &mut BTreeSet<Name>) -> bool {
    if !active.insert(at.clone()) {
        return true;
    }
    let Some(RefSource::Standoff { text, .. }) = schema
        .declarations
        .references
        .get(at)
        .map(|reference| &reference.source)
    else {
        active.remove(at);
        return false;
    };
    let cyclic = match text {
        RefTextSource::Referenced { reference, .. } | RefTextSource::Span { reference } => {
            text_chain_is_cyclic(schema, reference, active)
        }
        RefTextSource::Local(_)
        | RefTextSource::External(_)
        | RefTextSource::ExternalUri { .. } => false,
    };
    active.remove(at);
    cyclic
}

fn validate_scope_via(
    schema: &Schema,
    reference: &Reference,
    ancestry: &Ancestry<'_>,
    errors: &mut Vec<KeyRefError>,
) {
    let Some(via) = &reference.scope_via else {
        return;
    };
    let expected_scope = schema
        .declarations
        .target_set(&reference.target_vertex)
        .map(|set| &set.scope)
        .or_else(|| {
            target_members(schema, reference)
                .first()
                .map(|(_, key)| &key.scope)
        });
    let Some(KeyScope::Within(scope_vertex)) = expected_scope else {
        errors.push(KeyRefError::ScopeViaInvalid {
            reference: reference.id.clone(),
        });
        return;
    };
    let mut vertex = &via.ancestor;
    let mut seen = BTreeSet::new();
    let mut valid = !via.hops.is_empty()
        && schema.vertices.contains_key(&via.ancestor)
        && ancestry.is_proper_ancestor(&via.ancestor, &reference.holder);
    for hop in &via.hops {
        let Some(declaration) = schema.declarations.references.get(hop) else {
            valid = false;
            break;
        };
        valid &= seen.insert(hop)
            && declaration.holder == *vertex
            && declaration.cardinality == Cardinality::One
            && schema
                .declarations
                .target_set(&declaration.target_vertex)
                .is_none();
        vertex = &declaration.target_vertex;
    }
    valid &= vertex == scope_vertex;
    if !valid {
        errors.push(KeyRefError::ScopeViaInvalid {
            reference: reference.id.clone(),
        });
    }
}

/// Every edge path a reference source reads, each rooted at the holder.
fn source_paths(source: &RefSource) -> Vec<&[Edge]> {
    match source {
        RefSource::KeyValue { parts } => parts.iter().map(Vec::as_slice).collect(),
        RefSource::Path { field, syntax } => {
            let mut paths = vec![field.as_slice()];
            if let PathSyntax::Uri {
                base: UriBase::Field(base),
            } = syntax
            {
                paths.push(base.as_slice());
            }
            paths
        }
        RefSource::Standoff {
            text,
            start,
            end,
            fragments,
            ..
        } => {
            let mut paths = vec![start.as_slice(), end.as_slice()];
            match text {
                RefTextSource::Local(path) | RefTextSource::External(path) => {
                    paths.push(path.as_slice());
                }
                RefTextSource::ExternalUri { field, base } => {
                    paths.push(field.as_slice());
                    if let UriBase::Field(base) = base {
                        paths.push(base.as_slice());
                    }
                }
                RefTextSource::Referenced { .. } | RefTextSource::Span { .. } => {}
            }
            if let Some(edge) = fragments {
                paths.push(std::slice::from_ref(edge));
            }
            paths
        }
        RefSource::Variable { field, .. } | RefSource::Inline { field } => {
            vec![field.as_slice()]
        }
    }
}

/// Report `path` unless it is a chain of schema edges starting at `owner`.
///
/// The empty path resolves: it names the owner itself.
fn check_path(schema: &Schema, owner: &Name, path: &[Edge], errors: &mut Vec<KeyRefError>) {
    let mut at = owner;
    for edge in path {
        if edge.src != *at || !schema.edges.contains_key(edge) {
            errors.push(KeyRefError::PathDoesNotResolve {
                owner: owner.clone(),
                path: path.to_vec(),
            });
            return;
        }
        at = &edge.tgt;
    }
}

/// The proper ancestors of vertices along schema edges, computed on demand.
pub struct Ancestry<'a> {
    /// Sources of the edges into each vertex.
    parents: HashMap<&'a Name, Vec<&'a Name>>,
}

impl<'a> Ancestry<'a> {
    /// Index the edges of `schema` by target.
    pub fn new(schema: &'a Schema) -> Self {
        let mut parents: HashMap<&'a Name, Vec<&'a Name>> = HashMap::new();
        for edge in schema.edges.keys() {
            parents.entry(&edge.tgt).or_default().push(&edge.src);
        }
        Self { parents }
    }

    /// Whether a path of one or more edges runs from `ancestor` to `vertex`.
    pub fn is_proper_ancestor(&self, ancestor: &Name, vertex: &Name) -> bool {
        let mut seen: BTreeSet<&Name> = BTreeSet::new();
        let mut queue: VecDeque<&Name> = VecDeque::new();
        queue.push_back(vertex);
        while let Some(at) = queue.pop_front() {
            for parent in self.parents.get(at).map_or(&[][..], Vec::as_slice) {
                if *parent == ancestor {
                    return true;
                }
                if seen.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::protocol::{EdgeRule, Protocol};

    fn atproto_protocol() -> Protocol {
        Protocol {
            name: "atproto".to_owned(),
            schema_theory: "ThATProtoSchema".to_owned(),
            instance_theory: "ThWType".to_owned(),
            edge_rules: vec![
                EdgeRule {
                    edge_kind: "record-schema".to_owned(),
                    src_kinds: vec!["record".to_owned()],
                    tgt_kinds: vec!["object".to_owned()],
                },
                EdgeRule {
                    edge_kind: "prop".to_owned(),
                    src_kinds: vec!["object".to_owned()],
                    tgt_kinds: vec![
                        "string".to_owned(),
                        "integer".to_owned(),
                        "object".to_owned(),
                        "boolean".to_owned(),
                    ],
                },
            ],
            obj_kinds: vec![
                "record".to_owned(),
                "object".to_owned(),
                "string".to_owned(),
                "integer".to_owned(),
                "boolean".to_owned(),
            ],
            constraint_sorts: vec![
                "maxLength".to_owned(),
                "minLength".to_owned(),
                "format".to_owned(),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn valid_schema_passes() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("post", "record", Some("app.bsky.feed.post"))
            .expect("vertex")
            .vertex("post:body", "object", None)
            .expect("vertex")
            .vertex("post:body.text", "string", None)
            .expect("vertex")
            .edge("post", "post:body", "record-schema", None)
            .expect("edge")
            .edge("post:body", "post:body.text", "prop", Some("text"))
            .expect("edge")
            .constraint("post:body.text", "maxLength", "3000")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn invalid_constraint_sort_detected() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("v", "string", None)
            .expect("vertex")
            .constraint("v", "nonexistent_sort", "value")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidConstraintSort { .. })),
            "expected InvalidConstraintSort, got: {errors:?}"
        );
    }
}
