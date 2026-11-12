//! Complete reference operations assigned to `ansible`.
//!
//! An Ansible file is modelled as the data tree it denotes (the [`Doc`] of
//! the `kubernetes-crds` page), read under its [`Class`]. [`read`] applies
//! the rules of the `ansible` page, transcribed from the playbook keywords
//! of ansible-core 2.22, in the order the page states them, and returns
//! either the first refusal or the [`Reading`]: the schema's vertices,
//! edges and kept constraints, and the references to things outside the
//! file. [`canonical_text`] is the canonical file the page defines. It
//! shares no code with production, which reads YAML text through the
//! tree-sitter YAML grammar and builds a `panproto_schema::Schema`.

use std::collections::{BTreeMap, BTreeSet};

pub use super::kubernetes_crds::{Doc, json, json_string, m, render_yaml, s};

/// The class of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A playbook.
    Playbook,
    /// A task file.
    Tasks,
    /// A variables file.
    Vars,
}

/// What is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A required field is absent.
    Missing(&'static str),
    /// A value of the wrong kind: `a mapping`, `a sequence`, `a string` or
    /// `a string or a mapping`.
    WrongType(&'static str),
    /// A rule of the page is broken.
    Invalid,
    /// A key that is not a keyword where no module may stand.
    UnknownField(String),
    /// A feature outside the supported set.
    Unsupported(&'static str),
    /// A variable name ansible-core refuses.
    VariableName(String),
}

/// A refusal: the path of the field at fault, and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Keys joined by `.`, indices as `[i]`; empty for the document.
    pub path: String,
    /// What is wrong.
    pub kind: Kind,
}

/// One edge out of a vertex: its kind, its target and its name.
pub type OutEdge = (String, String, Option<String>);

/// What a file reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Each vertex id with its kind.
    pub vertices: BTreeMap<String, String>,
    /// Each vertex's out-edges, in the order they are added.
    pub edges: BTreeMap<String, Vec<OutEdge>>,
    /// Each vertex's kept constraints, sort and JSON text, in order.
    pub constraints: BTreeMap<String, Vec<(String, String)>>,
    /// The references outside the file: path, kind and name, in order.
    pub external: Vec<(String, &'static str, Option<String>)>,
}

const BASE: [&str; 24] = [
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

const PLAY: [&str; 18] = [
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

const SHARED: [&str; 4] = ["when", "notify", "delegate_to", "delegate_facts"];

const TASK: [&str; 11] = [
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

const ROLE: [&str; 3] = ["when", "delegate_to", "delegate_facts"];

const SECTIONS: [&str; 4] = ["pre_tasks", "tasks", "post_tasks", "handlers"];

type R<T> = Result<T, Refusal>;

fn refuse<T>(path: &str, kind: Kind) -> R<T> {
    Err(Refusal {
        path: path.to_owned(),
        kind,
    })
}

fn at(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn field<'d>(entries: &'d [(String, Doc)], key: &str) -> Option<&'d Doc> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn map<'d>(doc: &'d Doc, path: &str) -> R<&'d [(String, Doc)]> {
    match doc {
        Doc::Map(e) => Ok(e),
        _ => refuse(path, Kind::WrongType("a mapping")),
    }
}

fn seq<'d>(doc: &'d Doc, path: &str) -> R<&'d [Doc]> {
    match doc {
        Doc::Seq(i) => Ok(i),
        _ => refuse(path, Kind::WrongType("a sequence")),
    }
}

fn text<'d>(doc: &'d Doc, path: &str) -> R<&'d str> {
    match doc {
        Doc::Str(t) => Ok(t),
        _ => refuse(path, Kind::WrongType("a string")),
    }
}

/// A string, or each string of a sequence, with its path.
fn names<'d>(doc: &'d Doc, path: &str) -> R<Vec<(String, &'d str)>> {
    match doc {
        Doc::Seq(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let p = format!("{path}[{i}]");
                let t = text(item, &p)?;
                out.push((p, t));
            }
            Ok(out)
        }
        other => Ok(vec![(path.to_owned(), text(other, path)?)]),
    }
}

/// Whether ansible-core accepts `name` as a variable name.
#[must_use]
pub fn variable_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let head = bytes
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_');
    let tail = bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
    let reserved = ["true", "false", "none", "True", "False", "None", "not"];
    head && tail && !reserved.contains(&name)
}

fn variable(name: &str, path: &str) -> R<()> {
    if variable_name(name) {
        Ok(())
    } else {
        refuse(path, Kind::VariableName(name.to_owned()))
    }
}

#[derive(Default)]
struct State {
    out: Reading,
    seen: BTreeSet<(String, String, String, Option<String>)>,
    handlers: BTreeMap<String, Vec<String>>,
    in_play: bool,
    pending: Vec<(String, String)>,
}

impl State {
    fn vertex(&mut self, id: &str, kind: &str) {
        self.out.vertices.insert(id.to_owned(), kind.to_owned());
    }

    fn edge(&mut self, src: &str, kind: &str, tgt: &str, name: Option<&str>) {
        let key = (
            src.to_owned(),
            tgt.to_owned(),
            kind.to_owned(),
            name.map(str::to_owned),
        );
        if self.seen.insert(key) {
            self.out.edges.entry(src.to_owned()).or_default().push((
                kind.to_owned(),
                tgt.to_owned(),
                name.map(str::to_owned),
            ));
        }
    }

    fn keep(&mut self, vertex: &str, sort: &str, value: String) {
        self.out
            .constraints
            .entry(vertex.to_owned())
            .or_default()
            .push((sort.to_owned(), value));
    }

    fn external(&mut self, path: &str, kind: &'static str, name: Option<&str>) {
        self.out
            .external
            .push((path.to_owned(), kind, name.map(str::to_owned)));
    }

    /// Variables of `owner` from `doc` (a mapping, or null), joined by
    /// `edge` edges; ids are the variable's path with a leading `/`.
    fn variables(&mut self, owner: &str, doc: &Doc, path: &str, edge: &str) -> R<()> {
        if *doc == Doc::Null {
            return Ok(());
        }
        let e = map(doc, path)?;
        for (name, _) in e {
            variable(name, &at(path, name))?;
        }
        for (name, value) in e {
            let id = format!("/{}", at(path, name));
            self.vertex(&id, "variable");
            self.edge(owner, edge, &id, Some(name));
            self.keep(&id, "value", json(value));
        }
        Ok(())
    }

    fn vars_and_notify(&mut self, id: &str, e: &[(String, Doc)], path: &str) -> R<()> {
        if let Some(v) = field(e, "vars") {
            self.variables(id, v, &at(path, "vars"), "var")?;
        }
        if let Some(n) = field(e, "notify") {
            for (p, name) in names(n, &at(path, "notify"))? {
                let targets = if self.in_play {
                    self.handlers.get(name).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                };
                if targets.is_empty() {
                    self.external(&p, "handler", Some(name));
                }
                for t in targets {
                    self.pending.push((id.to_owned(), t));
                }
            }
        }
        Ok(())
    }
}

/// Read a file of class `class` as the page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(doc: &Doc, class: Class) -> Result<Reading, Refusal> {
    let mut st = State::default();
    match class {
        Class::Playbook => {
            let items = seq(doc, "")?;
            if items.is_empty() {
                return refuse("", Kind::Invalid);
            }
            st.vertex("/", "playbook");
            for (i, item) in items.iter().enumerate() {
                entry(&mut st, item, &format!("[{i}]"))?;
            }
        }
        Class::Tasks => {
            let items = seq(doc, "")?;
            st.vertex("/", "task-file");
            tasks(&mut st, "/", "tasks", items, "", false)?;
        }
        Class::Vars => {
            st.vertex("/", "vars-file");
            st.variables("/", doc, "", "var")?;
        }
    }
    Ok(st.out)
}

fn is_import(key: &str) -> bool {
    key == "import_playbook" || key == "ansible.builtin.import_playbook"
}

fn entry(st: &mut State, item: &Doc, path: &str) -> R<()> {
    let e = map(item, path)?;
    if e.iter().any(|(k, _)| is_import(k)) {
        import(st, e, path)
    } else {
        play(st, e, path)
    }
}

fn import(st: &mut State, e: &[(String, Doc)], path: &str) -> R<()> {
    let mut found = false;
    for (k, _) in e {
        if !(is_import(k) || ["name", "vars", "when", "tags"].contains(&k.as_str())) {
            return refuse(&at(path, k), Kind::UnknownField(k.clone()));
        }
        if is_import(k) {
            if found {
                return refuse(&at(path, k), Kind::Invalid);
            }
            found = true;
        }
    }
    let id = format!("/{path}");
    st.vertex(&id, "playbook-import");
    st.edge("/", "entry", &id, None);
    if let Some(n) = field(e, "name") {
        text(n, &at(path, "name"))?;
    }
    for (k, v) in e {
        if k == "vars" {
            continue;
        }
        if is_import(k) {
            let name = text(v, &at(path, k))?;
            st.external(&at(path, k), "playbook", Some(name));
            st.keep(&id, "import_playbook", json(v));
        } else {
            st.keep(&id, k, json(v));
        }
    }
    if let Some(v) = field(e, "vars") {
        st.variables(&id, v, &at(path, "vars"), "var")?;
    }
    Ok(())
}

/// The handlers of a play's section, by name and `listen` topic.
fn handler_names(items: &[Doc], path: &str, out: &mut BTreeMap<String, Vec<String>>) {
    for (i, item) in items.iter().enumerate() {
        let p = format!("{path}[{i}]");
        let Doc::Map(e) = item else {
            continue;
        };
        if field(e, "block").is_some() {
            for key in ["block", "rescue", "always"] {
                if let Some(Doc::Seq(children)) = field(e, key) {
                    handler_names(children, &at(&p, key), out);
                }
            }
            continue;
        }
        let id = format!("/{p}");
        let mut found = Vec::new();
        if let Some(Doc::Str(n)) = field(e, "name") {
            found.push(n.clone());
        }
        match field(e, "listen") {
            Some(Doc::Str(t)) => found.push(t.clone()),
            Some(Doc::Seq(ts)) => {
                for t in ts {
                    if let Doc::Str(t) = t {
                        found.push(t.clone());
                    }
                }
            }
            _ => {}
        }
        for n in found {
            let ids = out.entry(n).or_default();
            if !ids.contains(&id) {
                ids.push(id.clone());
            }
        }
    }
}

fn play(st: &mut State, e: &[(String, Doc)], path: &str) -> R<()> {
    if let Some((k, _)) = e
        .iter()
        .find(|(k, _)| !BASE.contains(&k.as_str()) && !PLAY.contains(&k.as_str()))
    {
        return refuse(&at(path, k), Kind::UnknownField(k.clone()));
    }
    if field(e, "hosts").is_none() {
        return refuse(path, Kind::Missing("hosts"));
    }
    let id = format!("/{path}");
    st.vertex(&id, "play");
    st.edge("/", "entry", &id, None);
    let structural = [
        "vars",
        "roles",
        "pre_tasks",
        "tasks",
        "post_tasks",
        "handlers",
    ];
    for (k, v) in e {
        if structural.contains(&k.as_str()) {
            continue;
        }
        let p = at(path, k);
        if k == "name" {
            text(v, &p)?;
        }
        if k == "vars_files" {
            for (j, f) in seq(v, &p)?.iter().enumerate() {
                for (fp, name) in names(f, &format!("{p}[{j}]"))? {
                    st.external(&fp, "vars-file", Some(name));
                }
            }
        }
        st.keep(&id, k, json(v));
    }
    if let Some(v) = field(e, "vars") {
        st.variables(&id, v, &at(path, "vars"), "var")?;
    }
    if let Some(r) = field(e, "roles") {
        let p = at(path, "roles");
        for (j, item) in seq(r, &p)?.iter().enumerate() {
            role(st, &id, item, &format!("{p}[{j}]"))?;
        }
    }
    st.handlers.clear();
    if let Some(Doc::Seq(hs)) = field(e, "handlers") {
        let mut found = BTreeMap::new();
        handler_names(hs, &at(path, "handlers"), &mut found);
        st.handlers = found;
    }
    st.in_play = true;
    for section in SECTIONS {
        if let Some(v) = field(e, section) {
            let p = at(path, section);
            let items = seq(v, &p)?;
            tasks(st, &id, section, items, &p, section == "handlers")?;
        }
    }
    st.in_play = false;
    st.handlers.clear();
    for (src, tgt) in std::mem::take(&mut st.pending) {
        st.edge(&src, "notify", &tgt, None);
    }
    Ok(())
}

fn role(st: &mut State, play: &str, item: &Doc, path: &str) -> R<()> {
    let id = format!("/{path}");
    let (name_doc, name_path, e): (&Doc, String, &[(String, Doc)]) = match item {
        Doc::Map(e) => {
            let key = if field(e, "role").is_some() {
                "role"
            } else {
                "name"
            };
            match field(e, key) {
                Some(n) => (n, at(path, key), e),
                None => return refuse(path, Kind::Missing("role")),
            }
        }
        Doc::Seq(_) => return refuse(path, Kind::WrongType("a string or a mapping")),
        scalar => (scalar, path.to_owned(), &[]),
    };
    let name = text(name_doc, &name_path)?.to_owned();
    let keyword = |k: &str| BASE.contains(&k) || ROLE.contains(&k);
    let mut params = Vec::new();
    for (k, v) in e {
        if at(path, k) == name_path || k == "vars" || keyword(k) {
            continue;
        }
        variable(k, &at(path, k))?;
        params.push((k.clone(), v.clone()));
    }
    st.vertex(&id, "role");
    st.edge(play, "roles", &id, None);
    st.keep(&id, "role", json(name_doc));
    for (k, v) in e {
        if at(path, k) == name_path || k == "vars" || !keyword(k) {
            continue;
        }
        check(k, v, &at(path, k))?;
        st.keep(&id, k, json(v));
    }
    if let Some(v) = field(e, "vars") {
        st.variables(&id, v, &at(path, "vars"), "var")?;
    }
    if !params.is_empty() {
        st.variables(&id, &Doc::Map(params), path, "param")?;
    }
    st.external(&name_path, "role", Some(&name));
    Ok(())
}

fn tasks(
    st: &mut State,
    parent: &str,
    edge: &str,
    items: &[Doc],
    path: &str,
    handlers: bool,
) -> R<()> {
    for (i, item) in items.iter().enumerate() {
        let p = format!("{path}[{i}]");
        let e = map(item, &p)?;
        if field(e, "block").is_some() {
            block(st, e, &p, handlers)?;
        } else {
            task(st, e, &p, handlers)?;
        }
        st.edge(parent, edge, &format!("/{p}"), None);
    }
    Ok(())
}

/// Check one keyword's shape.
fn check(k: &str, v: &Doc, p: &str) -> R<()> {
    match k {
        "name" => {
            text(v, p)?;
        }
        "notify" | "listen" => {
            names(v, p)?;
        }
        "loop_control" => {
            map(v, p)?;
        }
        "register" => {
            if let Doc::Map(e) = v {
                for (n, _) in e {
                    variable(n, &at(p, n))?;
                }
            } else {
                variable(text(v, p)?, p)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn block(st: &mut State, e: &[(String, Doc)], path: &str, handlers: bool) -> R<()> {
    let nested = ["block", "rescue", "always"];
    for (k, _) in e {
        let k = k.as_str();
        if !(BASE.contains(&k) || SHARED.contains(&k) || nested.contains(&k)) {
            return refuse(&at(path, k), Kind::UnknownField(k.to_owned()));
        }
    }
    let id = format!("/{path}");
    st.vertex(&id, "block");
    for (k, v) in e {
        if k == "vars" || nested.contains(&k.as_str()) {
            continue;
        }
        check(k, v, &at(path, k))?;
        st.keep(&id, k, json(v));
    }
    st.vars_and_notify(&id, e, path)?;
    for key in nested {
        if let Some(v) = field(e, key) {
            let p = at(path, key);
            let items = seq(v, &p)?;
            tasks(st, &id, key, items, &p, handlers)?;
        }
    }
    Ok(())
}

fn task(st: &mut State, e: &[(String, Doc)], path: &str, handlers: bool) -> R<()> {
    let mut module: Option<&str> = None;
    let mut loops = 0;
    for (k, _) in e {
        let k = k.as_str();
        let p = at(path, k);
        if k == "action" || k == "local_action" {
            return refuse(
                &p,
                Kind::Unsupported("the action and local_action keywords"),
            );
        }
        let lookup = k.starts_with("with_");
        if k == "loop" || lookup {
            loops += 1;
            if loops > 1 {
                return refuse(&p, Kind::Invalid);
            }
        }
        let keyword = BASE.contains(&k)
            || SHARED.contains(&k)
            || TASK.contains(&k)
            || (handlers && k == "listen");
        if keyword || lookup {
            continue;
        }
        if module.is_some() {
            return refuse(&p, Kind::Invalid);
        }
        module = Some(k);
    }
    let Some(module) = module else {
        return refuse(path, Kind::Missing("a module"));
    };
    let id = format!("/{path}");
    st.vertex(&id, if handlers { "handler" } else { "task" });
    for (k, v) in e {
        let p = at(path, k);
        if k == module {
            st.keep(&id, "module", json_string(module));
            st.keep(&id, "arguments", json(v));
            include(st, module, v, &p);
        } else if let Some(lookup) = k.strip_prefix("with_") {
            st.keep(&id, "with", json_string(lookup));
            st.keep(&id, "with_terms", json(v));
        } else if k != "vars" {
            check(k, v, &p)?;
            st.keep(&id, k, json(v));
        }
    }
    st.vars_and_notify(&id, e, path)
}

fn include(st: &mut State, module: &str, v: &Doc, p: &str) {
    let short = ["ansible.builtin.", "ansible.legacy."]
        .iter()
        .find_map(|prefix| module.strip_prefix(prefix))
        .unwrap_or(module);
    let (kind, key) = match short {
        "include_tasks" | "import_tasks" => ("tasks", "file"),
        "include_role" | "import_role" => ("role", "name"),
        "include_vars" => ("vars-file", "file"),
        _ => return,
    };
    let name = match v {
        Doc::Map(e) => match field(e, key) {
            Some(Doc::Str(t)) => Some(t.as_str()),
            _ => None,
        },
        Doc::Str(t) => Some(t.as_str()),
        _ => None,
    };
    st.external(p, kind, name);
}

/// The canonical file of a file the page reads under `class`.
///
/// It is compact JSON followed by a line feed: a playbook or task file a
/// sequence, a variables file a mapping (`{}` for a null file); an entry,
/// block or task its kept keywords in order, then `vars` when it declares
/// any; a play then `roles`, `pre_tasks`, `tasks`, `post_tasks` and
/// `handlers` that hold anything; a block then `block`, `rescue` and
/// `always` that hold anything; a role `role` with its name, its keywords,
/// its parameters and its `vars`; an import its playbook under
/// `import_playbook`.
#[must_use]
pub fn canonical_text(doc: &Doc, class: Class) -> String {
    let mut out = match (class, doc) {
        (Class::Vars, Doc::Map(e)) => object(&vars_fields(e)),
        (Class::Vars, _) => "{}".to_owned(),
        (Class::Playbook, Doc::Seq(items)) => list(items, canonical_entry),
        (Class::Tasks, Doc::Seq(items)) => list(items, canonical_task),
        _ => String::new(),
    };
    out.push('\n');
    out
}

fn object(fields: &[(String, String)]) -> String {
    format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(k, v)| format!("{}:{v}", json_string(k)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn list(items: &[Doc], each: fn(&Doc) -> String) -> String {
    format!("[{}]", items.iter().map(each).collect::<Vec<_>>().join(","))
}

fn vars_fields(e: &[(String, Doc)]) -> Vec<(String, String)> {
    e.iter().map(|(k, v)| (k.clone(), json(v))).collect()
}

/// `vars` as a field when it declares a variable.
fn vars_field(e: &[(String, Doc)], out: &mut Vec<(String, String)>) {
    if let Some(Doc::Map(v)) = field(e, "vars")
        && !v.is_empty()
    {
        out.push(("vars".into(), object(&vars_fields(v))));
    }
}

fn canonical_entry(doc: &Doc) -> String {
    let Doc::Map(e) = doc else {
        return String::new();
    };
    let mut out = Vec::new();
    if e.iter().any(|(k, _)| is_import(k)) {
        for (k, v) in e {
            if is_import(k) {
                out.push(("import_playbook".into(), json(v)));
            } else if k != "vars" {
                out.push((k.clone(), json(v)));
            }
        }
        vars_field(e, &mut out);
        return object(&out);
    }
    let structural = [
        "vars",
        "roles",
        "pre_tasks",
        "tasks",
        "post_tasks",
        "handlers",
    ];
    for (k, v) in e {
        if !structural.contains(&k.as_str()) {
            out.push((k.clone(), json(v)));
        }
    }
    vars_field(e, &mut out);
    if let Some(Doc::Seq(roles)) = field(e, "roles")
        && !roles.is_empty()
    {
        out.push(("roles".into(), list(roles, canonical_role)));
    }
    for section in SECTIONS {
        if let Some(Doc::Seq(items)) = field(e, section)
            && !items.is_empty()
        {
            out.push((section.into(), list(items, canonical_task)));
        }
    }
    object(&out)
}

fn canonical_role(doc: &Doc) -> String {
    let Doc::Map(e) = doc else {
        return object(&[("role".into(), json(doc))]);
    };
    let key = if field(e, "role").is_some() {
        "role"
    } else {
        "name"
    };
    let mut out = vec![(
        "role".to_owned(),
        field(e, key).map(json).unwrap_or_default(),
    )];
    let keyword = |k: &str| BASE.contains(&k) || ROLE.contains(&k);
    for (k, v) in e {
        if k != key && k != "vars" && keyword(k) {
            out.push((k.clone(), json(v)));
        }
    }
    for (k, v) in e {
        if k != key && k != "vars" && !keyword(k) {
            out.push((k.clone(), json(v)));
        }
    }
    vars_field(e, &mut out);
    object(&out)
}

fn canonical_task(doc: &Doc) -> String {
    let Doc::Map(e) = doc else {
        return String::new();
    };
    let nested = ["block", "rescue", "always"];
    let is_block = field(e, "block").is_some();
    let mut out = Vec::new();
    for (k, v) in e {
        if k != "vars" && !(is_block && nested.contains(&k.as_str())) {
            out.push((k.clone(), json(v)));
        }
    }
    vars_field(e, &mut out);
    if is_block {
        for key in nested {
            if let Some(Doc::Seq(items)) = field(e, key)
                && !items.is_empty()
            {
                out.push((key.into(), list(items, canonical_task)));
            }
        }
    }
    object(&out)
}
