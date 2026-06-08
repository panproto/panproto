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

use super::{BTreeMap, Grammar, Production, TokenRole};

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

/// True when a SEQ opens with a non-word bracket pair `OPEN ... CLOSE`
/// (`{ … }`, `( … )`, `[ … ]`) whose open literal precedes the first
/// content member. Such a SEQ introduces a delimited region: a leading
/// extra (a comment) attached to the SEQ's vertex but ordinally before
/// the first element sits *inside* that region, so it must be emitted
/// after the open bracket, never hoisted before it. Returns the index of
/// the open-bracket member so the caller can drain extras right after it.
///
/// Grammar-derived: it only inspects the production's STRING members and
/// the canonical bracket pairing (`matching_close_bracket`); no language
/// names. Word-like openers (`begin`/`do`/`struct`) are excluded because
/// their leading-comment semantics differ (the keyword is itself content
/// the comment may legitimately precede) and the canonical pairing here
/// targets the punctuation-delimited list forms (`field_declaration_list`,
/// argument lists, block braces) where the open bracket is position 0.
pub(crate) fn seq_open_bracket_index(members: &[Production]) -> Option<usize> {
    let string_positions: Vec<(usize, &str)> = members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| unwrap_to_string(m).map(|s| (i, s)))
        .collect();
    let first_content_idx = members.iter().position(|m| unwrap_to_string(m).is_none())?;
    for &(oi, ov) in &string_positions {
        // The open bracket must precede the first content member (so a
        // leading extra is genuinely inside the region, not before a
        // prefix token).
        if oi >= first_content_idx {
            break;
        }
        let Some(close_text) = matching_close_bracket(ov) else {
            continue;
        };
        if is_word_like(ov) {
            continue;
        }
        if string_positions
            .iter()
            .rev()
            .find(|(ci, _)| *ci > oi)
            .is_some_and(|(_, v)| *v == close_text)
        {
            return Some(oi);
        }
    }
    None
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

/// A connector punctuation token is tight on BOTH sides: it joins a
/// left and right operand into one lexeme (`Foo.Bar`, `a::b`, `x->y`).
/// These are the access / path / member-resolution operators, never
/// spaced. Used when a token's role is not recovered from the grammar
/// (an external scanner token resolved by a cassette, e.g. the
/// Haskell-family module-path `_dot`) so it still hugs its neighbours.
pub(crate) fn is_connector_punctuation(s: &str) -> bool {
    matches!(s, "." | "::" | "->")
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

/// True when a production, once emitted, contributes a token the lexer admits
/// only with no preceding whitespace. This is [`is_immediate_token`] extended
/// through a named `ALIAS` wrapper: tree-sitter wraps an `IMMEDIATE_TOKEN` in
/// an `ALIAS` to give the resulting node a kind name (tidal's `repeat_suffix =
/// SEQ["*", ALIAS{IMMEDIATE_TOKEN PATTERN, value:"number"}]` — the `2` must hug
/// the `*` or the suffix fails to lex). The alias-wrapped form does not reach
/// the rule-head no-space check in `emit_vertex` (it carries no rule), so the
/// SEQ walk consults this to hug such a member to its left neighbour.
pub(crate) fn reduces_to_immediate_token(prod: &Production) -> bool {
    match prod {
        Production::ImmediateToken { .. } => true,
        Production::Alias { content, .. }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::Field { content, .. }
        | Production::Reserved { content, .. } => reduces_to_immediate_token(content),
        // A CHOICE whose only materializing alternatives are immediate tokens
        // (the rest `BLANK`): whichever alt is selected hugs its left
        // neighbour (tidal `replicate_suffix = SEQ["!", CHOICE[number, BLANK]]`,
        // the `3` in `!3`).
        Production::Choice { members } => {
            let mut saw_immediate = false;
            for m in members {
                match m {
                    Production::Blank => {}
                    _ if reduces_to_immediate_token(m) => saw_immediate = true,
                    _ => return false,
                }
            }
            saw_immediate
        }
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

/// Unwrap the precedence wrappers (`Prec`, `PrecLeft`, `PrecRight`,
/// `PrecDynamic`) off a production, leaving everything else intact.
/// Precedence is irrelevant to emission; it only obscures the structural
/// shape of an alternative when checking for left recursion.
pub(crate) fn unwrap_prec(prod: &Production) -> &Production {
    match prod {
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. } => unwrap_prec(content),
        other => other,
    }
}

/// Collect every left-recursive alternative of a CHOICE rule body.
///
/// A rule `R = CHOICE[ base… | PREC(SEQ[R, rest…]) | … ]` is *left-recursive*:
/// an alternative, after stripping precedence wrappers, is a SEQ whose first
/// member is `SYMBOL(R)` — the rule referring to itself in head position. The
/// top-down emit walker collapses such a chain (the inner self-reference
/// re-enters the μ-frame and unfolds to the empty sequence, dropping every
/// operand but the operators), so it must instead be unrolled.
///
/// Returns the SEQ member slices `[SYMBOL(R), rest…]` of every left-recursive
/// alternative (a postfix rule like hcl `_expr_term` has several: `get_attr`,
/// `index`, `splat`), or `None` when no alternative is left-recursive.
pub(crate) fn left_recursive_alts<'a>(
    members: &'a [Production],
    rule_name: &str,
) -> Option<Vec<&'a [Production]>> {
    let seqs: Vec<&'a [Production]> = members
        .iter()
        .filter_map(|alt| match unwrap_prec(alt) {
            Production::Seq { members: seq }
                if matches!(seq.first(), Some(Production::Symbol { name }) if name == rule_name) =>
            {
                Some(seq.as_slice())
            }
            _ => None,
        })
        .collect();
    (!seqs.is_empty()).then_some(seqs)
}

/// True when a production is a *quote delimiter*: a (possibly wrapped) `STRING`
/// literal whose value ends in a string/char-literal quote character (`'`,
/// `"`, or `` ` ``), or a `CHOICE` every alternative of which is such a quote
/// `STRING`. This is the structural signature of the opening/closing token of
/// a string or character literal (C `char_literal` opens with
/// `CHOICE["L'","u'","U'","u8'","'"]` and closes with `STRING "'"`; the
/// `string_literal` quote pair is the `"`-suffixed analogue).
///
/// It deliberately does NOT match bracket delimiters (`{`, `(`, `[`) nor an
/// alias-over-`SYMBOL` delimiter (bash `brace_expression` opens with
/// `ALIAS{SYMBOL _brace_start, value:"{"}` and closes with
/// `IMMEDIATE_TOKEN(STRING "}")`), so a numeric brace-range body is never
/// mistaken for a quoted string body.
pub(crate) fn is_quote_delimiter(prod: &Production) -> bool {
    fn ends_in_quote(s: &str) -> bool {
        matches!(s.chars().last(), Some('\'' | '"' | '`'))
    }
    match prod {
        Production::Choice { members } => {
            !members.is_empty()
                && members
                    .iter()
                    .all(|m| unwrap_to_string(m).is_some_and(ends_in_quote))
        }
        other => unwrap_to_string(other).is_some_and(ends_in_quote),
    }
}

pub(crate) fn extract_line_comment_prefix(prod: &Production) -> Option<String> {
    match prod {
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => extract_line_comment_prefix(content),
        Production::Seq { members } if members.len() >= 2 => {
            // A line comment: a fixed prefix followed by the rest of the
            // line (a pattern, or REPEAT of one, that excludes newlines).
            // Recognising the prefix lets the layout pass insert a newline
            // after the comment so consecutive comments do not collapse
            // onto one line (and re-merge into a single comment node on
            // re-parse). The prefix is a STRING (`//`, `--`) or a
            // metacharacter-free PATTERN literal (julia's `#`).
            let prefix = match &members[0] {
                Production::String { value } => Some(value.clone()),
                Production::Pattern { value } => fixed_literal_pattern(value),
                _ => None,
            };
            if let Some(prefix) = prefix {
                if members[1..].iter().any(seq_member_is_line_rest) {
                    return Some(prefix);
                }
            }
            None
        }
        Production::Choice { members } => members.iter().find_map(extract_line_comment_prefix),
        _ => None,
    }
}

/// If a regex is a fixed literal (no regex metacharacters), return that
/// literal — its matched text is exactly the pattern itself, so it is a
/// usable line-comment prefix. Julia writes its `#` prefix as `PATTERN("#")`
/// rather than `STRING("#")`; this lets it be recognised. Any metacharacter
/// (`. * + ? [ ( | \` etc.) means the matched text is not fixed.
fn fixed_literal_pattern(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let has_meta = value.bytes().any(|b| {
        matches!(
            b,
            b'.' | b'*'
                | b'+'
                | b'?'
                | b'['
                | b']'
                | b'('
                | b')'
                | b'|'
                | b'{'
                | b'}'
                | b'^'
                | b'$'
                | b'\\'
        )
    });
    if has_meta {
        None
    } else {
        Some(value.to_string())
    }
}

/// A SEQ member that consumes the rest of a line: a newline-excluding
/// `PATTERN`, or a `REPEAT`/`REPEAT1`/wrapper thereof.
fn seq_member_is_line_rest(prod: &Production) -> bool {
    match prod {
        Production::Pattern { value } => {
            // `.*` (the canonical "rest of line" with DOTALL off) or any
            // *negated* character class that excludes the newline byte —
            // the latter covers the line-continuation-aware C-family form
            // `(\\+(.|\r?\n)|[^\\\n])*`, the multi-terminator JS/TS form
            // `[^\r\n  ]*`, and toml's `[^\x00-\x08\x0a-\x1f\x7f]`.
            // Scanning every negated class (not just a leading one) is what
            // recognises the c/cuda/move/pony/hare prefix where the class is
            // nested inside an alternation.
            value.contains(".*") || pattern_has_newline_excluding_class(value)
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Optional { content }
        | Production::Reserved { content, .. } => seq_member_is_line_rest(content),
        // A CHOICE or nested SEQ is a line-rest when any branch runs to
        // end of line. This is the shape of a line comment whose body
        // forks the doc-comment case from the plain-comment case: the rust
        // `line_comment` body is CHOICE[ SEQ[IMMEDIATE_TOKEN, PATTERN ".*"],
        // <doc branch via an external scanner>, IMMEDIATE_TOKEN(".*") ]. The
        // `.*` tail of the non-doc branches proves the token consumes the
        // rest of the line even though the doc branch is opaque to pattern
        // inspection.
        Production::Choice { members } | Production::Seq { members } => {
            members.iter().any(seq_member_is_line_rest)
        }
        _ => false,
    }
}

/// Whether a regex contains a *negated* character class `[^...]` whose body
/// excludes the newline byte (lists `\n`, `\r`, or their hex escapes). Such
/// a class is the regex idiom for "any character up to end of line", so a
/// terminal built from it consumes the rest of the line. We scan all negated
/// classes — not just a leading one — because the C-family line-comment body
/// nests `[^\\\n]` inside an alternation `(\\+(.|\r?\n)|[^\\\n])*`.
pub(crate) fn pattern_has_newline_excluding_class(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        // A negated class opens with `[^` not preceded by an escaping `\`.
        if bytes[i] == b'[' && bytes[i + 1] == b'^' && (i == 0 || bytes[i - 1] != b'\\') {
            // Find the unescaped closing `]` of this class.
            let mut j = i + 2;
            // A `]` immediately after `[^` is a literal member, not the end.
            if j < bytes.len() && bytes[j] == b']' {
                j += 1;
            }
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b']' {
                    break;
                }
                j += 1;
            }
            let class_end = j.min(bytes.len());
            let body = &value[i..class_end];
            if body.contains("\\n")
                || body.contains("\\r")
                || body.contains("\\x0a")
                || body.contains("\\x0A")
            {
                return true;
            }
            i = class_end + 1;
        } else {
            i += 1;
        }
    }
    false
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

/// True when a `PATTERN` regex consumes the rest of the source line: it
/// ends in an unbounded `.*` / `.+` (the regex `.` excludes newlines, so
/// such a tail greedily runs to the end of the line). A bare named rule of
/// this shape (`hash_bang_line = #!.*`, `shebang = #!...`) is a
/// *rest-of-line terminal*: like a line comment, the token it emits absorbs
/// any following text on the same line, so the next sibling must start on a
/// fresh line or it re-parses as part of this token. This is the same
/// structural fact `seq_member_is_line_rest` recognises for the body of a
/// line comment (a STRING prefix then a rest-of-line PATTERN); here the
/// whole token *is* the rest-of-line pattern, so there is no STRING prefix
/// to register in `line_comment_prefixes`.
///
/// The tail must be genuinely unbounded: `firrtl`'s `info = @\[.*\]` has a
/// `]` after the `.*`, so the `.*` is bounded and the token does NOT run to
/// end-of-line. We require that nothing meaningful follows the final
/// `.*`/`.+` except an optional captured newline / end-anchor / closing
/// group or quantifier.
pub(crate) fn is_rest_of_line_pattern(value: &str) -> bool {
    let bytes = value.as_bytes();
    // Find the last `.*` or `.+` whose `.` is a real metacharacter (not an
    // escaped literal dot `\.`).
    let mut tail_start = None;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'.' && (bytes[i + 1] == b'*' || bytes[i + 1] == b'+') {
            // Not an escaped dot: count preceding backslashes; an even
            // count (including zero) leaves the dot as a metacharacter.
            let mut bs = 0;
            let mut j = i;
            while j > 0 && bytes[j - 1] == b'\\' {
                bs += 1;
                j -= 1;
            }
            if bs % 2 == 0 {
                tail_start = Some(i + 2);
            }
        }
        i += 1;
    }
    if let Some(start) = tail_start {
        return rest_after_unbounded_tail_is_inert(&value[start..]);
    }
    // No `.*`/`.+`: a line comment may instead end in an unbounded
    // *newline-excluding negated class* quantifier — forth's `\\[^\n]*`,
    // the `//[^\n]*` / `#[^\r\n]*` idiom. Find the last `[^...]` whose body
    // excludes the newline byte and is immediately quantified by `*`/`+`,
    // then require the remainder to be inert (same rule as the `.*` tail).
    let mut neg_tail_start = None;
    let mut i = 0;
    while i + 1 < bytes.len() {
        // An unescaped `[^`: count preceding backslashes; an even count
        // (including zero) leaves the `[` as a class-open metacharacter.
        // forth's `\\[^\n]*` has two backslashes (an escaped literal `\`)
        // before the `[`, so the class IS real.
        let class_open = bytes[i] == b'[' && bytes[i + 1] == b'^' && {
            let mut bs = 0;
            let mut j = i;
            while j > 0 && bytes[j - 1] == b'\\' {
                bs += 1;
                j -= 1;
            }
            bs % 2 == 0
        };
        if class_open {
            // Locate the unescaped closing `]` of this negated class.
            let mut j = i + 2;
            if j < bytes.len() && bytes[j] == b']' {
                j += 1; // leading literal `]`
            }
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b']' {
                    break;
                }
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                // The inner members (between `[^` and `]`).
                let inner = &value[i + 2..j];
                let quant = j + 1;
                let unbounded =
                    quant < bytes.len() && (bytes[quant] == b'*' || bytes[quant] == b'+');
                // A genuine "rest of line" class excludes ONLY newline bytes
                // (`\n`, optionally `\r`). A class that ALSO excludes other
                // members (`[^"\\\r\n]` — a quote/backslash-bounded string
                // fragment) is bounded on the same line and is NOT a line
                // comment tail.
                if unbounded && negated_class_excludes_only_newlines(inner) {
                    neg_tail_start = Some(quant + 1);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    match neg_tail_start {
        Some(start) => rest_after_unbounded_tail_is_inert(&value[start..]),
        None => false,
    }
}

/// Whether a negated character class body (the `...` of `[^...]`) excludes
/// *only* newline bytes — `\n`, `\r`, and their hex/octal escapes — and
/// nothing else. Such a class is the regex idiom for "any character up to end
/// of line" (forth `[^\n]`, `[^\r\n]`); a class that also excludes other
/// members (`[^"\\\r\n]`) is a same-line-bounded fragment, not a rest-of-line
/// tail. The body must list at least one newline byte and no non-newline
/// member.
fn negated_class_excludes_only_newlines(inner: &str) -> bool {
    let bytes = inner.as_bytes();
    let mut i = 0;
    let mut saw_newline = false;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' | b'r' => {
                    saw_newline = true;
                    i += 2;
                }
                b'x' => {
                    // `\x0a` / `\x0A` / `\x0d` / `\x0D` are newline bytes.
                    let hex = inner.get(i + 2..i + 4).unwrap_or("");
                    if matches!(hex, "0a" | "0A" | "0d" | "0D") {
                        saw_newline = true;
                        i += 4;
                    } else {
                        return false;
                    }
                }
                _ => return false,
            }
        } else {
            // Any literal member that is not itself a newline disqualifies.
            match bytes[i] {
                b'\n' | b'\r' => {
                    saw_newline = true;
                    i += 1;
                }
                _ => return false,
            }
        }
    }
    saw_newline
}

/// Everything after an unbounded rest-of-line tail (`.*`/`.+` or a
/// newline-excluding `[^...]*`) must be inert for the terminal to genuinely
/// run to end of line: an optional captured newline, an end anchor, or a
/// closing group / quantifier (rust's `([^\[\n].*)?\n` ends `)?\n`). Any
/// literal that could appear on the same line (a `]`, a word char, ...) means
/// the tail is bounded and this is NOT a rest-of-line terminal.
fn rest_after_unbounded_tail_is_inert(rest: &str) -> bool {
    let rb = rest.as_bytes();
    let mut k = 0;
    while k < rb.len() {
        match rb[k] {
            b')' | b'?' | b'*' | b'+' | b'$' => k += 1,
            b'\\' if k + 1 < rb.len() => match rb[k + 1] {
                b'n' | b'r' | b'f' | b't' | b'v' | b'z' | b'Z' => k += 2,
                _ => return false,
            },
            _ => return false,
        }
    }
    true
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

/// Names of the `FIELD`s an alternative is *forced* to bind if taken: a
/// field is mandatory unless it is reachable only through an `OPTIONAL`,
/// a `REPEAT` (zero-or-more), or a `CHOICE` that offers a `BLANK` escape.
/// Symbol references are NOT expanded — a field hidden behind a SYMBOL is
/// the referenced rule's concern, not this alternative's surface demand.
///
/// Used to decide whether an alternative can consume a non-field-bound
/// edge: an alt whose only fields are optional (e.g. bash `_expansion_body`
/// SEQ with an optional `field('operator','!')` before a non-field
/// `variable_name`) must NOT be rejected just because the field name does
/// not match the generic `child_of` edge label.
pub(crate) fn mandatory_field_names(production: &Production) -> Vec<&str> {
    let mut out = Vec::new();
    collect_mandatory_fields(production, true, &mut out);
    out
}

fn collect_mandatory_fields<'p>(
    production: &'p Production,
    mandatory: bool,
    out: &mut Vec<&'p str>,
) {
    match production {
        Production::Field { name, content } => {
            if mandatory {
                out.push(name.as_str());
            }
            // A field's own body never re-introduces mandatory siblings.
            collect_mandatory_fields(content, false, out);
        }
        Production::Seq { members } => {
            for m in members {
                collect_mandatory_fields(m, mandatory, out);
            }
        }
        Production::Choice { members } => {
            // A CHOICE that offers BLANK is itself skippable: none of its
            // branches is forced.
            let escapable = members.iter().any(|m| matches!(m, Production::Blank));
            let inner = mandatory && !escapable;
            for m in members {
                collect_mandatory_fields(m, inner, out);
            }
        }
        Production::Optional { content } | Production::Repeat { content } => {
            collect_mandatory_fields(content, false, out);
        }
        Production::Repeat1 { content }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Alias { content, .. }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            collect_mandatory_fields(content, mandatory, out);
        }
        _ => {}
    }
}

/// Every `FIELD` name appearing structurally in `production` (all fields,
/// optional or not), without expanding `SYMBOL` references. Used to scope a
/// `field:<name>` trace token to CHOICEs whose alternatives actually bind
/// that field, so the recorded value cannot leak into an unrelated literal
/// CHOICE that merely shares the text (bash `_statements`' trailing
/// `_terminator` matching a sibling `case_item`'s `field:termination=";;"`).
pub(crate) fn collect_field_names<'p>(
    production: &'p Production,
    out: &mut std::collections::HashSet<&'p str>,
) {
    match production {
        Production::Field { name, content } => {
            out.insert(name.as_str());
            collect_field_names(content, out);
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                collect_field_names(m, out);
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
        | Production::Reserved { content, .. } => collect_field_names(content, out),
        _ => {}
    }
}

/// Collect FIELD names that a production re-binds its child to, expanding
/// hidden (`_`-prefixed) SYMBOL references inline (cycle-guarded). Stops at
/// visible rules — those are walked as their own vertex and carry their own
/// field structure. This is how a grammar like swift's
/// `type_annotation = SEQ[":", FIELD(type, _possibly_implicitly_unwrapped_type)]`
/// — whose hidden body re-binds the inner type to `FIELD(name, …)` — is
/// recognized as labelling its child `name`, not `type`: the parser
/// (authoritative) emits the inner FIELD name, so the outer FIELD's name is a
/// wrapper that the generated parser flattens away.
pub(crate) fn collect_inner_field_names_expanded<'p>(
    production: &'p Production,
    grammar: &'p crate::emit_pretty::grammar::Grammar,
    out: &mut std::collections::HashSet<&'p str>,
    seen: &mut std::collections::HashSet<&'p str>,
) {
    match production {
        Production::Field { name, content } => {
            out.insert(name.as_str());
            collect_inner_field_names_expanded(content, grammar, out, seen);
        }
        Production::Symbol { name } if name.starts_with('_') && seen.insert(name.as_str()) => {
            if let Some(rule) = grammar.rules.get(name) {
                collect_inner_field_names_expanded(rule, grammar, out, seen);
            }
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                collect_inner_field_names_expanded(m, grammar, out, seen);
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
            collect_inner_field_names_expanded(content, grammar, out, seen);
        }
        _ => {}
    }
}

/// Whether a REPEAT body iterates over DISTINCT whole sibling vertices
/// with no grammatical separator: each iteration produces a named-rule
/// vertex (or a CHOICE of them), so the source separates consecutive
/// iterations by whitespace/newline alone. Recognizes a bare named-rule
/// SYMBOL and a hidden (`_`-prefixed) SYMBOL whose rule unwraps (through
/// PREC/TOKEN) to a CHOICE every alternative of which is itself a
/// whole-vertex item (pkl `objectBody`'s `REPEAT(_objectMember)`, whose
/// `_objectMember` is `CHOICE[objectProperty|objectEntry|objectElement|…]`).
/// Such iterations must be separator-emitted so the next item's leading
/// bracket (`["k"]`) does not glue onto the prior item's trailing terminal
/// and re-lex as a subscript.
pub(crate) fn repeat_body_is_whole_vertex_item(
    content: &Production,
    grammar: &crate::emit_pretty::grammar::Grammar,
) -> bool {
    fn check(
        p: &Production,
        grammar: &crate::emit_pretty::grammar::Grammar,
        seen: &mut std::collections::HashSet<String>,
    ) -> bool {
        match p {
            Production::Field { .. } => true,
            Production::Symbol { name } => {
                if !name.starts_with('_') {
                    return grammar.rules.contains_key(name);
                }
                // Hidden rule: unwrap and require a CHOICE of whole-vertex items.
                if !seen.insert(name.clone()) {
                    return false;
                }
                grammar
                    .rules
                    .get(name)
                    .is_some_and(|rule| check(rule, grammar, seen))
            }
            Production::Choice { members } => {
                !members.is_empty() && members.iter().all(|m| check(m, grammar, seen))
            }
            Production::Token { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => check(content, grammar, seen),
            _ => false,
        }
    }
    let mut seen = std::collections::HashSet::new();
    check(content, grammar, &mut seen)
}

/// Whether the REPEAT body can produce a BRACKET-KEYED MEMBER: an item rule
/// shaped `SEQ["[", … , "]", "=" …]` — a subscript-bracketed key bound to a
/// value (pkl `objectEntry` `["k"] = v`). Such a member, juxtaposed after a
/// bare-expression sibling (`objectElement`) with only a space, is absorbed
/// as a SUBSCRIPT of the prior expression's trailing terminal even across
/// whitespace (`1 ["k"]` re-lexes as `1["k"]`), so the two must be
/// NEWLINE-separated. The `"]" then "="` shape is what distinguishes a
/// member binding from a bracket-leading COMMAND ARGUMENT (bash `echo [ x ]`),
/// which is never of the form `["k"] =` and stays space-separated.
pub(crate) fn repeat_has_bracket_keyed_member(content: &Production, grammar: &Grammar) -> bool {
    fn is_bracket_keyed(seq_members: &[Production]) -> bool {
        // First token must be `[`, a later token `]`, and a token after that `=`.
        if seq_members
            .first()
            .and_then(first_string_of)
            .is_none_or(|s| s != "[")
        {
            return false;
        }
        let rest: Vec<&Production> = seq_members.iter().collect();
        let close = rest
            .iter()
            .position(|m| first_string_of(m).is_some_and(|s| s == "]"));
        let Some(close_idx) = close else {
            return false;
        };
        rest[close_idx + 1..]
            .iter()
            .any(|m| has_leading_string(m, "="))
    }
    fn has_leading_string(p: &Production, target: &str) -> bool {
        match p {
            Production::Choice { members } => members.iter().any(|m| has_leading_string(m, target)),
            _ => first_string_of(p).is_some_and(|s| s == target),
        }
    }
    fn check(
        p: &Production,
        grammar: &Grammar,
        seen: &mut std::collections::HashSet<String>,
    ) -> bool {
        match p {
            Production::Symbol { name } => {
                if !seen.insert(name.clone()) {
                    return false;
                }
                grammar
                    .rules
                    .get(name)
                    .is_some_and(|rule| check(rule, grammar, seen))
            }
            Production::Choice { members } => members.iter().any(|m| check(m, grammar, seen)),
            Production::Seq { members } => is_bracket_keyed(members),
            Production::Token { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => check(content, grammar, seen),
            _ => false,
        }
    }
    let mut seen = std::collections::HashSet::new();
    check(content, grammar, &mut seen)
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

/// Whether `prod` is a "blank line" rule body: it reduces, through the
/// transparent precedence / token / FIELD / ALIAS wrappers, to a single
/// `PATTERN` that matches ONLY a newline (a `\n` line break). This is stricter
/// than [`contains_newline_pattern`], which admits a newline anywhere inside a
/// `SEQ`/`CHOICE`; here the whole body must BE the newline. It is the
/// signature of an in-grammar line-ending field such as vimdoc's
/// `_blank = FIELD(blank, PATTERN("\n"))`, distinguishing it from a generic
/// statement separator or an external scanner token.
pub(crate) fn is_blank_line_rule(prod: &Production) -> bool {
    match prod {
        Production::Pattern { value } => is_newline_like_pattern(value),
        Production::Field { content, .. }
        | Production::Alias { content, .. }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Reserved { content, .. } => is_blank_line_rule(content),
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
    // Bare `\s` / ` ` / `\t`, or the Unicode space-separator property
    // `\p{Zs}` (http uses `\p{Zs}+` as the inter-token whitespace between a
    // request's method, URL, and version).
    if matches!(trimmed, "\\s" | " " | "\\t" | "\\p{Zs}") {
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

/// If `pattern` is a greedy unbounded negated character class
/// (`[^...]+` or `[^...]*`, optionally `^`-anchored, with nothing after
/// the quantifier), return the class's inner content (the text between
/// `[^` and `]`). Such a terminal keeps consuming any character the class
/// admits, so an emitted-adjacent token whose first char is admitted
/// would be swallowed on re-parse (HTML unquoted `attribute_value`
/// `[^<>"'=\s]+` eating the `/` of a following `/>`). Returns `None` for
/// any other shape (bounded classes, trailing anchors, alternations),
/// which are not unbounded right-absorbers.
pub(crate) fn unbounded_negated_class(pattern: &str) -> Option<&str> {
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let inner = pattern.strip_prefix("[^")?;
    // Find the (unescaped) closing `]` of the class.
    let mut close = None;
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b']' {
            close = Some(i);
            break;
        }
        i += 1;
    }
    let close = close?;
    let rest = &inner[close + 1..];
    // The class must be the whole pattern up to a single `+`/`*`
    // quantifier; nothing may follow (a trailing literal or anchor would
    // bound the match).
    if rest == "+" || rest == "*" {
        Some(&inner[..close])
    } else {
        None
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

    // A literal core wrapped in optional-whitespace classes
    // (`[ \t]*:[ \t]*` -> ":", GLSL's `#extension` `extension : behavior`
    // separator). The whitespace runs are optional padding the layout pass
    // re-supplies; the constant `:` between them is the actual separator
    // token and must be emitted, not a `_` placeholder (which breaks the
    // re-parse of the surrounding directive).
    if let Some(lit) = decode_whitespace_padded_literal(pattern) {
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

/// Decode a PATTERN of the shape `<ws-pad><literal><ws-pad>` where each
/// `<ws-pad>` is an optional run of a whitespace character class (`[ \t]*`,
/// `\s*`) and `<literal>` is a constant byte sequence with no further regex
/// metacharacters. Returns the literal core. GLSL's `#extension`
/// `preproc_extension` separates `extension : behavior` with
/// `IMMEDIATE_TOKEN([ \t]*:[ \t]*)`: the `:` is the real separator token,
/// the surrounding `[ \t]*` is padding the layout pass re-supplies. Without
/// this the whole pattern hits the `_` placeholder, dropping the `:` and
/// breaking the directive's re-parse. Returns `None` unless exactly one
/// whitespace-class run brackets each side and the middle is a clean literal.
pub(crate) fn decode_whitespace_padded_literal(pattern: &str) -> Option<String> {
    // Strip a leading optional-whitespace-class run `[ \t]*` / `\s*`. When the
    // pattern has no leading run, fall through with the whole pattern as the
    // body so a TRAILING-only padded literal (http's comment prefix `#\s*` /
    // `//\s*`, where the constant `#` / `//` precedes the optional whitespace)
    // is still decoded: the trailing-run strip below reduces it to the literal
    // core and the layout pass re-supplies the optional whitespace.
    let body = strip_leading_ws_run(pattern).unwrap_or(pattern);
    // Strip a trailing optional-whitespace-class run from the END.
    let body = if let Some(idx) = body.rfind('[') {
        let tail = &body[idx..];
        // The tail must be exactly a whitespace-class run with nothing after.
        if strip_leading_ws_run(tail) == Some("") {
            &body[..idx]
        } else {
            body
        }
    } else if let Some(stripped) = body
        .strip_suffix("\\s*")
        .or_else(|| body.strip_suffix("\\s+"))
    {
        stripped
    } else {
        body
    };
    if body.is_empty() {
        return None;
    }
    // The remaining body must be a clean literal (no regex metacharacters).
    decode_simple_pattern_literal(body)
}

/// Strip a single leading optional-whitespace-class run (`[ \t]*` / `\s*` /
/// `\s+`) from the front of a regex, returning the remainder. `None` if no
/// such run is present.
fn strip_leading_ws_run(s: &str) -> Option<&str> {
    if let Some(rest) = s.strip_prefix("\\s*").or_else(|| s.strip_prefix("\\s+")) {
        return Some(rest);
    }
    // A bracketed class `[...]` followed by `*` or `+`, where the class
    // contains only whitespace atoms.
    let rest = s.strip_prefix('[')?;
    let end = rest.find(']')?;
    let class = &rest[..end];
    let after = &rest[end + 1..];
    let after = after
        .strip_prefix('*')
        .or_else(|| after.strip_prefix('+'))?;
    if !is_whitespace_only_pattern(&format!("[{class}]*")) {
        return None;
    }
    Some(after)
}

/// A scanner concatenation / no-space marker external token: the
/// adjacent tokens are lexically glued with no whitespace. These follow
/// a stable tree-sitter naming convention across grammars (bash
/// `_concat`, and the `_no_space` / `_brace_concat` / `_concat_list` /
/// `_no_line_break` family). Emit them as a `NoSpace` so the layout pass
/// suppresses the sibling-separation space (otherwise string content
/// around an interpolation re-spaces and grows one space per emit).
///
/// The `_immediate_*` family is the same idea from the other direction: a
/// zero-width marker the scanner emits only when the next token follows
/// with NO intervening whitespace (julia `_immediate_brace` /
/// `_immediate_paren` / `_immediate_bracket` / `_immediate_string_start`
/// gate `Foo{T}`, `f(x)`, `a[i]`, `r"..."` postfix forms). Without the
/// `NoSpace` the emitter inserts a separator (`Foo {T}`) and the immediacy
/// scanner rejects it on re-parse, collapsing the construct to an ERROR.
pub(crate) fn is_no_space_external(name: &str) -> bool {
    matches!(
        name,
        "_concat" | "_brace_concat" | "_concat_list" | "_no_space" | "_no_line_break"
    ) || name.starts_with("_immediate")
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

/// A required inter-token whitespace external scanner token: it carries
/// no text but the parser requires whitespace at that position
/// (dockerfile `_non_newline_whitespace` between path arguments). Emit
/// it as a forced space so the neighbours stay separated.
pub(crate) fn is_whitespace_external(name: &str) -> bool {
    matches!(
        name,
        "_non_newline_whitespace" | "_whitespace" | "_space" | "_ws" | "whitespace"
    )
}

#[cfg(test)]
mod line_comment_prefix_tests {
    use super::*;

    fn pat(v: &str) -> Production {
        Production::Pattern {
            value: v.to_string(),
        }
    }
    fn string(v: &str) -> Production {
        Production::String {
            value: v.to_string(),
        }
    }

    #[test]
    fn newline_excluding_class_detected_anywhere() {
        // C-family line-comment body: the negated class is nested inside an
        // alternation, not at the start.
        assert!(pattern_has_newline_excluding_class(
            r"(\\+(.|\r?\n)|[^\\\n])*"
        ));
        // JS/TS multi-terminator form.
        assert!(pattern_has_newline_excluding_class(r"[^\r\n  ]*"));
        // toml control-byte form.
        assert!(pattern_has_newline_excluding_class(
            r"[^\x00-\x08\x0a-\x1f\x7f]"
        ));
        // A positive class containing \n is NOT a rest-of-line class.
        assert!(!pattern_has_newline_excluding_class(r"[\n]+"));
        // A bare wildcard is handled separately (no negated class here).
        assert!(!pattern_has_newline_excluding_class("abc"));
    }

    #[test]
    fn fixed_literal_pattern_accepts_plain_prefix() {
        assert_eq!(fixed_literal_pattern("#"), Some("#".to_string()));
        assert_eq!(fixed_literal_pattern("//"), Some("//".to_string()));
        // Any metacharacter disqualifies it.
        assert_eq!(fixed_literal_pattern(".*"), None);
        assert_eq!(fixed_literal_pattern("a+"), None);
        assert_eq!(fixed_literal_pattern(""), None);
        // `#=` has no regex metacharacters, so it *is* a fixed literal;
        // it is the SEQ-member shape (a SYMBOL rest, not a line rest) that
        // keeps julia's block_comment from acquiring a prefix, not this.
        assert_eq!(fixed_literal_pattern("#="), Some("#=".to_string()));
    }

    #[test]
    fn julia_line_comment_prefix_from_pattern_first_member() {
        // julia: SEQ[PATTERN("#"), PATTERN(".*")] — prefix is a PATTERN.
        let rule = Production::Seq {
            members: vec![pat("#"), pat(".*")],
        };
        assert_eq!(extract_line_comment_prefix(&rule), Some("#".to_string()));
    }

    #[test]
    fn c_line_comment_prefix_from_nested_negated_class() {
        // c: TOKEN(CHOICE[ SEQ["//", line-rest], SEQ["/*", .., "/"] ]).
        let line = Production::Seq {
            members: vec![string("//"), pat(r"(\\+(.|\r?\n)|[^\\\n])*")],
        };
        let block = Production::Seq {
            members: vec![string("/*"), pat(r"[^*]*\*+([^/*][^*]*\*+)*"), string("/")],
        };
        let rule = Production::Token {
            content: Box::new(Production::Choice {
                members: vec![line, block],
            }),
        };
        assert_eq!(extract_line_comment_prefix(&rule), Some("//".to_string()));
    }

    #[test]
    fn block_comment_with_symbol_rest_yields_no_prefix() {
        // julia block_comment: SEQ[PATTERN("#="), SYMBOL] — the second
        // member is not a line-rest, so no prefix (otherwise a block
        // comment would spuriously trigger a trailing newline).
        let rule = Production::Seq {
            members: vec![
                pat("#="),
                Production::Symbol {
                    name: "_block_comment_rest".to_string(),
                },
            ],
        };
        assert_eq!(extract_line_comment_prefix(&rule), None);
    }

    #[test]
    fn rust_line_comment_prefix_through_choice_body() {
        // rust: SEQ[STRING "//", CHOICE[ SEQ[IMMEDIATE_TOKEN, PATTERN ".*"],
        // <doc branch>, IMMEDIATE_TOKEN(".*") ]]. The prefix is the leading
        // STRING; the CHOICE body proves line-rest via the `.*` branches
        // even though the doc branch routes through external-scanner SYMBOLs
        // that pattern inspection cannot see into.
        let immediate = |inner: Production| Production::ImmediateToken {
            content: Box::new(inner),
        };
        let sym = |n: &str| Production::Symbol {
            name: n.to_string(),
        };
        let non_doc = Production::Seq {
            members: vec![immediate(pat(r"\/\/")), pat(".*")],
        };
        let doc = Production::Seq {
            members: vec![sym("_line_doc_comment_marker"), sym("_line_doc_content")],
        };
        let plain = immediate(pat(".*"));
        let rule = Production::Seq {
            members: vec![
                string("//"),
                Production::Choice {
                    members: vec![non_doc, doc, plain],
                },
            ],
        };
        assert_eq!(extract_line_comment_prefix(&rule), Some("//".to_string()));
    }

    #[test]
    fn rust_block_comment_choice_body_yields_no_prefix() {
        // rust: SEQ[STRING "/*", CHOICE[.. external/BLANK ..], STRING "*/"].
        // The body routes through external scanners and BLANK, never a
        // rest-of-line PATTERN, so the broadened CHOICE/SEQ recursion must
        // still refuse `/*`: a block comment is not a line comment and must
        // not trigger the trailing-newline guard.
        let content = Production::Symbol {
            name: "_block_comment_content".to_string(),
        };
        let body = Production::Choice {
            members: vec![
                Production::Choice {
                    members: vec![
                        Production::Seq {
                            members: vec![
                                Production::Symbol {
                                    name: "_block_doc_comment_marker".to_string(),
                                },
                                Production::Choice {
                                    members: vec![content.clone(), Production::Blank],
                                },
                            ],
                        },
                        content,
                    ],
                },
                Production::Blank,
            ],
        };
        let rule = Production::Seq {
            members: vec![string("/*"), body, string("*/")],
        };
        assert_eq!(extract_line_comment_prefix(&rule), None);
    }

    #[test]
    fn reduces_to_immediate_through_alias_and_choice() {
        let alias = |inner: Production| Production::Alias {
            content: Box::new(inner),
            named: true,
            value: "number".to_string(),
        };
        let imm = || Production::ImmediateToken {
            content: Box::new(pat("[0-9]+")),
        };
        // Bare IMMEDIATE_TOKEN.
        assert!(reduces_to_immediate_token(&imm()));
        // Named ALIAS over an IMMEDIATE_TOKEN (tidal `repeat_suffix` number).
        assert!(reduces_to_immediate_token(&alias(imm())));
        // CHOICE of [immediate, BLANK] (tidal `replicate_suffix`).
        assert!(reduces_to_immediate_token(&Production::Choice {
            members: vec![alias(imm()), Production::Blank],
        }));
        // A CHOICE with a non-immediate, non-blank alternative is NOT immediate
        // (whichever alt is selected may carry preceding whitespace).
        assert!(!reduces_to_immediate_token(&Production::Choice {
            members: vec![alias(imm()), string("x")],
        }));
        // A plain STRING / SYMBOL is not immediate.
        assert!(!reduces_to_immediate_token(&string("*")));
        assert!(!reduces_to_immediate_token(&Production::Symbol {
            name: "number".to_string(),
        }));
    }
}
