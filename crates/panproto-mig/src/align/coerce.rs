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
/// `Iso` → `0.8`, `Retraction` → `0.55`, `Projection` → `0.35`. Ties
/// at the same confidence are broken by alphabetic witness name so
/// output is reproducible across runs.
///
/// Returns one coerce-anchor per source vertex at most, picking the
/// highest-confidence target among bridgeable kinds.
///
/// # Known limitation: CSP does not consume these anchors
///
/// The bare `.anchor` values this function produces are merged into
/// the CSP seed pool by
/// `panproto_lens::auto_lens::run_strategies`, but the morphism
/// search in [`crate::hom_search::find_morphisms_constrained`] still
/// rejects kind-mismatched vertex pairs at domain-construction time
/// (it filters candidate targets by `kinds_compatible`). A coerce
/// anchor therefore cannot steer the chosen morphism today; its
/// witness metadata is surfaced only on
/// `AutoLensResult.coerce_proposals` for CLI / Python / WASM
/// consumers, which may manually prepend a `CoerceSort` endofunctor
/// and re-run the migration.
///
/// The fix for end-to-end emission is either (a) relax
/// `kinds_compatible` in `hom_search::new_constrained` when a witness
/// exists and thread the witness through factorization, or (b)
/// post-process the morphism in `auto_lens`: when a source sort has
/// no assigned target, consult `coerce_proposals`, synthesize a
/// `CoerceSort` step, and re-factorize. Neither path is implemented
/// in this tier; treat this function as authoritative for *which*
/// witness would bridge which kind pair, but NOT as a guarantee that
/// the auto-generated lens contains the corresponding `CoerceSort`
/// step.
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
                // Tie-break: when two witnesses share the same
                // confidence (e.g. two Retractions for the same kind
                // pair), pick the one whose name sorts earliest. This
                // makes the emitted anchor reproducible even when the
                // library's internal iteration order for a kind pair
                // is an insertion-order artifact.
                let swap = best.as_ref().is_none_or(|(_, prev_w, prev_c)| {
                    // `total_cmp` gives a total order even at NaN/±0 so
                    // the tie-break stays deterministic if
                    // `class_confidence` ever grows to return non-finite
                    // values.
                    match confidence.total_cmp(prev_c) {
                        std::cmp::Ordering::Greater => true,
                        std::cmp::Ordering::Less => false,
                        std::cmp::Ordering::Equal => witness.name < prev_w.name,
                    }
                });
                if swap {
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
///
/// Covers the seven primitive carriers
/// (`bool`/`boolean`, `int`/`integer`, `float`/`number`,
/// `str`/`string`, `bytes`, `token`, `null`). Every other vertex
/// kind string - including protocol-specific nominal sorts such as
/// `record`, `array`, `variant`, or cartridge-defined carriers like
/// `uuid` or `timestamp` - is intentionally left out of the returned
/// map. `coerce_anchors` skips any vertex whose kind does not appear
/// here, so unknown kinds neither seed a coercion nor produce a
/// false match. Downstream cartridges that wish to coerce over
/// custom carriers should register additional witnesses keyed on one
/// of the recognized primitives and rename the custom vertex kind
/// accordingly.
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
        // tie-breaks on witness name (alphabetic). This test locks
        // the behaviour: whatever witness wins, it must be a library
        // witness (not dropped), and the anchor's class must match
        // the library entry.
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
    fn coerce_anchors_tie_breaks_on_witness_name() {
        // Two int→str witnesses with the same Retraction class
        // (equal confidence). Registered in reverse-alphabetic order
        // so the stable-over-insertion policy must pick "aaa" not
        // "zzz". This pins deterministic output under HashMap
        // iteration variance.
        let mut lib = WitnessLibrary::new();
        let mut alpha = crate::coerce::witness::int_to_str_witness();
        alpha.name = "aaa_int_to_str".to_owned();
        let mut omega = crate::coerce::witness::int_to_str_witness();
        omega.name = "zzz_int_to_str".to_owned();
        // Insert in z-then-a order so a purely-insertion-order picker
        // would choose "zzz_int_to_str" first.
        lib.register(omega);
        lib.register(alpha);

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
        assert_eq!(
            picked.witness_name, "aaa_int_to_str",
            "tie-break must pick alphabetically earliest witness name"
        );
    }

    #[test]
    fn custom_kinds_do_not_participate_in_coerce_anchors() {
        // `schema_value_kinds` only recognizes primitive carriers
        // (bool/int/float/str/bytes/token/null). Any vertex whose kind
        // is a cartridge-defined nominal sort (e.g. "record") is
        // skipped at resolve time, so no coerce anchor is ever emitted
        // for such pairs even when an Int→Str witness exists in the
        // library. This pins the documented behaviour from the
        // function docstring.
        let src = build(
            &[("r", "record"), ("r.rec", "record")],
            &[("r", "r.rec", "prop", "rec")],
        );
        let tgt = build(
            &[("r", "record"), ("r.rec", "string")],
            &[("r", "r.rec", "prop", "rec")],
        );
        let lib = default_witness_library();
        let anchors = coerce_anchors(&src, &tgt, &lib);
        // "record" on the source side is not a ValueKind, so r.rec is
        // never considered as a coerce candidate.
        assert!(
            anchors.iter().all(|a| a.anchor.src.as_str() != "r.rec"),
            "non-primitive source kinds must not seed coerce anchors; got {anchors:?}"
        );
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
