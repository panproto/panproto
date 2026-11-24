//! Protocol environments: which protocol each name means.
//!
//! An environment is a finite partial function from names to protocols, each
//! protocol given by the [`ObjectId`] of its [`Object::Protocol`]. A commit
//! records one, and staging data against a schema reads the schema's protocol
//! name in it. Committed data does not: a data set holds the identity of its
//! own protocol ([`DataSetObject::protocol_id`](crate::DataSetObject)).
//!
//! # Edits and their merge
//!
//! Fix a base environment `b`. An *edit* of `b` is a finite partial function
//! `e` from names to protocols with `e(n) != b(n)` wherever `e(n)` is
//! defined; applying it yields `b` overridden by `e`, and every environment
//! that defines at least the names of `b` is `b` with exactly one edit
//! applied. Edits ordered by inclusion form a poset, whose least element is
//! the empty edit, and a poset is a category with at most one morphism
//! between two objects.
//!
//! [`ProtocolEnvironment::merge`] is the pushout, in that category, of
//! `ours <- empty -> theirs`. If the two edits differ at a name both define,
//! nothing contains both, so there is no cocone and no pushout. Otherwise
//! their union is a partial function; it contains both, and any edit that
//! contains both contains it, so it is their join, which in a poset is the
//! pushout over the least element. A conflict is thus reported at exactly the
//! names that obstruct the pushout, and a merge without one is the pushout.
//! Being a union, it is commutative, associative and idempotent, with the
//! empty edit its unit.
//!
//! Protocols are compared by identity alone. Two different protocols under
//! one name are never combined into a third, under which no record was ever
//! checked.
//!
//! [`ProtocolEnvironment::updated`] is composition of edits, not a merge: the
//! later edit wins where both define a name.

use std::collections::BTreeMap;

use panproto_schema::Protocol;
use serde::{Deserialize, Serialize};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::merge::MergeConflict;
use crate::object::Object;
use crate::store::Store;

/// A finite partial function from protocol names to protocol objects.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolEnvironment(BTreeMap<String, ObjectId>);

impl ProtocolEnvironment {
    /// The environment that defines no name.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Whether no name is defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The protocol object `name` means, if it is defined.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<ObjectId> {
        self.0.get(name).copied()
    }

    /// Each name with the protocol object it means, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, ObjectId)> {
        self.0.iter().map(|(name, id)| (name.as_str(), *id))
    }

    /// Define `name` as `protocol`, in place of what it meant.
    pub fn define(&mut self, name: impl Into<String>, protocol: ObjectId) {
        self.0.insert(name.into(), protocol);
    }

    /// This environment with every definition of `edit` in place of its own.
    #[must_use]
    pub fn updated(&self, edit: &Self) -> Self {
        let mut updated = self.0.clone();
        updated.extend(edit.0.iter().map(|(name, id)| (name.clone(), *id)));
        Self(updated)
    }

    /// The pushout of the edits that take `base` to `ours` and to `theirs`.
    ///
    /// # Errors
    ///
    /// Every name the two edits define differently, in name order.
    pub fn merge(base: &Self, ours: &Self, theirs: &Self) -> Result<Self, Vec<MergeConflict>> {
        let ours_edit = ours.edit_from(base);
        let theirs_edit = theirs.edit_from(base);
        let conflicts: Vec<MergeConflict> = ours_edit
            .iter()
            .filter_map(|(name, ours)| {
                let theirs = theirs_edit.get(name).filter(|theirs| *theirs != ours)?;
                Some(MergeConflict::BothModifiedProtocol {
                    name: name.to_owned(),
                    ours,
                    theirs,
                })
            })
            .collect();
        if conflicts.is_empty() {
            Ok(base.updated(&ours_edit).updated(&theirs_edit))
        } else {
            Err(conflicts)
        }
    }

    /// The edit that takes `base` to this environment.
    fn edit_from(&self, base: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|(name, id)| base.get(name) != Some(**id))
                .map(|(name, id)| (name.clone(), *id))
                .collect(),
        )
    }

    /// The protocol `name` means to data staged in this environment: the one
    /// it defines, else the built-in protocol of that name or alias.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownProtocol`] when there is neither, and the errors of
    /// [`load_protocol`].
    pub fn resolve(&self, store: &dyn Store, name: &str) -> Result<Protocol, VcsError> {
        if let Some(id) = self.get(name) {
            return load_protocol(store, id);
        }
        panproto_protocols::registry::descriptor(name)
            .map(|descriptor| (descriptor.protocol)())
            .ok_or_else(|| VcsError::UnknownProtocol {
                name: name.to_owned(),
            })
    }
}

/// The environment recorded at `id`, the one that defines no name when there
/// is none.
///
/// A commit made before 0.78.0 recorded a lone [`Object::Protocol`] here,
/// which is the environment that defines its name.
///
/// # Errors
///
/// A store error, or [`VcsError::WrongObjectType`] when `id` names another
/// kind of object.
pub fn load_environment(
    store: &dyn Store,
    id: Option<ObjectId>,
) -> Result<ProtocolEnvironment, VcsError> {
    let Some(id) = id else {
        return Ok(ProtocolEnvironment::new());
    };
    match store.get(&id)? {
        Object::ProtocolEnvironment(environment) => Ok(environment),
        Object::Protocol(protocol) => {
            let mut environment = ProtocolEnvironment::new();
            environment.define(protocol.name, id);
            Ok(environment)
        }
        other => Err(VcsError::WrongObjectType {
            expected: "protocol_environment",
            found: other.type_name(),
        }),
    }
}

/// Store `environment`; `None` when it defines no name.
///
/// # Errors
///
/// A store error.
pub fn store_environment(
    store: &mut dyn Store,
    environment: ProtocolEnvironment,
) -> Result<Option<ObjectId>, VcsError> {
    if environment.is_empty() {
        return Ok(None);
    }
    store
        .put(&Object::ProtocolEnvironment(environment))
        .map(Some)
}

/// The protocol object at `id`.
///
/// # Errors
///
/// A store error, or [`VcsError::WrongObjectType`] when `id` names another
/// kind of object.
pub fn load_protocol(store: &dyn Store, id: ObjectId) -> Result<Protocol, VcsError> {
    match store.get(&id)? {
        Object::Protocol(protocol) => Ok(*protocol),
        other => Err(VcsError::WrongObjectType {
            expected: "protocol",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 2] = ["p", "q"];

    fn id(value: u8) -> ObjectId {
        ObjectId::from_bytes([value; 32])
    }

    /// Every environment over two names and three protocols.
    fn environments() -> Vec<ProtocolEnvironment> {
        let values = [None, Some(1), Some(2), Some(3)];
        values
            .iter()
            .flat_map(|p| values.iter().map(move |q| [*p, *q]))
            .map(|meanings| {
                let mut environment = ProtocolEnvironment::new();
                for (name, value) in NAMES.into_iter().zip(meanings) {
                    if let Some(value) = value {
                        environment.define(name, id(value));
                    }
                }
                environment
            })
            .collect()
    }

    /// Whether `upper` contains the edit from `base` to `lower`.
    fn contains(
        base: &ProtocolEnvironment,
        lower: &ProtocolEnvironment,
        upper: &ProtocolEnvironment,
    ) -> bool {
        let upper = upper.edit_from(base);
        lower
            .edit_from(base)
            .iter()
            .all(|(name, id)| upper.get(name) == Some(id))
    }

    /// The environments that extend `base`, so that an edit takes it there.
    fn extensions(base: &ProtocolEnvironment) -> Vec<ProtocolEnvironment> {
        environments()
            .into_iter()
            .filter(|environment| base.iter().all(|(name, _)| environment.get(name).is_some()))
            .collect()
    }

    #[test]
    fn merge_is_the_pushout_and_conflicts_exactly_when_there_is_none() {
        for base in environments() {
            let objects = extensions(&base);
            for ours in &objects {
                for theirs in &objects {
                    let bounds: Vec<_> = objects
                        .iter()
                        .filter(|x| contains(&base, ours, x) && contains(&base, theirs, x))
                        .collect();
                    match ProtocolEnvironment::merge(&base, ours, theirs) {
                        Ok(merged) => {
                            assert!(bounds.contains(&&merged));
                            assert!(bounds.iter().all(|bound| contains(&base, &merged, bound)));
                        }
                        Err(_) => assert!(bounds.is_empty(), "{base:?} {ours:?} {theirs:?}"),
                    }
                }
            }
        }
    }

    #[test]
    fn conflicts_are_exactly_the_names_defined_differently() {
        for base in environments() {
            for ours in extensions(&base) {
                for theirs in extensions(&base) {
                    let expected: Vec<&str> = NAMES
                        .into_iter()
                        .filter(|name| {
                            let (b, o, t) = (base.get(name), ours.get(name), theirs.get(name));
                            o != b && t != b && o != t
                        })
                        .collect();
                    let found: Vec<String> =
                        ProtocolEnvironment::merge(&base, &ours, &theirs).map_or_else(
                            |conflicts| {
                                conflicts
                                    .into_iter()
                                    .filter_map(|conflict| match conflict {
                                        MergeConflict::BothModifiedProtocol { name, .. } => {
                                            Some(name)
                                        }
                                        _ => None,
                                    })
                                    .collect()
                            },
                            |_| Vec::new(),
                        );
                    assert_eq!(found, expected);
                }
            }
        }
    }

    #[test]
    fn merge_is_commutative_associative_and_idempotent_with_the_base_its_unit() {
        let merge = |base: &_, left: Option<&ProtocolEnvironment>, right: Option<&_>| {
            ProtocolEnvironment::merge(base, left?, right?).ok()
        };
        for base in environments() {
            let objects = extensions(&base);
            for a in &objects {
                assert_eq!(merge(&base, Some(a), Some(&base)).as_ref(), Some(a));
                assert_eq!(merge(&base, Some(a), Some(a)).as_ref(), Some(a));
                for b in &objects {
                    let ab = merge(&base, Some(a), Some(b));
                    assert_eq!(ab, merge(&base, Some(b), Some(a)));
                    for c in &objects {
                        let bc = merge(&base, Some(b), Some(c));
                        assert_eq!(
                            merge(&base, ab.as_ref(), Some(c)),
                            merge(&base, Some(a), bc.as_ref())
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn updates_compose() {
        for base in environments() {
            for first in environments() {
                for second in environments() {
                    assert_eq!(
                        base.updated(&first).updated(&second),
                        base.updated(&first.updated(&second))
                    );
                }
            }
        }
    }
}
