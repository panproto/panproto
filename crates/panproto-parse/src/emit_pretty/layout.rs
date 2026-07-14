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

//! `emit_pretty::layout` (Phase A decomposition).

use super::{Grammar, TokenRole, is_word_like};

// ═══════════════════════════════════════════════════════════════════

/// Whitespace and indentation policy applied during emission.
///
/// The default policy inserts a single space between adjacent tokens,
/// a newline after `;` / `}` / `{`, and tracks indent on `{` / `}`
/// boundaries. Per-language overrides (idiomatic indent width,
/// trailing-comma rules, blank-line conventions) can ride alongside
/// this struct in a follow-up branch; today's defaults aim only for
/// syntactic validity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormatPolicy {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Separator inserted between adjacent terminals that the lexer
    /// would otherwise glue together (word ↔ word, operator ↔ operator).
    /// Default is a single space.
    pub separator: String,
    /// Newline byte sequence emitted after `line_break_after` tokens
    /// and at end-of-output. Default is `"\n"`.
    pub newline: String,
    /// Tokens after which the walker breaks to a new line.
    pub line_break_after: Vec<String>,
    /// Tokens that increase indent on emission.
    pub indent_open: Vec<String>,
    /// Tokens that decrease indent on emission.
    pub indent_close: Vec<String>,
}

impl Default for FormatPolicy {
    fn default() -> Self {
        Self {
            indent_width: 2,
            separator: " ".to_owned(),
            newline: "\n".to_owned(),
            line_break_after: vec![";".into(), "{".into(), "}".into()],
            indent_open: vec!["{".into()],
            indent_close: vec!["}".into()],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Token list output with Spacing algebra
// ═══════════════════════════════════════════════════════════════════
//
// Emit produces a free monoid over `Token`. Layout (spaces, newlines,
// indentation) is a homomorphism `Vec<Token> -> Vec<u8>` parameterised
// by `FormatPolicy`. Separating the structural output from the layout
// decision means each phase has one job: emit walks the grammar and
// pushes tokens; layout is a single fold, locally driven by adjacent
// pairs and a depth counter. Snapshot/restore is just `tokens.len()`.

#[derive(Clone)]
pub(crate) enum Token {
    /// A user-visible terminal contributed by the grammar, annotated
    /// with its structural role for spacing decisions.
    Lit(String, TokenRole),
    /// `indent_open` marker emitted when a `Lit` matched the policy's
    /// open list. Carried as a separate token so layout can decide to
    /// break + indent without re-scanning.
    IndentOpen,
    /// `indent_close` marker emitted before a closer-`Lit`.
    IndentClose,
    /// "Break a line here if not already at line start" — used after
    /// statements/declarations and after open braces.
    LineBreak,
    /// Force a space before the next Lit even if the role-pair table
    /// says tight. Pushed between consecutive content-producing SEQ
    /// members (e.g. between `command_name` and `argument`) to ensure
    /// sibling-vertex tokens are separated.
    ForceSpace,
    /// Suppress the next inter-Lit separator. Pushed by the REPEAT
    /// walker when an iteration's "separator slot" (a CHOICE-with-BLANK
    /// or OPTIONAL at SEQ position 0) emitted zero content tokens, so
    /// the categorical reading is "no source-level separator existed
    /// between these two sibling iterations of the body".
    NoSpace,
    /// Guard emitted right after a greedy unbounded negated-class
    /// terminal (`[^...]+`, e.g. HTML's unquoted `attribute_value`). The
    /// carried string is the negated set's inner content. If the NEXT
    /// `Lit` begins with a character that set ADMITS, the terminal would
    /// swallow that character on re-parse (`Ok` + `/>` lexes as the value
    /// `Ok/>`, turning a `self_closing_tag` into a `start_tag`), so the
    /// layout fold forces a separator. Transparent otherwise.
    AbsorberGuard(String),
    /// Exact source bytes replayed from the layout complement
    /// (`reconstruct_subtree_bytes`): a whole vertex subtree whose
    /// `interstitial-N` / `literal-value` fibre tiled its byte span exactly.
    /// The fold writes these bytes verbatim and inserts NO role-derived
    /// separator on either side — the replayed text already carries its own
    /// leading and trailing whitespace, so the byte-faithful path bypasses the
    /// role table entirely. The carried bytes may contain newlines; they are
    /// written through without disturbing the indent counter (the replay is
    /// self-contained, including its own indentation).
    Verbatim(String),
}

pub(crate) struct Output<'a> {
    pub(crate) tokens: Vec<Token>,
    pub(crate) policy: &'a FormatPolicy,
    pub(crate) grammar: &'a Grammar,
    pub(crate) current_rule: Option<String>,
    pub(crate) cassette: Option<&'a dyn crate::languages::cassettes::GrammarCassette>,
}

#[derive(Clone)]
pub(crate) struct OutputSnapshot {
    pub(crate) tokens_len: usize,
    /// The active per-vertex `ptrace` budget at snapshot time. Restored with
    /// the token stream so a REPEAT/OPTIONAL iteration that rolls back also
    /// rolls back any separator budget it spent (a `_terminator` that picked
    /// `;;` inside a zero-progress iteration must un-spend it).
    ptrace_budget: std::collections::HashMap<String, usize>,
}

impl<'a> Output<'a> {
    pub(crate) fn new(
        policy: &'a FormatPolicy,
        grammar: &'a Grammar,
        cassette: Option<&'a dyn crate::languages::cassettes::GrammarCassette>,
    ) -> Self {
        Self {
            tokens: Vec::new(),
            policy,
            grammar,
            current_rule: None,
            cassette,
        }
    }

    pub(crate) fn token(&mut self, value: &str) {
        self.token_with_role(value, None);
    }

    /// Emit a verbatim string-region leaf with NO layout side effects:
    /// the literal is pushed with the `Terminal` role but the
    /// `line_break_after` / `indent_open` machinery is bypassed. Tight
    /// string content (`kind_is_tight_content`, `string_content_kinds`,
    /// `external_content_kinds`) and the interpolation braces of a string
    /// (`$"…{x}…"`) are part of one lexical span where a literal `{`, `}`
    /// or `;` inside the captured text is data, not a block opener or a
    /// statement terminator: routing them through `token_with_role` would
    /// insert a newline / indent that the re-parse cannot absorb (the
    /// scanner only re-lexes the interpolation when the brace abuts its
    /// neighbours). The caller is responsible for any surrounding
    /// [`no_space`](Self::no_space) markers.
    pub(crate) fn tight_token(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        // Verbatim string-region content is glued to its delimiters and is
        // *data*, not syntax: a literal `;`/`#`/`//` inside the captured text
        // must not be re-interpreted as a line-comment opener (which would
        // append a newline in the layout fold). The `Immediate` role is
        // unconditionally tight on both sides and is excluded from the
        // line-comment-prefix newline, so it is the correct role for content.
        self.tokens
            .push(Token::Lit(value.to_owned(), TokenRole::Immediate));
    }

    pub(crate) fn token_with_role(&mut self, value: &str, explicit_role: Option<TokenRole>) {
        if value.is_empty() {
            return;
        }

        if value == "\n" || value == "\r\n" || value == "\r" {
            self.tokens.push(Token::LineBreak);
            return;
        }

        let trimmed = value.trim_end_matches(['\n', '\r']);
        let trailing_newlines = value.len() - trimmed.len();
        if trailing_newlines > 0 && !trimmed.is_empty() {
            let role = explicit_role.unwrap_or(TokenRole::Terminal);
            if role == TokenRole::BracketClose
                && self.policy.indent_close.iter().any(|t| t == trimmed)
            {
                self.tokens.push(Token::IndentClose);
            }
            self.tokens.push(Token::Lit(trimmed.to_owned(), role));
            if role == TokenRole::BracketOpen {
                if let Some(ref rule) = self.current_rule {
                    if self
                        .grammar
                        .indent_triggers
                        .contains(&(rule.clone(), trimmed.to_owned()))
                    {
                        self.tokens.push(Token::IndentOpen);
                    }
                }
            }
            self.tokens.push(Token::LineBreak);
            return;
        }

        let mut role = explicit_role.unwrap_or_else(|| self.lookup_role(value));
        // A cassette may declare a token lexically tight in a rule (a
        // scanner fact `grammar.json` omits, e.g. bash `VAR=1`): emit it
        // with the always-tight Connector role (which the layout pass
        // honours over the sibling-separation ForceSpace).
        if let (Some(rule), Some(cassette)) = (self.current_rule.as_ref(), self.cassette) {
            if cassette.operator_is_tight(rule, value) {
                role = TokenRole::Connector;
            }
        }

        if role == TokenRole::BracketClose && self.policy.indent_close.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentClose);
        }

        self.tokens.push(Token::Lit(value.to_owned(), role));

        if role == TokenRole::BracketOpen {
            let grammar_indent = self.current_rule.as_ref().is_some_and(|rule| {
                self.grammar
                    .indent_triggers
                    .contains(&(rule.clone(), value.to_owned()))
            });
            if grammar_indent {
                self.tokens.push(Token::IndentOpen);
                self.tokens.push(Token::LineBreak);
            }
        }
        // Line-break after tokens like `;` (statement terminator).
        // Skip for BracketOpen/BracketClose tokens that are NOT
        // indent-triggering (e.g. `{` in interpolation should not
        // trigger a line break).
        let is_non_indent_bracket = self.current_rule.is_some()
            && (role == TokenRole::BracketOpen || role == TokenRole::BracketClose)
            && !self.current_rule.as_ref().is_some_and(|rule| {
                self.grammar
                    .indent_triggers
                    .contains(&(rule.clone(), value.to_owned()))
            });
        if !is_non_indent_bracket && self.policy.line_break_after.iter().any(|t| t == value) {
            self.tokens.push(Token::LineBreak);
        }
    }

    pub(crate) fn lookup_role(&self, value: &str) -> TokenRole {
        if let Some(role) = self.explicit_role(value) {
            return role;
        }
        if is_word_like(value) {
            TokenRole::Keyword
        } else {
            TokenRole::Operator
        }
    }

    /// The role classified for `value` in the current rule, if any.
    /// `None` when the rule's grammar-derived `token_roles` map has no
    /// entry, leaving the caller to choose a structural default.
    pub(crate) fn explicit_role(&self, value: &str) -> Option<TokenRole> {
        self.current_rule
            .as_ref()
            .and_then(|rule| self.grammar.token_roles.get(rule))
            .and_then(|role_map| role_map.get(value).copied())
    }

    /// Emit a bracket-open token that triggers indentation. This is the
    /// inline-classification counterpart to the `indent_triggers` check
    /// in `token_with_role`: the SEQ walker computes indent-triggering
    /// from the SEQ structure directly rather than from a precomputed map.
    pub(crate) fn token_with_indent_open(&mut self, value: &str, role: TokenRole) {
        if value.is_empty() {
            return;
        }
        if role == TokenRole::BracketClose && self.policy.indent_close.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentClose);
        }
        self.tokens.push(Token::Lit(value.to_owned(), role));
        if role == TokenRole::BracketOpen {
            self.tokens.push(Token::IndentOpen);
            self.tokens.push(Token::LineBreak);
        }
    }

    pub(crate) fn newline(&mut self) {
        self.tokens.push(Token::LineBreak);
    }

    /// Push exact replayed source bytes (see [`Token::Verbatim`]). The bytes
    /// are written through the layout fold with no role-derived spacing on
    /// either edge: the layout complement already encodes the verbatim
    /// inter-token whitespace, so the byte-faithful replay path bypasses the
    /// role table for this span.
    pub(crate) fn verbatim(&mut self, bytes: &str) {
        if bytes.is_empty() {
            return;
        }
        self.tokens.push(Token::Verbatim(bytes.to_owned()));
    }

    /// Open an indent scope: subsequent `LineBreak`s render at the
    /// new depth until a matching `indent_close` pops it. Used by the
    /// external-token fallback to render indent-based grammars'
    /// `_indent` scanner outputs.
    pub(crate) fn indent_open(&mut self) {
        self.tokens.push(Token::IndentOpen);
        self.tokens.push(Token::LineBreak);
    }

    /// Close one indent scope opened by `indent_open`.
    pub(crate) fn indent_close(&mut self) {
        self.tokens.push(Token::IndentClose);
    }

    pub(crate) fn snapshot(&self) -> OutputSnapshot {
        OutputSnapshot {
            tokens_len: self.tokens.len(),
            ptrace_budget: super::snapshot_ptrace_budget(),
        }
    }

    pub(crate) fn restore(&mut self, snap: OutputSnapshot) {
        self.tokens.truncate(snap.tokens_len);
        super::restore_ptrace_budget(snap.ptrace_budget);
    }

    /// True iff at least one `Token::Lit` was pushed since `snap`.
    /// Control-only emissions (`LineBreak`, `IndentOpen` / `IndentClose`,
    /// `NoSpace`) do not count as content. Used by the REPEAT walker
    /// to detect that a "separator slot" CHOICE picked its BLANK
    /// alternative, so the next iteration's content can be marked
    /// tight against the previous iteration's content.
    pub(crate) fn lit_emitted_since(&self, snap: OutputSnapshot) -> bool {
        self.tokens[snap.tokens_len..]
            .iter()
            .any(|t| matches!(t, Token::Lit(_, _) | Token::Verbatim(_)))
    }

    /// Push a marker that suppresses the next inter-Lit separator the
    /// layout pass would otherwise insert. Used to encode "no source-
    /// level separator was emitted between these two Lits" without
    /// having to make per-grammar adjacency decisions in the layout.
    pub(crate) fn no_space(&mut self) {
        self.tokens.push(Token::NoSpace);
    }

    /// Push a marker that forces a separator (space) between the
    /// surrounding Lits. Used for an external scanner token that is
    /// required inter-token whitespace (dockerfile `_non_newline_whitespace`
    /// between path arguments), which carries no text of its own but
    /// must keep the neighbours apart.
    pub(crate) fn force_space(&mut self) {
        self.tokens.push(Token::ForceSpace);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        layout(
            &self.tokens,
            self.policy,
            &self.grammar.line_comment_prefixes,
            &self.grammar.trailing_break_markers,
            self.grammar.trailing_break_on_whitespace,
            self.grammar.top_level_text_admits_newline,
        )
    }
}

/// Fold a token list into bytes. The algebra:
/// * adjacent `Lit`s get a single space iff `needs_space_between(a, b)`,
/// * `IndentOpen` / `IndentClose` adjust a depth counter,
/// * `LineBreak` writes `\n` if not already at line start, then the
///   next `Lit` writes `indent * indent_width` spaces of indent.
pub(crate) fn layout(
    tokens: &[Token],
    policy: &FormatPolicy,
    line_comment_prefixes: &[String],
    trailing_break_markers: &[String],
    trailing_break_on_whitespace: bool,
    top_level_text_admits_newline: bool,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut indent: usize = 0;
    let mut at_line_start = true;
    let mut last_role: Option<TokenRole> = None;
    let mut last_text: String = String::new();
    let mut suppress_next_separator = false;
    let mut force_next_separator = false;
    // The negated-class content of a greedy terminal that just emitted; if
    // the next Lit's first char is admitted by it, force a separator.
    let mut pending_absorber: Option<String> = None;
    // True iff the most recently emitted content token was an exact-replay
    // `Verbatim` blob. The byte-faithful replay path reproduces the source's
    // trailing bytes verbatim (the trailing interstitial is part of the
    // reconstructed span), so the final line-terminating newline below must not
    // be appended after a verbatim tail: the source may legitimately have ended
    // without a trailing newline, and a spurious `\n` can flip a
    // newline-sensitive scanner's parse (scala `class A\n()\n()\n{}` — the
    // trailing `\n` inserts an automatic semicolon that re-binds the empty
    // `class_parameters`/`template_body` as top-level `unit`/`block`). Canonical
    // (forget_layout) schemas emit no `Verbatim` tokens, so this never relaxes
    // the conventional terminating newline on the reformatting path.
    let mut last_content_was_verbatim = false;
    let newline = policy.newline.as_bytes();
    let separator = policy.separator.as_bytes();

    for (tok_idx, tok) in tokens.iter().enumerate() {
        if std::env::var("DBG_LAYOUT").is_ok() {
            match tok {
                Token::Lit(v, r) => eprintln!(
                    "  TOK: Lit({v:?}, {r:?}) at_line_start={at_line_start} last_role={last_role:?}"
                ),
                Token::IndentOpen => eprintln!("  TOK: IndentOpen"),
                Token::IndentClose => eprintln!("  TOK: IndentClose"),
                Token::LineBreak => eprintln!("  TOK: LineBreak"),
                Token::NoSpace => eprintln!("  TOK: NoSpace"),
                Token::ForceSpace => eprintln!("  TOK: ForceSpace"),
                Token::AbsorberGuard(s) => eprintln!("  TOK: AbsorberGuard({s:?})"),
                Token::Verbatim(s) => eprintln!("  TOK: Verbatim({s:?})"),
            }
        }
        match tok {
            Token::IndentOpen => indent += 1,
            Token::IndentClose => {
                indent = indent.saturating_sub(1);
                pending_absorber = None;
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                }
            }
            Token::LineBreak => {
                pending_absorber = None;
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                }
            }
            Token::NoSpace => {
                suppress_next_separator = true;
            }
            Token::ForceSpace => {
                force_next_separator = true;
            }
            Token::AbsorberGuard(negated) => {
                pending_absorber = Some(negated.clone());
            }
            Token::Verbatim(bytes_str) => {
                // Exact replayed source: written through with NO role-derived
                // separator on either edge. The complement already encodes the
                // verbatim whitespace, so the byte-faithful path must not let
                // the role table inject or suppress a space here. Any pending
                // absorber/force/suppress markers are discharged without effect.
                pending_absorber = None;
                suppress_next_separator = false;
                force_next_separator = false;
                // Indentation only applies to the FIRST line of the blob if we
                // were at a fresh line start; the blob carries its own internal
                // indentation thereafter.
                if at_line_start && !bytes_str.is_empty() {
                    bytes.extend(std::iter::repeat_n(b' ', indent * policy.indent_width));
                }
                bytes.extend_from_slice(bytes_str.as_bytes());
                // The trailing byte determines the line state for whatever
                // follows; the role chain is reset so the next `Lit` does not
                // role-space against a stale predecessor.
                at_line_start = bytes_str.ends_with(['\n', '\r']);
                last_role = None;
                last_text.clear();
                last_content_was_verbatim = true;
            }
            Token::Lit(value, role) => {
                // A greedy negated-class terminal just emitted: if it would
                // lexically swallow this Lit's first char on re-parse, the
                // boundary needs a separator regardless of the role pair.
                if let Some(negated) = pending_absorber.take() {
                    if value
                        .chars()
                        .next()
                        .is_some_and(|c| negated_class_admits(&negated, c))
                    {
                        force_next_separator = true;
                    }
                }
                // Block-opening bracket: BracketOpen followed by IndentOpen.
                // After a Terminal/BracketClose, this should be spaced
                // (`}\n` not `0{`).
                let is_block_open = *role == TokenRole::BracketOpen
                    && tokens
                        .get(tok_idx + 1)
                        .is_some_and(|t| matches!(t, Token::IndentOpen));
                if at_line_start {
                    bytes.extend(std::iter::repeat_n(b' ', indent * policy.indent_width));
                } else if let Some(prev_role) = last_role {
                    // The role-spacer inserts at most ONE separator at a token
                    // boundary, but a content leaf can carry the boundary
                    // whitespace inside its own captured text: a marker token
                    // whose `literal-value` ends in a space (djot
                    // `block_quote_marker` = `"> "`, the ATX/list markers of
                    // lightweight-markup grammars) already supplies the gap to
                    // the following content, and a token whose text begins with
                    // a space supplies it to the preceding one. Adding a
                    // role-derived space on top would double it, and the doubled
                    // space is re-absorbed into the marker's text on re-parse, so
                    // it accretes one space per emit (`# Heading` -> `#  Heading`
                    // -> `#   Heading` ...): the canonical fixed point is lost.
                    // When the boundary already carries whitespace from either
                    // side, the separator is redundant; suppress it. This is
                    // derived purely from the emitted token text, not any
                    // per-language table, and applies uniformly: a genuine
                    // no-whitespace marker (Org's `* Heading`, whose literal is
                    // bare `*`) is unaffected, since neither side carries the
                    // space.
                    let boundary_has_whitespace =
                        last_text.ends_with([' ', '\t']) || value.starts_with([' ', '\t']);
                    // An explicit NoSpace (suppress) is authoritative: it
                    // records that the source had no separator at this
                    // boundary (an empty REPEAT separator slot, an
                    // IMMEDIATE_TOKEN). It overrides the sibling-separation
                    // ForceSpace heuristic — otherwise beamed notes
                    // (`CDEF`) re-space to `C D E F`.
                    let want_space = !suppress_next_separator
                        && !boundary_has_whitespace
                        && (force_next_separator
                            || needs_space_by_role(prev_role, &last_text, *role, value)
                            || (is_block_open
                                && matches!(
                                    prev_role,
                                    TokenRole::Terminal | TokenRole::BracketClose
                                )));
                    if want_space {
                        bytes.extend_from_slice(separator);
                    }
                }
                suppress_next_separator = false;
                force_next_separator = false;
                bytes.extend_from_slice(value.as_bytes());
                at_line_start = false;
                last_content_was_verbatim = false;
                last_role = Some(*role);
                last_text.clear();
                last_text.push_str(value);
                // A verbatim string-region content leaf (`Immediate` role) is
                // data, not syntax: a `;`/`#`/`//` inside captured string text
                // must not open a line comment.
                if *role != TokenRole::Immediate
                    && line_comment_prefixes
                        .iter()
                        .any(|p| value.starts_with(p.as_str()))
                {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                    last_role = None;
                }
            }
        }
    }

    // Append the customary end-of-output newline only when no suppressor
    // fires: not already at line start, not directly after an exact-replay
    // verbatim tail (scala), not on a top-level free-text repeat that admits a
    // bare newline (liquid `{% endcomment %}` must not gain a trailing
    // `template_content`), and not after a hard-line-break marker
    // (markdown_inline). Each suppressor guards against the appended newline
    // manufacturing a phantom node on re-parse.
    if !at_line_start
        && !last_content_was_verbatim
        && !top_level_text_admits_newline
        && !ends_with_trailing_break_marker(
            &bytes,
            trailing_break_markers,
            trailing_break_on_whitespace,
        )
    {
        bytes.extend_from_slice(newline);
    }
    bytes
}

/// Whether `bytes` ends with a hard-line-break marker — a bare break
/// literal (the `\` of `markdown_inline`'s `hard_line_break`) or, when the
/// grammar's break idiom admits it, trailing whitespace. Appending the
/// customary end-of-output newline after such a tail would manufacture a
/// phantom line-break node on re-parse, so the caller suppresses it.
fn ends_with_trailing_break_marker(bytes: &[u8], markers: &[String], on_whitespace: bool) -> bool {
    if markers.is_empty() && !on_whitespace {
        return false;
    }
    if on_whitespace && bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t') {
        return true;
    }
    markers.iter().any(|m| bytes.ends_with(m.as_bytes()))
}

/// True when the negated character class `[^<negated>]` ADMITS `c` — i.e.
/// `c` is not one of the excluded characters. `negated` is the inner text
/// of the class (the part after `[^`, before `]`), with backslash escapes
/// (`\s`, `\t`, `\n`, `\\`) and literal members. A greedy `[^...]+`
/// terminal continues to consume any admitted character, so an admitted
/// leading char on the following token would be swallowed on re-parse.
fn negated_class_admits(negated: &str, c: char) -> bool {
    let mut chars = negated.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let excluded = match chars.next() {
                Some('s') => c.is_whitespace(),
                Some('t') => c == '\t',
                Some('n') => c == '\n',
                Some('r') => c == '\r',
                Some(esc) => c == esc,
                None => false,
            };
            if excluded {
                return false;
            }
        } else if ch == c {
            return false;
        }
    }
    true
}

/// Effective spacing role: word-like bracket tokens (`function`, `end`,
/// `begin`, `done`, etc.) are structurally brackets (for indentation)
/// but space like keywords (they need whitespace on both sides).
pub(crate) fn effective_spacing_role(role: TokenRole, text: &str) -> TokenRole {
    match role {
        TokenRole::BracketOpen | TokenRole::BracketClose if is_word_like(text) => {
            TokenRole::Keyword
        }
        other => other,
    }
}

/// Role-pair spacing table. Determines whether a space separator
/// should be inserted between two adjacent tokens based on their
/// structural roles and word-likeness.
pub(crate) fn needs_space_by_role(
    last: TokenRole,
    last_text: &str,
    next: TokenRole,
    next_text: &str,
) -> bool {
    let last = effective_spacing_role(last, last_text);
    let next = effective_spacing_role(next, next_text);
    match (last, next) {
        // Immediate (IMMEDIATE_TOKEN) tokens are lexically glued to
        // their neighbours on both sides (`0.5`, not `0 . 5`).
        (TokenRole::Immediate, _) | (_, TokenRole::Immediate) => false,
        // Brackets: tight on the inside
        (TokenRole::BracketOpen, _) | (_, TokenRole::BracketClose) => false,
        // Separators: tight before, space after
        (_, TokenRole::Separator) => false,
        (TokenRole::Separator, _) => true,
        // Connectors: always tight (., ::, ->, etc.)
        (TokenRole::Connector, _) | (_, TokenRole::Connector) => false,
        // Terminal followed by bracket-open: tight (f() not f ())
        (TokenRole::Terminal, TokenRole::BracketOpen) => false,
        // Close followed by open: tight
        (TokenRole::BracketClose, TokenRole::BracketOpen) => false,
        // Keywords always spaced
        (TokenRole::Keyword, _) | (_, TokenRole::Keyword) => true,
        // Terminals and operators: space between
        (TokenRole::Terminal, TokenRole::Terminal) => true,
        (TokenRole::Terminal, TokenRole::Operator) | (TokenRole::Operator, TokenRole::Terminal) => {
            true
        }
        (TokenRole::Operator, TokenRole::Operator) => true,
        // Close followed by non-bracket: space
        (TokenRole::BracketClose, _) => true,
        // Operator before open: space
        (TokenRole::Operator, TokenRole::BracketOpen) => true,
    }
}
