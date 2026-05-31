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

use super::{TokenRole, Grammar, is_word_like};


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

        let role = explicit_role.unwrap_or_else(|| self.lookup_role(value));

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
        }
    }

    pub(crate) fn restore(&mut self, snap: OutputSnapshot) {
        self.tokens.truncate(snap.tokens_len);
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
            .any(|t| matches!(t, Token::Lit(_, _)))
    }

    /// Push a marker that suppresses the next inter-Lit separator the
    /// layout pass would otherwise insert. Used to encode "no source-
    /// level separator was emitted between these two Lits" without
    /// having to make per-grammar adjacency decisions in the layout.
    pub(crate) fn no_space(&mut self) {
        self.tokens.push(Token::NoSpace);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        layout(
            &self.tokens,
            self.policy,
            &self.grammar.line_comment_prefixes,
        )
    }
}


/// Fold a token list into bytes. The algebra:
/// * adjacent `Lit`s get a single space iff `needs_space_between(a, b)`,
/// * `IndentOpen` / `IndentClose` adjust a depth counter,
/// * `LineBreak` writes `\n` if not already at line start, then the
///   next `Lit` writes `indent * indent_width` spaces of indent.
pub(crate) fn layout(tokens: &[Token], policy: &FormatPolicy, line_comment_prefixes: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut indent: usize = 0;
    let mut at_line_start = true;
    let mut last_role: Option<TokenRole> = None;
    let mut last_text: String = String::new();
    let mut suppress_next_separator = false;
    let mut force_next_separator = false;
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
            }
        }
        match tok {
            Token::IndentOpen => indent += 1,
            Token::IndentClose => {
                indent = indent.saturating_sub(1);
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                }
            }
            Token::LineBreak => {
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
            Token::Lit(value, role) => {
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
                    // An explicit NoSpace (suppress) is authoritative: it
                    // records that the source had no separator at this
                    // boundary (an empty REPEAT separator slot, an
                    // IMMEDIATE_TOKEN). It overrides the sibling-separation
                    // ForceSpace heuristic — otherwise beamed notes
                    // (`CDEF`) re-space to `C D E F`.
                    let want_space = !suppress_next_separator
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
                last_role = Some(*role);
                last_text.clear();
                last_text.push_str(value);
                if line_comment_prefixes
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

    if !at_line_start {
        bytes.extend_from_slice(newline);
    }
    bytes
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
pub(crate) fn needs_space_by_role(last: TokenRole, last_text: &str, next: TokenRole, next_text: &str) -> bool {
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
