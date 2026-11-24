//! The GraphQL page's reference: a type system document of the September
//! 2025 edition read as the page states, over its own syntax tree, with its
//! own model of the type system, and its canonical text.
//!
//! A document is a list of [`Def`]s; [`render`] writes one as GraphQL
//! text, [`read`] reads it as the page states into a [`Reading`] or the
//! first [`Refusal`], and [`canonical_text`] is the canonical text of what
//! it reads as.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// A constant value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val {
    /// An integer, as written.
    Int(String),
    /// A float, as written.
    Float(String),
    /// A string.
    Str(String),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
    /// An enum value.
    Enum(String),
    /// A list.
    List(Vec<Self>),
    /// An input object, fields in the order written.
    Obj(Vec<(String, Self)>),
}

/// A type reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    /// A named type.
    N(String),
    /// A list of the inner type.
    L(Box<Self>),
    /// The inner type, non-null.
    NN(Box<Self>),
}

/// A named type.
#[must_use]
pub fn n(name: &str) -> Ty {
    Ty::N(name.to_owned())
}

/// A list type.
#[must_use]
pub fn l(inner: Ty) -> Ty {
    Ty::L(Box::new(inner))
}

/// A non-null type.
#[must_use]
pub fn nn(inner: Ty) -> Ty {
    Ty::NN(Box::new(inner))
}

/// An applied directive: its name and arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dir {
    /// The directive's name, without `@`.
    pub name: String,
    /// The arguments in the order written.
    pub args: Vec<(String, Val)>,
}

/// An argument or input field definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    /// The description.
    pub desc: Option<String>,
    /// The name.
    pub name: String,
    /// The type.
    pub ty: Ty,
    /// The default value.
    pub default: Option<Val>,
    /// The applied directives.
    pub dirs: Vec<Dir>,
}

/// A field definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The description.
    pub desc: Option<String>,
    /// The name.
    pub name: String,
    /// The arguments.
    pub args: Vec<Input>,
    /// The type.
    pub ty: Ty,
    /// The applied directives.
    pub dirs: Vec<Dir>,
}

/// An enum value definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumVal {
    /// The description.
    pub desc: Option<String>,
    /// The name.
    pub name: String,
    /// The applied directives.
    pub dirs: Vec<Dir>,
}

/// The kind of a named type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `scalar`.
    Scalar,
    /// `type`.
    Object,
    /// `interface`.
    Interface,
    /// `union`.
    Union,
    /// `enum`.
    Enum,
    /// `input`.
    Input,
}

impl Kind {
    const fn keyword(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Object => "type",
            Self::Interface => "interface",
            Self::Union => "union",
            Self::Enum => "enum",
            Self::Input => "input",
        }
    }

    const fn vertex(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Object => "object",
            Self::Interface => "interface",
            Self::Union => "union",
            Self::Enum => "enum",
            Self::Input => "input-object",
        }
    }

    const fn location(self) -> &'static str {
        match self {
            Self::Scalar => "SCALAR",
            Self::Object => "OBJECT",
            Self::Interface => "INTERFACE",
            Self::Union => "UNION",
            Self::Enum => "ENUM",
            Self::Input => "INPUT_OBJECT",
        }
    }
}

/// A type definition or extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDef {
    /// The kind.
    pub kind: Kind,
    /// The name.
    pub name: String,
    /// The description (a definition's only).
    pub desc: Option<String>,
    /// The applied directives.
    pub dirs: Vec<Dir>,
    /// The implemented interfaces.
    pub implements: Vec<String>,
    /// The fields of an object or interface type.
    pub fields: Vec<Field>,
    /// The members of a union.
    pub members: Vec<String>,
    /// The values of an enum.
    pub values: Vec<EnumVal>,
    /// The input fields of an input object type.
    pub inputs: Vec<Input>,
}

impl TypeDef {
    /// An empty definition of `kind` named `name`.
    #[must_use]
    pub fn new(kind: Kind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            desc: None,
            dirs: Vec::new(),
            implements: Vec::new(),
            fields: Vec::new(),
            members: Vec::new(),
            values: Vec::new(),
            inputs: Vec::new(),
        }
    }
}

/// A definition of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Def {
    /// `schema`.
    Schema {
        /// The description.
        desc: Option<String>,
        /// The applied directives.
        dirs: Vec<Dir>,
        /// The operation types and their root types.
        roots: Vec<(String, String)>,
    },
    /// `extend schema` with one operation type.
    ExtendSchema {
        /// The applied directives.
        dirs: Vec<Dir>,
        /// The operation type and its root type.
        root: (String, String),
    },
    /// A type definition.
    Type(TypeDef),
    /// A type extension.
    Extend(TypeDef),
    /// A directive definition.
    Directive {
        /// The description.
        desc: Option<String>,
        /// The name, without `@`.
        name: String,
        /// The arguments.
        args: Vec<Input>,
        /// Whether `repeatable`.
        repeatable: bool,
        /// The locations.
        locations: Vec<String>,
    },
}

/// A refusal: the schema coordinate of the definition at fault and the
/// rule (the name of the production `Rule` variant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The schema coordinate.
    pub path: String,
    /// The rule.
    pub rule: &'static str,
}

/// What a document reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Each vertex with its kind.
    pub vertices: BTreeMap<String, String>,
    /// Each vertex's out-edges in order: kind, target, name.
    pub edges: BTreeMap<String, Vec<(String, String, Option<String>)>>,
    /// Each vertex's constraints in order: sort, value.
    pub constraints: BTreeMap<String, Vec<(String, String)>>,
}

type R<T> = Result<T, Refusal>;

fn no<T>(path: impl Into<String>, rule: &'static str) -> R<T> {
    Err(Refusal {
        path: path.into(),
        rule,
    })
}

const SCALARS: [&str; 5] = ["Int", "Float", "String", "Boolean", "ID"];
const SCHEMA: &str = "{schema}";

// ---------------------------------------------------------------------------
// Text.
// ---------------------------------------------------------------------------

/// `BlockStringValue(raw)`.
fn block_value(raw: &str) -> String {
    let lines: Vec<String> = raw
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect();
    let lead = |s: &str| s.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let mut indent: Option<usize> = None;
    for line in lines.iter().skip(1) {
        let k = lead(line);
        if k < line.chars().count() && indent.is_none_or(|i| k < i) {
            indent = Some(k);
        }
    }
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| match indent {
            Some(k) if i > 0 => line.chars().skip(k).collect(),
            _ => line.clone(),
        })
        .collect();
    let blank = |s: &String| s.chars().all(|c| c == ' ' || c == '\t');
    while out.first().is_some_and(blank) {
        out.remove(0);
    }
    while out.last().is_some_and(blank) {
        out.pop();
    }
    out.join("\n")
}

/// The text of a string: quoted where it holds no quote, backslash or line
/// break, else the first of three block forms whose value it is.
#[must_use]
pub fn string(value: &str) -> Option<String> {
    if !value.chars().any(|c| matches!(c, '"' | '\\' | '\n' | '\r')) {
        return Some(format!("\"{value}\""));
    }
    for raw in [
        format!("\n{value}\n"),
        value.to_owned(),
        format!("{value}\n"),
    ] {
        if raw.contains("\"\"\"")
            || raw.contains('\r')
            || raw.starts_with('"')
            || raw.ends_with('"')
        {
            continue;
        }
        if block_value(&raw) == value {
            return Some(format!("\"\"\"{raw}\"\"\""));
        }
    }
    None
}

fn val_text(v: &Val) -> String {
    match v {
        Val::Int(t) | Val::Float(t) | Val::Enum(t) => t.clone(),
        Val::Str(s) => string(s).unwrap_or_default(),
        Val::Bool(b) => if *b { "true" } else { "false" }.to_owned(),
        Val::Null => "null".to_owned(),
        Val::List(items) => {
            format!(
                "[{}]",
                items.iter().map(val_text).collect::<Vec<_>>().join(", ")
            )
        }
        Val::Obj(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", val_text(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn ty_text(t: &Ty) -> String {
    match t {
        Ty::N(name) => name.clone(),
        Ty::L(inner) => format!("[{}]", ty_text(inner)),
        Ty::NN(inner) => format!("{}!", ty_text(inner)),
    }
}

fn dir_text(d: &Dir) -> String {
    if d.args.is_empty() {
        format!("@{}", d.name)
    } else {
        let args: Vec<String> = d
            .args
            .iter()
            .map(|(k, v)| format!("{k}: {}", val_text(v)))
            .collect();
        format!("@{}({})", d.name, args.join(", "))
    }
}

fn dirs_text(dirs: &[Dir]) -> String {
    let mut out = String::new();
    for d in dirs {
        out.push(' ');
        out.push_str(&dir_text(d));
    }
    out
}

fn desc_text(desc: Option<&String>) -> String {
    desc.map(|d| format!("{}\n", string(d).unwrap_or_default()))
        .unwrap_or_default()
}

fn input_text(i: &Input) -> String {
    let default = i
        .default
        .as_ref()
        .map(|v| format!(" = {}", val_text(v)))
        .unwrap_or_default();
    format!(
        "{}{}: {}{default}{}",
        desc_text(i.desc.as_ref()),
        i.name,
        ty_text(&i.ty),
        dirs_text(&i.dirs)
    )
}

fn args_text(args: &[Input]) -> String {
    if args.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            args.iter().map(input_text).collect::<Vec<_>>().join(", ")
        )
    }
}

fn type_text(t: &TypeDef, extend: bool) -> String {
    let mut out = if extend {
        "extend ".to_owned()
    } else {
        desc_text(t.desc.as_ref())
    };
    let _ = write!(out, "{} {}", t.kind.keyword(), t.name);
    if !t.implements.is_empty() {
        let _ = write!(out, " implements {}", t.implements.join(" & "));
    }
    out.push_str(&dirs_text(&t.dirs));
    if !t.members.is_empty() {
        let _ = write!(out, " = {}", t.members.join(" | "));
    }
    if !t.fields.is_empty() {
        out.push_str(" {\n");
        for f in &t.fields {
            let _ = writeln!(
                out,
                "{}{}{}: {}{}",
                desc_text(f.desc.as_ref()),
                f.name,
                args_text(&f.args),
                ty_text(&f.ty),
                dirs_text(&f.dirs)
            );
        }
        out.push('}');
    }
    if !t.values.is_empty() {
        out.push_str(" {\n");
        for v in &t.values {
            let _ = writeln!(
                out,
                "{}{}{}",
                desc_text(v.desc.as_ref()),
                v.name,
                dirs_text(&v.dirs)
            );
        }
        out.push('}');
    }
    if !t.inputs.is_empty() {
        out.push_str(" {\n");
        for i in &t.inputs {
            out.push_str(&input_text(i));
            out.push('\n');
        }
        out.push('}');
    }
    out.push('\n');
    out
}

/// The document as GraphQL text.
#[must_use]
pub fn render(doc: &[Def]) -> String {
    let mut out = String::new();
    for def in doc {
        match def {
            Def::Schema { desc, dirs, roots } => {
                out.push_str(&desc_text(desc.as_ref()));
                let _ = writeln!(out, "schema{} {{", dirs_text(dirs));
                for (op, ty) in roots {
                    let _ = writeln!(out, "{op}: {ty}");
                }
                out.push_str("}\n");
            }
            Def::ExtendSchema { dirs, root } => {
                let _ = writeln!(
                    out,
                    "extend schema{} {{ {}: {} }}",
                    dirs_text(dirs),
                    root.0,
                    root.1
                );
            }
            Def::Type(t) => out.push_str(&type_text(t, false)),
            Def::Extend(t) => out.push_str(&type_text(t, true)),
            Def::Directive {
                desc,
                name,
                args,
                repeatable,
                locations,
            } => {
                let _ = writeln!(
                    out,
                    "{}directive @{name}{}{} on {}",
                    desc_text(desc.as_ref()),
                    args_text(args),
                    if *repeatable { " repeatable" } else { "" },
                    locations.join(" | ")
                );
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Reading.
// ---------------------------------------------------------------------------

/// A directive definition in the model.
#[derive(Clone, Debug)]
struct Directive {
    args: Vec<Input>,
    repeatable: bool,
    locations: Vec<String>,
}

/// The specification's built-in directives.
fn built_ins() -> BTreeMap<&'static str, Directive> {
    let arg = |name: &str, ty: Ty, default: Option<Val>| Input {
        desc: None,
        name: name.to_owned(),
        ty,
        default,
        dirs: Vec::new(),
    };
    let locs = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let conditional = locs(&["FIELD", "FRAGMENT_SPREAD", "INLINE_FRAGMENT"]);
    BTreeMap::from([
        (
            "skip",
            Directive {
                args: vec![arg("if", nn(n("Boolean")), None)],
                repeatable: false,
                locations: conditional.clone(),
            },
        ),
        (
            "include",
            Directive {
                args: vec![arg("if", nn(n("Boolean")), None)],
                repeatable: false,
                locations: conditional,
            },
        ),
        (
            "deprecated",
            Directive {
                args: vec![arg(
                    "reason",
                    nn(n("String")),
                    Some(Val::Str("No longer supported".to_owned())),
                )],
                repeatable: false,
                locations: locs(&[
                    "FIELD_DEFINITION",
                    "ARGUMENT_DEFINITION",
                    "INPUT_FIELD_DEFINITION",
                    "ENUM_VALUE",
                ]),
            },
        ),
        (
            "specifiedBy",
            Directive {
                args: vec![arg("url", nn(n("String")), None)],
                repeatable: false,
                locations: locs(&["SCALAR"]),
            },
        ),
        (
            "oneOf",
            Directive {
                args: Vec::new(),
                repeatable: false,
                locations: locs(&["INPUT_OBJECT"]),
            },
        ),
    ])
}

fn reserved(name: &str) -> bool {
    name.len() >= 2 && &name[..2] == "__"
}

fn named(t: &Ty) -> &str {
    match t {
        Ty::N(s) => s,
        Ty::L(i) | Ty::NN(i) => named(i),
    }
}

const fn required(i: &Input) -> bool {
    matches!(i.ty, Ty::NN(_)) && i.default.is_none()
}

fn has(dirs: &[Dir], name: &str) -> bool {
    dirs.iter().any(|d| d.name == name)
}

/// The per-definition checks of the walk: reserved names, enum value names,
/// and extensions that add nothing, in document order.
fn walk(doc: &[Def]) -> R<()> {
    let input = |path: String, i: &Input| -> R<()> {
        if reserved(&i.name) {
            return no(path, "ReservedName");
        }
        Ok(())
    };
    for def in doc {
        match def {
            Def::Type(t) | Def::Extend(t) => {
                if reserved(&t.name) {
                    return no(t.name.clone(), "ReservedName");
                }
                for f in &t.fields {
                    let p = format!("{}.{}", t.name, f.name);
                    if reserved(&f.name) {
                        return no(p, "ReservedName");
                    }
                    for a in &f.args {
                        input(format!("{p}({}:)", a.name), a)?;
                    }
                }
                for v in &t.values {
                    let p = format!("{}.{}", t.name, v.name);
                    if ["true", "false", "null"].contains(&v.name.as_str()) {
                        return no(p, "EnumValueName");
                    }
                    if reserved(&v.name) {
                        return no(p, "ReservedName");
                    }
                }
                for i in &t.inputs {
                    input(format!("{}.{}", t.name, i.name), i)?;
                }
                let empty = t.dirs.is_empty()
                    && t.implements.is_empty()
                    && t.fields.is_empty()
                    && t.members.is_empty()
                    && t.values.is_empty()
                    && t.inputs.is_empty();
                if matches!(def, Def::Extend(_)) && empty {
                    return no(t.name.clone(), "EmptyExtension");
                }
            }
            Def::Directive { name, args, .. } => {
                if reserved(name) {
                    return no(format!("@{name}"), "ReservedName");
                }
                for a in args {
                    input(format!("@{name}({}:)", a.name), a)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The type system of a document.
struct System {
    order: Vec<String>,
    types: BTreeMap<String, TypeDef>,
    directives: BTreeMap<String, Directive>,
    directive_order: Vec<(String, Option<String>)>,
    built: BTreeMap<&'static str, Directive>,
    schema_desc: Option<String>,
    schema_dirs: Vec<Dir>,
    roots: Vec<(String, String)>,
}

fn same_as_built_in(args: &[Input], repeatable: bool, locations: &[String], d: &Directive) -> bool {
    repeatable == d.repeatable
        && locations == d.locations.as_slice()
        && args.len() == d.args.len()
        && args.iter().zip(&d.args).all(|(a, b)| {
            a.name == b.name
                && a.ty == b.ty
                && a.default == b.default
                && a.desc.is_none()
                && a.dirs.is_empty()
        })
}

/// Merge each extension into its type, in the order read.
fn merge_extensions(types: &mut BTreeMap<String, TypeDef>, doc: &[Def]) -> R<()> {
    for def in doc {
        let Def::Extend(e) = def else { continue };
        let Some(t) = types.get_mut(&e.name) else {
            if e.kind == Kind::Scalar && SCALARS.contains(&e.name.as_str()) {
                return no(e.name.clone(), "BuiltInScalarExtension");
            }
            return no(e.name.clone(), "ExtensionUndefined");
        };
        if t.kind != e.kind {
            return no(e.name.clone(), "ExtensionKind");
        }
        if t.kind == Kind::Input && has(&e.dirs, "oneOf") {
            return no(e.name.clone(), "OneOfOnExtension");
        }
        t.dirs.extend(e.dirs.iter().cloned());
        for i in &e.implements {
            if t.implements.contains(i) {
                return no(e.name.clone(), "DuplicateInterface");
            }
            t.implements.push(i.clone());
        }
        for f in &e.fields {
            if t.fields.iter().any(|g| g.name == f.name) {
                return no(format!("{}.{}", e.name, f.name), "DuplicateField");
            }
            t.fields.push(f.clone());
        }
        for m in &e.members {
            if t.members.contains(m) {
                return no(e.name.clone(), "DuplicateMember");
            }
            t.members.push(m.clone());
        }
        for v in &e.values {
            if t.values.iter().any(|w| w.name == v.name) {
                return no(format!("{}.{}", e.name, v.name), "DuplicateValue");
            }
            t.values.push(v.clone());
        }
        for i in &e.inputs {
            if t.inputs.iter().any(|j| j.name == i.name) {
                return no(format!("{}.{}", e.name, i.name), "DuplicateField");
            }
            t.inputs.push(i.clone());
        }
    }
    Ok(())
}

fn assemble(doc: &[Def]) -> R<System> {
    let built = built_ins();
    let mut types: BTreeMap<String, TypeDef> = BTreeMap::new();
    let mut order = Vec::new();
    for def in doc {
        if let Def::Type(t) = def {
            if SCALARS.contains(&t.name.as_str()) {
                return no(t.name.clone(), "BuiltInTypeName");
            }
            if types.contains_key(&t.name) {
                return no(t.name.clone(), "DuplicateType");
            }
            order.push(t.name.clone());
            types.insert(t.name.clone(), t.clone());
        }
    }
    let mut directives = BTreeMap::new();
    let mut directive_order = Vec::new();
    let mut names = BTreeSet::new();
    for def in doc {
        if let Def::Directive {
            desc,
            name,
            args,
            repeatable,
            locations,
        } = def
        {
            if !names.insert(name.clone()) {
                return no(format!("@{name}"), "DuplicateDirective");
            }
            if let Some(b) = built.get(name.as_str()) {
                if desc.is_some() || !same_as_built_in(args, *repeatable, locations, b) {
                    return no(format!("@{name}"), "BuiltInDirectiveRedefined");
                }
                continue;
            }
            directive_order.push((name.clone(), desc.clone()));
            directives.insert(
                name.clone(),
                Directive {
                    args: args.clone(),
                    repeatable: *repeatable,
                    locations: locations.clone(),
                },
            );
        }
    }
    let schemas: Vec<&Def> = doc
        .iter()
        .filter(|d| matches!(d, Def::Schema { .. }))
        .collect();
    if schemas.len() > 1 {
        return no(SCHEMA, "SchemaDefinitions");
    }
    merge_extensions(&mut types, doc)?;
    let (schema_desc, mut schema_dirs, mut roots) = match schemas.first() {
        Some(Def::Schema { desc, dirs, roots }) => (desc.clone(), dirs.clone(), roots.clone()),
        _ => (
            None,
            Vec::new(),
            [
                ("query", "Query"),
                ("mutation", "Mutation"),
                ("subscription", "Subscription"),
            ]
            .iter()
            .filter(|(_, t)| types.contains_key(*t))
            .map(|(o, t)| ((*o).to_owned(), (*t).to_owned()))
            .collect(),
        ),
    };
    for def in doc {
        if let Def::ExtendSchema { dirs, root } = def {
            if schemas.is_empty() {
                return no(SCHEMA, "SchemaUndefined");
            }
            schema_dirs.extend(dirs.iter().cloned());
            roots.push(root.clone());
        }
    }
    Ok(System {
        order,
        types,
        directives,
        directive_order,
        built,
        schema_desc,
        schema_dirs,
        roots,
    })
}

impl System {
    fn kind(&self, name: &str) -> Option<Kind> {
        if SCALARS.contains(&name) {
            Some(Kind::Scalar)
        } else {
            self.types.get(name).map(|t| t.kind)
        }
    }

    fn input_type(&self, t: &Ty) -> bool {
        matches!(
            self.kind(named(t)),
            Some(Kind::Scalar | Kind::Enum | Kind::Input)
        )
    }

    fn output_type(&self, t: &Ty) -> bool {
        matches!(
            self.kind(named(t)),
            Some(Kind::Scalar | Kind::Object | Kind::Interface | Kind::Union | Kind::Enum)
        )
    }

    fn ordered(&self) -> impl Iterator<Item = &TypeDef> {
        self.order.iter().filter_map(|n| self.types.get(n))
    }

    fn directive(&self, name: &str) -> Option<&Directive> {
        self.directives.get(name).or_else(|| self.built.get(name))
    }

    fn references(&self) -> R<()> {
        for (_, t) in &self.roots {
            if self.kind(t).is_none() {
                return no(SCHEMA, "UnknownType");
            }
        }
        for t in self.ordered() {
            if t.implements
                .iter()
                .chain(&t.members)
                .any(|i| self.kind(i).is_none())
            {
                return no(t.name.clone(), "UnknownType");
            }
            for f in &t.fields {
                let p = format!("{}.{}", t.name, f.name);
                if self.kind(named(&f.ty)).is_none() {
                    return no(p, "UnknownType");
                }
                for a in &f.args {
                    if self.kind(named(&a.ty)).is_none() {
                        return no(format!("{p}({}:)", a.name), "UnknownType");
                    }
                }
            }
            for i in &t.inputs {
                if self.kind(named(&i.ty)).is_none() {
                    return no(format!("{}.{}", t.name, i.name), "UnknownType");
                }
            }
        }
        for (name, _) in &self.directive_order {
            for a in &self.directives[name].args {
                if self.kind(named(&a.ty)).is_none() {
                    return no(format!("@{name}({}:)", a.name), "UnknownType");
                }
            }
        }
        Ok(())
    }

    fn roots(&self) -> R<()> {
        let mut ops = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for (op, t) in &self.roots {
            if !ops.insert(op.as_str()) {
                return no(SCHEMA, "DuplicateOperation");
            }
            if self.kind(t) != Some(Kind::Object) {
                return no(SCHEMA, "RootNotObject");
            }
            if !targets.insert(t.as_str()) {
                return no(SCHEMA, "RootsNotDistinct");
            }
        }
        if ops.contains("query") {
            Ok(())
        } else {
            no(SCHEMA, "QueryRootMissing")
        }
    }

    fn inputs(&self, owner: &str, list: &[Input], args: bool) -> R<()> {
        let mut seen = BTreeSet::new();
        for i in list {
            let p = if args {
                format!("{owner}({}:)", i.name)
            } else {
                format!("{owner}.{}", i.name)
            };
            if !seen.insert(i.name.as_str()) {
                return no(
                    p,
                    if args {
                        "DuplicateArgument"
                    } else {
                        "DuplicateField"
                    },
                );
            }
            if !self.input_type(&i.ty) {
                return no(p, "NotInputType");
            }
            if required(i) && has(&i.dirs, "deprecated") {
                return no(p, "DeprecatedRequired");
            }
        }
        Ok(())
    }

    fn sub(&self, a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        match (self.types.get(a), self.types.get(b)) {
            (Some(x), Some(y)) => {
                (x.kind == Kind::Object
                    && y.kind == Kind::Union
                    && y.members.iter().any(|m| m == a))
                    || (matches!(x.kind, Kind::Object | Kind::Interface)
                        && y.kind == Kind::Interface
                        && x.implements.iter().any(|i| i == b))
            }
            _ => false,
        }
    }

    fn covariant(&self, f: &Ty, g: &Ty) -> bool {
        match f {
            Ty::NN(inner) => {
                let g = if let Ty::NN(h) = g { h.as_ref() } else { g };
                self.covariant(inner, g)
            }
            Ty::L(inner) => matches!(g, Ty::L(h) if self.covariant(inner, h)),
            Ty::N(a) => matches!(g, Ty::N(b) if self.sub(a, b)),
        }
    }

    fn implementation(&self, ty: &TypeDef, iface: &TypeDef) -> R<()> {
        if iface.implements.iter().any(|j| !ty.implements.contains(j)) {
            return no(ty.name.clone(), "TransitiveInterface");
        }
        for wanted in &iface.fields {
            let Some(field) = ty.fields.iter().find(|f| f.name == wanted.name) else {
                return no(ty.name.clone(), "MissingInterfaceField");
            };
            let p = format!("{}.{}", ty.name, field.name);
            for declared in &wanted.args {
                match field.args.iter().find(|a| a.name == declared.name) {
                    None => return no(p, "MissingInterfaceArgument"),
                    Some(arg) if arg.ty != declared.ty => {
                        return no(format!("{p}({}:)", arg.name), "ArgumentNotInvariant");
                    }
                    Some(_) => {}
                }
            }
            if let Some(extra) = field
                .args
                .iter()
                .find(|a| required(a) && !wanted.args.iter().any(|b| b.name == a.name))
            {
                return no(format!("{p}({}:)", extra.name), "ExtraArgumentRequired");
            }
            if !self.covariant(&field.ty, &wanted.ty) {
                return no(p, "FieldNotCovariant");
            }
            if has(&field.dirs, "deprecated") && !has(&wanted.dirs, "deprecated") {
                return no(p, "DeprecationNotInherited");
            }
        }
        Ok(())
    }

    /// Whether `start` reaches itself through non-null singular input
    /// fields, and the field closing the first cycle found.
    fn cycle(&self, start: &str) -> Option<String> {
        let mut visited = BTreeSet::from([start.to_owned()]);
        // Depth-first in field order, as a stack of (type, next field).
        let mut stack: Vec<(String, usize)> = vec![(start.to_owned(), 0)];
        while let Some((name, k)) = stack.pop() {
            let t = &self.types[&name];
            let Some(field) = t.inputs.get(k) else {
                continue;
            };
            stack.push((name.clone(), k + 1));
            if let Ty::NN(inner) = &field.ty
                && let Ty::N(target) = inner.as_ref()
            {
                if target == start {
                    return Some(format!("{name}.{}", field.name));
                }
                if self.kind(target) == Some(Kind::Input) && visited.insert(target.clone()) {
                    stack.push((target.clone(), 0));
                }
            }
        }
        None
    }

    fn types(&self) -> R<()> {
        for t in self.ordered() {
            let name = t.name.clone();
            match t.kind {
                Kind::Object | Kind::Interface => {
                    if t.fields.is_empty() {
                        return no(name, "NoFields");
                    }
                    let mut seen = BTreeSet::new();
                    for f in &t.fields {
                        let p = format!("{name}.{}", f.name);
                        if !seen.insert(f.name.as_str()) {
                            return no(p, "DuplicateField");
                        }
                        if !self.output_type(&f.ty) {
                            return no(p, "NotOutputType");
                        }
                        self.inputs(&p, &f.args, true)?;
                    }
                    let mut declared = BTreeSet::new();
                    for i in &t.implements {
                        if !declared.insert(i.as_str()) {
                            return no(name, "DuplicateInterface");
                        }
                        if t.kind == Kind::Interface && *i == name {
                            return no(name, "ImplementsSelf");
                        }
                        match self.types.get(i) {
                            Some(it) if it.kind == Kind::Interface => self.implementation(t, it)?,
                            _ => return no(name, "ImplementsNonInterface"),
                        }
                    }
                }
                Kind::Union => {
                    if t.members.is_empty() {
                        return no(name, "NoMembers");
                    }
                    let mut seen = BTreeSet::new();
                    for m in &t.members {
                        if self.kind(m) != Some(Kind::Object) {
                            return no(name, "MemberNotObject");
                        }
                        if !seen.insert(m.as_str()) {
                            return no(name, "DuplicateMember");
                        }
                    }
                }
                Kind::Enum => {
                    if t.values.is_empty() {
                        return no(name, "NoValues");
                    }
                    let mut seen = BTreeSet::new();
                    for v in &t.values {
                        if !seen.insert(v.name.as_str()) {
                            return no(format!("{name}.{}", v.name), "DuplicateValue");
                        }
                    }
                }
                Kind::Input => {
                    if t.inputs.is_empty() {
                        return no(name, "NoFields");
                    }
                    let one_of = has(&t.dirs, "oneOf");
                    let mut seen = BTreeSet::new();
                    for i in &t.inputs {
                        let p = format!("{name}.{}", i.name);
                        if !seen.insert(i.name.as_str()) {
                            return no(p, "DuplicateField");
                        }
                        self.inputs(&name, std::slice::from_ref(i), false)?;
                        if one_of && matches!(i.ty, Ty::NN(_)) {
                            return no(p, "OneOfNonNull");
                        }
                        if one_of && i.default.is_some() {
                            return no(p, "OneOfDefault");
                        }
                    }
                    if let Some(p) = self.cycle(&name) {
                        return no(p, "InputCycle");
                    }
                }
                Kind::Scalar => {}
            }
        }
        Ok(())
    }

    /// The directives a directive's definition reaches.
    fn reaches(&self, start: &str) -> bool {
        let mut seen_dirs: BTreeSet<String> = BTreeSet::new();
        let mut seen_types: BTreeSet<String> = BTreeSet::new();
        let mut work_dirs = vec![start.to_owned()];
        let mut work_types: Vec<String> = Vec::new();
        let mut first = true;
        while !work_dirs.is_empty() || !work_types.is_empty() {
            let mut applied: Vec<&Dir> = Vec::new();
            let mut tys: Vec<&str> = Vec::new();
            if let Some(d) = work_dirs.pop() {
                if !first && d == start {
                    return true;
                }
                first = false;
                if let Some(def) = self.directives.get(&d) {
                    for a in &def.args {
                        applied.extend(&a.dirs);
                        tys.push(named(&a.ty));
                    }
                }
            } else if let Some(t) = work_types.pop()
                && let Some(def) = self.types.get(&t)
            {
                applied.extend(&def.dirs);
                for v in &def.values {
                    applied.extend(&v.dirs);
                }
                for i in &def.inputs {
                    applied.extend(&i.dirs);
                    tys.push(named(&i.ty));
                }
            }
            for d in applied {
                if d.name == start {
                    return true;
                }
                if seen_dirs.insert(d.name.clone()) {
                    work_dirs.push(d.name.clone());
                }
            }
            for t in tys {
                if seen_types.insert(t.to_owned()) {
                    work_types.push(t.to_owned());
                }
            }
        }
        false
    }

    fn directive_definitions(&self) -> R<()> {
        for (name, _) in &self.directive_order {
            let p = format!("@{name}");
            self.inputs(&p, &self.directives[name].args, true)?;
            if self.reaches(name) {
                return no(p, "DirectiveSelfReference");
            }
        }
        Ok(())
    }

    fn coerces(&self, v: &Val, t: &Ty, depth: usize) -> bool {
        if depth > 256 {
            return false;
        }
        match (t, v) {
            (Ty::NN(_), Val::Null) => false,
            (Ty::NN(inner), _) => self.coerces(v, inner, depth + 1),
            (_, Val::Null) => true,
            (Ty::L(inner), Val::List(items)) => {
                items.iter().all(|i| self.coerces(i, inner, depth + 1))
            }
            (Ty::L(inner), _) => self.coerces(v, inner, depth + 1),
            (Ty::N(name), _) => match name.as_str() {
                "Int" => {
                    matches!(v, Val::Int(s) if s.parse::<i64>().is_ok_and(|x| i32::try_from(x).is_ok()))
                }
                "Float" => {
                    matches!(v, Val::Int(s) | Val::Float(s) if s.parse::<f64>().is_ok_and(f64::is_finite))
                }
                "String" => matches!(v, Val::Str(_)),
                "Boolean" => matches!(v, Val::Bool(_)),
                "ID" => matches!(v, Val::Str(_) | Val::Int(_)),
                _ => match self.types.get(name) {
                    Some(def) if def.kind == Kind::Enum => {
                        matches!(v, Val::Enum(e) if def.values.iter().any(|x| &x.name == e))
                    }
                    Some(def) if def.kind == Kind::Input => {
                        let Val::Obj(fields) = v else { return false };
                        let keys: BTreeSet<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
                        if keys.len() != fields.len() {
                            return false;
                        }
                        let all = fields.iter().all(|(k, x)| {
                            def.inputs
                                .iter()
                                .find(|i| &i.name == k)
                                .is_some_and(|i| self.coerces(x, &i.ty, depth + 1))
                        });
                        let present =
                            def.inputs.iter().filter(|i| required(i)).all(|i| {
                                fields.iter().any(|(k, x)| *k == i.name && *x != Val::Null)
                            });
                        let one = !has(&def.dirs, "oneOf")
                            || (fields.len() == 1 && fields[0].1 != Val::Null);
                        all && present && one
                    }
                    _ => true,
                },
            },
        }
    }

    fn applied(&self, path: &str, dirs: &[Dir], location: &str) -> R<()> {
        let mut seen = BTreeSet::new();
        for d in dirs {
            let Some(def) = self.directive(&d.name) else {
                return no(path, "DirectiveUndefined");
            };
            if !def.locations.iter().any(|l| l == location) {
                return no(path, "DirectiveLocation");
            }
            if !seen.insert(d.name.as_str()) && !def.repeatable {
                return no(path, "DirectiveRepeated");
            }
            let mut given = BTreeSet::new();
            for (k, v) in &d.args {
                let Some(a) = def.args.iter().find(|a| &a.name == k) else {
                    return no(path, "ArgumentUndefined");
                };
                if !given.insert(k.as_str()) {
                    return no(path, "ArgumentRepeated");
                }
                if !self.coerces(v, &a.ty, 0) {
                    return no(path, "ValueNotCoercible");
                }
            }
            for a in def.args.iter().filter(|a| required(a)) {
                if !d.args.iter().any(|(k, v)| *k == a.name && *v != Val::Null) {
                    return no(path, "ArgumentMissing");
                }
            }
        }
        Ok(())
    }

    fn all_applied(&self) -> R<()> {
        self.applied(SCHEMA, &self.schema_dirs, "SCHEMA")?;
        for t in self.ordered() {
            self.applied(&t.name, &t.dirs, t.kind.location())?;
            for f in &t.fields {
                let p = format!("{}.{}", t.name, f.name);
                self.applied(&p, &f.dirs, "FIELD_DEFINITION")?;
                for a in &f.args {
                    self.applied(&format!("{p}({}:)", a.name), &a.dirs, "ARGUMENT_DEFINITION")?;
                }
            }
            for v in &t.values {
                self.applied(&format!("{}.{}", t.name, v.name), &v.dirs, "ENUM_VALUE")?;
            }
            for i in &t.inputs {
                self.applied(
                    &format!("{}.{}", t.name, i.name),
                    &i.dirs,
                    "INPUT_FIELD_DEFINITION",
                )?;
            }
        }
        for (name, _) in &self.directive_order {
            for a in &self.directives[name].args {
                self.applied(
                    &format!("@{name}({}:)", a.name),
                    &a.dirs,
                    "ARGUMENT_DEFINITION",
                )?;
            }
        }
        Ok(())
    }

    fn default_cycle(&self, t: &TypeDef, v: &Val, visited: &BTreeSet<String>) -> bool {
        match v {
            Val::List(items) => items.iter().any(|i| self.default_cycle(t, i, visited)),
            Val::Obj(fields) => t.inputs.iter().any(|f| {
                let Some(target) = self
                    .types
                    .get(named(&f.ty))
                    .filter(|x| x.kind == Kind::Input)
                else {
                    return false;
                };
                if let Some((_, given)) = fields.iter().find(|(k, _)| *k == f.name) {
                    return self.default_cycle(target, given, visited);
                }
                let Some(default) = &f.default else {
                    return false;
                };
                let key = format!("{}.{}", t.name, f.name);
                if visited.contains(&key) {
                    return true;
                }
                let mut next = visited.clone();
                next.insert(key);
                self.default_cycle(target, default, &next)
            }),
            _ => false,
        }
    }

    fn defaults(&self) -> R<()> {
        let check = |p: String, i: &Input| match &i.default {
            Some(v) if !self.coerces(v, &i.ty, 0) => no(p, "ValueNotCoercible"),
            _ => Ok(()),
        };
        for t in self.ordered() {
            for f in &t.fields {
                for a in &f.args {
                    check(format!("{}.{}({}:)", t.name, f.name, a.name), a)?;
                }
            }
            for i in &t.inputs {
                check(format!("{}.{}", t.name, i.name), i)?;
            }
        }
        for (name, _) in &self.directive_order {
            for a in &self.directives[name].args {
                check(format!("@{name}({}:)", a.name), a)?;
            }
        }
        for t in self.ordered().filter(|t| t.kind == Kind::Input) {
            if self.default_cycle(t, &Val::Obj(Vec::new()), &BTreeSet::new()) {
                return no(t.name.clone(), "DefaultValueCycle");
            }
        }
        Ok(())
    }
}

/// The reading of a document's model.
#[derive(Default)]
struct Out {
    reading: Reading,
    /// The built-in scalars the type references name.
    scalars: BTreeSet<String>,
}

impl Out {
    fn vertex(&mut self, id: &str, kind: &str) {
        self.reading.vertices.insert(id.to_owned(), kind.to_owned());
    }

    fn edge(&mut self, src: &str, kind: &str, tgt: &str, name: Option<&str>) {
        self.reading.edges.entry(src.to_owned()).or_default().push((
            kind.to_owned(),
            tgt.to_owned(),
            name.map(str::to_owned),
        ));
    }

    fn put(&mut self, id: &str, sort: &str, value: String) {
        self.reading
            .constraints
            .entry(id.to_owned())
            .or_default()
            .push((sort.to_owned(), value));
    }

    /// An edge from `id` to the named type under the wrappers of `ty`.
    fn type_edge(&mut self, id: &str, ty: &Ty) {
        self.edge(id, "type", named(ty), None);
        if SCALARS.contains(&named(ty)) {
            self.scalars.insert(named(ty).to_owned());
        }
    }

    /// An argument or input field of `owner`, whose vertex and the edge to
    /// it are of kind `kind`.
    fn input(&mut self, owner: &str, kind: &str, id: &str, input: &Input) {
        self.vertex(id, kind);
        self.edge(owner, kind, id, Some(&input.name));
        self.element(
            id,
            input.desc.as_ref(),
            Some(&input.ty),
            input.default.as_ref(),
            &input.dirs,
        );
        self.type_edge(id, &input.ty);
    }

    /// Description, type, default, custom directives, deprecation.
    fn element(
        &mut self,
        id: &str,
        desc: Option<&String>,
        ty: Option<&Ty>,
        default: Option<&Val>,
        dirs: &[Dir],
    ) {
        if let Some(d) = desc {
            self.put(id, "description", d.clone());
        }
        if let Some(t) = ty {
            self.put(id, "type", ty_text(t));
        }
        if let Some(v) = default {
            self.put(id, "default", val_text(v));
        }
        for d in dirs.iter().filter(|d| {
            !["skip", "include", "deprecated", "specifiedBy", "oneOf"].contains(&d.name.as_str())
        }) {
            self.put(id, "directive", dir_text(d));
        }
        if let Some(d) = dirs.iter().find(|d| d.name == "deprecated") {
            let reason = d
                .args
                .iter()
                .find_map(|(k, v)| match v {
                    Val::Str(s) if k == "reason" => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "No longer supported".to_owned());
            self.put(id, "deprecated", reason);
        }
    }
}

/// Read a document as the page states.
///
/// # Errors
///
/// Returns the first [`Refusal`].
pub fn read(doc: &[Def]) -> R<Reading> {
    walk(doc)?;
    let system = assemble(doc)?;
    system.references()?;
    system.roots()?;
    system.types()?;
    system.directive_definitions()?;
    system.all_applied()?;
    system.defaults()?;
    Ok(build(&system))
}

fn build(s: &System) -> Reading {
    let mut out = Out::default();
    out.vertex(SCHEMA, "schema");
    out.element(SCHEMA, s.schema_desc.as_ref(), None, None, &s.schema_dirs);
    for (op, t) in &s.roots {
        out.edge(SCHEMA, "root", t, Some(op));
    }
    for t in s.ordered() {
        build_type(&mut out, t);
    }
    for (name, desc) in &s.directive_order {
        let d = &s.directives[name];
        let id = format!("@{name}");
        out.vertex(&id, "directive");
        out.element(&id, desc.as_ref(), None, None, &[]);
        if d.repeatable {
            out.put(&id, "repeatable", "true".to_owned());
        }
        for loc in &d.locations {
            out.put(&id, "location", loc.clone());
        }
        for a in &d.args {
            out.input(&id, "argument", &format!("{id}({}:)", a.name), a);
        }
    }
    for scalar in std::mem::take(&mut out.scalars) {
        out.vertex(&scalar, "scalar");
        out.put(&scalar, "built-in", "true".to_owned());
    }
    out.reading
}

fn build_type(out: &mut Out, t: &TypeDef) {
    let name = t.name.as_str();
    out.vertex(name, t.kind.vertex());
    out.element(name, t.desc.as_ref(), None, None, &t.dirs);
    let url = t
        .dirs
        .iter()
        .find(|d| d.name == "specifiedBy")
        .and_then(|d| d.args.iter().find(|(k, _)| k == "url"));
    if let Some((_, Val::Str(url))) = url {
        out.put(name, "specified-by", url.clone());
    }
    if has(&t.dirs, "oneOf") {
        out.put(name, "one-of", "true".to_owned());
    }
    for i in &t.implements {
        out.edge(name, "implements", i, Some(i));
    }
    for f in &t.fields {
        let id = format!("{name}.{}", f.name);
        out.vertex(&id, "field");
        out.edge(name, "field", &id, Some(&f.name));
        out.element(&id, f.desc.as_ref(), Some(&f.ty), None, &f.dirs);
        for a in &f.args {
            out.input(&id, "argument", &format!("{id}({}:)", a.name), a);
        }
        out.type_edge(&id, &f.ty);
    }
    for m in &t.members {
        out.edge(name, "member", m, Some(m));
    }
    for v in &t.values {
        let id = format!("{name}.{}", v.name);
        out.vertex(&id, "enum-value");
        out.edge(name, "value", &id, Some(&v.name));
        out.element(&id, v.desc.as_ref(), None, None, &v.dirs);
    }
    for i in &t.inputs {
        out.input(name, "input-field", &format!("{name}.{}", i.name), i);
    }
}

// ---------------------------------------------------------------------------
// Canonical text.
// ---------------------------------------------------------------------------

fn sorts<'r>(r: &'r Reading, id: &str, sort: &str) -> Vec<&'r str> {
    r.constraints
        .get(id)
        .map(|cs| {
            cs.iter()
                .filter(|(s, _)| s == sort)
                .map(|(_, v)| v.as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn out_edges<'r>(r: &'r Reading, id: &str, kind: &str) -> Vec<(&'r str, &'r str)> {
    r.edges
        .get(id)
        .map(|es| {
            es.iter()
                .filter(|(k, _, _)| k == kind)
                .map(|(_, t, n)| (t.as_str(), n.as_deref().unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default()
}

fn applied_of(r: &Reading, id: &str) -> String {
    let mut out = String::new();
    for directive in sorts(r, id, "directive") {
        let _ = write!(out, " {directive}");
    }
    if let Some(reason) = sorts(r, id, "deprecated").first() {
        let _ = write!(
            out,
            " @deprecated(reason: {})",
            string(reason).unwrap_or_default()
        );
    }
    if let Some(url) = sorts(r, id, "specified-by").first() {
        let _ = write!(
            out,
            " @specifiedBy(url: {})",
            string(url).unwrap_or_default()
        );
    }
    if !sorts(r, id, "one-of").is_empty() {
        out.push_str(" @oneOf");
    }
    out
}

fn described(r: &Reading, id: &str, indent: &str) -> String {
    sorts(r, id, "description").first().map_or_else(
        || indent.to_owned(),
        |d| format!("{indent}{}\n{indent}", string(d).unwrap_or_default()),
    )
}

fn input_of(r: &Reading, id: &str, name: &str) -> String {
    let default = sorts(r, id, "default")
        .first()
        .map(|d| format!(" = {d}"))
        .unwrap_or_default();
    let desc = sorts(r, id, "description")
        .first()
        .map(|d| format!("{}\n", string(d).unwrap_or_default()))
        .unwrap_or_default();
    format!(
        "{desc}{name}: {}{default}{}",
        sorts(r, id, "type").first().copied().unwrap_or_default(),
        applied_of(r, id)
    )
}

fn arguments_of(r: &Reading, id: &str) -> String {
    let args = out_edges(r, id, "argument");
    if args.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        args.iter()
            .map(|(t, n)| input_of(r, t, n))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// The canonical text of what a document reads as, or `None` for a
/// document refused.
#[must_use]
pub fn canonical_text(doc: &[Def]) -> Option<String> {
    let r = read(doc).ok()?;
    let top = |id: &str| {
        sorts(&r, id, "description")
            .first()
            .map(|d| format!("{}\n", string(d).unwrap_or_default()))
            .unwrap_or_default()
    };
    let mut out = top(SCHEMA);
    let _ = writeln!(out, "schema{} {{", applied_of(&r, SCHEMA));
    for (t, op) in out_edges(&r, SCHEMA, "root") {
        let _ = writeln!(out, "  {op}: {t}");
    }
    out.push_str("}\n");
    for (id, kind) in &r.vertices {
        if kind != "directive" {
            continue;
        }
        out.push('\n');
        out.push_str(&top(id));
        let _ = write!(out, "directive {id}{}", arguments_of(&r, id));
        if !sorts(&r, id, "repeatable").is_empty() {
            out.push_str(" repeatable");
        }
        let _ = writeln!(out, " on {}", sorts(&r, id, "location").join(" | "));
    }
    let keyword = |k: &str| match k {
        "scalar" => Some("scalar"),
        "object" => Some("type"),
        "interface" => Some("interface"),
        "union" => Some("union"),
        "enum" => Some("enum"),
        "input-object" => Some("input"),
        _ => None,
    };
    for (id, kind) in &r.vertices {
        let Some(word) = keyword(kind) else { continue };
        if !sorts(&r, id, "built-in").is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&top(id));
        let _ = write!(out, "{word} {id}");
        let interfaces: Vec<&str> = out_edges(&r, id, "implements")
            .iter()
            .map(|(t, _)| *t)
            .collect();
        if !interfaces.is_empty() {
            let _ = write!(out, " implements {}", interfaces.join(" & "));
        }
        out.push_str(&applied_of(&r, id));
        match kind.as_str() {
            "object" | "interface" => {
                out.push_str(" {\n");
                for (f, name) in out_edges(&r, id, "field") {
                    out.push_str(&described(&r, f, "  "));
                    let _ = writeln!(
                        out,
                        "{name}{}: {}{}",
                        arguments_of(&r, f),
                        sorts(&r, f, "type").first().copied().unwrap_or_default(),
                        applied_of(&r, f)
                    );
                }
                out.push_str("}\n");
            }
            "union" => {
                let members: Vec<&str> = out_edges(&r, id, "member")
                    .iter()
                    .map(|(t, _)| *t)
                    .collect();
                if !members.is_empty() {
                    let _ = write!(out, " = {}", members.join(" | "));
                }
                out.push('\n');
            }
            "enum" => {
                out.push_str(" {\n");
                for (v, name) in out_edges(&r, id, "value") {
                    out.push_str(&described(&r, v, "  "));
                    let _ = writeln!(out, "{name}{}", applied_of(&r, v));
                }
                out.push_str("}\n");
            }
            "input-object" => {
                out.push_str(" {\n");
                for (f, name) in out_edges(&r, id, "input-field") {
                    let _ = writeln!(out, "  {}", input_of(&r, f, name));
                }
                out.push_str("}\n");
            }
            _ => out.push('\n'),
        }
    }
    Some(out)
}
