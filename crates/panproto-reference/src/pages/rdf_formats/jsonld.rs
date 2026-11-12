//! JSON-LD 1.1, transcribed from the algorithms of the JSON-LD 1.1
//! Processing Algorithms and API Recommendation over the subset the
//! `rdf-formats` page states.
//!
//! The document is read by the reference's own JSON reader
//! ([`crate::json::parse`]), whose value is this module's trusted boundary;
//! the module shares no code with production, which reads the document
//! through the tree-sitter JSON grammar. Context processing follows API Sec
//! 4.1 and 4.2, expansion Sec 5.1 to 5.3, and the reading of triples Sec
//! 8.1 to 8.3 over the expanded default graph, with the page's stated
//! refusals in place of the standard's silent drops. A refusal is named by
//! its kind and the JSON Pointer of the value at fault.
//!
//! A JSON number is converted with exact rationals: its decimal value is
//! rounded to binary64 by [`round_to`], and the value it rounds to is
//! written as an integer or in the canonical `xsd:double` form by exact
//! comparison, never by floating-point arithmetic.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::ntriples::{RDF_LANG_STRING, Term, Triple, XSD_STRING, absolute, excluded, term_text};
use super::turtle::{
    RDF_FIRST, RDF_NIL, RDF_REST, RDF_TYPE, XSD_BOOLEAN, XSD_DOUBLE, XSD_INTEGER, resolve,
};
use crate::json::{Value, parse};
use crate::num::{Int, Nat, Rat, floor_log10};
use crate::pages::values::float::{FloatValue, Format, Rounded, decode, round_to};

/// Why the reference refuses a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The text is not one JSON value.
    NotJson,
    /// A JSON-LD refusal: its kind and the JSON Pointer of the value.
    ///
    /// The kind is `error:` and the API's error code, `unsupported:` and
    /// the feature, or one of `duplicate-key`, `free-floating`,
    /// `named-graph`, `blank-predicate`, `reserved-keyword`,
    /// `relative-iri`, `iri-character`, `malformed-iri`, `language-tag` and
    /// `number-range`.
    JsonLd(String, String),
}

type Fault = (String, String);
type R<T> = Result<T, Fault>;

fn err(code: &str, pointer: &str) -> Fault {
    (format!("error:{code}"), pointer.to_owned())
}

fn unsupported(feature: &str, pointer: &str) -> Fault {
    (format!("unsupported:{feature}"), pointer.to_owned())
}

fn kind(name: &str, pointer: &str) -> Fault {
    (name.to_owned(), pointer.to_owned())
}

/// `pointer` followed by the reference token `segment`, escaped as RFC 6901
/// Sec 3 requires.
fn below(pointer: &str, segment: &str) -> String {
    format!(
        "{pointer}/{}",
        segment.replace('~', "~0").replace('/', "~1")
    )
}

const KEYWORDS: [&str; 29] = [
    "@base",
    "@container",
    "@context",
    "@default",
    "@direction",
    "@embed",
    "@explicit",
    "@graph",
    "@id",
    "@import",
    "@included",
    "@index",
    "@json",
    "@language",
    "@list",
    "@nest",
    "@none",
    "@omitDefault",
    "@prefix",
    "@preserve",
    "@propagate",
    "@protected",
    "@requireAll",
    "@reverse",
    "@set",
    "@type",
    "@value",
    "@version",
    "@vocab",
];

fn keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// `"@"1*ALPHA`.
fn keyword_form(s: &str) -> bool {
    s.len() > 1 && s.starts_with('@') && s[1..].chars().all(|c| c.is_ascii_alphabetic())
}

/// An IRI: absolute, and holding no character `IRIREF` excludes.
fn iri(s: &str) -> bool {
    absolute(s) && !s.chars().any(excluded)
}

/// `BLANK_NODE_LABEL` of N-Triples without its `_:`.
fn blank_label(label: &str) -> bool {
    fn base(c: char) -> bool {
        super::ntriples::pn_chars_base(c)
    }
    fn u(c: char) -> bool {
        base(c) || c == '_' || c == ':'
    }
    fn chars(c: char) -> bool {
        u(c) || c == '-'
            || c.is_ascii_digit()
            || c == '\u{B7}'
            || ('\u{300}'..='\u{36F}').contains(&c)
            || ('\u{203F}'..='\u{2040}').contains(&c)
    }
    let all: Vec<char> = label.chars().collect();
    match all.split_first() {
        None => false,
        Some((first, rest)) => {
            (u(*first) || first.is_ascii_digit())
                && rest.iter().all(|&c| chars(c) || c == '.')
                && all.last() != Some(&'.')
        }
    }
}

/// `LANGTAG` of N-Triples without its `@`.
fn language_tag(tag: &str) -> bool {
    let parts: Vec<&str> = tag.split('-').collect();
    !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphabetic())
        && parts[1..]
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

// ── Numbers ─────────────────────────────────────────────────────────

/// The exact value of a JSON numeral, with whether it is written negative.
fn numeral(text: &str) -> Option<(Rat, bool)> {
    let negative = text.starts_with('-');
    let body = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], body[i + 1..].parse::<i64>().ok()?),
        None => (body, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = Nat::parse(&format!("{whole}{fraction}"))?;
    let scale = exponent - i64::try_from(fraction.len()).ok()?;
    let magnitude = Rat::from_int(Int::new(false, digits)).mul(&Rat::pow10(scale));
    Some((if negative { magnitude.neg() } else { magnitude }, negative))
}

/// The canonical `xsd:double` form of a finite non-zero `value`: its
/// magnitude rounded to 16 significant digits, half away from zero.
fn double_form(value: &Rat) -> String {
    if value.is_zero() {
        return "0.0E0".to_owned();
    }
    let magnitude = value.abs();
    let mut exponent = floor_log10(&magnitude).unwrap_or(0);
    let scaled = magnitude.mul(&Rat::pow10(15 - exponent));
    let floor = scaled.floor();
    let half = Rat::new(Int::from_i64(1), Nat::from_u64(2)).unwrap_or_else(Rat::zero);
    let mut digits = if scaled.sub(&Rat::from_int(floor.clone())) >= half {
        floor.add(&Int::from_i64(1))
    } else {
        floor
    }
    .to_string();
    if digits.len() > 16 {
        digits.truncate(16);
        exponent += 1;
    }
    let trimmed = digits.trim_end_matches('0');
    let fraction = if trimmed.len() < 2 {
        "0"
    } else {
        &trimmed[1..]
    };
    let sign = if value.is_negative() { "-" } else { "" };
    format!("{sign}{}.{fraction}E{exponent}", &digits[..1])
}

/// The lexical form and datatype of a JSON number typed `datatype`, or
/// `None` when its binary64 rounding overflows.
#[must_use]
pub fn number_form(text: &str, datatype: Option<&str>) -> Option<(String, String)> {
    let (exact, negative) = numeral(text)?;
    let Rounded::Finite(bits) = round_to(Format::B64, &exact, negative) else {
        return None;
    };
    let FloatValue::Finite { value, .. } = decode(Format::B64, bits) else {
        return None;
    };
    let small = value.abs() < Rat::pow10(21);
    if value.is_integer() && small && datatype != Some(XSD_DOUBLE) {
        let lexical = if value.is_zero() {
            "0".to_owned()
        } else {
            value.floor().to_string()
        };
        return Some((lexical, datatype.unwrap_or(XSD_INTEGER).to_owned()));
    }
    Some((
        double_form(&value),
        datatype.unwrap_or(XSD_DOUBLE).to_owned(),
    ))
}

// ── Contexts ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
struct Def {
    iri: Option<String>,
    reverse: bool,
    ty: Option<String>,
    list: bool,
    language: Option<Option<String>>,
    prefix: bool,
}

#[derive(Clone, Debug, Default)]
struct Ctx {
    base: Option<String>,
    original: Option<String>,
    vocab: Option<String>,
    language: Option<String>,
    defs: BTreeMap<String, Def>,
}

/// One context definition while its terms are defined.
struct Local<'v> {
    entries: &'v [(String, Value)],
    pointer: String,
    defined: BTreeMap<String, bool>,
}

impl<'v> Local<'v> {
    fn get(&self, key: &str) -> Option<&'v Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

fn text(v: &Value) -> Option<&str> {
    if let Value::String(s) = v {
        Some(s)
    } else {
        None
    }
}

/// IRI Expansion (API Sec 5.2); `None` is `null`.
fn expand_iri(
    ctx: &mut Ctx,
    value: &str,
    relative: bool,
    vocab: bool,
    mut local: Option<&mut Local<'_>>,
) -> R<Option<String>> {
    if keyword(value) {
        return Ok(Some(value.to_owned()));
    }
    if keyword_form(value) {
        return Ok(None);
    }
    if let Some(l) = local.as_deref_mut()
        && l.get(value).is_some()
        && l.defined.get(value) != Some(&true)
    {
        define(ctx, l, value)?;
    }
    if let Some(def) = ctx.defs.get(value) {
        if def.iri.as_deref().is_some_and(keyword) {
            return Ok(def.iri.clone());
        }
        if vocab {
            return Ok(def.iri.clone());
        }
    }
    if let Some(i) = value.find(':')
        && i > 0
    {
        let (prefix, suffix) = (&value[..i], &value[i + 1..]);
        if prefix == "_" || suffix.starts_with("//") {
            return Ok(Some(value.to_owned()));
        }
        if let Some(l) = local.as_deref_mut()
            && l.get(prefix).is_some()
            && l.defined.get(prefix) != Some(&true)
        {
            define(ctx, l, prefix)?;
        }
        if let Some(Def {
            iri: Some(mapping),
            prefix: true,
            ..
        }) = ctx.defs.get(prefix)
        {
            return Ok(Some(format!("{mapping}{suffix}")));
        }
        if absolute(value) {
            return Ok(Some(value.to_owned()));
        }
    }
    if vocab && let Some(v) = &ctx.vocab {
        return Ok(Some(format!("{v}{value}")));
    }
    if relative && let Some(b) = &ctx.base {
        return Ok(Some(resolve(b, value)));
    }
    Ok(Some(value.to_owned()))
}

/// Create Term Definition (API Sec 4.2).
fn define(ctx: &mut Ctx, local: &mut Local<'_>, term: &str) -> R<()> {
    let Some(value) = local.get(term) else {
        return Ok(());
    };
    let here = below(&local.pointer, term);
    match local.defined.get(term) {
        Some(true) => return Ok(()),
        Some(false) => return Err(err("cyclic IRI mapping", &here)),
        None => {}
    }
    if term.is_empty() {
        return Err(err("invalid term definition", &here));
    }
    local.defined.insert(term.to_owned(), false);
    if term == "@type" {
        let Value::Object(entries) = value else {
            return Err(err("keyword redefinition", &here));
        };
        if entries.iter().any(|(k, _)| k == "@protected") {
            return Err(unsupported("@protected", &below(&here, "@protected")));
        }
        if entries.is_empty()
            || !entries
                .iter()
                .all(|(k, v)| k == "@container" && text(v) == Some("@set"))
        {
            return Err(err("keyword redefinition", &here));
        }
        ctx.defs.insert(
            "@type".into(),
            Def {
                iri: Some("@type".into()),
                ..Def::default()
            },
        );
    } else if keyword(term) {
        return Err(err("keyword redefinition", &here));
    } else if !keyword_form(term) {
        ctx.defs.remove(term);
        if let Some(def) = definition(ctx, local, term, value, &here)? {
            ctx.defs.insert(term.to_owned(), def);
        }
    }
    local.defined.insert(term.to_owned(), true);
    Ok(())
}

/// The definition of `term` from `value`, or `None` when a value of keyword
/// form ends the algorithm early.
fn definition(
    ctx: &mut Ctx,
    local: &mut Local<'_>,
    term: &str,
    value: &Value,
    here: &str,
) -> R<Option<Def>> {
    let (map, simple): (Vec<(String, Value)>, bool) = match value {
        Value::Null => (vec![("@id".into(), Value::Null)], false),
        Value::String(_) => (vec![("@id".into(), value.clone())], true),
        Value::Object(entries) => (entries.clone(), false),
        _ => return Err(err("invalid term definition", here)),
    };
    // A string or `null` definition is its own `@id`, at the term's place.
    let at = |key: &str| {
        if matches!(value, Value::Object(_)) {
            below(here, key)
        } else {
            here.to_owned()
        }
    };
    let get = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let mut def = Def::default();
    if get("@protected").is_some() {
        return Err(unsupported("@protected", &at("@protected")));
    }
    if let Some(ty) = get("@type") {
        let Some(t) = text(&ty) else {
            return Err(err("invalid type mapping", &at("@type")));
        };
        let expanded = expand_iri(ctx, t, false, true, Some(local))?;
        match expanded.as_deref() {
            Some("@json") => return Err(unsupported("@json", &at("@type"))),
            Some("@id" | "@vocab" | "@none") => {}
            Some(i) if iri(i) => {}
            _ => return Err(err("invalid type mapping", &at("@type"))),
        }
        def.ty = expanded;
    }
    if let Some(reverse) = get("@reverse") {
        if get("@id").is_some() || get("@nest").is_some() {
            return Err(err("invalid reverse property", here));
        }
        let Some(r) = text(&reverse) else {
            return Err(err("invalid IRI mapping", &at("@reverse")));
        };
        if keyword_form(r) {
            return Ok(None);
        }
        let mapped = expand_iri(ctx, r, false, true, Some(local))?;
        if !mapped
            .as_deref()
            .is_some_and(|m| iri(m) || m.starts_with("_:"))
        {
            return Err(err("invalid IRI mapping", &at("@reverse")));
        }
        def.iri = mapped;
        def.reverse = true;
    } else if let Some(id) = get("@id").filter(|id| text(id) != Some(term)) {
        if id != Value::Null {
            let Some(t) = text(&id) else {
                return Err(err("invalid IRI mapping", &at("@id")));
            };
            if !keyword(t) && keyword_form(t) {
                return Ok(None);
            }
            let mapped = expand_iri(ctx, t, false, true, Some(local))?;
            match mapped.as_deref() {
                Some("@context") => return Err(err("invalid keyword alias", &at("@id"))),
                Some(m) if keyword(m) || iri(m) || m.starts_with("_:") => {}
                _ => return Err(err("invalid IRI mapping", &at("@id"))),
            }
            def.iri = mapped;
            let chars: Vec<char> = term.chars().collect();
            let inner = (1..chars.len().saturating_sub(1)).any(|i| chars[i] == ':');
            if inner || term.contains('/') {
                local.defined.insert(term.to_owned(), true);
                if expand_iri(ctx, term, false, true, Some(local))? != def.iri {
                    return Err(err("invalid IRI mapping", &at("@id")));
                }
            }
            let delim = def.iri.as_deref().is_some_and(|m| {
                m.starts_with("_:") || m.ends_with([':', '/', '?', '#', '[', ']', '@'])
            });
            if !term.contains(':') && !term.contains('/') && simple && delim {
                def.prefix = true;
            }
        }
    } else if let Some(i) = term.find(':').filter(|&i| i > 0) {
        let prefix = &term[..i];
        if local.get(prefix).is_some() {
            define(ctx, local, prefix)?;
        }
        def.iri = Some(match ctx.defs.get(prefix).and_then(|d| d.iri.clone()) {
            Some(m) => format!("{m}{}", &term[i + 1..]),
            None => term.to_owned(),
        });
    } else if term.contains('/') {
        let mapped = expand_iri(ctx, term, false, true, None)?;
        if !mapped.as_deref().is_some_and(iri) {
            return Err(err("invalid IRI mapping", here));
        }
        def.iri = mapped;
    } else if let Some(v) = &ctx.vocab {
        def.iri = Some(format!("{v}{term}"));
    } else {
        return Err(err("invalid IRI mapping", here));
    }
    if let Some(container) = get("@container") {
        container_mapping(&container, &at("@container"), &mut def)?;
    }
    for (key, feature) in [("@index", "@index"), ("@context", "scoped context")] {
        if get(key).is_some() {
            return Err(unsupported(feature, &at(key)));
        }
    }
    if get("@type").is_none()
        && let Some(language) = get("@language")
    {
        def.language = Some(match language {
            Value::Null => None,
            Value::String(s) => Some(s),
            _ => return Err(err("invalid language mapping", &at("@language"))),
        });
    }
    for key in ["@direction", "@nest"] {
        if get(key).is_some() {
            return Err(unsupported(key, &at(key)));
        }
    }
    if let Some(prefix) = get("@prefix") {
        if term.contains(':') || term.contains('/') {
            return Err(err("invalid term definition", here));
        }
        let Value::Bool(flag) = prefix else {
            return Err(err("invalid @prefix value", &at("@prefix")));
        };
        def.prefix = flag;
        if flag && def.iri.as_deref().is_some_and(keyword) {
            return Err(err("invalid term definition", here));
        }
    }
    let allowed = [
        "@id",
        "@reverse",
        "@container",
        "@context",
        "@direction",
        "@index",
        "@language",
        "@nest",
        "@prefix",
        "@protected",
        "@type",
    ];
    if map.iter().any(|(k, _)| !allowed.contains(&k.as_str())) {
        return Err(err("invalid term definition", here));
    }
    Ok(Some(def))
}

/// A term's `@container` (API Sec 4.2 step 21), within the subset.
fn container_mapping(container: &Value, at: &str, def: &mut Def) -> R<()> {
    if def.reverse {
        return match text(container) {
            _ if *container == Value::Null => Ok(()),
            Some("@set") => Ok(()),
            Some("@index") => Err(unsupported("@container @index", at)),
            _ => Err(err("invalid reverse property", at)),
        };
    }
    let words: Vec<&str> = match container {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) => match items.iter().map(text).collect::<Option<Vec<_>>>() {
            Some(words) => words,
            None => return Err(err("invalid container mapping", at)),
        },
        _ => return Err(err("invalid container mapping", at)),
    };
    let known = [
        "@graph",
        "@id",
        "@index",
        "@language",
        "@list",
        "@set",
        "@type",
    ];
    let distinct: BTreeSet<&str> = words.iter().copied().collect();
    let has = |w: &str| distinct.contains(w);
    let valid = distinct.len() == words.len()
        && words.iter().all(|w| known.contains(w))
        && (words.len() == 1
            || (has("@graph")
                && (has("@id") ^ has("@index"))
                && words
                    .iter()
                    .all(|w| ["@graph", "@id", "@index", "@set"].contains(w)))
            || (has("@set") && !has("@list")));
    if !valid {
        return Err(err("invalid container mapping", at));
    }
    match words.as_slice() {
        ["@list"] => def.list = true,
        ["@set"] => {}
        _ => {
            let first = words
                .iter()
                .find(|w| **w != "@set")
                .copied()
                .unwrap_or("@set");
            return Err(unsupported(&format!("@container {first}"), at));
        }
    }
    Ok(())
}

/// Context Processing (API Sec 4.1).
fn process(active: &Ctx, local: &Value, pointer: &str) -> R<Ctx> {
    let mut result = active.clone();
    let contexts: Vec<(String, &Value)> = match local {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (below(pointer, &i.to_string()), v))
            .collect(),
        other => vec![(pointer.to_owned(), other)],
    };
    for (here, context) in contexts {
        let entries = match context {
            Value::Null => {
                result = Ctx {
                    base: active.original.clone(),
                    original: active.original.clone(),
                    ..Ctx::default()
                };
                continue;
            }
            Value::String(_) => return Err(unsupported("remote context", &here)),
            Value::Object(entries) => entries,
            _ => return Err(err("invalid local context", &here)),
        };
        let get = |k: &str| entries.iter().find(|(key, _)| key == k).map(|(_, v)| v);
        if let Some(version) = get("@version") {
            let ok = matches!(version, Value::Number(n)
                if numeral(n).is_some_and(|(q, _)| q == Rat::new(Int::from_i64(11), Nat::from_u64(10)).unwrap_or_else(Rat::zero)));
            if !ok {
                return Err(err("invalid @version value", &below(&here, "@version")));
            }
        }
        for k in ["@import", "@direction", "@propagate", "@protected"] {
            if get(k).is_some() {
                return Err(unsupported(k, &below(&here, k)));
            }
        }
        if let Some(base) = get("@base") {
            result.base = match (base, &result.base) {
                (Value::Null, _) => None,
                (Value::String(s), _) if iri(s) => Some(s.clone()),
                (Value::String(s), Some(current)) => Some(resolve(current, s)),
                _ => return Err(err("invalid base IRI", &below(&here, "@base"))),
            };
        }
        if let Some(vocab) = get("@vocab") {
            result.vocab = match vocab {
                Value::Null => None,
                Value::String(s) => match expand_iri(&mut result, s, true, true, None)? {
                    Some(v) if iri(&v) || v.starts_with("_:") => Some(v),
                    _ => return Err(err("invalid vocab mapping", &below(&here, "@vocab"))),
                },
                _ => return Err(err("invalid vocab mapping", &below(&here, "@vocab"))),
            };
        }
        if let Some(language) = get("@language") {
            result.language = match language {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(err("invalid default language", &below(&here, "@language"))),
            };
        }
        let mut l = Local {
            entries,
            pointer: here.clone(),
            defined: BTreeMap::new(),
        };
        let skipped = [
            "@base",
            "@direction",
            "@import",
            "@language",
            "@propagate",
            "@protected",
            "@version",
            "@vocab",
        ];
        for (key, _) in entries {
            if !skipped.contains(&key.as_str()) {
                define(&mut result, &mut l, key)?;
            }
        }
    }
    Ok(result)
}

// ── Expansion ───────────────────────────────────────────────────────

/// A scalar value.
#[derive(Clone, Debug)]
enum Scalar {
    Str(String),
    Num(String),
    Bool(bool),
}

/// The expanded form: node, value and list objects, each with the pointer
/// of the JSON value that gave it.
#[derive(Clone, Debug)]
enum Expanded {
    Node {
        at: String,
        id: Option<(String, String)>,
        types: Vec<(String, String)>,
        props: Vec<(String, String, Vec<Expanded>)>,
        reverse: Vec<(String, String, Vec<Expanded>)>,
        graph: Option<Vec<Expanded>>,
    },
    Value {
        at: String,
        value: Scalar,
        value_at: String,
        datatype: Option<(String, String)>,
        language: Option<(String, String)>,
    },
    List {
        at: String,
        items: Vec<Expanded>,
    },
}

impl Expanded {
    fn at(&self) -> &str {
        match self {
            Self::Node { at, .. } | Self::Value { at, .. } | Self::List { at, .. } => at,
        }
    }
}

/// The active property: `None` at the top level, else `@graph`,
/// `@reverse` or a key.
type Prop<'a> = Option<&'a str>;

fn floating(prop: Prop<'_>) -> bool {
    matches!(prop, None | Some("@graph"))
}

/// What one expansion step gives.
enum Out {
    Nothing,
    Single(Expanded),
    Several(Vec<Expanded>),
}

impl Out {
    fn all(self) -> Vec<Expanded> {
        match self {
            Self::Nothing => vec![],
            Self::Single(e) => vec![e],
            Self::Several(v) => v,
        }
    }
}

fn definition_of<'c>(ctx: &'c Ctx, prop: Prop<'_>) -> Option<&'c Def> {
    match prop {
        Some(key) if key != "@graph" && key != "@reverse" => ctx.defs.get(key),
        _ => None,
    }
}

fn expand(ctx: &Ctx, prop: Prop<'_>, element: &Value, at: &str) -> R<Out> {
    match element {
        Value::Null => Ok(Out::Nothing),
        Value::Array(items) => {
            let list = definition_of(ctx, prop).is_some_and(|d| d.list);
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let here = below(at, &i.to_string());
                match expand(ctx, prop, item, &here)? {
                    Out::Several(inner) if list => out.push(Expanded::List {
                        at: here,
                        items: inner,
                    }),
                    other => out.extend(other.all()),
                }
            }
            Ok(Out::Several(out))
        }
        Value::Object(entries) => expand_map(&mut ctx.clone(), prop, entries, at),
        scalar => {
            if floating(prop) {
                return Err(kind("free-floating", at));
            }
            value_expansion(&mut ctx.clone(), prop, scalar, at).map(Out::Single)
        }
    }
}

fn scalar(v: &Value) -> Option<Scalar> {
    match v {
        Value::String(s) => Some(Scalar::Str(s.clone())),
        Value::Number(n) => Some(Scalar::Num(n.clone())),
        Value::Bool(b) => Some(Scalar::Bool(*b)),
        _ => None,
    }
}

/// Value Expansion (API Sec 5.3).
fn value_expansion(ctx: &mut Ctx, prop: Prop<'_>, value: &Value, at: &str) -> R<Expanded> {
    let def = definition_of(ctx, prop).cloned().unwrap_or_default();
    if let (Value::String(s), Some(mode @ ("@id" | "@vocab"))) = (value, def.ty.as_deref()) {
        let Some(id) = expand_iri(ctx, s, true, mode == "@vocab", None)? else {
            return Err(kind("reserved-keyword", at));
        };
        return Ok(Expanded::Node {
            at: at.to_owned(),
            id: Some((id, at.to_owned())),
            types: vec![],
            props: vec![],
            reverse: vec![],
            graph: None,
        });
    }
    let mut datatype = None;
    let mut language = None;
    match def.ty.as_deref() {
        None | Some("@id" | "@vocab" | "@none") => {
            if let Value::String(_) = value {
                let tag = def.language.clone().unwrap_or_else(|| ctx.language.clone());
                language = tag.map(|t| (t, at.to_owned()));
            }
        }
        Some(t) => datatype = Some((t.to_owned(), at.to_owned())),
    }
    Ok(Expanded::Value {
        at: at.to_owned(),
        value: scalar(value).unwrap_or(Scalar::Bool(false)),
        value_at: at.to_owned(),
        datatype,
        language,
    })
}

const BODY: [&str; 9] = [
    "@context",
    "@graph",
    "@id",
    "@language",
    "@list",
    "@reverse",
    "@set",
    "@type",
    "@value",
];

#[derive(Default)]
struct Map {
    id: Option<(String, String)>,
    types: Vec<(String, String)>,
    one_type_string: bool,
    typed: bool,
    graph: Option<Vec<Expanded>>,
    value: Option<(Value, String)>,
    language: Option<(String, String)>,
    list: Option<Vec<Expanded>>,
    set: Option<Out>,
    props: Vec<(String, String, Vec<Expanded>)>,
    reverse: Vec<(String, String, Vec<Expanded>)>,
}

fn reverse_value_check(items: &[Expanded]) -> R<()> {
    for item in items {
        if !matches!(item, Expanded::Node { .. }) {
            return Err(err("invalid reverse property value", item.at()));
        }
    }
    Ok(())
}

/// Expansion steps 7 to 20 for a map.
fn expand_map(ctx: &mut Ctx, prop: Prop<'_>, entries: &[(String, Value)], at: &str) -> R<Out> {
    if let Some((_, local)) = entries.iter().find(|(k, _)| k == "@context") {
        *ctx = process(ctx, local, &below(at, "@context"))?;
    }
    for (key, value) in entries {
        if key == "@context" || expand_iri(ctx, key, false, true, None)?.as_deref() != Some("@type")
        {
            continue;
        }
        let here = below(at, key);
        let values: Vec<(String, &Value)> = match value {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| (below(&here, &i.to_string()), v))
                .collect(),
            other => vec![(here.clone(), other)],
        };
        for (p, v) in values {
            if let Value::String(s) = v
                && expand_iri(ctx, s, true, true, None)?.as_deref() == Some("@json")
            {
                return Err(unsupported("@json", &p));
            }
        }
    }
    let mut m = Map::default();
    let mut used: Vec<String> = Vec::new();
    for (key, value) in entries {
        if key == "@context" {
            continue;
        }
        let here = below(at, key);
        let Some(property) = expand_iri(ctx, key, false, true, None)? else {
            continue;
        };
        if keyword(&property) {
            if prop == Some("@reverse") {
                return Err(err("invalid reverse property map", &here));
            }
            if property != "@type" && used.contains(&property) {
                return Err(err("colliding keywords", &here));
            }
            if !BODY.contains(&property.as_str()) {
                return Err(unsupported(&property, &here));
            }
            used.push(property.clone());
            keyword_value(ctx, prop, &property, value, &here, &mut m)?;
        } else if property.contains(':') {
            let def = ctx.defs.get(key).cloned().unwrap_or_default();
            let out = expand(ctx, Some(key), value, &here)?;
            let items = match out {
                Out::Nothing => continue,
                Out::Single(list @ Expanded::List { .. }) => vec![list],
                other if def.list => vec![Expanded::List {
                    at: here.clone(),
                    items: other.all(),
                }],
                other => other.all(),
            };
            if def.reverse {
                reverse_value_check(&items)?;
                m.reverse.push((property, here, items));
            } else {
                m.props.push((property, here, items));
            }
        }
    }
    classify(prop, m, at)
}

/// Expansion step 13.4 for one keyword entry.
fn keyword_value(
    ctx: &mut Ctx,
    prop: Prop<'_>,
    property: &str,
    value: &Value,
    here: &str,
    m: &mut Map,
) -> R<()> {
    match property {
        "@id" => {
            let Value::String(s) = value else {
                return Err(err("invalid @id value", here));
            };
            let Some(id) = expand_iri(ctx, s, true, false, None)? else {
                return Err(kind("reserved-keyword", here));
            };
            m.id = Some((id, here.to_owned()));
        }
        "@type" => {
            let values: Vec<(String, &str)> = match value {
                Value::String(s) => vec![(here.to_owned(), s.as_str())],
                Value::Array(items) => {
                    let mut out = Vec::new();
                    for (i, item) in items.iter().enumerate() {
                        let Value::String(s) = item else {
                            return Err(err("invalid type value", here));
                        };
                        out.push((below(here, &i.to_string()), s.as_str()));
                    }
                    out
                }
                _ => return Err(err("invalid type value", here)),
            };
            m.one_type_string = !m.typed && matches!(value, Value::String(_));
            m.typed = true;
            for (p, s) in values {
                let Some(t) = expand_iri(ctx, s, true, true, None)? else {
                    return Err(kind("reserved-keyword", &p));
                };
                m.types.push((t, p));
            }
        }
        "@graph" => m.graph = Some(expand(ctx, Some("@graph"), value, here)?.all()),
        "@value" => {
            if matches!(value, Value::Array(_) | Value::Object(_)) {
                return Err(err("invalid value object value", here));
            }
            m.value = Some((value.clone(), here.to_owned()));
        }
        "@language" => {
            let Value::String(s) = value else {
                return Err(err("invalid language-tagged string", here));
            };
            m.language = Some((s.clone(), here.to_owned()));
        }
        "@list" => {
            if floating(prop) {
                return Err(kind("free-floating", here));
            }
            m.list = Some(expand(ctx, prop, value, here)?.all());
        }
        "@set" => m.set = Some(expand(ctx, prop, value, here)?),
        _ => {
            if !matches!(value, Value::Object(_)) {
                return Err(err("invalid @reverse value", here));
            }
            if let Out::Single(Expanded::Node { props, reverse, .. }) =
                expand(ctx, Some("@reverse"), value, here)?
            {
                m.props.extend(reverse);
                for (p, k, items) in props {
                    reverse_value_check(&items)?;
                    m.reverse.push((p, k, items));
                }
            }
        }
    }
    Ok(())
}

/// Expansion steps 15 to 20.
fn classify(prop: Prop<'_>, m: Map, at: &str) -> R<Out> {
    let node_like =
        m.id.is_some() || m.graph.is_some() || !m.props.is_empty() || !m.reverse.is_empty();
    if let Some((value, value_at)) = m.value {
        if node_like || m.list.is_some() || m.set.is_some() || (m.typed && m.language.is_some()) {
            return Err(err("invalid value object", at));
        }
        let Some(value) = scalar(&value) else {
            return Ok(Out::Nothing);
        };
        if m.language.is_some() && !matches!(value, Scalar::Str(_)) {
            return Err(err("invalid language-tagged value", at));
        }
        let mut datatype = None;
        if m.typed {
            match m.types.as_slice() {
                [(t, p)] if m.one_type_string && iri(t) => datatype = Some((t.clone(), p.clone())),
                _ => return Err(err("invalid typed value", at)),
            }
        }
        if floating(prop) {
            return Err(kind("free-floating", at));
        }
        return Ok(Out::Single(Expanded::Value {
            at: at.to_owned(),
            value,
            value_at,
            datatype,
            language: m.language,
        }));
    }
    if m.list.is_some() || m.set.is_some() {
        if node_like || m.typed || m.language.is_some() || (m.list.is_some() && m.set.is_some()) {
            return Err(err("invalid set or list object", at));
        }
        return Ok(match (m.list, m.set) {
            (Some(items), _) => Out::Single(Expanded::List {
                at: at.to_owned(),
                items,
            }),
            (None, Some(set)) => set,
            (None, None) => Out::Nothing,
        });
    }
    if m.language.is_some() {
        if node_like || m.typed {
            return Err(unsupported("@language in a node object", at));
        }
        return Ok(Out::Nothing);
    }
    if floating(prop)
        && m.types.is_empty()
        && m.props.is_empty()
        && m.reverse.is_empty()
        && m.graph.is_none()
    {
        return Ok(Out::Nothing);
    }
    Ok(Out::Single(Expanded::Node {
        at: at.to_owned(),
        id: m.id,
        types: m.types,
        props: m.props,
        reverse: m.reverse,
        graph: m.graph,
    }))
}

// ── Triples ─────────────────────────────────────────────────────────

struct Triples {
    out: Vec<Triple>,
    kept: HashSet<String>,
    renamed: BTreeMap<String, String>,
    counter: u64,
}

/// Keep the label of the blank node identifier `id`, when it is a label.
fn keep(id: &str, into: &mut HashSet<String>) {
    if let Some(l) = id.strip_prefix("_:")
        && blank_label(l)
    {
        into.insert(l.to_owned());
    }
}

fn kept(items: &[Expanded], into: &mut HashSet<String>) {
    for item in items {
        match item {
            Expanded::Node {
                id,
                types,
                props,
                reverse,
                graph,
                ..
            } => {
                if let Some((i, _)) = id {
                    keep(i, into);
                }
                for (t, _) in types {
                    keep(t, into);
                }
                for (_, _, v) in props.iter().chain(reverse) {
                    kept(v, into);
                }
                if let Some(g) = graph {
                    kept(g, into);
                }
            }
            Expanded::List { items, .. } => kept(items, into),
            Expanded::Value { .. } => {}
        }
    }
}

fn well_formed(i: &str, at: &str) -> R<()> {
    if i.chars().any(excluded) {
        return Err(kind("iri-character", at));
    }
    if !absolute(i) {
        return Err(kind("relative-iri", at));
    }
    if i.chars().filter(|&c| c == '#').count() > 1 {
        return Err(kind("malformed-iri", at));
    }
    Ok(())
}

impl Triples {
    fn fresh(&mut self) -> String {
        loop {
            let l = format!("b{}", self.counter);
            self.counter += 1;
            if !self.kept.contains(&l) {
                return l;
            }
        }
    }

    fn reference(&mut self, id: &str, at: &str) -> R<Term> {
        if let Some(l) = id.strip_prefix("_:") {
            if blank_label(l) {
                return Ok(Term::Blank(l.to_owned()));
            }
            if let Some(f) = self.renamed.get(l) {
                return Ok(Term::Blank(f.clone()));
            }
            let f = self.fresh();
            self.renamed.insert(l.to_owned(), f.clone());
            return Ok(Term::Blank(f));
        }
        well_formed(id, at)?;
        Ok(Term::Iri(id.to_owned()))
    }

    fn predicate(p: &str, at: &str) -> R<String> {
        if p.starts_with("_:") {
            return Err(kind("blank-predicate", at));
        }
        well_formed(p, at)?;
        Ok(p.to_owned())
    }

    fn object(&mut self, item: &Expanded) -> R<Term> {
        match item {
            Expanded::Node {
                at,
                id,
                types,
                props,
                reverse,
                graph,
            } => {
                if graph.is_some() {
                    return Err(kind("named-graph", at));
                }
                let subject = match id {
                    Some((i, p)) => self.reference(i, p)?,
                    None => Term::Blank(self.fresh()),
                };
                for (t, p) in types {
                    let o = self.reference(t, p)?;
                    self.out.push(Triple(subject.clone(), RDF_TYPE.into(), o));
                }
                for (p, k, values) in props {
                    let p = Self::predicate(p, k)?;
                    for v in values {
                        let o = self.object(v)?;
                        self.out.push(Triple(subject.clone(), p.clone(), o));
                    }
                }
                for (p, k, values) in reverse {
                    let p = Self::predicate(p, k)?;
                    for v in values {
                        let s = self.object(v)?;
                        self.out.push(Triple(s, p.clone(), subject.clone()));
                    }
                }
                Ok(subject)
            }
            Expanded::List { items, .. } => {
                if items.is_empty() {
                    return Ok(Term::Iri(RDF_NIL.into()));
                }
                let cells: Vec<String> = items.iter().map(|_| self.fresh()).collect();
                for (i, item) in items.iter().enumerate() {
                    let o = self.object(item)?;
                    let cell = Term::Blank(cells[i].clone());
                    self.out.push(Triple(cell.clone(), RDF_FIRST.into(), o));
                    let rest = cells
                        .get(i + 1)
                        .map_or_else(|| Term::Iri(RDF_NIL.into()), |c| Term::Blank(c.clone()));
                    self.out.push(Triple(cell, RDF_REST.into(), rest));
                }
                Ok(Term::Blank(cells[0].clone()))
            }
            Expanded::Value {
                value,
                value_at,
                datatype,
                language,
                ..
            } => {
                if let Some((d, p)) = datatype {
                    well_formed(d, p)?;
                }
                let dt = datatype.as_ref().map(|(d, _)| d.as_str());
                let (lexical, mut ty) = match value {
                    Scalar::Str(s) => (s.clone(), dt.unwrap_or(XSD_STRING).to_owned()),
                    Scalar::Num(n) => {
                        number_form(n, dt).ok_or_else(|| kind("number-range", value_at))?
                    }
                    Scalar::Bool(b) => (b.to_string(), dt.unwrap_or(XSD_BOOLEAN).to_owned()),
                };
                let tag = match language {
                    Some((t, p)) if !language_tag(t) => return Err(kind("language-tag", p)),
                    Some((t, _)) => {
                        if dt.is_none() {
                            ty = RDF_LANG_STRING.to_owned();
                        }
                        Some(t.clone())
                    }
                    None => None,
                };
                Ok(Term::Literal(lexical, ty, tag))
            }
        }
    }
}

/// Every member, in document order, whose key an earlier member of the
/// same object has: the first one found, by its pointer.
fn first_duplicate(value: &Value, at: &str) -> Option<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| first_duplicate(v, &below(at, &i.to_string()))),
        Value::Object(entries) => {
            let mut seen = HashSet::new();
            for (k, v) in entries {
                let here = below(at, k);
                if !seen.insert(k.as_str()) {
                    return Some(here);
                }
                if let Some(found) = first_duplicate(v, &here) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// The triples of the JSON-LD document `text` read with base `base`, in
/// the order the page states, or its refusal.
///
/// # Errors
///
/// Returns [`Refusal::NotJson`] when the text is not one JSON value, and
/// [`Refusal::JsonLd`] with the kind and pointer of the first refusal.
pub fn read_document(text: &str, base: Option<&str>) -> Result<Vec<Triple>, Refusal> {
    let document = parse(text).ok_or(Refusal::NotJson)?;
    if let Some(at) = first_duplicate(&document, "") {
        return Err(Refusal::JsonLd("duplicate-key".into(), at));
    }
    read_value(&document, base).map_err(|(k, p)| Refusal::JsonLd(k, p))
}

fn read_value(document: &Value, base: Option<&str>) -> R<Vec<Triple>> {
    let ctx = Ctx {
        base: base.map(str::to_owned),
        original: base.map(str::to_owned),
        ..Ctx::default()
    };
    let mut items = expand(&ctx, None, document, "")?.all();
    if let (
        Value::Object(_),
        [
            Expanded::Node {
                id: None,
                types,
                props,
                reverse,
                graph: Some(graph),
                ..
            },
        ],
    ) = (document, items.as_slice())
        && types.is_empty()
        && props.is_empty()
        && reverse.is_empty()
    {
        items = graph.clone();
    }
    let mut keep = HashSet::new();
    kept(&items, &mut keep);
    let mut walk = Triples {
        out: Vec::new(),
        kept: keep,
        renamed: BTreeMap::new(),
        counter: 0,
    };
    for item in &items {
        match item {
            Expanded::Node { .. } => {
                walk.object(item)?;
            }
            other => return Err(kind("free-floating", other.at())),
        }
    }
    Ok(walk.out)
}

/// The canonical JSON-LD text of a set of triples, as the page defines it.
#[must_use]
pub fn canonical_text(triples: &[Triple]) -> String {
    fn string(s: &str) -> String {
        let mut out = String::from('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\u{8}' => out.push_str("\\b"),
                '\u{C}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    fn id(term: &Term) -> String {
        match term {
            Term::Blank(l) => string(&format!("_:{l}")),
            Term::Iri(i) => string(i),
            Term::Literal(l, ..) => string(l),
        }
    }
    let mut by_subject: BTreeMap<String, (Term, BTreeMap<String, BTreeMap<String, Term>>)> =
        BTreeMap::new();
    for Triple(s, p, o) in triples {
        by_subject
            .entry(term_text(s))
            .or_insert_with(|| (s.clone(), BTreeMap::new()))
            .1
            .entry(p.clone())
            .or_default()
            .insert(term_text(o), o.clone());
    }
    let nodes: Vec<String> = by_subject
        .values()
        .map(|(s, preds)| {
            let mut node = format!("  {{\"@id\": {}", id(s));
            for (p, objects) in preds {
                let values: Vec<String> = objects
                    .values()
                    .map(|o| match o {
                        Term::Literal(l, _, Some(tag)) => {
                            format!(
                                "{{\"@value\": {}, \"@language\": {}}}",
                                string(l),
                                string(tag)
                            )
                        }
                        Term::Literal(l, d, None) if d == XSD_STRING || d == RDF_LANG_STRING => {
                            format!("{{\"@value\": {}}}", string(l))
                        }
                        Term::Literal(l, d, None) => {
                            format!("{{\"@value\": {}, \"@type\": {}}}", string(l), string(d))
                        }
                        other => format!("{{\"@id\": {}}}", id(other)),
                    })
                    .collect();
                node.push_str(&format!(", {}: [{}]", string(p), values.join(", ")));
            }
            node.push('}');
            node
        })
        .collect();
    if nodes.is_empty() {
        "[]\n".to_owned()
    } else {
        format!("[\n{}\n]\n", nodes.join(",\n"))
    }
}
