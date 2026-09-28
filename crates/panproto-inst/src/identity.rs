//! Durable identity for schemas, compiled lenses and the complements they
//! produce.
//!
//! A complement is only meaningful against the lens that produced it: `put`
//! reassembles a source from a view and a complement, and a complement
//! taken under one lens and handed to another reassembles garbage without
//! any error. This module gives every lens a content identity and every
//! complement a record of the lens it came from.
//!
//! # Digests
//!
//! A [`Digest`] is a 32-byte blake3 key derivation. Each kind of object is
//! hashed under its own context string, so digests of different kinds never
//! collide by construction:
//!
//! | context                       | input                                        |
//! |-------------------------------|----------------------------------------------|
//! | [`SCHEMA_CONTEXT`]            | [`panproto_schema::canonical_bytes`]         |
//! | [`LENS_CONTEXT`]              | both schema digests, then [`canonical_compiled_bytes`] |
//! | [`GRAPH_INSTANCE_CONTEXT`]    | the canonical bytes of a graph instance      |
//! | [`VCS_GRAPH_DATASET_CONTEXT`], [`VCS_GRAPH_COMPLEMENT_CONTEXT`], [`VCS_GRAPH_EDIT_LOG_CONTEXT`] | VCS objects |
//!
//! A context string names the version of its encoding. Any change to
//! `canonical_bytes`, to [`canonical_compiled_bytes`], or to an envelope
//! field bumps the context version and [`IDENTITY_ENVELOPE_VERSION`]
//! together.
//!
//! # Envelopes and status
//!
//! An [`IdentityEnvelope`] names the source schema, the target schema, the
//! compiled lens and the complement format. `get` stamps one on every
//! complement it produces. When a complement comes back to `put`, its
//! [`IdentityStatus`] is decided from the envelope, the legacy fingerprint
//! and whether the complement is empty (see [`identity_status`]), and an
//! [`IdentityPolicy`] decides whether that status is accepted
//! ([`IdentityPolicy::admit`]).
//!
//! # Legacy complements
//!
//! Complements written before envelopes existed carry only a 64-bit
//! fingerprint of the source schema, computed with the standard library's
//! `DefaultHasher`. That hasher wrote the edge count as a native-width
//! `usize`, so the fingerprint differs between 64-bit hosts and wasm32.
//! [`legacy_schema_fingerprint`] reimplements it at either width without
//! depending on the standard library's hasher, whose algorithm is not
//! guaranteed across Rust releases.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use panproto_gat::{CoercionClass, Name};
use panproto_schema::{CanonicalWriter, Schema};
use serde::de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::complement::Complement;
use crate::value::{FieldPresence, Value};
use crate::wtype::{
    CaseBranch, CompiledMigration, FieldTransform, TermAssignment, TermBranch, TermScope,
};

// ---------------------------------------------------------------------------
// Contexts.
// ---------------------------------------------------------------------------

/// The blake3 key-derivation context for a schema's identity. Its input is
/// [`panproto_schema::canonical_bytes`].
pub const SCHEMA_CONTEXT: &str = "panproto 2026-09 schema identity v1";

/// The blake3 key-derivation context for a compiled lens's identity. Its
/// input is the source schema digest, the target schema digest and
/// [`canonical_compiled_bytes`], concatenated.
pub const LENS_CONTEXT: &str = "panproto 2026-09 compiled lens identity v1";

/// The blake3 key-derivation context for a graph instance's content digest.
pub const GRAPH_INSTANCE_CONTEXT: &str = "panproto 2026-09 graph instance v1";

/// The blake3 key-derivation context for a VCS graph data set object.
pub const VCS_GRAPH_DATASET_CONTEXT: &str = "panproto 2026-09 vcs graph_dataset v1";

/// The blake3 key-derivation context for a VCS graph complement object.
pub const VCS_GRAPH_COMPLEMENT_CONTEXT: &str = "panproto 2026-09 vcs graph_complement v1";

/// The blake3 key-derivation context for a VCS graph edit log object.
pub const VCS_GRAPH_EDIT_LOG_CONTEXT: &str = "panproto 2026-09 vcs graph_editlog v1";

/// The version of the [`IdentityEnvelope`] layout.
pub const IDENTITY_ENVELOPE_VERSION: u16 = 1;

/// The version of the tree ([`ComplementShape::WType`]) complement format.
pub const TREE_COMPLEMENT_FORMAT_VERSION: u16 = 1;

/// The version of the graph ([`ComplementShape::Graph`]) complement format.
pub const GRAPH_COMPLEMENT_FORMAT_VERSION: u16 = 1;

// ---------------------------------------------------------------------------
// Digest.
// ---------------------------------------------------------------------------

/// A 32-byte content digest.
///
/// Rendered as 64 lowercase hex characters by [`Display`](fmt::Display) and
/// parsed back by [`FromStr`]. On the wire it is a hex string in
/// human-readable formats (JSON, Python) and a byte string otherwise
/// (msgpack `bin`, CBOR major type 2). Deserialization accepts any of a hex
/// string, 32 bytes, or a sequence of 32 integers, whatever the format.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    /// Derive a digest from `input` under a blake3 key-derivation `context`.
    #[must_use]
    pub fn derive(context: &str, input: &[u8]) -> Self {
        Self(blake3::derive_key(context, input))
    }

    /// The 64-character lowercase hex rendering.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({self})")
    }
}

/// A string that is not 64 hex characters.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a digest is 64 hex characters, got {0:?}")]
pub struct DigestParseError(pub String);

impl FromStr for Digest {
    type Err = DigestParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 64 {
            return Err(DigestParseError(s.to_owned()));
        }
        let nibble = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
            let (Some(hi), Some(lo)) = (nibble(pair[0]), nibble(pair[1])) else {
                return Err(DigestParseError(s.to_owned()));
            };
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DigestVisitor;

        impl<'de> Visitor<'de> for DigestVisitor {
            type Value = Digest;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 64-character hex string, 32 bytes, or 32 integers")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Digest, E> {
                v.parse().map_err(E::custom)
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Digest, E> {
                <[u8; 32]>::try_from(v)
                    .map(Digest)
                    .map_err(|_| E::invalid_length(v.len(), &self))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Digest, A::Error> {
                let mut out = [0u8; 32];
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(33, &self));
                }
                Ok(Digest(out))
            }
        }

        deserializer.deserialize_any(DigestVisitor)
    }
}

// ---------------------------------------------------------------------------
// Schema and lens digests.
// ---------------------------------------------------------------------------

/// The identity of a schema: [`SCHEMA_CONTEXT`] over its canonical bytes.
#[must_use]
pub fn schema_digest(schema: &Schema) -> Digest {
    Digest::derive(SCHEMA_CONTEXT, &panproto_schema::canonical_bytes(schema))
}

/// The identity of a compiled lens: [`LENS_CONTEXT`] over the source schema
/// digest, the target schema digest and [`canonical_compiled_bytes`].
#[must_use]
pub fn lens_digest(src: &Schema, tgt: &Schema, compiled: &CompiledMigration) -> Digest {
    lens_digest_over(&schema_digest(src), &schema_digest(tgt), compiled)
}

/// [`lens_digest`] from schema digests already in hand.
fn lens_digest_over(src: &Digest, tgt: &Digest, compiled: &CompiledMigration) -> Digest {
    let mut hasher = blake3::Hasher::new_derive_key(LENS_CONTEXT);
    hasher.update(&src.0);
    hasher.update(&tgt.0);
    hasher.update(&canonical_compiled_bytes(compiled));
    Digest(*hasher.finalize().as_bytes())
}

/// A total, deterministic byte encoding of a [`CompiledMigration`].
///
/// Written with [`CanonicalWriter`], in the same form as
/// [`panproto_schema::canonical_bytes`]: every field in declaration order
/// and always present, every map and set at every nesting level in
/// ascending key order behind its length, every `Vec` in stored order
/// behind its length, every sum behind a one-byte discriminant, and every
/// float by its bit pattern. Equal migrations give equal bytes in any
/// process; migrations that differ in any field give different bytes.
#[must_use]
pub fn canonical_compiled_bytes(compiled: &CompiledMigration) -> Vec<u8> {
    let mut w = CanonicalWriter::new();

    // 1. surviving_verts
    let mut verts: Vec<&Name> = compiled.surviving_verts.iter().collect();
    verts.sort();
    w.len(verts.len());
    for v in verts {
        w.name(v);
    }

    // 2. surviving_edges
    let mut edges: Vec<_> = compiled.surviving_edges.iter().collect();
    edges.sort();
    w.len(edges.len());
    for e in edges {
        w.edge(e);
    }

    // 3. vertex_remap
    let vertex_remap = sorted(&compiled.vertex_remap);
    w.len(vertex_remap.len());
    for (src, tgt) in vertex_remap {
        w.name(src);
        w.name(tgt);
    }

    // 4. edge_remap
    let edge_remap = sorted(&compiled.edge_remap);
    w.len(edge_remap.len());
    for (src, tgt) in edge_remap {
        w.edge(src);
        w.edge(tgt);
    }

    // 5. resolver
    let resolver = sorted(&compiled.resolver);
    w.len(resolver.len());
    for ((src, tgt), edge) in resolver {
        w.name(src);
        w.name(tgt);
        w.edge(edge);
    }

    // 6. hyper_resolver
    let hyper_resolver = sorted(&compiled.hyper_resolver);
    w.len(hyper_resolver.len());
    for ((hyper_edge, labels), (target, label_remap)) in hyper_resolver {
        w.name(hyper_edge);
        write_names(&mut w, labels);
        w.name(target);
        let label_remap = sorted(label_remap);
        w.len(label_remap.len());
        for (from, to) in label_remap {
            w.name(from);
            w.name(to);
        }
    }

    // 7. field_transforms
    let field_transforms = sorted(&compiled.field_transforms);
    w.len(field_transforms.len());
    for (anchor, transforms) in field_transforms {
        w.name(anchor);
        w.len(transforms.len());
        for t in transforms {
            write_field_transform(&mut w, t);
        }
    }

    // 8. conditional_survival
    let conditional_survival = sorted(&compiled.conditional_survival);
    w.len(conditional_survival.len());
    for (anchor, predicate) in conditional_survival {
        w.name(anchor);
        w.expr(predicate);
    }

    // 9. op_term_assignments
    let op_term_assignments = sorted(&compiled.op_term_assignments);
    w.len(op_term_assignments.len());
    for (anchor, assignments) in op_term_assignments {
        w.name(anchor);
        w.len(assignments.len());
        for a in assignments {
            write_term_assignment(&mut w, a);
        }
    }

    // 10. expansion_path
    let expansion_path = sorted(&compiled.expansion_path);
    w.len(expansion_path.len());
    for ((src, tgt), path) in expansion_path {
        w.name(src);
        w.name(tgt);
        write_names(&mut w, path);
    }

    w.into_bytes()
}

/// A map's entries in ascending key order.
fn sorted<K: Ord, V, S>(map: &HashMap<K, V, S>) -> Vec<(&K, &V)> {
    let mut items: Vec<(&K, &V)> = map.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    items
}

fn write_names(w: &mut CanonicalWriter, names: &[Name]) {
    w.len(names.len());
    for n in names {
        w.name(n);
    }
}

fn write_strings(w: &mut CanonicalWriter, strings: &[String]) {
    w.len(strings.len());
    for s in strings {
        w.str(s);
    }
}

fn write_opt_expr(w: &mut CanonicalWriter, expr: Option<&panproto_expr::Expr>) {
    match expr {
        None => w.tag(0),
        Some(e) => {
            w.tag(1);
            w.expr(e);
        }
    }
}

/// A coercion class as its index in [`CoercionClass::all`], the enum's
/// authoritative variant order. `CoercionClass` is `#[non_exhaustive]`, so
/// this is the one total spelling of it available outside its crate.
fn write_coercion_class(w: &mut CanonicalWriter, class: CoercionClass) {
    let index = CoercionClass::all()
        .iter()
        .position(|c| *c == class)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(u8::MAX);
    w.tag(index);
}

fn write_rename_map(w: &mut CanonicalWriter, map: &HashMap<String, Option<String>>) {
    let entries = sorted(map);
    w.len(entries.len());
    for (from, to) in entries {
        w.str(from);
        match to {
            None => w.tag(0),
            Some(to) => {
                w.tag(1);
                w.str(to);
            }
        }
    }
}

/// Write a [`Value`], discriminant first.
pub fn write_value(w: &mut CanonicalWriter, value: &Value) {
    match value {
        Value::Bool(b) => {
            w.tag(0);
            w.bool(*b);
        }
        Value::Int(i) => {
            w.tag(1);
            w.i64(*i);
        }
        Value::Float(f) => {
            w.tag(2);
            w.f64(*f);
        }
        Value::Str(s) => {
            w.tag(3);
            w.str(s);
        }
        Value::Bytes(b) => {
            w.tag(4);
            w.bytes(b);
        }
        Value::CidLink(s) => {
            w.tag(5);
            w.str(s);
        }
        Value::Blob { ref_, mime, size } => {
            w.tag(6);
            w.str(ref_);
            w.str(mime);
            w.u64(*size);
        }
        Value::Token(s) => {
            w.tag(7);
            w.str(s);
        }
        Value::Null => w.tag(8),
        Value::Opaque { type_, fields } => {
            w.tag(9);
            w.str(type_);
            write_value_map(w, fields);
        }
        Value::Unknown(fields) => {
            w.tag(10);
            write_value_map(w, fields);
        }
        Value::List(items) => {
            w.tag(11);
            w.len(items.len());
            for item in items {
                write_value(w, item);
            }
        }
        Value::LabeledNull(id) => {
            w.tag(12);
            w.u64(*id);
        }
    }
}

/// Write a string-keyed map of values in ascending key order.
pub fn write_value_map<S>(w: &mut CanonicalWriter, map: &HashMap<String, Value, S>) {
    let entries = sorted(map);
    w.len(entries.len());
    for (key, value) in entries {
        w.str(key);
        write_value(w, value);
    }
}

/// Write a [`FieldPresence`], discriminant first.
pub fn write_field_presence(w: &mut CanonicalWriter, presence: &FieldPresence) {
    match presence {
        FieldPresence::Present(v) => {
            w.tag(0);
            write_value(w, v);
        }
        FieldPresence::Null => w.tag(1),
        FieldPresence::Absent => w.tag(2),
    }
}

/// Write a [`FieldTransform`], discriminant first.
pub fn write_field_transform(w: &mut CanonicalWriter, transform: &FieldTransform) {
    match transform {
        FieldTransform::RenameField { old_key, new_key } => {
            w.tag(0);
            w.str(old_key);
            w.str(new_key);
        }
        FieldTransform::DropField { key } => {
            w.tag(1);
            w.str(key);
        }
        FieldTransform::AddField { key, value } => {
            w.tag(2);
            w.str(key);
            write_value(w, value);
        }
        FieldTransform::KeepFields { keys } => {
            w.tag(3);
            write_strings(w, keys);
        }
        FieldTransform::ApplyExpr {
            key,
            expr,
            inverse,
            coercion_class,
        } => {
            w.tag(4);
            w.str(key);
            w.expr(expr);
            write_opt_expr(w, inverse.as_ref());
            write_coercion_class(w, *coercion_class);
        }
        FieldTransform::PathTransform { path, inner } => {
            w.tag(5);
            write_strings(w, path);
            write_field_transform(w, inner);
        }
        FieldTransform::ComputeField {
            target_key,
            expr,
            inverse,
            coercion_class,
        } => {
            w.tag(6);
            w.str(target_key);
            w.expr(expr);
            write_opt_expr(w, inverse.as_ref());
            write_coercion_class(w, *coercion_class);
        }
        FieldTransform::Case { branches } => {
            w.tag(7);
            w.len(branches.len());
            for branch in branches {
                write_case_branch(w, branch);
            }
        }
        FieldTransform::MapReferences { field, rename_map } => {
            w.tag(8);
            w.str(field);
            write_rename_map(w, rename_map);
        }
    }
}

/// Write a [`CaseBranch`]: its predicate, then its transforms.
pub fn write_case_branch(w: &mut CanonicalWriter, branch: &CaseBranch) {
    let CaseBranch {
        predicate,
        transforms,
    } = branch;
    w.expr(predicate);
    w.len(transforms.len());
    for t in transforms {
        write_field_transform(w, t);
    }
}

/// Write a [`TermScope`] as a one-byte discriminant.
pub fn write_term_scope(w: &mut CanonicalWriter, scope: &TermScope) {
    match scope {
        TermScope::Field => w.tag(0),
        TermScope::Row => w.tag(1),
    }
}

/// Write a [`TermAssignment`], discriminant first.
pub fn write_term_assignment(w: &mut CanonicalWriter, assignment: &TermAssignment) {
    match assignment {
        TermAssignment::Compute {
            target,
            scope,
            term,
            inverse,
            coercion_class,
        } => {
            w.tag(0);
            w.str(target);
            write_term_scope(w, scope);
            w.expr(term);
            write_opt_expr(w, inverse.as_ref());
            write_coercion_class(w, *coercion_class);
        }
        TermAssignment::Rename { old, new } => {
            w.tag(1);
            w.str(old);
            w.str(new);
        }
        TermAssignment::Drop { key } => {
            w.tag(2);
            w.str(key);
        }
        TermAssignment::Default { key, value } => {
            w.tag(3);
            w.str(key);
            write_value(w, value);
        }
        TermAssignment::Keep { keys } => {
            w.tag(4);
            write_strings(w, keys);
        }
        TermAssignment::MapReferences { field, rename_map } => {
            w.tag(5);
            w.str(field);
            write_rename_map(w, rename_map);
        }
        TermAssignment::AtPath { path, inner } => {
            w.tag(6);
            write_strings(w, path);
            write_term_assignment(w, inner);
        }
        TermAssignment::Case { branches } => {
            w.tag(7);
            w.len(branches.len());
            for branch in branches {
                write_term_branch(w, branch);
            }
        }
    }
}

/// Write a [`TermBranch`]: its predicate, then its assignments.
pub fn write_term_branch(w: &mut CanonicalWriter, branch: &TermBranch) {
    let TermBranch {
        predicate,
        assignments,
    } = branch;
    w.expr(predicate);
    w.len(assignments.len());
    for a in assignments {
        write_term_assignment(w, a);
    }
}

// ---------------------------------------------------------------------------
// Complement formats and the envelope.
// ---------------------------------------------------------------------------

/// The shape of instance a complement belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplementShape {
    /// A tree ([`WInstance`](crate::WInstance)) complement,
    /// [`Complement`].
    WType,
    /// A graph ([`GInstance`](crate::GInstance)) complement.
    Graph,
}

impl ComplementShape {
    /// The current format of this shape's complement.
    #[must_use]
    pub const fn current_format(self) -> ComplementFormat {
        ComplementFormat {
            shape: self,
            version: match self {
                Self::WType => TREE_COMPLEMENT_FORMAT_VERSION,
                Self::Graph => GRAPH_COMPLEMENT_FORMAT_VERSION,
            },
        }
    }

    /// The other shape.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::WType => Self::Graph,
            Self::Graph => Self::WType,
        }
    }
}

impl fmt::Display for ComplementShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WType => "wtype",
            Self::Graph => "graph",
        })
    }
}

/// A complement's shape and the version of that shape's wire layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComplementFormat {
    /// The instance shape.
    pub shape: ComplementShape,
    /// The layout version.
    pub version: u16,
}

impl fmt::Display for ComplementFormat {
    /// Rendered as `shape/version`, e.g. `wtype/1`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.shape, self.version)
    }
}

/// The record a complement carries of the lens that produced it.
///
/// Serialized with its fields in declaration order. Deserialization reads
/// `version` first and refuses any version other than
/// [`IDENTITY_ENVELOPE_VERSION`] without decoding the rest, so a future
/// envelope with more fields is reported as unsupported rather than as a
/// malformed complement.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct IdentityEnvelope {
    /// The envelope layout version, [`IDENTITY_ENVELOPE_VERSION`].
    pub version: u16,
    /// The source schema's [`schema_digest`].
    pub source_schema: Digest,
    /// The target schema's [`schema_digest`].
    pub target_schema: Digest,
    /// The compiled lens's [`lens_digest`].
    pub lens: Digest,
    /// The complement's shape and layout version.
    pub complement_format: ComplementFormat,
}

thread_local! {
    /// The envelope version most recently refused by
    /// [`IdentityEnvelope`]'s deserializer on this thread.
    ///
    /// serde reports a deserializer's error as a message, so the version a
    /// refused envelope carried would otherwise be lost by the time the
    /// error reaches [`decode_complement`]. The deserializer records it
    /// here and `decode_complement` reads it back to report
    /// [`IdentityError::UnsupportedEnvelopeVersion`].
    static REFUSED_ENVELOPE_VERSION: Cell<Option<u16>> = const { Cell::new(None) };
}

impl IdentityEnvelope {
    const FIELDS: &'static [&'static str] = &[
        "version",
        "source_schema",
        "target_schema",
        "lens",
        "complement_format",
    ];

    /// The envelope of a compiled lens producing complements of `shape`.
    #[must_use]
    pub fn for_parts(
        src: &Schema,
        tgt: &Schema,
        compiled: &CompiledMigration,
        shape: ComplementShape,
    ) -> Self {
        let source_schema = schema_digest(src);
        let target_schema = schema_digest(tgt);
        let lens = lens_digest_over(&source_schema, &target_schema, compiled);
        Self {
            version: IDENTITY_ENVELOPE_VERSION,
            source_schema,
            target_schema,
            lens,
            complement_format: shape.current_format(),
        }
    }

    /// The first field on which `self` (the expected envelope) and `found`
    /// differ, as an [`IdentityError::Mismatch`], or `None` when they agree.
    #[must_use]
    pub fn mismatch(&self, found: &Self) -> Option<IdentityError> {
        let digest = |field, a: &Digest, b: &Digest| {
            (a != b).then(|| IdentityError::Mismatch {
                field,
                expected: a.to_hex(),
                found: b.to_hex(),
            })
        };
        digest(
            IdentityField::SourceSchema,
            &self.source_schema,
            &found.source_schema,
        )
        .or_else(|| {
            digest(
                IdentityField::TargetSchema,
                &self.target_schema,
                &found.target_schema,
            )
        })
        .or_else(|| digest(IdentityField::Lens, &self.lens, &found.lens))
        .or_else(|| {
            (self.complement_format != found.complement_format).then(|| IdentityError::Mismatch {
                field: IdentityField::Format,
                expected: self.complement_format.to_string(),
                found: found.complement_format.to_string(),
            })
        })
    }
}

impl<'de> Deserialize<'de> for IdentityEnvelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn refuse<E: de::Error>(found: u16) -> E {
            REFUSED_ENVELOPE_VERSION.with(|slot| slot.set(Some(found)));
            E::custom(format!(
                "unsupported identity envelope version {found} (this build reads version \
                 {IDENTITY_ENVELOPE_VERSION})"
            ))
        }

        struct EnvelopeVisitor;

        impl<'de> Visitor<'de> for EnvelopeVisitor {
            type Value = IdentityEnvelope;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an identity envelope")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let version: u16 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                if version != IDENTITY_ENVELOPE_VERSION {
                    return Err(refuse(version));
                }
                let source_schema = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let target_schema = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let lens = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let complement_format = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                Ok(IdentityEnvelope {
                    version,
                    source_schema,
                    target_schema,
                    lens,
                    complement_format,
                })
            }

            /// A map's keys may come in any order (a JSON object read back
            /// through a sorted map puts `lens` before `version`), so every
            /// other field is held undecoded until the version is known.
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut version = None;
                let mut held: Vec<(String, ciborium::Value)> = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key == "version" {
                        let v: u16 = map.next_value()?;
                        if v != IDENTITY_ENVELOPE_VERSION {
                            return Err(refuse(v));
                        }
                        version = Some(v);
                    } else if Self::FIELD_NAMES.contains(&key.as_str()) {
                        held.push((key, map.next_value()?));
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                let version = version.ok_or_else(|| de::Error::missing_field("version"))?;

                let mut take = |name: &'static str| -> Result<ciborium::Value, A::Error> {
                    let at = held
                        .iter()
                        .position(|(k, _)| k == name)
                        .ok_or_else(|| de::Error::missing_field(name))?;
                    Ok(held.swap_remove(at).1)
                };
                let source_schema: Digest = held_field("source_schema", &take("source_schema")?)?;
                let target_schema: Digest = held_field("target_schema", &take("target_schema")?)?;
                let lens: Digest = held_field("lens", &take("lens")?)?;
                let complement_format: ComplementFormat =
                    held_field("complement_format", &take("complement_format")?)?;
                Ok(IdentityEnvelope {
                    version,
                    source_schema,
                    target_schema,
                    lens,
                    complement_format,
                })
            }
        }

        impl EnvelopeVisitor {
            const FIELD_NAMES: &'static [&'static str] = &[
                "source_schema",
                "target_schema",
                "lens",
                "complement_format",
            ];
        }

        deserializer.deserialize_struct("IdentityEnvelope", Self::FIELDS, EnvelopeVisitor)
    }
}

/// Decode one held envelope field.
fn held_field<T: de::DeserializeOwned, E: de::Error>(
    name: &str,
    value: &ciborium::Value,
) -> Result<T, E> {
    value
        .deserialized()
        .map_err(|e| E::custom(format!("{name}: {e}")))
}

/// The envelope of a compiled lens producing complements of `shape`.
///
/// Pure: computed from the two schemas and the compiled migration alone.
#[must_use]
pub fn lens_identity(
    src: &Schema,
    tgt: &Schema,
    compiled: &CompiledMigration,
    shape: ComplementShape,
) -> IdentityEnvelope {
    IdentityEnvelope::for_parts(src, tgt, compiled, shape)
}

// ---------------------------------------------------------------------------
// Status, policy and errors.
// ---------------------------------------------------------------------------

/// A field of an [`IdentityEnvelope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityField {
    /// The source schema digest.
    SourceSchema,
    /// The target schema digest.
    TargetSchema,
    /// The compiled lens digest.
    Lens,
    /// The complement format.
    Format,
}

impl fmt::Display for IdentityField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourceSchema => "source_schema",
            Self::TargetSchema => "target_schema",
            Self::Lens => "lens",
            Self::Format => "format",
        })
    }
}

/// The pointer width a legacy fingerprint was computed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyWidth {
    /// A 64-bit host: the edge count hashed as eight bytes.
    Bits64,
    /// wasm32: the edge count hashed as four bytes.
    Bits32,
}

/// How a complement is bound to a lens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    /// The complement carries an envelope equal to the lens's.
    Verified,
    /// The complement carries no envelope, only a legacy fingerprint, and
    /// that fingerprint matches the lens's source schema at `width`.
    Legacy {
        /// The complement's fingerprint.
        fingerprint: u64,
        /// The pointer width at which it matched.
        width: LegacyWidth,
    },
    /// The complement carries neither an envelope nor a fingerprint.
    Unbound,
}

/// Which [`IdentityStatus`]es a caller accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityPolicy {
    /// Accept [`IdentityStatus::Verified`], and
    /// [`IdentityStatus::Unbound`] only for an empty complement.
    Strict,
    /// Also accept [`IdentityStatus::Legacy`].
    #[default]
    AllowLegacy,
}

impl IdentityPolicy {
    /// Whether this policy accepts `status` for a complement that is
    /// (`complement_is_empty`) or is not empty.
    ///
    /// A non-empty [`IdentityStatus::Unbound`] complement is refused under
    /// every policy: it carries data and no record of where the data came
    /// from, so accepting it would let any stripped complement through.
    ///
    /// # Errors
    ///
    /// [`IdentityError::UnboundComplement`] for a non-empty unbound
    /// complement; [`IdentityError::LegacyComplementRejected`] with
    /// [`LegacyRejectReason::PolicyStrict`] for a legacy complement under
    /// [`Self::Strict`].
    pub const fn admit(
        self,
        status: &IdentityStatus,
        complement_is_empty: bool,
    ) -> Result<(), IdentityError> {
        match (status, self) {
            (IdentityStatus::Verified, _) | (IdentityStatus::Legacy { .. }, Self::AllowLegacy) => {
                Ok(())
            }
            (IdentityStatus::Legacy { .. }, Self::Strict) => {
                Err(IdentityError::LegacyComplementRejected {
                    reason: LegacyRejectReason::PolicyStrict,
                })
            }
            (IdentityStatus::Unbound, _) => {
                if complement_is_empty {
                    Ok(())
                } else {
                    Err(IdentityError::UnboundComplement)
                }
            }
        }
    }
}

/// Why a legacy complement was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRejectReason {
    /// The policy was [`IdentityPolicy::Strict`].
    PolicyStrict,
    /// It was to be combined with a complement that carries an envelope.
    MixedWithVerified,
    /// Its fingerprint matches neither width of the expected schema's, or
    /// the other legacy complement's.
    FingerprintMismatch,
}

impl fmt::Display for LegacyRejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PolicyStrict => "the policy is strict",
            Self::MixedWithVerified => {
                "it cannot be combined with a complement that has an envelope"
            }
            Self::FingerprintMismatch => "its fingerprint does not match",
        })
    }
}

/// Why a complement's identity was refused.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum IdentityError {
    /// The complement's envelope differs from the expected one in `field`.
    /// Digests are rendered as hex, formats as `shape/version`.
    #[error("identity mismatch on {field}: expected {expected}, found {found}")]
    Mismatch {
        /// The first differing field.
        field: IdentityField,
        /// The expected value.
        expected: String,
        /// The complement's value.
        found: String,
    },
    /// A legacy complement was refused.
    #[error("legacy complement rejected: {reason}")]
    LegacyComplementRejected {
        /// Why.
        reason: LegacyRejectReason,
    },
    /// The complement carries data but neither an envelope nor a
    /// fingerprint, so nothing binds it to a lens.
    #[error("the complement carries data but no identity, so it cannot be bound to this lens")]
    UnboundComplement,
    /// The complement's envelope has a layout version this build does not
    /// read.
    #[error(
        "unsupported identity envelope version {found} (this build reads version {IDENTITY_ENVELOPE_VERSION})"
    )]
    UnsupportedEnvelopeVersion {
        /// The version found.
        found: u16,
    },
    /// The bytes decode as neither complement shape.
    #[error("undecodable complement: {0}")]
    Undecodable(String),
}

// ---------------------------------------------------------------------------
// Legacy fingerprint.
// ---------------------------------------------------------------------------

/// The source-schema fingerprint complements carried before envelopes.
///
/// A frozen reimplementation of that algorithm: SipHash-1-3 with keys
/// `(0, 0)` over each vertex name in ascending order, each written as its
/// UTF-8 bytes followed by `0xFF`, then the edge count as a little-endian
/// integer of `width` bytes. The `SipHash` is implemented here rather than
/// taken from the standard library, whose hasher is not guaranteed to stay
/// the same across Rust releases.
#[must_use]
pub fn legacy_schema_fingerprint(schema: &Schema, width: LegacyWidth) -> u64 {
    let mut names: Vec<&str> = schema.vertices.keys().map(AsRef::as_ref).collect();
    names.sort_unstable();
    let mut message = Vec::new();
    for name in names {
        message.extend_from_slice(name.as_bytes());
        message.push(0xFF);
    }
    let edges = schema.edges.len();
    match width {
        LegacyWidth::Bits64 => {
            message.extend_from_slice(&u64::try_from(edges).unwrap_or(u64::MAX).to_le_bytes());
        }
        LegacyWidth::Bits32 => {
            message.extend_from_slice(&u32::try_from(edges).unwrap_or(u32::MAX).to_le_bytes());
        }
    }
    siphash13_zero_key(&message)
}

/// The width at which `fingerprint` is the legacy fingerprint of `schema`,
/// trying 64-bit first.
#[must_use]
pub fn legacy_width_matching(schema: &Schema, fingerprint: u64) -> Option<LegacyWidth> {
    [LegacyWidth::Bits64, LegacyWidth::Bits32]
        .into_iter()
        .find(|&w| legacy_schema_fingerprint(schema, w) == fingerprint)
}

/// SipHash-1-3 of `message` with both keys zero.
fn siphash13_zero_key(message: &[u8]) -> u64 {
    struct State([u64; 4]);

    impl State {
        const fn round(&mut self) {
            let v = &mut self.0;
            v[0] = v[0].wrapping_add(v[1]);
            v[1] = v[1].rotate_left(13);
            v[1] ^= v[0];
            v[0] = v[0].rotate_left(32);
            v[2] = v[2].wrapping_add(v[3]);
            v[3] = v[3].rotate_left(16);
            v[3] ^= v[2];
            v[0] = v[0].wrapping_add(v[3]);
            v[3] = v[3].rotate_left(21);
            v[3] ^= v[0];
            v[2] = v[2].wrapping_add(v[1]);
            v[1] = v[1].rotate_left(17);
            v[1] ^= v[2];
            v[2] = v[2].rotate_left(32);
        }

        const fn absorb(&mut self, m: u64) {
            self.0[3] ^= m;
            self.round();
            self.0[0] ^= m;
        }
    }

    let mut state = State([
        0x736f_6d65_7073_6575,
        0x646f_7261_6e64_6f6d,
        0x6c79_6765_6e65_7261,
        0x7465_6462_7974_6573,
    ]);
    let mut chunks = message.chunks_exact(8);
    for chunk in &mut chunks {
        let mut word = [0u8; 8];
        word.copy_from_slice(chunk);
        state.absorb(u64::from_le_bytes(word));
    }
    let mut tail = [0u8; 8];
    let rest = chunks.remainder();
    tail[..rest.len()].copy_from_slice(rest);
    // The message length modulo 256 in the top byte, as SipHash specifies.
    let length_byte = u64::from(u8::try_from(message.len() & 0xFF).unwrap_or(0));
    state.absorb(u64::from_le_bytes(tail) | (length_byte << 56));
    state.0[2] ^= 0xFF;
    for _ in 0..3 {
        state.round();
    }
    state.0[0] ^ state.0[1] ^ state.0[2] ^ state.0[3]
}

// ---------------------------------------------------------------------------
// Complements that carry an identity.
// ---------------------------------------------------------------------------

/// A complement type that records the lens it came from.
///
/// Every complement shape implements this, and the status, policy and
/// composition rules below are written once against it.
pub trait IdentityCarrier {
    /// The shape of instance this complement belongs to.
    const SHAPE: ComplementShape;

    /// The complement's envelope, if it carries one.
    fn identity(&self) -> Option<&IdentityEnvelope>;

    /// Replace the complement's envelope.
    fn set_identity(&mut self, envelope: Option<IdentityEnvelope>);

    /// The complement's legacy fingerprint, `0` when it has none. A shape
    /// with no legacy form always returns `0`.
    fn legacy_fingerprint(&self) -> u64;

    /// Write the legacy fingerprint alongside a new envelope. A shape with
    /// no legacy form ignores it.
    fn set_legacy_fingerprint(&mut self, fingerprint: u64);

    /// Whether the complement records nothing at all, bookkeeping included.
    /// The envelope and the fingerprint do not count.
    fn records_nothing(&self) -> bool;

    /// Stamp the complement as produced by the lens `envelope` names, whose
    /// source schema is `source`.
    ///
    /// The legacy fingerprint is written too, at 64-bit width, so a reader
    /// that predates envelopes keeps its fingerprint check.
    fn stamp(&mut self, envelope: IdentityEnvelope, source: &Schema) {
        self.set_legacy_fingerprint(legacy_schema_fingerprint(source, LegacyWidth::Bits64));
        self.set_identity(Some(envelope));
    }
}

/// The [`IdentityStatus`] of `complement` against the lens whose envelope is
/// `expected` and whose source schema is `source`.
///
/// A total function of the complement's envelope, its legacy fingerprint and
/// the expected identity:
///
/// | envelope | fingerprint | status |
/// |----------|-------------|--------|
/// | `Some(e)`, `e` equal to `expected` | any | `Verified` |
/// | `Some(e)`, `e` differing | any | error `Mismatch` on the first differing field |
/// | `None` | `≠ 0`, matching `source` at some width | `Legacy` with that width |
/// | `None` | `≠ 0`, matching at neither width | error `LegacyComplementRejected(FingerprintMismatch)` |
/// | `None` | `0` | `Unbound` |
///
/// Whether an `Unbound` complement is acceptable depends on whether it is
/// empty, which [`IdentityPolicy::admit`] decides.
///
/// # Errors
///
/// As in the table.
pub fn identity_status<C: IdentityCarrier + ?Sized>(
    complement: &C,
    expected: &IdentityEnvelope,
    source: &Schema,
) -> Result<IdentityStatus, IdentityError> {
    if let Some(found) = complement.identity() {
        return expected
            .mismatch(found)
            .map_or(Ok(IdentityStatus::Verified), Err);
    }
    match complement.legacy_fingerprint() {
        0 => Ok(IdentityStatus::Unbound),
        fingerprint => legacy_width_matching(source, fingerprint)
            .map(|width| IdentityStatus::Legacy { fingerprint, width })
            .ok_or(IdentityError::LegacyComplementRejected {
                reason: LegacyRejectReason::FingerprintMismatch,
            }),
    }
}

/// The identity of the composite of two complements of the same lens: its
/// envelope and its legacy fingerprint.
///
/// | left | right | result |
/// |------|-------|--------|
/// | non-empty unbound | any | error `UnboundComplement`, and symmetrically |
/// | empty unbound | `x` | `x` (the empty unbound complement is the unit), and symmetrically |
/// | `Some(a)` | `Some(b)` | `Some(a)` if `a == b`, else error `Mismatch` |
/// | `Some(_)` | legacy | error `LegacyComplementRejected(MixedWithVerified)`, and symmetrically |
/// | legacy `f` | legacy `g` | legacy `f` if `f == g`, else error `LegacyComplementRejected(FingerprintMismatch)` |
///
/// # Errors
///
/// As in the table.
pub fn compose_identities<C: IdentityCarrier + ?Sized>(
    left: &C,
    right: &C,
) -> Result<(Option<IdentityEnvelope>, u64), IdentityError> {
    let unbound = |c: &C| c.identity().is_none() && c.legacy_fingerprint() == 0;
    if (unbound(left) && !left.records_nothing()) || (unbound(right) && !right.records_nothing()) {
        return Err(IdentityError::UnboundComplement);
    }
    let fingerprint = |a: u64, b: u64| match (a, b) {
        (0, x) | (x, 0) => Ok(x),
        (x, y) if x == y => Ok(x),
        _ => Err(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::FingerprintMismatch,
        }),
    };
    if unbound(left) {
        return Ok((right.identity().cloned(), right.legacy_fingerprint()));
    }
    if unbound(right) {
        return Ok((left.identity().cloned(), left.legacy_fingerprint()));
    }
    match (left.identity(), right.identity()) {
        (Some(a), Some(b)) => match a.mismatch(b) {
            Some(err) => Err(err),
            None => Ok((
                Some(a.clone()),
                fingerprint(left.legacy_fingerprint(), right.legacy_fingerprint())?,
            )),
        },
        (Some(_), None) | (None, Some(_)) => Err(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::MixedWithVerified,
        }),
        (None, None) => Ok((
            None,
            fingerprint(left.legacy_fingerprint(), right.legacy_fingerprint())?,
        )),
    }
}

// ---------------------------------------------------------------------------
// Decoding.
// ---------------------------------------------------------------------------

/// A wire encoding of a complement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// msgpack with structs as arrays (`rmp_serde::to_vec`), the VCS form.
    MsgpackPositional,
    /// msgpack with structs as maps (`rmp_serde::to_vec_named`), the
    /// WASM/TS form.
    MsgpackNamed,
    /// CBOR, the C ABI form.
    Cbor,
    /// JSON.
    Json,
}

/// A decoded complement of either shape.
#[derive(Clone, Debug)]
pub enum AnyComplement {
    /// A tree complement.
    WType(Box<Complement>),
}

impl AnyComplement {
    /// The complement's shape.
    #[must_use]
    pub const fn shape(&self) -> ComplementShape {
        match self {
            Self::WType(_) => ComplementShape::WType,
        }
    }

    /// The complement's envelope, if it carries one.
    #[must_use]
    pub fn identity(&self) -> Option<&IdentityEnvelope> {
        match self {
            Self::WType(c) => c.identity.as_ref(),
        }
    }
}

fn decode_as<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    encoding: Encoding,
) -> Result<T, String> {
    match encoding {
        Encoding::MsgpackPositional | Encoding::MsgpackNamed => {
            rmp_serde::from_slice(bytes).map_err(|e| e.to_string())
        }
        Encoding::Cbor => {
            let mut cursor = std::io::Cursor::new(bytes);
            let value: T = ciborium::de::from_reader(&mut cursor).map_err(|e| e.to_string())?;
            let consumed = usize::try_from(cursor.position()).unwrap_or(bytes.len());
            if consumed < bytes.len() {
                return Err(format!(
                    "{} trailing byte(s) after the CBOR value",
                    bytes.len() - consumed
                ));
            }
            Ok(value)
        }
        Encoding::Json => serde_json::from_slice(bytes).map_err(|e| e.to_string()),
    }
}

/// Decode `bytes` as a complement of `shape`, or explain why not.
///
/// Returns `None` for a shape this build has no complement type for.
fn decode_shape(
    bytes: &[u8],
    encoding: Encoding,
    shape: ComplementShape,
) -> Option<Result<AnyComplement, String>> {
    match shape {
        ComplementShape::WType => Some(
            decode_as::<Complement>(bytes, encoding).map(|c| AnyComplement::WType(Box::new(c))),
        ),
        ComplementShape::Graph => None,
    }
}

/// Decode a complement read from outside the process.
///
/// Decodes `bytes` as a complement of the `expected` shape. If that fails
/// and the bytes decode as the other shape instead, the error is a
/// [`IdentityError::Mismatch`] on [`IdentityField::Format`]. A decoded
/// complement whose envelope names a different format is refused the same
/// way. An envelope whose layout version this build does not read gives
/// [`IdentityError::UnsupportedEnvelopeVersion`].
///
/// Every path that takes a complement from bytes or from a host-language
/// value goes through this function.
///
/// # Errors
///
/// As above, or [`IdentityError::Undecodable`] when the bytes decode as
/// neither shape.
pub fn decode_complement(
    bytes: &[u8],
    encoding: Encoding,
    expected: ComplementShape,
) -> Result<AnyComplement, IdentityError> {
    REFUSED_ENVELOPE_VERSION.with(|slot| slot.set(None));
    let refused = || REFUSED_ENVELOPE_VERSION.with(Cell::take);

    let first = decode_shape(bytes, encoding, expected);
    if let Some(Ok(decoded)) = first {
        let format = decoded
            .identity()
            .map_or_else(|| expected.current_format(), |e| e.complement_format);
        let wanted = expected.current_format();
        if format != wanted {
            return Err(IdentityError::Mismatch {
                field: IdentityField::Format,
                expected: wanted.to_string(),
                found: format.to_string(),
            });
        }
        return Ok(decoded);
    }
    if let Some(found) = refused() {
        return Err(IdentityError::UnsupportedEnvelopeVersion { found });
    }

    let other = expected.other();
    match decode_shape(bytes, encoding, other) {
        Some(Ok(decoded)) => {
            let found = decoded
                .identity()
                .map_or_else(|| other.current_format(), |e| e.complement_format);
            Err(IdentityError::Mismatch {
                field: IdentityField::Format,
                expected: expected.current_format().to_string(),
                found: found.to_string(),
            })
        }
        second => {
            if let Some(found) = refused() {
                return Err(IdentityError::UnsupportedEnvelopeVersion { found });
            }
            let reason = |shape: ComplementShape, r: Option<Result<AnyComplement, String>>| match r
            {
                Some(Err(e)) => format!("as {shape}: {e}"),
                Some(Ok(_)) => format!("as {shape}: decoded"),
                None => format!("as {shape}: this build has no {shape} complement type"),
            };
            Err(IdentityError::Undecodable(format!(
                "{}; {}",
                reason(expected, first),
                reason(other, second)
            )))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::hash::Hasher;

    use panproto_expr::{Expr, Literal};
    use panproto_schema::{Edge, Protocol, SchemaBuilder};

    use super::*;

    fn schema(extra_vertex: Option<&str>) -> Schema {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol)
            .vertex("post", "object", None)
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap();
        if let Some(v) = extra_vertex {
            builder = builder.vertex(v, "string", None).unwrap();
        }
        builder.build().unwrap()
    }

    fn envelope() -> IdentityEnvelope {
        let s = schema(None);
        lens_identity(
            &s,
            &s,
            &CompiledMigration::default(),
            ComplementShape::WType,
        )
    }

    /// A value that serializes as a byte string.
    struct Bytes<'a>(&'a [u8]);

    impl Serialize for Bytes<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_bytes(self.0)
        }
    }

    #[test]
    fn a_digest_is_hex_in_json_and_bytes_in_msgpack_and_cbor() {
        let d = Digest([0xab; 32]);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
        assert_eq!(serde_json::from_str::<Digest>(&json).unwrap(), d);

        let msgpack = rmp_serde::to_vec(&d).unwrap();
        assert_eq!(&msgpack[..2], &[0xc4, 32], "msgpack bin8 of 32 bytes");
        assert_eq!(rmp_serde::from_slice::<Digest>(&msgpack).unwrap(), d);

        let mut cbor = Vec::new();
        ciborium::ser::into_writer(&d, &mut cbor).unwrap();
        assert_eq!(&cbor[..2], &[0x58, 32], "CBOR major type 2, 32 bytes");
        assert_eq!(
            ciborium::de::from_reader::<Digest, _>(cbor.as_slice()).unwrap(),
            d
        );

        // A 32-integer sequence, the default serde form of `[u8; 32]`, and
        // a hex string inside a binary format are both accepted.
        let seq = rmp_serde::to_vec(&vec![0xabu8; 32]).unwrap();
        assert_eq!(rmp_serde::from_slice::<Digest>(&seq).unwrap(), d);
        let hex_in_msgpack = rmp_serde::to_vec(&"ab".repeat(32)).unwrap();
        assert_eq!(rmp_serde::from_slice::<Digest>(&hex_in_msgpack).unwrap(), d);
    }

    #[test]
    fn a_digest_of_the_wrong_length_is_refused() {
        assert!(serde_json::from_str::<Digest>("\"abcd\"").is_err());
        assert!(
            rmp_serde::from_slice::<Digest>(&rmp_serde::to_vec(&vec![1u8; 31]).unwrap()).is_err()
        );
        assert!(
            rmp_serde::from_slice::<Digest>(&rmp_serde::to_vec(&vec![1u8; 33]).unwrap()).is_err()
        );
        let bytes31 = rmp_serde::to_vec(&Bytes(&[1u8; 31])).unwrap();
        assert!(rmp_serde::from_slice::<Digest>(&bytes31).is_err());
        assert!("zz".repeat(32).parse::<Digest>().is_err());
        assert!(rmp_serde::from_slice::<Digest>(&rmp_serde::to_vec(&7u8).unwrap()).is_err());
    }

    #[test]
    fn an_envelope_round_trips_in_every_encoding() {
        let e = envelope();
        assert_eq!(
            rmp_serde::from_slice::<IdentityEnvelope>(&rmp_serde::to_vec(&e).unwrap()).unwrap(),
            e
        );
        assert_eq!(
            rmp_serde::from_slice::<IdentityEnvelope>(&rmp_serde::to_vec_named(&e).unwrap())
                .unwrap(),
            e
        );
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["complement_format"]["shape"], "wtype");
        assert_eq!(serde_json::from_value::<IdentityEnvelope>(json).unwrap(), e);
        let mut cbor = Vec::new();
        ciborium::ser::into_writer(&e, &mut cbor).unwrap();
        assert_eq!(
            ciborium::de::from_reader::<IdentityEnvelope, _>(cbor.as_slice()).unwrap(),
            e
        );
    }

    #[test]
    fn an_envelope_of_another_version_is_refused_before_the_rest_is_read() {
        // Version 2 with a tail this build cannot decode at all.
        let positional = rmp_serde::to_vec(&(2u16, "not a digest", 7u8)).unwrap();
        let err = rmp_serde::from_slice::<IdentityEnvelope>(&positional).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported identity envelope version 2"),
            "{err}"
        );

        let named = serde_json::json!({"version": 3, "source_schema": false});
        let err = serde_json::from_value::<IdentityEnvelope>(named).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported identity envelope version 3"),
            "{err}"
        );
    }

    #[test]
    fn the_legacy_fingerprint_is_the_standard_hasher_at_native_width() {
        for extra in [
            None,
            Some("a"),
            Some("zzz.long.vertex.name.over.eight.bytes"),
        ] {
            let s = schema(extra);
            let mut std_hasher = std::collections::hash_map::DefaultHasher::new();
            let mut names: Vec<&str> = s.vertices.keys().map(AsRef::as_ref).collect();
            names.sort_unstable();
            for n in &names {
                std::hash::Hash::hash(n, &mut std_hasher);
            }
            std::hash::Hash::hash(&s.edges.len(), &mut std_hasher);
            assert_eq!(
                legacy_schema_fingerprint(&s, LegacyWidth::Bits64),
                std_hasher.finish()
            );
            assert_ne!(
                legacy_schema_fingerprint(&s, LegacyWidth::Bits64),
                legacy_schema_fingerprint(&s, LegacyWidth::Bits32)
            );
        }
    }

    #[test]
    fn siphash_matches_the_standard_hasher_on_every_tail_length() {
        for len in 0..40u8 {
            let message: Vec<u8> = (0..len).collect();
            let mut std_hasher = std::collections::hash_map::DefaultHasher::new();
            std_hasher.write(&message);
            assert_eq!(
                siphash13_zero_key(&message),
                std_hasher.finish(),
                "len {len}"
            );
        }
    }

    fn rich_migration() -> CompiledMigration {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let mut m = CompiledMigration::default();
        for v in ["a", "b", "c", "d", "e"] {
            m.surviving_verts.insert(v.into());
            m.vertex_remap
                .insert(v.into(), format!("t.{v}").as_str().into());
        }
        m.surviving_edges.insert(edge.clone());
        m.edge_remap.insert(edge.clone(), edge.clone());
        m.resolver.insert(("a".into(), "b".into()), edge);
        m.hyper_resolver.insert(
            ("h".into(), vec!["x".into(), "y".into()]),
            (
                "h2".into(),
                [("x".into(), "x2".into()), ("y".into(), "y2".into())]
                    .into_iter()
                    .collect(),
            ),
        );
        m.field_transforms.insert(
            "a".into(),
            vec![
                FieldTransform::RenameField {
                    old_key: "o".into(),
                    new_key: "n".into(),
                },
                FieldTransform::MapReferences {
                    field: "refs".into(),
                    rename_map: [("p".into(), Some("q".into())), ("r".into(), None)]
                        .into_iter()
                        .collect(),
                },
                FieldTransform::AddField {
                    key: "k".into(),
                    value: Value::Unknown(
                        [("u".into(), Value::Float(-0.0)), ("v".into(), Value::Null)]
                            .into_iter()
                            .collect(),
                    ),
                },
            ],
        );
        m.conditional_survival
            .insert("b".into(), Expr::Lit(Literal::Bool(true)));
        m.op_term_assignments.insert(
            "c".into(),
            vec![TermAssignment::Compute {
                target: "t".into(),
                scope: TermScope::Row,
                term: Expr::Var("x".into()),
                inverse: None,
                coercion_class: CoercionClass::Projection,
            }],
        );
        m.expansion_path
            .insert(("a".into(), "b".into()), vec!["i1".into(), "i2".into()]);
        m
    }

    #[test]
    fn the_compiled_encoding_ignores_hash_seeds() {
        let baseline = canonical_compiled_bytes(&rich_migration());
        for _ in 0..50 {
            assert_eq!(canonical_compiled_bytes(&rich_migration()), baseline);
        }
    }

    type Edit = fn(&mut CompiledMigration);

    #[test]
    fn every_compiled_field_reaches_the_encoding() {
        let base = canonical_compiled_bytes(&rich_migration());
        let edits: [(&str, Edit); 11] = [
            ("surviving_verts", |m| {
                m.surviving_verts.insert("z".into());
            }),
            ("surviving_edges", |m| m.surviving_edges.clear()),
            ("vertex_remap", |m| {
                m.vertex_remap.insert("a".into(), "other".into());
            }),
            ("edge_remap", |m| m.edge_remap.clear()),
            ("resolver", |m| m.resolver.clear()),
            ("hyper_resolver", |m| m.hyper_resolver.clear()),
            ("field_transforms order", |m| {
                if let Some(ts) = m.field_transforms.get_mut("a") {
                    ts.reverse();
                }
            }),
            ("conditional_survival", |m| {
                m.conditional_survival
                    .insert("b".into(), Expr::Lit(Literal::Bool(false)));
            }),
            ("op_term_assignments", |m| m.op_term_assignments.clear()),
            ("expansion_path", |m| m.expansion_path.clear()),
            ("a float's sign", |m| {
                if let Some(ts) = m.field_transforms.get_mut("a") {
                    ts[2] = FieldTransform::AddField {
                        key: "k".into(),
                        value: Value::Unknown(
                            [("u".into(), Value::Float(0.0)), ("v".into(), Value::Null)]
                                .into_iter()
                                .collect(),
                        ),
                    };
                }
            }),
        ];
        for (what, edit) in edits {
            let mut m = rich_migration();
            edit(&mut m);
            assert_ne!(canonical_compiled_bytes(&m), base, "{what} was invisible");
        }
    }

    #[test]
    fn schema_and_lens_digests_use_their_contexts() {
        let s = schema(None);
        assert_eq!(
            schema_digest(&s).0,
            blake3::derive_key(SCHEMA_CONTEXT, &panproto_schema::canonical_bytes(&s))
        );
        let m = CompiledMigration::default();
        let e = lens_identity(&s, &s, &m, ComplementShape::Graph);
        assert_eq!(e.lens, lens_digest(&s, &s, &m));
        assert_eq!(e.complement_format.to_string(), "graph/1");
        assert_ne!(e.lens, e.source_schema);
    }

    #[test]
    fn a_mismatch_names_the_first_differing_field() {
        let e = envelope();
        let mut other = e.clone();
        other.lens = Digest([0; 32]);
        other.complement_format = ComplementShape::Graph.current_format();
        assert_eq!(
            e.mismatch(&other),
            Some(IdentityError::Mismatch {
                field: IdentityField::Lens,
                expected: e.lens.to_hex(),
                found: "00".repeat(32),
            })
        );
        other.lens = e.lens;
        assert_eq!(
            e.mismatch(&other),
            Some(IdentityError::Mismatch {
                field: IdentityField::Format,
                expected: "wtype/1".into(),
                found: "graph/1".into(),
            })
        );
        assert!(e.mismatch(&e).is_none());
    }

    #[test]
    fn decoding_reports_a_foreign_format_and_an_unsupported_envelope() {
        let mut c = Complement::empty();
        let mut e = envelope();
        e.complement_format = ComplementShape::Graph.current_format();
        c.identity = Some(e);
        let bytes = serde_json::to_vec(&c).unwrap();
        assert!(matches!(
            decode_complement(&bytes, Encoding::Json, ComplementShape::WType),
            Err(IdentityError::Mismatch {
                field: IdentityField::Format,
                ..
            })
        ));

        let mut value = serde_json::to_value(&c).unwrap();
        value["identity"]["version"] = serde_json::json!(9);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            decode_complement(&bytes, Encoding::Json, ComplementShape::WType).unwrap_err(),
            IdentityError::UnsupportedEnvelopeVersion { found: 9 }
        );

        assert!(matches!(
            decode_complement(b"not a complement", Encoding::Json, ComplementShape::WType),
            Err(IdentityError::Undecodable(_))
        ));
    }

    #[test]
    fn decoding_reports_a_tree_complement_where_a_graph_one_was_expected() {
        let mut c = Complement::empty();
        c.identity = Some(envelope());
        for (encoding, bytes) in [
            (Encoding::Json, serde_json::to_vec(&c).unwrap()),
            (Encoding::MsgpackNamed, rmp_serde::to_vec_named(&c).unwrap()),
        ] {
            assert_eq!(
                decode_complement(&bytes, encoding, ComplementShape::Graph).unwrap_err(),
                IdentityError::Mismatch {
                    field: IdentityField::Format,
                    expected: "graph/1".into(),
                    found: "wtype/1".into(),
                }
            );
        }
    }

    #[test]
    fn cbor_with_trailing_bytes_is_undecodable() {
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&Complement::empty(), &mut bytes).unwrap();
        bytes.push(0);
        assert!(matches!(
            decode_complement(&bytes, Encoding::Cbor, ComplementShape::WType),
            Err(IdentityError::Undecodable(_))
        ));
    }
}
