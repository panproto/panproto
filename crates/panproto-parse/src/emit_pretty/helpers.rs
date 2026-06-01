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

//! `emit_pretty::helpers` (Phase A decomposition).

use super::{Production, Grammar, TokenRole, BTreeMap};


/// Check if a SEQ's bracket at position `idx` triggers indentation.
#[allow(clippy::branches_sharing_code)]
pub(crate) fn seq_bracket_triggers_indent(
    members: &[Production],
    open_idx: usize,
    _grammar: &Grammar,
) -> bool {
    let string_positions: Vec<(usize, &str)> = members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| unwrap_to_string(m).map(|s| (i, s)))
        .collect();
    if string_positions.len() < 2 {
        return false;
    }
    let open_val = string_positions.iter().find(|(i, _)| *i == open_idx);
    let close_val = string_positions.last();
    if let (Some((_, open_text)), Some((close_idx, close_text))) = (open_val, close_val) {
        if open_idx >= *close_idx {
            return false;
        }
        // Word-like bracket pairs (function/end, if/end, while/end,
        // for/end, module/end, struct/end, begin/end, do/done, etc.)
        // always wrap block bodies that need indentation. This is a
        // structural invariant: word-like delimiters only appear in
        // block constructs across all 261 grammars.
        if is_word_like(open_text) && is_word_like(close_text) {
            return true;
        }
        let between = &members[open_idx + 1..*close_idx];
        // Only { } bracket pairs trigger indentation from direct
        // REPEAT content. Other pairs like ( ), < >, [ ] are
        // inline even when they contain REPEAT (comma-separated
        // lists, type parameters, function arguments).
        if *open_text == "{" && has_repeat_recursive(between) {
            return true;
        }
        // Follow SYMBOL → rule one level for CHOICE[SYMBOL, BLANK]
        // patterns where the SYMBOL's rule body has REPEAT.
        // Only for { } bracket pairs (block constructs). Other pairs
        // like < > (type parameters) are always inline.
        if *open_text == "{" {
            for m in between {
                if let Production::Choice { members: alts } = m {
                    let has_blank = alts.iter().any(|a| matches!(a, Production::Blank));
                    if has_blank {
                        for alt in alts {
                            if let Production::Symbol { name } = alt {
                                if let Some(rule) = _grammar.rules.get(name) {
                                    if has_repeat_in(rule) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    } else {
        false
    }
}


/// Check if a production's rule body starts with a bracket pair's open
/// `STRING`. Used to suppress `ForceSpace` before call-pattern members
/// (e.g. `argument_list` whose rule starts with `(`).
pub(crate) fn member_has_leading_bracket(prod: &Production, grammar: &Grammar) -> bool {
    match prod {
        Production::Symbol { name } => grammar
            .rules
            .get(name)
            .is_some_and(|rule| first_string_of(rule).is_some_and(|s| !is_word_like(s))),
        // A SEQ member whose first token is a non-word bracket (`(`, `[`)
        // is a call/index pattern: the preceding callee must stay tight
        // against it (`f(`, not `f (`). This also covers a CHOICE of such
        // SEQs (e.g. qvr's `morphism_call` arg-list alternatives), which
        // recurses here per alternative.
        Production::Seq { .. } => first_string_of(prod).is_some_and(|s| !is_word_like(s)),
        Production::Field { content, .. } => member_has_leading_bracket(content, grammar),
        Production::Choice { members } => {
            let non_blank: Vec<_> = members
                .iter()
                .filter(|m| !matches!(m, Production::Blank))
                .collect();
            !non_blank.is_empty()
                && non_blank
                    .iter()
                    .all(|m| member_has_leading_bracket(m, grammar))
        }
        Production::Alias { content, .. } => {
            if let Production::Symbol { name } = content.as_ref() {
                grammar
                    .rules
                    .get(name)
                    .is_some_and(|rule| first_string_of(rule).is_some_and(|s| !is_word_like(s)))
            } else {
                false
            }
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Optional { content } => member_has_leading_bracket(content, grammar),
        Production::Repeat { .. } | Production::Repeat1 { .. } => false,
        _ => false,
    }
}


pub(crate) fn first_string_of(prod: &Production) -> Option<&str> {
    match prod {
        Production::String { value } => Some(value.as_str()),
        Production::Seq { members } => members.first().and_then(first_string_of),
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Field { content, .. } => first_string_of(content),
        _ => None,
    }
}


/// Check if any member of a slice contains REPEAT/REPEAT1 recursively.
pub(crate) fn has_repeat_recursive(members: &[Production]) -> bool {
    members.iter().any(has_repeat_in)
}


pub(crate) fn has_repeat_in(prod: &Production) -> bool {
    match prod {
        Production::Repeat { .. } | Production::Repeat1 { .. } => true,
        Production::Choice { members } | Production::Seq { members } => {
            members.iter().any(has_repeat_in)
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Reserved { content, .. }
        | Production::Alias { content, .. } => has_repeat_in(content),
        _ => false,
    }
}


/// A single-character unary sign (`-`, `+`) that, when it sits in an
/// optional *leading* slot before a single operand, glues to that
/// operand (`signed_number`: `-1.0`, not `- 1.0`). These are excluded
/// from [`is_prefix_sigil`] because in a binary position they are
/// spaced operators; the leading-optional-slot structure is what
/// disambiguates them as unary.
pub(crate) fn is_unary_sign(s: &str) -> bool {
    matches!(s, "-" | "+")
}


/// Extract the unary sign STRING(s) carried by an optional *leading*
/// SEQ member: a `CHOICE[sign | … | BLANK]` or `OPTIONAL(sign)`. Returns
/// empty unless the member is structurally an optional sign slot, which
/// marks the sign as a tight unary prefix on the following operand.
pub(crate) fn leading_optional_sign(prod: &Production) -> Vec<String> {
    match prod {
        Production::Choice { members }
            if members.iter().any(|m| matches!(m, Production::Blank)) =>
        {
            members
                .iter()
                .filter_map(unwrap_to_string)
                .filter(|s| is_unary_sign(s))
                .map(str::to_owned)
                .collect()
        }
        Production::Optional { content } => unwrap_to_string(content)
            .filter(|s| is_unary_sign(s))
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default(),
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Field { content, .. } => leading_optional_sign(content),
        _ => Vec::new(),
    }
}


/// The canonical closing delimiter for an opening bracket STRING, if it
/// is one of the universal nesting brackets. `<`/`>` are excluded: they
/// are ambiguous with comparison operators and are handled by the
/// first/last fallback only when they genuinely bound the SEQ.
pub(crate) fn matching_close_bracket(open: &str) -> Option<&'static str> {
    match open {
        "(" => Some(")"),
        "[" => Some("]"),
        "{" => Some("}"),
        _ => None,
    }
}


/// Check if a string value is word-like (alphanumeric/underscore).
pub(crate) fn is_word_like(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.starts_with(|c: char| c.is_alphabetic() || c == '_')
}


/// A prefix `STRING` (position before all content in a non-`CHOICE` SEQ) is a
/// tight sigil (`BracketOpen`) only when it is NOT a common binary/assignment
/// operator. Single-character ASCII operators like `=`, `+`, `-` need space;
/// multi-character prefixes (`...`, `::`, `@`, `#`, `$`) and non-ASCII
/// prefixes are tight.
pub(crate) fn is_prefix_sigil(s: &str) -> bool {
    if s.len() == 1 {
        let c = s.as_bytes()[0];
        !matches!(
            c,
            b'=' | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'<'
                | b'>'
                | b'!'
                | b'?'
                | b'|'
                | b'&'
                | b'^'
                | b'%'
                | b'~'
        )
    } else {
        true
    }
}


/// Unwrap wrapper productions (`Token`, `ImmediateToken`, `Prec`, `PrecLeft`,
/// `PrecRight`, `PrecDynamic`, `Field`, `Reserved`) to find the inner `STRING`
/// value. Returns `None` if the production is not a (possibly wrapped)
/// `STRING`.
pub(crate) fn is_immediate_token(prod: &Production) -> bool {
    match prod {
        Production::ImmediateToken { .. } => true,
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::Field { content, .. }
        | Production::Reserved { content, .. } => is_immediate_token(content),
        _ => false,
    }
}


pub(crate) fn unwrap_to_string(prod: &Production) -> Option<&str> {
    match prod {
        Production::String { value } => Some(value.as_str()),
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Field { content, .. }
        | Production::Reserved { content, .. } => unwrap_to_string(content),
        _ => None,
    }
}


pub(crate) fn extract_line_comment_prefix(prod: &Production) -> Option<String> {
    match prod {
        Production::Token { content } | Production::ImmediateToken { content } => {
            extract_line_comment_prefix(content)
        }
        Production::Seq { members } if members.len() >= 2 => {
            if let Production::String { value } = &members[0] {
                if members[1..].iter().any(|m| {
                    matches!(m, Production::Pattern { value } if value.contains(".*") || value.contains("[^\\n]*") || value.contains("[^\\r\\n]*"))
                }) {
                    return Some(value.clone());
                }
            }
            None
        }
        Production::Choice { members } => members.iter().find_map(extract_line_comment_prefix),
        _ => None,
    }
}


/// The `PATTERN` regex of a (possibly token/precedence-wrapped) bare
/// terminal production, or `None` if it is not a bare pattern.
pub(crate) fn terminal_pattern_of(prod: &Production) -> Option<&str> {
    match prod {
        Production::Pattern { value } => Some(value),
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => terminal_pattern_of(content),
        _ => None,
    }
}


/// The role for a leaf vertex's captured `literal-value`, given its
/// kind: a string/heredoc delimiter external (`string_start`/`string_end`)
/// brackets its content tightly, so it is emitted as a bracket rather
/// than a free-standing [`Terminal`](TokenRole::Terminal) that the layout
/// pass would space (`'hello'`, not `' hello '`).
pub(crate) fn leaf_terminal_role(grammar: &Grammar, kind: &str) -> TokenRole {
    if grammar.external_bracket_opens.contains(kind) {
        TokenRole::BracketOpen
    } else if grammar.external_bracket_closes.contains(kind) {
        TokenRole::BracketClose
    } else {
        TokenRole::Terminal
    }
}


/// True when a production is (a possibly precedence/token-wrapped) bare
/// `PATTERN` terminal — the shape a case-insensitive keyword takes inside
/// an anonymous `ALIAS{…, value: "<word>"}`. Such a pattern carries no
/// canonical text of its own; the alias value supplies it.
pub(crate) fn alias_content_is_terminal_pattern(prod: &Production) -> bool {
    match prod {
        Production::Pattern { .. } => true,
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => alias_content_is_terminal_pattern(content),
        _ => false,
    }
}


/// Unwrap precedence/token wrappers to reach a SEQ production.
pub(crate) fn unwrap_to_seq(prod: &Production) -> &Production {
    match prod {
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::Reserved { content, .. } => unwrap_to_seq(content),
        other => other,
    }
}


/// The SYMBOL name a member references directly (no aliasing), if it is a
/// bare `SYMBOL`. Used to spot external delimiter tokens.
pub(crate) fn external_symbol_name(prod: &Production) -> Option<&str> {
    match prod {
        Production::Symbol { name } => Some(name.as_str()),
        _ => None,
    }
}


/// Collect all SYMBOL names referenced anywhere in the grammar rules.
pub(crate) fn collect_all_symbol_refs(
    rules: &BTreeMap<String, Production>,
) -> std::collections::HashSet<String> {
    let mut refs = std::collections::HashSet::new();
    fn walk(prod: &Production, refs: &mut std::collections::HashSet<String>) {
        match prod {
            Production::Symbol { name } => {
                refs.insert(name.clone());
            }
            Production::Seq { members } | Production::Choice { members } => {
                for m in members {
                    walk(m, refs);
                }
            }
            Production::Alias { content, .. }
            | Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, refs),
            _ => {}
        }
    }
    for rule in rules.values() {
        walk(rule, &mut refs);
    }
    refs
}


/// Collect every literal STRING token directly inside `production`
/// (without descending into SYMBOLs / hidden rules). Used to score
/// CHOICE alternatives against the parent vertex's interstitials so
/// the right operator / keyword form is picked when the schema
/// preserves interstitial fragments from a prior parse.
pub(crate) fn literal_strings(production: &Production) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(p: &Production, out: &mut Vec<String>) {
        match p {
            Production::String { value } if !value.is_empty() => {
                out.push(value.clone());
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(m, out);
                }
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Alias { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, out),
            _ => {}
        }
    }
    walk(production, &mut out);
    out
}


/// Collect every SYMBOL name reachable from `production` without
/// crossing into nested rules. Used by `pick_choice_with_cursor` to
/// rank alternatives by "any SYMBOL inside this alt matches something
/// on the cursor", instead of just the first SYMBOL: a leading
/// optional like `attribute_item` then `parameter` is otherwise
/// rejected when only the parameter children are present.
pub(crate) fn referenced_symbols(production: &Production) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(p: &'a Production, out: &mut Vec<&'a str>) {
        match p {
            Production::Symbol { name } => out.push(name.as_str()),
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(m, out);
                }
            }
            Production::Alias {
                content,
                named,
                value,
            } => {
                // A named ALIAS produces a child vertex whose kind is
                // the alias `value` (e.g. `ALIAS { content: STRING "=",
                // value: "punctuation", named: true }` introduces a
                // `punctuation` child). For cursor-driven dispatch to
                // recognise alts that emit such children, yield the
                // alias value as a referenced symbol. Anonymous aliases
                // do not introduce a named node and only need their
                // inner content's symbols.
                if *named && !value.is_empty() {
                    out.push(value.as_str());
                }
                walk(content, out);
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
            | Production::Reserved { content, .. } => walk(content, out),
            _ => {}
        }
    }
    walk(production, &mut out);
    out
}


#[cfg(test)]
pub(crate) fn first_symbol(production: &Production) -> Option<&str> {
    match production {
        Production::Symbol { name } => Some(name),
        Production::Seq { members } => members.iter().find_map(first_symbol),
        Production::Choice { members } => members.iter().find_map(first_symbol),
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => first_symbol(content),
        _ => None,
    }
}


pub(crate) fn prec_value(prod: &Production) -> i64 {
    match prod {
        Production::Prec { value, .. }
        | Production::PrecLeft { value, .. }
        | Production::PrecRight { value, .. }
        | Production::PrecDynamic { value, .. } => value.as_i64().unwrap_or(0),
        _ => 0,
    }
}


pub(crate) fn has_any_field(production: &Production) -> bool {
    match production {
        Production::Field { .. } => true,
        Production::Seq { members } | Production::Choice { members } => {
            members.iter().any(has_any_field)
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
        | Production::Reserved { content, .. } => has_any_field(content),
        _ => false,
    }
}


pub(crate) fn has_field_in(production: &Production, edge_kinds: &[&str]) -> bool {
    match production {
        Production::Field { name, .. } => edge_kinds.contains(&name.as_str()),
        Production::Seq { members } | Production::Choice { members } => {
            members.iter().any(|m| has_field_in(m, edge_kinds))
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
        | Production::Reserved { content, .. } => has_field_in(content, edge_kinds),
        _ => false,
    }
}


/// If `p` unwraps to an ALIAS whose inner content is a CHOICE-of-STRINGs
/// (or a single STRING), return that set. Otherwise None.
pub(crate) fn literal_choice_set(p: &Production) -> Option<Vec<&str>> {
    fn unwrap(p: &Production) -> &Production {
        match p {
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => unwrap(content),
            _ => p,
        }
    }
    let p = unwrap(p);
    let Production::Alias { content, .. } = p else {
        return None;
    };
    let inner = unwrap(content);
    match inner {
        Production::String { value } => Some(vec![value.as_str()]),
        Production::Choice { members } => {
            let mut out = Vec::new();
            for m in members {
                match unwrap(m) {
                    Production::String { value } => out.push(value.as_str()),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}


/// True iff `pattern` matches a (possibly optional / repeated) sequence
/// of carriage-return and newline characters only. Examples: `\r?\n`,
/// `\n`, `\r\n`, `\n+`, `\r?\n+`. Distinguishes structural newline
/// terminals from generic whitespace and from other patterns that
/// happen to contain a newline escape inside a larger class.
/// True when a CHOICE alternative emits a newline: a newline-like
/// PATTERN, an external `_newline`-family scanner token, or a hidden
/// rule whose body is a newline PATTERN. Used to prefer the newline
/// form of a statement separator over a `;` whose only support is a
/// fingerprint contaminated by a `;` elsewhere in the vertex.
pub(crate) fn is_newline_alt(grammar: &Grammar, alt: &Production) -> bool {
    match alt {
        Production::Pattern { value } => is_newline_like_pattern(value),
        Production::Symbol { name } => {
            grammar.external_newlines.contains(name)
                || (name.starts_with('_')
                    && grammar
                        .rules
                        .get(name)
                        .is_some_and(contains_newline_pattern))
        }
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => is_newline_alt(grammar, content),
        _ => false,
    }
}


pub(crate) fn contains_newline_pattern(prod: &Production) -> bool {
    match prod {
        Production::Pattern { value } => is_newline_like_pattern(value),
        Production::Choice { members } | Production::Seq { members } => {
            members.iter().any(contains_newline_pattern)
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Alias { content, .. }
        | Production::Reserved { content, .. } => contains_newline_pattern(content),
        _ => false,
    }
}


pub(crate) fn is_newline_like_pattern(pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    // A top-level alternation of newline-only branches (e.g. CSV's row
    // terminator `\r|\r\n|\n`) is itself newline-like: every branch matches
    // only newline characters, so the whole pattern is a structural line
    // break, not free text. Without this the pattern fell through to the
    // `_` placeholder, which re-parsed into a phantom field and grew.
    split_top_level_alternation(pattern)
        .iter()
        .all(|branch| is_newline_branch(branch))
}


/// One branch of a newline pattern: a non-empty run of `\n` / `\r` atoms
/// (raw or escaped), newline-only character classes (`[\r\n]`), and
/// quantifiers, with nothing else.
pub(crate) fn is_newline_branch(branch: &str) -> bool {
    if branch.is_empty() {
        return false;
    }
    let mut chars = branch.chars();
    let mut saw_newline_atom = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n' | 'r') => saw_newline_atom = true,
                _ => return false,
            },
            '\n' | '\r' => saw_newline_atom = true,
            // A character class is a newline atom iff it contains only
            // newline characters (`[\r\n]`, `[\n]`): escaped `\n`/`\r` or the
            // raw newline bytes, nothing else.
            '[' => {
                let mut class_has_atom = false;
                let mut esc = false;
                let mut closed = false;
                for cc in chars.by_ref() {
                    if esc {
                        match cc {
                            'n' | 'r' => class_has_atom = true,
                            _ => return false,
                        }
                        esc = false;
                        continue;
                    }
                    match cc {
                        ']' => {
                            closed = true;
                            break;
                        }
                        '\\' => esc = true,
                        '\n' | '\r' => class_has_atom = true,
                        _ => return false,
                    }
                }
                if !closed || !class_has_atom {
                    return false;
                }
                saw_newline_atom = true;
            }
            '?' | '*' | '+' => {} // quantifiers on the previous atom
            _ => return false,
        }
    }
    saw_newline_atom
}


/// Split a regex on its top-level `|` alternation operators, ignoring `|`
/// that is escaped (`\|`) or inside a character class (`[...]`). Returns the
/// whole pattern as a single element when there is no top-level alternation.
pub(crate) fn split_top_level_alternation(pattern: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_class = false;
    let mut escaped = false;
    for (i, c) in pattern.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '[' => in_class = true,
            ']' => in_class = false,
            '|' if !in_class => {
                parts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&pattern[start..]);
    parts
}


/// True iff `pattern` matches a (possibly quantified) run of generic
/// whitespace characters: `\s+`, `[ \t]+`, ` +`, `\s*`. Such patterns
/// describe interstitial spacing rather than syntactic content, so the
/// pretty emitter can drop them and let the layout pass insert the
/// configured separator.
pub(crate) fn is_whitespace_only_pattern(pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    // Strip an outer quantifier suffix.
    let trimmed = pattern.trim_end_matches(['?', '*', '+']);
    if trimmed.is_empty() {
        return false;
    }
    // Bare `\s` / ` ` / `\t`.
    if matches!(trimmed, "\\s" | " " | "\\t") {
        return true;
    }
    // Character class containing only whitespace atoms.
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let mut chars = inner.chars();
        let mut saw_atom = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('s' | 't' | 'r' | 'n') => saw_atom = true,
                    _ => return false,
                },
                ' ' | '\t' => saw_atom = true,
                _ => return false,
            }
        }
        return saw_atom;
    }
    false
}


/// True iff `pattern` can match a string whose first character is an
/// ordinary space, so that any layout space the emitter inserts *before*
/// this terminal would be absorbed into the terminal's text on re-parse.
///
/// Such a terminal must be emitted tight against its predecessor: otherwise
/// the inserted space folds into the captured literal on re-parse, and the
/// next emit inserts another space, growing the output by one space per
/// round-trip (e.g. INI's `setting_value = PATTERN ".+"`, where `.` matches
/// a space: `key = value` -> `key =  value` -> `key =   value`). This is the
/// same unbounded-growth class as a content node spaced from its delimiters.
///
/// Conservative: only the unambiguous leading atoms that admit a space are
/// recognised, so a terminal that genuinely cannot start with a space keeps
/// its normal spacing.
pub(crate) fn pattern_absorbs_leading_space(pattern: &str) -> bool {
    // Skip a leading anchor; it does not consume input.
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let mut chars = pattern.chars();
    match chars.next() {
        // `.` matches any character except newline, including a space.
        Some('.') => true,
        // A negated character class matches a space unless the negation
        // explicitly excludes it (a literal space, `\s`, or `\t`).
        Some('[') if pattern.starts_with("[^") => {
            let inner = &pattern[2..];
            let end = inner.find(']').unwrap_or(inner.len());
            let negated = &inner[..end];
            !(negated.contains(' ') || negated.contains("\\s") || negated.contains("\\t"))
        }
        _ => false,
    }
}


pub(crate) fn placeholder_for_pattern(pattern: &str) -> String {
    // Heuristic placeholder for unconstrained PATTERN terminals.
    //
    // First handle the "the regex IS a literal escape" cases that
    // tree-sitter grammars use as separators (`\n`, `\r\n`, `;`,
    // etc.); emitting the matching character is always preferable
    // to a `_x` identifier-like placeholder when the surrounding
    // grammar expects a separator.
    let simple_lit = decode_simple_pattern_literal(pattern);
    if let Some(lit) = simple_lit {
        return lit;
    }

    // A positive char class of fixed literals (`[;#]`, `[<>]`): emit the
    // first member -- a valid token of the terminal (an ini/properties
    // comment marker `[;#]` reparses as a comment), unlike the `_`
    // fallback which is not a token of the class. The exact member is
    // lost without a complement, but the structure is preserved.
    if let Some(lit) = char_class_first_literal(pattern) {
        return lit;
    }

    if pattern.contains("[0-9]") || pattern.contains("\\d") {
        "0".into()
    } else if pattern.contains("[a-zA-Z_]") || pattern.contains("\\w") {
        "_x".into()
    } else if pattern.contains('"') || pattern.contains('\'') {
        "\"\"".into()
    } else {
        "_".into()
    }
}


/// Decode a tree-sitter PATTERN whose regex is a simple literal
/// (newline, semicolon, comma, etc.) to the byte sequence it matches.
/// Returns `None` for patterns with character classes, alternations,
/// or quantifiers; the caller falls back to the heuristic placeholder.
pub(crate) fn decode_simple_pattern_literal(pattern: &str) -> Option<String> {
    // Skip patterns containing regex metachars that would broaden the
    // match beyond a single literal byte sequence.
    if pattern
        .chars()
        .any(|c| matches!(c, '[' | ']' | '(' | ')' | '*' | '+' | '?' | '|' | '{' | '}'))
    {
        return None;
    }
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some(other) => out.push(other),
                None => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// A scanner concatenation / no-space marker external token: the
/// adjacent tokens are lexically glued with no whitespace. These follow
/// a stable tree-sitter naming convention across grammars (bash
/// `_concat`, and the `_no_space` / `_brace_concat` / `_concat_list` /
/// `_no_line_break` family). Emit them as a `NoSpace` so the layout pass
/// suppresses the sibling-separation space (otherwise string content
/// around an interpolation re-spaces and grows one space per emit).
pub(crate) fn is_no_space_external(name: &str) -> bool {
    matches!(
        name,
        "_concat" | "_brace_concat" | "_concat_list" | "_no_space" | "_no_line_break"
    )
}

/// The first literal member of a positive fixed-char class PATTERN
/// (`[;#]` -> ";", `[<>]` -> "<"). Returns `None` for negated classes
/// (`[^...]`), ranges (`[a-z]`), quantified or composite patterns, or
/// anything that is not exactly a bare `[...]` of literal chars. Used by
/// [`placeholder_for_pattern`] so a marker terminal emits a valid token.
pub(crate) fn char_class_first_literal(pattern: &str) -> Option<String> {
    let inner = pattern.strip_prefix('[')?.strip_suffix(']')?;
    if inner.is_empty() || inner.starts_with('^') || inner.contains('-') {
        return None;
    }
    let mut chars = inner.chars();
    let first = chars.next()?;
    if first == '\\' {
        return Some(
            match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            }
            .to_string(),
        );
    }
    Some(first.to_string())
}
