//! RDF 1.1 Turtle, transcribed from the grammar of the Recommendation.
//!
//! The grammar is that of Sec 6.5, read over characters, with the IRI
//! resolution of RFC 3986 Sec 5.2, the canonical Turtle the `rdf-formats`
//! page defines, and the edit behaviour of the format-preserving route.
//!
//! The reader tokenizes by longest match at each position, as Sec 6.5
//! requires, and reads the productions `turtleDoc`, `statement`,
//! `directive`, `triples`, `predicateObjectList`, `objectList`, `verb`,
//! `subject`, `object`, `literal`, `blankNodePropertyList`, `collection`
//! and the terminals over characters. It shares no code with production,
//! which parses with the tree-sitter Turtle grammar. Beyond the grammar it
//! applies the page's stated refusals: a raw U+0000 or U+007F anywhere, an
//! IRI whose escapes decode to a character `IRIREF` excludes, a relative
//! reference with no base IRI in force, and an undefined prefix.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::ntriples::{
    RDF_LANG_STRING, Term, Triple, XSD_STRING, absolute, excluded, pn_chars_base, quote, term_text,
};

/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdf:first`.
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
/// `rdf:rest`.
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// `rdf:nil`.
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
/// `xsd:integer`.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// `xsd:decimal`.
pub const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// `xsd:double`.
pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
/// `xsd:boolean`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

// ── RFC 3986 Sec 5.2 ────────────────────────────────────────────────

/// A reference split by the regular expression of RFC 3986 Appendix B,
/// `^(([^:/?#]+):)?(//([^/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?`, with the
/// scheme kept only when it has the form of Sec 3.1.
struct Split {
    scheme: Option<String>,
    authority: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

fn split(reference: &str) -> Split {
    let chars: Vec<char> = reference.chars().collect();
    let n = chars.len();
    let take = |from: usize, stop: &[char]| {
        let mut to = from;
        while to < n && !stop.contains(&chars[to]) {
            to += 1;
        }
        (chars[from..to].iter().collect::<String>(), to)
    };
    let mut at = 0;
    let (candidate, end) = take(0, &[':', '/', '?', '#']);
    let scheme = if end < n && chars[end] == ':' && absolute(&format!("{candidate}:")) {
        at = end + 1;
        Some(candidate)
    } else {
        None
    };
    let authority = if chars.get(at) == Some(&'/') && chars.get(at + 1) == Some(&'/') {
        let (authority, end) = take(at + 2, &['/', '?', '#']);
        at = end;
        Some(authority)
    } else {
        None
    };
    let (path, end) = take(at, &['?', '#']);
    at = end;
    let query = if chars.get(at) == Some(&'?') {
        let (query, end) = take(at + 1, &['#']);
        at = end;
        Some(query)
    } else {
        None
    };
    let fragment = (chars.get(at) == Some(&'#')).then(|| chars[at + 1..].iter().collect());
    Split {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

/// `remove_dot_segments` of RFC 3986 Sec 5.2.4, its steps A to E applied
/// to an input buffer of characters until it is empty.
fn remove_dot_segments(path: &str) -> String {
    let mut input: Vec<char> = path.chars().collect();
    let mut output: Vec<char> = Vec::new();
    let starts = |input: &[char], prefix: &str| {
        let prefix: Vec<char> = prefix.chars().collect();
        input.starts_with(&prefix)
    };
    let is = |input: &[char], whole: &str| input.iter().copied().eq(whole.chars());
    while !input.is_empty() {
        if starts(&input, "../") {
            input.drain(..3);
        } else if starts(&input, "./") || starts(&input, "/./") {
            input.drain(..2);
        } else if is(&input, "/.") {
            input = vec!['/'];
        } else if starts(&input, "/../") || is(&input, "/..") {
            if is(&input, "/..") {
                input = vec!['/'];
            } else {
                input.drain(..3);
            }
            while let Some(c) = output.pop() {
                if c == '/' {
                    break;
                }
            }
        } else if is(&input, ".") || is(&input, "..") {
            input.clear();
        } else {
            let mut segment = usize::from(input[0] == '/');
            while segment < input.len() && input[segment] != '/' {
                segment += 1;
            }
            output.extend(input.drain(..segment));
        }
    }
    output.into_iter().collect()
}

/// The resolution of `reference` against the absolute IRI `base`: the
/// algorithm of RFC 3986 Sec 5.2.2, the merge of Sec 5.2.3 and the
/// recomposition of Sec 5.3, with the strict parser.
#[must_use]
pub fn resolve(base: &str, reference: &str) -> String {
    let r = split(reference);
    let b = split(base);
    let t = if r.scheme.is_some() {
        Split {
            path: remove_dot_segments(&r.path),
            ..r
        }
    } else if r.authority.is_some() {
        Split {
            scheme: b.scheme,
            path: remove_dot_segments(&r.path),
            ..r
        }
    } else if r.path.is_empty() {
        Split {
            scheme: b.scheme,
            authority: b.authority,
            path: b.path,
            query: r.query.or(b.query),
            fragment: r.fragment,
        }
    } else {
        let path = if r.path.starts_with('/') {
            remove_dot_segments(&r.path)
        } else {
            let merged = if b.authority.is_some() && b.path.is_empty() {
                format!("/{}", r.path)
            } else {
                match b.path.rfind('/') {
                    Some(i) => format!("{}{}", &b.path[..=i], r.path),
                    None => r.path,
                }
            };
            remove_dot_segments(&merged)
        };
        Split {
            scheme: b.scheme,
            authority: b.authority,
            path,
            query: r.query,
            fragment: r.fragment,
        }
    };
    let mut out = String::new();
    if let Some(scheme) = t.scheme {
        out += &scheme;
        out.push(':');
    }
    if let Some(authority) = t.authority {
        out += "//";
        out += &authority;
    }
    out += &t.path;
    if let Some(query) = t.query {
        out.push('?');
        out += &query;
    }
    if let Some(fragment) = t.fragment {
        out.push('#');
        out += &fragment;
    }
    out
}

// ── Characters ─────────────────────────────────────────────────────────

/// `PN_CHARS_U` of Turtle: no `:`, unlike N-Triples.
fn pn_chars_u(c: char) -> bool {
    pn_chars_base(c) || c == '_'
}

fn pn_chars(c: char) -> bool {
    let n = u32::from(c);
    pn_chars_u(c)
        || c == '-'
        || c.is_ascii_digit()
        || n == 0xB7
        || (0x300..=0x36F).contains(&n)
        || (0x203F..=0x2040).contains(&n)
}

const LOCAL_ESCAPES: &str = "_~.-!$&'()*+,;=/?#@%";

// ── Reading ────────────────────────────────────────────────────────────

/// A term of a document, as the edit behaviour keys it: an RDF term or a
/// language tag.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// An RDF term.
    Term(Term),
    /// A language tag.
    Language(String),
}

/// How a token writes its term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// An IRI reference, a prefixed name or `a`.
    Iri,
    /// A blank node label.
    Blank,
    /// A literal's quoted string.
    Quoted,
    /// A number or boolean, with the datatype it implies.
    Bare(String),
    /// A language tag.
    Tag,
}

/// A token that writes a term: its key, its byte span and its spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The term.
    pub key: Key,
    /// The byte offset of its first byte.
    pub start: usize,
    /// The byte offset after its last byte.
    pub end: usize,
    /// How it writes the term.
    pub spelling: Spelling,
}

/// What a document reads as: its triples, the tokens that write its terms,
/// and the terms it also writes with no token of their own.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Reading {
    /// The triples, in the order the reading completes them.
    pub triples: Vec<Triple>,
    /// The term tokens, in document order of completion.
    pub tokens: Vec<Token>,
    /// The terms written somewhere without a token.
    pub implicit: BTreeSet<Key>,
}

/// A failed reading: the character position of the fault.
type Fault = usize;

struct Reader {
    chars: Vec<char>,
    /// The byte offset of each character, and of the end.
    bytes: Vec<usize>,
    at: usize,
    base: Option<String>,
    prefixes: HashMap<String, String>,
    used: HashSet<String>,
    labels: BTreeSet<String>,
    counter: u64,
    reading: Reading,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    fn looking_at(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }

    /// Skip white space and comments.
    fn skip(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r') => self.at += 1,
                Some('#') => {
                    while !matches!(self.peek(), None | Some('\n' | '\r')) {
                        self.at += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn hex(&mut self, digits: usize) -> Result<char, Fault> {
        let start = self.at;
        let mut code = 0u32;
        for _ in 0..digits {
            let d = self.peek().and_then(|c| c.to_digit(16)).ok_or(start)?;
            code = code * 16 + d;
            self.at += 1;
        }
        char::from_u32(code).ok_or(start)
    }

    fn uchar(&mut self) -> Result<char, Fault> {
        match self.peek() {
            Some('u') => {
                self.at += 1;
                self.hex(4)
            }
            Some('U') => {
                self.at += 1;
                self.hex(8)
            }
            _ => Err(self.at),
        }
    }

    fn fresh(&mut self) -> Term {
        loop {
            let label = format!("b{}", self.counter);
            self.counter += 1;
            if !self.used.contains(&label) {
                self.reading
                    .implicit
                    .insert(Key::Term(Term::Blank(label.clone())));
                return Term::Blank(label);
            }
        }
    }

    fn implicit_iri(&mut self, iri: &str) {
        self.reading
            .implicit
            .insert(Key::Term(Term::Iri(iri.to_owned())));
    }

    fn token(&mut self, key: Key, start: usize, spelling: Spelling) {
        let token = Token {
            key,
            start: self.bytes[start],
            end: self.bytes[self.at],
            spelling,
        };
        self.reading.tokens.push(token);
    }

    /// `IRIREF`, decoded and resolved.
    fn iriref(&mut self) -> Result<String, Fault> {
        let start = self.at;
        if self.peek() != Some('<') {
            return Err(start);
        }
        self.at += 1;
        let mut iri = String::new();
        loop {
            let c = self.peek().ok_or(self.at)?;
            self.at += 1;
            let decoded = match c {
                '>' => break,
                '\\' => self.uchar()?,
                _ => c,
            };
            if excluded(decoded) {
                return Err(start);
            }
            iri.push(decoded);
        }
        if absolute(&iri) {
            Ok(iri)
        } else {
            self.base
                .as_ref()
                .map(|base| resolve(base, &iri))
                .ok_or(start)
        }
    }

    /// `PN_PREFIX`, if one starts here; the position is left after it.
    fn pn_prefix(&mut self) -> String {
        let mut prefix = String::new();
        if !self.peek().is_some_and(pn_chars_base) {
            return prefix;
        }
        while let Some(c) = self.peek().filter(|&c| pn_chars(c) || c == '.') {
            prefix.push(c);
            self.at += 1;
        }
        while prefix.ends_with('.') {
            prefix.pop();
            self.at -= 1;
        }
        prefix
    }

    /// `PLX`, if one starts here: its decoded text.
    fn plx(&mut self) -> Option<String> {
        match (self.peek(), self.peek_at(1), self.peek_at(2)) {
            (Some('%'), Some(a), Some(b)) if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => {
                self.at += 3;
                Some(format!("%{a}{b}"))
            }
            (Some('\\'), Some(c), _) if LOCAL_ESCAPES.contains(c) => {
                self.at += 2;
                Some(c.to_string())
            }
            _ => None,
        }
    }

    /// `PN_LOCAL`, decoded (possibly empty).
    ///
    /// Each piece is one character or one `PLX`; a trailing run of
    /// unescaped dots is given back, since `PN_LOCAL` cannot end in one.
    fn pn_local(&mut self) -> String {
        // Each piece: the position after it, its decoded text, and whether
        // it is an unescaped dot.
        let mut pieces: Vec<(usize, String, bool)> = Vec::new();
        let start = self.at;
        loop {
            let first = pieces.is_empty();
            if let Some(text) = self.plx() {
                pieces.push((self.at, text, false));
                continue;
            }
            let Some(c) = self.peek() else { break };
            let allowed = if first {
                pn_chars_u(c) || c == ':' || c.is_ascii_digit()
            } else {
                pn_chars(c) || c == '.' || c == ':'
            };
            if !allowed {
                break;
            }
            self.at += 1;
            pieces.push((self.at, c.to_string(), c == '.'));
        }
        while pieces.last().is_some_and(|(_, _, dot)| *dot) {
            pieces.pop();
        }
        self.at = pieces.last().map_or(start, |(end, _, _)| *end);
        pieces.into_iter().map(|(_, text, _)| text).collect()
    }

    /// A prefixed name, if one starts here: its IRI.
    fn prefixed_name(&mut self) -> Result<Option<String>, Fault> {
        let start = self.at;
        let prefix = self.pn_prefix();
        if self.peek() != Some(':') {
            self.at = start;
            return Ok(None);
        }
        self.at += 1;
        let local = self.pn_local();
        let namespace = self.prefixes.get(&prefix).ok_or(start)?;
        Ok(Some(format!("{namespace}{local}")))
    }

    /// An `iri` term, recorded.
    fn iri(&mut self) -> Result<String, Fault> {
        let start = self.at;
        let iri = if self.peek() == Some('<') {
            self.iriref()?
        } else {
            self.prefixed_name()?.ok_or(start)?
        };
        self.token(Key::Term(Term::Iri(iri.clone())), start, Spelling::Iri);
        Ok(iri)
    }

    fn blank_label(&mut self) -> Result<Term, Fault> {
        let start = self.at;
        self.at += 2;
        let first = self.peek().ok_or(start)?;
        if !(pn_chars_u(first) || first.is_ascii_digit()) {
            return Err(start);
        }
        let mut label = String::from(first);
        self.at += 1;
        while let Some(c) = self.peek().filter(|&c| pn_chars(c) || c == '.') {
            label.push(c);
            self.at += 1;
        }
        while label.ends_with('.') {
            label.pop();
            self.at -= 1;
        }
        self.labels.insert(label.clone());
        let term = Term::Blank(label);
        self.token(Key::Term(term.clone()), start, Spelling::Blank);
        Ok(term)
    }

    /// The length of the `ANON` that starts here, if one does.
    fn anon(&self) -> Option<usize> {
        let mut i = 1;
        while matches!(self.peek_at(i), Some(' ' | '\t' | '\n' | '\r')) {
            i += 1;
        }
        (self.peek() == Some('[') && self.peek_at(i) == Some(']')).then_some(i + 1)
    }

    /// A fresh blank node for the `ANON` of length `len` that starts here.
    fn read_anon(&mut self, len: usize) -> Term {
        self.at += len;
        self.fresh()
    }

    fn subject(&mut self) -> Result<Term, Fault> {
        match self.peek() {
            Some('_') if self.peek_at(1) == Some(':') => self.blank_label(),
            Some('[') => match self.anon() {
                Some(len) => Ok(self.read_anon(len)),
                None => Err(self.at),
            },
            Some('(') => self.collection(),
            _ => self.iri().map(Term::Iri),
        }
    }

    fn verb(&mut self) -> Result<String, Fault> {
        let start = self.at;
        if self.peek() == Some('a') {
            let save = self.at;
            let name = self.prefixed_name();
            self.at = save;
            if matches!(name, Ok(None)) {
                self.at += 1;
                self.token(
                    Key::Term(Term::Iri(RDF_TYPE.to_owned())),
                    start,
                    Spelling::Iri,
                );
                return Ok(RDF_TYPE.to_owned());
            }
        }
        self.iri()
    }

    /// The longest numeric token at the position, if any, with its datatype.
    fn number(&self) -> Option<(usize, &'static str)> {
        let digits = |mut i: usize| {
            let from = i;
            while self.peek_at(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            (i, i > from)
        };
        let exponent = |i: usize| {
            if !matches!(self.peek_at(i), Some('e' | 'E')) {
                return None;
            }
            let mut j = i + 1;
            if matches!(self.peek_at(j), Some('+' | '-')) {
                j += 1;
            }
            let (end, any) = digits(j);
            any.then_some(end)
        };
        let sign = usize::from(matches!(self.peek(), Some('+' | '-')));
        let (whole_end, whole) = digits(sign);
        let mut best = whole.then_some((whole_end, XSD_INTEGER));
        if self.peek_at(whole_end) == Some('.') {
            let (fraction_end, fraction) = digits(whole_end + 1);
            if fraction {
                best = Some((fraction_end, XSD_DECIMAL));
            }
            if (whole || fraction)
                && let Some(end) = exponent(fraction_end)
            {
                best = Some((end, XSD_DOUBLE));
            }
        } else if whole && let Some(end) = exponent(whole_end) {
            best = Some((end, XSD_DOUBLE));
        }
        best
    }

    /// One of the four string forms, decoded.
    fn string(&mut self) -> Result<String, Fault> {
        let start = self.at;
        let quote = self.peek().ok_or(start)?;
        let long = self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote);
        self.at += if long { 3 } else { 1 };
        let mut text = String::new();
        loop {
            let c = self.peek().ok_or(start)?;
            if c == quote {
                if !long {
                    self.at += 1;
                    return Ok(text);
                }
                // A long string's body holds no three quotes in a row, so
                // the first three close it.
                if self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote) {
                    self.at += 3;
                    return Ok(text);
                }
            }
            if !long && matches!(c, '\n' | '\r') {
                return Err(self.at);
            }
            self.at += 1;
            if c != '\\' {
                text.push(c);
                continue;
            }
            let escaped = match self.peek().ok_or(self.at)? {
                't' => '\t',
                'b' => '\u{8}',
                'n' => '\n',
                'r' => '\r',
                'f' => '\u{C}',
                '"' => '"',
                '\'' => '\'',
                '\\' => '\\',
                _ => {
                    text.push(self.uchar()?);
                    continue;
                }
            };
            self.at += 1;
            text.push(escaped);
        }
    }

    fn langtag(&mut self) -> Result<String, Fault> {
        let start = self.at;
        self.at += 1;
        let mut tag = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            tag.push(c);
            self.at += 1;
        }
        if tag.is_empty() {
            return Err(start);
        }
        while self.peek() == Some('-') && self.peek_at(1).is_some_and(|c| c.is_ascii_alphanumeric())
        {
            tag.push('-');
            self.at += 1;
            while let Some(c) = self.peek().filter(char::is_ascii_alphanumeric) {
                tag.push(c);
                self.at += 1;
            }
        }
        Ok(tag)
    }

    fn literal(&mut self) -> Result<Term, Fault> {
        let start = self.at;
        let lexical = self.string()?;
        let string_end = self.at;
        self.skip();
        // `@prefix` and `@base` also match `LANGTAG`; the keyword is read,
        // as the tree-sitter grammar reads it.
        let keyword = self.directive().is_some_and(|(_, at_style, _)| at_style);
        let term = if self.peek() == Some('@') && !keyword {
            let tag_start = self.at;
            let tag = self.langtag()?;
            self.token(Key::Language(tag.clone()), tag_start, Spelling::Tag);
            self.implicit_iri(RDF_LANG_STRING);
            Term::Literal(lexical, RDF_LANG_STRING.to_owned(), Some(tag))
        } else if self.looking_at("^^") {
            self.at += 2;
            self.skip();
            let datatype = self.iri()?;
            Term::Literal(lexical, datatype, None)
        } else {
            self.at = string_end;
            self.implicit_iri(XSD_STRING);
            Term::Literal(lexical, XSD_STRING.to_owned(), None)
        };
        let end = self.at;
        self.at = string_end;
        self.token(Key::Term(term.clone()), start, Spelling::Quoted);
        self.at = end;
        Ok(term)
    }

    fn object(&mut self) -> Result<Term, Fault> {
        let start = self.at;
        match self.peek().ok_or(start)? {
            '_' if self.peek_at(1) == Some(':') => self.blank_label(),
            '[' => match self.anon() {
                Some(len) => Ok(self.read_anon(len)),
                None => self.blank_property_list(),
            },
            '(' => self.collection(),
            '"' | '\'' => self.literal(),
            '<' => self.iri().map(Term::Iri),
            _ => {
                if let Some((end, datatype)) = self.number() {
                    self.at += end;
                    return Ok(self.bare(start, datatype));
                }
                let save = self.at;
                if let Some(iri) = self.prefixed_name()? {
                    self.token(Key::Term(Term::Iri(iri.clone())), save, Spelling::Iri);
                    return Ok(Term::Iri(iri));
                }
                for word in ["true", "false"] {
                    if self.looking_at(word) {
                        self.at += word.len();
                        return Ok(self.bare(start, XSD_BOOLEAN));
                    }
                }
                Err(start)
            }
        }
    }

    fn bare(&mut self, start: usize, datatype: &str) -> Term {
        let lexical: String = self.chars[start..self.at].iter().collect();
        let term = Term::Literal(lexical, datatype.to_owned(), None);
        self.implicit_iri(datatype);
        self.token(
            Key::Term(term.clone()),
            start,
            Spelling::Bare(datatype.to_owned()),
        );
        term
    }

    fn blank_property_list(&mut self) -> Result<Term, Fault> {
        self.at += 1;
        let subject = self.fresh();
        self.skip();
        self.predicate_object_list(&subject)?;
        self.skip();
        if self.peek() != Some(']') {
            return Err(self.at);
        }
        self.at += 1;
        Ok(subject)
    }

    fn collection(&mut self) -> Result<Term, Fault> {
        self.at += 1;
        // The cells are allocated, first to last, before any element is
        // read: count the elements by reading ahead with a scratch reader
        // would share state, so read the elements first into positions.
        let mut elements = Vec::new();
        let saved = (
            self.at,
            self.counter,
            self.reading.clone(),
            self.labels.clone(),
        );
        loop {
            self.skip();
            if self.peek() == Some(')') {
                break;
            }
            elements.push(self.at);
            self.object()?;
        }
        let close = self.at;
        (self.at, self.counter, self.reading, self.labels) = saved;
        if elements.is_empty() {
            self.at = close + 1;
            self.implicit_iri(RDF_NIL);
            return Ok(Term::Iri(RDF_NIL.to_owned()));
        }
        let cells: Vec<Term> = elements.iter().map(|_| self.fresh()).collect();
        for (i, &position) in elements.iter().enumerate() {
            self.at = position;
            let object = self.object()?;
            self.implicit_iri(RDF_FIRST);
            self.implicit_iri(RDF_REST);
            let rest = cells
                .get(i + 1)
                .cloned()
                .unwrap_or_else(|| Term::Iri(RDF_NIL.to_owned()));
            if i + 1 == cells.len() {
                self.implicit_iri(RDF_NIL);
            }
            self.reading
                .triples
                .push(Triple(cells[i].clone(), RDF_FIRST.to_owned(), object));
            self.reading
                .triples
                .push(Triple(cells[i].clone(), RDF_REST.to_owned(), rest));
        }
        self.at = close + 1;
        Ok(cells[0].clone())
    }

    fn predicate_object_list(&mut self, subject: &Term) -> Result<(), Fault> {
        loop {
            let predicate = self.verb()?;
            loop {
                self.skip();
                let object = self.object()?;
                self.reading
                    .triples
                    .push(Triple(subject.clone(), predicate.clone(), object));
                let end = self.at;
                self.skip();
                if self.peek() != Some(',') {
                    self.at = end;
                    break;
                }
                self.at += 1;
            }
            let end = self.at;
            self.skip();
            if self.peek() != Some(';') {
                self.at = end;
                return Ok(());
            }
            while self.peek() == Some(';') {
                self.at += 1;
                self.skip();
            }
            if matches!(self.peek(), Some('.' | ']') | None) {
                return Ok(());
            }
        }
    }

    /// A directive keyword, if one starts here: whether it is `@`-style,
    /// and whether it declares a prefix.
    fn directive(&self) -> Option<(usize, bool, bool)> {
        let word_end = |len: usize| {
            !self
                .peek_at(len)
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':' || pn_chars(c))
        };
        if self.looking_at("@prefix") && word_end(7) {
            return Some((7, true, true));
        }
        if self.looking_at("@base") && word_end(5) {
            return Some((5, true, false));
        }
        let word: String = (0..6)
            .filter_map(|i| self.peek_at(i))
            .collect::<String>()
            .to_ascii_lowercase();
        if word == "prefix" && word_end(6) {
            return Some((6, false, true));
        }
        if word.starts_with("base") && word_end(4) {
            return Some((4, false, false));
        }
        None
    }

    fn statement(&mut self) -> Result<(), Fault> {
        if let Some((len, at_style, prefix)) = self.directive() {
            self.at += len;
            self.skip();
            if prefix {
                let start = self.at;
                let name = self.pn_prefix();
                if self.peek() != Some(':') {
                    return Err(start);
                }
                self.at += 1;
                self.skip();
                let iri = self.iriref()?;
                self.prefixes.insert(name, iri);
            } else {
                self.base = Some(self.iriref()?);
            }
            if at_style {
                let end = self.at;
                self.skip();
                if self.peek() != Some('.') {
                    return Err(end);
                }
                self.at += 1;
            }
            return Ok(());
        }
        if self.peek() == Some('[') && self.anon().is_none() {
            let subject = self.blank_property_list()?;
            let end = self.at;
            self.skip();
            if self.peek() == Some('.') {
                self.at = end;
            } else {
                self.predicate_object_list(&subject)?;
            }
        } else {
            let subject = self.subject()?;
            self.skip();
            self.predicate_object_list(&subject)?;
        }
        let end = self.at;
        self.skip();
        if self.peek() != Some('.') {
            return Err(end);
        }
        self.at += 1;
        Ok(())
    }

    fn document(&mut self) -> Result<(), Fault> {
        loop {
            self.skip();
            if self.peek().is_none() {
                return Ok(());
            }
            self.statement()?;
        }
    }
}

/// Read `text` with `used` as the set of labels fresh labels skip.
fn read_with(text: &str, base: Option<&str>, used: HashSet<String>) -> (Reader, Result<(), Fault>) {
    let chars: Vec<char> = text.chars().collect();
    let mut bytes: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    bytes.push(text.len());
    let mut reader = Reader {
        chars,
        bytes,
        at: 0,
        base: base.map(str::to_owned),
        prefixes: HashMap::new(),
        used,
        labels: BTreeSet::new(),
        counter: 0,
        reading: Reading::default(),
    };
    let result = reader.document();
    (reader, result)
}

/// The lines a refusal may name: from the line where the fault's gap
/// begins to the line of the first token after it.
pub type FaultLines = std::ops::RangeInclusive<usize>;

/// The reading of a Turtle document with base IRI `base`, or the lines
/// (zero-based, counting line feeds) its first fault may be reported on.
///
/// A fault is found at a position: the start of a token the grammar or a
/// stated refusal rejects, or the end of the last token read when the
/// token the grammar requires next is absent. Since white space and
/// comments may separate tokens, a reader may name any place from that
/// position to the first token after it; the lines of that stretch are
/// returned. The document is read twice: once to collect the labels it
/// writes, and once with fresh labels skipping them.
///
/// # Errors
///
/// Returns the lines of the first fault.
pub fn read(text: &str, base: Option<&str>) -> Result<Reading, FaultLines> {
    let line = |position: usize| text.chars().take(position).filter(|&c| c == '\n').count();
    if let Some(position) = text.chars().position(|c| matches!(c, '\u{0}' | '\u{7F}')) {
        return Err(line(position)..=line(position));
    }
    let lines = |mut reader: Reader, position: usize| {
        reader.at = position;
        reader.skip();
        line(position)..=line(reader.at)
    };
    let (first, result) = read_with(text, base, HashSet::new());
    if let Err(position) = result {
        return Err(lines(first, position));
    }
    let used = first.labels.into_iter().collect();
    let (second, result) = read_with(text, base, used);
    match result {
        Ok(()) => Ok(second.reading),
        Err(position) => Err(lines(second, position)),
    }
}

/// The triples of a Turtle document, or the lines of its first fault.
///
/// # Errors
///
/// Returns the lines of the first fault.
pub fn read_document(text: &str, base: Option<&str>) -> Result<Vec<Triple>, FaultLines> {
    read(text, base).map(|reading| reading.triples)
}

// ── Canonical Turtle ───────────────────────────────────────────────────

/// The canonical Turtle text of a list of triples.
///
/// The distinct triples are sorted by the N-Triples text of subject,
/// predicate and object; a subject's run is written as one statement, its
/// predicates separated by ` ;`, a line feed and four spaces, a
/// predicate's objects by `, `.
#[must_use]
pub fn canonical_text(triples: &[Triple]) -> String {
    let rows: BTreeSet<(String, String, String)> = triples
        .iter()
        .map(|Triple(s, p, o)| (term_text(s), format!("<{p}>"), term_text(o)))
        .collect();
    let mut out = String::new();
    let mut previous: Option<(&str, &str)> = None;
    for (s, p, o) in &rows {
        match previous {
            Some((ps, pp)) if ps == s && pp == p => {
                out += ", ";
                out += o;
            }
            Some((ps, _)) if ps == s => {
                for piece in [" ;\n    ", p, " ", o] {
                    out += piece;
                }
            }
            Some(_) => {
                for piece in [" .\n", s, " ", p, " ", o] {
                    out += piece;
                }
            }
            None => {
                for piece in [s.as_str(), " ", p, " ", o] {
                    out += piece;
                }
            }
        }
        previous = Some((s, p));
    }
    if previous.is_some() {
        out += " .\n";
    }
    out
}

// ── The format-preserving route ────────────────────────────────────────

/// What the format-preserving route does with one value edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    /// The document written, its edited tokens rewritten.
    Written(String),
    /// The edit is refused for the named reason.
    Refused(&'static str),
}

fn retarget(key: &Key, value: &str) -> Key {
    match key {
        Key::Term(Term::Iri(_)) => Key::Term(Term::Iri(value.to_owned())),
        Key::Term(Term::Blank(_)) => Key::Term(Term::Blank(value.to_owned())),
        Key::Term(Term::Literal(_, datatype, tag)) => Key::Term(Term::Literal(
            value.to_owned(),
            datatype.clone(),
            tag.clone(),
        )),
        Key::Language(_) => Key::Language(value.to_owned()),
    }
}

/// The term `term` with the edit `from` to `to` applied, wherever `from`
/// is written in it (a literal's datatype or tag included).
fn substitute(term: &Term, from: &Key, to: &Key) -> Term {
    if Key::Term(term.clone()) == *from
        && let Key::Term(to) = to
    {
        return to.clone();
    }
    match (term, from, to) {
        (
            Term::Literal(lexical, datatype, tag),
            Key::Term(Term::Iri(old)),
            Key::Term(Term::Iri(new)),
        ) if datatype == old => Term::Literal(lexical.clone(), new.clone(), tag.clone()),
        (Term::Literal(lexical, datatype, Some(tag)), Key::Language(old), Key::Language(new))
            if tag == old =>
        {
            Term::Literal(lexical.clone(), datatype.clone(), Some(new.clone()))
        }
        _ => term.clone(),
    }
}

/// The triples of `reading` with the edit applied.
#[must_use]
pub fn edited_triples(reading: &Reading, from: &Key, value: &str) -> Vec<Triple> {
    let to = retarget(from, value);
    reading
        .triples
        .iter()
        .map(|Triple(s, p, o)| {
            let predicate = match (from, &to) {
                (Key::Term(Term::Iri(old)), Key::Term(Term::Iri(new))) if old == p => new.clone(),
                _ => p.clone(),
            };
            Triple(
                substitute(s, from, &to),
                predicate,
                substitute(o, from, &to),
            )
        })
        .collect()
}

/// Whether `lexical` is a bare token of the form `datatype` implies.
fn bare_form(datatype: &str, lexical: &str) -> bool {
    let reader = Reader {
        chars: lexical.chars().collect(),
        bytes: Vec::new(),
        at: 0,
        base: None,
        prefixes: HashMap::new(),
        used: HashSet::new(),
        labels: BTreeSet::new(),
        counter: 0,
        reading: Reading::default(),
    };
    match datatype {
        XSD_BOOLEAN => matches!(lexical, "true" | "false"),
        _ => reader
            .number()
            .is_some_and(|(end, kind)| end == reader.chars.len() && kind == datatype),
    }
}

/// The terms of a reading's graph, each with its key.
fn terms(reading: &Reading) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    for Triple(s, p, o) in &reading.triples {
        for term in [s, o] {
            out.insert(Key::Term(term.clone()));
            if let Term::Literal(_, datatype, tag) = term {
                out.insert(Key::Term(Term::Iri(datatype.clone())));
                if let Some(tag) = tag {
                    out.insert(Key::Language(tag.clone()));
                }
            }
        }
        out.insert(Key::Term(Term::Iri(p.clone())));
    }
    out
}

/// What the format-preserving route does when the term `key` of the
/// document `text`, read with base `base`, has its value set to `value`.
///
/// The checks run in this order: the new value's form (an IRI must be
/// absolute and hold no character `IRIREF` excludes, a label must be an
/// N-Triples label, a tag a language tag); an IRI edit that leaves a
/// literal with a tag and a datatype other than `rdf:langString`, or the
/// converse (`LanguageDatatype`); a term the edit makes equal to another
/// term of the graph (`Duplicate`, which production reports as a
/// duplicate term or a duplicate triple, whichever node it meets first); a
/// term the document writes without a token;
/// a label holding `:`, which Turtle cannot write. A permitted edit writes
/// the document with each of the term's tokens replaced: an IRI by `<` the
/// IRI `>`, a label by `_:` and the label, a quoted string by the
/// canonical N-Triples string, a bare token by the new lexical form when
/// it is a token of the same form and otherwise by the canonical string,
/// `^^` and the datatype, and a tag by `@` and the tag.
///
/// # Panics
///
/// Panics when `text` does not read or `key` is not a term of it.
#[must_use]
pub fn edit(text: &str, base: Option<&str>, key: &Key, value: &str) -> EditOutcome {
    let Ok(reading) = read(text, base) else {
        panic!("the document does not read");
    };
    let all = terms(&reading);
    assert!(all.contains(key), "{key:?} is not a term of the document");
    let valid = match key {
        Key::Term(Term::Iri(_)) => absolute(value) && !value.chars().any(excluded),
        Key::Term(Term::Blank(_)) => {
            let mut chars = value.chars();
            chars
                .next()
                .is_some_and(|c| pn_chars_u(c) || c == ':' || c.is_ascii_digit())
                && value.chars().all(|c| pn_chars(c) || c == ':' || c == '.')
                && !value.ends_with('.')
        }
        Key::Language(_) => {
            let mut parts = value.split('-');
            parts
                .next()
                .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphabetic()))
                && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
        }
        Key::Term(Term::Literal(..)) => true,
    };
    if !valid {
        return EditOutcome::Refused(match key {
            Key::Term(Term::Iri(_)) if value.chars().any(excluded) => "IriCharacter",
            Key::Term(Term::Iri(_)) => "RelativeIri",
            Key::Term(Term::Blank(_)) => "InvalidBlankLabel",
            _ => "InvalidLanguageTag",
        });
    }
    if let Key::Term(Term::Iri(old)) = key {
        let breaks = |Triple(_, _, o): &Triple| {
            matches!(o, Term::Literal(_, datatype, tag)
                if datatype == old && (tag.is_some() != (value == RDF_LANG_STRING)))
        };
        if old != value && reading.triples.iter().any(breaks) {
            return EditOutcome::Refused("LanguageDatatype");
        }
    }
    let target = retarget(key, value);
    if target != *key && all.contains(&target) {
        return EditOutcome::Refused("Duplicate");
    }
    if target == *key {
        return EditOutcome::Written(text.to_owned());
    }
    if reading.implicit.contains(key) {
        return EditOutcome::Refused("ImplicitTerm");
    }
    if matches!(key, Key::Term(Term::Blank(_))) && value.contains(':') {
        return EditOutcome::Refused("InvalidBlankLabel");
    }
    let mut spans: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    for token in reading.tokens.iter().filter(|t| t.key == *key) {
        let text = match &token.spelling {
            Spelling::Iri => format!("<{value}>"),
            Spelling::Blank => format!("_:{value}"),
            Spelling::Quoted => quote(value),
            Spelling::Bare(datatype) if bare_form(datatype, value) => value.to_owned(),
            Spelling::Bare(datatype) => format!("{}^^<{datatype}>", quote(value)),
            Spelling::Tag => format!("@{value}"),
        };
        spans.insert(token.start, (token.end, text));
    }
    let mut out = String::new();
    let mut at = 0;
    for (start, (end, replacement)) in spans {
        out += &text[at..start];
        out += &replacement;
        at = end;
    }
    out += &text[at..];
    EditOutcome::Written(out)
}

/// The keys of the terms of `reading`'s graph, in order.
#[must_use]
pub fn term_keys(reading: &Reading) -> Vec<Key> {
    terms(reading).into_iter().collect()
}
