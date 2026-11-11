//! Complete reference operations assigned to `text-formats`.

pub mod toml {
    //! TOML 1.0.0 key resolution and document reading over a statement
    //! model.
    //!
    //! The model keeps every table and value at its absolute path from the
    //! root, with one step per key and one per array-of-tables element, and
    //! records how each table was created. It shares no structure with the
    //! production resolver, which builds a nested tree of tables tagged by
    //! origin.

    use std::collections::BTreeMap;

    /// One TOML statement, in source order.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Statement {
        /// `k1.k2 = v`: a pair whose value is the statement's index.
        Pair(Vec<String>),
        /// `[k1.k2]`: a table header.
        Header(Vec<String>),
        /// `[[k1.k2]]`: an array-of-tables header.
        ArrayHeader(Vec<String>),
    }

    /// The resolved content of one key.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Item {
        /// The value of the pair at this statement index.
        Value(usize),
        /// A table: its keys and their items in first-definition order.
        Table(Vec<(String, Self)>),
        /// An array of tables, element by element.
        Tables(Vec<Vec<(String, Self)>>),
    }

    /// A table's keys and their items, in first-definition order.
    pub type Entries = Vec<(String, Item)>;

    /// The root table, or the index of the first refused statement and why.
    pub type Resolution = Result<Entries, (usize, KeyRefusal)>;

    /// Why a statement is refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyRefusal {
        /// A pair's full key already names a value or table.
        DuplicateKey,
        /// A dotted key passes through a key that is not a table defined by
        /// the dotted keys of the table holding the pair.
        DottedExtension,
        /// A header passes through a key that holds a value.
        HeaderExtendsValue,
        /// A `[header]` names a key already defined as anything other than an
        /// implicit table.
        TableRedefined,
        /// A `[[header]]` names a key that is not an array of tables.
        ArrayConflict,
    }

    /// One step of an absolute path.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Step {
        Key(String),
        Element(usize),
    }

    /// How the thing at a path was created.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Kind {
        Value(usize),
        Implicit,
        Header,
        /// Created by a dotted key of the pair held by the table at `owner`.
        Dotted {
            owner: Vec<Step>,
        },
        /// An array of tables with this many elements.
        Tables(usize),
    }

    struct State {
        kinds: BTreeMap<Vec<Step>, Kind>,
        /// The keys of each table path, in creation order.
        keys: BTreeMap<Vec<Step>, Vec<String>>,
    }

    impl State {
        fn create(&mut self, parent: &[Step], key: &str, kind: Kind) -> Vec<Step> {
            let mut path = parent.to_vec();
            path.push(Step::Key(key.to_owned()));
            self.kinds.insert(path.clone(), kind);
            self.keys
                .entry(parent.to_vec())
                .or_default()
                .push(key.to_owned());
            path
        }

        fn item(&self, path: &[Step]) -> Item {
            match self.kinds.get(path) {
                Some(Kind::Value(index)) => Item::Value(*index),
                Some(Kind::Tables(count)) => Item::Tables(
                    (0..*count)
                        .map(|element| {
                            let mut at = path.to_vec();
                            at.push(Step::Element(element));
                            self.table(&at)
                        })
                        .collect(),
                ),
                _ => Item::Table(self.table(path)),
            }
        }

        fn table(&self, path: &[Step]) -> Vec<(String, Item)> {
            self.keys.get(path).map_or_else(Vec::new, |keys| {
                keys.iter()
                    .map(|key| {
                        let mut at = path.to_vec();
                        at.push(Step::Key(key.clone()));
                        (key.clone(), self.item(&at))
                    })
                    .collect()
            })
        }

        /// The path a header's leading keys lead to, creating implicit
        /// tables, or the refusal.
        fn header_prefix(&mut self, keys: &[String]) -> Result<Vec<Step>, KeyRefusal> {
            let mut path = Vec::new();
            for key in keys {
                let mut next = path.clone();
                next.push(Step::Key(key.clone()));
                path = match self.kinds.get(&next) {
                    None => self.create(&path, key, Kind::Implicit),
                    Some(Kind::Value(_)) => return Err(KeyRefusal::HeaderExtendsValue),
                    Some(Kind::Tables(count)) => {
                        let mut element = next.clone();
                        element.push(Step::Element(count - 1));
                        element
                    }
                    Some(_) => next,
                };
            }
            Ok(path)
        }
    }

    /// Resolve a TOML statement sequence into its root table, or name the
    /// first statement refused and why.
    ///
    /// Definition (TOML 1.0.0, Keys and Table sections). Pairs before the
    /// first header belong to the root table; pairs after a header belong to
    /// the table it opens. A pair `k1...kn = v` held by table `C` walks from
    /// `C`: each leading key that is absent becomes a table created by the
    /// dotted keys of `C`, each that is such a table is entered, and any
    /// other leading key refuses with `DottedExtension`; the last key must
    /// be absent (`DuplicateKey`) and becomes the value. A header walks its
    /// leading keys from the root: an absent key becomes an implicit table,
    /// an array of tables is entered at its last element, a table is
    /// entered, and a value refuses with `HeaderExtendsValue`. `[k]` then
    /// creates `k` as a defined table, defines an implicit table `k`, or
    /// refuses with `TableRedefined`; `[[k]]` creates a one-element array of
    /// tables, appends an element to an existing one, or refuses with
    /// `ArrayConflict`. An empty key list is no statement the grammar
    /// admits and gives `None`.
    #[must_use]
    pub fn resolve_keys(statements: &[Statement]) -> Option<Resolution> {
        let mut state = State {
            kinds: BTreeMap::new(),
            keys: BTreeMap::new(),
        };
        let mut current: Vec<Step> = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let (Statement::Pair(keys) | Statement::Header(keys) | Statement::ArrayHeader(keys)) =
                statement;
            let (last, leading) = keys.split_last()?;
            let refused = |reason| Some(Err((index, reason)));
            match statement {
                Statement::Pair(_) => {
                    let mut path = current.clone();
                    for key in leading {
                        let mut next = path.clone();
                        next.push(Step::Key(key.clone()));
                        path = match state.kinds.get(&next) {
                            None => state.create(
                                &path,
                                key,
                                Kind::Dotted {
                                    owner: current.clone(),
                                },
                            ),
                            Some(Kind::Dotted { owner }) if *owner == current => next,
                            Some(_) => return refused(KeyRefusal::DottedExtension),
                        };
                    }
                    let mut target = path.clone();
                    target.push(Step::Key(last.clone()));
                    if state.kinds.contains_key(&target) {
                        return refused(KeyRefusal::DuplicateKey);
                    }
                    state.create(&path, last, Kind::Value(index));
                }
                Statement::Header(_) => {
                    let parent = match state.header_prefix(leading) {
                        Ok(parent) => parent,
                        Err(reason) => return refused(reason),
                    };
                    let mut target = parent.clone();
                    target.push(Step::Key(last.clone()));
                    match state.kinds.get(&target) {
                        None => {
                            state.create(&parent, last, Kind::Header);
                        }
                        Some(Kind::Implicit) => {
                            state.kinds.insert(target.clone(), Kind::Header);
                        }
                        Some(_) => return refused(KeyRefusal::TableRedefined),
                    }
                    current = target;
                }
                Statement::ArrayHeader(_) => {
                    let parent = match state.header_prefix(leading) {
                        Ok(parent) => parent,
                        Err(reason) => return refused(reason),
                    };
                    let mut target = parent.clone();
                    target.push(Step::Key(last.clone()));
                    let count = match state.kinds.get(&target) {
                        None => {
                            state.create(&parent, last, Kind::Tables(1));
                            1
                        }
                        Some(Kind::Tables(count)) => {
                            let count = count + 1;
                            state.kinds.insert(target.clone(), Kind::Tables(count));
                            count
                        }
                        Some(_) => return refused(KeyRefusal::ArrayConflict),
                    };
                    target.push(Step::Element(count - 1));
                    current = target;
                }
            }
        }
        Some(Ok(state.table(&[])))
    }

    /// A TOML document's content read whole: its resolved tables, with
    /// each pair's value read by the TOML scalar function.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Read {
        /// A scalar value.
        Value(crate::value::Value),
        /// A table: its keys and their contents in first-definition order.
        Table(Vec<(String, Self)>),
        /// An array of tables, element by element.
        Tables(Vec<Vec<(String, Self)>>),
    }

    /// Why a TOML document is refused when read whole.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum DocumentRefusal {
        /// A statement's key, by [`resolve_keys`].
        Key(usize, KeyRefusal),
        /// A pair's value token, by the TOML scalar function.
        Scalar(usize, crate::value_io::ValueIoError),
    }

    fn read_entries(
        entries: &[(String, Item)],
        tokens: &[&[u8]],
    ) -> Result<Vec<(String, Read)>, DocumentRefusal> {
        entries
            .iter()
            .map(|(key, item)| Ok((key.clone(), read_item(item, tokens)?)))
            .collect()
    }

    fn read_item(item: &Item, tokens: &[&[u8]]) -> Result<Read, DocumentRefusal> {
        Ok(match item {
            Item::Value(index) => {
                let token = tokens.get(*index).copied().unwrap_or_default();
                Read::Value(
                    crate::value_io::parse(crate::value_io::Route::Toml, token)
                        .map_err(|error| DocumentRefusal::Scalar(*index, error))?,
                )
            }
            Item::Table(entries) => Read::Table(read_entries(entries, tokens)?),
            Item::Tables(elements) => Read::Tables(
                elements
                    .iter()
                    .map(|element| read_entries(element, tokens))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }

    /// Read a TOML document whole, the token of statement `i` being
    /// `tokens[i]` (ignored for a header).
    ///
    /// Definition. The document reads as [`resolve_keys`] resolves its
    /// statements, with each pair's value the TOML scalar function
    /// [`crate::value_io::parse`] applied to its token. A key refusal is
    /// the document's refusal; otherwise the first pair, in statement
    /// order, whose token the scalar function refuses is. `None` where
    /// [`resolve_keys`] gives `None`.
    #[must_use]
    pub fn read_document(
        statements: &[Statement],
        tokens: &[&[u8]],
    ) -> Option<Result<Vec<(String, Read)>, DocumentRefusal>> {
        let resolved = match resolve_keys(statements)? {
            Ok(entries) => entries,
            Err((index, reason)) => return Some(Err(DocumentRefusal::Key(index, reason))),
        };
        for (index, statement) in statements.iter().enumerate() {
            if let Statement::Pair(_) = statement
                && let Err(error) = crate::value_io::parse(
                    crate::value_io::Route::Toml,
                    tokens.get(index).copied().unwrap_or_default(),
                )
            {
                return Some(Err(DocumentRefusal::Scalar(index, error)));
            }
        }
        Some(read_entries(&resolved, tokens))
    }
}

pub mod yaml {
    //! YAML 1.2.2 directive acceptance and block-style documents.

    /// One directive line that opens a YAML document.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Directive {
        /// `%YAML major.minor`.
        Version {
            /// The major version number.
            major: u8,
            /// The minor version number.
            minor: u8,
        },
        /// `%TAG handle prefix`.
        Tag,
        /// Any other directive name, which YAML reserves.
        Reserved,
    }

    /// Why a directive is refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DirectiveRefusal {
        /// A second `%YAML` directive in one document (YAML 1.2.2 Sec 6.8.1).
        Repeated,
        /// A `%YAML` directive naming a version other than 1.2.
        Version,
        /// A `%TAG` directive.
        Tag,
    }

    /// Whether the YAML 1.2 route reads a document opened by `directives`,
    /// or the index of the first refused directive and why.
    ///
    /// Definition. The directives are taken in source order. A `%TAG`
    /// directive is refused. A `%YAML` directive after an earlier one is
    /// refused as repeated whatever version it names; otherwise one naming
    /// a version other than 1.2 is refused. A reserved directive is read
    /// past.
    ///
    /// # Errors
    ///
    /// The index of the first refused directive and the reason.
    pub fn read_directives(directives: &[Directive]) -> Result<(), (usize, DirectiveRefusal)> {
        let mut seen_version = false;
        for (index, directive) in directives.iter().enumerate() {
            match *directive {
                Directive::Tag => return Err((index, DirectiveRefusal::Tag)),
                Directive::Version { .. } if seen_version => {
                    return Err((index, DirectiveRefusal::Repeated));
                }
                Directive::Version { major, minor } => {
                    if (major, minor) != (1, 2) {
                        return Err((index, DirectiveRefusal::Version));
                    }
                    seen_version = true;
                }
                Directive::Reserved => {}
            }
        }
        Ok(())
    }

    /// A YAML document in the block style the reference renders: a scalar
    /// token, a mapping from distinct keys, or a sequence.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Node {
        /// A scalar written as this one-line token.
        Scalar(String),
        /// A mapping, entries in order.
        Mapping(Vec<(String, Self)>),
        /// A sequence, items in order.
        Sequence(Vec<Self>),
    }

    /// A YAML document read whole.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Read {
        /// A scalar value.
        Value(crate::value::Value),
        /// A mapping, entries in order.
        Mapping(Vec<(String, Self)>),
        /// A sequence, items in order.
        Sequence(Vec<Self>),
    }

    fn indent(out: &mut String, columns: usize) {
        out.extend(std::iter::repeat_n(' ', columns));
    }

    fn render_value(out: &mut String, node: &Node, columns: usize) {
        match node {
            Node::Scalar(token) if token.is_empty() => out.push('\n'),
            Node::Scalar(token) => {
                out.push(' ');
                out.push_str(token);
                out.push('\n');
            }
            Node::Mapping(entries) if entries.is_empty() => out.push_str(" {}\n"),
            Node::Sequence(items) if items.is_empty() => out.push_str(" []\n"),
            collection => {
                out.push('\n');
                render_block_at(out, collection, columns + 2);
            }
        }
    }

    fn render_block_at(out: &mut String, node: &Node, columns: usize) {
        match node {
            Node::Scalar(token) => {
                indent(out, columns);
                out.push_str(token);
                out.push('\n');
            }
            Node::Mapping(entries) if entries.is_empty() => {
                indent(out, columns);
                out.push_str("{}\n");
            }
            Node::Sequence(items) if items.is_empty() => {
                indent(out, columns);
                out.push_str("[]\n");
            }
            Node::Mapping(entries) => {
                for (key, value) in entries {
                    indent(out, columns);
                    out.push_str(key);
                    out.push(':');
                    render_value(out, value, columns);
                }
            }
            Node::Sequence(items) => {
                for item in items {
                    indent(out, columns);
                    out.push('-');
                    render_value(out, item, columns);
                }
            }
        }
    }

    /// The block-style text of a document.
    ///
    /// Definition. A mapping writes one `key:` line per entry and a
    /// sequence one `-` line per item, at the collection's indentation; a
    /// scalar follows its indicator after one space on the same line, an
    /// empty scalar writes nothing after it, an empty collection is `{}`
    /// or `[]`, and any other collection follows on the next lines,
    /// indented two columns further. A top-level scalar is its token on one
    /// line.
    #[must_use]
    pub fn render_block(node: &Node) -> String {
        let mut out = String::new();
        render_block_at(&mut out, node, 0);
        out
    }

    /// Read a block-style document whole.
    ///
    /// Definition. A mapping reads as its entries in order and a sequence
    /// as its items in order, each value read the same way, and a scalar as
    /// the YAML scalar function [`crate::value_io::parse`] applied to its
    /// token; the first scalar, in document order, the scalar function
    /// refuses is the document's refusal.
    ///
    /// # Errors
    ///
    /// The scalar function's refusal of the first refused token.
    pub fn read_block_document(node: &Node) -> Result<Read, crate::value_io::ValueIoError> {
        Ok(match node {
            Node::Scalar(token) => Read::Value(crate::value_io::parse(
                crate::value_io::Route::Yaml,
                token.as_bytes(),
            )?),
            Node::Mapping(entries) => Read::Mapping(
                entries
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), read_block_document(value)?)))
                    .collect::<Result<_, _>>()?,
            ),
            Node::Sequence(items) => Read::Sequence(
                items
                    .iter()
                    .map(read_block_document)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}
