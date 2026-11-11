//! The `PostgreSQL` 17 lexical structure (Sec 4.1 of its documentation),
//! split into statements.
//!
//! Comments and whitespace separate tokens and are dropped. Unquoted words
//! are folded to ASCII lowercase, as `PostgreSQL` folds them; quoted
//! identifiers keep their bytes. String constants are decoded where their
//! value is needed (standard and `E` strings, with continuation across a
//! line break); other constants keep only their extent, since the reader
//! carries every expression as the source text that spells it.

use super::error::SqlDdlError;

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// An unquoted word: a keyword or an identifier, folded to lowercase.
    Word,
    /// A double-quoted identifier.
    Quoted,
    /// A string constant of any form, or a dollar-quoted string.
    Str,
    /// A numeric constant.
    Num,
    /// An operator, `::` included.
    Op,
    /// One of `( ) , [ ] . :`.
    Punct,
    /// A positional parameter, `$1`.
    Param,
}

/// One token.
#[derive(Clone, Debug)]
pub(super) struct Token {
    /// What the token is.
    pub kind: Kind,
    /// Its first byte in the script.
    pub start: usize,
    /// The byte after its last.
    pub end: usize,
    /// For a word, its folded spelling; for a quoted identifier, the name;
    /// for a standard or `E` string constant, its value; for an operator or
    /// punctuation, the text; otherwise empty.
    pub text: String,
    /// For a string constant, whether `text` holds its decoded value.
    pub decoded: bool,
}

impl Token {
    /// Whether the token is the unquoted word `word` (given in lowercase).
    pub fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text == word
    }

    /// Whether the token is the punctuation or operator `p`.
    pub fn is_punct(&self, p: &str) -> bool {
        matches!(self.kind, Kind::Punct | Kind::Op) && self.text == p
    }
}

/// The tokens of one statement, up to its `;`.
#[derive(Debug)]
pub(super) struct Statement {
    /// Its tokens, never empty.
    pub tokens: Vec<Token>,
    /// The byte after its last token, where the statement ends.
    pub end: usize,
}

/// Splits a script into statements.
pub(super) struct Lexer<'a> {
    src: &'a str,
    at: usize,
}

/// The longest identifier `PostgreSQL` keeps whole (`NAMEDATALEN - 1`).
pub(super) const MAX_IDENTIFIER_BYTES: usize = 63;

const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

const fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

const fn is_op_char(b: u8) -> bool {
    matches!(
        b,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'<'
            | b'>'
            | b'='
            | b'~'
            | b'!'
            | b'@'
            | b'#'
            | b'%'
            | b'^'
            | b'&'
            | b'|'
            | b'?'
    )
}

const fn lex_error(offset: usize, reason: &'static str) -> SqlDdlError {
    SqlDdlError::Lex { offset, reason }
}

fn unsupported(offset: usize, feature: &str) -> SqlDdlError {
    SqlDdlError::Unsupported {
        offset,
        feature: feature.to_owned(),
    }
}

impl<'a> Lexer<'a> {
    /// A lexer at the start of `src`.
    pub const fn new(src: &'a str) -> Self {
        Self { src, at: 0 }
    }

    /// The next statement, or `None` at the end of the script.
    ///
    /// The data rows that follow `COPY ... FROM stdin;`, up to the `\.`
    /// line that ends them, belong to that statement and are skipped.
    pub fn statement(&mut self) -> Result<Option<Statement>, SqlDdlError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_space()?;
            let bytes = self.src.as_bytes();
            if self.at >= bytes.len() {
                let end = tokens.last().map_or(self.at, |t: &Token| t.end);
                return Ok((!tokens.is_empty()).then_some(Statement { tokens, end }));
            }
            if bytes[self.at] == b';' {
                let end = self.at;
                self.at += 1;
                if tokens.is_empty() {
                    continue;
                }
                if is_copy_from_stdin(&tokens) {
                    self.skip_copy_data();
                }
                return Ok(Some(Statement { tokens, end }));
            }
            tokens.push(self.token()?);
        }
    }

    /// Skip whitespace and comments.
    fn skip_space(&mut self) -> Result<(), SqlDdlError> {
        let bytes = self.src.as_bytes();
        loop {
            while self.at < bytes.len() && bytes[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if bytes[self.at..].starts_with(b"--") {
                self.at = self.src[self.at..]
                    .find('\n')
                    .map_or(bytes.len(), |n| self.at + n);
            } else if bytes[self.at..].starts_with(b"/*") {
                let open = self.at;
                let mut depth = 0usize;
                loop {
                    if self.at >= bytes.len() {
                        return Err(lex_error(open, "unterminated block comment"));
                    }
                    if bytes[self.at..].starts_with(b"/*") {
                        depth += 1;
                        self.at += 2;
                    } else if bytes[self.at..].starts_with(b"*/") {
                        depth -= 1;
                        self.at += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        self.at += 1;
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Skip the rows of a `COPY ... FROM stdin`: from the line after the
    /// statement through the line `\.`.
    fn skip_copy_data(&mut self) {
        let src = self.src;
        let mut at = src[self.at..]
            .find('\n')
            .map_or(src.len(), |n| self.at + n + 1);
        while at < src.len() {
            let end = src[at..].find('\n').map_or(src.len(), |n| at + n);
            let line = &src[at..end];
            at = (end + 1).min(src.len());
            if line.trim_end_matches('\r') == "\\." {
                break;
            }
        }
        self.at = at;
    }

    const fn make(&self, kind: Kind, start: usize, text: String) -> Token {
        Token {
            kind,
            start,
            end: self.at,
            text,
            decoded: false,
        }
    }

    /// The token at the current position, which is not whitespace, a
    /// comment or `;`.
    fn token(&mut self) -> Result<Token, SqlDdlError> {
        let bytes = self.src.as_bytes();
        let start = self.at;
        let b = bytes[start];
        let next = bytes.get(start + 1).copied();
        match b {
            b'\'' => self.string(start, start, StringForm::Standard),
            b'"' => self.quoted_identifier(start),
            b'`' => Err(unsupported(start, "MySQL backquoted identifier")),
            b'\\' => Err(lex_error(
                start,
                "a backslash outside a literal (a psql meta-command)",
            )),
            b'$' => self.dollar(start),
            b'(' | b')' | b',' | b'[' | b']' => {
                self.at += 1;
                Ok(self.make(Kind::Punct, start, (b as char).to_string()))
            }
            b':' if next == Some(b':') => {
                self.at += 2;
                Ok(self.make(Kind::Op, start, "::".to_owned()))
            }
            b':' => {
                self.at += 1;
                Ok(self.make(Kind::Punct, start, ":".to_owned()))
            }
            b'.' if next.is_some_and(|n| n.is_ascii_digit()) => self.number(start),
            b'.' => {
                self.at += 1;
                Ok(self.make(Kind::Punct, start, ".".to_owned()))
            }
            b'0'..=b'9' => self.number(start),
            b if is_op_char(b) => {
                let mut end = start;
                while end < bytes.len()
                    && is_op_char(bytes[end])
                    && !bytes[end..].starts_with(b"--")
                    && !bytes[end..].starts_with(b"/*")
                {
                    end += 1;
                }
                self.at = end;
                Ok(self.make(Kind::Op, start, self.src[start..end].to_owned()))
            }
            b if is_ident_start(b) => {
                let quote_after = |prefix: usize| bytes.get(start + prefix) == Some(&b'\'');
                match b.to_ascii_lowercase() {
                    b'e' if quote_after(1) => self.string(start, start + 1, StringForm::Escape),
                    b'n' if quote_after(1) => self.string(start, start + 1, StringForm::Standard),
                    b'b' | b'x' if quote_after(1) => {
                        self.string(start, start + 1, StringForm::Opaque)
                    }
                    b'u' if next == Some(b'&') && quote_after(2) => Err(unsupported(
                        start,
                        "Unicode escape string constant (U&'...')",
                    )),
                    b'u' if next == Some(b'&') && bytes.get(start + 2) == Some(&b'"') => {
                        Err(unsupported(start, "Unicode escape identifier (U&\"...\")"))
                    }
                    _ => {
                        let mut end = start;
                        while end < bytes.len() && is_ident_continue(bytes[end]) {
                            end += 1;
                        }
                        self.at = end;
                        let word = self.src[start..end].to_ascii_lowercase();
                        Ok(self.make(Kind::Word, start, word))
                    }
                }
            }
            _ => Err(lex_error(start, "a character that starts no SQL token")),
        }
    }

    fn number(&mut self, start: usize) -> Result<Token, SqlDdlError> {
        let bytes = self.src.as_bytes();
        let mut end = start;
        let digits = |end: &mut usize| {
            while *end < bytes.len() && (bytes[*end].is_ascii_digit() || bytes[*end] == b'_') {
                *end += 1;
            }
        };
        if bytes[start] == b'0'
            && matches!(
                bytes.get(start + 1),
                Some(b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            )
        {
            end = start + 2;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
        } else {
            digits(&mut end);
            if bytes.get(end) == Some(&b'.') && bytes.get(end + 1) != Some(&b'.') {
                end += 1;
                digits(&mut end);
            }
            if matches!(bytes.get(end), Some(b'e' | b'E')) {
                let mut exp = end + 1;
                if matches!(bytes.get(exp), Some(b'+' | b'-')) {
                    exp += 1;
                }
                if bytes.get(exp).is_some_and(u8::is_ascii_digit) {
                    end = exp;
                    digits(&mut end);
                }
            }
        }
        if bytes.get(end).is_some_and(|b| is_ident_continue(*b)) {
            return Err(lex_error(end, "trailing junk after a numeric constant"));
        }
        self.at = end;
        Ok(self.make(Kind::Num, start, self.src[start..end].to_owned()))
    }

    fn quoted_identifier(&mut self, start: usize) -> Result<Token, SqlDdlError> {
        let bytes = self.src.as_bytes();
        let mut name = String::new();
        let mut j = start + 1;
        let mut run = j;
        loop {
            let Some(&b) = bytes.get(j) else {
                return Err(lex_error(start, "unterminated quoted identifier"));
            };
            if b == b'"' {
                name.push_str(&self.src[run..j]);
                if bytes.get(j + 1) == Some(&b'"') {
                    name.push('"');
                    j += 2;
                    run = j;
                } else {
                    j += 1;
                    break;
                }
            } else {
                j += 1;
            }
        }
        if name.is_empty() {
            return Err(lex_error(start, "zero-length quoted identifier"));
        }
        self.at = j;
        Ok(self.make(Kind::Quoted, start, name))
    }

    /// A positional parameter or a dollar-quoted string.
    fn dollar(&mut self, start: usize) -> Result<Token, SqlDdlError> {
        let bytes = self.src.as_bytes();
        let mut j = start + 1;
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            self.at = j;
            return Ok(self.make(Kind::Param, start, String::new()));
        }
        if bytes.get(j).is_some_and(|b| is_ident_start(*b)) {
            while bytes
                .get(j)
                .is_some_and(|b| is_ident_continue(*b) && *b != b'$')
            {
                j += 1;
            }
        }
        if bytes.get(j) != Some(&b'$') {
            return Err(lex_error(start, "a `$` that opens no dollar-quoted string"));
        }
        let tag = &self.src[start..=j];
        let body = j + 1;
        let close = self.src[body..]
            .find(tag)
            .ok_or_else(|| lex_error(start, "unterminated dollar-quoted string"))?;
        self.at = body + close + tag.len();
        let mut token = self.make(Kind::Str, start, self.src[body..body + close].to_owned());
        token.decoded = true;
        Ok(token)
    }

    /// A string constant whose opening quote is at `quote`, continued
    /// across whitespace holding a line break by a further `'...'`.
    fn string(
        &mut self,
        start: usize,
        quote: usize,
        form: StringForm,
    ) -> Result<Token, SqlDdlError> {
        let mut value = Vec::new();
        let mut at = quote;
        loop {
            at = self.string_part(start, at, form, &mut value)?;
            // A further segment continues the constant only across a
            // line break (Sec 4.1.2.2).
            let rest = &self.src.as_bytes()[at..];
            let gap = rest
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or(rest.len());
            if rest.get(gap) == Some(&b'\'') && rest[..gap].contains(&b'\n') {
                at += gap;
            } else {
                break;
            }
        }
        self.at = at;
        let decoded = form != StringForm::Opaque;
        let text = if decoded {
            String::from_utf8(value)
                .map_err(|_| lex_error(start, "an escape that gives invalid UTF-8"))?
        } else {
            String::new()
        };
        let mut token = self.make(Kind::Str, start, text);
        token.decoded = decoded;
        Ok(token)
    }

    /// Read one `'...'` segment opening at `quote` into `value`; the byte
    /// after its closing quote.
    fn string_part(
        &self,
        start: usize,
        quote: usize,
        form: StringForm,
        value: &mut Vec<u8>,
    ) -> Result<usize, SqlDdlError> {
        let bytes = self.src.as_bytes();
        let mut j = quote + 1;
        loop {
            let Some(&b) = bytes.get(j) else {
                return Err(lex_error(start, "unterminated string constant"));
            };
            match b {
                b'\'' if bytes.get(j + 1) == Some(&b'\'') => {
                    value.push(b'\'');
                    j += 2;
                }
                b'\'' => return Ok(j + 1),
                b'\\' if form == StringForm::Escape => {
                    j = escape(bytes, j, value).ok_or_else(|| {
                        lex_error(j, "an invalid backslash escape in an E string")
                    })?;
                }
                _ => {
                    value.push(b);
                    j += 1;
                }
            }
        }
    }
}

/// How a string constant's body is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StringForm {
    /// `'...'` and `N'...'`: `''` is a quote, a backslash is itself.
    Standard,
    /// `E'...'`: backslash escapes as well.
    Escape,
    /// `B'...'` and `X'...'`: bit strings, whose value is not decoded.
    Opaque,
}

/// Decode the backslash escape at `j` (Sec 4.1.2.2, table 4.1) into
/// `value`; the byte after it.
fn escape(bytes: &[u8], j: usize, value: &mut Vec<u8>) -> Option<usize> {
    let c = *bytes.get(j + 1)?;
    let radix_run = |from: usize, radix: u32, max: usize| {
        let mut n = 0u32;
        let mut k = from;
        while k < from + max && bytes.get(k).is_some_and(|b| (*b as char).is_digit(radix)) {
            n = n * radix + (bytes[k] as char).to_digit(radix)?;
            k += 1;
        }
        (k > from).then_some((n, k))
    };
    let simple = |b: u8, value: &mut Vec<u8>| {
        value.push(b);
        Some(j + 2)
    };
    match c {
        b'b' => simple(0x08, value),
        b'f' => simple(0x0c, value),
        b'n' => simple(b'\n', value),
        b'r' => simple(b'\r', value),
        b't' => simple(b'\t', value),
        b'0'..=b'7' => {
            let (n, k) = radix_run(j + 1, 8, 3)?;
            value.push(u8::try_from(n).ok()?);
            Some(k)
        }
        b'x' => {
            let (n, k) = radix_run(j + 2, 16, 2)?;
            value.push(u8::try_from(n).ok()?);
            Some(k)
        }
        b'u' | b'U' => {
            let width = if c == b'u' { 4 } else { 8 };
            let (n, k) = radix_run(j + 2, 16, width)?;
            if k - (j + 2) != width {
                return None;
            }
            let ch = char::from_u32(n)?;
            value.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            Some(k)
        }
        _ => {
            // Any other character is taken literally, a multi-byte one whole.
            let ch = std::str::from_utf8(&bytes[j + 1..]).ok()?.chars().next()?;
            let width = ch.len_utf8();
            value.extend_from_slice(&bytes[j + 1..j + 1 + width]);
            Some(j + 1 + width)
        }
    }
}

/// Whether the statement is `COPY ... FROM stdin`, which data rows follow.
fn is_copy_from_stdin(tokens: &[Token]) -> bool {
    tokens.first().is_some_and(|t| t.is_word("copy"))
        && tokens
            .windows(2)
            .any(|w| w[0].is_word("from") && w[1].is_word("stdin"))
}
