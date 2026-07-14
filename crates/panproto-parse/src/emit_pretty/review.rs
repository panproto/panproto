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

//! `emit_pretty::review` (Phase A decomposition).

use super::{
    ChildCursor, EMIT_DEPTH, EMIT_MU_FRAMES, Edge, Grammar, Output, ParseError, Production, Schema,
    Token, TokenRole, accepts_first_edge, alias_content_is_terminal_pattern,
    aliased_source_literals, alt_satisfies_field_token_restrictions,
    alt_satisfies_pre_alias_constraints, byte_span_consistent_with_parent, children_for,
    classify_seq_positions, clear_field_context, collect_field_names,
    collect_inner_field_names_expanded, contains_newline_pattern, current_field_context,
    enter_ptrace_budget, first_unconsumed_target_fingerprint, has_field_in,
    has_relevant_constraint, has_repeat_in, is_blank_line_rule, is_connector_punctuation,
    is_immediate_token, is_newline_alt, is_newline_like_pattern, is_no_space_external,
    is_ptrace_budget_owner, is_whitespace_external, is_whitespace_only_pattern, is_word_like,
    leaf_terminal_role, left_recursive_alts, literal_strings, literal_value, mandatory_field_names,
    member_has_leading_bracket, pattern_absorbs_leading_space, placeholder_for_pattern,
    pre_alias_symbol, prec_value, ptrace_budget_allows, ptrace_budget_consume, push_field_context,
    reconstruct_subtree_bytes, reduces_to_immediate_token, referenced_symbols,
    repeat_body_is_whole_vertex_item, repeat_has_bracket_keyed_member, seq_bracket_triggers_indent,
    seq_open_bracket_index, unbounded_negated_class, unwrap_prec, unwrap_to_string,
    vertex_has_byte_span, vertex_id_kind, yield_of_production,
};

pub(crate) fn collect_roots(schema: &Schema) -> Vec<&panproto_gat::Name> {
    if !schema.entries.is_empty() {
        return schema
            .entries
            .iter()
            .filter(|name| schema.vertices.contains_key(*name))
            .collect();
    }

    // Fallback: every vertex that is not the target of any structural edge
    // (sorted by id for determinism).
    let mut targets: std::collections::HashSet<&panproto_gat::Name> =
        std::collections::HashSet::new();
    for edge in schema.edges.keys() {
        targets.insert(&edge.tgt);
    }
    let mut roots: Vec<&panproto_gat::Name> = schema
        .vertices
        .keys()
        .filter(|name| !targets.contains(name))
        .collect();
    roots.sort();
    roots
}

/// The set of symbol names referenced anywhere in `rule`, expanding
/// hidden (`_`-prefixed) and supertype rules transitively (cycle-guarded),
/// so a child placeable only through a dispatch chain (cpp constructor's
/// `field_initializer_list` is a direct member of
/// `constructor_or_destructor_definition`) is reachable.
fn rule_symbol_closure<'g>(
    grammar: &'g Grammar,
    rule: &'g Production,
) -> std::collections::HashSet<&'g str> {
    fn walk<'g>(
        grammar: &'g Grammar,
        prod: &'g Production,
        out: &mut std::collections::HashSet<&'g str>,
        visited: &mut std::collections::HashSet<&'g str>,
    ) {
        match prod {
            Production::Symbol { name } => {
                out.insert(name.as_str());
                let expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
                if expand && visited.insert(name.as_str()) {
                    if let Some(r) = grammar.rules.get(name) {
                        walk(grammar, r, out, visited);
                    }
                }
            }
            Production::Alias {
                content,
                value,
                named,
            } => {
                if *named && !value.is_empty() {
                    out.insert(value.as_str());
                }
                walk(grammar, content, out, visited);
            }
            Production::Seq { members } | Production::Choice { members } => {
                for m in members {
                    walk(grammar, m, out, visited);
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
            | Production::Reserved { content, .. } => walk(grammar, content, out, visited),
            _ => {}
        }
    }
    let mut out = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    walk(grammar, rule, &mut out, &mut visited);
    out
}

/// How many of the vertex's distinct child kinds the production `rule`
/// can place (a child of kind K is placeable iff K satisfies some symbol
/// the rule references through its dispatch closure).
fn rule_admits_count(grammar: &Grammar, rule: &Production, demand: &[&str]) -> usize {
    let syms = rule_symbol_closure(grammar, rule);
    demand
        .iter()
        .filter(|k| {
            syms.iter()
                .any(|s| kind_satisfies_symbol(grammar, Some(k), s))
        })
        .count()
}

/// How many of the vertex's ORDERED child kinds the production `rule` can
/// place when walked left-to-right, using the same strict dispatch-only
/// satisfaction the emit walk itself uses ([`match_demand`]/`sat`). Unlike
/// [`rule_admits_count`] (an unordered count over the rule's subtype closure
/// that over-approximates), this respects SEQ order and CHOICE/REPEAT
/// structure: a child kind reachable only inside an already-consumed earlier
/// member is not credited at a later position, and an alias-valued kind is
/// placed only where the grammar actually aliases to it (not merely where a
/// REPEAT slot's subtype closure happens to reach it). The result is the
/// furthest demand index reached from the start — the count the parser's rule
/// would consume in sequence.
fn ordered_consumption(grammar: &Grammar, rule: &Production, demand: &[&str]) -> usize {
    let mut visited = Vec::new();
    super::match_demand(grammar, rule, demand, &[], 0, None, &mut visited)
        .into_iter()
        .max()
        .unwrap_or(0)
}

/// Lower bound on the number of NAMED-symbol child slots a rule's body
/// mandates: SEQ sums its members, REPEAT1 contributes its body once (it
/// must iterate at least once), REPEAT/OPTIONAL/CHOICE/BLANK contribute the
/// minimum over their branches (0 for optionals). A bare named SYMBOL is one
/// slot; hidden (`_`) SYMBOLs expand inline (cycle-guarded). FIELD-of-literal
/// and string/pattern terminals contribute 0. Used to detect when a vertex's
/// OWN rule structurally cannot be the parse (it mandates more children than
/// the vertex actually has, e.g. vhdl `simple_expression`'s REPEAT1 binary
/// chain forced onto a bare aliased `_expr`, emitting a spurious `+`).
fn rule_min_required_children(grammar: &Grammar, rule: &Production) -> usize {
    // This is a least-fixpoint over the rule graph. A naive DFS that pushes
    // every hidden symbol on a path-stack and cuts back-edges to 0 (the BLANK
    // base case) re-explores every distinct path through the shared sub-DAG;
    // yaml is 201/202 hidden rules forming one dense mutually-recursive SCC, so
    // that DFS is exponential and never terminates in practice.
    //
    // The minimum a rule mandates is path-INDEPENDENT once cycles bottom out at
    // 0: that is exactly the least fixpoint of `min[r] = eval(body_r)` with all
    // hidden-symbol references read from `min` (initialized to 0). `eval` is
    // monotone non-decreasing and the values are bounded by the rule sizes, so
    // worklist iteration converges. `min[r] = 0` for an unresolved reference
    // reproduces the DFS's back-edge cut; a fully-resolved rule gets its real
    // count. Resolving over `min` (not recursion) makes each pass linear.
    // `eval` takes `grammar` so a visible (non-hidden) SYMBOL contributes one
    // mandatory child IFF it names a real rule — exactly the original DFS's
    // `grammar.rules.contains_key(name)`. A bare token/external name (no rule
    // entry) contributes 0, as before. Hidden references read the `min` map.
    fn eval(
        grammar: &Grammar,
        p: &Production,
        min: &std::collections::HashMap<String, usize>,
    ) -> usize {
        match p {
            Production::Symbol { name } => {
                if name.starts_with('_') {
                    *min.get(name).unwrap_or(&0)
                } else {
                    usize::from(grammar.rules.contains_key(name))
                }
            }
            Production::Field { content, .. } => eval(grammar, content, min),
            Production::Seq { members } => members.iter().map(|m| eval(grammar, m, min)).sum(),
            Production::Choice { members } => members
                .iter()
                .map(|m| eval(grammar, m, min))
                .min()
                .unwrap_or(0),
            Production::Repeat1 { content } => eval(grammar, content, min),
            Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. }
            | Production::Alias { content, .. } => eval(grammar, content, min),
            // Optional / Repeat (zero-or-more) / Blank / literals: no minimum.
            _ => 0,
        }
    }
    // Resolve the hidden-rule fixpoint over the whole grammar ONCE and cache it
    // on the grammar (it is a pure function of the grammar). Recomputing it on
    // every emit-dispatch call was an O(rules) tax per decision and made yaml's
    // 202-rule SCC grammar intractably slow.
    let min = grammar.min_children.get_or_init(|| {
        let mut min: std::collections::HashMap<String, usize> = grammar
            .rules
            .keys()
            .filter(|k| k.starts_with('_'))
            .map(|k| (k.clone(), 0usize))
            .collect();
        loop {
            let mut changed = false;
            for (name, body) in &grammar.rules {
                if !name.starts_with('_') {
                    continue;
                }
                let v = eval(grammar, body, &min);
                if min.get(name) != Some(&v) {
                    min.insert(name.clone(), v);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        min
    });
    eval(grammar, rule, min)
}

/// Whether a named-ALIAS `content` can genuinely place the children of a
/// vertex whose surface kind equals the alias `value`.
///
/// A named ALIAS renames its `content` production to `value`. The common
/// shape `ALIAS { SYMBOL real_rule, value: K }` is a pure symbol-rename and
/// is always honoured (the symbol's own rule walks the child). But when the
/// content is a structural WRAPPER (REPEAT / SEQ / PREC over a DIFFERENT set
/// of symbols) that merely happens to be renamed to a value that ALSO names
/// a real rule reachable elsewhere, the demand matcher at the parent can
/// mis-consume a child of that kind into the wrapper alias even though the
/// wrapper cannot emit the child's structure. markdown's `document` is the
/// canonical case: member 1 is `ALIAS(REPEAT(_block_not_section), "section")`
/// and member 2 is `REPEAT(SYMBOL section)`; a leading heading section's
/// `atx_heading` grandchild is reachable only through `_section1` (the real
/// `section` rule), NOT through `_block_not_section`, so consuming it into
/// member 1 drops the entire heading. This guard rejects that consumption so
/// the child falls through to the member that can actually emit it.
///
/// Returns true (consume) when the content is a bare symbol-rename, or when
/// the content's dispatch closure admits EVERY one of the child's child
/// kinds. A childless (leaf) matched child is always admitted (the leaf
/// shortcut in `emit_aliased_child` emits its captured literal).
fn aliased_content_admits_child(
    schema: &Schema,
    grammar: &Grammar,
    content: &Production,
    child_id: &panproto_gat::Name,
) -> bool {
    // Pure symbol-rename: emit_aliased_child resolves the symbol's own rule,
    // which is the correct dispatch (the dominant YAML / function_definition
    // case). Always honour it.
    if let Production::Symbol { .. } = content {
        return true;
    }
    let edges = children_for(schema, child_id);
    let demand: Vec<&str> = edges
        .iter()
        .filter_map(|e| vertex_id_kind(schema, &e.tgt))
        .collect();
    if demand.is_empty() {
        return true;
    }
    rule_admits_count(grammar, content, &demand) == demand.len()
}

/// Choose which rule production to walk at a vertex of surface kind
/// `kind`. Normally the vertex's own rule. But when several rules collapse
/// to `kind` via `alias($.src, kind)` and the own rule cannot consume one
/// of the vertex's child edges while an alias-source rule can, walk the
/// source rule (the parser.c/grammar.json desync fallback, e.g. cpp
/// constructor bodies surfaced as `function_definition` whose own rule
/// lacks the `field_initializer_list` member).
fn select_walk_rule<'g>(
    schema: &Schema,
    grammar: &'g Grammar,
    edges: &[&Edge],
    kind: &'g str,
    own_rule: &'g Production,
    vertex_id: &panproto_gat::Name,
) -> (&'g str, &'g Production) {
    let Some(sources) = grammar.named_alias_sources.get(kind) else {
        return (kind, own_rule);
    };
    let demand: Vec<&str> = edges
        .iter()
        .filter_map(|e| schema.vertices.get(&e.tgt).map(|v| v.kind.as_ref()))
        .collect();
    if demand.is_empty() {
        return (kind, own_rule);
    }
    let own_admits = rule_admits_count(grammar, own_rule, &demand);
    if own_admits == demand.len() {
        // `rule_admits_count` is the LOOSE measure: it counts a child kind as
        // admitted when the rule's symbol closure reaches it ANYWHERE,
        // unordered, and through the subtype relation. That over-approximates
        // when a kind is reachable only deep inside an earlier member (julia
        // `macrocall_expression = SEQ[_macro_head, CHOICE[macro_argument_list,
        // BLANK]]`: an `argument_list` is reachable through `_macro_head`'s
        // qualified-head expression dispatch, so the loose count says the rule
        // admits both `macro_identifier` and `argument_list` — yet the rule
        // cannot place `argument_list` as the SECOND child after consuming
        // `macro_identifier`, because the `macro_argument_list` slot is a
        // REPEAT, not a dispatch position the alias-valued `argument_list`
        // satisfies). The strict, ORDERED `match_demand` measure (the same one
        // the emit walk uses, via `sat`) catches this: if the own rule cannot
        // consume the whole demand in sequence but an alias source can, the
        // source is the rule the parser actually used (the paren-form
        // `_closed_macrocall_expression`), so walk it instead.
        if ordered_consumption(grammar, own_rule, &demand) < demand.len() {
            let recorded_src = pre_alias_symbol(schema, vertex_id);
            let mut fit: Option<(&str, &Production)> = None;
            for src in sources {
                if src == kind {
                    continue;
                }
                let Some(src_rule) = grammar.rules.get(src) else {
                    continue;
                };
                if ordered_consumption(grammar, src_rule, &demand) == demand.len() {
                    if recorded_src == Some(src.as_str()) {
                        return (src.as_str(), src_rule);
                    }
                    fit.get_or_insert((src.as_str(), src_rule));
                }
            }
            if let Some(found) = fit {
                return found;
            }
        }
        // The own rule has a slot for every child. Normally that settles it.
        // BUT a rule can ADMIT the demand yet still MANDATE more children than
        // the vertex carries: vhdl's `simple_expression = SEQ[_expr,
        // REPEAT1(SEQ[FIELD(operator,'+'|'-'), _expr])]` is an alias target of
        // a bare `_expr`; with a single-child demand the own rule admits it
        // (one `_expr` slot) but its REPEAT1 still iterates once and emits the
        // forced `+` operator (`0` -> `0 +`). When the own rule's MINIMUM
        // required child count exceeds the demand, it structurally cannot be
        // the parse; prefer an alias source whose rule both admits the demand
        // and does NOT over-mandate. Confirmed by the recorded pre-alias
        // source when present.
        let own_min = rule_min_required_children(grammar, own_rule);
        if own_min <= demand.len() {
            return (kind, own_rule);
        }
        let recorded_src = pre_alias_symbol(schema, vertex_id);
        let mut fit: Option<(&str, &Production)> = None;
        for src in sources {
            if src == kind {
                continue;
            }
            let Some(src_rule) = grammar.rules.get(src) else {
                continue;
            };
            if rule_admits_count(grammar, src_rule, &demand) == demand.len()
                && rule_min_required_children(grammar, src_rule) <= demand.len()
            {
                // Prefer the source the parser actually recorded.
                if recorded_src == Some(src.as_str()) {
                    return (src.as_str(), src_rule);
                }
                fit.get_or_insert((src.as_str(), src_rule));
            }
        }
        return fit.unwrap_or((kind, own_rule));
    }
    // Find the alias source whose production admits strictly more of the
    // demand than the own rule. Sources are tried in grammar order; the
    // one admitting the most wins (full coverage preferred).
    let mut best: Option<(&str, &Production, usize)> = None;
    for src in sources {
        if src == kind {
            continue;
        }
        let Some(src_rule) = grammar.rules.get(src) else {
            continue;
        };
        let c = rule_admits_count(grammar, src_rule, &demand);
        if c > own_admits && best.as_ref().is_none_or(|&(_, _, bc)| c > bc) {
            best = Some((src.as_str(), src_rule, c));
        }
    }
    match best {
        Some((name, r, _)) => (name, r),
        None => (kind, own_rule),
    }
}

/// True when a leaf of this `kind` is verbatim string-region content that
/// must emit tight on both sides with no layout side effects. Three
/// sources, all converging on the same emission:
///
/// * the cassette's [`kind_is_tight_content`](crate::languages::cassettes::GrammarCassette::kind_is_tight_content)
///   (HTML/markup `text`; C#'s `string_content` / `interpolation_brace`);
/// * [`external_content_kinds`](crate::emit_pretty::Grammar::external_content_kinds)
///   — content between *external* string/heredoc delimiters;
/// * [`string_content_kinds`](crate::emit_pretty::Grammar::string_content_kinds)
///   — content / escape leaves between *literal-quote* delimiters.
///
/// Shared by the [`emit_vertex`] and [`emit_aliased_child`] leaf shortcuts
/// so a string-region leaf is treated identically whether the walker
/// reaches it as a standalone vertex or as a named ALIAS child.
fn is_tight_content_kind(
    grammar: &Grammar,
    cassette: Option<&dyn crate::languages::cassettes::GrammarCassette>,
    kind: &str,
) -> bool {
    cassette.is_some_and(|c| c.kind_is_tight_content(kind))
        || grammar.external_content_kinds.contains(kind)
        || grammar.string_content_kinds.contains(kind)
}

pub(crate) fn emit_vertex(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let vertex = schema
        .vertices
        .get(vertex_id)
        .ok_or_else(|| ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: format!("vertex '{vertex_id}' not found"),
        })?;

    // ── Subtree replay (the byte-faithful layout fibre) ───────────────
    // When this vertex carries a `[start-byte, end-byte)` span (the replay
    // path), the layout complement records the EXACT source text of every
    // leaf in its subtree (`literal-value`) and every gap between named
    // children (`interstitial-N`). If those fragments tile the vertex's span
    // completely (`reconstruct_subtree_bytes` returns `Some`), replay them
    // verbatim instead of letting the role table approximate the inter-token
    // spacing — circom `circom  2.0.0 ;` -> `circom 2.0.0;`, cylc
    // `end  = True` -> `end = True`. The gate is the byte span rather than the
    // vertex's own interstitials, so a pure CONTAINER vertex (djot
    // `content` / `block_quote` / `document`, which records the span but no
    // interstitial of its own) is tiled directly from its leaves' fragments —
    // the structural role-table walk otherwise mis-orders block-quote
    // continuation markers (`_block_quote_prefix = REPEAT1(block_quote_marker)`
    // greedily munches every sibling marker into the first iteration).
    //
    // This is canonical-only-OFF by construction: a by-construction /
    // transpiled schema carries no byte anchors (`forget_layout` strips them),
    // so it never enters this branch and keeps the canonical role-table
    // section. And because the reconstruction is taken ONLY when the fibre is
    // self-consistent (the completeness check), it is byte-faithful exactly
    // where the role table already was — it can only ever FIX a boundary the
    // role table got wrong, never corrupt one it got right.
    if vertex_has_byte_span(schema, vertex_id) {
        if let Some(bytes) = reconstruct_subtree_bytes(schema, vertex_id) {
            // A RELOCATED subtree (grafted under a parent that does not contain
            // its recorded span) lost the separators that, in its original
            // source, lived in the surrounding parent's interstitials. Restore a
            // line break on each edge so the byte-faithful body cannot glue onto
            // an adjacent sibling (issue #202: a grafted Python `class` running
            // into the next `def`). Line breaks are idempotent at a line start,
            // so a body that already ends in a newline gains none; and a freshly
            // parsed schema (every child's span nested in its parent's) is never
            // relocated, so this leaves the canonical replay byte-identical.
            let relocated = !byte_span_consistent_with_parent(schema, vertex_id);
            if relocated {
                out.newline();
            }
            out.verbatim(&bytes);
            if relocated {
                out.newline();
            }
            return Ok(());
        }
    }

    // IMMEDIATE_TOKEN at the rule head: emit a tightness marker
    // before any content the leaf shortcut or rule-body walk produces.
    // This is the unique structural site where IMMEDIATE_TOKEN's "no
    // preceding whitespace" property attaches; downstream layout reads
    // the NoSpace marker without re-inspecting the production tree.
    let kind_head = vertex.kind.as_ref();
    if let Some(rule) = grammar.rules.get(kind_head) {
        if is_immediate_token(rule) {
            out.no_space();
        }
    }

    // Leaf shortcut: a vertex carrying a `literal-value` constraint
    // and no outgoing structural edges is a terminal token. Emit the
    // captured value directly. This handles identifiers, numeric
    // literals, and string literals that the parser stored as
    // `literal-value` even on by-construction schemas.
    if let Some(literal) = literal_value(schema, vertex_id) {
        if children_for(schema, vertex_id).is_empty() {
            // Skip leaf shortcut for an EMPTY bracket-pair literal (`()`,
            // `[]`, `{}`) when the vertex has an alias-resolved rule: the
            // rule-based path emits the pair as separate BracketOpen/Close
            // tokens with proper spacing, and there is no content to lose.
            // A NON-empty bracket-pair literal (a token tree captured whole,
            // rust `(clippy::module_inception)`) carries text the rule walk
            // cannot reproduce from a childless vertex — the walk would emit
            // bare `()` and drop the run, including any anonymous punctuation
            // (`::`) that has no CST vertex. Such a literal must take the leaf
            // shortcut and emit verbatim, so by-construction schemas can
            // encode an opaque token tree as a single literal-value leaf.
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            let is_empty_bracket_pair = is_bracket_pair && literal.len() == 2;
            let vkind = vertex.kind.as_ref();
            let has_alias_rule = grammar
                .named_alias_map
                .get(vkind)
                .is_some_and(|src| grammar.rules.contains_key(src));
            if !(is_empty_bracket_pair && has_alias_rule) {
                // An empty bracket-pair literal (`()`, `[]`, `{}` captured
                // as one token, e.g. empty parameters) hugs its callee on
                // the left (`f()`) but still spaces after a keyword
                // (`return ()`). That is exactly the BracketClose role
                // (tight inner/left edge, keyword-spaced). Other leaves
                // keep their delimiter-or-terminal role.
                // A verbatim string-region leaf (HTML `text`, ruby/regex
                // `string_content`, a literal-quoted string's content /
                // escape run, C#'s interpolation braces): emit it tight on
                // both sides with no layout side effects, so the captured
                // text neither grows a sibling-separation space nor (for a
                // literal `{`/`;`) triggers a spurious block newline. See
                // [`is_tight_content_kind`].
                if is_tight_content_kind(grammar, out.cassette, vkind) {
                    out.no_space();
                    out.tight_token(literal);
                    out.no_space();
                    return Ok(());
                }
                let role = if is_bracket_pair {
                    TokenRole::BracketClose
                } else {
                    leaf_terminal_role(grammar, vkind)
                };
                // A terminal whose regex absorbs leading whitespace and whose
                // captured text *already starts with* whitespace carries its
                // own separator: an additional layout space would fold into
                // the text on re-parse and accrete one space per emit. Suppress
                // the layout space so the captured whitespace is the only
                // separator (stable). When the literal does not start with
                // whitespace, the layout space is the genuine separator and
                // must be kept (e.g. Org's `* Heading`).
                if grammar.leading_space_terminals.contains(vkind)
                    && literal.starts_with([' ', '\t'])
                {
                    out.no_space();
                }
                // An immediate-token alias (`character` in a `char_literal`)
                // is lexed only with no preceding whitespace, so hug it to
                // its predecessor: `'hey'` stays tight rather than re-spacing
                // to `' h e y'` (whose spaces re-parse as extra `character`s).
                if grammar.immediate_token_alias_kinds.contains(vkind) {
                    out.no_space();
                }
                out.token_with_role(literal, Some(role));
                // Symmetric to the leading-space suppression above: a terminal
                // whose regex absorbs whitespace and whose captured text *ends*
                // with whitespace carries its own trailing separator. An
                // additional layout space after it would fold into the text on
                // re-parse and accrete one space per emit. Suppress the
                // following layout space so the captured trailing whitespace is
                // the only separator (angular `icu_category = [^{}]+` captures
                // `=0 `; without this the layout adds a space → `=0  {`, which
                // re-parses with the extra space swallowed back into
                // `icu_category`, breaking the case boundary).
                if grammar.leading_space_terminals.contains(vkind) && literal.ends_with([' ', '\t'])
                {
                    out.no_space();
                }
                // This leaf's rule may be a greedy unbounded negated class
                // (`[^...]+`, HTML's unquoted `attribute_value`). Such a
                // terminal keeps consuming admitted characters, so guard the
                // boundary: a following literal whose first char it admits
                // must stay separate (`value=Ok` then ` />`, not `Ok/>`).
                if let Some(negated) = grammar.rules.get(vkind).and_then(|r| match r {
                    Production::Pattern { value } => unbounded_negated_class(value),
                    _ => None,
                }) {
                    out.tokens.push(Token::AbsorberGuard(negated.to_owned()));
                }
                // A rest-of-line terminal (`hash_bang_line = #!.*`) absorbs
                // any following text on the same line, so the next sibling
                // must start on a fresh line (the same fact as a line
                // comment, keyed on kind rather than a text prefix). If the
                // captured text already ends in a newline, `token_with_role`
                // emitted the LineBreak; only add one when it did not.
                if grammar.line_rest_kinds.contains(vkind) && !literal.ends_with(['\n', '\r']) {
                    out.newline();
                }
                return Ok(());
            }
        }
    }

    let kind = vertex.kind.as_ref();
    let edges = children_for(schema, vertex_id);
    if let Some(rule) = grammar.rules.get(kind) {
        // Rule selection among alias-collapsed siblings. Several distinct
        // rules can collapse to the same surface kind via `alias($.src, K)`
        // (cpp `function_definition` is also the alias value of
        // `constructor_or_destructor_definition` / `inline_method_definition`
        // / `operator_cast_definition`). When the parser.c-derived node-types
        // give a vertex a child the *own* rule `K` cannot place (a desync:
        // the collapsed `function_definition` rule omits the constructor-only
        // `field_initializer_list` member), but an alias-source rule's
        // production *does* admit the full child set, walk that source rule.
        // Purely grammar-derived (the alias map + the unification matcher);
        // only fires when the own rule genuinely under-consumes, so the
        // common case (own rule consumes everything) is untouched.
        let (walk_name, walk_rule): (&str, &Production) =
            select_walk_rule(schema, grammar, &edges, kind, rule, vertex_id);
        let old_rule = out.current_rule.take();
        out.current_rule = Some(walk_name.to_owned());
        // An indented-block rule (`block = SEQ[REPEAT(_statement),
        // _dedent]`) is reached directly because its opening `_indent`
        // lives in the hidden parent. Synthesize the opening indent so
        // the body is indented; the rule's own `_dedent` closes it.
        let synthetic_indent = grammar.synthetic_indent_rules.contains(walk_name);
        if synthetic_indent {
            out.indent_open();
        }
        let mut cursor = ChildCursor::new(&edges);
        emit_production(
            protocol,
            schema,
            grammar,
            vertex_id,
            walk_rule,
            &mut cursor,
            out,
        )?;
        drain_extras(protocol, schema, grammar, &mut cursor, out)?;
        out.current_rule = old_rule;
        return Ok(());
    }

    // Named alias resolution: if the vertex kind was produced by
    // `alias($.source, $.kind)`, look up the source rule and walk
    // it. This preserves bracket pairs, separators, and token roles
    // that the source rule defines.
    if let Some(source_name) = grammar.named_alias_map.get(kind) {
        if let Some(rule) = grammar.rules.get(source_name) {
            let old_rule = out.current_rule.take();
            out.current_rule = Some(source_name.to_owned());
            let mut cursor = ChildCursor::new(&edges);
            emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out)?;
            drain_extras(protocol, schema, grammar, &mut cursor, out)?;
            out.current_rule = old_rule;
            return Ok(());
        }
    }

    // No rule for this kind and no named alias. The parser produced
    // it via an external scanner (e.g. YAML's `document` root).
    // Fall back to walking the children directly.
    for edge in &edges {
        emit_vertex(protocol, schema, grammar, &edge.tgt, out)?;
    }
    Ok(())
}

/// Walk a rule at a vertex inside a μ-binder. The wrapping frame is
/// pushed before recursion and popped after, so any SYMBOL inside
/// `rule` that re-enters the same `(vertex_id, rule_name)` pair
/// returns the empty sequence (μ X . body unfolds once).
pub(crate) fn walk_in_mu_frame(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    rule_name: &str,
    rule: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // The frame is keyed on (vertex, rule, consumed-edge count). A re-entry at
    // a STRICTLY LARGER consumed count is a progress-bearing recursion (a
    // right-recursive list flattened onto one vertex consumed an item between
    // levels) and is admitted so each item emits; a re-entry at the SAME count
    // is a zero-progress cycle and collapses to ε (the coinductive
    // μ-fixed-point reading). Recursion is bounded by the edge count.
    let consumed = cursor.consumed.iter().filter(|&&c| c).count();
    let key = (vertex_id.to_string(), rule_name.to_owned(), consumed);
    let inserted = EMIT_MU_FRAMES.with(|frames| frames.borrow_mut().insert(key.clone()));
    if !inserted {
        // We are already walking this rule at this vertex with this much of the
        // cursor consumed: a zero-progress cycle. The coinductive μ-fixed-point
        // reading returns the empty sequence here; the surrounding production
        // resumes after the SYMBOL.
        return Ok(());
    }
    let result = emit_production(protocol, schema, grammar, vertex_id, rule, cursor, out);
    EMIT_MU_FRAMES.with(|frames| {
        frames.borrow_mut().remove(&key);
    });
    result
}

/// The per-vertex `ptrace`-literal budget for `vertex_id`: for each anonymous
/// grammar token the parser recorded (`ptrace-<slot> = T<lit>`), the number of
/// times that exact literal occurred in source. Because the parser records
/// EVERY anonymous token of a node, this count is the literal's true source
/// multiplicity — the byte-faithful cap on how many separator CHOICEs the
/// vertex's by-construction walk may resolve to it (see [`EMIT_PTRACE_BUDGET`]).
/// Empty when the vertex carries no `ptrace` fibre (canonical / transpiled
/// schemas), leaving the literal tie-break unbounded as before.
fn compute_ptrace_budget(
    schema: &Schema,
    vertex_id: &panproto_gat::Name,
) -> std::collections::HashMap<String, usize> {
    let mut budget = std::collections::HashMap::new();
    if let Some(cs) = schema.constraints.get(vertex_id) {
        for c in cs {
            if c.sort.as_ref().starts_with("ptrace-") {
                if let Some(lit) = c.value.strip_prefix('T') {
                    *budget.entry(lit.to_owned()).or_insert(0) += 1;
                }
            }
        }
    }
    budget
}

pub(crate) fn emit_production(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    production: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // Install this vertex's `ptrace` budget the first time the walk descends
    // into it (owner changes). A nested walk of the SAME vertex — a hidden
    // rule inlined onto it (bash `_statements`), or the successive SEQ members
    // / REPEAT iterations of its own rule — keeps the budget already installed,
    // so a separator literal spent at one CHOICE site stays spent at the next.
    // Descending into a CHILD vertex installs (and, on unwind, restores) that
    // child's own budget, so a `;;` inside a nested `case` cannot exhaust the
    // outer case_item's budget.
    let _budget_guard = if is_ptrace_budget_owner(vertex_id) {
        None
    } else {
        enter_ptrace_budget(vertex_id, compute_ptrace_budget(schema, vertex_id))
    };
    let depth = EMIT_DEPTH.with(|d| {
        let v = d.get() + 1;
        d.set(v);
        v
    });
    if depth > 500 {
        EMIT_DEPTH.with(|d| d.set(d.get() - 1));
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: format!(
                "emit_production recursion >500 (likely a cyclic grammar; \
                     vertex='{vertex_id}')"
            ),
        });
    }
    // A SEQ that opens with a bracket pair (`{ … }`, `( … )`, `[ … ]`)
    // introduces a delimited region: a leading extra (comment) attached to
    // this vertex but ordinally before the first element sits INSIDE the
    // brackets. Draining it here would hoist it before the open bracket
    // (`struct // c {` instead of `struct { // c`). Defer the drain so
    // `emit_seq_with_roles` emits it right after the open bracket instead.
    let defer_leading_extras = matches!(
        production,
        Production::Seq { members } if seq_open_bracket_index(members).is_some()
    );
    if !defer_leading_extras {
        drain_extras(protocol, schema, grammar, cursor, out)?;
    }
    let result = emit_production_inner(
        protocol, schema, grammar, vertex_id, production, cursor, out,
    );
    EMIT_DEPTH.with(|d| d.set(d.get() - 1));
    result
}

/// Consume and emit every leading edge on `cursor` whose target kind
/// is in `grammar.extras` (typically `line_comment` / `block_comment`).
/// Extras live outside the production grammar — tree-sitter skips them
/// at parse time and records them as children of the surrounding
/// vertex — so the rule walker cannot reconcile them against the
/// cursor. Draining them as a side channel preserves their content in
/// the output without confusing the structural matchers.
pub(crate) fn drain_extras(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    if grammar.extras.is_empty() {
        return Ok(());
    }
    loop {
        let next_extra: Option<usize> = cursor
            .edges
            .iter()
            .enumerate()
            .find(|(i, _)| !cursor.consumed[*i])
            .and_then(|(i, edge)| {
                let kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref())?;
                if grammar.extras.contains(kind) {
                    Some(i)
                } else {
                    None
                }
            });
        let Some(idx) = next_extra else {
            return Ok(());
        };
        cursor.consumed[idx] = true;
        let target = &cursor.edges[idx].tgt;
        emit_vertex(protocol, schema, grammar, target, out)?;
    }
}

/// Emit a SEQ production with positionally classified token roles.
///
/// Instead of looking up roles from a precomputed flat map (which
/// conflates the same token text across structural contexts within
/// one rule), this function computes roles for each STRING position
/// from the SEQ's own structure at emission time.
pub(crate) fn emit_seq_with_roles(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    members: &[Production],
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
    in_choice: bool,
) -> Result<(), ParseError> {
    let positional_roles = classify_seq_positions(members, in_choice);

    // Detect which bracket-open position triggers indentation so we can
    // emit the matching IndentClose for the bracket-close.
    let indent_open_idx: Option<usize> = positional_roles.iter().enumerate().position(|(i, r)| {
        *r == Some(TokenRole::BracketOpen) && seq_bracket_triggers_indent(members, i, grammar)
    });

    // For word-like bracket pairs (function/end, if/end, etc.), find
    // positions that need LineBreak: the body CHOICE and any FIELD
    // members that follow it (elseif/else clauses, catch blocks, etc.).
    let mut line_break_positions: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    if let Some(oi) = indent_open_idx {
        let open_text = unwrap_to_string(&members[oi]);
        if open_text.is_some_and(is_word_like) {
            let mut found_body = false;
            for (j, member) in members.iter().enumerate().skip(oi + 1) {
                if let Production::Choice { members: alts } = member {
                    let has_blank = alts.iter().any(|a| matches!(a, Production::Blank));
                    let has_block_symbol = alts.iter().any(|a| match a {
                        Production::Symbol { name } => {
                            grammar.rules.get(name).is_some_and(has_repeat_in)
                        }
                        _ => false,
                    });
                    if has_blank && has_block_symbol {
                        line_break_positions.insert(j);
                        found_body = true;
                    }
                } else if found_body && matches!(member, Production::Field { .. }) {
                    line_break_positions.insert(j);
                }
            }
        }
    }

    // The open-bracket position whose region encloses any leading extras
    // (comments). `emit_production` deferred its top-level drain for such a
    // SEQ; drain them here, right after the open bracket is emitted, so a
    // comment ordinally before the first element stays INSIDE the brackets.
    let leading_extra_after = seq_open_bracket_index(members);

    let mut prev_member_emitted_content = false;
    for (i, member) in members.iter().enumerate() {
        let tokens_before_member = out.tokens.len();
        if let Some(value) = unwrap_to_string(member) {
            let role = positional_roles[i].unwrap_or_else(|| {
                if is_word_like(value) {
                    TokenRole::Keyword
                } else {
                    TokenRole::Operator
                }
            });

            if indent_open_idx == Some(i) {
                if is_word_like(value) {
                    out.tokens.push(Token::Lit(value.to_owned(), role));
                    out.tokens.push(Token::IndentOpen);
                } else {
                    out.token_with_indent_open(value, role);
                }
            } else if role == TokenRole::BracketClose && indent_open_idx.is_some() {
                out.tokens.push(Token::IndentClose);
                out.tokens.push(Token::Lit(value.to_owned(), role));
            } else {
                out.token_with_role(value, Some(role));
            }
        } else if reduces_to_immediate_token(member) {
            // The member is an ALIAS-wrapped IMMEDIATE_TOKEN: the lexer admits
            // it only with no preceding whitespace, so hug it to the previous
            // token (tidal `repeat_suffix` `*`<-`2`). This is the alias-wrapped
            // analogue of the rule-head no-space check in `emit_vertex`; it is
            // derived purely from the production shape, so it stays generic.
            if prev_member_emitted_content {
                out.no_space();
            }
            emit_production(protocol, schema, grammar, vertex_id, member, cursor, out)?;
        } else {
            // ForceSpace between consecutive content-producing SEQ
            // members so that sibling-vertex tokens are separated
            // (e.g. echo and $((  ...  )) in bash command). Skip
            // when the current member's rule body starts with a
            // bracket pair, because the preceding Terminal and the
            // bracket should be tight (call pattern like f(...)).
            if i > 0 && unwrap_to_string(&members[i - 1]).is_none() && prev_member_emitted_content {
                let member_starts_with_bracket = member_has_leading_bracket(member, grammar);
                let is_zero_width_external = matches!(
                    member,
                    Production::Symbol { name }
                        if name.starts_with('_') && !grammar.rules.contains_key(name)
                );
                let is_separator_choice = matches!(member, Production::Choice { members: alts }
                    if alts.iter().all(|a| matches!(a, Production::Blank) || unwrap_to_string(a).is_some()));
                let is_repeat = matches!(
                    member,
                    Production::Repeat { .. } | Production::Repeat1 { .. }
                );
                // Never force a space after a token that is tight on its
                // right edge: a BracketOpen (`(`, or a unary-sign prefix
                // classified as BracketOpen) or an IMMEDIATE_TOKEN. The
                // sibling-separation ForceSpace must not override the
                // structural tightness (`f(-1.0)`, not `f(- 1.0)`).
                let prev_tight_right = matches!(
                    out.tokens.last(),
                    Some(Token::Lit(_, TokenRole::BracketOpen | TokenRole::Immediate))
                );
                // The previous *member* is itself a cassette-tight operator
                // literal (bash `variable_assignment`'s `=` / `+=` CHOICE):
                // it hugs its operand, so no sibling-separation space. This
                // is member-scoped — it fires only when the operator is a
                // direct SEQ member here, NOT when a child vertex merely
                // ends in that operator (an empty-value assignment followed
                // by a sibling command keeps its space, because there the
                // previous member is the child SYMBOL, not the operator).
                let prev_member_tight_operator = out
                    .current_rule
                    .as_ref()
                    .zip(out.cassette)
                    .is_some_and(|(rule, cassette)| {
                        literal_strings(&members[i - 1])
                            .iter()
                            .any(|lit| cassette.operator_is_tight(rule, lit))
                    });
                if !member_starts_with_bracket
                    && !is_zero_width_external
                    && !is_separator_choice
                    && !is_repeat
                    && !prev_tight_right
                    && !prev_member_tight_operator
                {
                    out.tokens.push(Token::ForceSpace);
                }
            }
            if line_break_positions.contains(&i) {
                out.newline();
            }
            emit_production(protocol, schema, grammar, vertex_id, member, cursor, out)?;
        }
        prev_member_emitted_content = out.tokens[tokens_before_member..]
            .iter()
            .any(|t| matches!(t, Token::Lit(_, _) | Token::Verbatim(_)));
        // Drain leading extras (comments) right after the open bracket, so
        // a comment that the deferred top-level drain skipped lands INSIDE
        // the bracket region rather than before the opener.
        if leading_extra_after == Some(i) {
            drain_extras(protocol, schema, grammar, cursor, out)?;
        }
    }
    Ok(())
}

/// Count the cursor's unconsumed edges whose target kind is NOT a grammar
/// extra (comment). Operand edges of an unrolled chain are these.
fn unconsumed_non_extra(schema: &Schema, grammar: &Grammar, cursor: &ChildCursor<'_>) -> usize {
    cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, edge)| {
            !cursor.consumed[*i]
                && schema
                    .vertices
                    .get(&edge.tgt)
                    .is_none_or(|v| !grammar.extras.contains(v.kind.as_ref()))
        })
        .count()
}

/// Emit one `step` of an unrolled left-recursive chain: the SEQ members of a
/// left-recursive alternative *after* the leading head `SYMBOL(R)`. Operator
/// literals emit with their role; an embedded `SYMBOL(R)` (the trailing
/// operand of a binary rule) dispatches another base operand; any other
/// member (a postfix suffix `SYMBOL(get_attr)` / FIELD) is walked against the
/// cursor so its own child edge is consumed.
#[allow(clippy::too_many_arguments)]
fn emit_lr_step(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    rule_name: &str,
    rest: &[Production],
    base_choice: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let mut prev_emitted = true;
    for member in rest {
        let tokens_before = out.tokens.len();
        if matches!(member, Production::Symbol { name } if name == rule_name) {
            emit_production(
                protocol,
                schema,
                grammar,
                vertex_id,
                base_choice,
                cursor,
                out,
            )?;
        } else if let Some(value) = unwrap_to_string(member) {
            let role = if is_word_like(value) {
                TokenRole::Keyword
            } else {
                TokenRole::Operator
            };
            out.token_with_role(value, Some(role));
        } else {
            if !prev_emitted {
                out.force_space();
            }
            emit_production(protocol, schema, grammar, vertex_id, member, cursor, out)?;
        }
        prev_emitted = out.tokens.len() > tokens_before;
    }
    Ok(())
}

/// Emit a left-recursive CHOICE rule body by *unrolling* the recursion.
///
/// For a rule `R = CHOICE[ base… | PREC(SEQ[R, rest…]) | … ]`, a top-down walk
/// recurses into the head `R` and collapses (the μ-frame unfolds it to the
/// empty sequence on re-entry), dropping every operand but the base. The
/// parse, however, is a left-leaning chain `((base step₁) step₂) …` whose
/// operands sit as sibling edges on this one vertex. We reconstruct that
/// chain in source order. Two shapes:
///
/// * **Binary** — some left-recursive alt's `rest` ends in `SYMBOL(R)`
///   (query `_group_expression = … | SEQ[R, ".", R]`). All operands share the
///   base kind. Emit base, then repeat `op + base` while operand edges remain.
/// * **Postfix** — no `rest` references `R` (hcl `_expr_term = … |
///   SEQ[R, get_attr] | SEQ[R, index] | SEQ[R, splat]`). Emit the single base
///   operand, then for each remaining child edge pick the matching suffix
///   alternative (by which `rest` its kind satisfies) and emit it.
///
/// `rec_seqs` holds every left-recursive alt's SEQ `[SYMBOL(R), rest…]`;
/// `base_alts` is the reduced CHOICE body (left-recursive alts removed).
#[allow(clippy::too_many_arguments)]
fn emit_left_recursive_unrolled(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    rule_name: &str,
    rec_seqs: &[&[Production]],
    base_alts: Vec<Production>,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let base_choice = Production::Choice { members: base_alts };

    // Binary when any left-recursive alternative's tail references R again.
    let binary = rec_seqs.iter().any(|seq| {
        seq[1..]
            .iter()
            .any(|m| matches!(m, Production::Symbol { name } if name == rule_name))
    });

    // 1. Leftmost / sole base operand.
    emit_production(
        protocol,
        schema,
        grammar,
        vertex_id,
        &base_choice,
        cursor,
        out,
    )?;

    // 2. Apply steps until no operand edge remains.
    let mut guard = 0usize;
    while unconsumed_non_extra(schema, grammar, cursor) > 0 {
        guard += 1;
        if guard > cursor.edges.len() + 1 {
            break;
        }
        let before = unconsumed_non_extra(schema, grammar, cursor);

        if binary {
            // Single-shape repeat: the first left-recursive alt's `rest`.
            emit_lr_step(
                protocol,
                schema,
                grammar,
                vertex_id,
                rule_name,
                &rec_seqs[0][1..],
                &base_choice,
                cursor,
                out,
            )?;
        } else {
            // Postfix: pick the alternative whose `rest` consumes the next
            // unconsumed child edge (its leading suffix SYMBOL satisfies the
            // edge's target kind). `kind_satisfies_symbol` follows the subtype
            // closure, so an `index` alt matches a `new_index`/`legacy_index`
            // child.
            let next_kind = cursor
                .edges
                .iter()
                .enumerate()
                .find(|(i, edge)| {
                    !cursor.consumed[*i]
                        && schema
                            .vertices
                            .get(&edge.tgt)
                            .is_none_or(|v| !grammar.extras.contains(v.kind.as_ref()))
                })
                .and_then(|(_, edge)| schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref()));
            let chosen = rec_seqs.iter().find(|seq| {
                seq[1..].iter().any(|m| match m {
                    Production::Symbol { name } => kind_satisfies_symbol(grammar, next_kind, name),
                    Production::Field { content, .. } => matches!(
                        content.as_ref(),
                        Production::Symbol { name }
                            if kind_satisfies_symbol(grammar, next_kind, name)
                    ),
                    _ => false,
                })
            });
            let Some(seq) = chosen else {
                break;
            };
            emit_lr_step(
                protocol,
                schema,
                grammar,
                vertex_id,
                rule_name,
                &seq[1..],
                &base_choice,
                cursor,
                out,
            )?;
        }

        if unconsumed_non_extra(schema, grammar, cursor) >= before {
            break;
        }
    }
    Ok(())
}

pub(crate) fn emit_production_inner(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    production: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    match production {
        Production::String { value } => {
            // Per-vertex `ptrace` budget: a literal recorded N times in source
            // may be emitted at most N times across this vertex's walk. When a
            // variant CHOICE has already spent the budget for this literal (the
            // second/third `;;` a bash `case_item`'s separator CHOICEs would
            // otherwise fabricate), suppress the duplicate. Literals with no
            // recorded budget (canonical / by-construction schemas, and every
            // token the parser never traced) are always admitted.
            if ptrace_budget_allows(value) {
                out.token(value);
                ptrace_budget_consume(value);
            }
            Ok(())
        }
        Production::Pattern { value } => {
            if let Some(literal) = literal_value(schema, vertex_id) {
                // A terminal whose regex can match a leading space must hug
                // its predecessor: a space inserted here folds into the
                // captured text on re-parse and accretes one space per emit.
                if pattern_absorbs_leading_space(value) {
                    out.no_space();
                }
                out.token_with_role(literal, Some(TokenRole::Terminal));
                // A greedy unbounded negated-class terminal (`[^...]+`)
                // keeps consuming admitted characters: guard the boundary so
                // a following literal whose first char it admits is kept
                // separate (HTML `attribute_value` vs the trailing `/>`).
                if let Some(negated) = unbounded_negated_class(value) {
                    out.tokens.push(Token::AbsorberGuard(negated.to_owned()));
                }
            } else if is_newline_like_pattern(value) {
                // Patterns like `\r?\n`, `\n`, `\r\n` are the structural
                // newline tokens grammars use to separate top-level
                // statements (csound's `_new_line`, ABC's line-end, etc.).
                // Emitting them through the placeholder fallback rendered
                // the bare `_` sentinel between siblings; route them to
                // the layout pass's line-break instead so the output
                // re-parses.
                out.newline();
            } else if is_whitespace_only_pattern(value) {
                // `\s+`, `[ \t]+` and friends are interstitial whitespace
                // tokens. Emit nothing: the layout pass inserts the
                // policy separator between adjacent Lits if needed.
            } else {
                out.token_with_role(&placeholder_for_pattern(value), Some(TokenRole::Terminal));
            }
            Ok(())
        }
        Production::Blank => Ok(()),
        Production::Symbol { name } => {
            // Inside a FIELD body, a SYMBOL consumes by field-name on
            // the outer cursor rather than searching by symbol-match.
            // This covers the simple `FIELD(SYMBOL X)` case as well as
            // every nesting under FIELD that contains SYMBOLs (SEQ,
            // CHOICE, REPEAT, OPTIONAL, ALIAS). Without the override,
            // shapes like `field('args', commaSep1($.X))` consume one
            // field edge in the FIELD handler and then the REPEAT
            // inside SEQ searches the consumed child's cursor — where
            // no sibling field edges sit — and breaks after one
            // iteration.
            if let Some(field) = current_field_context() {
                // A HIDDEN rule under a FIELD that wraps grammar literals
                // around the field's child, e.g. solidity
                // `FIELD(ancestor_arguments, _call_arguments)` where
                // `_call_arguments = SEQ['(', call_argument, …, ')']`. The
                // parser labels the inner `call_argument` edge with the FIELD
                // name AND records the surrounding `(`/`)` as `field:<name>`
                // literal constraints on this vertex. Taking a single field
                // edge here emits the child bare (`AContract 1`) and DROPS the
                // parens. When such `field:<name>` literal constraints exist,
                // expand the hidden rule inline (retaining the field context so
                // its inner bare symbol still takes the field edge) so its
                // literals emit. Gated on the recorded field literals so the
                // common `FIELD(x, _hidden)` pass-through (no wrapped literals,
                // e.g. ruby's many hidden-rule fields) is untouched: there the
                // single field-edge take is correct and expansion would wrongly
                // re-walk / leak the field context.
                let field_sort = format!("field:{field}");
                let has_field_literals = schema
                    .constraints
                    .get(vertex_id)
                    .is_some_and(|cs| cs.iter().any(|c| c.sort.as_ref() == field_sort));
                if has_field_literals && name.starts_with('_') {
                    if let Some(rule) = grammar.rules.get(name) {
                        let old_rule = out.current_rule.take();
                        out.current_rule = Some(name.to_owned());
                        let result = walk_in_mu_frame(
                            protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                        );
                        out.current_rule = old_rule;
                        return result;
                    }
                }
                // Nested-FIELD re-label: a FIELD(outer, _hidden) whose hidden
                // body re-binds the child to a *different* inner FIELD name
                // (swift `FIELD(type, _possibly_implicitly_unwrapped_type)`,
                // whose `_type` body carries `FIELD(name, _unannotated_type)`).
                // The generated parser flattens the hidden rule and labels the
                // child with the INNER field name, so the outer `field` matches
                // no edge while the inner name does. Expand the hidden rule
                // inline (clearing the outer field context) so the inner FIELD
                // claims its correctly-labelled edge instead of dropping the
                // child. Gated on the node-types desync signature (the parser is
                // authoritative): the inner rebound field must be a real field of
                // the parent kind in node-types AND its node-types type set must
                // contain the pending edge's target kind, i.e. the parser truly
                // binds this child under the inner name. This confines the
                // expansion to genuine FIELD-name flattening and leaves the
                // common `FIELD(x, _hidden)` pass-through (and CHOICEs whose
                // alternatives merely happen to nest a same-named field for a
                // different child) untouched.
                if name.starts_with('_') && !cursor.has_field(&field) {
                    if let Some(rule) = grammar.rules.get(name) {
                        let mut inner_fields = std::collections::HashSet::new();
                        let mut seen = std::collections::HashSet::new();
                        collect_inner_field_names_expanded(
                            rule,
                            grammar,
                            &mut inner_fields,
                            &mut seen,
                        );
                        let parent_kind = vertex_id_kind(schema, vertex_id);
                        let nt_fields =
                            parent_kind.and_then(|pk| grammar.node_type_field_children.get(pk));
                        let rebound = inner_fields.iter().any(|f| {
                            if *f == field.as_str() {
                                return false;
                            }
                            // The parser bound a child under the inner name.
                            let Some(child_kind) = cursor
                                .peek_field(f)
                                .and_then(|e| schema.vertices.get(&e.tgt))
                                .map(|v| v.kind.as_ref())
                            else {
                                return false;
                            };
                            // node-types confirms the inner field genuinely
                            // carries this child kind: the flattened re-label,
                            // not a coincidental same-named field nesting.
                            nt_fields
                                .and_then(|m| m.get(*f))
                                .is_some_and(|ks| ks.contains(child_kind))
                        });
                        if rebound {
                            let _clear = clear_field_context();
                            let old_rule = out.current_rule.take();
                            out.current_rule = Some(name.to_owned());
                            let result = walk_in_mu_frame(
                                protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                            );
                            out.current_rule = old_rule;
                            return result;
                        }
                    }
                }
                if let Some(edge) = cursor.take_field(&field) {
                    return emit_in_child_context(
                        protocol, schema, grammar, &edge.tgt, production, out,
                    );
                }
                // grammar.json ↔ parser.c FIELD desync: the grammar rule
                // declares this FIELD, but the generated parser (authoritative
                // via node-types.json) does NOT bind it — the field's child is
                // recorded as a plain `child_of` edge, not `field=`. Detect the
                // desync precisely: the parent kind appears in node-types as a
                // parent with NON-field children, yet node-types lists NO field
                // of this name for it. (enforce's `if` rule has
                // FIELD(condition)/FIELD(consequence) but its node-types entry
                // has `fields: {}` and emits the condition/consequence as
                // unnamed children, so the field demand drops them →
                // `if (true) a;` → `if ()`.) In that case fall back to taking
                // a `child_of` edge whose target satisfies this FIELD's SYMBOL,
                // in source order. Gated on the node-types desync signature so a
                // grammar whose fields the parser DOES bind (the common case,
                // where node_type_field_children carries the field) keeps the
                // exact field-edge match and is never loosened.
                let parent_kind = vertex_id_kind(schema, vertex_id);
                let field_absent_in_node_types = parent_kind.is_some_and(|pk| {
                    grammar.node_type_nonfield_children.contains_key(pk)
                        && !grammar
                            .node_type_field_children
                            .get(pk)
                            .is_some_and(|fields| fields.contains_key(field.as_str()))
                });
                if field_absent_in_node_types {
                    if let Some(edge) = take_symbol_match(grammar, schema, cursor, name) {
                        return emit_in_child_context(
                            protocol, schema, grammar, &edge.tgt, production, out,
                        );
                    }
                }
                // No child edge for this field. If the field's value was
                // an anonymous token (no named child) the walker captured
                // it as a `field:<name>` constraint on this vertex (rust
                // `let _`'s `_` wildcard pattern; field('op', '+') forms).
                // Emit that value rather than dropping the field. Guarded
                // by current_rule so a REPEAT cannot re-emit it: it
                // consumes no child, so the surrounding REPEAT halts.
                let sort = format!("field:{field}");
                if let Some(v) = schema.constraints.get(vertex_id).and_then(|cs| {
                    cs.iter()
                        .find(|c| c.sort.as_ref() == sort)
                        .map(|c| c.value.clone())
                }) {
                    // A `field:<name>` anonymous token is a redundant copy of a
                    // `ptrace` slot, so it draws on the same per-vertex budget:
                    // emit it only while that literal's source count is unspent.
                    if ptrace_budget_allows(&v) {
                        out.token(&v);
                        ptrace_budget_consume(&v);
                    }
                }
                // Otherwise surface nothing; the surrounding REPEAT /
                // OPTIONAL / CHOICE backtracks when it sees no progress.
                return Ok(());
            }
            if is_whitespace_external(name) {
                // Required inter-token whitespace (dockerfile
                // `_non_newline_whitespace` between path arguments).
                // Whether it is an external or a (hidden) rule whose body
                // is a whitespace pattern, its meaning is "the neighbours
                // are separated": force a space and consume no child.
                out.force_space();
                return Ok(());
            }
            // A rule listed in the grammar's top-level `inline` directive is
            // spliced by tree-sitter into every referencing production: it
            // emits NO node of its own and its children (FIELDs + bare
            // SYMBOL members) are promoted to be direct children of the
            // referencing vertex. On the schema side there is therefore no
            // child vertex of this rule's kind, so `take_symbol_match` below
            // would find nothing and the whole inlined body (parameters,
            // body, end_statement, ...) would be silently dropped
            // (brightscript `sub_impl`/`function_impl`). Treat such a rule
            // like a hidden `_`-prefixed rule: expand its body inline so its
            // promoted children take edges from the current cursor. Inline
            // rules with their own literals (brightscript's `comment`) are
            // still consumed correctly because the expansion walks the rule's
            // SEQ in place.
            let is_inlined = name.starts_with('_')
                || (grammar.inline_rules.contains(name) && !grammar.extras.contains(name));
            if is_inlined {
                // Hidden / inlined rule: not a vertex kind on the schema side.
                // Inline-expand the rule body so its children take
                // edges from the current cursor, instead of trying to
                // take a single child edge that "satisfies" the
                // hidden rule and discarding the rest of the body
                // (which would drop tokens like `=` and the trailing
                // value SYMBOL inside e.g. TOML's `_inline_pair`).
                //
                // Wrapped in a μ-frame so a hidden rule that
                // references its own kind cyclically (or another
                // hidden rule that closes the cycle) unfolds once
                // and then collapses to the empty sequence at the
                // second visit, rather than blowing the stack.
                if let Some(rule) = grammar.rules.get(name) {
                    let old_rule = out.current_rule.take();
                    out.current_rule = Some(name.to_owned());
                    let result = walk_in_mu_frame(
                        protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                    );
                    out.current_rule = old_rule;
                    result
                } else {
                    // External hidden rule (declared in the
                    // grammar's `externals` block, scanned by C code,
                    // not listed in `rules`). Heuristic fallback by
                    // name:
                    //
                    // - `_indent` / `*_indent`: open an indent block.
                    //   Indent-based grammars (Python, YAML, qvr)
                    //   declare an `_indent` external scanner before
                    //   the body of a block-bodied declaration; the
                    //   emitted output is unparseable without the
                    //   corresponding indentation jump.
                    // - `_dedent` / `*_dedent`: close the matching
                    //   indent block.
                    // - `_newline` / `*_line_ending` / `*_or_eof`:
                    //   universally newline-or-empty; emitting a
                    //   single newline is the right default for
                    //   grammars like TOML whose `pair` SEQ trails
                    //   into `_line_ending_or_eof`.
                    //
                    // Check the precomputed alias map first: if this
                    // external token appears as the content of an
                    // anonymous ALIAS elsewhere in the grammar, emit
                    // the alias value as the token text.
                    // A delimiter external (string_start/string_end and
                    // friends) hugs the content it brackets, so emit its
                    // text with a bracket role rather than the untyped
                    // default (which spaces like an operator → `' hello '`).
                    let bracket_role = if grammar.external_bracket_opens.contains(name) {
                        Some(TokenRole::BracketOpen)
                    } else if grammar.external_bracket_closes.contains(name) {
                        Some(TokenRole::BracketClose)
                    } else {
                        None
                    };
                    if let Some(alias_value) = grammar.external_alias_map.get(name) {
                        if out
                            .cassette
                            .is_some_and(|c| c.external_leads_no_space(name))
                        {
                            out.no_space();
                        }
                        match bracket_role {
                            Some(role) => out.token_with_role(alias_value, Some(role)),
                            None => out.token(alias_value),
                        }
                        return Ok(());
                    }
                    if let Some(close_lit) = grammar.external_close_text.get(name) {
                        // An external *closing* delimiter whose opener is a
                        // literal STRING (TOML `_multiline_basic_string_end`
                        // closes the `"""`-opened string). The scanner token
                        // has no rule and no resolvable text, so without this
                        // the string is left unterminated (`"""..` with no
                        // close) and the re-parse ERRORs. The string closes
                        // with the same delimiter it opens with; emit it tight
                        // (BracketClose) so it hugs the preceding content.
                        out.no_space();
                        out.token_with_role(close_lit, Some(TokenRole::BracketClose));
                    } else if is_whitespace_external(name) {
                        // A required inter-token whitespace external
                        // (dockerfile `_non_newline_whitespace` between
                        // path args): no text of its own, but it must
                        // separate the neighbours -- force a space.
                        out.force_space();
                    } else if is_no_space_external(name) {
                        // A scanner concatenation / no-space marker
                        // (`_concat`, `_no_space`, ...): the adjacent
                        // tokens are glued with no whitespace. Emit a
                        // NoSpace so the sibling-separation space is
                        // suppressed -- otherwise string content around
                        // an interpolation (`"$a / $b"`) accretes a space
                        // per emit (`"$a  /  $b"`).
                        out.no_space();
                    } else if grammar.external_indent_opens.contains(name) {
                        out.indent_open();
                    } else if grammar.external_indent_closes.contains(name) {
                        out.indent_close();
                    } else if grammar.external_newlines.contains(name)
                        || out.cassette.is_some_and(|c| c.external_is_newline(name))
                    {
                        out.newline();
                    } else if grammar.external_semicolons.contains(name) {
                        out.token_with_role(";", Some(TokenRole::Separator));
                    } else if let Some(default) = out
                        .cassette
                        .and_then(|c| crate::languages::cassettes::resolve_external_token(c, name))
                    {
                        if !default.is_empty() {
                            match bracket_role {
                                Some(role) => out.token_with_role(default, Some(role)),
                                // A connector punctuation resolved from the
                                // cassette (the Haskell-family module-path
                                // `_dot` -> `.`) is tight on both sides
                                // (`Foo.Bar`); without the Connector role the
                                // sibling-separation glues a space and the
                                // re-parse splits the qualified name.
                                None if is_connector_punctuation(default) => {
                                    // A connector punctuation resolved from the
                                    // cassette joins its neighbours into one
                                    // lexeme and hugs both sides. NoSpace is
                                    // authoritative in the layout fold, so it
                                    // survives a surrounding sibling-separation
                                    // ForceSpace (the REPEAT separator that
                                    // would otherwise split `Foo.Bar`).
                                    out.no_space();
                                    out.token_with_role(default, Some(TokenRole::Connector));
                                    out.no_space();
                                }
                                None => out.token(default),
                            }
                        }
                    }
                    Ok(())
                }
            } else if let Some(edge) = { take_symbol_match(grammar, schema, cursor, name) } {
                // For supertype / hidden-rule dispatch the child's
                // own kind names the actual production to walk
                // (`child.kind` IS the subtype). For ALIAS the
                // dependent-optic context is carried by the
                // surrounding `Production::Alias` branch, which calls
                // `emit_aliased_child` directly; we don't reach here
                // for that case. So walking `grammar.rules[child.kind]`
                // via `emit_vertex` is correct: the dependent-optic
                // path is preserved at every site where it actually
                // diverges from `child.kind`.
                emit_vertex(protocol, schema, grammar, &edge.tgt, out)
            } else if vertex_id_kind(schema, vertex_id) == Some(name.as_str()) {
                let rule = grammar
                    .rules
                    .get(name)
                    .ok_or_else(|| ParseError::EmitFailed {
                        protocol: protocol.to_owned(),
                        reason: format!("no production for SYMBOL '{name}'"),
                    })?;
                // Self-reference (`X = ... SYMBOL X ...`): wrap in a
                // μ-frame so re-entry collapses to the empty sequence.
                {
                    let old_rule = out.current_rule.take();
                    out.current_rule = Some(name.to_owned());
                    let result = walk_in_mu_frame(
                        protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                    );
                    out.current_rule = old_rule;
                    result
                }
            } else {
                // Named rule with no matching child: emit nothing and
                // let the surrounding CHOICE / OPTIONAL / REPEAT
                // resolve the absence.
                Ok(())
            }
        }
        Production::Seq { members } => emit_seq_with_roles(
            protocol, schema, grammar, vertex_id, members, cursor, out, false,
        ),
        Production::Choice { members } => {
            // Left-recursive rule body `R = CHOICE[ base… | PREC(SEQ[R,…]) ]`:
            // the top-down walk collapses the head self-reference (μ-frame
            // unfold to ε), dropping every operand but the base. When we are
            // inside the rule whose head this CHOICE is (`out.current_rule`),
            // unroll the chain against the sibling operand edges instead of
            // recursing. Requires at least one operand edge beyond the base
            // so a single, non-chained occurrence still takes the ordinary
            // dispatch path. Purely grammar-shape-derived: no language names.
            //
            // Restricted to HIDDEN (`_`-prefixed) rules: tree-sitter inlines a
            // hidden rule into its parent, so a hidden left-recursive chain
            // flattens its operands as sibling edges on ONE vertex (query
            // `_group_expression`, hcl `_expr_term`) — exactly the shape the
            // μ-frame collapses. A VISIBLE left-recursive rule (chuck
            // `dur_expression`, every C-family precedence rule) instead nests
            // its left operand as a real child vertex of the same kind; the
            // ordinary SYMBOL recursion walks that nested vertex correctly and
            // must not be unrolled.
            if let Some(rule_name) = out.current_rule.clone() {
                if rule_name.starts_with('_') {
                    if let Some(rec_seqs) = left_recursive_alts(members, &rule_name) {
                        if unconsumed_non_extra(schema, grammar, cursor) >= 2 {
                            let base_alts: Vec<Production> = members
                                .iter()
                                .filter(|m| {
                                    !matches!(unwrap_prec(m), Production::Seq { members: s }
                                        if matches!(s.first(), Some(Production::Symbol { name }) if name == &rule_name))
                                })
                                .cloned()
                                .collect();
                            if !base_alts.is_empty() {
                                return emit_left_recursive_unrolled(
                                    protocol, schema, grammar, vertex_id, &rule_name, &rec_seqs,
                                    base_alts, cursor, out,
                                );
                            }
                        }
                    }
                }
            }
            if let Some(matched) = pick_choice_with_cursor(
                schema,
                grammar,
                vertex_id,
                cursor,
                members,
                out.current_rule.as_deref(),
            ) {
                match matched {
                    Production::Seq {
                        members: seq_members,
                    } => emit_seq_with_roles(
                        protocol,
                        schema,
                        grammar,
                        vertex_id,
                        seq_members,
                        cursor,
                        out,
                        true,
                    ),
                    Production::String { value } => {
                        // A bare STRING alternative of a CHOICE. Prefer a
                        // role derived for this token in the current rule
                        // (e.g. a leading unary sign classified as a tight
                        // prefix: `signed_number`'s `-`); otherwise a
                        // word-like token spaces as a keyword and a
                        // punctuation token acts as a separator.
                        let role = out.explicit_role(value).unwrap_or_else(|| {
                            if is_word_like(value) {
                                TokenRole::Keyword
                            } else {
                                TokenRole::Separator
                            }
                        });
                        out.token_with_role(value, Some(role));
                        // Spend one unit of this literal's per-vertex budget: a
                        // CHOICE just resolved to a recorded anonymous token, so
                        // a later separator CHOICE on the same vertex may not
                        // re-emit the same literal past its source count. A
                        // snapshot/restore around a rolled-back REPEAT iteration
                        // un-spends it. No-op for literals with no `ptrace`
                        // budget.
                        ptrace_budget_consume(value);
                        Ok(())
                    }
                    _ => {
                        emit_production(protocol, schema, grammar, vertex_id, matched, cursor, out)
                    }
                }
            } else {
                Ok(())
            }
        }
        Production::Repeat { content } | Production::Repeat1 { content } => {
            // Detect a "separator-leading SEQ" iteration body: SEQ whose
            // first member is a CHOICE containing BLANK (or an OPTIONAL),
            // i.e. the source-level separator between two iterations is
            // syntactically optional. When the chosen alternative for
            // that separator slot emits zero content tokens at runtime,
            // there was no source-level separator between this iteration
            // and the previous one; the layout pass must suppress its
            // policy separator to match the source's tight adjacency.
            //
            // Categorical reading: REPEAT body `B = SEQ(SEP, BODY)` is
            // the pullback of two halves. The bytes emitted in iteration
            // k+1 are a concatenation of `SEP_k+1` and `BODY_k+1`; if
            // `SEP_k+1` is the empty word, the concatenation of
            // `BODY_k` and `BODY_k+1` must remain a single contiguous
            // span. Hence the NoSpace marker.
            // Also detect mandatory separators: STRING at position 0
            // of a SEQ body (e.g. `SEQ[";", SYMBOL stmt]` in Python's
            // _simple_statements). For these, the cassette may override
            // the separator with a line break.
            let mandatory_sep_text: Option<&str> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => unwrap_to_string(&members[0]),
                _ => None,
            };
            let separator_leading_seq: Option<&[Production]> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => {
                    let first = &members[0];
                    let is_mandatory_sep = unwrap_to_string(first).is_some();
                    let cassette_overrides = is_mandatory_sep
                        && unwrap_to_string(first).is_some_and(|sep| {
                            out.cassette.is_some_and(|c| c.separator_is_line_break(sep))
                        });
                    let is_separator_slot = match first {
                        Production::Choice { members } => {
                            members.iter().any(|m| matches!(m, Production::Blank))
                        }
                        Production::Optional { .. } => true,
                        _ => cassette_overrides,
                    };
                    if is_separator_slot {
                        Some(members.as_slice())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            // A REPEAT whose body is a single FIELD-wrapped item (`field('item',
            // $._expr)`) or a bare named-rule SYMBOL iterates over DISTINCT
            // sibling vertices with no grammatical separator between them — the
            // source separates them by whitespace alone (Lisp/janet list items:
            // `[:hi @{...}]`). Consecutive whole-vertex items must therefore be
            // space-separated, even when the next item's first token is a
            // bracket-open (`@{`, `(`, `[`): the role table glues `Terminal`
            // before `BracketOpen` (the `f(...)` call pattern), which would fuse
            // two sibling items (`:hi@{...}`, mis-parsing the table as a struct).
            // Force a separator between such items; an explicit NoSpace inside an
            // item still wins (it is authoritative in the layout fold), so this
            // never tightens an intra-item adjacency.
            let item_per_iteration = matches!(content.as_ref(), Production::Field { .. })
                || matches!(content.as_ref(), Production::Symbol { name }
                    if grammar.rules.contains_key(name) && !name.starts_with('_'));

            // A REPEAT of a hidden CHOICE-of-whole-vertex items with NO
            // grammatical separator (pkl `objectBody`'s `REPEAT(_objectMember)`,
            // `_objectMember = CHOICE[objectProperty|objectEntry|objectElement|…]`)
            // juxtaposes distinct sibling vertices. When an item can begin with a
            // bracket-open, the source-level whitespace/newline boundary is
            // significant: emitted with only a space, the next item's `["k"]` is
            // absorbed as a SUBSCRIPT of the prior item's trailing terminal
            // (`1 ["k"]` re-lexes as `1["k"]`). Force a newline between such
            // iterations. Restricted to brace-delimited grammars (no
            // synthetic-indent externals) so indent-sensitive blocks (python
            // `block`, layout-managed via `_indent`/`_dedent`) — whose line
            // structure already comes from the indent machinery — are untouched.
            let item_needs_newline = !item_per_iteration
                && grammar.external_indent_opens.is_empty()
                && repeat_body_is_whole_vertex_item(content, grammar)
                && repeat_has_bracket_keyed_member(content, grammar);

            // A REPEAT body `SEQ[item.., CHOICE[<newline-separator>|BLANK]]`
            // whose TRAILING optional slot is a statement-terminating NEWLINE
            // separator (a newline-classified external, or a newline pattern)
            // iterates whole statements separated by a newline. When that slot
            // is absent (BLANK chosen, the canonical default for optionals),
            // consecutive statements merge onto one line; for grammars where a
            // statement boundary is significant (V's `_automatic_separator`
            // between `*ap = size` and the prior stmt — without it the leading
            // `*` re-lexes as a binary `&u64(a) * ap`), force a newline between
            // iterations so the statement boundary survives.
            let trailing_newline_separator = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => members
                    .last()
                    .is_some_and(|last| seq_trailing_newline_separator(grammar, last)),
                _ => false,
            };

            // A REPEAT body `SEQ[item.., MANDATORY_STRING_sep]` whose LAST
            // member is a bare separator STRING (`;`/`,`) iterates items each
            // FOLLOWED by that separator. The enclosing rule typically pairs it
            // with a trailing optional item slot (sql `program =
            // SEQ[REPEAT(SEQ[CHOICE[stmt..], ";"]), CHOICE[statement|BLANK]]`):
            // a final item with NO trailing separator in source is parsed into
            // that trailing slot, so the REPEAT must iterate one fewer time
            // than there are item children. The grammar walk, by contrast,
            // greedily munches every item child into the REPEAT and emits a
            // separator after each, fabricating a terminator the source did not
            // have (`SELECT 1` -> `SELECT 1;`). On the REPLAY path the parent
            // vertex records exactly how many separators occurred as `ptrace`
            // anonymous-token entries (`ptrace-k = T;`); cap the number of
            // emitted separators at that recorded count, suppressing the
            // trailing separator on the final, separator-less iteration. With
            // NO recorded ptrace (canonical / by-construction schemas) the
            // budget is `None` and every iteration emits its separator as
            // before, preserving the existing canonical behaviour.
            let trailing_mandatory_sep: Option<&str> = match content.as_ref() {
                Production::Seq { members }
                    if members.len() >= 2 && separator_leading_seq.is_none() =>
                {
                    members.last().and_then(unwrap_to_string)
                }
                _ => None,
            };
            // Count the recorded `T<sep>` anonymous-token traces on the parent
            // vertex. This is the source-faithful number of separators; emit at
            // most this many. `None` ⇒ no trace ⇒ unbounded (canonical default).
            let sep_budget: Option<usize> = trailing_mandatory_sep.and_then(|sep| {
                let cs = schema.constraints.get(vertex_id)?;
                let has_ptrace = cs.iter().any(|c| c.sort.as_ref().starts_with("ptrace-"));
                if !has_ptrace {
                    return None;
                }
                let count = cs
                    .iter()
                    .filter(|c| {
                        c.sort.as_ref().starts_with("ptrace-")
                            && c.value.strip_prefix('T') == Some(sep)
                    })
                    .count();
                Some(count)
            });
            let mut seps_emitted = 0usize;

            // The REPEAT body's LEADING mandatory token. A body
            // `SEQ[<concrete SYMBOL>, …]` whose first member is a required
            // reference to a concrete (non-hidden, non-optional) rule can only
            // start an iteration when an unconsumed edge satisfies that symbol.
            // Without this guard the SEQ walk proceeds past a non-matching
            // leading keyword and lets a LATER member greedily consume an edge
            // that belongs to a FOLLOWING production: sql `case`'s
            // `REPEAT(SEQ[keyword_when, _expr, keyword_then, _expr])` runs a
            // phantom iteration whose `keyword_when` does not match but whose
            // `_expr` steals the trailing `ELSE` clause's expression, dropping
            // the `keyword_else`. The guard is conservative: it fires only when
            // the leading member is a bare SYMBOL to a present, concrete rule
            // (not hidden/`_`-prefixed, not aliased, not an external), so a
            // supertype/hidden leading member still iterates as before.
            let repeat_lead_symbol: Option<&str> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => match &members[0] {
                    Production::Symbol { name }
                        if !name.starts_with('_')
                            && grammar.rules.contains_key(name)
                            && !grammar.named_alias_map.contains_key(name.as_str()) =>
                    {
                        Some(name.as_str())
                    }
                    _ => None,
                },
                _ => None,
            };

            let mut emitted_any = false;
            loop {
                let cursor_snap = cursor.consumed.clone();
                let out_snap = out.snapshot();
                let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
                // Stop iterating when the body's leading mandatory symbol cannot
                // be satisfied: a further iteration would steal a following
                // production's child (the sql CASE/ELSE drop above).
                if let Some(lead) = repeat_lead_symbol {
                    let lead_available = cursor.edges.iter().enumerate().any(|(i, e)| {
                        !cursor.consumed[i]
                            && kind_satisfies_symbol(
                                grammar,
                                schema.vertices.get(&e.tgt).map(|v| v.kind.as_ref()),
                                lead,
                            )
                    });
                    if !lead_available {
                        break;
                    }
                }
                if item_per_iteration && emitted_any {
                    out.force_space();
                }
                if item_needs_newline && emitted_any {
                    out.newline();
                }
                if trailing_newline_separator && emitted_any {
                    out.newline();
                }
                let result: Result<(), ParseError> =
                    if let Some(seq_members) = separator_leading_seq {
                        // Emit the separator slot first and observe
                        // whether it contributed any Lit. If not, push
                        // a NoSpace marker before walking the remaining
                        // SEQ members. The OutputSnapshot here covers
                        // only the separator's emission window.
                        let cassette_replaces_sep = mandatory_sep_text.is_some_and(|sep| {
                            out.cassette.is_some_and(|c| c.separator_is_line_break(sep))
                        });
                        let pre_sep = out.snapshot();
                        let sep_result = if cassette_replaces_sep {
                            out.newline();
                            Ok(())
                        } else {
                            emit_production(
                                protocol,
                                schema,
                                grammar,
                                vertex_id,
                                &seq_members[0],
                                cursor,
                                out,
                            )
                        };
                        match sep_result {
                            Err(e) => Err(e),
                            Ok(()) => {
                                // The separator slot emitted no Lit (its BLANK
                                // alternative was chosen). Normally that means
                                // the two iterations were tightly adjacent in
                                // source, so suppress the policy separator
                                // (NoSpace). But when the separator CHOICE
                                // offers an explicit SEPARATOR token (`;`/`,`/a
                                // newline pattern) as its non-BLANK alternative,
                                // its absence means the items were separated by
                                // WHITESPACE/newline (pony `block = SEQ[expr,
                                // REPEAT(SEQ[CHOICE[";"|BLANK], expr])]` lists
                                // `1`/`2`/`3` on separate lines), NOT glued: a
                                // forced NoSpace would FUSE two value terminals
                                // (`1 2`->`12`, re-parsed as one number). In
                                // that case let the default role spacing apply
                                // (keeps them separate) instead of NoSpace.
                                let sep_is_optional_statement_sep =
                                    choice_offers_separator_literal(&seq_members[0]);
                                if !cassette_replaces_sep
                                    && !sep_is_optional_statement_sep
                                    && !out.lit_emitted_since(pre_sep)
                                {
                                    out.no_space();
                                }
                                let mut rest_result = Ok(());
                                for member in &seq_members[1..] {
                                    rest_result = emit_production(
                                        protocol, schema, grammar, vertex_id, member, cursor, out,
                                    );
                                    if rest_result.is_err() {
                                        break;
                                    }
                                }
                                rest_result
                            }
                        }
                    } else if let Some(budget) = sep_budget {
                        // Trailing-mandatory-separator body on the replay path:
                        // emit the item members, then the trailing separator
                        // only while the recorded ptrace budget remains. Once
                        // exhausted, the final item is emitted with no
                        // separator (it belongs to the rule's trailing optional
                        // slot in source). `content` is a SEQ by construction
                        // (trailing_mandatory_sep only matches a multi-member
                        // SEQ); emit all but the last member, then the last
                        // (separator) member conditionally.
                        let Production::Seq {
                            members: seq_members,
                        } = content.as_ref()
                        else {
                            unreachable!("trailing_mandatory_sep implies a SEQ body")
                        };
                        let (sep_member, lead_members) = seq_members.split_last().expect("len>=2");
                        let mut body_result = Ok(());
                        for member in lead_members {
                            body_result = emit_production(
                                protocol, schema, grammar, vertex_id, member, cursor, out,
                            );
                            if body_result.is_err() {
                                break;
                            }
                        }
                        if body_result.is_ok() && seps_emitted < budget {
                            body_result = emit_production(
                                protocol, schema, grammar, vertex_id, sep_member, cursor, out,
                            );
                            if body_result.is_ok() {
                                seps_emitted += 1;
                            }
                        }
                        body_result
                    } else {
                        emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
                    };
                let consumed_after = cursor.consumed.iter().filter(|&&c| c).count();
                if result.is_err() || consumed_after == consumed_before {
                    cursor.consumed = cursor_snap;
                    out.restore(out_snap);
                    break;
                }
                emitted_any = true;
            }
            if matches!(production, Production::Repeat1 { .. }) && !emitted_any {
                emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)?;
            }
            Ok(())
        }
        Production::Optional { content } => {
            let cursor_snap = cursor.consumed.clone();
            let out_snap = out.snapshot();
            let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
            let result =
                emit_production(protocol, schema, grammar, vertex_id, content, cursor, out);
            // OPTIONAL is a backtracking site: if the inner production
            // errored *or* made no progress without leaving a witness
            // constraint, restore both cursor and output to their
            // pre-attempt state. Mirrors `Repeat`'s loop body.
            if result.is_err() {
                cursor.consumed = cursor_snap;
                out.restore(out_snap);
                return result;
            }
            let consumed_after = cursor.consumed.iter().filter(|&&c| c).count();
            if consumed_after == consumed_before
                && !has_relevant_constraint(content, schema, vertex_id)
            {
                cursor.consumed = cursor_snap;
                out.restore(out_snap);
            }
            Ok(())
        }
        Production::Field { name, content } => {
            // Set the field context for the duration of `content`'s
            // walk and emit the content against the *outer* cursor.
            // The SYMBOL handler picks up the context and pulls
            // successive `take_field(name)` edges as it encounters
            // SYMBOLs anywhere under `content` (under SEQ, CHOICE,
            // REPEAT, OPTIONAL, ALIAS — arbitrarily nested). This
            // subsumes the prior carve-outs for FIELD(REPEAT(...)),
            // FIELD(REPEAT1(...)), and the bare FIELD(SYMBOL ...)
            // case, and adds coverage for
            // `field('xs', commaSep1($.X))` which expands to
            // FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X)))) and
            // any other shape where REPEAT/REPEAT1 sits inside SEQ /
            // CHOICE / OPTIONAL under a FIELD. A FIELD that wraps a
            // non-SYMBOL production (e.g. `field('op', '+')` or
            // `field('op', CHOICE(STRING ...))`) still works: STRING
            // handlers ignore the context and emit literals
            // directly, so the operator token survives the round
            // trip.
            let _guard = push_field_context(name);
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::Alias {
            content,
            named,
            value,
        } => {
            // A named ALIAS rewrites the parser-visible kind to
            // `value`. If the cursor has an unconsumed child whose
            // kind matches that alias name, take it and emit the
            // child using the alias's INNER content as the rule
            // (e.g. `ALIAS { SYMBOL real_rule, value: "kind_x" }`
            // means a `kind_x` vertex on the schema should be walked
            // through `real_rule`'s body, not through whatever rule
            // happens to be keyed under `kind_x`). This is the
            // dependent-optic shape: the rule the emitter walks at a
            // child position is determined by the parent's chosen
            // alias, not by the child kind alone — without it,
            // grammars like YAML that introduce the same kind through
            // many ALIAS sites lose the parent context the moment
            // emit_vertex is called.
            if *named && !value.is_empty() {
                if let Some(edge) = cursor.take_matching(|edge| {
                    schema
                        .vertices
                        .get(&edge.tgt)
                        .map(|v| v.kind.as_ref() == value.as_str())
                        .unwrap_or(false)
                    // Only consume the kind-matching child when the alias's
                    // content can actually emit its structure. A structural
                    // wrapper alias (markdown `ALIAS(REPEAT(_block_not_section),
                    // "section")`) must not steal a `section` child whose
                    // grandchildren it cannot place; that child belongs to a
                    // later member that walks the real rule.
                        && aliased_content_admits_child(schema, grammar, content, &edge.tgt)
                }) {
                    return emit_aliased_child(protocol, schema, grammar, &edge.tgt, content, out);
                }
            }
            // For anonymous aliases (named: false) whose content is an
            // external scanner token with no grammar rule (e.g.
            // JavaScript's `_ternary_qmark` aliased to `"?"`), emit the
            // alias value directly. The content's SYMBOL handler would
            // fall through the external-token heuristic and produce
            // nothing; the alias value IS the token text.
            //
            // The same applies when the content is a bare PATTERN with
            // no captured literal-value: case-insensitive grammars spell
            // keywords as patterns (`[pP][rR]…`) aliased to the canonical
            // word (Ada's `procedure`, `is`, `begin`, `end`). The pattern
            // alone would emit a `_` placeholder; the alias value is the
            // keyword text and re-parses to the same kind.
            if !*named && !value.is_empty() {
                if let Production::Symbol { name: sym } = content.as_ref() {
                    // Any external scanner symbol (no grammar rule) aliased
                    // to a literal: the alias value IS the token text. This
                    // covers `_`-prefixed externals AND unprefixed ones like
                    // rust's `string_close` (aliased to `"`); without it the
                    // closing string delimiter emits nothing and every
                    // string becomes an unterminated ERROR on re-parse.
                    if !grammar.rules.contains_key(sym) {
                        // A cassette-declared immediate external (C#'s
                        // interpolation delimiters) must hug its predecessor.
                        if out.cassette.is_some_and(|c| c.external_leads_no_space(sym)) {
                            out.no_space();
                        }
                        out.token(value);
                        return Ok(());
                    }
                }
                if alias_content_is_terminal_pattern(content)
                    && literal_value(schema, vertex_id).is_none()
                {
                    out.token(value);
                    return Ok(());
                }
            }
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::ImmediateToken { content } => {
            // IMMEDIATE_TOKEN is the grammar's explicit signal that the
            // wrapped token must have no preceding whitespace. Lift it
            // to a NoSpace marker here, at the unique structural site
            // where the property is declared. The layout pass reads
            // the marker; downstream code does not need to inspect
            // production shapes to recover this property.
            out.no_space();
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::Token { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
    }
}

/// Take the next cursor edge whose target vertex's kind matches the
/// SYMBOL `name` directly or via inline expansion of a hidden rule.
pub(crate) fn take_symbol_match<'a>(
    grammar: &Grammar,
    schema: &Schema,
    cursor: &mut ChildCursor<'a>,
    name: &str,
) -> Option<&'a Edge> {
    // Prefer non-field edges (`child_of`) to avoid consuming a
    // field-named edge that a later FIELD handler should claim.
    // Field-named edges (edge.kind != "child_of") are reserved for
    // the FIELD production that names them; consuming one here would
    // steal it from its intended handler (e.g. `as_pattern`'s
    // `alias` field edge consumed by the leading `expression`
    // SYMBOL instead of the trailing FIELD "alias" handler).
    if let Some(edge) = cursor.take_matching(|edge| {
        edge.kind.as_ref() == "child_of" && {
            let target_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
            kind_satisfies_symbol(grammar, target_kind, name)
        }
    }) {
        return Some(edge);
    }
    cursor.take_matching(|edge| {
        let target_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
        kind_satisfies_symbol(grammar, target_kind, name)
    })
}

/// Decide whether a schema vertex of kind `target_kind` satisfies a
/// SYMBOL `name` reference in the grammar.
///
/// Operates as an O(1) lookup against the precomputed subtype
/// closure built at [`Grammar::from_bytes`]. The semantic content is
/// "K satisfies SYMBOL S iff K is reachable from S by walking the
/// grammar's hidden, supertype, and named-alias dispatch": this is
/// exactly the relation tree-sitter induces on `(parser-visible kind,
/// rule-position)` pairs.
pub(crate) fn kind_satisfies_symbol(
    grammar: &Grammar,
    target_kind: Option<&str>,
    name: &str,
) -> bool {
    let Some(target) = target_kind else {
        return false;
    };
    if target == name {
        return true;
    }
    grammar
        .subtypes
        .get(target)
        .is_some_and(|set| set.contains(name))
}

/// Emit a child reached through an ALIAS production using the
/// alias's inner content as the rule, not `grammar.rules[child.kind]`.
///
/// This carries the dependent-optic context across the ALIAS edge:
/// at the parent rule's site we know which underlying production the
/// alias wraps (typically `SYMBOL real_rule`), and that's the
/// production that should drive the emit walk on the child's
/// children. Looking up `grammar.rules.get(child.kind)` instead would
/// either fail (the renamed kind has no top-level rule, e.g. YAML's
/// `block_mapping_pair`) or pick an arbitrary same-kinded rule from
/// elsewhere in the grammar.
///
/// Walk-context invariant. The dependent-optic shape of `emit_pretty`
/// says: the production walked at any vertex is determined by the
/// path from the root through the grammar, not by the vertex kind in
/// isolation. Two dispatch sites realise that invariant:
///
/// * [`emit_vertex`] looks up `grammar.rules[child.kind]` and walks
///   it. Correct for supertype / hidden-rule dispatch: the child's
///   kind on the schema IS the subtype tree-sitter selected, so its
///   top-level rule is the right production to walk.
/// * `emit_aliased_child` threads the parent rule's `Production`
///   directly (the inner `content` of `Production::Alias`) and walks
///   it on the child's children. Correct for ALIAS dispatch: the
///   child's kind on the schema is the alias's `value` (a renamed
///   kind that may have no top-level rule), and the production to
///   walk is the alias's content body, supplied by the parent.
///
/// Together these cover every site where the rule-walked-at-child
/// diverges from `grammar.rules[child.kind]`; the recursion site for
/// plain SYMBOL therefore correctly delegates to `emit_vertex`, and
/// we do not need a richer `WalkContext` value passed by reference.
/// The grammar dependency is the thread.
pub(crate) fn emit_aliased_child(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    child_id: &panproto_gat::Name,
    content: &Production,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // Subtree replay (the byte-faithful layout fibre) — mirror of the
    // `emit_vertex` entry guard. An aliased child reached here (a CHOICE alt
    // whose `value` renames a SYMBOL: vhdl's `selected_concurrent_signal_
    // assignment` is the alias of `selected_waveform_assignment`) records the
    // EXACT source text of its subtree (`literal-value` leaves + `interstitial-N`
    // gaps) carrying the spacing between its anonymous keyword tokens and its
    // named children (vhdl `with … select … <= …`). Without this branch the
    // aliased subtree falls straight to the role-table production walk, which
    // approximates the inter-token spacing and can drop a separator the role
    // table got wrong (`select\n t` -> `selectt`). Like `emit_vertex`, the gate
    // is the byte span and replay is taken ONLY when the fibre tiles the span
    // completely, so it is canonical-only-OFF (a `forget_layout` schema has no
    // byte anchors) and can never corrupt a boundary the role table got right.
    if vertex_has_byte_span(schema, child_id) {
        if let Some(bytes) = reconstruct_subtree_bytes(schema, child_id) {
            // Restore a line break on each edge of a relocated aliased subtree;
            // see the matching guard in `emit_vertex` for the rationale.
            let relocated = !byte_span_consistent_with_parent(schema, child_id);
            if relocated {
                out.newline();
            }
            out.verbatim(&bytes);
            if relocated {
                out.newline();
            }
            return Ok(());
        }
    }
    // Leaf shortcut: if the child has a literal-value and no
    // structural children, emit the captured text. Skip for bracket-pair
    // literals when the production resolves to a rule with those brackets.
    if let Some(literal) = literal_value(schema, child_id) {
        if children_for(schema, child_id).is_empty() {
            let kind = vertex_id_kind(schema, child_id).unwrap_or("");
            // A verbatim string-region leaf reached as an aliased child
            // (C#'s `interpolation_brace` / `string_content`, which the
            // interpolation rule walks as named ALIASes rather than as
            // standalone vertices): emit tight with no layout side effects,
            // exactly as the `emit_vertex` leaf shortcut does. This is
            // checked BEFORE the bracket-pair guard below, because a
            // string's captured content may itself *look* like a bracket
            // pair (an interpolated `$"{{nope}}"` has the verbatim escaped
            // text `{{nope}}` as one `string_content` leaf): treating it as
            // a `{ … }` block would drop it through the rule-walk path. A
            // string-content kind is never a structural bracket, whatever
            // its bytes. Without this the literal `{` of an interpolation
            // brace also block-indents (`{\n  x }`), breaking the re-parse.
            if is_tight_content_kind(grammar, out.cassette, kind) {
                out.no_space();
                out.tight_token(literal);
                out.no_space();
                return Ok(());
            }
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            // Only an EMPTY bracket-pair literal (`()`, `[]`, `{}`) rule-walks
            // to recover its open/close roles. A non-empty bracket-pair literal
            // (an opaque token tree captured whole, `(clippy::module_inception)`)
            // carries content the childless rule walk cannot reproduce — it
            // would emit bare `()` and drop the run — so it emits verbatim, like
            // the matching guard in `emit_vertex`.
            let is_empty_bracket_pair = is_bracket_pair && literal.len() == 2;
            if !is_empty_bracket_pair {
                // See the matching guard in `emit_vertex`: a captured leading
                // whitespace is the separator, so suppress the redundant layout
                // space to keep the fixed point (INI's `setting_value`); keep
                // it when the literal carries no leading whitespace.
                if grammar.leading_space_terminals.contains(kind)
                    && literal.starts_with([' ', '\t'])
                {
                    out.no_space();
                }
                if grammar.immediate_token_alias_kinds.contains(kind) {
                    out.no_space();
                }
                // A whole bracket pair hugs its callee on the left like a call
                // (`allow(…)`); a plain leaf keeps its terminal role.
                let role = if is_bracket_pair {
                    TokenRole::BracketClose
                } else {
                    leaf_terminal_role(grammar, kind)
                };
                out.token_with_role(literal, Some(role));
                // Symmetric trailing-space suppression (see `emit_vertex`): a
                // captured trailing whitespace is the separator, so suppress
                // the redundant following layout space to keep the fixed point.
                if grammar.leading_space_terminals.contains(kind) && literal.ends_with([' ', '\t'])
                {
                    out.no_space();
                }
                return Ok(());
            }
        }
    }

    // Clear the enclosing FIELD context so it does not leak into the
    // aliased child's production walk. Without this, a FIELD("alias")
    // containing an ALIAS whose content is SYMBOL "expression" would
    // cause the inner SYMBOL handler to pull by field name "alias"
    // instead of by symbol match, failing to find the child edge.
    let _guard = clear_field_context();

    // Resolve `content` to a rule when it's a SYMBOL (the dominant
    // shape: `ALIAS { content: SYMBOL real_rule, value: "kind_x" }`).
    if let Production::Symbol { name } = content {
        if let Some(rule) = grammar.rules.get(name) {
            let edges = children_for(schema, child_id);
            // Several rules may alias to the same surface kind (cpp
            // `function_definition` is the alias value of
            // `inline_method_definition`,
            // `constructor_or_destructor_definition`, …). The parent CHOICE
            // picked ONE of them, but they are indistinguishable to the
            // demand matcher at the parent (each consumes one
            // `function_definition` child). If the picked source's rule
            // cannot place one of THIS child's grandchildren (the desync
            // where the chosen rule lacks `field_initializer_list`) while a
            // sibling alias source can, walk the better source. Keyed on the
            // child's surface kind (the alias `value`).
            let child_kind = vertex_id_kind(schema, child_id).unwrap_or(name);
            let (walk_name, walk_rule) =
                select_walk_rule(schema, grammar, &edges, child_kind, rule, child_id);
            let mut cursor = ChildCursor::new(&edges);
            let old_rule = out.current_rule.take();
            out.current_rule = Some(walk_name.to_owned());
            let result = emit_production(
                protocol,
                schema,
                grammar,
                child_id,
                walk_rule,
                &mut cursor,
                out,
            );
            out.current_rule = old_rule;
            return result;
        }
    }

    // An ALIAS over a CHOICE of bare SYMBOL alternatives (markdown's
    // nested-section slot `ALIAS(CHOICE[_section6 .. _sectionN], "section")`):
    // the alternatives are structurally indistinguishable to the demand
    // matcher (every `_sectionK` is `SEQ[atx_heading, REPEAT(..)]`), so the
    // CHOICE dispatch cannot pick the level tree-sitter chose. The walker
    // recorded that choice as the vertex's `pre-alias-symbol`; resolve it
    // directly to the matching alternative's rule, mirroring the recorded-
    // source preference in `select_walk_rule`. Without this, a deeper heading
    // (`### …` under `## …` under `# …`) is walked through the wrong
    // `_sectionK` whose nested-section slot cannot place the next level, and
    // the remaining headings drop.
    if let Production::Choice { members } = content {
        if let Some(src) = pre_alias_symbol(schema, child_id) {
            let picked = members.iter().find_map(|m| match m {
                Production::Symbol { name } if name.as_str() == src => grammar.rules.get(name),
                _ => None,
            });
            if let Some(rule) = picked {
                let edges = children_for(schema, child_id);
                let mut cursor = ChildCursor::new(&edges);
                let old_rule = out.current_rule.take();
                out.current_rule = Some(src.to_owned());
                let result =
                    emit_production(protocol, schema, grammar, child_id, rule, &mut cursor, out);
                out.current_rule = old_rule;
                return result;
            }
        }
    }

    // Other ALIAS contents (CHOICE, SEQ, literals) walk in place.
    let edges = children_for(schema, child_id);
    let mut cursor = ChildCursor::new(&edges);
    emit_production(
        protocol,
        schema,
        grammar,
        child_id,
        content,
        &mut cursor,
        out,
    )
}

pub(crate) fn emit_in_child_context(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    child_id: &panproto_gat::Name,
    production: &Production,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // The child walks under its own production tree, with its own
    // FIELDs setting their own contexts. Clear the outer FIELD hint
    // so it does not leak through and cause sibling SYMBOLs inside
    // the child's body to mistakenly pull edges from the child's
    // cursor by the parent's field name.
    let _guard = clear_field_context();
    // If `production` is a structural wrapper (CHOICE / SEQ /
    // OPTIONAL / ...) whose referenced symbols cover the child's own
    // kind, the child IS the production's target node and the right
    // emit path is `emit_vertex(child)` (which honours the
    // literal-value leaf shortcut). Without this guard, FIELD(pattern,
    // CHOICE { _pattern, self }) on an identifier child walks the
    // CHOICE on the identifier's empty cursor, falls through to the
    // first non-BLANK alt, and loses the captured identifier text.
    if !matches!(production, Production::Symbol { .. }) {
        let child_kind = schema.vertices.get(child_id).map(|v| v.kind.as_ref());
        let symbols = referenced_symbols(production);
        if symbols
            .iter()
            .any(|s| kind_satisfies_symbol(grammar, child_kind, s) || child_kind == Some(s))
        {
            return emit_vertex(protocol, schema, grammar, child_id, out);
        }
    }
    match production {
        Production::Symbol { .. } => emit_vertex(protocol, schema, grammar, child_id, out),
        _ => {
            let edges = children_for(schema, child_id);
            let mut cursor = ChildCursor::new(&edges);
            emit_production(
                protocol,
                schema,
                grammar,
                child_id,
                production,
                &mut cursor,
                out,
            )
        }
    }
}

/// The canonical default section for a `CHOICE`, used when the dependent-
/// optic review (grammar unification + variant-tag tie-break) does not
/// uniquely determine the alternative — a by-construction schema with no
/// disambiguating signal, or genuine under-determination.
///
/// It dispatches on pure grammar/cursor structure only (no recorded
/// `interstitial`/`chose-alt`/`subtype`-closure heuristics): FIELD-name
/// match against an unconsumed edge, then the categorical
/// CHOICE-with-`BLANK` semantics — a newline-like terminator, else
/// `BLANK` when the cursor is exhausted (ε is correct iff no child
/// remains), else a pure-literal alternative, else the first non-`BLANK`.
fn default_choice<'a>(
    schema: &Schema,
    grammar: &Grammar,
    cursor: &ChildCursor<'_>,
    alternatives: &'a [Production],
) -> Option<&'a Production> {
    let any_unconsumed = cursor
        .edges
        .iter()
        .enumerate()
        .any(|(i, _)| !cursor.consumed[i]);
    let edge_kinds: Vec<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .map(|(_, e)| e.kind.as_ref())
        .collect();
    // The (edge-label, target-kind) pairs of the unconsumed edges, for the
    // `accepts_first_edge` acceptance test below.
    let uc_edge_pairs: Vec<(&str, &str)> = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .filter_map(|(_, e)| {
            schema
                .vertices
                .get(&e.tgt)
                .map(|v| (e.kind.as_ref(), v.kind.as_ref()))
        })
        .collect();

    // FIELD dispatch: an alternative whose FIELD name matches an
    // unconsumed edge kind.
    for alt in alternatives {
        if has_field_in(alt, &edge_kinds) {
            return Some(alt);
        }
    }

    // Prefer a newline-like PATTERN terminator (a structural LineBreak)
    // over a STRING separator (Go's source_file REPEAT terminator).
    if let Some(nl) = alternatives
        .iter()
        .find(|a| matches!(a, Production::Pattern { value } if is_newline_like_pattern(value)))
    {
        return Some(nl);
    }
    if alternatives.iter().any(|a| matches!(a, Production::Blank)) {
        // A hidden-rule alternative resolving to a newline-like PATTERN
        // is preferred over BLANK (Julia's `_terminator`).
        for alt in alternatives {
            if let Production::Symbol { name } = alt {
                if name.starts_with('_') {
                    if let Some(rule) = grammar.rules.get(name) {
                        if contains_newline_pattern(rule) {
                            return Some(alt);
                        }
                    }
                }
            }
        }
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    // A pure-literal alternative (only STRINGs/PATTERNs, no SYMBOL/ALIAS)
    // always materializes its bytes without consuming a child. Prefer it
    // over a symbol-bearing alternative that would emit nothing here:
    //   * the cursor is exhausted (`!any_unconsumed`), so no symbol alt can
    //     consume anything; or
    //   * children remain, but NO alternative can `accept` any of the
    //     unconsumed edges — the symbol alts are all inapplicable at this
    //     position, so falling to the first non-BLANK (a symbol) drops the
    //     structural literal. This is csharp's
    //     `REPEAT1[CHOICE[explicit_interface_specifier, "operator",
    //     "checked"]]` in a conversion operator: the `type`/`parameters`/
    //     `body` edges match no alt, so the bare `operator` keyword must
    //     materialize rather than the unmatchable `explicit_interface_specifier`
    //     symbol emitting nothing (which drops `operator` and breaks the parse).
    let no_alt_accepts_remaining = any_unconsumed
        && !alternatives.iter().any(|alt| {
            uc_edge_pairs
                .iter()
                .any(|&(ek, tk)| accepts_first_edge(grammar, alt, ek, tk))
        });
    if !any_unconsumed || no_alt_accepts_remaining {
        // Unmarked-base preference for a string-prefix CHOICE. When every
        // alternative is a bare literal STRING and exactly one is a strict
        // SUFFIX of all the others, that suffix is the unmarked base form
        // and the others are prefixed variants (C/C++ string/char opener
        // `CHOICE["L\"","u\"","U\"","u8\"","\""]`: `"` is the suffix of every
        // other). `forget_layout` strips which prefix the source used, so
        // the canonical section must default to the base `"` — picking the
        // first alt (`L"`) instead corrupts EVERY string literal into a
        // wide-char `L"…"`. The byte-faithful replay path is unaffected: it
        // recovers the real prefix from the recorded `ptrace`. Narrow by
        // construction (all-STRING, a unique strict suffix), so it cannot
        // fire for an ordinary literal CHOICE whose alternatives are
        // unrelated tokens.
        if let Some(base) = unmarked_base_literal(alternatives) {
            return Some(base);
        }
        if let Some(pure_lit) = alternatives
            .iter()
            .find(|alt| referenced_symbols(alt).is_empty() && !matches!(alt, Production::Blank))
        {
            return Some(pure_lit);
        }
    }
    alternatives
        .iter()
        .find(|alt| !matches!(alt, Production::Blank))
}

/// When the `alternatives` of a string-opener CHOICE all reduce to a
/// trailing constant delimiter and exactly one bare `STRING` alternative is
/// a strict suffix of every other, return that `STRING` (the unmarked base
/// form). The prefixed variants may be either bare literal `STRING`s
/// (C/C++/Obj-C opener `["L\"","u\"","U\"","u8\"","\""]`: `"` is the suffix
/// of each) OR a prefix `PATTERN` whose only variation is a leading
/// character class over a constant tail (PHP / SQL binary-string opener
/// `[PATTERN "[bB]'", STRING "'"]`: the `[bB]` prefix precedes the same `'`
/// the base `STRING` carries). `forget_layout` strips which prefix the
/// source used, so the canonical section must default to the base `'`/`"` —
/// picking the first alt (`[bB]'` rendered through the placeholder, or `L"`)
/// otherwise corrupts EVERY such literal. The byte-faithful replay path is
/// unaffected: it recovers the real prefix from the recorded `ptrace`.
/// Narrow by construction (a unique bare-STRING strict suffix of every
/// alternative's trailing literal), so it cannot fire for an ordinary
/// literal CHOICE whose alternatives are unrelated tokens.
fn unmarked_base_literal(alternatives: &[Production]) -> Option<&Production> {
    // Each alternative's trailing constant literal: a bare STRING is itself;
    // a prefix-PATTERN (`[class]<lit>`) contributes its constant tail. Any
    // other shape disqualifies the whole CHOICE (returns None).
    let tails: Vec<(&Production, String)> = alternatives
        .iter()
        .map(|a| match a {
            Production::String { value } => Some((a, value.clone())),
            Production::Pattern { value } => pattern_trailing_literal(value).map(|tail| (a, tail)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if tails.len() < 2 {
        return None;
    }
    let mut base: Option<&Production> = None;
    for &(prod, ref val) in &tails {
        // Only a bare STRING can be the base form (a PATTERN renders through
        // the placeholder and is never the canonical unmarked spelling).
        if !matches!(prod, Production::String { .. }) {
            continue;
        }
        // `val` is the base iff every OTHER alternative's trailing literal
        // ends with it and is strictly longer (a proper suffix, not a twin).
        let is_base = tails
            .iter()
            .all(|(_, other)| other == val || (other.len() > val.len() && other.ends_with(val)));
        if is_base {
            if base.is_some() {
                return None; // Not unique.
            }
            base = Some(prod);
        }
    }
    base
}

/// The constant trailing literal of a `PATTERN` shaped `[<char-class>]<lit>`:
/// a leading bracketed character class (optionally quantified) followed by a
/// constant byte sequence with no further regex metacharacters. PHP's
/// binary-string opener `[bB]'` yields `'`; SQL's `[xX]'` yields `'`.
/// `None` for any pattern that is not a leading-class-then-literal shape.
fn pattern_trailing_literal(value: &str) -> Option<String> {
    let rest = value.strip_prefix('[')?;
    let end = rest.find(']')?;
    let after = &rest[end + 1..];
    // Skip an optional quantifier on the class.
    let after = after
        .strip_prefix('*')
        .or_else(|| after.strip_prefix('+'))
        .or_else(|| after.strip_prefix('?'))
        .unwrap_or(after);
    if after.is_empty() {
        return None;
    }
    // The remainder must be a clean constant literal (no further metachars).
    crate::emit_pretty::helpers::decode_simple_pattern_literal(after)
}

/// Whether the trailing REPEAT-body slot `prod` is an OPTIONAL statement
/// NEWLINE separator: a `CHOICE` containing `BLANK` (or an `OPTIONAL`) whose
/// non-BLANK alternative is a newline-classified external scanner token
/// (`_automatic_separator`, `*_newline`, `*_line_ending`) or a newline-like
/// pattern. Such a separator being absent (BLANK) merges adjacent statements,
/// so the REPEAT walker forces a newline between iterations to keep the
/// statement boundary (V `*ap = size` after `&u64(a)`).
fn seq_trailing_newline_separator(grammar: &Grammar, prod: &Production) -> bool {
    fn alt_is_newline(grammar: &Grammar, p: &Production) -> bool {
        match p {
            Production::Symbol { name } => {
                grammar.external_newlines.contains(name)
                    || (name.starts_with('_')
                        && !grammar.rules.contains_key(name)
                        && (name.contains("newline") || name.contains("line_ending")))
            }
            Production::Pattern { value } => is_newline_like_pattern(value),
            Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => alt_is_newline(grammar, content),
            _ => false,
        }
    }
    match prod {
        Production::Choice { members } => members.iter().any(|m| alt_is_newline(grammar, m)),
        Production::Optional { content } => alt_is_newline(grammar, content),
        _ => false,
    }
}

/// Whether the REPEAT-body separator slot `prod` (a `CHOICE` containing
/// `BLANK`, or an `OPTIONAL`) offers an explicit statement / list separator
/// token as its non-BLANK alternative: a `;` / `,` literal, or a newline-like
/// pattern. Such a separator being absent (BLANK chosen) means the iterations
/// were whitespace/newline separated in source, NOT glued — so the REPEAT
/// walker must NOT force them tight (which would fuse adjacent value tokens).
/// A separator slot with NO such literal (pure juxtaposition) keeps the tight
/// default.
fn choice_offers_separator_literal(prod: &Production) -> bool {
    fn alt_is_separator(p: &Production) -> bool {
        match p {
            Production::String { value } => value == ";" || value == ",",
            Production::Pattern { value } => is_newline_like_pattern(value),
            Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => alt_is_separator(content),
            _ => false,
        }
    }
    match prod {
        Production::Choice { members } => members.iter().any(alt_is_separator),
        Production::Optional { content } => alt_is_separator(content),
        _ => false,
    }
}

/// The `CHOICE` alternatives slice at the heart of a rule body, unwrapping
/// transparent precedence / token wrappers. Returns an empty slice when the
/// body is not a `CHOICE`. Used to test (by pointer identity) whether a
/// dispatched `CHOICE`'s `alternatives` slice IS the body of `current_rule`.
fn unwrap_choice_body(prod: &Production) -> &[Production] {
    match prod {
        Production::Choice { members } => members,
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Reserved { content, .. } => unwrap_choice_body(content),
        _ => &[],
    }
}

pub(crate) fn pick_choice_with_cursor<'a>(
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    cursor: &ChildCursor<'_>,
    alternatives: &'a [Production],
    current_rule: Option<&str>,
) -> Option<&'a Production> {
    // ── Canonical-section CHOICE dispatch (primary) ──────────────────
    // Grammar-unification: pick the alternative whose yield structurally
    // admits the vertex's unconsumed child edges, with NO parse trace.
    // This is the total semantics for by-construction / transpiled
    // schemas (the dominant case); the positional/fingerprint heuristics
    // below are the fallback for genuine under-determination (a tie or
    // no structural match) and are subsumed once trace-replay lands.
    // `demand` is the ordered kinds of the unconsumed child edges; `labels`
    // is their parallel field-name labels (`child_of` when not field-bound).
    // Built together so they stay index-aligned through the same filter.
    let (demand, labels): (Vec<&str>, Vec<&str>) = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .filter_map(|(_, e)| {
            schema
                .vertices
                .get(&e.tgt)
                .map(|v| (v.kind.as_ref(), e.kind.as_ref()))
        })
        .unzip();
    // The recorded literal-token fibre is the variant tag that
    // disambiguates CHOICEs the child demand alone ties on. Two sources,
    // both surviving as constraints:
    //   * `ptrace-<n> = T<text>`: anonymous grammar tokens in source
    //     order (kotlin return/throw). Stripped by forget_layout.
    //   * `field:<name> = <text>`: field-bound anonymous tokens, e.g.
    //     python `field('operator', '+')` distinguishing the
    //     binary_operator alternatives. NOT a layout sort, so it
    //     survives forget_layout — this is what lets the canonical
    //     section pick the right operator for a transpiled vertex that
    //     carries no trace.
    // Together they are the literal component of the variant tag the
    // review consumes rather than re-deriving.
    // Field names bound anywhere in this CHOICE's alternatives. A
    // `field:<name>` trace token may only disambiguate the CHOICE when some
    // alternative actually binds `<name>`; otherwise the recorded value
    // leaks into an unrelated literal CHOICE that merely shares the text
    // (bash `_statements`' trailing `_terminator` CH[';'|';;'|…] picking up
    // a sibling `case_item`'s `field:termination=";;"`, emitting a spurious
    // second `;;`). `ptrace-` tokens are positional/anonymous and stay
    // unscoped — they are matched by literal as before.
    // A CHOICE that is the direct content of a FIELD inherits that field
    // context. The field name `<f>` is the variant tag scope for this CHOICE
    // even when the alternatives are bare literals that do not themselves
    // re-declare the FIELD (`field('operator', CHOICE['!','~','-','+'])`):
    // the enclosing FIELD proves the CHOICE binds `<f>`, so `field:<f>` is a
    // legitimate disambiguator (C unary/update operator selection).
    let field_ctx = current_field_context();
    let mut alt_field_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for alt in alternatives {
        collect_field_names(alt, &mut alt_field_names);
    }
    let mut trace_tokens: Vec<String> = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            cs.iter()
                .filter_map(|c| {
                    let s = c.sort.as_ref();
                    if s.starts_with("ptrace-") {
                        c.value.strip_prefix('T').map(ToOwned::to_owned)
                    } else if let Some(field) = s.strip_prefix("field:") {
                        (alt_field_names.contains(field) || field_ctx.as_deref() == Some(field))
                            .then(|| c.value.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    // Per-vertex `ptrace`-literal budget: drop any recorded literal whose
    // emission budget is already spent, so the variant-tag tie-break below
    // cannot re-satisfy the SAME separator at a later CHOICE site. bash
    // `case_item` records one `;;` (as `ptrace` and, for a non-last item, a
    // redundant `field:termination`), yet its walk visits three separator
    // CHOICEs; once the first has claimed the `;;`, the rest fall through to
    // their newline / BLANK alternative. Vertices with no `ptrace` fibre carry
    // no budget, so this is a no-op on canonical / by-construction schemas.
    trace_tokens.retain(|t| ptrace_budget_allows(t));
    // The recorded `field:<name>=<value>` pairs. An alternative that binds
    // field <name> to a literal set excluding <value> contradicts the parse
    // and is rejected before maximal-munch (JS `for (const x of …)`: the
    // `field('kind','var')` member must not out-consume the `let|const`
    // member via its optional `= expr` swallowing the `right` operand).
    let field_constraints: Vec<(&str, &str)> = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            cs.iter()
                .filter_map(|c| {
                    c.sort
                        .as_ref()
                        .strip_prefix("field:")
                        .map(|name| (name, c.value.as_str()))
                })
                .collect()
        })
        .unwrap_or_default();
    // (`field_ctx` is consumed below; it also lets a field-labeled child be
    // consumed by the field's own production, go `F:body(CH[block | BLANK])`.)
    // `self_rule` is the hidden dispatch rule whose body IS this very CHOICE
    // (`current_rule` set by the hidden-rule μ-frame expansion). A REPEAT body
    // that is such a hidden CHOICE-rule iterates ONE child per iteration, so an
    // alternative that out-munches by RE-ENTERING `self_rule` recursively must
    // not beat the direct single-child match (the cmake `_untrimmed_argument`
    // /`_paren_argument` self-recursion). Threaded so the unification can
    // distinguish that from a legitimate longer alternative (go grouped
    // `const_declaration`, whose CHOICE is nested in a named SEQ, not a hidden
    // rule body, so `self_rule` is None and the longer alt wins normally).
    let self_rule = current_rule.filter(|r| {
        grammar
            .rules
            .get(*r)
            .is_some_and(|body| std::ptr::eq(unwrap_choice_body(body), alternatives))
    });
    // Positional layout slice FROM the current cursor position forward
    // (interstitial-k is the gap before the k-th named child). Computed here
    // so the unification's optional-separator tie-break can subtract a
    // separator literal that the parser emitted upstream (a REPEAT separator)
    // and that no longer occurs in the remaining layout — its presence in the
    // UNORDERED trace would otherwise falsely satisfy a trailing
    // `CHOICE[sep, BLANK]` optional slot. Re-used by the positional
    // discriminator below.
    let consumed_count = cursor.consumed.iter().filter(|&&c| c).count();
    let positional_interstitials: Vec<&str> = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            let mut indexed: Vec<(usize, &str)> = cs
                .iter()
                .filter_map(|c| {
                    let s = c.sort.as_ref();
                    if !s.starts_with("interstitial-") || s.ends_with("-start-byte") {
                        return None;
                    }
                    let idx: usize = s["interstitial-".len()..].parse().ok()?;
                    Some((idx, c.value.as_str()))
                })
                .collect();
            indexed.sort_by_key(|&(i, _)| i);
            indexed.into_iter().map(|(_, v)| v).collect()
        })
        .unwrap_or_default();
    let positional_slice: String = if positional_interstitials.is_empty() {
        String::new()
    } else {
        positional_interstitials
            .iter()
            .skip(consumed_count)
            .copied()
            .collect::<Vec<&str>>()
            .join(" ")
    };
    // ASI (automatic-semicolon) resolution, replay-only. A statement
    // terminator CHOICE pairs a scanner-inserted terminator with a literal
    // one: `_semicolon = CHOICE[_automatic_semicolon, ";"]` (php/js/perl). The
    // scanner alt (`_automatic_semicolon`, classified into `external_newlines`)
    // fires zero-width where the source ended the statement with a construct
    // boundary (a `?>` close tag, end-of-line, `}`) rather than a written `;`;
    // it emits a layout newline, NOT a character. The literal `;` alt
    // materializes a real `;`. With no recorded trace the dispatch falls
    // through to `default_choice`, whose pure-literal preference picks `;` and
    // FABRICATES a terminator the source lacked — corrupting `<?=$url?>` into
    // `<?=$url;?>`. When the parent vertex carries a complement (a `ptrace`, so
    // this is the replay path) that records NO occurrence of the literal
    // terminator (neither as a `T<sep>` trace token nor in the remaining
    // positional layout), the source used the scanner terminator: prefer the
    // ASI alt. Gated on ptrace presence so canonical / by-construction schemas
    // (which carry no evidence either way) keep the prior default.
    let has_ptrace = schema
        .constraints
        .get(vertex_id)
        .is_some_and(|cs| cs.iter().any(|c| c.sort.as_ref().starts_with("ptrace-")));
    if has_ptrace {
        let asi_alt = alternatives.iter().position(|a| {
            matches!(a, Production::Symbol { name }
                if grammar.external_newlines.contains(name))
        });
        if let Some(asi_idx) = asi_alt {
            // The literal terminator the ASI alt stands in for (`;`).
            let literal_term: Option<&str> = alternatives
                .iter()
                .filter_map(unwrap_to_string)
                .find(|s| *s == ";");
            if let Some(term) = literal_term {
                let in_trace = trace_tokens.iter().any(|t| t == term);
                let in_layout = positional_slice.contains(term);
                if !in_trace && !in_layout {
                    return Some(&alternatives[asi_idx]);
                }
            }
        }
    }
    if let Some(idx) = super::select_choice_with_trace(
        grammar,
        alternatives,
        &demand,
        &labels,
        field_ctx.as_deref(),
        &field_constraints,
        &trace_tokens,
        self_rule,
        &positional_slice,
    ) {
        return Some(&alternatives[idx]);
    }

    // Positional discriminator: use the interstitials FROM the
    // current cursor position forward. Interstitials are indexed by
    // their gap position (interstitial-k is the gap before the k-th
    // named child); the slice from `consumed_count` onward captures
    // exactly the text the remaining CHOICE branches must consume.
    // This eliminates the cross-position contamination of the prior
    // flat blob (where a trailing-CHOICE-with-BLANK saw all the
    // commas separating earlier REPEAT iterations and wrongly
    // preferred the comma alt).
    //
    // The chose-alt-fingerprint (a single string joined from every
    // non-empty interstitial trimmed) is retained as a fallback for
    // by-construction schemas with no positional interstitials; it
    // is strictly less precise than positional matching.
    let fingerprint_blob = schema
        .constraints
        .get(vertex_id)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.sort.as_ref() == "chose-alt-fingerprint")
                .map(|c| c.value.clone())
        })
        .unwrap_or_default();
    let constraint_blob: String = if positional_slice.is_empty() {
        fingerprint_blob
    } else {
        positional_slice
    };
    let child_kinds: Vec<&str> = schema
        .constraints
        .get(vertex_id)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.sort.as_ref() == "chose-alt-child-kinds")
                .map(|c| c.value.split_whitespace().collect())
        })
        .unwrap_or_default();
    // Concrete-named-witness guard. The subtype closure `subtypes[K]` is a
    // deep-reachability relation: it admits a SYMBOL `S` for target kind `K`
    // whenever `S`'s rule body can *eventually* reach `K`, even through an
    // intervening concrete node. That over-admits an optional alternative
    // that wraps `K` in its own node (D's `declarator` CHOICE picks
    // `template_parameters` for an `int_literal` because a template value
    // argument can be an `int_literal`), stealing the edge from a later
    // mandatory member and dropping it.
    //
    // The `chose-alt-child-kinds` witness records the actual named children
    // the parser produced. A rule that carries its *own* literal tokens
    // (brackets / keywords, e.g. D's `template_parameters = SEQ["(", …, ")"]`)
    // always materialises as a node under its own name when matched, so it
    // would appear in that witness if it had really been taken. When the
    // witness is present and such a self-anchored, concrete-named symbol is
    // absent from it, that alternative was not taken: skip it.
    //
    // The guard is deliberately narrow:
    //   - hidden `_`-rules and declared supertypes dispatch *through* to
    //     their target and never appear under their own name (exempt);
    //   - a pure-symbol wrapper rule with no literal tokens of its own
    //     (e.g. Julia's `macro_argument_list = REPEAT1(_block_form)`) can be
    //     inlined transparently and materialise under a *different* kind
    //     (`argument_list`), so it too is exempt — requiring an own literal
    //     token avoids skipping it.
    let concrete_named_absent = |sym: &str| -> bool {
        !child_kinds.is_empty()
            && !sym.starts_with('_')
            && !grammar.supertypes.contains(sym)
            && grammar
                .rules
                .get(sym)
                .is_some_and(|r| !literal_strings(r).is_empty())
            && !child_kinds.contains(&sym)
    };
    // Same-kind alias disambiguation. When a CHOICE alt is
    // `ALIAS{value: K, content: SYMBOL S}` and another rule also surfaces as
    // kind K, the aliased source S is the right one only if the child's
    // recorded operator witness contains one of S's own keyword literals
    // (Ruby aliases `command_binary` {and,or} to `binary`, but arithmetic
    // `binary` {+,-,…} surfaces as `binary` too). Empty source literals or a
    // missing witness do not filter.
    let alias_source_ok = |content: &Production, value: &str| -> bool {
        let src_lits = aliased_source_literals(grammar, content);
        if src_lits.is_empty() {
            return true;
        }
        match first_unconsumed_target_fingerprint(schema, cursor, value) {
            None => true,
            Some(b) => src_lits.iter().any(|l| b.contains(l.as_str())),
        }
    };
    // Cursor-exhaustion BLANK-preference: when all cursor edges have
    // been consumed AND `BLANK` is one of the alternatives, the only
    // alt that won't introduce a non-existent child is `BLANK`.
    //
    // This gate fires before the literal-blob discriminator because
    // the fingerprint is shared across every CHOICE position in the
    // vertex's rule body: a vertex like `sample_step` that ends in
    // `..., REPEAT(SEQ(",", arg)), CHOICE(",", BLANK)` records all of
    // its `","` interstitials in a single blob, so the literal-score
    // matcher would otherwise prefer `","` for the trailing CHOICE
    // even when the source had no trailing comma. By the time the
    // emitter reaches the trailing CHOICE, the REPEAT has consumed
    // every arg edge in cursor order; the residual unconsumed multiset
    // is empty; and the categorical reading of a CHOICE-with-BLANK at
    // a position with no remaining children is the no-op alternative.
    let any_unconsumed = cursor
        .edges
        .iter()
        .enumerate()
        .any(|(i, _)| !cursor.consumed[i]);
    let blank_present = alternatives.iter().any(|a| matches!(a, Production::Blank));
    let edge_kinds: Vec<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .map(|(_, e)| e.kind.as_ref())
        .collect();
    if !any_unconsumed && blank_present {
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    if !any_unconsumed && !blank_present {
        // When the cursor is exhausted: first prefer a newline-like
        // PATTERN over STRING separators (e.g. Go source_file terminator
        // CHOICE[PATTERN("\n"), ";", "\0"] should emit newline not ";").
        for alt in alternatives {
            if let Production::Pattern { value } = alt {
                if is_newline_like_pattern(value) {
                    return Some(alt);
                }
            }
        }
        // Unattested-terminator preference. A trailing two-way CHOICE that
        // pairs an anonymous single-character `IMMEDIATE_TOKEN` sentinel with a
        // visible-rule blank-line FIELD alternative (vimdoc `block`'s closing
        // `CHOICE[IMMEDIATE_TOKEN("<") | _blank]`, where `_blank` is the
        // line-ending `FIELD(blank, PATTERN("\n"))`) is an OPTIONAL closer: the
        // source either wrote the literal sentinel or simply ended the
        // construct with a blank line. With the cursor exhausted, the literal
        // alternative is the parsed one ONLY when the recorded variant tag
        // attests it (a `ptrace` `T<lit>`). Absent that attestation the closer
        // was the blank line, so the pure-literal preference below would
        // fabricate the sentinel (`>\n  code\n` -> `>\n  code\n<\n`). When no
        // trace token carries the literal, prefer the blank-line alt. The
        // replay path is unaffected: a source that wrote the sentinel records
        // its `T<lit>` and keeps the literal.
        //
        // The blank-line alternative is required to resolve to a SYMBOL whose
        // grammar rule body is a `FIELD`/`PATTERN` matching ONLY a newline (an
        // in-grammar blank line), NOT an external scanner token. This is the
        // discriminator that excludes the automatic-semicolon terminator
        // `_semicolon = CHOICE[_automatic_semicolon, ";"]`: there the newline
        // alt is the EXTERNAL `_automatic_semicolon`, and defaulting to it on
        // the canonical path drops the `;` that statements genuinely need (the
        // §WAVE-5-B js 108->97 regression). The `_blank` field is a real
        // grammar rule, so it passes; `_automatic_semicolon` is external, so it
        // does not. The literal alt is restricted to a single-char
        // `IMMEDIATE_TOKEN` so an ordinary keyword/separator CHOICE is
        // untouched.
        if alternatives.len() == 2 {
            let lit_alt = alternatives.iter().find(|a| {
                matches!(a, Production::ImmediateToken { .. })
                    && referenced_symbols(a).is_empty()
                    && matches!(
                        literal_strings(a).as_slice(),
                        [s] if s.chars().count() == 1
                    )
            });
            let blank_alt = alternatives.iter().find(|a| {
                matches!(a, Production::Symbol { name }
                    if grammar.rules.get(name).is_some_and(is_blank_line_rule))
            });
            if let (Some(lit), Some(blank)) = (lit_alt, blank_alt) {
                let lit_attested = literal_strings(lit)
                    .iter()
                    .any(|s| trace_tokens.iter().any(|t| t == s));
                if !lit_attested {
                    return Some(blank);
                }
            }
        }
        // Then prefer a pure-literal alternative (only STRINGs, no
        // SYMBOLs/FIELDs) over one that merely CAN produce epsilon.
        // A pure-literal alternative emits concrete tokens without
        // needing children (e.g. ";" terminator in Rust struct_item).
        if let Some(pure_lit) = alternatives.iter().find(|alt| {
            let syms = referenced_symbols(alt);
            let strings = literal_strings(alt);
            syms.is_empty() && !strings.is_empty()
        }) {
            return Some(pure_lit);
        }
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        for alt in alternatives {
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            if ys.contains("") {
                return Some(alt);
            }
            visited.clear();
        }
    }

    // Literal match: when a cursor edge's target vertex kind or
    // literal-value matches a STRING alternative exactly, pick that
    // alternative. Handles grammars like Go's binary_expression where
    // operators are anonymous named children (kind IS the operator text
    // like ">") and the CHOICE is over STRING operators.
    for edge_idx in 0..cursor.edges.len() {
        if cursor.consumed[edge_idx] {
            continue;
        }
        let edge = &cursor.edges[edge_idx];
        let tgt_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
        let tgt_lit = literal_value(schema, &edge.tgt);
        for alt in alternatives {
            if let Production::String { value } = alt {
                if Some(value.as_str()) == tgt_kind || tgt_lit == Some(value.as_str()) {
                    return Some(alt);
                }
            }
        }
    }

    // Pure-separator newline preference: a CHOICE whose alternatives all
    // emit no named child (a statement separator / terminator, e.g.
    // Odin's `_separator = CHOICE[_newline, ";"]`) is decided by which
    // punctuation to emit, not by a cursor edge. When a separator is
    // genuinely needed (the cursor-exhaustion gates above already handled
    // the empty case) and a newline alternative exists, prefer it: it is
    // the canonical separator and is immune to a fingerprint contaminated
    // by a `;` elsewhere in the vertex (which would otherwise flip the
    // choice to `;` only on the re-emit, breaking the fixed point).
    if any_unconsumed {
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        let all_non_consuming = alternatives.iter().all(|alt| {
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            visited.clear();
            ys.is_empty() || (ys.len() == 1 && ys.contains(""))
        });
        if all_non_consuming {
            if let Some(nl) = alternatives.iter().find(|a| is_newline_alt(grammar, a)) {
                return Some(nl);
            }
        }
    }

    if !constraint_blob.is_empty() {
        // Categorical filter: when the cursor has an unconsumed first
        // edge, an alt should only be considered if it can consume
        // that edge — OR no alt in the CHOICE can. Acceptance is the
        // inductive predicate `accepts_first_edge`: it fuses FIELD-name
        // matching with content-yield admission and SYMBOL subtype
        // dispatch into one rule.
        let first_uc_edge_pre = cursor
            .edges
            .iter()
            .enumerate()
            .find(|(i, _)| !cursor.consumed[*i])
            .map(|(_, e)| e);
        let alt_accepts = |a: &Production| -> bool {
            let Some(edge) = first_uc_edge_pre else {
                return false;
            };
            let edge_kind = edge.kind.as_ref();
            let Some(tgt_kind) = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref()) else {
                return false;
            };
            accepts_first_edge(grammar, a, edge_kind, tgt_kind)
        };
        let any_consumes = any_unconsumed && alternatives.iter().any(alt_accepts);

        // Primary score: literal-token match length. This dominates
        // alt selection so existing language tests that depend on
        // literal-only fingerprints keep working.
        // Secondary score (tiebreaker only): named-symbol kind match
        // count, read from the separate `chose-alt-child-kinds`
        // constraint (kept apart from the literal fingerprint so
        // identifiers like `:` in the kind list don't contaminate the
        // literal match). An alt that matches the recorded kinds is a
        // stronger witness than one whose only
        // overlap is literal punctuation.
        let mut best_literal: usize = 0;
        let mut best_symbols: usize = 0;
        let mut best_total_chars: usize = usize::MAX;
        let mut best_alt: Option<&Production> = None;
        let mut tied = false;
        for alt in alternatives {
            let strings = literal_strings(alt);
            if strings.is_empty() {
                continue;
            }
            // Categorical filter: skip alts that can't consume the
            // first unconsumed edge when SOME alt can.
            if any_consumes && !alt_accepts(alt) {
                continue;
            }
            let literal_score = strings
                .iter()
                .filter(|s| constraint_blob.contains(s.as_str()))
                .map(String::len)
                .sum::<usize>();
            if literal_score == 0 {
                continue;
            }
            let total_chars: usize = strings.iter().map(String::len).sum();
            let symbol_score = if literal_score >= best_literal && !child_kinds.is_empty() {
                let symbols = referenced_symbols(alt);
                symbols
                    .iter()
                    .filter(|sym| {
                        let sym_str: &str = sym;
                        if child_kinds.contains(&sym_str) {
                            return true;
                        }
                        grammar.subtypes.get(sym_str).is_some_and(|sub_set| {
                            sub_set
                                .iter()
                                .any(|sub| child_kinds.contains(&sub.as_str()))
                        })
                    })
                    .count()
            } else {
                0
            };
            let better = literal_score > best_literal
                || (literal_score == best_literal && symbol_score > best_symbols)
                || (literal_score == best_literal
                    && symbol_score == best_symbols
                    && total_chars < best_total_chars);
            let same = literal_score == best_literal
                && symbol_score == best_symbols
                && total_chars == best_total_chars;
            if better {
                best_literal = literal_score;
                best_symbols = symbol_score;
                best_total_chars = total_chars;
                best_alt = Some(alt);
                tied = false;
            } else if same && best_alt.is_some() {
                tied = true;
            }
        }
        if let Some(alt) = best_alt {
            if !tied {
                if any_unconsumed {
                    if alt_accepts(alt) {
                        return Some(alt);
                    }
                    // The best literal-score alt can't consume the
                    // first unconsumed cursor edge. Three sub-cases:
                    //  (a) No BLANK alternative: blob is the only
                    //      signal; return best_alt.
                    //  (b) BLANK present AND best_alt is pure-literal
                    //      (no referenced SYMBOLs): emitting best_alt
                    //      adds the matched literals and consumes no
                    //      child; the unconsumed cursor edge is for a
                    //      later SEQ position anyway. Return best_alt
                    //      (BUGS `model_block`: CHOICE[CHOICE["model",
                    //      "data"], BLANK] picks the literal because
                    //      the blob recorded it).
                    //  (c) BLANK present AND best_alt has SYMBOLs:
                    //      emitting best_alt would walk SYMBOLs that
                    //      can't be satisfied (they consume no edge,
                    //      a downstream SYMBOL would silently fail).
                    //      Fall through to final selection of BLANK
                    //      (Java `formal_parameters` inner CHOICE
                    //      [SEQ[receiver, ","], BLANK] with a
                    //      formal_parameter edge: pick BLANK).
                    if !blank_present || referenced_symbols(alt).is_empty() {
                        return Some(alt);
                    }
                } else {
                    return Some(alt);
                }
            }
        }
    }

    // Cursor-driven dispatch via Yield-set preimage.
    //
    // For a CHOICE C = A1 | ... | An, Yield(Ai) is the set of vertex
    // kinds that can appear as the first named child when Ai is taken
    // (see `yield_of_production`). Given the first unconsumed cursor
    // edge with target kind K, select the first Ai (grammar order)
    // where K ∈ Yield(Ai). This is deterministic: grammar order is
    // the tiebreak, matching tree-sitter's own disambiguation.
    let first_unconsumed_kind: Option<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .find(|(i, _)| !cursor.consumed[*i])
        .and_then(|(_, edge)| schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref()));
    if let Some(target_kind) = first_unconsumed_kind {
        // The subtype closure `subtypes[target_kind]` contains every
        // symbol name S such that a vertex of kind `target_kind` can
        // appear where the grammar says `SYMBOL S`. For a CHOICE
        // C = A1 | ... | An, the correct alternative is the one whose
        // top-level symbol is in `subtypes[target_kind]` (the target
        // kind IS a subtype of that symbol, so the symbol's rule body
        // dispatches to the target kind at parse time). This is an
        // O(1) set-membership check per alternative — no recursive
        // Yield computation needed.
        //
        // Preference order:
        //   1. Direct name match (target_kind == symbol name)
        //   2. Subtype match (symbol name ∈ subtypes[target_kind])
        //   3. Yield-set match (target_kind ∈ Yield(alt)) as fallback
        //      for non-SYMBOL alternatives (ALIAS, SEQ, etc.)
        let target_supers = grammar.subtypes.get(target_kind);

        // Indented-form preference: when multiple alternatives match
        // the target kind (e.g. Python _suite where all three alts
        // produce `block`), prefer the alternative containing an
        // `_indent` SYMBOL. Check this BEFORE the standard passes
        // since they would pick the first match in grammar order.
        {
            let mut match_count = 0usize;
            let mut indent_alt_idx: Option<usize> = None;
            let mut visited = std::collections::HashSet::new();
            let mut yield_cache = grammar.yield_sets.clone();
            for (i, alt) in alternatives.iter().enumerate() {
                let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
                if ys.contains(target_kind) {
                    match_count += 1;
                    if indent_alt_idx.is_none()
                        && referenced_symbols(alt)
                            .iter()
                            .any(|s| grammar.external_indent_opens.contains(*s))
                    {
                        indent_alt_idx = Some(i);
                    }
                }
                visited.clear();
            }
            if match_count > 1 {
                if let Some(idx) = indent_alt_idx {
                    return Some(&alternatives[idx]);
                }
            }
        }

        // Pass 1: direct name match
        for alt in alternatives {
            if let Production::Symbol { name } = alt {
                if name.as_str() == target_kind {
                    return Some(alt);
                }
            }
            if let Production::Alias {
                named: true,
                value,
                content,
            } = alt
            {
                if value.as_str() == target_kind && alias_source_ok(content, value) {
                    return Some(alt);
                }
            }
        }

        // Pass 2: subtype match (the target kind's supertype set
        // tells us which SYMBOL names it satisfies)
        if let Some(supers) = target_supers {
            for alt in alternatives {
                if let Production::Symbol { name } = alt {
                    if supers.contains(name.as_str()) && !concrete_named_absent(name) {
                        return Some(alt);
                    }
                }
                if let Production::Alias {
                    named: true,
                    value,
                    content,
                } = alt
                {
                    if supers.contains(value.as_str())
                        && !concrete_named_absent(value)
                        && alias_source_ok(content, value)
                    {
                        return Some(alt);
                    }
                }
            }
        }

        // Pass 3: Yield-set fallback for alternatives that are not
        // plain SYMBOLs or named ALIASes (e.g. SEQ, PREC wrappers
        // around SYMBOLs that the above passes don't unwrap).
        // Guard: skip alternatives whose FIELDs don't match any
        // unconsumed edge kind. A FIELD that can't be satisfied
        // would consume the wrong child, and the alternative is
        // structurally wrong for the current cursor state.
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        let mut matching_alts: Vec<&Production> = Vec::new();
        for alt in alternatives {
            // Skip an alt only when it is FORCED to bind a field that no
            // unconsumed edge kind matches. An *optional* field (behind
            // OPTIONAL / REPEAT / a CHOICE-with-BLANK) can simply be left
            // unbound, so it must not disqualify the alt — otherwise a
            // SEQ like bash `_expansion_body`'s `[opt field(operator,'!'),
            // variable_name, …]` is wrongly rejected for a non-field-bound
            // `variable_name` edge (labelled `child_of`), and the dispatch
            // falls through to a structurally-impossible default.
            let mandatory_fields = mandatory_field_names(alt);
            if !mandatory_fields.is_empty()
                && !mandatory_fields.iter().any(|f| edge_kinds.contains(f))
            {
                visited.clear();
                continue;
            }
            // Token-set restriction: when a FIELD's body is an
            // ALIAS{CHOICE[STRING...]}, the field admits only those
            // literal values. An alt whose token-restricted FIELDs
            // can't accept the cursor's edge for that field is
            // structurally invalid (e.g. Go `call_expression` alt 0
            // has `function: ALIAS{CHOICE["new","make"], ...}` and
            // is only valid when the function child's literal is
            // "new" or "make").
            if !alt_satisfies_field_token_restrictions(schema, cursor, alt) {
                visited.clear();
                continue;
            }
            // Alias-source discriminator: if the cursor has a
            // field-named edge whose `pre-alias-symbol` was recorded by
            // the walker, the alt's FIELD body (when it's a named
            // ALIAS over a SYMBOL) must reference that same source
            // symbol. This is the exact tree-sitter-derived signal for
            // ALIAS dispatch when literal-value restriction does not
            // apply.
            if !alt_satisfies_pre_alias_constraints(schema, cursor, alt) {
                visited.clear();
                continue;
            }
            // Concrete-named-witness guard (see `concrete_named_absent`):
            // an optional alt that wraps the target kind in its own concrete
            // node, absent from the recorded child kinds, was not taken.
            let concrete_absent = match alt {
                Production::Symbol { name } => concrete_named_absent(name),
                Production::Alias {
                    named: true,
                    value,
                    content,
                } => concrete_named_absent(value) || !alias_source_ok(content, value),
                _ => false,
            };
            if concrete_absent {
                visited.clear();
                continue;
            }
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            if ys.contains(target_kind) {
                matching_alts.push(alt);
            }
            visited.clear();
        }
        if matching_alts.len() == 1 {
            return Some(matching_alts[0]);
        }
        if matching_alts.len() > 1 {
            // When multiple alternatives match via yield-set, apply
            // tree-sitter's precedence ordering: higher PREC wins.
            // This is the grammar author's explicit disambiguator for
            // ambiguous productions; it should be honored unconditionally,
            // not gated on whether the constraint blob is empty.
            matching_alts.sort_by_key(|alt| std::cmp::Reverse(prec_value(alt)));
            return Some(matching_alts[0]);
        }
    }

    // No recorded-complement heuristic tier matched: fall to the
    // canonical default section (FIELD-name dispatch + CHOICE-with-BLANK
    // categorical semantics). This is the same default the unification
    // review will rely on once the heuristics above are retired; a
    // measurement confirmed they are NOT yet subsumed (bypassing them
    // regresses even arduino 0/3), so they stay until the review is
    // strengthened, but the default section is now factored out.
    default_choice(schema, grammar, cursor, alternatives)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod review_tests {
    use super::{pattern_trailing_literal, unmarked_base_literal};
    use crate::emit_pretty::Production;

    fn s(v: &str) -> Production {
        Production::String { value: v.into() }
    }
    fn p(v: &str) -> Production {
        Production::Pattern { value: v.into() }
    }

    #[test]
    fn pattern_trailing_literal_extracts_constant_tail() {
        assert_eq!(pattern_trailing_literal("[bB]'").as_deref(), Some("'"));
        assert_eq!(pattern_trailing_literal("[xX]'").as_deref(), Some("'"));
        assert_eq!(pattern_trailing_literal("[bB]*'").as_deref(), Some("'"));
        // No constant tail, or not a leading-class shape.
        assert_eq!(pattern_trailing_literal("[bB]"), None);
        assert_eq!(pattern_trailing_literal("'"), None);
        assert_eq!(pattern_trailing_literal("[a-z]+[0-9]"), None);
    }

    #[test]
    fn unmarked_base_prefers_bare_string_over_prefix_pattern() {
        // PHP single-quote opener: CHOICE[PATTERN "[bB]'", STRING "'"].
        // The bare STRING "'" is the unmarked base; the PATTERN is the
        // binary-string prefix variant.
        let alts = [p("[bB]'"), s("'")];
        let base = unmarked_base_literal(&alts).unwrap();
        assert!(matches!(base, Production::String { value } if value == "'"));
    }

    #[test]
    fn unmarked_base_all_string_suffix_still_works() {
        // C-family opener: " is the suffix of every prefixed variant.
        let alts = [s("L\""), s("u\""), s("U\""), s("u8\""), s("\"")];
        let base = unmarked_base_literal(&alts).unwrap();
        assert!(matches!(base, Production::String { value } if value == "\""));
    }

    #[test]
    fn unmarked_base_declines_unrelated_choice() {
        // An ordinary CHOICE of unrelated literals has no unique suffix base.
        assert!(unmarked_base_literal(&[s("+"), s("-")]).is_none());
        // A non-string / non-prefix-pattern member disqualifies the CHOICE.
        assert!(unmarked_base_literal(&[p("[a-z]+"), s("'")]).is_none());
    }
}
