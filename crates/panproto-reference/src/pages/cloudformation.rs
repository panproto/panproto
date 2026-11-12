//! Complete reference operations assigned to `cloudformation`.
//!
//! A template is modelled as the data tree it denotes (the [`Doc`] of the
//! `kubernetes-crds` page: mappings with their entries in order, sequences,
//! and the scalars null, booleans, integers and strings), with every
//! intrinsic function in its full form. [`read`] applies the rules of the
//! `cloudformation` page, transcribed from the `2010-09-09` template format,
//! in the order the page states them, and returns either the first refusal
//! or the [`Reading`]: the schema's vertices, edges and kept constraints,
//! and the imports, which stay unresolved. [`canonical_text`] is the
//! canonical template the page defines. It shares no code with production,
//! which reads JSON and YAML text through the tree-sitter grammars and
//! builds a `panproto_schema::Schema`.

use std::collections::{BTreeMap, BTreeSet};

pub use super::kubernetes_crds::{Doc, json, json_string, m, render_yaml, s};

/// What is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `AWSTemplateFormatVersion` is not `2010-09-09`.
    FormatVersion,
    /// A required field is absent.
    Missing(&'static str),
    /// A value of the wrong kind: `a mapping`, `a sequence`, `a string` or
    /// `a scalar or a sequence`.
    WrongType(&'static str),
    /// A rule of the page is broken.
    Invalid,
    /// A field the format does not declare.
    UnknownField(String),
    /// A feature outside the supported set.
    Unsupported(&'static str),
    /// A one-key `Fn::` mapping naming no function.
    UnknownFunction(String),
    /// A function whose arguments do not have its form.
    Arguments(String),
    /// A logical ID declared as a parameter and as a resource.
    DuplicateName(String),
    /// A reference naming nothing it can reach.
    Unresolved(String),
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

/// What a template reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Each vertex id with its kind.
    pub vertices: BTreeMap<String, String>,
    /// Each vertex's out-edges, in the order they are added.
    pub edges: BTreeMap<String, Vec<OutEdge>>,
    /// Each vertex's kept constraints, sort and JSON text, in order.
    pub constraints: BTreeMap<String, Vec<(String, String)>>,
    /// The imports: path and export name, in document order.
    pub imports: Vec<(String, Option<String>)>,
}

/// The pseudo parameters.
pub const PSEUDO: [&str; 8] = [
    "AWS::AccountId",
    "AWS::NotificationARNs",
    "AWS::NoValue",
    "AWS::Partition",
    "AWS::Region",
    "AWS::StackId",
    "AWS::StackName",
    "AWS::URLSuffix",
];

/// The declaring sections, in reading order, with their vertex kinds.
const DECLARING: [(&str, &str); 5] = [
    ("Parameters", "parameter"),
    ("Mappings", "mapping"),
    ("Conditions", "condition"),
    ("Resources", "resource"),
    ("Outputs", "output"),
];

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

fn only(entries: &[(String, Doc)], known: &[&str], path: &str) -> R<()> {
    match entries.iter().find(|(k, _)| !known.contains(&k.as_str())) {
        Some((k, _)) => refuse(&at(path, k), Kind::UnknownField(k.clone())),
        None => Ok(()),
    }
}

/// The variables of an `Fn::Sub` string: each `${name}` in order, a `${!`
/// being a literal and an unclosed `${` ending the scan.
#[must_use]
pub fn sub_variables(t: &str) -> Vec<String> {
    let chars: Vec<char> = t.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '$' && chars[i + 1] == '{' {
            if chars.get(i + 2) == Some(&'!') {
                i += 3;
                continue;
            }
            match chars[i + 2..].iter().position(|c| *c == '}') {
                Some(n) => {
                    out.push(chars[i + 2..i + 2 + n].iter().collect());
                    i += n + 3;
                }
                None => break,
            }
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Default)]
struct State {
    out: Reading,
    declared: BTreeMap<&'static str, BTreeSet<String>>,
    seen: BTreeSet<(String, String, String, Option<String>)>,
}

impl State {
    fn declares(&self, section: &'static str, name: &str) -> bool {
        self.declared.get(section).is_some_and(|n| n.contains(name))
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

    fn keep(&mut self, vertex: &str, sort: &str, value: &Doc) {
        self.out
            .constraints
            .entry(vertex.to_owned())
            .or_default()
            .push((sort.to_owned(), json(value)));
    }

    /// The target of a `Ref` name, or of an `Fn::Sub` variable without a dot.
    fn ref_target(&mut self, name: &str, path: &str, in_condition: bool) -> R<String> {
        if PSEUDO.contains(&name) {
            let id = format!("Pseudo.{name}");
            self.out
                .vertices
                .insert(id.clone(), "pseudo-parameter".into());
            Ok(id)
        } else if self.declares("Parameters", name) {
            Ok(format!("Parameters.{name}"))
        } else if self.declares("Resources", name) && in_condition {
            refuse(path, Kind::Invalid)
        } else if self.declares("Resources", name) {
            Ok(format!("Resources.{name}"))
        } else {
            refuse(path, Kind::Unresolved(name.to_owned()))
        }
    }

    fn get_att(
        &mut self,
        holder: &str,
        name: &str,
        attribute: &str,
        path: &str,
        in_condition: bool,
    ) -> R<()> {
        if in_condition {
            return refuse(path, Kind::Invalid);
        }
        if !self.declares("Resources", name) {
            return refuse(path, Kind::Unresolved(name.to_owned()));
        }
        self.edge(
            holder,
            "get-att",
            &format!("Resources.{name}"),
            Some(attribute),
        );
        Ok(())
    }

    fn condition_named(&mut self, holder: &str, kind: &str, doc: &Doc, path: &str) -> R<()> {
        let name = text(doc, path)?;
        if !self.declares("Conditions", name) {
            return refuse(path, Kind::Unresolved(name.to_owned()));
        }
        self.edge(holder, kind, &format!("Conditions.{name}"), None);
        Ok(())
    }

    /// Resolve the references below `doc`, depth first in order.
    fn walk(&mut self, holder: &str, doc: &Doc, path: &str, in_condition: bool) -> R<()> {
        match doc {
            Doc::Seq(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.walk(holder, item, &format!("{path}[{i}]"), in_condition)?;
                }
                Ok(())
            }
            Doc::Map(entries) if entries.len() == 1 && entries[0].0 == "Ref" => {
                let p = at(path, "Ref");
                let name = text(&entries[0].1, &p)?;
                let target = self.ref_target(name, &p, in_condition)?;
                self.edge(holder, "ref", &target, None);
                Ok(())
            }
            Doc::Map(entries)
                if entries.len() == 1 && entries[0].0 == "Condition" && in_condition =>
            {
                let p = at(path, "Condition");
                self.condition_named(holder, "uses-condition", &entries[0].1, &p)
            }
            Doc::Map(entries) if entries.len() == 1 && entries[0].0.starts_with("Fn::") => {
                let (key, arg) = (&entries[0].0, &entries[0].1);
                self.function(holder, key, arg, &at(path, key), in_condition)
            }
            Doc::Map(entries) => {
                for (k, v) in entries {
                    self.walk(holder, v, &at(path, k), in_condition)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn function(
        &mut self,
        holder: &str,
        key: &str,
        arg: &Doc,
        path: &str,
        in_condition: bool,
    ) -> R<()> {
        let items: &[Doc] = match arg {
            Doc::Seq(items) => items,
            _ => &[],
        };
        let count = |low: usize, high: usize| {
            if matches!(arg, Doc::Seq(_)) && (low..=high).contains(&items.len()) {
                Ok(())
            } else {
                refuse(path, Kind::Arguments(key.to_owned()))
            }
        };
        match key {
            "Fn::Transform" => refuse(path, Kind::Unsupported("a macro")),
            "Fn::Length" | "Fn::ToJsonString" | "Fn::ForEach" => {
                refuse(path, Kind::Unsupported("a language extension function"))
            }
            k if k.starts_with("Fn::ForEach::") => {
                refuse(path, Kind::Unsupported("a language extension function"))
            }
            "Fn::GetAtt" => match (arg, items) {
                (Doc::Str(t), _) if t.contains('.') => {
                    let (name, attribute) = t.split_at(t.find('.').unwrap_or_default());
                    self.get_att(holder, name, &attribute[1..], path, in_condition)
                }
                (Doc::Seq(_), [Doc::Str(name), Doc::Str(attribute)]) => {
                    self.get_att(holder, name, attribute, path, in_condition)
                }
                _ => refuse(path, Kind::Arguments(key.to_owned())),
            },
            "Fn::FindInMap" => {
                count(3, 3)?;
                let first = format!("{path}[0]");
                if let Doc::Str(name) = &items[0] {
                    if !self.declares("Mappings", name) {
                        return refuse(&first, Kind::Unresolved(name.clone()));
                    }
                    self.edge(holder, "find-in-map", &format!("Mappings.{name}"), None);
                } else {
                    self.walk(holder, &items[0], &first, in_condition)?;
                }
                self.walk_from(holder, items, 1, path, in_condition)
            }
            "Fn::If" => {
                count(3, 3)?;
                if !matches!(items[0], Doc::Str(_)) {
                    return refuse(path, Kind::Arguments(key.to_owned()));
                }
                self.condition_named(holder, "uses-condition", &items[0], &format!("{path}[0]"))?;
                self.walk_from(holder, items, 1, path, in_condition)
            }
            "Fn::Sub" => self.sub(holder, arg, path, in_condition),
            "Fn::ImportValue" => {
                let name = match arg {
                    Doc::Str(t) => Some(t.clone()),
                    _ => None,
                };
                self.out.imports.push((path.to_owned(), name));
                self.walk(holder, arg, path, in_condition)
            }
            "Fn::And" | "Fn::Or" => {
                count(2, 10)?;
                self.walk(holder, arg, path, in_condition)
            }
            "Fn::Not" => {
                count(1, 1)?;
                self.walk(holder, arg, path, in_condition)
            }
            "Fn::Equals" | "Fn::Join" | "Fn::Select" | "Fn::Split" => {
                count(2, 2)?;
                self.walk(holder, arg, path, in_condition)
            }
            "Fn::Cidr" => {
                count(3, 3)?;
                self.walk(holder, arg, path, in_condition)
            }
            "Fn::Base64" | "Fn::GetAZs" => self.walk(holder, arg, path, in_condition),
            _ => refuse(path, Kind::UnknownFunction(key.to_owned())),
        }
    }

    fn walk_from(
        &mut self,
        holder: &str,
        items: &[Doc],
        from: usize,
        path: &str,
        in_condition: bool,
    ) -> R<()> {
        for (i, item) in items.iter().enumerate().skip(from) {
            self.walk(holder, item, &format!("{path}[{i}]"), in_condition)?;
        }
        Ok(())
    }

    fn sub(&mut self, holder: &str, arg: &Doc, path: &str, in_condition: bool) -> R<()> {
        let (template, locals, text_path): (&str, &[(String, Doc)], String) = match arg {
            Doc::Str(t) => (t, &[], path.to_owned()),
            Doc::Seq(items) => match items.as_slice() {
                [Doc::Str(t), Doc::Map(vars)] => (t, vars, format!("{path}[0]")),
                _ => return refuse(path, Kind::Arguments("Fn::Sub".into())),
            },
            _ => return refuse(path, Kind::Arguments("Fn::Sub".into())),
        };
        for variable in sub_variables(template) {
            if locals.iter().any(|(k, _)| *k == variable) {
                continue;
            }
            match variable.find('.') {
                Some(dot) if !PSEUDO.contains(&variable.as_str()) => {
                    let (name, attribute) = variable.split_at(dot);
                    self.get_att(holder, name, &attribute[1..], &text_path, in_condition)?;
                }
                _ => {
                    let target = self.ref_target(&variable, &text_path, in_condition)?;
                    self.edge(holder, "ref", &target, None);
                }
            }
        }
        if let Doc::Seq(items) = arg {
            self.walk(holder, &items[1], &format!("{path}[1]"), in_condition)?;
        }
        Ok(())
    }
}

/// Read a template as the page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(doc: &Doc) -> Result<Reading, Refusal> {
    let top = map(doc, "")?;
    only(
        top,
        &[
            "AWSTemplateFormatVersion",
            "Description",
            "Metadata",
            "Parameters",
            "Rules",
            "Mappings",
            "Conditions",
            "Transform",
            "Resources",
            "Outputs",
        ],
        "",
    )?;
    let mut st = State::default();
    st.out.vertices.insert("Template".into(), "template".into());
    header(&mut st, top)?;
    let Some(resources) = field(top, "Resources") else {
        return refuse("", Kind::Missing("Resources"));
    };
    names(&mut st, top)?;
    if map(resources, "Resources")?.is_empty() {
        return refuse("Resources", Kind::Invalid);
    }
    for (section, _) in DECLARING {
        let Some(Doc::Map(entries)) = field(top, section) else {
            continue;
        };
        for (name, body) in entries {
            let id = format!("{section}.{name}");
            match section {
                "Parameters" => parameter(&mut st, &id, body)?,
                "Mappings" => mapping(&mut st, &id, body)?,
                "Conditions" => condition(&mut st, &id, body)?,
                "Resources" => resource(&mut st, &id, body)?,
                _ => output(&mut st, &id, body)?,
            }
        }
    }
    Ok(st.out)
}

fn header(st: &mut State, top: &[(String, Doc)]) -> R<()> {
    if let Some(v) = field(top, "AWSTemplateFormatVersion")
        && text(v, "AWSTemplateFormatVersion")? != "2010-09-09"
    {
        return refuse("AWSTemplateFormatVersion", Kind::FormatVersion);
    }
    if field(top, "Transform").is_some() {
        return refuse("Transform", Kind::Unsupported("a transform"));
    }
    if field(top, "Rules").is_some() {
        return refuse("Rules", Kind::Unsupported("the Rules section"));
    }
    if let Some(d) = field(top, "Description") {
        text(d, "Description")?;
    }
    if let Some(md) = field(top, "Metadata") {
        map(md, "Metadata")?;
    }
    for key in ["AWSTemplateFormatVersion", "Description", "Metadata"] {
        if let Some(v) = field(top, key) {
            st.keep("Template", key, v);
        }
    }
    Ok(())
}

/// Check the declaring sections and their names, and declare every
/// logical ID.
fn names(st: &mut State, top: &[(String, Doc)]) -> R<()> {
    for (section, _) in DECLARING {
        let Some(node) = field(top, section) else {
            continue;
        };
        for (name, _) in map(node, section)? {
            let alnum = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric());
            if section != "Conditions" && !alnum {
                return refuse(&at(section, name), Kind::Invalid);
            }
            st.declared.entry(section).or_default().insert(name.clone());
        }
    }
    let resources = st.declared.get("Resources").cloned().unwrap_or_default();
    if let Some(Doc::Map(entries)) = field(top, "Resources") {
        for (name, _) in entries {
            if st.declares("Parameters", name) && resources.contains(name) {
                return refuse(&at("Resources", name), Kind::DuplicateName(name.clone()));
            }
        }
    }
    for (section, kind) in DECLARING {
        let Some(Doc::Map(entries)) = field(top, section) else {
            continue;
        };
        for (name, _) in entries {
            let id = format!("{section}.{name}");
            st.out.vertices.insert(id.clone(), kind.into());
            st.edge("Template", "declares", &id, Some(name));
        }
    }
    Ok(())
}

fn parameter(st: &mut State, id: &str, body: &Doc) -> R<()> {
    let e = map(body, id)?;
    only(
        e,
        &[
            "AllowedPattern",
            "AllowedValues",
            "ConstraintDescription",
            "Default",
            "Description",
            "MaxLength",
            "MaxValue",
            "MinLength",
            "MinValue",
            "NoEcho",
            "Type",
        ],
        id,
    )?;
    let Some(ty) = field(e, "Type") else {
        return refuse(id, Kind::Missing("Type"));
    };
    let t = text(ty, &at(id, "Type"))?;
    if !parameter_type(t) {
        return refuse(&at(id, "Type"), Kind::Invalid);
    }
    for (k, v) in e {
        match k.as_str() {
            "AllowedValues" => {
                seq(v, &at(id, k))?;
            }
            "Description" | "AllowedPattern" | "ConstraintDescription" => {
                text(v, &at(id, k))?;
            }
            _ => {}
        }
        st.keep(id, k, v);
    }
    Ok(())
}

/// Whether `t` is a parameter type: one of the four general types, an
/// AWS-specific type, or a list of AWS-specific types.
fn parameter_type(t: &str) -> bool {
    if ["String", "Number", "List<Number>", "CommaDelimitedList"].contains(&t) {
        return true;
    }
    if t.starts_with("AWS::") {
        return true;
    }
    t.strip_prefix("List<AWS::")
        .is_some_and(|rest| rest.ends_with('>'))
}

fn mapping(st: &mut State, id: &str, body: &Doc) -> R<()> {
    for (top_key, second) in map(body, id)? {
        let p = at(id, top_key);
        for (k, v) in map(second, &p)? {
            if matches!(v, Doc::Map(_)) {
                return refuse(&at(&p, k), Kind::WrongType("a scalar or a sequence"));
            }
        }
    }
    st.keep(id, "Map", body);
    Ok(())
}

fn condition(st: &mut State, id: &str, body: &Doc) -> R<()> {
    let e = map(body, id)?;
    let functions = [
        "Fn::And",
        "Fn::Equals",
        "Fn::If",
        "Fn::Not",
        "Fn::Or",
        "Condition",
    ];
    if e.len() != 1 || !functions.contains(&e[0].0.as_str()) {
        return refuse(id, Kind::Invalid);
    }
    st.walk(id, body, id, true)?;
    st.keep(id, "Definition", body);
    Ok(())
}

fn resource(st: &mut State, id: &str, body: &Doc) -> R<()> {
    let e = map(body, id)?;
    only(
        e,
        &[
            "Type",
            "Properties",
            "DependsOn",
            "Condition",
            "Metadata",
            "CreationPolicy",
            "DeletionPolicy",
            "UpdatePolicy",
            "UpdateReplacePolicy",
        ],
        id,
    )?;
    let Some(ty) = field(e, "Type") else {
        return refuse(id, Kind::Missing("Type"));
    };
    let tp = at(id, "Type");
    let parts: Vec<&str> = text(ty, &tp)?.split("::").collect();
    let filled = parts.iter().all(|p| !p.is_empty());
    if filled && parts.len() == 4 && parts[3] == "MODULE" {
        return refuse(&tp, Kind::Unsupported("a module"));
    }
    if !(filled && (parts.len() == 3 || (parts.len() == 2 && parts[0] == "Custom"))) {
        return refuse(&tp, Kind::Invalid);
    }
    if let Some(c) = field(e, "Condition") {
        st.condition_named(id, "condition", c, &at(id, "Condition"))?;
    }
    if let Some(d) = field(e, "DependsOn") {
        let p = at(id, "DependsOn");
        let names: Vec<(String, &Doc)> = match d {
            Doc::Seq(items) => items
                .iter()
                .enumerate()
                .map(|(i, x)| (format!("{p}[{i}]"), x))
                .collect(),
            other => vec![(p, other)],
        };
        for (p, x) in names {
            let name = text(x, &p)?;
            if !st.declares("Resources", name) {
                return refuse(&p, Kind::Unresolved(name.to_owned()));
            }
            st.edge(id, "depends-on", &format!("Resources.{name}"), None);
        }
    }
    for (k, v) in e {
        let p = at(id, k);
        match k.as_str() {
            "Condition" | "DependsOn" => continue,
            "Type" => {}
            "DeletionPolicy" => {
                let ok = ["Delete", "Retain", "RetainExceptOnCreate", "Snapshot"];
                if !ok.contains(&text(v, &p)?) {
                    return refuse(&p, Kind::Invalid);
                }
            }
            "UpdateReplacePolicy" => {
                if !["Delete", "Retain", "Snapshot"].contains(&text(v, &p)?) {
                    return refuse(&p, Kind::Invalid);
                }
            }
            _ => {
                map(v, &p)?;
                st.walk(id, v, &p, false)?;
            }
        }
        st.keep(id, k, v);
    }
    Ok(())
}

fn output(st: &mut State, id: &str, body: &Doc) -> R<()> {
    let e = map(body, id)?;
    only(e, &["Description", "Value", "Export", "Condition"], id)?;
    if field(e, "Value").is_none() {
        return refuse(id, Kind::Missing("Value"));
    }
    if let Some(c) = field(e, "Condition") {
        st.condition_named(id, "condition", c, &at(id, "Condition"))?;
    }
    for (k, v) in e {
        let p = at(id, k);
        match k.as_str() {
            "Condition" => continue,
            "Description" => {
                text(v, &p)?;
            }
            "Export" => {
                let x = map(v, &p)?;
                only(x, &["Name"], &p)?;
                if field(x, "Name").is_none() {
                    return refuse(&p, Kind::Missing("Name"));
                }
                st.walk(id, v, &p, false)?;
            }
            _ => st.walk(id, v, &p, false)?,
        }
        st.keep(id, k, v);
    }
    Ok(())
}

/// The canonical template of a template the page reads.
///
/// It is compact JSON: the three header fields that are present in their
/// fixed order, then each
/// declaring section that declares anything, in reading order; a resource
/// writes its fields other than `Condition` and `DependsOn` in order, then
/// `Condition`, then `DependsOn` as a sequence without repeats; an output
/// its fields other than `Condition`, then `Condition`; then a line feed.
#[must_use]
pub fn canonical_text(doc: &Doc) -> String {
    let Doc::Map(top) = doc else {
        return String::new();
    };
    let object = |fields: Vec<(String, String)>| {
        format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{}:{v}", json_string(k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut out = Vec::new();
    for key in ["AWSTemplateFormatVersion", "Description", "Metadata"] {
        if let Some(v) = field(top, key) {
            out.push((key.to_owned(), json(v)));
        }
    }
    for (section, _) in DECLARING {
        let Some(Doc::Map(entries)) = field(top, section) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        let bodies = entries
            .iter()
            .map(|(name, body)| {
                let text = match (section, body) {
                    ("Resources" | "Outputs" | "Parameters", Doc::Map(e)) => {
                        let mut fields: Vec<(String, String)> = e
                            .iter()
                            .filter(|(k, _)| k != "Condition" && k != "DependsOn")
                            .map(|(k, v)| (k.clone(), json(v)))
                            .collect();
                        if let Some(c) = field(e, "Condition") {
                            fields.push(("Condition".into(), json(c)));
                        }
                        if let Some(d) = field(e, "DependsOn") {
                            let mut seen = Vec::new();
                            let items = match d {
                                Doc::Seq(items) => items.clone(),
                                other => vec![other.clone()],
                            };
                            for item in items {
                                if !seen.contains(&item) {
                                    seen.push(item);
                                }
                            }
                            fields.push(("DependsOn".into(), json(&Doc::Seq(seen))));
                        }
                        object(fields)
                    }
                    _ => json(body),
                };
                (name.clone(), text)
            })
            .collect();
        out.push((section.to_owned(), object(bodies)));
    }
    let mut text = object(out);
    text.push('\n');
    text
}
