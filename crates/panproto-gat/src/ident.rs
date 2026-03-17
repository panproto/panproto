//! First-class identifiers separating identity from display name.
//!
//! Follows the `GATlab` design (Lynch et al., 2024): an identifier's
//! *identity* is a `(ScopeTag, index)` pair, while its *name* is
//! purely for display and serialization. Renaming an identifier does
//! not change its identity, so `HashMap<Ident, _>` entries survive
//! renames without rehashing.
//!
//! The module also provides [`Name`] (an interned string handle with
//! a pointer-equality fast path) and [`NameSite`]/[`SiteRename`] for
//! site-qualified rename operations across the 9 naming sites in
//! panproto.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ScopeTag
// ---------------------------------------------------------------------------

/// Global monotonic counter for generating unique scope tags.
static SCOPE_COUNTER: AtomicU32 = AtomicU32::new(1);

/// An opaque scope tag distinguishing different naming contexts.
///
/// Each [`Theory`](crate::Theory) or schema gets its own `ScopeTag`
/// at construction time. Two sorts named `"Vertex"` in different
/// theories have different scope tags, so their [`Ident`]s compare
/// as unequal even though their display names match.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeTag(u32);

impl ScopeTag {
    /// Generate a fresh scope tag (monotonically increasing).
    #[must_use]
    pub fn fresh() -> Self {
        Self(SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Construct from a known raw value (for deserialization or legacy data).
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The legacy scope tag (scope 0), used for deserializing old-format data.
    pub const LEGACY: Self = Self(0);
}

// ---------------------------------------------------------------------------
// Ident
// ---------------------------------------------------------------------------

/// A first-class identifier separating stable identity from display name.
///
/// **Identity** is the `(scope, index)` pair — [`PartialEq`], [`Eq`],
/// and [`Hash`] use only these two fields, making comparisons O(1)
/// regardless of name length.
///
/// **Name** is an [`Arc<str>`] used for display, serialization, and
/// human readability. Changing the name (via [`Ident::renamed`]) does
/// not change the identity.
///
/// This design follows `GATlab` (Lynch et al., 2024) where identifiers
/// consist of a scope tag (UUID in `GATlab`, monotonic u32 here), a
/// positional index, and a display name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ident {
    /// The scope this identifier belongs to.
    pub scope: ScopeTag,
    /// Positional index within the scope (0-based).
    pub index: u32,
    /// Human-readable display name. Changeable without affecting identity.
    pub name: Arc<str>,
}

impl PartialEq for Ident {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.scope == other.scope && self.index == other.index
    }
}

impl Eq for Ident {}

impl std::hash::Hash for Ident {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.index.hash(state);
    }
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.scope
            .cmp(&other.scope)
            .then(self.index.cmp(&other.index))
    }
}

impl Ident {
    /// Create a new identifier.
    #[must_use]
    pub fn new(scope: ScopeTag, index: u32, name: impl Into<Arc<str>>) -> Self {
        Self {
            scope,
            index,
            name: name.into(),
        }
    }

    /// Create a renamed copy. Same identity, different display name.
    #[must_use]
    pub fn renamed(&self, new_name: impl Into<Arc<str>>) -> Self {
        Self {
            scope: self.scope,
            index: self.index,
            name: new_name.into(),
        }
    }

    /// Construct from a legacy string identifier.
    ///
    /// Uses [`ScopeTag::LEGACY`] (scope 0) and a hash-based index
    /// derived from the name, ensuring deterministic identity for
    /// data created before the `Ident` migration.
    #[must_use]
    pub fn from_legacy(name: impl Into<Arc<str>>) -> Self {
        let name: Arc<str> = name.into();
        // Use a simple hash to derive a deterministic index from the name.
        let index = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            name.hash(&mut hasher);
            #[allow(clippy::cast_possible_truncation)]
            {
                hasher.finish() as u32
            }
        };
        Self {
            scope: ScopeTag::LEGACY,
            index,
            name,
        }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Name
// ---------------------------------------------------------------------------

/// An interned name handle with a pointer-equality fast path.
///
/// Wraps an [`Arc<str>`]. Equality checks use [`Arc::ptr_eq`] first
/// (a single pointer comparison) before falling back to string
/// comparison. This makes equality O(1) in the common case where
/// both sides originate from the same schema construction.
///
/// `Name` is a drop-in replacement for `String` in hot-path structs
/// like `Edge`, `Vertex`, and `Node`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Name(pub Arc<str>);

impl PartialEq for Name {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl Eq for Name {}

impl std::hash::Hash for Name {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the string content (not the pointer) so that equal
        // strings from different Arcs hash identically.
        self.0.hash(state);
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl Name {
    /// Create a new name from a string.
    #[must_use]
    pub fn new(s: impl Into<Arc<str>>) -> Self {
        Self(s.into())
    }

    /// Return this name as a string slice.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for Name {
    fn from(s: String) -> Self {
        Self(Arc::from(s))
    }
}

impl From<&str> for Name {
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}

impl From<Arc<str>> for Name {
    fn from(s: Arc<str>) -> Self {
        Self(s)
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl<'a> PartialEq<&'a str> for Name {
    fn eq(&self, other: &&'a str) -> bool {
        &*self.0 == *other
    }
}

impl std::ops::Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Default for Name {
    fn default() -> Self {
        Self(Arc::from(""))
    }
}

impl From<Name> for String {
    fn from(n: Name) -> Self {
        n.0.to_string()
    }
}

// ---------------------------------------------------------------------------
// NameSite and SiteRename
// ---------------------------------------------------------------------------

/// Enumerates the 9 naming sites in the panproto system.
///
/// A protolens rename can target any of these sites, providing a
/// unified renaming algebra across the entire stack.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameSite {
    /// Edge label (field/property name). Currently the only site
    /// transformable via `RenameField`.
    EdgeLabel,
    /// Vertex ID (structural identifier, e.g., `"post:body.text"`).
    VertexId,
    /// Vertex kind (type classification, e.g., `"string"`, `"object"`).
    VertexKind,
    /// Edge kind (relationship type, e.g., `"prop"`, `"field-of"`).
    EdgeKind,
    /// Namespace identifier (e.g., `"app.bsky.feed.post"`).
    Nsid,
    /// Constraint sort (validation property name, e.g., `"maxLength"`).
    ConstraintSort,
    /// Instance anchor (a node's reference to its schema vertex).
    InstanceAnchor,
    /// Theory name (e.g., `"ThATProtoSchema"`).
    TheoryName,
    /// Sort name within a theory (e.g., `"Vertex"`, `"Node"`).
    SortName,
}

/// A site-qualified rename operation.
///
/// Specifies *what* to rename (`site`), *from* (`old`), and *to* (`new`).
/// `SiteRename` values are stored in `SchemaMorphism` provenance and
/// in `CommitObject` rename metadata.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SiteRename {
    /// Which naming site this rename targets.
    pub site: NameSite,
    /// The old name.
    pub old: Arc<str>,
    /// The new name.
    pub new: Arc<str>,
}

impl SiteRename {
    /// Create a new site rename.
    #[must_use]
    pub fn new(site: NameSite, old: impl Into<Arc<str>>, new: impl Into<Arc<str>>) -> Self {
        Self {
            site,
            old: old.into(),
            new: new.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ident_equality_ignores_name() {
        let scope = ScopeTag::fresh();
        let a = Ident::new(scope, 0, "Vertex");
        let b = Ident::new(scope, 0, "Node");
        assert_eq!(
            a, b,
            "same (scope, index) should be equal regardless of name"
        );
    }

    #[test]
    fn ident_inequality_different_scope() {
        let a = Ident::new(ScopeTag::fresh(), 0, "Vertex");
        let b = Ident::new(ScopeTag::fresh(), 0, "Vertex");
        assert_ne!(
            a, b,
            "different scopes should be unequal even with same name"
        );
    }

    #[test]
    fn ident_inequality_different_index() {
        let scope = ScopeTag::fresh();
        let a = Ident::new(scope, 0, "Vertex");
        let b = Ident::new(scope, 1, "Vertex");
        assert_ne!(a, b, "different indices should be unequal");
    }

    #[test]
    fn ident_hash_consistency() {
        use std::collections::HashMap;
        let scope = ScopeTag::fresh();
        let key = Ident::new(scope, 42, "original");
        let mut map = HashMap::new();
        map.insert(key.clone(), "value");

        // Rename the key — should still find the entry.
        let renamed = key.renamed("renamed");
        assert_eq!(map.get(&renamed), Some(&"value"));
    }

    #[test]
    fn ident_renamed_preserves_identity() {
        let scope = ScopeTag::fresh();
        let a = Ident::new(scope, 5, "old_name");
        let b = a.renamed("new_name");
        assert_eq!(a, b);
        assert_eq!(b.name.as_ref(), "new_name");
    }

    #[test]
    fn ident_from_legacy_is_deterministic() {
        let a = Ident::from_legacy("post:body.text");
        let b = Ident::from_legacy("post:body.text");
        assert_eq!(a, b);
        assert_eq!(a.scope, ScopeTag::LEGACY);
        assert_eq!(a.name.as_ref(), "post:body.text");
    }

    #[test]
    fn ident_from_legacy_different_names_differ() {
        let a = Ident::from_legacy("post:body.text");
        let b = Ident::from_legacy("post:body.content");
        assert_ne!(a, b);
    }

    #[test]
    fn name_ptr_eq_fast_path() {
        let arc: Arc<str> = Arc::from("hello");
        let a = Name(Arc::clone(&arc));
        let b = Name(Arc::clone(&arc));
        // These share the same Arc, so ptr_eq should hit.
        assert_eq!(a, b);
    }

    #[test]
    fn name_string_eq_fallback() {
        let a = Name::from("hello");
        let b = Name::from(String::from("hello"));
        // Different Arcs, but same content.
        assert_eq!(a, b);
    }

    #[test]
    fn name_from_conversions() {
        let from_str: Name = "hello".into();
        let from_string: Name = String::from("hello").into();
        let from_arc: Name = Arc::<str>::from("hello").into();
        assert_eq!(from_str, from_string);
        assert_eq!(from_string, from_arc);
    }

    #[test]
    fn name_partial_eq_str() {
        let name = Name::from("test");
        assert!(name == "test");
        assert!(name == "test");
    }

    #[test]
    fn name_display() {
        let name = Name::from("my_field");
        assert_eq!(format!("{name}"), "my_field");
    }

    #[test]
    fn name_ordering() {
        let a = Name::from("alpha");
        let b = Name::from("beta");
        assert!(a < b);
    }

    #[test]
    fn site_rename_construction() {
        let rename = SiteRename::new(NameSite::EdgeLabel, "text", "body");
        assert_eq!(rename.site, NameSite::EdgeLabel);
        assert_eq!(rename.old.as_ref(), "text");
        assert_eq!(rename.new.as_ref(), "body");
    }

    #[test]
    fn ident_display() {
        let scope = ScopeTag::fresh();
        let id = Ident::new(scope, 0, "Vertex");
        assert_eq!(format!("{id}"), "Vertex");
    }

    #[test]
    fn ident_serde_roundtrip() {
        let scope = ScopeTag::fresh();
        let id = Ident::new(scope, 42, "test_sort");
        let json = serde_json::to_string(&id).unwrap();
        let restored: Ident = serde_json::from_str(&json).unwrap();
        assert_eq!(id, restored);
        assert_eq!(id.name, restored.name);
    }

    #[test]
    fn name_serde_roundtrip() {
        let name = Name::from("field_name");
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, "\"field_name\""); // transparent serialization
        let restored: Name = serde_json::from_str(&json).unwrap();
        assert_eq!(name, restored);
    }

    #[test]
    fn site_rename_serde_roundtrip() {
        let rename = SiteRename::new(NameSite::VertexKind, "string", "text");
        let json = serde_json::to_string(&rename).unwrap();
        let restored: SiteRename = serde_json::from_str(&json).unwrap();
        assert_eq!(rename, restored);
    }
}
