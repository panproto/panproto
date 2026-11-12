//! Complete reference operations assigned to `rdf-formats`.

pub mod turtle;

pub mod ntriples {
    //! RDF 1.1 N-Triples, transcribed from the grammar of the
    //! Recommendation (Sec 7) over characters, with the RDF graph encoding
    //! and the canonical writer the `rdf-formats` page defines.
    //!
    //! The reader is a direct recursive reading of the productions
    //! `ntriplesDoc`, `triple`, `subject`, `predicate`, `object`, `literal`,
    //! `IRIREF`, `BLANK_NODE_LABEL`, `STRING_LITERAL_QUOTE`, `LANGTAG`,
    //! `UCHAR` and `ECHAR`, with white space (space and tab) between
    //! terminals and a comment from `#` to the end of its line. It shares no
    //! code with production, which parses with the tree-sitter Turtle
    //! grammar. Beyond the grammar it applies the page's three stated
    //! refusals: a document holding a raw U+0000 or U+007F, an IRI that is
    //! not absolute, and an IRI whose escapes decode to a character
    //! `IRIREF` excludes.

    use std::collections::{BTreeMap, BTreeSet};

    /// `xsd:string`.
    pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    /// `rdf:langString`.
    pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

    /// An RDF term.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Term {
        /// An IRI.
        Iri(String),
        /// A blank node, by its label.
        Blank(String),
        /// A literal: lexical form, datatype IRI, language tag.
        Literal(String, String, Option<String>),
    }

    /// An RDF triple: subject, predicate IRI, object.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Triple(pub Term, pub String, pub Term);

    /// A reading position over the characters of one document.
    struct Input {
        chars: Vec<char>,
        at: usize,
    }

    impl Input {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.at).copied()
        }

        fn eat(&mut self, c: char) -> bool {
            if self.peek() == Some(c) {
                self.at += 1;
                true
            } else {
                false
            }
        }

        /// Skip spaces and tabs, then a comment if one starts here.
        fn skip_blanks(&mut self) {
            while matches!(self.peek(), Some(' ' | '\t')) {
                self.at += 1;
            }
            if self.peek() == Some('#') {
                while !matches!(self.peek(), None | Some('\n' | '\r')) {
                    self.at += 1;
                }
            }
        }

        fn hex(&mut self, digits: usize) -> Option<char> {
            let mut code = 0u32;
            for _ in 0..digits {
                code = code * 16 + self.peek()?.to_digit(16)?;
                self.at += 1;
            }
            char::from_u32(code)
        }

        /// `UCHAR`, after its backslash.
        fn uchar(&mut self) -> Option<char> {
            if self.eat('u') {
                self.hex(4)
            } else if self.eat('U') {
                self.hex(8)
            } else {
                None
            }
        }

        /// `IRIREF`, checked absolute and free of excluded characters.
        fn iriref(&mut self) -> Option<String> {
            if !self.eat('<') {
                return None;
            }
            let mut iri = String::new();
            loop {
                let c = self.peek()?;
                self.at += 1;
                let decoded = match c {
                    '>' => break,
                    '\\' => self.uchar()?,
                    _ => c,
                };
                if excluded(decoded) {
                    return None;
                }
                iri.push(decoded);
            }
            absolute(&iri).then_some(iri)
        }

        /// `BLANK_NODE_LABEL`, without its `_:`.
        fn blank(&mut self) -> Option<String> {
            if !(self.eat('_') && self.eat(':')) {
                return None;
            }
            let first = self.peek()?;
            if !(pn_chars_u(first) || first.is_ascii_digit()) {
                return None;
            }
            let mut label = String::from(first);
            self.at += 1;
            while let Some(c) = self.peek() {
                if !(pn_chars(c) || c == '.') {
                    break;
                }
                label.push(c);
                self.at += 1;
            }
            while label.ends_with('.') {
                label.pop();
                self.at -= 1;
            }
            Some(label)
        }

        /// `STRING_LITERAL_QUOTE`, decoded.
        fn string(&mut self) -> Option<String> {
            if !self.eat('"') {
                return None;
            }
            let mut text = String::new();
            loop {
                let c = self.peek()?;
                self.at += 1;
                match c {
                    '"' => return Some(text),
                    '\n' | '\r' => return None,
                    '\\' => {
                        let escaped = match self.peek()? {
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
                    _ => text.push(c),
                }
            }
        }

        /// `LANGTAG`, without its `@`.
        fn langtag(&mut self) -> Option<String> {
            let mut tag = String::new();
            while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
                tag.push(c);
                self.at += 1;
            }
            if tag.is_empty() {
                return None;
            }
            while self.peek() == Some('-') {
                tag.push('-');
                self.at += 1;
                let before = tag.len();
                while let Some(c) = self.peek().filter(char::is_ascii_alphanumeric) {
                    tag.push(c);
                    self.at += 1;
                }
                if tag.len() == before {
                    return None;
                }
            }
            Some(tag)
        }

        fn subject(&mut self) -> Option<Term> {
            match self.peek()? {
                '<' => self.iriref().map(Term::Iri),
                '_' => self.blank().map(Term::Blank),
                _ => None,
            }
        }

        fn object(&mut self) -> Option<Term> {
            if self.peek()? != '"' {
                return self.subject();
            }
            let lexical = self.string()?;
            self.skip_blanks();
            if self.eat('@') {
                let tag = self.langtag()?;
                return Some(Term::Literal(lexical, RDF_LANG_STRING.into(), Some(tag)));
            }
            if self.eat('^') {
                if !self.eat('^') {
                    return None;
                }
                self.skip_blanks();
                let datatype = self.iriref()?;
                return Some(Term::Literal(lexical, datatype, None));
            }
            Some(Term::Literal(lexical, XSD_STRING.into(), None))
        }

        /// `triple`, with the white space between its terminals.
        fn triple(&mut self) -> Option<Triple> {
            let subject = self.subject()?;
            self.skip_blanks();
            let predicate = self.iriref()?;
            self.skip_blanks();
            let object = self.object()?;
            self.skip_blanks();
            self.eat('.').then_some(Triple(subject, predicate, object))
        }
    }

    /// Whether `IRIREF` excludes `c`.
    pub(super) const fn excluded(c: char) -> bool {
        matches!(
            c,
            '\u{0}'..='\u{20}' | '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\'
        )
    }

    /// Whether `iri` starts with a scheme: a letter, then letters, digits,
    /// `+`, `-` and `.`, then `:`.
    pub(super) fn absolute(iri: &str) -> bool {
        let Some(colon) = iri.find(':') else {
            return false;
        };
        let scheme: Vec<char> = iri[..colon].chars().collect();
        scheme.first().is_some_and(char::is_ascii_alphabetic)
            && scheme
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    }

    pub(super) fn pn_chars_base(c: char) -> bool {
        let n = u32::from(c);
        c.is_ascii_alphabetic()
            || (0xC0..=0xD6).contains(&n)
            || (0xD8..=0xF6).contains(&n)
            || (0xF8..=0x2FF).contains(&n)
            || (0x370..=0x37D).contains(&n)
            || (0x37F..=0x1FFF).contains(&n)
            || (0x200C..=0x200D).contains(&n)
            || (0x2070..=0x218F).contains(&n)
            || (0x2C00..=0x2FEF).contains(&n)
            || (0x3001..=0xD7FF).contains(&n)
            || (0xF900..=0xFDCF).contains(&n)
            || (0xFDF0..=0xFFFD).contains(&n)
            || (0x10000..=0xEFFFF).contains(&n)
    }

    fn pn_chars_u(c: char) -> bool {
        pn_chars_base(c) || c == '_' || c == ':'
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

    /// The triples of an N-Triples document in document order, or the
    /// zero-based index of the line (counting line feeds) on which reading
    /// fails.
    ///
    /// # Errors
    ///
    /// Returns the line of the first fault.
    pub fn read_document(text: &str) -> Result<Vec<Triple>, usize> {
        if let Some(at) = text.find(['\u{0}', '\u{7F}']) {
            return Err(text[..at].matches('\n').count());
        }
        let mut input = Input {
            chars: text.chars().collect(),
            at: 0,
        };
        let line_of = |input: &Input| {
            input.chars[..input.at]
                .iter()
                .filter(|&&c| c == '\n')
                .count()
        };
        let mut triples = Vec::new();
        let mut need_eol = false;
        loop {
            input.skip_blanks();
            match input.peek() {
                None => return Ok(triples),
                Some('\n' | '\r') => {
                    input.at += 1;
                    need_eol = false;
                }
                Some(_) if need_eol => return Err(line_of(&input)),
                Some(_) => {
                    let start = input.at;
                    let Some(triple) = input.triple() else {
                        input.at = start;
                        return Err(line_of(&input));
                    };
                    triples.push(triple);
                    need_eol = true;
                }
            }
        }
    }

    // ── The RDF graph encoding ──────────────────────────────────────

    /// A graph in the RDF graph encoding: nodes by id with their anchor and
    /// value, and edges in id order as source, target and name.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Graph {
        /// Each node's anchor and string value.
        pub nodes: BTreeMap<u32, (String, Option<String>)>,
        /// Each edge as source, target and name, in id order.
        pub edges: Vec<(u32, u32, String)>,
    }

    /// The encoding's edges, as source anchor, target anchor and name.
    pub const EDGES: &[(&str, &str, &str)] = &[
        ("triple", "iri", "subject"),
        ("triple", "blank", "subject"),
        ("triple", "iri", "predicate"),
        ("triple", "iri", "object"),
        ("triple", "blank", "object"),
        ("triple", "literal", "object"),
        ("literal", "iri", "datatype"),
        ("literal", "language", "language"),
    ];

    struct Encoder {
        graph: Graph,
        keys: BTreeMap<(String, Term), u32>,
    }

    impl Encoder {
        fn node(&mut self, anchor: &str, value: Option<String>) -> u32 {
            let id = u32::try_from(self.graph.nodes.len()).unwrap_or(u32::MAX);
            self.graph.nodes.insert(id, (anchor.into(), value));
            id
        }

        fn keyed(&mut self, anchor: &str, key: Term, value: &str) -> (u32, bool) {
            if let Some(&id) = self.keys.get(&(anchor.into(), key.clone())) {
                return (id, false);
            }
            let id = self.node(anchor, Some(value.into()));
            self.keys.insert((anchor.into(), key), id);
            (id, true)
        }

        fn term(&mut self, term: &Term) -> (u32, &'static str) {
            match term {
                Term::Iri(iri) => (self.keyed("iri", Term::Iri(iri.clone()), iri).0, "iri"),
                Term::Blank(label) => (
                    self.keyed("blank", Term::Blank(label.clone()), label).0,
                    "blank",
                ),
                Term::Literal(lexical, datatype, language) => {
                    let (id, new) = self.keyed("literal", term.clone(), lexical);
                    if new {
                        let dt = self.term(&Term::Iri(datatype.clone())).0;
                        self.graph.edges.push((id, dt, "datatype".into()));
                        if let Some(tag) = language {
                            let tag_node = self.keyed("language", Term::Iri(tag.clone()), tag).0;
                            self.graph.edges.push((id, tag_node, "language".into()));
                        }
                    }
                    (id, "literal")
                }
            }
        }
    }

    /// The graph of `triples`.
    ///
    /// Per triple not seen before, a `triple` node, then its subject,
    /// predicate and object (a new literal followed by its datatype and
    /// tag), each term reusing the node of an equal term.
    #[must_use]
    pub fn encode(triples: &[Triple]) -> Graph {
        let mut encoder = Encoder {
            graph: Graph::default(),
            keys: BTreeMap::new(),
        };
        let mut seen = BTreeSet::new();
        for triple in triples {
            if !seen.insert(triple.clone()) {
                continue;
            }
            let id = encoder.node("triple", None);
            let (s, _) = encoder.term(&triple.0);
            encoder.graph.edges.push((id, s, "subject".into()));
            let (p, _) = encoder.term(&Term::Iri(triple.1.clone()));
            encoder.graph.edges.push((id, p, "predicate".into()));
            let (o, _) = encoder.term(&triple.2);
            encoder.graph.edges.push((id, o, "object".into()));
        }
        encoder.graph
    }

    // ── Canonical writing ───────────────────────────────────────────

    pub(super) fn quote(text: &str) -> String {
        let mut out = String::from('"');
        for c in text.chars() {
            let n = u32::from(c);
            let escape = match c {
                '\u{8}' => Some("\\b".to_owned()),
                '\t' => Some("\\t".to_owned()),
                '\n' => Some("\\n".to_owned()),
                '\u{C}' => Some("\\f".to_owned()),
                '\r' => Some("\\r".to_owned()),
                '"' => Some("\\\"".to_owned()),
                '\\' => Some("\\\\".to_owned()),
                _ if n <= 0x7 || n == 0xB || (0xE..=0x1F).contains(&n) || n == 0x7F => {
                    Some(format!("\\u{n:04X}"))
                }
                _ if n == 0xFFFE || n == 0xFFFF => Some(format!("\\u{n:04X}")),
                _ => None,
            };
            match escape {
                Some(e) => out.push_str(&e),
                None => out.push(c),
            }
        }
        out.push('"');
        out
    }

    pub(super) fn term_text(term: &Term) -> String {
        match term {
            Term::Iri(iri) => format!("<{iri}>"),
            Term::Blank(label) => format!("_:{label}"),
            Term::Literal(lexical, _, Some(tag)) => format!("{}@{tag}", quote(lexical)),
            Term::Literal(lexical, datatype, None) if datatype == XSD_STRING => quote(lexical),
            Term::Literal(lexical, datatype, None) => format!("{}^^<{datatype}>", quote(lexical)),
        }
    }

    /// The canonical N-Triples text of a set of triples: one line per
    /// distinct triple, `S P O .`, the lines in ascending order.
    #[must_use]
    pub fn canonical_text(triples: &[Triple]) -> String {
        let lines: BTreeSet<String> = triples
            .iter()
            .map(|Triple(s, p, o)| format!("{} <{p}> {} .\n", term_text(s), term_text(o)))
            .collect();
        lines.into_iter().collect()
    }

    /// Why a graph is refused by the writer.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum WriteRefusal {
        /// The anchor is not one of the encoding's five.
        UnknownAnchor,
        /// A triple holds a value, or a term holds none.
        Value,
        /// An edge that is not one of the encoding's between these anchors.
        UnexpectedEdge,
        /// A node without exactly one out-edge of a required name, or a
        /// literal with more than one tag.
        EdgeCount(String),
        /// A tag without `rdf:langString`, or `rdf:langString` without tag.
        LanguageDatatype,
        /// An earlier node holds the same term.
        DuplicateTerm(u32),
        /// An earlier node holds the same triple.
        DuplicateTriple(u32),
        /// No triple reaches the node.
        Unused,
        /// The IRI is not absolute.
        RelativeIri,
        /// The IRI holds an excluded character.
        IriCharacter(char),
        /// The label is not a `BLANK_NODE_LABEL` label.
        InvalidBlankLabel,
        /// The tag is not a `LANGTAG` tag.
        InvalidLanguageTag,
    }

    fn valid_label(label: &str) -> bool {
        let mut input = Input {
            chars: format!("_:{label}").chars().collect(),
            at: 0,
        };
        input.blank().as_deref() == Some(label) && input.peek().is_none()
    }

    fn valid_tag(tag: &str) -> bool {
        let mut input = Input {
            chars: tag.chars().collect(),
            at: 0,
        };
        input.langtag().as_deref() == Some(tag) && input.peek().is_none()
    }

    /// The first-pass checks of node `id` of [`write_graph`], and its
    /// out-edges by name.
    fn check_node(
        graph: &Graph,
        id: u32,
    ) -> Result<BTreeMap<String, Vec<u32>>, (u32, WriteRefusal)> {
        let (node_anchor, node_value) = &graph.nodes[&id];
        let node_anchor = node_anchor.as_str();
        if !["triple", "iri", "blank", "literal", "language"].contains(&node_anchor) {
            return Err((id, WriteRefusal::UnknownAnchor));
        }
        if (node_anchor == "triple") == node_value.is_some() {
            return Err((id, WriteRefusal::Value));
        }
        let text = node_value.clone().unwrap_or_default();
        match node_anchor {
            "iri" => {
                if let Some(c) = text.chars().find(|&c| excluded(c)) {
                    return Err((id, WriteRefusal::IriCharacter(c)));
                }
                if !absolute(&text) {
                    return Err((id, WriteRefusal::RelativeIri));
                }
            }
            "blank" if !valid_label(&text) => {
                return Err((id, WriteRefusal::InvalidBlankLabel));
            }
            "language" if !valid_tag(&text) => {
                return Err((id, WriteRefusal::InvalidLanguageTag));
            }
            _ => {}
        }
        let mut names: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (src, tgt, name) in &graph.edges {
            if *src != id {
                continue;
            }
            let target = graph.nodes[tgt].0.as_str();
            if !EDGES
                .iter()
                .any(|(s, t, n)| *s == node_anchor && *t == target && n == name)
            {
                return Err((id, WriteRefusal::UnexpectedEdge));
            }
            names.entry(name.clone()).or_default().push(*tgt);
        }
        let count = |name: &str| names.get(name).map_or(0, Vec::len);
        let required: &[&str] = match node_anchor {
            "triple" => &["subject", "predicate", "object"],
            "literal" => &["datatype"],
            _ => &[],
        };
        for name in required {
            if count(name) != 1 {
                return Err((id, WriteRefusal::EdgeCount((*name).into())));
            }
        }
        if node_anchor == "literal" {
            if count("language") > 1 {
                return Err((id, WriteRefusal::EdgeCount("language".into())));
            }
            let datatype = &graph.nodes[&names["datatype"][0]].1;
            let lang_string = datatype.as_deref() == Some(RDF_LANG_STRING);
            if lang_string != (count("language") == 1) {
                return Err((id, WriteRefusal::LanguageDatatype));
            }
        }
        Ok(names)
    }

    /// The canonical text of a graph in the RDF graph encoding, or the first
    /// node at fault and why.
    ///
    /// The checks run on each node in id order (its anchor, its value,
    /// the form of an IRI, label or tag, then each out-edge in id order, then
    /// the required out-edge counts and the literal's datatype rule), then
    /// in id order each node an earlier node duplicates, then the first
    /// node no triple reaches.
    ///
    /// # Errors
    ///
    /// Returns the node at fault and the [`WriteRefusal`].
    pub fn write_graph(graph: &Graph) -> Result<String, (u32, WriteRefusal)> {
        let anchor = |id: u32| graph.nodes[&id].0.as_str();
        let value = |id: u32| graph.nodes[&id].1.clone();
        let mut out: BTreeMap<u32, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
        for &id in graph.nodes.keys() {
            out.insert(id, check_node(graph, id)?);
        }

        let term = |id: u32| -> Term {
            let text = value(id).unwrap_or_default();
            match anchor(id) {
                "iri" | "language" => Term::Iri(text),
                "blank" => Term::Blank(text),
                _ => Term::Literal(
                    text,
                    value(out[&id]["datatype"][0]).unwrap_or_default(),
                    out[&id]
                        .get("language")
                        .map(|tags| value(tags[0]).unwrap_or_default()),
                ),
            }
        };
        let mut seen_terms: BTreeMap<(String, Term), u32> = BTreeMap::new();
        let mut seen_triples: BTreeMap<Triple, u32> = BTreeMap::new();
        let mut triples = Vec::new();
        for &id in graph.nodes.keys() {
            if anchor(id) == "triple" {
                let edges = &out[&id];
                let triple = Triple(
                    term(edges["subject"][0]),
                    value(edges["predicate"][0]).unwrap_or_default(),
                    term(edges["object"][0]),
                );
                if let Some(&other) = seen_triples.get(&triple) {
                    return Err((id, WriteRefusal::DuplicateTriple(other)));
                }
                seen_triples.insert(triple.clone(), id);
                triples.push(triple);
            } else {
                let key = (anchor(id).to_owned(), term(id));
                if let Some(&other) = seen_terms.get(&key) {
                    return Err((id, WriteRefusal::DuplicateTerm(other)));
                }
                seen_terms.insert(key, id);
            }
        }

        let mut reached = BTreeSet::new();
        for (&id, edges) in &out {
            if anchor(id) == "triple" {
                reached.insert(id);
                for &target in edges.values().flatten() {
                    reached.insert(target);
                    reached.extend(out[&target].values().flatten());
                }
            }
        }
        if let Some(&unused) = graph.nodes.keys().find(|id| !reached.contains(id)) {
            return Err((unused, WriteRefusal::Unused));
        }
        Ok(canonical_text(&triples))
    }
}
