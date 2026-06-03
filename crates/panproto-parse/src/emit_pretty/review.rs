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
    alt_satisfies_pre_alias_constraints, children_for, classify_seq_positions, clear_field_context,
    collect_field_names, contains_newline_pattern, current_field_context,
    first_unconsumed_target_fingerprint, has_field_in, has_relevant_constraint, has_repeat_in,
    is_immediate_token, is_newline_alt, is_newline_like_pattern, is_no_space_external,
    is_whitespace_external, is_whitespace_only_pattern, is_word_like, leaf_terminal_role,
    literal_strings, literal_value, mandatory_field_names, member_has_leading_bracket,
    pattern_absorbs_leading_space, placeholder_for_pattern, prec_value, push_field_context,
    referenced_symbols, seq_bracket_triggers_indent, unwrap_to_string, vertex_id_kind,
    yield_of_production,
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
            // Skip leaf shortcut for bracket-pair literals like "()"
            // when the vertex has an alias-resolved rule. The rule-based
            // path correctly emits them as separate BracketOpen/Close
            // tokens with proper spacing.
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            let vkind = vertex.kind.as_ref();
            let has_alias_rule = grammar
                .named_alias_map
                .get(vkind)
                .is_some_and(|src| grammar.rules.contains_key(src));
            if !(is_bracket_pair && has_alias_rule) {
                // An empty bracket-pair literal (`()`, `[]`, `{}` captured
                // as one token, e.g. empty parameters) hugs its callee on
                // the left (`f()`) but still spaces after a keyword
                // (`return ()`). That is exactly the BracketClose role
                // (tight inner/left edge, keyword-spaced). Other leaves
                // keep their delimiter-or-terminal role.
                // Raw inline content (HTML/markup element `text`) abuts
                // its surrounding tags with no inserted whitespace; emit
                // it tight on both sides so the layout pass does not grow
                // the captured text on each re-emit.
                if out.cassette.is_some_and(|c| c.kind_is_tight_content(vkind)) {
                    out.no_space();
                    out.token_with_role(literal, Some(TokenRole::Terminal));
                    out.no_space();
                    return Ok(());
                }
                // Captured-content external token between string/heredoc
                // delimiters (ruby `string_content`/`heredoc_content`, regex
                // content). Its literal text IS the verbatim source between
                // the delimiters; the layout pass must not wrap it in
                // sibling-separation spaces (`"bar"`, not `" bar "`), which
                // would fold into the captured text on re-parse and accrete
                // one space per emit. Derived structurally (the
                // `SEQ[open_ext, content.., close_ext]` shape), so it stays
                // in the generic emitter rather than a per-language cassette.
                if grammar.external_content_kinds.contains(vkind) {
                    out.no_space();
                    out.token_with_role(literal, Some(TokenRole::Terminal));
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
                out.token_with_role(literal, Some(role));
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
        let old_rule = out.current_rule.take();
        out.current_rule = Some(kind.to_owned());
        // An indented-block rule (`block = SEQ[REPEAT(_statement),
        // _dedent]`) is reached directly because its opening `_indent`
        // lives in the hidden parent. Synthesize the opening indent so
        // the body is indented; the rule's own `_dedent` closes it.
        let synthetic_indent = grammar.synthetic_indent_rules.contains(kind);
        if synthetic_indent {
            out.indent_open();
        }
        let mut cursor = ChildCursor::new(&edges);
        emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out)?;
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
    let key = (vertex_id.to_string(), rule_name.to_owned());
    let inserted = EMIT_MU_FRAMES.with(|frames| frames.borrow_mut().insert(key.clone()));
    if !inserted {
        // We are already walking this rule at this vertex deeper in
        // the call stack. The coinductive μ-fixed-point reading
        // returns the empty sequence here; the surrounding
        // production resumes after the SYMBOL.
        return Ok(());
    }
    let result = emit_production(protocol, schema, grammar, vertex_id, rule, cursor, out);
    EMIT_MU_FRAMES.with(|frames| {
        frames.borrow_mut().remove(&key);
    });
    result
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
    drain_extras(protocol, schema, grammar, cursor, out)?;
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
            .any(|t| matches!(t, Token::Lit(_, _)));
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
            out.token(value);
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
                if let Some(edge) = cursor.take_field(&field) {
                    return emit_in_child_context(
                        protocol, schema, grammar, &edge.tgt, production, out,
                    );
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
                    out.token(&v);
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
            if name.starts_with('_') {
                // Hidden rule: not a vertex kind on the schema side.
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
                    if is_whitespace_external(name) {
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
            if let Some(matched) =
                pick_choice_with_cursor(schema, grammar, vertex_id, cursor, members)
            {
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

            let mut emitted_any = false;
            loop {
                let cursor_snap = cursor.consumed.clone();
                let out_snap = out.snapshot();
                let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
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
                                if !cassette_replaces_sep && !out.lit_emitted_since(pre_sep) {
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
    // Leaf shortcut: if the child has a literal-value and no
    // structural children, emit the captured text. Skip for bracket-pair
    // literals when the production resolves to a rule with those brackets.
    if let Some(literal) = literal_value(schema, child_id) {
        if children_for(schema, child_id).is_empty() {
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            if !is_bracket_pair {
                let kind = vertex_id_kind(schema, child_id).unwrap_or("");
                // See the matching guard in `emit_vertex`: a captured leading
                // whitespace is the separator, so suppress the redundant layout
                // space to keep the fixed point (INI's `setting_value`); keep
                // it when the literal carries no leading whitespace.
                if grammar.leading_space_terminals.contains(kind)
                    && literal.starts_with([' ', '\t'])
                {
                    out.no_space();
                }
                out.token_with_role(literal, Some(leaf_terminal_role(grammar, kind)));
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
            let mut cursor = ChildCursor::new(&edges);
            let old_rule = out.current_rule.take();
            out.current_rule = Some(name.to_owned());
            let result =
                emit_production(protocol, schema, grammar, child_id, rule, &mut cursor, out);
            out.current_rule = old_rule;
            return result;
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

pub(crate) fn pick_choice_with_cursor<'a>(
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    cursor: &ChildCursor<'_>,
    alternatives: &'a [Production],
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
    let trace_tokens: Vec<String> = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            cs.iter()
                .filter_map(|c| {
                    let s = c.sort.as_ref();
                    if s.starts_with("ptrace-") {
                        c.value.strip_prefix('T').map(ToOwned::to_owned)
                    } else if let Some(field) = s.strip_prefix("field:") {
                        (alt_field_names.contains(field)
                            || field_ctx.as_deref() == Some(field))
                        .then(|| c.value.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
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
    if let Some(idx) = super::select_choice_with_trace(
        grammar,
        alternatives,
        &demand,
        &labels,
        field_ctx.as_deref(),
        &field_constraints,
        &trace_tokens,
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
