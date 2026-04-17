//! Sort-coercion alignment strategy.
//!
//! Proposes anchors between source and target vertices whose kinds
//! differ but are bridged by a witness in the supplied
//! [`WitnessLibrary`]. Gated to the `Exploratory` tier via the
//! tier's `uses_coerce` knob; Lenient-and-below never fires this
//! strategy because coerced sort assignments are categorically
//! distinct from the identity-sort assignments lower tiers enforce.
//!
//! Each emitted anchor is tagged with the [`SortLensWitness`] that
//! bridges the kinds so downstream code can realize the coercion as
//! a [`TheoryTransform::CoerceSort`](panproto_gat::TheoryTransform)
//! endofunctor without re-looking-up the witness.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};
use crate::coerce::{SortLensWitness, WitnessLibrary};

/// Emit anchors for source-target vertex pairs bridgeable by a
/// library witness.
///
/// For each source vertex, scans the target schema for kind-compatible
/// vertices *and* kind-bridgeable vertices (via the witness library).
/// Identity-kind matches are skipped (they're handled by other
/// strategies); only coercion pairings are emitted here. Confidence
/// is derived from the witness's [`panproto_gat::CoercionClass`]:
/// `Iso` → `0.8`, `Retraction` → `0.55`, `Projection` → `0.35`.
///
/// Returns one coerce-anchor per source vertex at most, picking the
/// highest-confidence target among bridgeable kinds.
#[must_use]
pub fn coerce_anchors(src: &Schema, tgt: &Schema, library: &WitnessLibrary) -> Vec<CoerceAnchor> {
    if library.is_empty() {
        return Vec::new();
    }

    // Resolve each schema's vertex kind to its ValueKind, when possible.
    let src_value_kinds = schema_value_kinds(src);
    let tgt_value_kinds = schema_value_kinds(tgt);

    // Sort source and target IDs so ties (equal-confidence witnesses)
    // resolve deterministically across runs. `Schema::vertices` is a
    // `HashMap`, so its iteration order would otherwise vary.
    let mut src_ids: Vec<&Name> = src.vertices.keys().collect();
    src_ids.sort_by_key(|n| n.as_str());
    let mut tgt_ids: Vec<&Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by_key(|n| n.as_str());

    let mut out = Vec::new();
    for src_id in src_ids {
        let Some(src_kind) = src_value_kinds.get(src_id).copied() else {
            continue;
        };
        let mut best: Option<(Name, &SortLensWitness, f64)> = None;
        for tgt_id in &tgt_ids {
            if kinds_compatible(src, src_id, tgt, tgt_id) {
                continue; // identity-kind handled elsewhere
            }
            let Some(tgt_kind) = tgt_value_kinds.get(*tgt_id).copied() else {
                continue;
            };
            for witness in library.lookup(src_kind, tgt_kind) {
                let confidence = class_confidence(witness.class);
                if best.as_ref().is_none_or(|(_, _, prev)| confidence > *prev) {
                    best = Some(((*tgt_id).clone(), witness, confidence));
                }
            }
        }
        if let Some((tgt_id, witness, confidence)) = best {
            out.push(CoerceAnchor {
                anchor: Anchor {
                    src: src_id.clone(),
                    tgt: tgt_id.clone(),
                    confidence,
                    strategy: StrategyTag::Coerce,
                    explanation: format!(
                        "sort-coercion {}: {} ↔ {} ({:?})",
                        witness.description,
                        src_id.as_str(),
                        tgt_id.as_str(),
                        witness.class
                    ),
                },
                witness_name: witness.name.clone(),
                witness_class: witness.class,
            });
        }
    }
    out
}

/// Enriched anchor carrying the name + class of the witness that
/// motivated it. Callers integrating with `auto_lens` can look the
/// witness back up in the library to emit a `CoerceSort` endofunctor.
#[derive(Clone, Debug)]
pub struct CoerceAnchor {
    /// The bare anchor suitable for merging into the anchor pool.
    pub anchor: Anchor,
    /// Name of the bridging [`SortLensWitness`].
    pub witness_name: String,
    /// Round-trip classification of the bridging witness.
    pub witness_class: panproto_gat::CoercionClass,
}

const fn class_confidence(class: panproto_gat::CoercionClass) -> f64 {
    match class {
        panproto_gat::CoercionClass::Iso => 0.8,
        panproto_gat::CoercionClass::Retraction => 0.55,
        panproto_gat::CoercionClass::Projection => 0.35,
        // `CoercionClass` is `#[non_exhaustive]`; `Opaque` and any
        // future variants default to the conservative low-confidence
        // floor so the strategy remains safe under upstream additions.
        _ => 0.2,
    }
}

/// Map each vertex in `schema` to its `ValueKind`, when the vertex
/// kind name parses as one of the primitive carriers.
fn schema_value_kinds(schema: &Schema) -> HashMap<Name, panproto_gat::ValueKind> {
    use panproto_gat::ValueKind;
    let mut out = HashMap::new();
    for (id, vertex) in &schema.vertices {
        let vk = match vertex.kind.as_str() {
            "bool" | "boolean" => Some(ValueKind::Bool),
            "int" | "integer" => Some(ValueKind::Int),
            "float" | "number" => Some(ValueKind::Float),
            "str" | "string" => Some(ValueKind::Str),
            "bytes" => Some(ValueKind::Bytes),
            "token" => Some(ValueKind::Token),
            "null" => Some(ValueKind::Null),
            _ => None,
        };
        if let Some(vk) = vk {
            out.insert(id.clone(), vk);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::coerce::default_witness_library;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "record".into(),
                "string".into(),
                "integer".into(),
                "boolean".into(),
                "float".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build(verts: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, k) in verts {
            b = b.vertex(id, k, None::<&str>).unwrap();
        }
        for (s, t, k, n) in edges {
            b = b.edge(s, t, k, Some(*n)).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn proposes_int_to_str_coercion() {
        let src = build(
            &[("r", "record"), ("r.n", "integer")],
            &[("r", "r.n", "prop", "n")],
        );
        let tgt = build(
            &[("r", "record"), ("r.n", "string")],
            &[("r", "r.n", "prop", "n")],
        );
        let lib = default_witness_library();
        let anchors = coerce_anchors(&src, &tgt, &lib);
        assert!(
            anchors.iter().any(|a| a.anchor.src.as_str() == "r.n"
                && a.anchor.tgt.as_str() == "r.n"
                && a.witness_name == "int_to_str"),
            "expected int_to_str coerce anchor on r.n; got {anchors:?}"
        );
    }

    #[test]
    fn skips_identity_kind_matches() {
        let src = build(
            &[("r", "record"), ("r.n", "integer")],
            &[("r", "r.n", "prop", "n")],
        );
        let tgt = src.clone();
        let lib = default_witness_library();
        let anchors = coerce_anchors(&src, &tgt, &lib);
        assert!(
            anchors.iter().all(|a| a.anchor.src.as_str() != "r.n"),
            "should not emit coerce anchors on identity-kind pairs"
        );
    }

    #[test]
    fn empty_library_emits_nothing() {
        let src = build(
            &[("r", "record"), ("r.n", "integer")],
            &[("r", "r.n", "prop", "n")],
        );
        let tgt = build(
            &[("r", "record"), ("r.n", "string")],
            &[("r", "r.n", "prop", "n")],
        );
        let lib = WitnessLibrary::new();
        let anchors = coerce_anchors(&src, &tgt, &lib);
        assert!(anchors.is_empty());
    }

    #[test]
    fn prefers_higher_class_when_tied() {
        // Both int→str and int→float are Retraction (0.55 each) in
        // the default library. With tied confidence, `coerce_anchors`
        // keeps the first candidate encountered (strict `>` in the
        // tie-breaker). This test locks the behaviour: whatever
        // witness wins, it must be a library witness (not dropped),
        // and the anchor's class must match the library entry.
        let src = build(
            &[("r", "record"), ("r.n", "integer")],
            &[("r", "r.n", "prop", "n")],
        );
        let tgt = build(
            &[
                ("r", "record"),
                ("r.n_str", "string"),
                ("r.n_float", "float"),
            ],
            &[
                ("r", "r.n_str", "prop", "n_str"),
                ("r", "r.n_float", "prop", "n_float"),
            ],
        );
        let lib = default_witness_library();
        let anchors = coerce_anchors(&src, &tgt, &lib);
        let picked = anchors
            .iter()
            .find(|a| a.anchor.src.as_str() == "r.n")
            .expect("should emit a coerce anchor for r.n");
        assert!(
            picked.witness_name == "int_to_str" || picked.witness_name == "int_to_float",
            "expected a library witness; got {}",
            picked.witness_name
        );
        assert_eq!(
            picked.witness_class,
            panproto_gat::CoercionClass::Retraction
        );
    }

    #[test]
    fn iso_beats_retraction_when_both_available() {
        // Register an Iso int→str witness alongside the default
        // Retraction one. `coerce_anchors` should prefer the Iso
        // because 0.8 > 0.55.
        let mut lib = WitnessLibrary::new();
        let mut iso = crate::coerce::witness::int_to_str_witness();
        iso.name = "int_to_str_iso".to_owned();
        iso.class = panproto_gat::CoercionClass::Iso;
        lib.register(iso);
        lib.register(crate::coerce::witness::int_to_str_witness()); // Retraction

        let src = build(
            &[("r", "record"), ("r.n", "integer")],
            &[("r", "r.n", "prop", "n")],
        );
        let tgt = build(
            &[("r", "record"), ("r.s", "string")],
            &[("r", "r.s", "prop", "s")],
        );
        let anchors = coerce_anchors(&src, &tgt, &lib);
        let picked = anchors
            .iter()
            .find(|a| a.anchor.src.as_str() == "r.n")
            .expect("should emit a coerce anchor");
        assert_eq!(picked.witness_name, "int_to_str_iso");
    }

    #[test]
    fn class_confidence_is_monotone_across_known_variants() {
        // Compile-time-ish check: the four known variants satisfy
        // Iso > Retraction > Projection > unknown.
        use panproto_gat::CoercionClass;
        assert!(class_confidence(CoercionClass::Iso) > class_confidence(CoercionClass::Retraction));
        assert!(
            class_confidence(CoercionClass::Retraction)
                > class_confidence(CoercionClass::Projection)
        );
        // `Opaque` (the current "other" variant): falls through to
        // the conservative 0.2 floor. This pins the value so a future
        // bump gets a test failure rather than silently widening the
        // strategy's recall.
        assert!(
            (class_confidence(CoercionClass::Opaque) - 0.2).abs() < 1e-9,
            "Opaque should hit the conservative floor (0.2)"
        );
        // Pin the exact confidence values to catch accidental drift.
        assert!((class_confidence(CoercionClass::Iso) - 0.8).abs() < 1e-9);
        assert!((class_confidence(CoercionClass::Retraction) - 0.55).abs() < 1e-9);
        assert!((class_confidence(CoercionClass::Projection) - 0.35).abs() < 1e-9);
    }
}
