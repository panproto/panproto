//! The Ansible playbook, task-file and variables-file reader and its
//! canonical writer.
//!
//! [`read_ansible`] reads one file of a named [`FileClass`], as
//! ansible-core 2.22 defines its keywords, through the shared YAML route
//! ([`super::tree::read_yaml`]) into a schema of the `ansible` protocol, and
//! refuses with an [`AnsibleError`] naming the field at fault (its path and
//! byte offset) every file the reader does not carry exactly. A module's
//! arguments, a lookup's terms and every templated string are kept as
//! written: the reader resolves no plugin and evaluates no template.
//! [`write_ansible`] writes such a schema back as the canonical file: one
//! line of JSON, which YAML 1.2 reads as the same document.
//!
//! A vertex id is `/` followed by the path of its node in the file (`/[0]`
//! for the first play, `/[0].tasks[1]` for its second task); a variable is
//! `<owner>.vars.<name>`, or `/<name>` in a variables file.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use panproto_inst::Value;
use panproto_schema::{Schema, SchemaBuilder};

use super::tree::{Content, JsonTextError, Node, json_string, read_yaml};

/// The ansible-core release whose playbook keywords the reader follows.
pub const ANSIBLE_CORE_VERSION: &str = "2.22.0";

/// The keywords every play, block and task accepts (ansible-core's `Base`).
const BASE: &[&str] = &[
    "name",
    "connection",
    "port",
    "remote_user",
    "vars",
    "module_defaults",
    "environment",
    "no_log",
    "run_once",
    "ignore_errors",
    "ignore_unreachable",
    "check_mode",
    "diff",
    "any_errors_fatal",
    "throttle",
    "timeout",
    "debugger",
    "become",
    "become_method",
    "become_user",
    "become_flags",
    "become_exe",
    "tags",
    "collections",
];

/// The keywords a play adds.
const PLAY: &[&str] = &[
    "hosts",
    "gather_facts",
    "gather_subset",
    "gather_timeout",
    "fact_path",
    "vars_files",
    "vars_prompt",
    "validate_argspec",
    "roles",
    "handlers",
    "pre_tasks",
    "post_tasks",
    "tasks",
    "force_handlers",
    "max_fail_percentage",
    "serial",
    "strategy",
    "order",
];

/// The keywords a block and a task share beyond [`BASE`].
const BLOCK_AND_TASK: &[&str] = &["when", "notify", "delegate_to", "delegate_facts"];

/// The keywords a task adds.
const TASK: &[&str] = &[
    "args",
    "async",
    "changed_when",
    "delay",
    "failed_when",
    "loop",
    "loop_control",
    "poll",
    "register",
    "retries",
    "until",
];

/// The keywords a role entry adds beyond [`BASE`].
const ROLE: &[&str] = &["when", "delegate_to", "delegate_facts"];

/// The play sections that hold tasks, in the order the reader reads them.
const SECTIONS: &[&str] = &["pre_tasks", "tasks", "post_tasks", "handlers"];

/// The keys of a playbook entry that imports another playbook.
const IMPORT_KEYS: &[&str] = &[
    "import_playbook",
    "ansible.builtin.import_playbook",
    "name",
    "vars",
    "when",
    "tags",
];

/// The modules that include or import another file, with the kind of file
/// each names and the argument key that names it.
const INCLUDES: &[(&str, &str, &str)] = &[
    ("include_tasks", "tasks", "file"),
    ("import_tasks", "tasks", "file"),
    ("include_role", "role", "name"),
    ("import_role", "role", "name"),
    ("include_vars", "vars-file", "file"),
];

/// The names Ansible refuses as variable names besides non-identifiers.
const RESERVED: &[&str] = &["true", "false", "none", "True", "False", "None", "not"];

/// The class of an Ansible file, which the file itself does not state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClass {
    /// A playbook: a sequence of plays and playbook imports.
    Playbook,
    /// A task file (a role's `tasks/main.yml` or `handlers/main.yml`): a
    /// sequence of tasks and blocks.
    Tasks,
    /// A variables file (a role's `defaults/main.yml` or `vars/main.yml`,
    /// `group_vars`, `host_vars`): a mapping of variables.
    Vars,
}

/// A refusal of the Ansible reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnsibleError {
    /// The path of the field at fault, keys joined by `.` and sequence
    /// indices as `[i]`; empty for the document itself.
    pub path: String,
    /// The byte offset at which the field's value starts.
    pub offset: usize,
    /// What is wrong.
    pub kind: AnsibleErrorKind,
}

impl fmt::Display for AnsibleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the document"
        } else {
            &self.path
        };
        write!(f, "{path} (byte {}): {}", self.offset, self.kind)
    }
}

impl std::error::Error for AnsibleError {}

/// What an [`AnsibleError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AnsibleErrorKind {
    /// The shared YAML route refuses the document.
    #[error("the YAML route refuses the document: {0}")]
    Syntax(String),
    /// A required field is absent.
    #[error("the required field {0} is missing")]
    Missing(&'static str),
    /// A field holds a value of another kind.
    #[error("expected {0}")]
    WrongType(&'static str),
    /// A field breaks a rule of ansible-core, which the message states.
    #[error("{0}")]
    Invalid(&'static str),
    /// A key that is not a keyword where no module may stand.
    #[error("unknown field {0:?}")]
    UnknownField(String),
    /// A feature the reader does not carry.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// A variable name ansible-core refuses.
    #[error("{0:?} is not a valid variable name")]
    VariableName(String),
    /// A plain scalar that YAML 1.1, which ansible-core reads, reads as a
    /// number or a date and YAML 1.2 does not (`0644`, `1_000`, `1:30`,
    /// `2001-12-14`, `0b11`), or the reverse (`0o644`).
    #[error("the plain scalar {0} reads differently in YAML 1.1; quote it")]
    Yaml11Scalar(String),
    /// A local tag other than `!unsafe` and `!vault`.
    #[error("the tag !{0} is not an Ansible tag")]
    UnknownTag(String),
    /// The schema the file describes cannot be built.
    #[error("the schema cannot be built: {0}")]
    Schema(String),
}

/// A reference the file makes to something outside it: a role, a
/// variables file, a playbook or task file, or a handler no play of the
/// file declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    /// The path of the reference.
    pub path: String,
    /// The byte offset of the reference.
    pub offset: usize,
    /// What it names: `role`, `vars-file`, `playbook`, `tasks` or
    /// `handler`.
    pub kind: &'static str,
    /// The name when it is written as a string, `None` when it is not.
    pub name: Option<String>,
}

/// What [`read_ansible`] reads: the schema, and the references that stay
/// unresolved, in document order.
#[derive(Clone, Debug)]
pub struct AnsibleReading {
    /// The schema of the `ansible` protocol.
    pub schema: Schema,
    /// The references to things outside the file.
    pub external: Vec<External>,
}

type R<T> = Result<T, AnsibleError>;

const fn error(path: String, offset: usize, kind: AnsibleErrorKind) -> AnsibleError {
    AnsibleError { path, offset, kind }
}

fn child(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn get<'n>(entries: &'n [(String, Node)], key: &str) -> Option<&'n Node> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn mapping<'n>(node: &'n Node, path: &str) -> R<&'n [(String, Node)]> {
    node.mapping().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::WrongType("a mapping"),
        )
    })
}

fn sequence<'n>(node: &'n Node, path: &str) -> R<&'n [Node]> {
    node.sequence().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::WrongType("a sequence"),
        )
    })
}

fn string<'n>(node: &'n Node, path: &str) -> R<&'n str> {
    node.string().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::WrongType("a string"),
        )
    })
}

/// A string, or a sequence of strings.
fn strings<'n>(node: &'n Node, path: &str) -> R<Vec<(String, &'n Node)>> {
    if let Content::Sequence(items) = &node.content {
        return items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let at = format!("{path}[{i}]");
                string(item, &at)?;
                Ok((at, item))
            })
            .collect();
    }
    string(node, path)?;
    Ok(vec![(path.to_owned(), node)])
}

/// Whether ansible-core accepts `name` as a variable name: an ASCII
/// identifier other than the Jinja singletons and `not`.
#[must_use]
pub fn variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !RESERVED.contains(&name)
}

fn check_variable(name: &str, node: &Node, path: &str) -> R<()> {
    if variable_name(name) {
        Ok(())
    } else {
        Err(error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::VariableName(name.to_owned()),
        ))
    }
}

/// Read `source`, one Ansible file of class `class` in YAML or JSON, into
/// an `ansible` schema.
///
/// The order of the checks, and so which refusal comes first, is the one
/// the Ansible page of the book states.
///
/// # Errors
///
/// Returns an [`AnsibleError`] naming the field at fault.
pub fn read_ansible(source: &[u8], class: FileClass) -> Result<AnsibleReading, AnsibleError> {
    let document = read_yaml(source)
        .map_err(|e| error(String::new(), 0, AnsibleErrorKind::Syntax(e.to_string())))?;
    let document = untag(&document, "", source)?;
    read_document(&document, class)
}

/// The booleans YAML 1.1 spells as words that YAML 1.2 reads as strings.
const YAML11_BOOLEANS: &[(&str, bool)] = &[
    ("yes", true),
    ("Yes", true),
    ("YES", true),
    ("on", true),
    ("On", true),
    ("ON", true),
    ("no", false),
    ("No", false),
    ("NO", false),
    ("off", false),
    ("Off", false),
    ("OFF", false),
];

/// The value YAML 1.1 gives a plain scalar `text` that the YAML 1.2 core
/// schema reads as a string, when it is an octal integer (`0644`).
fn yaml11_octal(text: &str) -> Option<i64> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let digits = unsigned.strip_prefix('0')?;
    if digits.is_empty() || !digits.chars().all(|c| matches!(c, '0'..='7' | '_')) {
        return None;
    }
    let value = i64::from_str_radix(&digits.replace('_', ""), 8).ok()?;
    Some(if negative { -value } else { value })
}

/// Whether a plain scalar `text` that the YAML 1.2 core schema reads as a
/// string is a number or a date under YAML 1.1 that the reader does not
/// carry: a binary integer (`0b11`), a number with `_` separators
/// (`1_000`), a sexagesimal number (`1:30`), or a timestamp
/// (`2001-12-14`).
fn yaml11_number_or_date(text: &str) -> bool {
    let unsigned = text.trim_start_matches(['-', '+']);
    let digits = |t: &str, ok: fn(char) -> bool| !t.is_empty() && t.chars().all(ok);
    let binary = unsigned
        .strip_prefix("0b")
        .is_some_and(|t| digits(t, |c| matches!(c, '0' | '1' | '_')));
    let separated = unsigned.contains('_')
        && unsigned.starts_with(|c: char| c.is_ascii_digit())
        && digits(unsigned, |c| c.is_ascii_digit() || matches!(c, '_' | '.'));
    let sexagesimal = unsigned.contains(':')
        && unsigned.starts_with(|c: char| c.is_ascii_digit())
        && digits(unsigned, |c| {
            c.is_ascii_digit() || matches!(c, '_' | ':' | '.')
        });
    let bytes = text.as_bytes();
    let date = bytes.len() >= 8
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && text[5..]
            .split(['-', 'T', 't', ' '])
            .take(2)
            .all(|part| matches!(part.len(), 1 | 2) && part.chars().all(|c| c.is_ascii_digit()));
    binary || separated || sexagesimal || date
}

/// Read a plain scalar as ansible-core's YAML 1.1 loader reads it: a word
/// boolean (`yes`, `off`) as its boolean and an octal integer (`0755`) as
/// its value; refuse a plain scalar whose YAML 1.1 value is another number
/// or a date the YAML 1.2 route reads otherwise.
fn yaml11(node: &Node, path: &str, source: &[u8]) -> R<Content> {
    let first = source.get(node.offset).copied().unwrap_or(b'"');
    let plain = !matches!(first, b'"' | b'\'' | b'|' | b'>' | b'!');
    let divergent = |text: &str| {
        Err(error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::Yaml11Scalar(text.to_owned()),
        ))
    };
    match &node.content {
        Content::Scalar(Value::Str(text)) if plain => {
            if let Some((_, flag)) = YAML11_BOOLEANS.iter().find(|(w, _)| *w == text) {
                return Ok(Content::Scalar(Value::Bool(*flag)));
            }
            if let Some(value) = yaml11_octal(text) {
                return Ok(Content::Scalar(Value::Int(value)));
            }
            if yaml11_number_or_date(text) {
                return divergent(text);
            }
        }
        Content::Scalar(Value::Int(_)) if plain => {
            let token = &source[node.offset..];
            let unsigned = token
                .strip_prefix(b"-")
                .or_else(|| token.strip_prefix(b"+"))
                .unwrap_or(token);
            if unsigned.starts_with(b"0o") {
                let end = token
                    .iter()
                    .position(|b| b.is_ascii_whitespace() || matches!(b, b',' | b']' | b'}'))
                    .unwrap_or(token.len());
                return divergent(&String::from_utf8_lossy(&token[..end]));
            }
        }
        _ => {}
    }
    Ok(node.content.clone())
}

/// The document with `!vault x` read as `{"__ansible_vault": x}` and
/// `!unsafe x` as `{"__ansible_unsafe": x}`, the forms ansible-core's JSON
/// encoder writes them in, and every plain scalar read as YAML 1.1 reads
/// it ([`yaml11`]).
fn untag(node: &Node, path: &str, source: &[u8]) -> R<Node> {
    let content = match &node.content {
        Content::Scalar(Value::Opaque { type_, fields }) => {
            let tag = type_.trim_start_matches('!');
            let key = match tag {
                "vault" => "__ansible_vault",
                "unsafe" => "__ansible_unsafe",
                _ => {
                    return Err(error(
                        path.to_owned(),
                        node.offset,
                        AnsibleErrorKind::UnknownTag(tag.to_owned()),
                    ));
                }
            };
            let payload = match fields.get("value") {
                Some(Value::Str(text)) => text.clone(),
                _ => String::new(),
            };
            let argument = Node {
                content: Content::Scalar(Value::Str(payload)),
                offset: node.offset,
            };
            Content::Mapping(vec![(key.to_owned(), argument)])
        }
        Content::Scalar(_) => yaml11(node, path, source)?,
        Content::Sequence(items) => Content::Sequence(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| untag(item, &format!("{path}[{i}]"), source))
                .collect::<R<_>>()?,
        ),
        Content::Mapping(entries) => Content::Mapping(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), untag(value, &child(path, key), source)?)))
                .collect::<R<_>>()?,
        ),
    };
    Ok(Node {
        content,
        offset: node.offset,
    })
}

/// The reader's state.
struct Reader {
    builder: Option<SchemaBuilder>,
    external: Vec<External>,
    /// The handlers of the play being read, by name and by `listen` topic.
    handlers: BTreeMap<String, Vec<String>>,
    /// Whether a play is being read, so that a handler may resolve.
    in_play: bool,
    /// The `notify` edges of the play being read, added once its handlers
    /// exist, in the order they were met.
    pending: Vec<(String, String, String, usize)>,
    edges: BTreeSet<(String, String, String, Option<String>)>,
}

impl Reader {
    fn try_with(
        &mut self,
        path: &str,
        offset: usize,
        step: impl FnOnce(SchemaBuilder) -> Result<SchemaBuilder, panproto_schema::SchemaError>,
    ) -> R<()> {
        let Some(builder) = self.builder.take() else {
            return Ok(());
        };
        let built = step(builder).map_err(|e| {
            error(
                path.to_owned(),
                offset,
                AnsibleErrorKind::Schema(e.to_string()),
            )
        })?;
        self.builder = Some(built);
        Ok(())
    }

    fn vertex(&mut self, id: &str, kind: &str, at: (&str, usize)) -> R<()> {
        self.try_with(at.0, at.1, |b| b.vertex(id, kind, None))
    }

    fn edge(
        &mut self,
        src: &str,
        tgt: &str,
        kind: &str,
        name: Option<&str>,
        at: (&str, usize),
    ) -> R<()> {
        let key = (
            src.to_owned(),
            tgt.to_owned(),
            kind.to_owned(),
            name.map(str::to_owned),
        );
        if self.edges.insert(key) {
            self.try_with(at.0, at.1, |b| b.edge(src, tgt, kind, name))?;
        }
        Ok(())
    }

    fn keep(&mut self, vertex: &str, sort: &str, node: &Node, path: &str) -> R<()> {
        let text = json(node, path)?;
        self.keep_text(vertex, sort, &text);
        Ok(())
    }

    fn keep_text(&mut self, vertex: &str, sort: &str, text: &str) {
        self.builder = self
            .builder
            .take()
            .map(|b| b.constraint(vertex, sort, text));
    }

    fn external(&mut self, path: &str, node: &Node, kind: &'static str, name: Option<&str>) {
        self.external.push(External {
            path: path.to_owned(),
            offset: node.offset,
            kind,
            name: name.map(str::to_owned),
        });
    }
}

/// `node`'s canonical JSON text, or the refusal of the scalar JSON cannot
/// write.
fn json(node: &Node, path: &str) -> R<String> {
    node.json_text().map_err(|e: JsonTextError| {
        error(
            path.to_owned(),
            node.offset,
            AnsibleErrorKind::Unsupported(e.feature()),
        )
    })
}

/// Read a document tree, with its tags read, into an `ansible` schema, as
/// [`read_ansible`] does.
///
/// # Errors
///
/// Returns an [`AnsibleError`] naming the field at fault.
pub fn read_document(document: &Node, class: FileClass) -> Result<AnsibleReading, AnsibleError> {
    let proto = panproto_protocols::config::ansible::protocol();
    let mut reader = Reader {
        builder: Some(SchemaBuilder::new(&proto)),
        external: Vec::new(),
        handlers: BTreeMap::new(),
        in_play: false,
        pending: Vec::new(),
        edges: BTreeSet::new(),
    };
    match class {
        FileClass::Playbook => {
            let items = sequence(document, "")?;
            if items.is_empty() {
                return Err(error(
                    String::new(),
                    document.offset,
                    AnsibleErrorKind::Invalid("a playbook holds at least one play"),
                ));
            }
            reader.vertex("/", "playbook", ("", document.offset))?;
            for (i, item) in items.iter().enumerate() {
                read_entry(&mut reader, item, &format!("[{i}]"))?;
            }
        }
        FileClass::Tasks => {
            let items = sequence(document, "")?;
            reader.vertex("/", "task-file", ("", document.offset))?;
            read_tasks(&mut reader, "/", "tasks", items, "", false)?;
        }
        FileClass::Vars => {
            reader.vertex("/", "vars-file", ("", document.offset))?;
            read_vars(&mut reader, "/", document, "")?;
        }
    }
    let schema = reader.builder.take().map_or_else(
        || {
            Err(error(
                String::new(),
                0,
                AnsibleErrorKind::Schema("no builder".into()),
            ))
        },
        |b| {
            b.build()
                .map_err(|e| error(String::new(), 0, AnsibleErrorKind::Schema(e.to_string())))
        },
    )?;
    Ok(AnsibleReading {
        schema,
        external: reader.external,
    })
}

/// The vertex id of the node at `path`.
fn id_of(path: &str) -> String {
    format!("/{path}")
}

/// Read the variables of `node` (a mapping of variable names, or null) as
/// `variable` vertices joined to `owner` by `var` edges named by the
/// variable, each keeping its value. In a variables file `path` is empty
/// and the ids are `/<name>`.
fn read_vars(reader: &mut Reader, owner: &str, node: &Node, path: &str) -> R<()> {
    read_variables(reader, owner, node, path, "var")
}

/// Read `node`'s variables as [`read_vars`] does, joined by edges of kind
/// `edge`.
fn read_variables(reader: &mut Reader, owner: &str, node: &Node, path: &str, edge: &str) -> R<()> {
    if matches!(node.content, Content::Scalar(Value::Null)) {
        return Ok(());
    }
    let entries = mapping(node, path)?;
    for (name, value) in entries {
        check_variable(name, value, &child(path, name))?;
    }
    for (name, value) in entries {
        let at = child(path, name);
        let id = id_of(&at);
        reader.vertex(&id, "variable", (&at, value.offset))?;
        reader.edge(owner, &id, edge, Some(name), (&at, value.offset))?;
        reader.keep(&id, "value", value, &at)?;
    }
    Ok(())
}

fn read_entry(reader: &mut Reader, item: &Node, path: &str) -> R<()> {
    let entries = mapping(item, path)?;
    let import = entries
        .iter()
        .any(|(k, _)| k == "import_playbook" || k == "ansible.builtin.import_playbook");
    if import {
        read_import(reader, item, entries, path)
    } else {
        read_play(reader, item, entries, path)
    }
}

fn read_import(reader: &mut Reader, item: &Node, entries: &[(String, Node)], path: &str) -> R<()> {
    let mut target = None;
    for (key, value) in entries {
        let at = child(path, key);
        if !IMPORT_KEYS.contains(&key.as_str()) {
            return Err(error(
                at,
                value.offset,
                AnsibleErrorKind::UnknownField(key.clone()),
            ));
        }
        if key.ends_with("import_playbook") {
            if target.is_some() {
                return Err(error(
                    at,
                    value.offset,
                    AnsibleErrorKind::Invalid("an entry imports one playbook"),
                ));
            }
            target = Some((at, value));
        }
    }
    let id = id_of(path);
    reader.vertex(&id, "playbook-import", (path, item.offset))?;
    reader.edge("/", &id, "entry", None, (path, item.offset))?;
    if let Some(name) = get(entries, "name") {
        string(name, &child(path, "name"))?;
    }
    for (key, value) in entries {
        let at = child(path, key);
        match key.as_str() {
            "vars" => {}
            k if k.ends_with("import_playbook") => {
                let name = string(value, &at)?.to_owned();
                reader.external(&at, value, "playbook", Some(&name));
                reader.keep(&id, "import_playbook", value, &at)?;
            }
            _ => reader.keep(&id, key, value, &at)?,
        }
    }
    if let Some(vars) = get(entries, "vars") {
        read_vars(reader, &id, vars, &child(path, "vars"))?;
    }
    Ok(())
}

/// Collect the handlers of a play's `handlers` section, by name and by
/// `listen` topic, with their vertex ids, as the reader will build them.
fn collect_handlers(items: &[Node], path: &str, out: &mut BTreeMap<String, Vec<String>>) {
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}[{i}]");
        let Some(entries) = item.mapping() else {
            continue;
        };
        if get(entries, "block").is_some() {
            for key in ["block", "rescue", "always"] {
                if let Some(children) = get(entries, key).and_then(Node::sequence) {
                    collect_handlers(children, &child(&at, key), out);
                }
            }
            continue;
        }
        let id = id_of(&at);
        let mut names: Vec<&str> = get(entries, "name")
            .and_then(Node::string)
            .into_iter()
            .collect();
        if let Some(listen) = get(entries, "listen") {
            match &listen.content {
                Content::Sequence(items) => names.extend(items.iter().filter_map(Node::string)),
                _ => names.extend(listen.string()),
            }
        }
        for name in names {
            let ids = out.entry(name.to_owned()).or_default();
            if !ids.contains(&id) {
                ids.push(id.clone());
            }
        }
    }
}

fn read_play(reader: &mut Reader, item: &Node, entries: &[(String, Node)], path: &str) -> R<()> {
    for (key, value) in entries {
        if !BASE.contains(&key.as_str()) && !PLAY.contains(&key.as_str()) {
            return Err(error(
                child(path, key),
                value.offset,
                AnsibleErrorKind::UnknownField(key.clone()),
            ));
        }
    }
    if get(entries, "hosts").is_none() {
        return Err(error(
            path.to_owned(),
            item.offset,
            AnsibleErrorKind::Missing("hosts"),
        ));
    }
    let id = id_of(path);
    reader.vertex(&id, "play", (path, item.offset))?;
    reader.edge("/", &id, "entry", None, (path, item.offset))?;
    for (key, value) in entries {
        let at = child(path, key);
        match key.as_str() {
            "vars" | "roles" | "pre_tasks" | "tasks" | "post_tasks" | "handlers" => continue,
            "name" => {
                string(value, &at)?;
            }
            "vars_files" => {
                for (j, file) in sequence(value, &at)?.iter().enumerate() {
                    for (file_at, name) in strings(file, &format!("{at}[{j}]"))? {
                        reader.external(&file_at, name, "vars-file", name.string());
                    }
                }
            }
            _ => {}
        }
        reader.keep(&id, key, value, &at)?;
    }
    if let Some(vars) = get(entries, "vars") {
        read_vars(reader, &id, vars, &child(path, "vars"))?;
    }
    if let Some(roles) = get(entries, "roles") {
        let at = child(path, "roles");
        for (j, role) in sequence(roles, &at)?.iter().enumerate() {
            read_role(reader, &id, role, &format!("{at}[{j}]"))?;
        }
    }
    reader.handlers.clear();
    if let Some(handlers) = get(entries, "handlers").and_then(Node::sequence) {
        let mut found = BTreeMap::new();
        collect_handlers(handlers, &child(path, "handlers"), &mut found);
        reader.handlers = found;
    }
    reader.in_play = true;
    for section in SECTIONS {
        if let Some(tasks) = get(entries, section) {
            let at = child(path, section);
            let items = sequence(tasks, &at)?;
            read_tasks(reader, &id, section, items, &at, *section == "handlers")?;
        }
    }
    reader.in_play = false;
    reader.handlers.clear();
    for (src, tgt, at, offset) in std::mem::take(&mut reader.pending) {
        reader.edge(&src, &tgt, "notify", None, (&at, offset))?;
    }
    Ok(())
}

/// Read a role entry of a play: a role name, or a mapping naming the role
/// by `role` (or, without `role`, by `name`) beside its keywords, its
/// `vars` and its parameters, which are variables of the role.
fn read_role(reader: &mut Reader, play: &str, role: &Node, path: &str) -> R<()> {
    let id = id_of(path);
    let (name_node, name_at, entries): (&Node, String, &[(String, Node)]) = match &role.content {
        Content::Scalar(_) => (role, path.to_owned(), &[]),
        Content::Mapping(entries) => {
            let key = if get(entries, "role").is_some() {
                "role"
            } else {
                "name"
            };
            let Some(name) = get(entries, key) else {
                return Err(error(
                    path.to_owned(),
                    role.offset,
                    AnsibleErrorKind::Missing("role"),
                ));
            };
            (name, child(path, key), entries.as_slice())
        }
        Content::Sequence(_) => {
            return Err(error(
                path.to_owned(),
                role.offset,
                AnsibleErrorKind::WrongType("a string or a mapping"),
            ));
        }
    };
    let name = string(name_node, &name_at)?.to_owned();
    let mut params = Vec::new();
    for (key, value) in entries {
        let at = child(path, key);
        if at == name_at || key == "vars" {
            continue;
        }
        if !BASE.contains(&key.as_str()) && !ROLE.contains(&key.as_str()) {
            check_variable(key, value, &at)?;
            params.push((key.clone(), value.clone()));
        }
    }
    reader.vertex(&id, "role", (path, role.offset))?;
    reader.edge(play, &id, "roles", None, (path, role.offset))?;
    reader.keep(&id, "role", name_node, &name_at)?;
    for (key, value) in entries {
        let at = child(path, key);
        let keyword = BASE.contains(&key.as_str()) || ROLE.contains(&key.as_str());
        if at != name_at && key != "vars" && keyword {
            check_keyword(key, value, &at)?;
            reader.keep(&id, key, value, &at)?;
        }
    }
    if let Some(vars) = get(entries, "vars") {
        read_vars(reader, &id, vars, &child(path, "vars"))?;
    }
    if !params.is_empty() {
        let node = Node {
            content: Content::Mapping(params),
            offset: role.offset,
        };
        read_variables(reader, &id, &node, path, "param")?;
    }
    reader.external(&name_at, name_node, "role", Some(&name));
    Ok(())
}

/// Read the tasks (or, under `handlers`, the handlers) `items`, joining
/// each to `parent` by an edge of kind `edge`.
fn read_tasks(
    reader: &mut Reader,
    parent: &str,
    edge: &str,
    items: &[Node],
    path: &str,
    handlers: bool,
) -> R<()> {
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}[{i}]");
        let entries = mapping(item, &at)?;
        let id = id_of(&at);
        if get(entries, "block").is_some() {
            read_block(reader, item, entries, &at, handlers)?;
        } else {
            read_task(reader, item, entries, &at, handlers)?;
        }
        reader.edge(parent, &id, edge, None, (&at, item.offset))?;
    }
    Ok(())
}

fn read_block(
    reader: &mut Reader,
    item: &Node,
    entries: &[(String, Node)],
    path: &str,
    handlers: bool,
) -> R<()> {
    for (key, value) in entries {
        let known = BASE.contains(&key.as_str())
            || BLOCK_AND_TASK.contains(&key.as_str())
            || matches!(key.as_str(), "block" | "rescue" | "always");
        if !known {
            return Err(error(
                child(path, key),
                value.offset,
                AnsibleErrorKind::UnknownField(key.clone()),
            ));
        }
    }
    let id = id_of(path);
    reader.vertex(&id, "block", (path, item.offset))?;
    keep_fields(reader, &id, entries, path)?;
    for key in ["block", "rescue", "always"] {
        if let Some(children) = get(entries, key) {
            let at = child(path, key);
            let items = sequence(children, &at)?;
            read_tasks(reader, &id, key, items, &at, handlers)?;
        }
    }
    Ok(())
}

/// Check one keyword of a block or task: a string `name`, a string or
/// sequence of strings for `notify` and `listen`, a mapping `loop_control`,
/// and a `register` naming a variable (or a mapping of them).
fn check_keyword(key: &str, value: &Node, at: &str) -> R<()> {
    match key {
        "name" => {
            string(value, at)?;
        }
        "notify" | "listen" => {
            strings(value, at)?;
        }
        "loop_control" => {
            mapping(value, at)?;
        }
        "register" => match &value.content {
            Content::Mapping(names) => {
                for (name, node) in names {
                    check_variable(name, node, &child(at, name))?;
                }
            }
            _ => check_variable(string(value, at)?, value, at)?,
        },
        _ => {}
    }
    Ok(())
}

/// Check and keep the keywords of a block, in source order, then read its
/// variables and resolve its `notify`.
fn keep_fields(reader: &mut Reader, id: &str, entries: &[(String, Node)], path: &str) -> R<()> {
    for (key, value) in entries {
        if matches!(key.as_str(), "vars" | "block" | "rescue" | "always") {
            continue;
        }
        let at = child(path, key);
        check_keyword(key, value, &at)?;
        reader.keep(id, key, value, &at)?;
    }
    vars_and_notify(reader, id, entries, path)
}

/// Read the variables of a block or task and resolve its `notify` names
/// against the handlers of the play, reporting each name no handler
/// answers to.
fn vars_and_notify(reader: &mut Reader, id: &str, entries: &[(String, Node)], path: &str) -> R<()> {
    if let Some(vars) = get(entries, "vars") {
        read_vars(reader, id, vars, &child(path, "vars"))?;
    }
    if let Some(notify) = get(entries, "notify") {
        for (at, name) in strings(notify, &child(path, "notify"))? {
            let text = name.string().unwrap_or_default();
            let targets = if reader.in_play {
                reader.handlers.get(text).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            if targets.is_empty() {
                reader.external(&at, name, "handler", Some(text));
            }
            for target in targets {
                reader
                    .pending
                    .push((id.to_owned(), target, at.clone(), name.offset));
            }
        }
    }
    Ok(())
}

/// The module key of a task, after checking that the task has exactly one
/// and at most one loop: every key that is not a keyword of a task (or,
/// in a handler, `listen`) and does not begin with `with_` names a module.
fn module_key<'e>(
    entries: &'e [(String, Node)],
    item: &Node,
    path: &str,
    handlers: bool,
) -> R<&'e str> {
    let mut module = None;
    let mut looped = false;
    for (key, value) in entries {
        let at = child(path, key);
        let k = key.as_str();
        let keyword = BASE.contains(&k)
            || BLOCK_AND_TASK.contains(&k)
            || TASK.contains(&k)
            || (handlers && k == "listen");
        if matches!(k, "action" | "local_action") {
            return Err(error(
                at,
                value.offset,
                AnsibleErrorKind::Unsupported("the action and local_action keywords"),
            ));
        }
        if k == "loop" || k.starts_with("with_") {
            if looped {
                return Err(error(
                    at,
                    value.offset,
                    AnsibleErrorKind::Invalid("a task has one loop"),
                ));
            }
            looped = true;
        }
        if keyword || k.starts_with("with_") {
            continue;
        }
        if module.is_some() {
            return Err(error(
                at,
                value.offset,
                AnsibleErrorKind::Invalid("a task names one module"),
            ));
        }
        module = Some(k);
    }
    module.ok_or_else(|| {
        error(
            path.to_owned(),
            item.offset,
            AnsibleErrorKind::Missing("a module"),
        )
    })
}

fn read_task(
    reader: &mut Reader,
    item: &Node,
    entries: &[(String, Node)],
    path: &str,
    handlers: bool,
) -> R<()> {
    let module = module_key(entries, item, path, handlers)?;
    let id = id_of(path);
    let kind = if handlers { "handler" } else { "task" };
    reader.vertex(&id, kind, (path, item.offset))?;
    for (key, value) in entries {
        let at = child(path, key);
        if key == module {
            reader.keep_text(&id, "module", &json_string(module));
            reader.keep(&id, "arguments", value, &at)?;
            include(reader, module, value, &at);
        } else if let Some(lookup) = key.strip_prefix("with_") {
            reader.keep_text(&id, "with", &json_string(lookup));
            reader.keep(&id, "with_terms", value, &at)?;
        } else if key != "vars" {
            check_keyword(key, value, &at)?;
            reader.keep(&id, key, value, &at)?;
        }
    }
    vars_and_notify(reader, &id, entries, path)
}

/// Report the file or role an include or import module names.
fn include(reader: &mut Reader, module: &str, value: &Node, at: &str) {
    let short = module
        .strip_prefix("ansible.builtin.")
        .or_else(|| module.strip_prefix("ansible.legacy."))
        .unwrap_or(module);
    let Some((_, kind, key)) = INCLUDES.iter().find(|(m, ..)| *m == short) else {
        return;
    };
    let name = match &value.content {
        Content::Mapping(entries) => get(entries, key).and_then(Node::string),
        _ => value.string(),
    };
    reader.external(at, value, kind, name);
}

/// Why [`write_ansible`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AnsibleWriteError {
    /// The schema has no root vertex `/` of a file kind.
    #[error("the schema has no file vertex")]
    File,
    /// A vertex the writer reached has a constraint whose value is not
    /// JSON text.
    #[error("constraint {sort} of {vertex} is not JSON text")]
    Constraint {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
    /// A vertex has no place in an Ansible file.
    #[error("{0} has no place in an Ansible file")]
    Structure(String),
}

type W<T> = Result<T, AnsibleWriteError>;

/// Write an `ansible` schema as its canonical file.
///
/// The canonical file is one line of JSON with no whitespace between
/// tokens, ended by a line feed: a playbook or task file as a sequence, a
/// variables file as a mapping. A play, import, block or task writes its
/// kept keywords in source order (a task's module as its key with its
/// arguments, a loop over a lookup as its `with_` key), then `vars`; a play
/// then writes `roles`, `pre_tasks`, `tasks`, `post_tasks` and `handlers`,
/// and a block `block`, `rescue` and `always`, each when it holds
/// anything; a role writes `role` and then its parameters. Reading the text
/// back with [`read_ansible`] under the same class gives the schema the
/// text was written from.
///
/// # Errors
///
/// Returns an [`AnsibleWriteError`] for a schema no file reads as.
pub fn write_ansible(schema: &Schema) -> Result<String, AnsibleWriteError> {
    let root = schema.vertices.get("/").ok_or(AnsibleWriteError::File)?;
    let mut text = match root.kind.as_ref() {
        "playbook" => list(schema, "/", "entry")?,
        "task-file" => list(schema, "/", "tasks")?,
        "vars-file" => object(&variables(schema, "/", "var")?),
        _ => return Err(AnsibleWriteError::File),
    };
    text.push('\n');
    Ok(text)
}

fn object(fields: &[(String, String)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string(key)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// The targets of `vertex`'s out-edges of kind `kind`, with their names,
/// in the order the schema lists them.
fn targets(schema: &Schema, vertex: &str, kind: &str) -> Vec<(String, Option<String>)> {
    schema
        .outgoing_edges(vertex)
        .iter()
        .filter(|e| e.kind.as_ref() == kind)
        .map(|e| (e.tgt.to_string(), e.name.as_ref().map(ToString::to_string)))
        .collect()
}

/// `vertex`'s kept constraints, each checked to be JSON, in order.
fn kept(schema: &Schema, vertex: &str) -> W<Vec<(String, String)>> {
    schema
        .constraints
        .get(vertex)
        .into_iter()
        .flatten()
        .map(|c| {
            serde_json::from_str::<serde_json::Value>(&c.value).map_err(|_| {
                AnsibleWriteError::Constraint {
                    vertex: vertex.to_owned(),
                    sort: c.sort.to_string(),
                }
            })?;
            Ok((c.sort.to_string(), c.value.clone()))
        })
        .collect()
}

fn list(schema: &Schema, vertex: &str, kind: &str) -> W<String> {
    let items: Vec<String> = targets(schema, vertex, kind)
        .iter()
        .map(|(target, _)| node(schema, target))
        .collect::<W<_>>()?;
    Ok(format!("[{}]", items.join(",")))
}

/// The variables of `vertex`, each its name and its value's JSON text.
fn variables(schema: &Schema, vertex: &str, edge: &str) -> W<Vec<(String, String)>> {
    targets(schema, vertex, edge)
        .into_iter()
        .map(|(target, name)| {
            let value = kept(schema, &target)?
                .into_iter()
                .find(|(sort, _)| sort == "value")
                .map(|(_, text)| text)
                .ok_or_else(|| AnsibleWriteError::Structure(target.clone()))?;
            Ok((name.unwrap_or_default(), value))
        })
        .collect()
}

/// The JSON text of the play, import, role, block, task or handler
/// `vertex`.
fn node(schema: &Schema, vertex: &str) -> W<String> {
    let kind = schema
        .vertices
        .get(vertex)
        .map(|v| v.kind.to_string())
        .ok_or_else(|| AnsibleWriteError::Structure(vertex.to_owned()))?;
    let mut fields = Vec::new();
    let mut module = None;
    let mut lookup = None;
    for (sort, text) in kept(schema, vertex)? {
        match sort.as_str() {
            "module" => {
                module = serde_json::from_str::<String>(&text).ok();
            }
            "arguments" => {
                let key = module
                    .take()
                    .ok_or_else(|| AnsibleWriteError::Structure(vertex.to_owned()))?;
                fields.push((key, text));
            }
            "with" => {
                lookup = serde_json::from_str::<String>(&text).ok();
            }
            "with_terms" => {
                let name = lookup
                    .take()
                    .ok_or_else(|| AnsibleWriteError::Structure(vertex.to_owned()))?;
                fields.push((format!("with_{name}"), text));
            }
            _ => fields.push((sort, text)),
        }
    }
    fields.extend(variables(schema, vertex, "param")?);
    let vars = variables(schema, vertex, "var")?;
    if !vars.is_empty() {
        fields.push(("vars".to_owned(), object(&vars)));
    }
    let children: &[&str] = match kind.as_str() {
        "play" => &["roles", "pre_tasks", "tasks", "post_tasks", "handlers"],
        "block" => &["block", "rescue", "always"],
        _ => &[],
    };
    for key in children {
        if !targets(schema, vertex, key).is_empty() {
            fields.push(((*key).to_owned(), list(schema, vertex, key)?));
        }
    }
    Ok(object(&fields))
}
