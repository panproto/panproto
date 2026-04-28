//! The parse / emit pair as a verified asymmetric lens.
//!
//! `parse` and `emit_pretty` together form an asymmetric lens between a
//! source-byte object `B` and a schema-object `S(p)` in the image of
//! `parse_p`:
//!
//! ```text
//!     parse_p :  B  ─▶  (S(p), C)         get-direction (with complement)
//!     emit_p  :  S(p) ─▶ B                put-direction (drops complement)
//! ```
//!
//! The complement `C` carries everything the schema doesn't fix:
//! byte positions, interstitial whitespace, comments. `emit_pretty`
//! reconstitutes a *canonical* element of `parse_p^{-1}(s)` using a
//! whitespace policy in place of the discarded complement.
//!
//! In categorical terms this is a *retraction in the image of the
//! parser*: for every `s ∈ image(parse_p)`, the round-trip
//! `parse_p ∘ emit_p` returns a schema with the same vertex/edge kind
//! multiset as `s`. We don't get pointwise equality because byte
//! positions and interstitial constraints are not part of the
//! emit_pretty image; that's the price of a canonical section.
//!
//! The law-checkers in this module verify the retraction explicitly so
//! that any future change to `emit_pretty` is provably faithful.
//!
//! # Laws
//!
//! ## `EmitParse` (the retraction law)
//!
//! For all `s` in the image of `parse_p`,
//!
//! ```text
//!     kind_multiset(parse_p(emit_p(s))) = kind_multiset(s')
//! ```
//!
//! where `s'` is `s` with the byte-position constraints stripped (the
//! portion that emit_pretty cannot preserve by construction).
//!
//! ## `ParseEmit` (stability under round-trip)
//!
//! For all `b` such that `parse_p(b)` succeeds,
//!
//! ```text
//!     parse_p(emit_p(strip(parse_p(b)))) ≅_kinds strip(parse_p(b))
//! ```
//!
//! That is, the emit_pretty image is a fixed point of the round-trip
//! up to kind-multiset equivalence.

use std::collections::BTreeMap;

use panproto_schema::Schema;

use crate::error::ParseError;
use crate::registry::ParserRegistry;

/// A protolens for the source-bytes ↔ schema relation at one protocol.
///
/// `ParseEmitLens` is parameterised by a protocol name; calls into the
/// underlying `ParserRegistry` thread the parser and grammar through.
pub struct ParseEmitLens<'r> {
    registry: &'r ParserRegistry,
    protocol: String,
}

impl<'r> ParseEmitLens<'r> {
    /// Build a parse/emit lens for `protocol` against `registry`.
    #[must_use]
    pub fn new(registry: &'r ParserRegistry, protocol: impl Into<String>) -> Self {
        Self {
            registry,
            protocol: protocol.into(),
        }
    }

    /// Forward direction: source bytes → schema. The complement (byte
    /// positions, interstitials) lives on the schema as constraints.
    ///
    /// # Errors
    ///
    /// Returns the parser's error if `protocol` is unknown or `source`
    /// is unparseable for the protocol.
    pub fn parse(&self, source: &[u8]) -> Result<Schema, ParseError> {
        self.registry
            .parse_with_protocol(&self.protocol, source, "parse_emit_lens")
    }

    /// Backward direction: schema → canonical source bytes. Drops
    /// complement; output is one canonical representative of the
    /// parse-preimage of `schema`.
    ///
    /// # Errors
    ///
    /// Returns the emitter's error if `protocol` is unknown or the
    /// schema cannot be rendered (e.g. grammar.json was not vendored).
    pub fn emit(&self, schema: &Schema) -> Result<Vec<u8>, ParseError> {
        self.registry
            .emit_pretty_with_protocol(&self.protocol, schema)
    }
}

/// Possible failures when verifying the parse/emit lens laws.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LawViolation {
    /// The retraction law `parse(emit(s)) ≅_kinds strip(s)` failed.
    #[error("EmitParse law violated for protocol {protocol}: {detail}")]
    EmitParse {
        /// Protocol whose lens failed the law.
        protocol: String,
        /// Human-readable description of the divergence.
        detail: String,
    },
    /// The stability law `parse(emit(strip(parse(b)))) ≅ strip(parse(b))` failed.
    #[error("ParseEmit law violated for protocol {protocol}: {detail}")]
    ParseEmit {
        /// Protocol whose lens failed the law.
        protocol: String,
        /// Human-readable description of the divergence.
        detail: String,
    },
    /// An underlying `parse` or `emit` step returned an error.
    #[error("underlying parse/emit error: {0}")]
    Underlying(#[from] ParseError),
}

/// Strip byte-position constraints from a schema.
///
/// Removes `start-byte`, `end-byte`, and `interstitial-*` constraints:
/// these are the "complement" portion that `emit_pretty` does not
/// reconstruct, so they are projected out before comparing.
pub fn strip_complement(schema: &mut Schema) {
    for constraints in schema.constraints.values_mut() {
        constraints.retain(|c| {
            let s = c.sort.as_ref();
            !(s == "start-byte" || s == "end-byte" || s.starts_with("interstitial-"))
        });
    }
}

/// Vertex-kind multiset, half of the equivalence class used for law checks.
///
/// See also [`edge_multiset`]; both are required for a faithful retraction
/// witness because two schemas can share a vertex-kind multiset while
/// differing in edge structure (e.g. a tree and its mirror).
#[must_use]
pub fn kind_multiset(schema: &Schema) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for v in schema.vertices.values() {
        *map.entry(v.kind.to_string()).or_insert(0) += 1;
    }
    map
}

/// Edge-shape multiset: counts of `(src_kind, edge_kind, tgt_kind)` triples.
///
/// Combined with [`kind_multiset`] this is the full witness the retraction
/// law requires — vertex kinds alone leave edge structure unconstrained.
///
/// Note: `Schema::edges` is a `HashMap<Edge, Name>` so distinct edges
/// (different src/tgt vertex IDs) are kept apart, but two edges that
/// happen to share `(src_id, tgt_id, kind, name)` collapse to one
/// entry. The shape multiset projects these structurally-distinct
/// edges onto a kind-level signature, which is the appropriate
/// granularity for the retraction law: `emit_pretty` cannot be expected
/// to preserve vertex IDs (the parser invents fresh ones) but it must
/// preserve the kind-shape of every edge.
#[must_use]
pub fn edge_multiset(schema: &Schema) -> BTreeMap<(String, String, String), usize> {
    let mut map: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for edge in schema.edges.keys() {
        let src_kind = schema
            .vertices
            .get(&edge.src)
            .map(|v| v.kind.to_string())
            .unwrap_or_default();
        let tgt_kind = schema
            .vertices
            .get(&edge.tgt)
            .map(|v| v.kind.to_string())
            .unwrap_or_default();
        *map.entry((src_kind, edge.kind.to_string(), tgt_kind))
            .or_insert(0) += 1;
    }
    map
}

/// Verify the `EmitParse` law on a given schema:
///
/// ```text
///     kind_multiset(parse(emit(strip(s)))) == kind_multiset(strip(s))
/// ```
///
/// Returns `Ok(())` iff the round-trip preserves the kind multiset.
///
/// # Errors
///
/// Returns [`LawViolation::EmitParse`] when the round-trip changes
/// the kind multiset, or [`LawViolation::Underlying`] if `parse` or
/// `emit` itself fails.
pub fn check_emit_parse(lens: &ParseEmitLens<'_>, schema: &Schema) -> Result<(), LawViolation> {
    let mut stripped = schema.clone();
    strip_complement(&mut stripped);
    let expected_kinds = kind_multiset(&stripped);
    let expected_edges = edge_multiset(&stripped);

    let bytes = lens.emit(&stripped)?;
    let mut round = lens.parse(&bytes)?;
    strip_complement(&mut round);
    let actual_kinds = kind_multiset(&round);
    let actual_edges = edge_multiset(&round);

    if expected_kinds != actual_kinds {
        return Err(LawViolation::EmitParse {
            protocol: lens.protocol.clone(),
            detail: format!(
                "vertex-kind multiset mismatch: expected {} distinct kinds, got {}; \
                 first divergence: {:?}",
                expected_kinds.len(),
                actual_kinds.len(),
                first_divergence(&expected_kinds, &actual_kinds),
            ),
        });
    }
    if expected_edges != actual_edges {
        return Err(LawViolation::EmitParse {
            protocol: lens.protocol.clone(),
            detail: format!(
                "edge-shape multiset mismatch: expected {} distinct edge shapes, got {}",
                expected_edges.len(),
                actual_edges.len(),
            ),
        });
    }
    Ok(())
}

/// Verify the `ParseEmit` stability law on a source byte string:
///
/// ```text
///     kind_multiset(parse(emit(strip(parse(b)))))
///         == kind_multiset(strip(parse(b)))
/// ```
///
/// Returns `Ok(())` iff `b` round-trips through the lens up to kind
/// multiset.
///
/// # Errors
///
/// Returns [`LawViolation::EmitParse`] when the round-trip changes
/// the kind multiset, or [`LawViolation::Underlying`] if `parse` or
/// `emit` itself fails.
pub fn check_parse_emit(lens: &ParseEmitLens<'_>, bytes: &[u8]) -> Result<(), LawViolation> {
    let parsed = lens.parse(bytes)?;
    check_emit_parse(lens, &parsed)
}

fn first_divergence(
    expected: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> Option<(String, Option<usize>, Option<usize>)> {
    for (k, &v) in expected {
        if actual.get(k) != Some(&v) {
            return Some((k.clone(), Some(v), actual.get(k).copied()));
        }
    }
    for (k, &v) in actual {
        if !expected.contains_key(k) {
            return Some((k.clone(), None, Some(v)));
        }
    }
    None
}

#[cfg(test)]
#[cfg(feature = "grammars")]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, dead_code)]
mod tests {
    use super::*;

    fn run_check(protocol: &str, source: &[u8]) {
        let registry = ParserRegistry::new();
        let lens = ParseEmitLens::new(&registry, protocol);
        check_parse_emit(&lens, source)
            .unwrap_or_else(|e| panic!("law check failed for {protocol}: {e}"));
    }

    #[cfg(feature = "lang-json")]
    #[test]
    fn json_lens_satisfies_laws() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| run_check("json", br#"{"a": 1, "b": [2, 3]}"#))
            .expect("spawn")
            .join()
            .expect("worker panicked");
    }

    #[cfg(feature = "lang-toml")]
    #[test]
    fn toml_lens_satisfies_laws() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| run_check("toml", b"name = \"foo\"\nversion = \"1.0\"\n"))
            .expect("spawn")
            .join()
            .expect("worker panicked");
    }

    #[cfg(feature = "lang-json")]
    #[test]
    fn json_check_emit_parse_directly() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let registry = ParserRegistry::new();
                let lens = ParseEmitLens::new(&registry, "json");
                let parsed = lens.parse(br#"[1, 2, 3]"#).expect("parse");
                check_emit_parse(&lens, &parsed).expect("retraction holds for parsed schema");
            })
            .expect("spawn")
            .join()
            .expect("worker panicked");
    }

    #[cfg(feature = "lang-json")]
    #[test]
    fn strip_complement_removes_byte_constraints_only() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let registry = ParserRegistry::new();
                let lens = ParseEmitLens::new(&registry, "json");
                let mut parsed = lens.parse(br#"{"a": 1}"#).expect("parse");

                let total_constraint_count: usize = parsed.constraints.values().map(Vec::len).sum();
                strip_complement(&mut parsed);
                let stripped_total: usize = parsed.constraints.values().map(Vec::len).sum();

                assert!(
                    stripped_total < total_constraint_count,
                    "strip_complement must remove byte-position constraints"
                );
                // Walker emits chose-alt-fingerprint and similar
                // constraints that strip_complement preserves.
                let preserved = parsed.constraints.values().any(|cs| {
                    cs.iter()
                        .any(|c| c.sort.as_ref() == "chose-alt-fingerprint")
                });
                assert!(
                    preserved,
                    "strip_complement must preserve chose-alt-fingerprint witnesses"
                );
            })
            .expect("spawn")
            .join()
            .expect("worker panicked");
    }

    #[cfg(feature = "lang-json")]
    #[test]
    fn edge_multiset_distinguishes_structurally_different_schemas() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let registry = ParserRegistry::new();
                let lens = ParseEmitLens::new(&registry, "json");
                let s1 = lens.parse(br#"{"a": 1}"#).expect("parse");
                let s2 = lens.parse(br#"[1]"#).expect("parse");
                let m1 = edge_multiset(&s1);
                let m2 = edge_multiset(&s2);
                assert_ne!(
                    m1, m2,
                    "object and array schemas have distinct edge-shape multisets"
                );
            })
            .expect("spawn")
            .join()
            .expect("worker panicked");
    }

    #[test]
    fn first_divergence_finds_count_mismatch() {
        let mut a = BTreeMap::new();
        a.insert("x".to_owned(), 1);
        let mut b = BTreeMap::new();
        b.insert("x".to_owned(), 2);
        assert_eq!(
            first_divergence(&a, &b),
            Some(("x".to_owned(), Some(1), Some(2)))
        );
    }

    #[test]
    fn first_divergence_finds_extra_key_in_actual() {
        let a = BTreeMap::new();
        let mut b = BTreeMap::new();
        b.insert("y".to_owned(), 3);
        assert_eq!(
            first_divergence(&a, &b),
            Some(("y".to_owned(), None, Some(3)))
        );
    }

    #[test]
    fn first_divergence_returns_none_on_match() {
        let mut a = BTreeMap::new();
        a.insert("x".to_owned(), 1);
        let b = a.clone();
        assert_eq!(first_divergence(&a, &b), None);
    }
}
