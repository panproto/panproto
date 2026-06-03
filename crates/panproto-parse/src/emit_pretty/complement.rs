#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::elidable_lifetime_names,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::single_char_pattern,
    clippy::naive_bytecount,
    clippy::expect_used,
    clippy::redundant_pub_crate,
    clippy::used_underscore_binding,
    clippy::redundant_field_names,
    clippy::struct_field_names,
    clippy::redundant_else,
    clippy::similar_names
)]

//! `emit_pretty::complement` (Phase A decomposition).

use super::{
    ChildCursor, Edge, Grammar, Production, Schema, literal_choice_set, literal_strings,
    yield_of_production,
};

/// The literal keyword / punctuation tokens carried by the rule that an
/// `ALIAS` content references, unwrapping precedence / token wrappers to the
/// head `SYMBOL`. Used to disambiguate two source rules aliased to the same
/// kind by checking the child's recorded operator against each source's
/// literals. Returns empty when the content is not a (wrapped) bare symbol or
/// the referenced rule carries no literals.
pub(crate) fn aliased_source_literals(grammar: &Grammar, content: &Production) -> Vec<String> {
    fn head_symbol(p: &Production) -> Option<&str> {
        match p {
            Production::Symbol { name } => Some(name.as_str()),
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::Reserved { content, .. } => head_symbol(content),
            _ => None,
        }
    }
    head_symbol(content)
        .and_then(|s| grammar.rules.get(s))
        .map(literal_strings)
        .unwrap_or_default()
}

/// The `chose-alt-fingerprint` witness of the first unconsumed cursor edge
/// whose target vertex has kind `kind`, if recorded. This is the child's
/// operator / literal witness, used by the same-kind-alias disambiguation.
pub(crate) fn first_unconsumed_target_fingerprint(
    schema: &Schema,
    cursor: &ChildCursor<'_>,
    kind: &str,
) -> Option<String> {
    let edge = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .map(|(_, e)| e)
        .find(|e| schema.vertices.get(&e.tgt).map(|v| v.kind.as_ref()) == Some(kind))?;
    schema.constraints.get(&edge.tgt).and_then(|cs| {
        cs.iter()
            .find(|c| c.sort.as_ref() == "chose-alt-fingerprint")
            .map(|c| c.value.clone())
    })
}

/// Collect every `(field_name, restricted_token_set)` pair under `production`
/// where the FIELD's body is an ALIAS whose inner content is a CHOICE of
/// pure STRINGs (or a single STRING). Such a FIELD restricts the field's
/// child literal-value to that set: the alternative is only structurally
/// valid for cursors whose field-named edge target carries a literal in
/// the set. Returns an empty vec when `production` has no token-restricted
/// FIELDs.
pub(crate) fn collect_field_token_restrictions<'a>(
    production: &'a Production,
    out: &mut Vec<(&'a str, Vec<&'a str>)>,
) {
    match production {
        Production::Field { name, content } => {
            if let Some(strings) = literal_choice_set(content) {
                out.push((name.as_str(), strings));
            }
            collect_field_token_restrictions(content, out);
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                collect_field_token_restrictions(m, out);
            }
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            collect_field_token_restrictions(content, out);
        }
        _ => {}
    }
}

/// Categorical acceptance predicate: does `production` accept a cursor
/// edge whose field name is `edge_field` (or `child_of`) and whose target
/// vertex has kind `target_kind`?
///
/// Defined inductively over the production tree:
///
/// - `STRING` / `PATTERN` / `BLANK` / ε-only: reject (consume no edges).
/// - `SYMBOL X` (concrete): `edge_field == "child_of"` and `target_kind ⊑ X`.
/// - `SYMBOL X` (hidden / supertype): `accepts(X.rule, e)`.
/// - `ALIAS{c, named:true, value:V}`: `edge_field == "child_of"` and
///   `target_kind == V` (the alias rewrites the child kind to `V`).
/// - `FIELD{name, content}`: `edge_field == name` and `content.yield` admits
///   `target_kind` (the field content must accept the target as one of its
///   first kinds).
/// - `SEQ[m1, m2, ...]`: `accepts(m1, e)` or
///   (`m1` is ε-able and `accepts(SEQ[m2..], e)`).
/// - `CHOICE[a1, a2, ...]`: any of `accepts(ai, e)`.
/// - `OPTIONAL` / `REPEAT` / `REPEAT1` / wrappers: `accepts(inner, e)`.
pub(crate) fn accepts_first_edge(
    grammar: &Grammar,
    production: &Production,
    edge_field: &str,
    target_kind: &str,
) -> bool {
    let mut visited = std::collections::HashSet::new();
    accepts_first_edge_inner(grammar, production, edge_field, target_kind, &mut visited)
}

/// Inductive acceptance predicate. `visited` guards the hidden/supertype
/// `SYMBOL → rule` expansion against cyclic rule graphs (e.g. Zig's
/// mutually-recursive hidden declaration rules), which would otherwise
/// recurse until the stack overflows.
pub(crate) fn accepts_first_edge_inner(
    grammar: &Grammar,
    production: &Production,
    edge_field: &str,
    target_kind: &str,
    visited: &mut std::collections::HashSet<String>,
) -> bool {
    fn yield_contains(grammar: &Grammar, prod: &Production, kind: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        let ys = yield_of_production(grammar, prod, &mut visited, &mut cache);
        ys.contains(kind)
            || grammar
                .subtypes
                .get(kind)
                .is_some_and(|subs| subs.iter().any(|s| ys.contains(s.as_str())))
    }
    fn yield_has_epsilon(grammar: &Grammar, prod: &Production) -> bool {
        // A CHOICE can be taken as its empty / keyword-only branch, so it is
        // ε-able if ANY alternative is. The unioned yield set loses this: a
        // CHOICE like `[STRING "return", SYMBOL _return_at]` (the keyword
        // position of Kotlin's `return`) has a non-empty union (the label
        // kinds from `_return_at`) yet can produce no named child via the
        // bare `"return"` branch. Without descending, the SEQ walker would
        // treat that position as a mandatory consumer and refuse to look past
        // it to the following `_expression`, so `return x` would be rejected
        // (and a sibling `throw` alt wrongly preferred).
        if let Production::Choice { members } = prod {
            return members.iter().any(|m| yield_has_epsilon(grammar, m));
        }
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        let ys = yield_of_production(grammar, prod, &mut visited, &mut cache);
        // SEQ with all-ε-able members, OPTIONAL, REPEAT, BLANK all
        // carry the ε marker (empty string) in their yield set.
        ys.contains("") || ys.is_empty()
    }
    match production {
        Production::String { .. } | Production::Pattern { .. } | Production::Blank => false,
        Production::Symbol { name } => {
            if edge_field != "child_of" {
                return false;
            }
            if name == target_kind {
                return true;
            }
            if grammar
                .subtypes
                .get(target_kind)
                .is_some_and(|s| s.contains(name))
            {
                return true;
            }
            // Hidden / supertype: walk the rule body, guarding against
            // cyclic hidden-rule graphs.
            let is_expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
            if is_expand {
                if !visited.insert(name.clone()) {
                    return false;
                }
                let accepted = grammar.rules.get(name).is_some_and(|rule| {
                    accepts_first_edge_inner(grammar, rule, edge_field, target_kind, visited)
                });
                visited.remove(name);
                return accepted;
            }
            false
        }
        Production::Alias {
            named,
            value,
            content,
        } => {
            if *named && !value.is_empty() {
                edge_field == "child_of" && value == target_kind
            } else {
                accepts_first_edge_inner(grammar, content, edge_field, target_kind, visited)
            }
        }
        Production::Field { name, content } => {
            edge_field == name.as_str() && yield_contains(grammar, content, target_kind)
        }
        Production::Seq { members } => {
            for m in members {
                if accepts_first_edge_inner(grammar, m, edge_field, target_kind, visited) {
                    return true;
                }
                if !yield_has_epsilon(grammar, m) {
                    return false;
                }
            }
            false
        }
        Production::Choice { members } => members
            .iter()
            .any(|m| accepts_first_edge_inner(grammar, m, edge_field, target_kind, visited)),
        Production::Optional { content }
        | Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            accepts_first_edge_inner(grammar, content, edge_field, target_kind, visited)
        }
    }
}

/// Read the walker-recorded `pre-alias-symbol` constraint for a vertex.
/// Returns `None` when the vertex has no such constraint (either there
/// was no alias rewrite or the schema was built without the walker).
pub(crate) fn pre_alias_symbol<'a>(
    schema: &'a Schema,
    vertex_id: &panproto_gat::Name,
) -> Option<&'a str> {
    schema.constraints.get(vertex_id).and_then(|cs| {
        cs.iter()
            .find(|c| c.sort.as_ref() == "pre-alias-symbol")
            .map(|c| c.value.as_str())
    })
}

/// Walk `production` and collect every alias-source-symbol declared
/// inside a FIELD with name `field_name`. Specifically, for each
/// `FIELD { name = field_name, content = ALIAS { content = SYMBOL X,
/// named: true, value: _ } }`, append `X`. Returns an empty Vec when
/// the alt's FIELD body is not a named-ALIAS-over-SYMBOL.
pub(crate) fn field_alias_sources<'a>(
    production: &'a Production,
    field_name: &str,
    out: &mut Vec<&'a str>,
) {
    fn unwrap_to_alias_source(p: &Production) -> Option<&str> {
        let inner = match p {
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => content.as_ref(),
            _ => p,
        };
        match inner {
            Production::Alias { content, named, .. } if *named => {
                if let Production::Symbol { name } = content.as_ref() {
                    return Some(name.as_str());
                }
                None
            }
            _ => None,
        }
    }
    match production {
        Production::Field { name, content } if name.as_str() == field_name => {
            if let Some(src) = unwrap_to_alias_source(content) {
                out.push(src);
            }
        }
        Production::Field { content, .. }
        | Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            field_alias_sources(content, field_name, out);
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                field_alias_sources(m, field_name, out);
            }
        }
        _ => {}
    }
}

/// Categorical alias-source discriminator: when the cursor edge for a
/// field-named edge has a recorded `pre-alias-symbol = X`, an alt
/// whose FIELD of that name takes its content from `ALIAS { SYMBOL Y }`
/// is structurally compatible iff `Y == X` — i.e. the alias's source
/// rule matches what the parser actually walked through. When the alt
/// has a FIELD with a named-ALIAS-over-SYMBOL whose source disagrees
/// with the cursor's recorded pre-alias-symbol, the alt is rejected.
pub(crate) fn alt_satisfies_pre_alias_constraints(
    schema: &Schema,
    cursor: &ChildCursor<'_>,
    alt: &Production,
) -> bool {
    for (i, edge) in cursor.edges.iter().enumerate() {
        if cursor.consumed[i] {
            continue;
        }
        let edge_kind = edge.kind.as_ref();
        if edge_kind == "child_of" {
            continue;
        }
        let Some(actual_source) = pre_alias_symbol(schema, &edge.tgt) else {
            continue;
        };
        let mut sources: Vec<&str> = Vec::new();
        field_alias_sources(alt, edge_kind, &mut sources);
        if sources.is_empty() {
            // The alt's FIELD content is not a named-ALIAS-over-SYMBOL,
            // so this discriminator does not apply (the alt may still
            // be correct).
            continue;
        }
        if !sources.contains(&actual_source) {
            return false;
        }
    }
    true
}

/// Returns true iff `alt` is structurally compatible with the cursor under
/// the field-token-restriction discipline: for every FIELD in `alt` whose
/// content is `ALIAS{CHOICE[STRING...], value: V}`, the cursor's field-named
/// edge for that field must carry a literal-value in the restricted set.
/// When the alt has no token-restricted FIELDs the check is vacuously true.
pub(crate) fn alt_satisfies_field_token_restrictions(
    schema: &Schema,
    cursor: &ChildCursor<'_>,
    alt: &Production,
) -> bool {
    let mut restrictions: Vec<(&str, Vec<&str>)> = Vec::new();
    collect_field_token_restrictions(alt, &mut restrictions);
    for (field_name, allowed) in &restrictions {
        let mut field_seen = false;
        let mut field_admits = false;
        for (i, edge) in cursor.edges.iter().enumerate() {
            if cursor.consumed[i] {
                continue;
            }
            if edge.kind.as_ref() != *field_name {
                continue;
            }
            field_seen = true;
            let lit = literal_value(schema, &edge.tgt);
            if let Some(l) = lit {
                if allowed.contains(&l) {
                    field_admits = true;
                    break;
                }
            }
        }
        if field_seen && !field_admits {
            return false;
        }
    }
    true
}

pub(crate) fn has_relevant_constraint(
    production: &Production,
    schema: &Schema,
    vertex_id: &panproto_gat::Name,
) -> bool {
    let constraints = match schema.constraints.get(vertex_id) {
        Some(c) => c,
        None => return false,
    };
    fn walk(production: &Production, constraints: &[panproto_schema::Constraint]) -> bool {
        match production {
            Production::String { value } => constraints
                .iter()
                .any(|c| c.value == *value || c.sort.as_ref() == value),
            Production::Field { name, content } => {
                constraints.iter().any(|c| c.sort.as_ref() == name) || walk(content, constraints)
            }
            Production::Seq { members } | Production::Choice { members } => {
                members.iter().any(|m| walk(m, constraints))
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Alias { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, constraints),
            _ => false,
        }
    }
    walk(production, constraints)
}

pub(crate) fn children_for<'a>(
    schema: &'a Schema,
    vertex_id: &panproto_gat::Name,
) -> Vec<&'a Edge> {
    // Walk `outgoing` (insertion-ordered by SchemaBuilder via SmallVec
    // append) rather than the unordered `edges` HashMap so abstract
    // schemas under REPEAT(CHOICE(...)) preserve the order their edges
    // were inserted in. The previous implementation walked the HashMap
    // and sorted lexicographically by (kind, target id), which fused
    // interleaved children of the same kind into runs (e.g. a sequence
    // [symbol, punct, int, symbol, punct, int] became [symbol, symbol,
    // punct, punct, int, int] after the lex sort).
    let Some(edges) = schema.outgoing.get(vertex_id) else {
        return Vec::new();
    };

    // Look up the canonical Edge reference (the key in `schema.edges`)
    // for each entry in `outgoing`. Falls back to the SmallVec entry if
    // the canonical key is missing, which would indicate index drift.
    let mut indexed: Vec<(usize, u32, &Edge)> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let canonical = schema.edges.get_key_value(e).map_or(e, |(k, _)| k);
            let pos = schema.orderings.get(canonical).copied().unwrap_or(u32::MAX);
            (i, pos, canonical)
        })
        .collect();

    // Stable sort by (explicit-ordering, insertion-index). Edges with
    // an explicit `orderings` entry come first in their declared order;
    // the remainder fall through in insertion order.
    indexed.sort_by_key(|(i, pos, _)| (*pos, *i));
    indexed.into_iter().map(|(_, _, e)| e).collect()
}

pub(crate) fn vertex_id_kind<'a>(
    schema: &'a Schema,
    vertex_id: &panproto_gat::Name,
) -> Option<&'a str> {
    schema.vertices.get(vertex_id).map(|v| v.kind.as_ref())
}

pub(crate) fn literal_value<'a>(
    schema: &'a Schema,
    vertex_id: &panproto_gat::Name,
) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex_id)?
        .iter()
        .find(|c| c.sort.as_ref() == "literal-value")
        .map(|c| c.value.as_str())
}

/// The named integer value of a `start-byte` / `end-byte` constraint on a
/// vertex, if present.
fn byte_anchor(schema: &Schema, vertex_id: &panproto_gat::Name, sort: &str) -> Option<usize> {
    schema
        .constraints
        .get(vertex_id)?
        .iter()
        .find(|c| c.sort.as_ref() == sort)
        .and_then(|c| c.value.parse::<usize>().ok())
}

/// True iff `vertex_id` records at least one `interstitial-N` constraint.
/// This is the per-vertex replay witness: an `interstitial-N` is the exact
/// source text between two of the vertex's named children (and the leading /
/// trailing gaps), so its presence means the layout fibre carries the verbatim
/// inter-child spacing the role table only approximates.
pub(crate) fn vertex_has_interstitials(schema: &Schema, vertex_id: &panproto_gat::Name) -> bool {
    schema
        .constraints
        .get(vertex_id)
        .is_some_and(|cs| {
            cs.iter().any(|c| {
                let s = c.sort.as_ref();
                s.starts_with("interstitial-") && !s.ends_with("-start-byte")
            })
        })
}

/// Reconstruct the EXACT source bytes of the subtree rooted at `vertex_id`
/// from the recorded layout fibre, returning `None` unless the reconstruction
/// is provably complete (tiles the root's `[start-byte, end-byte)` span with no
/// gap or overlap).
///
/// This is the per-subtree form of [`emit_from_schema`](crate::languages::common):
/// every vertex in the subtree contributes its `literal-value` (anchored at its
/// `start-byte`) and every `interstitial-N` (anchored at its companion
/// `interstitial-N-start-byte`) as a `(pos, text)` fragment. Sorting the
/// fragments by source position and concatenating non-overlapping runs replays
/// the original bytes — the named-child spans and the interstitial gaps between
/// them tile the parent span exactly. The completeness check (the cursor must
/// advance from the root `start-byte` to the root `end-byte` with no hole)
/// makes this byte-faithful BY CONSTRUCTION: it is taken only when the fibre is
/// self-consistent, so it can never corrupt a replay the role table got right —
/// where the tiling is incomplete (a by-construction child with no anchor, an
/// external-scanner token outside the fibre) it declines and the caller keeps
/// the structural role-table walk.
pub(crate) fn reconstruct_subtree_bytes(
    schema: &Schema,
    vertex_id: &panproto_gat::Name,
) -> Option<String> {
    let root_start = byte_anchor(schema, vertex_id, "start-byte")?;
    let root_end = byte_anchor(schema, vertex_id, "end-byte")?;
    if root_end < root_start {
        return None;
    }

    // Collect the subtree's vertices (reachable from `vertex_id` via edges).
    let mut subtree: std::collections::HashSet<panproto_gat::Name> = std::collections::HashSet::new();
    let mut stack = vec![vertex_id.clone()];
    while let Some(v) = stack.pop() {
        if !subtree.insert(v.clone()) {
            continue;
        }
        if let Some(edges) = schema.outgoing.get(&v) {
            for e in edges {
                stack.push(e.tgt.clone());
            }
        }
    }

    // Gather every text fragment in the subtree with its source position.
    let mut fragments: Vec<(usize, &str)> = Vec::new();
    for v in &subtree {
        let Some(cons) = schema.constraints.get(v) else {
            continue;
        };
        let start = cons
            .iter()
            .find(|c| c.sort.as_ref() == "start-byte")
            .and_then(|c| c.value.parse::<usize>().ok());
        if let Some(s) = start {
            if let Some(lit) = cons.iter().find(|c| c.sort.as_ref() == "literal-value") {
                fragments.push((s, lit.value.as_str()));
            }
        }
        for c in cons {
            let sort = c.sort.as_ref();
            if sort.starts_with("interstitial-") && !sort.ends_with("-start-byte") {
                let pos_sort = format!("{sort}-start-byte");
                if let Some(pos) = cons
                    .iter()
                    .find(|c2| c2.sort.as_ref() == pos_sort.as_str())
                    .and_then(|c2| c2.value.parse::<usize>().ok())
                {
                    fragments.push((pos, c.value.as_str()));
                }
            }
        }
    }

    fragments.sort_by_key(|(pos, _)| *pos);

    // Tile [root_start, root_end): each fragment must abut the running cursor
    // (no hole) and stay within the root span. Fragments that begin before the
    // cursor are overlaps already covered by an earlier (longer) fragment and
    // are skipped, mirroring `emit_from_schema`'s dedup.
    let mut out = String::new();
    let mut cursor = root_start;
    for (pos, text) in fragments {
        if pos < root_start || pos < cursor {
            continue;
        }
        if pos > cursor {
            // A hole: some bytes in the span carry no fragment (an external
            // scanner token, a by-construction child). The reconstruction is
            // not provably faithful, so decline.
            return None;
        }
        out.push_str(text);
        cursor = pos + text.len();
    }
    if cursor == root_end {
        Some(out)
    } else {
        None
    }
}
