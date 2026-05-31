#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::option_if_let_else,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::similar_names
)]

//! Grammar-unification CHOICE dispatch: the canonical-section semantics.
//!
//! This is the **primary** way the emit review picks a `CHOICE`
//! alternative. It works from the abstract schema alone (the ordered
//! kinds of the vertex's child edges) with **no parse trace** — exactly
//! the transpilation case, where the schema was built from another
//! language's AST and never parsed in this protocol. Trace replay
//! ([`super::complement`] / the `ptrace` fibre) is layered on top as an
//! optimization that short-circuits this when a complement is present;
//! the unification here is the total semantics underneath.
//!
//! ## The matcher
//!
//! [`match_demand`] is the put-direction review of the composite optic,
//! set-valued because a `CHOICE` is a coproduct and a `REPEAT` a
//! traversal (both nondeterministic). Given a production and the
//! ordered *demand* (the kinds of the as-yet-unconsumed child edges) it
//! returns the set of demand positions reachable by matching that
//! production from `pos`. A grammar literal (`STRING`/`PATTERN`/token)
//! is zero-width against the demand — the grammar provides those bytes,
//! they consume no child edge. Only a *concrete* `SYMBOL`/`ALIAS`
//! consumes one demand slot, and only when the child kind
//! [`satisfies`](sat) it.
//!
//! ## The relation
//!
//! [`sat`] is the rigorous Child-satisfaction relation: exact kind
//! equality, or expansion through **hidden / supertype** dispatch only
//! (the supertype's `CHOICE` alternatives and pass-through wrappers),
//! never through `SEQ` members or concrete rules. That non-transitivity
//! is what stops a wrapping alternative (D's `template_parameters`) from
//! stealing a child (`int_literal`) that belongs to a later mandatory
//! member, while still admitting genuine supertype dispatch.

use super::{Grammar, Production};

/// Does an abstract child of surface kind `k` satisfy a concrete
/// grammar `SYMBOL`/`ALIAS` target named `name`?
///
/// Exact equality, or `name` is a hidden (`_`-prefixed) / supertype
/// rule that dispatches to `k` through its `CHOICE` alternatives and
/// pass-through wrappers. The expansion is cycle-guarded and never
/// descends into `SEQ` members or non-dispatch concrete rules.
#[must_use]
pub(crate) fn sat(grammar: &Grammar, k: &str, name: &str) -> bool {
    if k == name {
        return true;
    }
    let mut visited = std::collections::HashSet::new();
    dispatches_to(grammar, name, k, &mut visited)
}

/// True iff dispatch symbol `name` can yield a node of surface kind `k`
/// by expanding only hidden / supertype rules.
fn dispatches_to<'g>(
    grammar: &'g Grammar,
    name: &'g str,
    k: &str,
    visited: &mut std::collections::HashSet<&'g str>,
) -> bool {
    let is_dispatch = name.starts_with('_') || grammar.supertypes.contains(name);
    if !is_dispatch || !visited.insert(name) {
        return false;
    }
    let Some(rule) = grammar.rules.get(name) else {
        return false;
    };
    dispatch_prod(grammar, rule, k, visited)
}

/// Walk a dispatch rule's body, following only the structure that
/// represents alternatives / pass-through (CHOICE members, wrappers),
/// not SEQ positions.
fn dispatch_prod<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    k: &str,
    visited: &mut std::collections::HashSet<&'g str>,
) -> bool {
    match prod {
        Production::Symbol { name } => name == k || dispatches_to(grammar, name, k, visited),
        Production::Alias { value, named, .. } => *named && value == k,
        Production::Choice { members } => members
            .iter()
            .any(|m| dispatch_prod(grammar, m, k, visited)),
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. }
        | Production::Field { content, .. } => dispatch_prod(grammar, content, k, visited),
        // A supertype/hidden rule whose body is a SEQ produces a single
        // structural node only when that SEQ has exactly one named
        // member that carries the identity; in practice dispatch rules
        // are CHOICEs. We deliberately do NOT walk SEQ members here:
        // that is the over-reach that lets a wrapper steal a child.
        _ => false,
    }
}

/// Set-valued review: the demand positions reachable by matching
/// `prod` against `demand` from `pos`. Empty result ⇒ no match.
///
/// `demand` is the ordered list of unconsumed child-edge kinds. Grammar
/// literals are zero-width; concrete symbols/aliases consume one slot
/// iff the child kind satisfies them; hidden/supertype symbols expand.
#[must_use]
pub(crate) fn match_demand<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    demand: &[&str],
    pos: usize,
    visited: &mut Vec<(&'g str, usize)>,
) -> Vec<usize> {
    match prod {
        Production::Blank => vec![pos],
        Production::String { .. } | Production::Pattern { .. } => vec![pos],
        Production::Symbol { name } => {
            let is_dispatch = name.starts_with('_') || grammar.supertypes.contains(name);
            if is_dispatch {
                // Expand inline (cycle-guarded on (name, pos)).
                if visited.contains(&(name.as_str(), pos)) {
                    return vec![];
                }
                if let Some(rule) = grammar.rules.get(name) {
                    visited.push((name.as_str(), pos));
                    let out = match_demand(grammar, rule, demand, pos, visited);
                    visited.pop();
                    return out;
                }
                // Hidden/supertype with no rule: treat as zero-width.
                return vec![pos];
            }
            if !grammar.rules.contains_key(name) {
                // External scanner token: zero-width.
                return vec![pos];
            }
            // Concrete symbol: consume one child iff it satisfies.
            match demand.get(pos) {
                Some(k) if sat(grammar, k, name) => vec![pos + 1],
                _ => vec![],
            }
        }
        Production::Alias { named, value, .. } => {
            if *named && !value.is_empty() {
                match demand.get(pos) {
                    Some(k) if sat(grammar, k, value) => vec![pos + 1],
                    _ => vec![],
                }
            } else {
                // Anonymous alias renames a token: zero-width.
                vec![pos]
            }
        }
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. }
        | Production::Field { content, .. } => {
            match_demand(grammar, content, demand, pos, visited)
        }
        Production::Seq { members } => {
            let mut frontier = vec![pos];
            for m in members {
                let mut next: Vec<usize> = Vec::new();
                for &p in &frontier {
                    for end in match_demand(grammar, m, demand, p, visited) {
                        if !next.contains(&end) {
                            next.push(end);
                        }
                    }
                }
                if next.is_empty() {
                    return vec![];
                }
                frontier = next;
            }
            frontier
        }
        Production::Choice { members } => {
            let mut out: Vec<usize> = Vec::new();
            for m in members {
                for end in match_demand(grammar, m, demand, pos, visited) {
                    if !out.contains(&end) {
                        out.push(end);
                    }
                }
            }
            out
        }
        Production::Optional { content } => {
            let mut out = vec![pos];
            for end in match_demand(grammar, content, demand, pos, visited) {
                if !out.contains(&end) {
                    out.push(end);
                }
            }
            out
        }
        Production::Repeat { content } => closure(grammar, content, demand, pos, visited, true),
        Production::Repeat1 { content } => closure(grammar, content, demand, pos, visited, false),
    }
}

/// Reflexive-transitive (REPEAT) or transitive-from-one (REPEAT1)
/// closure of one iteration of `content`.
fn closure<'g>(
    grammar: &'g Grammar,
    content: &'g Production,
    demand: &[&str],
    pos: usize,
    visited: &mut Vec<(&'g str, usize)>,
    reflexive: bool,
) -> Vec<usize> {
    let mut seen = if reflexive { vec![pos] } else { vec![] };
    let mut frontier = vec![pos];
    while let Some(p) = frontier.pop() {
        for end in match_demand(grammar, content, demand, p, visited) {
            // A zero-progress iteration would loop forever; require advance.
            if end > p && !seen.contains(&end) {
                seen.push(end);
                frontier.push(end);
            }
        }
    }
    seen
}

/// Pick the `CHOICE` alternative whose yield uniquely-maximally consumes
/// the demand prefix. Returns the alternative index, or `None` when the
/// demand under-determines the variant (a tie) — the review then defers
/// to the canonical default rather than guessing.
#[must_use]
pub(crate) fn select_choice_by_unification(
    grammar: &Grammar,
    alternatives: &[Production],
    demand: &[&str],
) -> Option<usize> {
    let mut best_len = 0usize;
    let mut best_idx: Option<usize> = None;
    let mut best_count = 0usize;
    for (i, alt) in alternatives.iter().enumerate() {
        let mut visited = Vec::new();
        let ends = match_demand(grammar, alt, demand, 0, &mut visited);
        let Some(max_end) = ends.into_iter().max() else {
            continue;
        };
        if max_end > best_len {
            best_len = max_end;
            best_idx = Some(i);
            best_count = 1;
        } else if max_end == best_len && max_end > 0 {
            best_count += 1;
        }
    }
    // A zero-consumption "best" means no alternative consumed any child;
    // that is not a positive selection (e.g. all alternatives are pure
    // tokens). Defer. A tie (≥2 alternatives reach the same maximal
    // length) is genuine under-determination. Defer.
    if best_len == 0 || best_count != 1 {
        return None;
    }
    best_idx
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::emit_pretty::Grammar;

    fn grammar(json: &str) -> Grammar {
        Grammar::from_bytes("test", json.as_bytes()).expect("parse grammar")
    }

    fn sym(name: &str) -> serde_json::Value {
        serde_json::json!({"type": "SYMBOL", "name": name})
    }
    fn str_(v: &str) -> serde_json::Value {
        serde_json::json!({"type": "STRING", "value": v})
    }

    /// CHOICE between two concrete alternatives keyed by their first
    /// child kind: unification picks by structural match, no trace.
    #[test]
    fn picks_alternative_by_child_kind() {
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "expr": {"type": "CHOICE", "members": [
                        {"type": "SEQ", "members": [sym("number"), str_("+"), sym("number")]},
                        {"type": "SEQ", "members": [sym("string"), str_("~"), sym("string")]},
                    ]},
                    "number": str_("0"),
                    "string": str_("s"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["expr"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // demand = two number children → first alt
        assert_eq!(
            select_choice_by_unification(&g, alts, &["number", "number"]),
            Some(0)
        );
        // demand = two string children → second alt
        assert_eq!(
            select_choice_by_unification(&g, alts, &["string", "string"]),
            Some(1)
        );
    }

    /// The D-bug shape: a wrapping alternative must NOT steal a child it
    /// can only reach through a concrete intervening node.
    #[test]
    fn wrapper_does_not_steal_via_deep_reachability() {
        // declarator = CHOICE[ template_parameters , identifier ]
        // template_parameters = SEQ["<", int_literal, ">"]  (concrete)
        // A bare int_literal demand must NOT pick template_parameters
        // (which only reaches int_literal through its own concrete node).
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "declarator": {"type": "CHOICE", "members": [
                        sym("template_parameters"),
                        sym("identifier"),
                    ]},
                    "template_parameters": {"type": "SEQ", "members": [
                        str_("<"), sym("int_literal"), str_(">")]},
                    "identifier": str_("x"),
                    "int_literal": str_("0"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["declarator"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // An identifier child picks `identifier`, not template_parameters.
        assert_eq!(
            select_choice_by_unification(&g, alts, &["identifier"]),
            Some(1)
        );
        // An int_literal child matches NEITHER concrete alternative
        // directly (template_parameters needs the whole < int > shape,
        // identifier needs an identifier) → defer, do not steal.
        assert_eq!(
            select_choice_by_unification(&g, alts, &["int_literal"]),
            None
        );
    }

    /// Supertype dispatch: a child whose kind is a supertype member
    /// satisfies a SYMBOL reference to the supertype.
    #[test]
    fn supertype_member_satisfies_supertype_symbol() {
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "supertypes": ["_literal"],
                "rules": {
                    "_literal": {"type": "CHOICE", "members": [sym("int"), sym("float")]},
                    "int": str_("0"),
                    "float": str_("0.0"),
                }
            })
            .to_string(),
        );
        assert!(sat(&g, "int", "_literal"));
        assert!(sat(&g, "float", "_literal"));
        assert!(!sat(&g, "string", "_literal"));
        // exact still holds
        assert!(sat(&g, "int", "int"));
    }

    /// Many-to-one aliasing is under-determined → defer (the ruby case).
    #[test]
    fn ambiguous_alias_defers() {
        // Two alternatives both surface as kind `binary` (one via _pow,
        // one via command_binary). A `binary` demand ties → None.
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "site": {"type": "CHOICE", "members": [
                        {"type": "ALIAS", "named": true, "value": "binary",
                         "content": sym("_pow")},
                        {"type": "ALIAS", "named": true, "value": "binary",
                         "content": sym("command_binary")},
                    ]},
                    "_pow": str_("**"),
                    "command_binary": str_("+"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["site"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        assert_eq!(select_choice_by_unification(&g, alts, &["binary"]), None);
    }

    /// REPEAT before a mandatory member must not swallow it (set-valued
    /// matcher keeps both the swallowed and non-swallowed frontiers).
    #[test]
    fn repeat_does_not_force_swallow_of_mandatory() {
        // statements = SEQ[ REPEAT(stmt), stmt ]  — demand of 2 stmts
        // must be consumable (REPEAT takes 1, mandatory takes 1).
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "statements": {"type": "SEQ", "members": [
                        {"type": "REPEAT", "content": sym("stmt")},
                        sym("stmt"),
                    ]},
                    "stmt": str_(";"),
                }
            })
            .to_string(),
        );
        let mut v = Vec::new();
        let ends = match_demand(&g, &g.rules["statements"], &["stmt", "stmt"], 0, &mut v);
        assert!(ends.contains(&2), "must fully consume 2 stmts: {ends:?}");
    }
}
