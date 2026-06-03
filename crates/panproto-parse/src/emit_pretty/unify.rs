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
//! they consume no child edge. A *concrete* `SYMBOL`/`ALIAS`, or a
//! *visible* external scanner token (a non-`_`-prefixed external that
//! materializes as a named node, e.g. ruby `heredoc_content`), consumes
//! one demand slot, and only when the child kind [`satisfies`](sat) it.
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

use super::{Grammar, Production, collect_field_names};

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

/// The name of a bare `SYMBOL` alternative (unwrapping precedence/token
/// wrappers), or `None` if the alternative is an `ALIAS`, `SEQ`,
/// `CHOICE`, literal, etc. A bare symbol dispatches by the child's own
/// kind, so a candidate whose bare-symbol name equals the child's
/// surface kind is the direct, unaliased interpretation.
fn bare_symbol_name(prod: &Production) -> Option<&str> {
    match prod {
        Production::Symbol { name } => Some(name.as_str()),
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Reserved { content, .. } => bare_symbol_name(content),
        _ => None,
    }
}

/// The field label recorded on demand slot `pos`. Three distinct values:
///
/// * `""` — the **label-blind** call: the caller passed an empty `labels`
///   slice (the `num_viable` pass). [`label_ok`] is fully permissive here,
///   which is the keystone invariant that `num_viable` ignores labels.
/// * `"child_of"` — a genuine **unlabeled** child edge (not field-bound).
/// * any other — the edge's field name.
///
/// Preserving `"child_of"` distinct from `""` is what lets [`label_ok`]
/// reject an unlabeled edge from a named `FIELD` slot in the WINNER pass
/// while leaving the label-blind `num_viable` pass untouched.
fn slot_label<'a>(labels: &[&'a str], pos: usize) -> &'a str {
    labels.get(pos).copied().unwrap_or("")
}

/// Whether a production in field context `field_ctx` may consume a demand
/// slot labeled `slot`. This mirrors emit's `ChildCursor::take_field`
/// exactly, so the WINNER demand-match and the actual emit agree on which
/// edge a `FIELD` binds:
///
/// * The label-blind slot (`""`, an empty `labels` slice) is fully
///   permissive — this is the `num_viable` pass, which the keystone keeps
///   label-blind so the "only one viable" preemption (and the heuristics
///   it guards) is never perturbed.
/// * A **named** field context (`field_ctx == Some(f)`) consumes a slot
///   ONLY when it is labeled `f`. It must NOT consume an unlabeled
///   (`child_of`) slot, because emit's `take_field(f)` requires
///   `edge.kind == f` and would bind nothing — so counting an unlabeled
///   edge toward a named `FIELD`'s match makes the CHOICE winner disagree
///   with emit (csharp `argument`'s optional `FIELD(name, identifier) ":"`
///   munching a bare `m(x)` argument as its `name`, then emitting a
///   spurious `: x`).
/// * A non-field context (`field_ctx == None`) consumes only the unlabeled
///   (`child_of`) slot; a field-labeled edge is bound by its own `FIELD`
///   via `take_field`, never positionally (this stops rust `impl Foo`'s
///   `F:trait` eating the `type` edge → spurious `for`, and the JS
///   for-header `= expr` swallowing the `right` operand).
fn label_ok(slot: &str, field_ctx: Option<&str>) -> bool {
    if slot.is_empty() {
        // Label-blind call (num_viable): permissive.
        return true;
    }
    match field_ctx {
        Some(f) => slot == f,
        None => slot == "child_of",
    }
}

/// Set-valued review: the demand positions reachable by matching `prod`
/// against `demand` from `pos`. Empty result ⇒ no match.
///
/// `demand` is the ordered list of unconsumed child-edge kinds and
/// `labels` their parallel field-name labels (see [`label_ok`]). Grammar
/// literals are zero-width; concrete symbols/aliases consume one slot iff
/// the child kind satisfies them AND the slot's label is compatible with
/// `field_ctx`; hidden/supertype symbols expand. An empty `labels` slice
/// is fully permissive (the label-blind matcher).
#[must_use]
pub(crate) fn match_demand<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    demand: &[&str],
    labels: &[&str],
    pos: usize,
    field_ctx: Option<&str>,
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
                    let out = match_demand(grammar, rule, demand, labels, pos, field_ctx, visited);
                    visited.pop();
                    return out;
                }
                // Hidden/supertype with no rule: treat as zero-width.
                return vec![pos];
            }
            if !grammar.rules.contains_key(name) {
                // A non-dispatch symbol with no rule is an external scanner
                // token declared in the grammar's `externals` block. Two
                // kinds: a *visible* external (no leading `_`) materializes
                // as a named node in the tree (ruby `heredoc_beginning` /
                // `heredoc_content` / `heredoc_end`, `string_content`), so it
                // consumes a demand child of its own kind exactly like a
                // concrete symbol; a *hidden* external (`_line_break`,
                // `_indent`) is `_`-prefixed and never reaches here (it was
                // routed through the dispatch branch above). So if this
                // external's name satisfies the next demand child, consume it;
                // otherwise it contributes no node and stays zero-width.
                if let Some(k) = demand.get(pos) {
                    if sat(grammar, k, name) && label_ok(slot_label(labels, pos), field_ctx) {
                        return vec![pos + 1];
                    }
                }
                return vec![pos];
            }
            // Concrete symbol: consume one child iff it satisfies AND the
            // slot's field label is compatible with this position.
            match demand.get(pos) {
                Some(k)
                    if sat(grammar, k, name) && label_ok(slot_label(labels, pos), field_ctx) =>
                {
                    vec![pos + 1]
                }
                _ => vec![],
            }
        }
        Production::Alias { named, value, .. } => {
            if *named && !value.is_empty() {
                match demand.get(pos) {
                    Some(k)
                        if sat(grammar, k, value)
                            && label_ok(slot_label(labels, pos), field_ctx) =>
                    {
                        vec![pos + 1]
                    }
                    _ => vec![],
                }
            } else {
                // Anonymous alias renames a token: zero-width.
                vec![pos]
            }
        }
        Production::Field { name, content } => match_demand(
            grammar,
            content,
            demand,
            labels,
            pos,
            Some(name.as_str()),
            visited,
        ),
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            match_demand(grammar, content, demand, labels, pos, field_ctx, visited)
        }
        Production::Seq { members } => {
            let mut frontier = vec![pos];
            for m in members {
                let mut next: Vec<usize> = Vec::new();
                for &p in &frontier {
                    for end in match_demand(grammar, m, demand, labels, p, field_ctx, visited) {
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
                for end in match_demand(grammar, m, demand, labels, pos, field_ctx, visited) {
                    if !out.contains(&end) {
                        out.push(end);
                    }
                }
            }
            out
        }
        Production::Optional { content } => {
            let mut out = vec![pos];
            for end in match_demand(grammar, content, demand, labels, pos, field_ctx, visited) {
                if !out.contains(&end) {
                    out.push(end);
                }
            }
            out
        }
        Production::Repeat { content } => closure(
            grammar, content, demand, labels, pos, field_ctx, visited, true,
        ),
        Production::Repeat1 { content } => closure(
            grammar, content, demand, labels, pos, field_ctx, visited, false,
        ),
    }
}

/// Reflexive-transitive (REPEAT) or transitive-from-one (REPEAT1)
/// closure of one iteration of `content`.
#[allow(clippy::too_many_arguments)]
fn closure<'g>(
    grammar: &'g Grammar,
    content: &'g Production,
    demand: &[&str],
    labels: &[&str],
    pos: usize,
    field_ctx: Option<&str>,
    visited: &mut Vec<(&'g str, usize)>,
    reflexive: bool,
) -> Vec<usize> {
    let mut seen = if reflexive { vec![pos] } else { vec![] };
    let mut frontier = vec![pos];
    while let Some(p) = frontier.pop() {
        for end in match_demand(grammar, content, demand, labels, p, field_ctx, visited) {
            // A zero-progress iteration would loop forever; require advance.
            if end > p && !seen.contains(&end) {
                seen.push(end);
                frontier.push(end);
            }
        }
    }
    seen
}

/// The structural candidates of a `CHOICE`.
struct Candidates {
    /// Maximal demand-consumption length achieved by any viable alternative.
    best_len: usize,
    /// Indices of the viable alternatives achieving `best_len` (includes
    /// zero-consuming alternatives such as `BLANK` when `best_len == 0`).
    cands: Vec<usize>,
    /// How many alternatives are *viable* (have a non-empty match). An
    /// alternative with an empty match is structurally rejected: it
    /// requires a child the demand does not supply.
    num_viable: usize,
    /// Indices of all *viable* alternatives (label-aware, field-consistent),
    /// regardless of munch length. A direct bare-symbol match for the head
    /// child may live here without being maximal (a recursive wrapper can
    /// out-munch it), so the head-child preference is computed over this set.
    viable: Vec<usize>,
}

/// How an alternative binds a particular `FIELD` name, aggregated over its
/// whole structure (so a field appearing in several `CHOICE` branches is
/// summarised once).
struct FieldBinding<'p> {
    /// Union of literal values the field can take (STRING / anonymous-ALIAS
    /// values through CHOICE and precedence/token wrappers).
    literals: Vec<&'p str>,
    /// Some binding of the field is a SYMBOL / PATTERN (a child edge), so the
    /// parser may have recorded an edge rather than a `field:<name>`.
    binds_symbol: bool,
    /// The field is bound on EVERY path through the alternative (not
    /// skippable via OPTIONAL / REPEAT, and bound in every CHOICE branch).
    always: bool,
}

/// Analyse how `prod` binds the field named `target`. Literals are unioned
/// across CHOICE branches (the alternative can take *any* of them), and
/// `always` is the genuine mandatory analysis (every SEQ does all members;
/// a CHOICE binds it only if all branches do; OPTIONAL/REPEAT never force
/// it). This union semantics is essential for grammars that group operators
/// in a nested CHOICE (cpp `binary_expression` = `CH[F:op '&&', F:op '|',
/// …]`): the member is consistent with a recorded `field:operator="&&"` via
/// its `&&` branch and must NOT be rejected for also offering `|`.
fn analyse_field<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    target: &str,
    visited: &mut Vec<&'g str>,
) -> FieldBinding<'g> {
    fn direct_literals<'g>(prod: &'g Production, out: &mut Vec<&'g str>) {
        match prod {
            Production::String { value } => out.push(value),
            Production::Alias {
                named: false,
                value,
                ..
            } if !value.is_empty() => out.push(value),
            Production::Choice { members } => {
                for m in members {
                    direct_literals(m, out);
                }
            }
            Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => direct_literals(content, out),
            _ => {}
        }
    }
    fn binds_symbol(prod: &Production) -> bool {
        match prod {
            Production::Symbol { .. } | Production::Pattern { .. } => true,
            Production::Choice { members } | Production::Seq { members } => {
                members.iter().any(binds_symbol)
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
            | Production::Reserved { content, .. } => binds_symbol(content),
            _ => false,
        }
    }
    let empty = || FieldBinding {
        literals: Vec::new(),
        binds_symbol: false,
        always: false,
    };
    match prod {
        Production::Field { name, content } => {
            if name == target {
                let mut literals = Vec::new();
                direct_literals(content, &mut literals);
                FieldBinding {
                    literals,
                    binds_symbol: binds_symbol(content),
                    always: true,
                }
            } else {
                analyse_field(grammar, content, target, visited)
            }
        }
        // Expand hidden / supertype rules (cycle-guarded): a field of the
        // same name can be re-bound deeper, with other literals. bash
        // `_expansion_body` member binds `field('operator','!')` shallowly
        // but the recorded `field:operator="-"` lives in `_expansion_expression`
        // reached through this symbol; without expansion the shallow `{!}`
        // would spuriously contradict `-`.
        Production::Symbol { name } => {
            let is_dispatch = name.starts_with('_') || grammar.supertypes.contains(name);
            if is_dispatch && !visited.contains(&name.as_str()) {
                if let Some(rule) = grammar.rules.get(name) {
                    visited.push(name.as_str());
                    let fb = analyse_field(grammar, rule, target, visited);
                    visited.pop();
                    return fb;
                }
            }
            empty()
        }
        Production::Seq { members } => {
            let mut acc = empty();
            for m in members {
                let fb = analyse_field(grammar, m, target, visited);
                acc.literals.extend(fb.literals);
                acc.binds_symbol |= fb.binds_symbol;
                // A SEQ binds the field if any member always binds it.
                acc.always |= fb.always;
            }
            acc
        }
        Production::Choice { members } => {
            let mut acc = FieldBinding {
                literals: Vec::new(),
                binds_symbol: false,
                always: !members.is_empty(),
            };
            for m in members {
                let fb = analyse_field(grammar, m, target, visited);
                acc.literals.extend(fb.literals);
                acc.binds_symbol |= fb.binds_symbol;
                // A CHOICE binds the field only if every branch does.
                acc.always &= fb.always;
            }
            acc
        }
        Production::Optional { content } | Production::Repeat { content } => {
            let fb = analyse_field(grammar, content, target, visited);
            FieldBinding {
                literals: fb.literals,
                binds_symbol: fb.binds_symbol,
                always: false,
            }
        }
        Production::Repeat1 { content }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Alias { content, .. }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => analyse_field(grammar, content, target, visited),
        _ => empty(),
    }
}

/// Whether `alt` is consistent with the recorded `field:<name>=<value>`
/// constraints. For each field `name` bound in `alt` (gathered via
/// [`collect_field_names`]), with its binding analysed by [`analyse_field`]:
///
/// 1. **Contradiction.** If `name` is bound only to literals (never a
///    symbol) and that literal union is non-empty, the recorded `value`
///    must be among them. JS `_for_header`'s `field('kind','var')` against a
///    recorded `field:kind="const"` is rejected, so it cannot out-consume
///    the `let|const` alternative on maximal-munch length.
/// 2. **Absent mandatory field.** If `name` is bound on every path to a
///    literal (and never a symbol) but no `field:<name>` was recorded, the
///    parser did not take `alt`. JS `for (x in y)` has no `kind`, so the
///    var/let/const members are rejected and the bare `field('left', …)`
///    member wins instead of emitting a spurious `var`.
///
/// A field that can bind a SYMBOL is skipped by both checks (the parser may
/// have recorded a child edge, not a `field:<name>`). Both checks are safe
/// by construction: the alternative the parser actually took produced the
/// recorded value (so it is in the union) and recorded every mandatory
/// literal field, so a correct alternative is never rejected.
fn field_value_consistent(
    grammar: &Grammar,
    alt: &Production,
    field_constraints: &[(&str, &str)],
) -> bool {
    let mut names = std::collections::HashSet::new();
    collect_field_names(alt, &mut names);
    for name in names {
        let mut visited = Vec::new();
        let fb = analyse_field(grammar, alt, name, &mut visited);
        if fb.binds_symbol || fb.literals.is_empty() {
            continue;
        }
        match field_constraints.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => {
                if !fb.literals.iter().any(|l| l == value) {
                    return false;
                }
            }
            None => {
                if fb.always {
                    return false;
                }
            }
        }
    }
    true
}

fn choice_candidates(
    grammar: &Grammar,
    alternatives: &[Production],
    demand: &[&str],
    labels: &[&str],
    initial_field_ctx: Option<&str>,
    field_constraints: &[(&str, &str)],
) -> Candidates {
    let mut best_len = 0usize;
    let mut cands: Vec<usize> = Vec::new();
    let mut num_viable = 0usize;
    let mut viable: Vec<usize> = Vec::new();
    for (i, alt) in alternatives.iter().enumerate() {
        // `num_viable` counts STRUCTURAL viability (can this alt consume the
        // demand at all?) over ALL alternatives, label-BLIND. It drives the
        // "only one viable" preemption, which neither field labels nor field
        // values may perturb — changing it preempts the heuristics that
        // legitimately resolve cpp/c ties (the lesson from the reverted
        // global field-aware-demand attempt).
        let mut visited = Vec::new();
        if match_demand(grammar, alt, demand, &[], 0, None, &mut visited)
            .into_iter()
            .max()
            .is_none()
        {
            continue;
        }
        num_viable += 1;
        // The WINNER (best_len / cands) is computed label-AWARE and
        // field-value-consistent: an alt that would only reach a longer
        // match by consuming a field-labeled edge through a non-matching
        // field (rust `impl Foo`'s `F:trait` taking the `type` edge; JS
        // for-header's optional `= expr` taking the `right` operand), or
        // whose literal field contradicts the recorded value, must NOT win
        // on maximal munch — but it stays counted as structurally viable.
        if !field_value_consistent(grammar, alt, field_constraints) {
            continue;
        }
        // The labeled match starts in the CHOICE's enclosing field context
        // (e.g. go `function_declaration`'s `F:body(CH[block | BLANK])` —
        // the `block` is `body`-labeled, so the dispatch must begin with
        // field_ctx = "body" or `label_ok` would reject the only real
        // alternative and drop the function body).
        let mut lv = Vec::new();
        let Some(max_end) =
            match_demand(grammar, alt, demand, labels, 0, initial_field_ctx, &mut lv)
                .into_iter()
                .max()
        else {
            continue;
        };
        viable.push(i);
        if max_end > best_len {
            best_len = max_end;
            cands = vec![i];
        } else if max_end == best_len {
            cands.push(i);
        }
    }
    Candidates {
        best_len,
        cands,
        num_viable,
        viable,
    }
}

/// Does `prod` reach a `Repeat`/`Repeat1` whose body references the hidden
/// dispatch rule `self_rule` (directly or through transparent wrappers /
/// nested SEQ/CHOICE)? This is the "recursive re-entry" signal: an
/// alternative that out-munches by looping back into the same CHOICE rule
/// it belongs to is absorbing sibling iterations, not a genuinely longer
/// single production. Cycle-guarded on rule names.
fn reenters_repeat<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    self_rule: &str,
    in_repeat: bool,
    visited: &mut Vec<&'g str>,
) -> bool {
    match prod {
        Production::Symbol { name } => {
            if in_repeat && name == self_rule {
                return true;
            }
            // Follow hidden pass-through rules so a REPEAT of a hidden alias
            // of `self_rule` still counts.
            if name.starts_with('_') && !visited.contains(&name.as_str()) {
                if let Some(rule) = grammar.rules.get(name) {
                    visited.push(name.as_str());
                    let r = reenters_repeat(grammar, rule, self_rule, in_repeat, visited);
                    visited.pop();
                    return r;
                }
            }
            false
        }
        Production::Repeat { content } | Production::Repeat1 { content } => {
            reenters_repeat(grammar, content, self_rule, true, visited)
        }
        Production::Seq { members } | Production::Choice { members } => members
            .iter()
            .any(|m| reenters_repeat(grammar, m, self_rule, in_repeat, visited)),
        Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Alias { content, .. }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            reenters_repeat(grammar, content, self_rule, in_repeat, visited)
        }
        _ => false,
    }
}

/// The literal `STRING` tokens an alternative would emit directly (its
/// own grammar literals, recursively). Used as the variant tag for
/// trace tie-breaking.
fn alt_literals(prod: &Production) -> Vec<String> {
    let mut out = Vec::new();
    collect_literals(prod, &mut out);
    out
}

/// The literal tokens an alternative would emit, **resolving bare `SYMBOL`
/// references one hop through the grammar** to reach the referenced rule's
/// leading delimiter literal. A `CHOICE` whose alternatives are bare
/// symbols discriminated by their opening delimiter (TOML's `string =
/// CHOICE[_basic_string, _multiline_basic_string, …]`, keyed by `"` vs
/// `"""` vs `'` vs `'''`) has no inline literals on the alternative itself —
/// the discriminating literal lives at the head of the referenced rule's
/// `SEQ`. The recorded variant tag (`ptrace` `T<open-delimiter>`) names
/// that opener, so collecting the resolved rule's leading literal lets the
/// tie-break pick the alternative whose delimiter the parser actually saw.
/// Cycle-guarded; only the *leading* literal(s) of a `SEQ` are taken (a
/// trailing literal deep in the rule is not a discriminator and could
/// false-match the trace).
fn alt_literals_resolved(grammar: &Grammar, prod: &Production) -> Vec<String> {
    let inline = alt_literals(prod);
    if !inline.is_empty() {
        return inline;
    }
    let Some(name) = bare_symbol_name(prod) else {
        return inline;
    };
    let mut out = Vec::new();
    let mut visiting = Vec::new();
    collect_leading_literals(grammar, name, &mut out, &mut visiting);
    out
}

/// Collect the *leading* literal token(s) reachable from the named rule:
/// the head of a `SEQ`, every branch of a leading `CHOICE`, or the body of
/// a leading `REPEAT`/`OPTIONAL`/precedence/token wrapper. Stops at the
/// first non-literal-yielding member of a `SEQ` (a delimiter precedes the
/// variable content). Resolves one further bare-`SYMBOL` hop (cycle-guarded
/// on the visiting stack).
fn collect_leading_literals<'g>(
    grammar: &'g Grammar,
    name: &'g str,
    out: &mut Vec<String>,
    visiting: &mut Vec<&'g str>,
) {
    if visiting.contains(&name) {
        return;
    }
    let Some(rule) = grammar.rules.get(name) else {
        return;
    };
    visiting.push(name);
    leading_literals_of(grammar, rule, out, visiting);
    visiting.pop();
}

fn leading_literals_of<'g>(
    grammar: &'g Grammar,
    prod: &'g Production,
    out: &mut Vec<String>,
    visiting: &mut Vec<&'g str>,
) {
    match prod {
        Production::String { value } => out.push(value.clone()),
        Production::Seq { members } => {
            if let Some(first) = members.first() {
                leading_literals_of(grammar, first, out, visiting);
            }
        }
        Production::Choice { members } => {
            for m in members {
                leading_literals_of(grammar, m, out, visiting);
            }
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. }
        | Production::Alias { content, .. } => {
            leading_literals_of(grammar, content, out, visiting);
        }
        Production::Symbol { name } => {
            collect_leading_literals(grammar, name, out, visiting);
        }
        _ => {}
    }
}

fn collect_literals(prod: &Production, out: &mut Vec<String>) {
    match prod {
        Production::String { value } => out.push(value.clone()),
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                collect_literals(m, out);
            }
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => collect_literals(content, out),
        _ => {}
    }
}

/// Pick a `CHOICE` alternative using structural unification, with the
/// trace's literal-token fibre as the tie-breaker (the variant tag).
///
/// Unification first computes the structurally-maximal candidates. If
/// exactly one, that is the answer. If several tie (e.g. kotlin
/// `return expr` vs `throw expr`: same child demand `[expr]`), the
/// parser's recorded literal tokens disambiguate: the alternative whose
/// own literals overlap the trace's `Token` slots most (uniquely) is the
/// one the parser took. This *consumes* the Prism tag rather than
/// re-deriving it, and needs no absolute trace position. Still defers
/// (`None`) on genuine under-determination.
#[must_use]
pub(crate) fn select_choice_with_trace(
    grammar: &Grammar,
    alternatives: &[Production],
    demand: &[&str],
    labels: &[&str],
    initial_field_ctx: Option<&str>,
    field_constraints: &[(&str, &str)],
    trace_tokens: &[String],
    self_rule: Option<&str>,
) -> Option<usize> {
    let Candidates {
        best_len,
        cands,
        num_viable,
        viable,
    } = choice_candidates(
        grammar,
        alternatives,
        demand,
        labels,
        initial_field_ctx,
        field_constraints,
    );
    if cands.is_empty() {
        // No alternative can consume the demand at all.
        return None;
    }
    // Head-child direct match preempts a recursive-wrapper maximal munch.
    // When this CHOICE is the body of the hidden dispatch rule `self_rule`
    // (so it is being iterated ONE child at a time by a surrounding REPEAT),
    // an alternative that out-munches the direct single-child match does so
    // only by RE-ENTERING `self_rule` recursively and swallowing later
    // sibling iterations (cmake `_untrimmed_argument = CHOICE[…, argument,
    // _paren_argument]` where `_paren_argument = SEQ['(', REPEAT(
    // _untrimmed_argument), ')']` absorbs every following `argument`, beating
    // the direct single-child `argument` alt and emitting `()`). The direct
    // one-to-one alternative is correct per REPEAT-iteration semantics.
    // Guarded by `self_rule` so this NEVER fires for a CHOICE nested in a
    // named SEQ (go grouped `const_declaration`, whose longer grouped
    // alternative legitimately consumes every `const_spec`).
    if let (Some(self_rule), Some(&child_kind)) = (self_rule, demand.first()) {
        let direct_viable: Vec<usize> = viable
            .iter()
            .copied()
            .filter(|&i| bare_symbol_name(&alternatives[i]) == Some(child_kind))
            .collect();
        if direct_viable.len() == 1 {
            let d = direct_viable[0];
            let winner_is_direct = cands
                .iter()
                .any(|&i| bare_symbol_name(&alternatives[i]) == Some(child_kind));
            // Only preempt when the winner out-munched by recursive re-entry
            // (not a genuinely-longer non-recursive alternative).
            let winner_reenters = cands.iter().any(|&i| {
                let mut v = Vec::new();
                reenters_repeat(grammar, &alternatives[i], self_rule, false, &mut v)
            });
            if !winner_is_direct && !cands.contains(&d) && winner_reenters {
                return Some(d);
            }
        }
    }
    // A unique structural answer exists when one alternative is maximal
    // AND there is real discrimination: either it consumed children
    // (best_len > 0, uniquely-maximal munch) or it is the *only* viable
    // alternative (every other was structurally rejected for needing an
    // absent child — e.g. `BLANK` is chosen over `_initializer` for a
    // declarator with no value, since `= expr` requires a child the
    // demand does not supply). A zero-consumption tie among *all* viable
    // alternatives (num_viable == num_total, best_len == 0) carries no
    // structural signal — those fall through to the literal tie-break.
    if cands.len() == 1 && (best_len > 0 || num_viable == 1) {
        return Some(cands[0]);
    }
    // Tie among viable candidates. A bare `SYMBOL` whose name is exactly
    // the child's surface kind is the most direct interpretation: the
    // parser's child kind names that production directly, with no alias
    // rename or supertype indirection. When exactly one tied candidate
    // is such a direct match, pick it. This resolves the C-family
    // `_top_level_item = CHOICE[function_definition, …, declaration,
    // ALIAS(declaration), …]` tie for a `declaration` child (the bare
    // `declaration` alt wins over the same-surface ALIAS and the hidden
    // pass-through), without disturbing ties whose candidates are
    // supertypes/aliases/SEQs — those fall through to the variant-tag
    // tie-break or defer. Disambiguating a bare symbol from a same-kind
    // ALIAS needs the child's `pre-alias-symbol` witness, which the
    // heuristic fallback still supplies; absent it, the direct bare
    // symbol is the right default.
    if let Some(&child_kind) = demand.first() {
        let direct: Vec<usize> = cands
            .iter()
            .copied()
            .filter(|&i| bare_symbol_name(&alternatives[i]) == Some(child_kind))
            .collect();
        if direct.len() == 1 {
            return Some(direct[0]);
        }
    }
    // Variant-tag tie-break: maximize overlap between the alternative's
    // own literals and the trace's token slots; require a unique non-zero
    // maximum. This runs FIRST so a recorded literal (e.g. a `;` the
    // parser actually emitted, present in `ptrace`) wins — byte-faithful
    // reconstruction is preserved.
    if !trace_tokens.is_empty() {
        let mut best_overlap = 0usize;
        let mut winner: Option<usize> = None;
        let mut winner_count = 0usize;
        for &i in &cands {
            let lits = alt_literals_resolved(grammar, &alternatives[i]);
            let overlap = lits.iter().filter(|l| trace_tokens.contains(l)).count();
            if overlap > best_overlap {
                best_overlap = overlap;
                winner = Some(i);
                winner_count = 1;
            } else if overlap == best_overlap && overlap > 0 {
                winner_count += 1;
            }
        }
        if best_overlap > 0 && winner_count == 1 {
            return winner;
        }
    }
    // No variant tag resolved the tie and no candidate consumes a child
    // (`best_len == 0`): this is an OPTIONAL production — an optional
    // token / separator with nothing structural demanding it. The
    // canonical section omits optional tokens (the categorical ε of a
    // CHOICE-with-BLANK), so prefer `BLANK` when present. A recorded
    // literal would have been caught by the trace tie-break above, so
    // this only fires for genuinely-absent optionals (kotlin's optional
    // `;` / trailing `,`, which the lossy heuristics otherwise emit
    // spuriously on a complement-free schema).
    if best_len == 0 {
        if let Some(b) = alternatives
            .iter()
            .position(|a| matches!(a, Production::Blank))
        {
            return Some(b);
        }
    }
    None
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
            select_choice_with_trace(&g, alts, &["number", "number"], &[], None, &[], &[], None),
            Some(0)
        );
        // demand = two string children → second alt
        assert_eq!(
            select_choice_with_trace(&g, alts, &["string", "string"], &[], None, &[], &[], None),
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
            select_choice_with_trace(&g, alts, &["identifier"], &[], None, &[], &[], None),
            Some(1)
        );
        // An int_literal child matches NEITHER concrete alternative
        // directly (template_parameters needs the whole < int > shape,
        // identifier needs an identifier) → defer, do not steal.
        assert_eq!(
            select_choice_with_trace(&g, alts, &["int_literal"], &[], None, &[], &[], None),
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

    /// A bare SYMBOL naming the child's exact surface kind wins a tie
    /// over a same-surface ALIAS (the C-family `_top_level_item`
    /// declaration tie). The direct, unaliased interpretation is chosen.
    #[test]
    fn direct_bare_symbol_wins_tie_over_alias() {
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "_top_level_item": {"type": "CHOICE", "members": [
                        sym("function_definition"),
                        sym("declaration"),
                        {"type": "ALIAS", "named": true, "value": "declaration",
                         "content": sym("command_declaration")},
                    ]},
                    "function_definition": str_("f"),
                    "declaration": str_("d"),
                    "command_declaration": str_("c"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["_top_level_item"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // bare `declaration` (idx 1) and ALIAS=declaration (idx 2) both
        // match a `declaration` child; function_definition (idx 0) is
        // rejected. The direct bare symbol wins.
        assert_eq!(
            select_choice_with_trace(&g, alts, &["declaration"], &[], None, &[], &[], None),
            Some(1)
        );
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
        assert_eq!(
            select_choice_with_trace(&g, alts, &["binary"], &[], None, &[], &[], None),
            None
        );
    }

    /// An optional token defaults to absent (BLANK) when no child demands
    /// it and the variant tag does not attest the literal — but a recorded
    /// literal still wins (byte-faithful reconstruction).
    #[test]
    fn optional_token_defaults_to_blank_unless_traced() {
        // CHOICE[ ";" , BLANK ] — the optional semicolon (kotlin shape).
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "opt_semi": {"type": "CHOICE", "members": [str_(";"), {"type": "BLANK"}]},
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["opt_semi"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // No child, no trace → omit the optional (BLANK, index 1).
        assert_eq!(
            select_choice_with_trace(&g, alts, &[], &[], None, &[], &[], None),
            Some(1)
        );
        // The parser recorded a ";" (in ptrace) → emit it (index 0).
        assert_eq!(
            select_choice_with_trace(&g, alts, &[], &[], None, &[], &[";".to_owned()], None),
            Some(0)
        );
    }

    /// Literal-keyword CHOICE (kotlin return/throw shape): the child
    /// demand ties, the trace's token fibre disambiguates.
    #[test]
    fn trace_token_breaks_keyword_tie() {
        // jump = CHOICE[ SEQ["return", expr] , SEQ["throw", expr] ]
        // demand [expr] matches BOTH (same child) → structural tie.
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "jump": {"type": "CHOICE", "members": [
                        {"type": "SEQ", "members": [str_("return"), sym("expr")]},
                        {"type": "SEQ", "members": [str_("throw"), sym("expr")]},
                    ]},
                    "expr": str_("e"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["jump"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // No trace → genuine tie → defer.
        assert_eq!(
            select_choice_with_trace(&g, alts, &["expr"], &[], None, &[], &[], None),
            None
        );
        // Trace contains "return" → first alt; "throw" → second.
        assert_eq!(
            select_choice_with_trace(
                &g,
                alts,
                &["expr"],
                &[],
                None,
                &[],
                &["return".to_owned()],
                None
            ),
            Some(0)
        );
        assert_eq!(
            select_choice_with_trace(
                &g,
                alts,
                &["expr"],
                &[],
                None,
                &[],
                &["throw".to_owned()],
                None
            ),
            Some(1)
        );
        // Trace with neither keyword → still ambiguous → defer.
        assert_eq!(
            select_choice_with_trace(
                &g,
                alts,
                &["expr"],
                &[],
                None,
                &[],
                &["xyz".to_owned()],
                None
            ),
            None
        );
    }

    /// A unique viable alternative wins even when it consumes nothing:
    /// `BLANK` is chosen over an optional `_initializer` (`= expr`) when
    /// the demand supplies no value child (the js for-in declarator).
    #[test]
    fn unique_viable_blank_beats_rejected_optional() {
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "declarator": {"type": "SEQ", "members": [
                        {"type": "FIELD", "name": "name", "content": sym("identifier")},
                        {"type": "CHOICE", "members": [sym("_initializer"), {"type": "BLANK"}]},
                    ]},
                    "_initializer": {"type": "SEQ", "members": [str_("="), sym("expression")]},
                    "identifier": str_("x"),
                    "expression": str_("e"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["declarator"] {
            Production::Seq { members } => match &members[1] {
                Production::Choice { members } => members,
                _ => panic!(),
            },
            _ => panic!(),
        };
        // No value child in the demand → `_initializer` (needs an
        // `expression` child) is structurally rejected; `BLANK` is the
        // unique viable alternative and must be chosen, not deferred.
        assert_eq!(
            select_choice_with_trace(&g, alts, &[], &[], None, &[], &[], None),
            Some(1)
        );
        // With a value child available, `_initializer` becomes viable and
        // consumes it → chosen over BLANK (maximal munch).
        assert_eq!(
            select_choice_with_trace(&g, alts, &["expression"], &[], None, &[], &[], None),
            Some(0)
        );
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
        let ends = match_demand(
            &g,
            &g.rules["statements"],
            &["stmt", "stmt"],
            &[],
            0,
            None,
            &mut v,
        );
        assert!(ends.contains(&2), "must fully consume 2 stmts: {ends:?}");
    }

    /// A field-bound literal that contradicts the recorded `field:<name>`
    /// must not let an alternative win by maximal-munch. Models the JS
    /// `_for_header` shape: the `var`+optional-initializer member would
    /// greedily consume two children, but with `field:kind="const"` recorded
    /// it is rejected in favour of the `let|const` member.
    #[test]
    fn field_value_rejects_contradicting_and_absent_literal() {
        let g = grammar(
            &serde_json::json!({
                "name": "test",
                "rules": {
                    "header": {"type": "CHOICE", "members": [
                        // member 0: bare left, no `kind`.
                        {"type": "FIELD", "name": "left", "content": sym("expr")},
                        // member 1: `var` + optional initializer (can munch 2).
                        {"type": "SEQ", "members": [
                            {"type": "FIELD", "name": "kind", "content": str_("var")},
                            {"type": "FIELD", "name": "left", "content": sym("expr")},
                            {"type": "CHOICE", "members": [
                                {"type": "SEQ", "members": [str_("="), sym("expr")]},
                                {"type": "BLANK"},
                            ]},
                        ]},
                        // member 2: `let`/`const`, no initializer (munches 1).
                        {"type": "SEQ", "members": [
                            {"type": "FIELD", "name": "kind", "content":
                                {"type": "CHOICE", "members": [str_("let"), str_("const")]}},
                            {"type": "FIELD", "name": "left", "content": sym("expr")},
                        ]},
                    ]},
                    "expr": str_("x"),
                }
            })
            .to_string(),
        );
        let alts = match &g.rules["header"] {
            Production::Choice { members } => members,
            _ => panic!(),
        };
        // `for (const x …)`: kind=const recorded, two `expr` children. The
        // `var` member (1) contradicts the recorded kind, so it is excluded
        // from the candidates and can NOT win by maximal munch. The bare
        // `left` (0) and `let|const` (2) members tie at length 1, so
        // unification defers (the downstream field-token tie-break picks 2) —
        // the essential property here is that the `var` member never wins.
        assert_ne!(
            select_choice_with_trace(
                &g,
                alts,
                &["expr", "expr"],
                &[],
                None,
                &[("kind", "const")],
                &[],
                None
            ),
            Some(1)
        );
        // `for (x in y)`: no kind recorded. Members 1 and 2 are forced to bind
        // a literal `kind`, so both are rejected; the bare `left` member (0)
        // is the unique remaining candidate and wins instead of emitting a
        // spurious `var`/`let`.
        assert_eq!(
            select_choice_with_trace(&g, alts, &["expr", "expr"], &[], None, &[], &[], None),
            Some(0)
        );
    }
}
