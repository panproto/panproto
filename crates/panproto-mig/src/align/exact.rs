//! Exact-name-equality alignment strategy.
//!
//! Proposes an anchor `(s, t)` whenever `s` and `t` have identical names
//! and compatible kinds. Confidence is `1.0`.

use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};

/// Emit anchors for every source vertex whose name exists in `tgt` with
/// the same kind.
#[must_use]
pub fn exact_anchors(src: &Schema, tgt: &Schema) -> Vec<Anchor> {
    let mut out = Vec::new();
    for src_id in src.vertices.keys() {
        if tgt.has_vertex(src_id) && kinds_compatible(src, src_id, tgt, src_id) {
            out.push(Anchor {
                src: src_id.clone(),
                tgt: src_id.clone(),
                confidence: 1.0,
                strategy: StrategyTag::Exact,
                explanation: format!("exact name match: {}", src_id.as_str()),
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema(verts: &[(&str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, k) in verts {
            b = b.vertex(id, k, None::<&str>).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn emits_anchors_only_for_shared_names_with_matching_kind() {
        let src = schema(&[("a", "string"), ("b", "object"), ("c", "string")]);
        let tgt = schema(&[("a", "string"), ("b", "string"), ("d", "string")]);

        let anchors = exact_anchors(&src, &tgt);
        let pairs: Vec<_> = anchors
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();

        assert!(pairs.contains(&("a".into(), "a".into())));
        // "b" exists on both sides but kinds differ → excluded.
        assert!(!pairs.iter().any(|(s, _)| s == "b"));
        // "c" missing on target → excluded.
        assert!(!pairs.iter().any(|(s, _)| s == "c"));
    }
}
