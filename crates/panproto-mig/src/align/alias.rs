//! Domain-agnostic alias-dictionary alignment strategy.
//!
//! Maintains clusters of field-name synonyms that recur across protocols
//! (e.g. `createdAt ≡ created ≡ timestamp`). Casing variants (`camelCase`,
//! `snake_case`, kebab-case) are handled automatically via token
//! normalization so the dictionary itself stays compact.
//!
//! Per the protocol-genericity principle, the built-in clusters mention
//! **no** specific protocol. Protocol-specific cartridges may be merged
//! in at load time by callers via [`AliasDict::extend`].

use std::collections::HashMap;

use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_and_constraints_compatible};

/// A cluster of mutually-aliased terms. All members of the same cluster
/// are treated as interchangeable at alignment time.
#[derive(Clone, Debug, Default)]
pub struct AliasDict {
    /// Canonical-form term → cluster id.
    term_to_cluster: HashMap<String, usize>,
    /// All clusters (each is the list of canonical terms it contains).
    clusters: Vec<Vec<String>>,
}

impl AliasDict {
    /// Construct an empty dictionary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a synonym cluster. Each term is canonicalized (lowercased,
    /// underscores/dashes stripped, camelCase flattened) before insertion.
    ///
    /// The operation is a union-of-equivalence-classes: if any canonical
    /// term in the new cluster is already bound to an existing cluster,
    /// that cluster is extended in place with the fresh terms (and any
    /// other pre-existing clusters touched by the new cluster are
    /// merged into the first). This preserves the user's stated
    /// invariant that every term in one `add_cluster` call is mutually
    /// aliased, even when the call partially overlaps prior
    /// registrations.
    ///
    /// Edge cases:
    /// - Exact duplicate (every term already maps to the same existing
    ///   cluster): no-op.
    /// - Fully disjoint: allocate a new cluster.
    /// - Partial overlap with one existing cluster: extend it.
    /// - Partial overlap with multiple existing clusters: merge them
    ///   into the lowest-indexed one.
    ///
    /// No empty slots are left in `self.clusters`; merged clusters have
    /// their contents moved into the surviving cluster and the emptied
    /// entry is left as an empty `Vec` (so cluster ids remain stable
    /// for `term_to_cluster` lookups). `cluster_count()` counts only
    /// non-empty clusters.
    pub fn add_cluster<I, S>(&mut self, cluster: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let canonical: Vec<String> = cluster
            .into_iter()
            .map(|s| canonical_form(s.as_ref()))
            .filter(|s| !s.is_empty())
            .collect();
        if canonical.len() < 2 {
            return;
        }

        // Collect the distinct existing cluster ids touched by `canonical`.
        let mut touched: Vec<usize> = canonical
            .iter()
            .filter_map(|t| self.term_to_cluster.get(t).copied())
            .collect();
        touched.sort_unstable();
        touched.dedup();

        match touched.as_slice() {
            // Fully disjoint: allocate.
            [] => {
                let cluster_id = self.clusters.len();
                for term in &canonical {
                    self.term_to_cluster.insert(term.clone(), cluster_id);
                }
                self.clusters.push(canonical);
            }
            // Exactly one touched cluster.
            [only] => {
                let survivor = *only;
                // If every term already maps to that cluster, this is
                // an exact duplicate. No-op.
                if canonical
                    .iter()
                    .all(|t| self.term_to_cluster.get(t) == Some(&survivor))
                {
                    return;
                }
                // Extend the existing cluster with fresh terms. Keep
                // pre-existing terms' bindings intact (they already
                // point at `survivor`).
                for term in &canonical {
                    if !self.term_to_cluster.contains_key(term) {
                        self.term_to_cluster.insert(term.clone(), survivor);
                        self.clusters[survivor].push(term.clone());
                    }
                }
            }
            // Multiple touched clusters: merge them all into the
            // lowest-indexed one, then add any fresh terms.
            _ => {
                let survivor = touched[0];
                for &victim in &touched[1..] {
                    let moved = std::mem::take(&mut self.clusters[victim]);
                    for term in moved {
                        // Re-point every victim-bound term at survivor.
                        // `term_to_cluster` may already reflect the
                        // victim id even for canonical duplicates; we
                        // unconditionally re-insert to survivor.
                        self.term_to_cluster.insert(term.clone(), survivor);
                        if !self.clusters[survivor].contains(&term) {
                            self.clusters[survivor].push(term);
                        }
                    }
                }
                // Any term in `canonical` that was previously bound to
                // a victim is now bound to survivor; fresh terms are
                // inserted and appended.
                for term in &canonical {
                    if !self.term_to_cluster.contains_key(term) {
                        self.term_to_cluster.insert(term.clone(), survivor);
                        self.clusters[survivor].push(term.clone());
                    }
                }
            }
        }
    }

    /// Number of non-empty clusters. Emptied entries left behind by
    /// merges in [`Self::add_cluster`] are not counted.
    #[must_use]
    pub fn cluster_count(&self) -> usize {
        self.clusters.iter().filter(|c| !c.is_empty()).count()
    }

    /// Merge additional clusters from another dictionary.
    pub fn extend(&mut self, other: &Self) {
        for cluster in &other.clusters {
            self.add_cluster(cluster.iter().map(String::as_str));
        }
    }

    /// Test whether two strings refer to aliased terms. Casing variants
    /// and separator conventions are normalized out before lookup; this
    /// function therefore also returns `true` for `camelCase`/`snake_case`
    /// variants of the same word (`createdAt` vs `created_at`).
    #[must_use]
    pub fn are_aliases(&self, a: &str, b: &str) -> bool {
        let ca = canonical_form(a);
        let cb = canonical_form(b);
        // Two inputs that both reduce to empty carry no information; do
        // not treat them as aliases.
        if ca.is_empty() || cb.is_empty() {
            return false;
        }
        if ca == cb {
            return true;
        }
        match (self.term_to_cluster.get(&ca), self.term_to_cluster.get(&cb)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// Return the cluster containing `term`, if any.
    #[must_use]
    pub fn cluster_for(&self, term: &str) -> Option<&[String]> {
        let canonical = canonical_form(term);
        self.term_to_cluster
            .get(&canonical)
            .and_then(|id| self.clusters.get(*id).map(Vec::as_slice))
    }
}

/// Canonicalize `s` for dictionary lookup: lowercased, with
/// underscores/dashes removed and camelCase boundaries flattened.
/// `createdAt`, `created_at`, `CreatedAt`, and `created-at` all map to
/// the same canonical form `createdat`.
fn canonical_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// The built-in domain-agnostic alias dictionary.
///
/// Clusters are deliberately generic: no protocol-specific terms. Each
/// cluster is a group of English-language field names that recur across
/// every protocol panproto parses.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_alias_dict() -> AliasDict {
    let mut dict = AliasDict::new();

    // Temporal event timestamps. All flavors of "when did this thing
    // happen" alias loosely so the CSP gets a candidate; the naturality
    // check still has the final say. Keep distinct concepts (creation,
    // update, sending, indexing) all in this cluster — Lenient/
    // Exploratory rely on this to surface cross-event candidates such
    // as post.createdAt ↔ message.sentAt.
    dict.add_cluster([
        "createdAt",
        "created",
        "creationTime",
        "updatedAt",
        "updated",
        "modifiedAt",
        "modified",
        "lastModified",
        "indexedAt",
        "indexed",
        "ingestedAt",
        "processedAt",
        "sentAt",
        "sent",
        "sendTime",
        "postedAt",
        "publishedAt",
        "receivedAt",
        "timestamp",
        "ts",
        "date",
        "datetime",
        "when",
        "time",
        "mtime",
        "ctime",
        "atime",
    ]);

    // Identity and references.
    dict.add_cluster([
        "id",
        "identifier",
        "key",
        "uuid",
        "guid",
        "primary_key",
        "pk",
    ]);
    dict.add_cluster([
        "ref",
        "reference",
        "target",
        "subject",
        "object",
        "referent",
    ]);

    // Content/body.
    dict.add_cluster([
        "text", "body", "content", "message", "data", "payload", "value",
    ]);

    // Naming.
    dict.add_cluster([
        "name",
        "displayName",
        "display_name",
        "label",
        "title",
        "caption",
        "heading",
    ]);

    // Description.
    dict.add_cluster(["description", "summary", "about", "notes", "bio", "details"]);

    // URI-ish.
    dict.add_cluster([
        "uri", "url", "link", "href", "location", "address", "endpoint",
    ]);
    dict.add_cluster(["hash", "digest", "checksum", "fingerprint", "cid"]);

    // Quantity.
    dict.add_cluster([
        "count", "total", "num", "number", "n", "size", "length", "len",
    ]);

    // Status/state.
    dict.add_cluster(["status", "state", "phase", "stage", "condition"]);

    // Authorship.
    dict.add_cluster(["author", "creator", "owner", "user", "actor", "by"]);

    // Versioning.
    dict.add_cluster(["version", "rev", "revision", "ver", "v"]);

    // Tags/categories.
    dict.add_cluster(["tags", "labels", "categories", "keywords", "topics"]);

    // Boolean flags.
    dict.add_cluster(["active", "enabled", "on"]);
    dict.add_cluster(["deleted", "removed", "archived", "tombstoned"]);

    // Ordering.
    dict.add_cluster(["order", "rank", "position", "index", "seq", "sequence"]);

    // Parent/child structure.
    dict.add_cluster(["parent", "parentId", "parent_id", "ancestor", "up"]);
    dict.add_cluster(["child", "children", "descendants", "items", "entries"]);

    dict
}

/// Emit anchors for pairs whose outgoing-edge name sets overlap under
/// the alias dictionary.
///
/// The child-name signature of a container vertex is the score key: if
/// source vertex `s` has a prop-edge named `createdAt` and target `t`
/// has a prop-edge named `sentAt`, and both names are in the temporal
/// alias cluster, that counts as evidence that `s` and `t` may align.
///
/// For each source vertex, scores every kind-compatible target vertex by
/// a symmetric size-normalized overlap: the number of source edge names
/// that alias-match some target edge name, divided by the larger of the
/// two edge-name list sizes. This penalizes pairings where one side has
/// many extra unmatched fields. Returns all `(s, t)` pairs whose score
/// (plus a small bonus when the vertex names themselves alias) clears
/// `0.4`.
#[must_use]
pub fn alias_anchors(src: &Schema, tgt: &Schema, dict: &AliasDict) -> Vec<Anchor> {
    let mut out = Vec::new();

    let mut src_ids: Vec<&panproto_gat::Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut tgt_ids: Vec<&panproto_gat::Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for src_id in src_ids.iter().copied() {
        let src_edge_names: Vec<&str> = src
            .outgoing_edges(src_id)
            .iter()
            .filter_map(|e| e.name.as_deref())
            .collect();

        if src_edge_names.is_empty() {
            // Pure leaf: we cannot score it by children. Fall back to
            // name-level alias comparison against each candidate.
            for tgt_id in tgt_ids.iter().copied() {
                if !kinds_and_constraints_compatible(src, src_id, tgt, tgt_id) {
                    continue;
                }
                if dict.are_aliases(src_id.as_str(), tgt_id.as_str())
                    && src_id.as_str() != tgt_id.as_str()
                {
                    out.push(Anchor {
                        src: src_id.clone(),
                        tgt: tgt_id.clone(),
                        confidence: 0.85,
                        strategy: StrategyTag::Alias,
                        explanation: format!(
                            "alias match: {} ↔ {}",
                            src_id.as_str(),
                            tgt_id.as_str()
                        ),
                    });
                }
            }
            continue;
        }

        for tgt_id in tgt_ids.iter().copied() {
            if !kinds_and_constraints_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let tgt_edge_names: Vec<&str> = tgt
                .outgoing_edges(tgt_id)
                .iter()
                .filter_map(|e| e.name.as_deref())
                .collect();
            if tgt_edge_names.is_empty() {
                continue;
            }

            let (score, matched) = alias_edge_overlap(&src_edge_names, &tgt_edge_names, dict);
            if matched == 0 {
                continue;
            }
            // Name-level bonus if the vertex names themselves alias.
            let name_bonus = if dict.are_aliases(src_id.as_str(), tgt_id.as_str()) {
                0.1
            } else {
                0.0
            };
            let confidence = (score + name_bonus).clamp(0.0, 1.0);
            if confidence < 0.4 {
                continue;
            }
            out.push(Anchor {
                src: src_id.clone(),
                tgt: tgt_id.clone(),
                confidence,
                strategy: StrategyTag::Alias,
                explanation: format!(
                    "alias-match on {matched} shared child field name(s): {} ↔ {}",
                    src_id.as_str(),
                    tgt_id.as_str()
                ),
            });
        }
    }

    out
}

/// Size-normalized overlap: counts each source name as "matched" if some
/// target name is its alias, then divides by `max(|src|, |tgt|)`. This
/// is symmetric in list sizes (swapping sides preserves the score) but
/// is NOT the fraction of source fields that matched: when the target
/// has many unmatched extra fields, the denominator grows and the score
/// shrinks accordingly. Empty-empty returns `(1.0, 0)` as a vacuous
/// edge case; callers gate emission on `matched > 0`. Returns
/// `(score, matched_count)`.
fn alias_edge_overlap(src_names: &[&str], tgt_names: &[&str], dict: &AliasDict) -> (f64, usize) {
    if src_names.is_empty() && tgt_names.is_empty() {
        return (1.0, 0);
    }
    let mut matched = 0usize;
    for s in src_names {
        if tgt_names.iter().any(|t| dict.are_aliases(s, t)) {
            matched += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let denom = (src_names.len().max(tgt_names.len())) as f64;
    let score = if denom > 0.0 {
        #[allow(clippy::cast_precision_loss)]
        {
            matched as f64 / denom
        }
    } else {
        0.0
    };
    (score, matched)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::option_if_let_else)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            b = b.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            b = b.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn canonical_form_normalizes_casings() {
        assert_eq!(canonical_form("createdAt"), "createdat");
        assert_eq!(canonical_form("created_at"), "createdat");
        assert_eq!(canonical_form("CREATED-AT"), "createdat");
        assert_eq!(canonical_form("Created At"), "createdat");
    }

    #[test]
    fn are_aliases_handles_casing_without_cluster() {
        let dict = AliasDict::new();
        assert!(dict.are_aliases("createdAt", "created_at"));
        assert!(dict.are_aliases("displayName", "DisplayName"));
        assert!(!dict.are_aliases("createdAt", "sentAt"));
    }

    #[test]
    fn default_dict_matches_temporal_aliases() {
        let dict = default_alias_dict();
        assert!(dict.are_aliases("createdAt", "sentAt"));
        assert!(dict.are_aliases("createdAt", "timestamp"));
        assert!(dict.are_aliases("indexedAt", "processedAt"));
        assert!(!dict.are_aliases("createdAt", "text"));
    }

    #[test]
    fn default_dict_matches_identity_aliases() {
        let dict = default_alias_dict();
        assert!(dict.are_aliases("id", "uuid"));
        assert!(dict.are_aliases("ref", "target"));
        assert!(dict.are_aliases("subject", "referent"));
    }

    #[test]
    fn are_aliases_rejects_empty_canonical() {
        let dict = AliasDict::new();
        assert!(!dict.are_aliases("", ""));
        assert!(!dict.are_aliases("___", "---"));
        assert!(!dict.are_aliases("foo", ""));
    }

    #[test]
    fn add_cluster_ignores_singletons() {
        let mut dict = AliasDict::new();
        dict.add_cluster(["solo"]);
        assert_eq!(dict.cluster_count(), 0);
        // After filtering empty canonical forms, only one term remains.
        dict.add_cluster(["bar", "___"]);
        assert_eq!(dict.cluster_count(), 0);
        dict.add_cluster(["bar", "baz"]);
        assert_eq!(dict.cluster_count(), 1);
        assert!(dict.are_aliases("bar", "baz"));
    }

    #[test]
    fn alias_edge_overlap_asymmetric_sizes_uses_max_denominator() {
        // 1 match on src side, but target has 9 extra unmatched fields:
        // score = 1 / max(1, 10) = 0.1. Use an empty dict so that "a"..
        // "i" do not incidentally alias with anything in src.
        let dict = AliasDict::new();
        let (score_small_large, matched) = alias_edge_overlap(
            &["id"],
            &["id", "a", "b", "c", "d", "e", "f", "g", "i", "j"],
            &dict,
        );
        assert_eq!(matched, 1);
        assert!((score_small_large - 0.1).abs() < 1e-9);

        // Swapping src and tgt yields the same score: the metric is
        // symmetric in list sizes.
        let (score_large_small, matched_swapped) = alias_edge_overlap(
            &["id", "a", "b", "c", "d", "e", "f", "g", "i", "j"],
            &["id"],
            &dict,
        );
        assert_eq!(matched_swapped, 1);
        assert!((score_large_small - score_small_large).abs() < 1e-9);
    }

    #[test]
    fn alias_edge_overlap_duplicate_src_names_double_count() {
        // When the src side contains duplicate edge names (a schema
        // malformation we treat defensively), each
        // duplicate is tested independently via
        // `tgt_names.iter().any(...)`, so duplicates double-count in
        // `matched`. `matched` can therefore exceed the number of
        // *distinct* matching names. Pin the behaviour so it is an
        // observed property rather than an unintentional drift.
        let dict = default_alias_dict();
        // "text" and "body" both lie in the content cluster; duplicates
        // on the src side each score as a match.
        let (score, matched) = alias_edge_overlap(&["text", "text", "body"], &["body"], &dict);
        assert_eq!(
            matched, 3,
            "duplicate src names count independently; matched = 3 not 2"
        );
        // denominator is max(|src|=3, |tgt|=1) = 3, so score = 3/3 = 1.0.
        assert!(
            (score - 1.0).abs() < 1e-9,
            "duplicate-src double-counting can saturate the score: {score}"
        );
    }

    #[test]
    fn alias_edge_overlap_empty_sides() {
        let dict = AliasDict::new();
        // Empty-empty: documented as 1.0 but matched=0.
        let (score, matched) = alias_edge_overlap(&[], &[], &dict);
        assert_eq!(matched, 0);
        assert!((score - 1.0).abs() < 1e-9);
        // Empty-nonempty: no matches possible.
        let (score, matched) = alias_edge_overlap(&[], &["x"], &dict);
        assert_eq!(matched, 0);
        assert!(score.abs() < f64::EPSILON);
    }

    #[test]
    fn alias_anchors_minimal_disjoint_schema_returns_empty() {
        // A schema with a single unrelated vertex cannot match anything in
        // a target schema with a different single unrelated vertex; this
        // is the smallest possible case and verifies no-panic behavior.
        let src = build_schema(&[("unused_x", "string")], &[]);
        let tgt = build_schema(&[("unused_y", "integer")], &[]);
        let dict = default_alias_dict();
        assert!(alias_anchors(&src, &tgt, &dict).is_empty());
    }

    #[test]
    fn alias_anchors_deterministic_emission() {
        let dict = default_alias_dict();
        let perms: [&[(&str, &str)]; 2] = [
            &[("aaa", "object"), ("bbb", "object"), ("ccc", "object")],
            &[("ccc", "object"), ("aaa", "object"), ("bbb", "object")],
        ];
        let targets = build_schema(
            &[
                ("aaa", "object"),
                ("bbb", "object"),
                ("ccc", "object"),
                ("aaa.x", "string"),
                ("bbb.x", "string"),
                ("ccc.x", "string"),
            ],
            &[
                ("aaa", "aaa.x", "prop", "id"),
                ("bbb", "bbb.x", "prop", "id"),
                ("ccc", "ccc.x", "prop", "id"),
            ],
        );
        let mut results = Vec::new();
        for verts in perms {
            let leaves: Vec<(&str, &str)> = verts
                .iter()
                .map(|(id, _)| {
                    (
                        Box::leak(format!("{id}.x").into_boxed_str()) as &str,
                        "string",
                    )
                })
                .collect();
            let all_verts: Vec<(&str, &str)> = verts.iter().copied().chain(leaves).collect();
            let edges: Vec<(&str, &str, &str, &str)> = verts
                .iter()
                .map(|(id, _)| {
                    (
                        *id,
                        Box::leak(format!("{id}.x").into_boxed_str()) as &str,
                        "prop",
                        "uuid",
                    )
                })
                .collect();
            let src = build_schema(&all_verts, &edges);
            let anchors = alias_anchors(&src, &targets, &dict);
            let mut pairs: Vec<(String, String)> = anchors
                .iter()
                .map(|a| (a.src.as_str().into(), a.tgt.as_str().into()))
                .collect();
            pairs.sort();
            results.push(pairs);
        }
        assert_eq!(
            results[0], results[1],
            "anchor set must be permutation-invariant"
        );
    }

    #[test]
    fn alias_anchors_emit_only_kind_compatible() {
        let dict = default_alias_dict();
        let src = build_schema(
            &[("root", "object"), ("root.id", "string")],
            &[("root", "root.id", "prop", "id")],
        );
        let tgt = build_schema(
            &[("root", "integer"), ("root.id", "string")],
            &[("root", "root.id", "prop", "uuid")],
        );
        for anchor in alias_anchors(&src, &tgt, &dict) {
            assert!(super::super::kinds_compatible(
                &src,
                &anchor.src,
                &tgt,
                &anchor.tgt
            ));
        }
    }

    #[test]
    fn alias_anchors_single_isolated_vertex() {
        // Schema-building requires at least one vertex, so the smallest
        // schema is a single isolated vertex. Asserts no-panic on the
        // thinnest legal input.
        let src = build_schema(&[("lone", "string")], &[]);
        let tgt = build_schema(&[("other", "integer")], &[]);
        let dict = default_alias_dict();
        assert!(alias_anchors(&src, &tgt, &dict).is_empty());
    }

    #[test]
    fn alias_anchors_bit_identical_across_100_runs() {
        let dict = default_alias_dict();
        let src = build_schema(
            &[
                ("root", "object"),
                ("root.text", "string"),
                ("root.createdAt", "string"),
            ],
            &[
                ("root", "root.text", "prop", "text"),
                ("root", "root.createdAt", "prop", "createdAt"),
            ],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.body", "string"),
                ("root.sentAt", "string"),
            ],
            &[
                ("root", "root.body", "prop", "body"),
                ("root", "root.sentAt", "prop", "sentAt"),
            ],
        );
        let baseline: Vec<(String, String, u64)> = alias_anchors(&src, &tgt, &dict)
            .iter()
            .map(|a| {
                (
                    a.src.as_str().into(),
                    a.tgt.as_str().into(),
                    a.confidence.to_bits(),
                )
            })
            .collect();
        for _ in 0..100 {
            let again: Vec<(String, String, u64)> = alias_anchors(&src, &tgt, &dict)
                .iter()
                .map(|a| {
                    (
                        a.src.as_str().into(),
                        a.tgt.as_str().into(),
                        a.confidence.to_bits(),
                    )
                })
                .collect();
            assert_eq!(again, baseline);
        }
    }

    #[test]
    fn add_cluster_is_idempotent_on_exact_duplicate() {
        // Calling `add_cluster` with a cluster already fully
        // contained in the registry must be idempotent: no new entry
        // allocated in `self.clusters`, and `cluster_count()` stays
        // unchanged.
        let mut dict = AliasDict::new();
        dict.add_cluster(["foo", "bar"]);
        assert_eq!(dict.cluster_count(), 1);
        dict.add_cluster(["foo", "bar"]);
        assert_eq!(
            dict.cluster_count(),
            1,
            "duplicate add_cluster must not create a new cluster"
        );
        // Casing/separator variants canonicalize to the same cluster
        // and must also be idempotent.
        dict.add_cluster(["FOO", "Bar"]);
        assert_eq!(dict.cluster_count(), 1);
        // Disjoint cluster still allocates.
        dict.add_cluster(["baz", "qux"]);
        assert_eq!(dict.cluster_count(), 2);
        // Partial overlap: "foo" already bound (cluster 0); "new" is
        // fresh. The user's assertion "foo and new are aliases"
        // extends the existing cluster in place so `foo` and `new`
        // are mutually aliased.
        dict.add_cluster(["foo", "new"]);
        assert_eq!(
            dict.cluster_count(),
            2,
            "partial overlap must extend an existing cluster, not allocate"
        );
        assert!(dict.are_aliases("foo", "bar"));
        assert!(
            dict.are_aliases("foo", "new"),
            "partial overlap must make fresh terms aliases of the existing cluster"
        );
        assert!(
            dict.are_aliases("bar", "new"),
            "transitivity must hold after in-place extension"
        );
    }

    #[test]
    fn add_cluster_partial_overlap_extends_in_place() {
        // `add_cluster(["a","b","c"])` against an existing cluster
        // `{a,b}` must extend the existing cluster in place so that
        // `are_aliases(a, c) == true`. Pin the union-of-equivalence-
        // classes semantics so a future rewrite cannot silently split
        // the user's stated equivalence "a ≡ b ≡ c".
        let mut dict = AliasDict::new();
        dict.add_cluster(["a", "b"]);
        dict.add_cluster(["a", "b", "c"]);
        assert_eq!(dict.cluster_count(), 1);
        assert!(dict.are_aliases("a", "c"));
        assert!(dict.are_aliases("b", "c"));
        // `cluster_for` must be consistent across every member.
        let mut via_a: Vec<String> = dict.cluster_for("a").unwrap().to_vec();
        via_a.sort();
        let mut via_c: Vec<String> = dict.cluster_for("c").unwrap().to_vec();
        via_c.sort();
        assert_eq!(
            via_a, via_c,
            "cluster_for must be consistent for cluster members"
        );
        assert_eq!(via_a, vec!["a", "b", "c"]);
    }

    #[test]
    fn add_cluster_merges_multiple_existing_clusters() {
        // When a new cluster bridges two previously-disjoint clusters,
        // the equivalence-class semantics demand that all of them fuse.
        let mut dict = AliasDict::new();
        dict.add_cluster(["a", "b"]);
        dict.add_cluster(["c", "d"]);
        assert_eq!(dict.cluster_count(), 2);
        dict.add_cluster(["b", "c"]);
        assert_eq!(
            dict.cluster_count(),
            1,
            "bridging cluster must merge both pre-existing clusters"
        );
        for x in ["a", "b", "c", "d"] {
            for y in ["a", "b", "c", "d"] {
                if x != y {
                    assert!(dict.are_aliases(x, y), "transitivity failed on {x} ↔ {y}");
                }
            }
        }
        // All four members observe the same merged cluster.
        let mut via_a: Vec<String> = dict.cluster_for("a").unwrap().to_vec();
        via_a.sort();
        for x in ["b", "c", "d"] {
            let mut via_x: Vec<String> = dict.cluster_for(x).unwrap().to_vec();
            via_x.sort();
            assert_eq!(via_a, via_x, "cluster_for must match for {x}");
        }
    }

    #[test]
    fn canonical_form_handles_multi_kilobyte_input() {
        // `canonical_form` should allocate O(n) and run in O(n). Stress
        // it with ~55 KB of input to confirm no unbounded growth.
        let big: String = "abcDEF_123-".repeat(5_000);
        let canon = canonical_form(&big);
        // Of the 11 chars in the repeated unit, 9 are alphanumeric
        // (a,b,c,D,E,F,1,2,3); '_' and '-' drop. 9 × 5000 = 45_000.
        assert_eq!(canon.len(), 45_000);
        assert!(canon.chars().all(|c| !c.is_uppercase()));
    }

    #[test]
    fn alias_anchors_leaf_vs_leaf_emits_on_default_temporal_cluster() {
        // Concern 9: when both sides are pure leaves (no outgoing edges)
        // and the vertex names themselves alias under the default
        // dictionary, `alias_anchors` emits a name-level anchor.
        let src = build_schema(&[("created", "string")], &[]);
        let tgt = build_schema(&[("createdAt", "string")], &[]);
        let dict = default_alias_dict();
        let anchors = alias_anchors(&src, &tgt, &dict);
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "created" && a.tgt.as_str() == "createdAt"),
            "leaf-vs-leaf temporal aliases must emit an anchor; got {anchors:?}"
        );
    }

    #[test]
    fn extend_with_overlapping_dict_does_not_create_duplicates() {
        // Regression: `extend` delegates to `add_cluster`, so before
        // the idempotency fix merging a dictionary against itself
        // doubled `cluster_count()`.
        let base = default_alias_dict();
        let base_count = base.cluster_count();
        let mut merged = base.clone();
        merged.extend(&base);
        assert_eq!(
            merged.cluster_count(),
            base_count,
            "extend with overlapping clusters must be idempotent"
        );
        // Alias relationships are unchanged.
        assert!(merged.are_aliases("createdAt", "sentAt"));
        assert!(merged.are_aliases("id", "uuid"));
    }

    #[test]
    fn alias_anchors_composite_source_leaf_target_emits_nothing() {
        // Pins the documented asymmetry in `alias_anchors`: when the
        // source is composite (has named outgoing edges) and the target
        // is a pure leaf (no outgoing edges), no anchor is emitted
        // even if the names would alias. The symmetric case (leaf
        // source, composite target) DOES attempt a name-level alias
        // check. Different layouts → not a mismatch, but a deliberate
        // asymmetry because composite↔leaf is wrap/unwrap territory,
        // not an alias pairing.
        let src = build_schema(
            &[("id", "object"), ("id.x", "string")],
            &[("id", "id.x", "prop", "x")],
        );
        let tgt = build_schema(&[("uuid", "object")], &[]);
        let dict = default_alias_dict();
        let anchors = alias_anchors(&src, &tgt, &dict);
        assert!(
            anchors
                .iter()
                .all(|a| !(a.src.as_str() == "id" && a.tgt.as_str() == "uuid")),
            "composite-source vs leaf-target must not emit alias anchor: {anchors:?}"
        );
    }

    #[test]
    fn alias_anchors_link_temporally_named_leaves() {
        // Two records whose children have alias-matching names: `createdAt`
        // ↔ `sentAt` (temporal), `text` ↔ `body` (content).
        let src = build_schema(
            &[
                ("root", "object"),
                ("root.text", "string"),
                ("root.createdAt", "string"),
            ],
            &[
                ("root", "root.text", "prop", "text"),
                ("root", "root.createdAt", "prop", "createdAt"),
            ],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.body", "string"),
                ("root.sentAt", "string"),
            ],
            &[
                ("root", "root.body", "prop", "body"),
                ("root", "root.sentAt", "prop", "sentAt"),
            ],
        );

        let dict = default_alias_dict();
        let anchors = alias_anchors(&src, &tgt, &dict);

        // Root should align; its two children should each get an alias anchor too.
        let root_anchor = anchors
            .iter()
            .find(|a| a.src.as_str() == "root" && a.tgt.as_str() == "root");
        assert!(
            root_anchor.is_some(),
            "root↔root should anchor by shared alias-child-names"
        );
    }

    #[test]
    fn canonical_form_strips_extended_ascii_and_diacritics() {
        // `canonical_form` uses `is_ascii_alphanumeric`, so anything
        // outside `[A-Za-z0-9]` — including Latin-1 letters with
        // diacritics and combining marks — is stripped. Two names
        // that differ only in their diacritics collapse to the same
        // canonical form. Pin the behaviour so a future widening to
        // Unicode-aware folding has to update this test deliberately.
        assert_eq!(canonical_form("Ä"), "");
        assert_eq!(canonical_form("ÄÅÆ"), "");
        assert_eq!(canonical_form("café"), "caf");
        assert_eq!(canonical_form("cafe"), "cafe");
        // The NFC `café` (precomposed) and NFD `cafe\u{0301}` both
        // collapse to "caf" and "cafe" respectively once the non-ASCII
        // bits are stripped; they are NOT rendered equivalent.
        assert_ne!(
            canonical_form("caf\u{00E9}"),
            canonical_form("cafe\u{0301}")
        );
        // Diacritic-only distinctions collapse: `naïve` ↦ `nave`,
        // `naive` ↦ `naive`, so they are NOT aliases.
        assert_ne!(canonical_form("naïve"), canonical_form("naive"));
    }

    #[test]
    fn alias_anchors_ignore_edge_kind_when_matching_names() {
        // `alias_edge_overlap` scores overlap on edge *names* and
        // never consults the edge kind.
        // A source with a `prop` edge named `id` and a target with
        // (say) an `item` edge also named `id` therefore register as
        // a match even though the edge kinds disagree. The CSP's
        // naturality check is the ultimate guard; this test pins the
        // fact that the alias anchor itself is kind-agnostic on the
        // edge level (it does check vertex kinds via
        // `kinds_compatible`).
        let src = build_schema(
            &[("r", "object"), ("r.id", "string")],
            &[("r", "r.id", "prop", "id")],
        );
        // Use a different edge kind on the target side; the edge name
        // is the same.
        let tgt = build_schema(
            &[("r", "object"), ("r.id", "string")],
            &[("r", "r.id", "item", "id")],
        );
        let dict = default_alias_dict();
        let anchors = alias_anchors(&src, &tgt, &dict);
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "r" && a.tgt.as_str() == "r"),
            "alias_anchors scores on edge *names* regardless of edge kind: {anchors:?}"
        );
    }
}
