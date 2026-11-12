//! Context processing and expansion (JSON-LD 1.1 API Sec 4.1, 4.2, 5.1,
//! 5.2 and 5.3), over the subset the module documentation states.
//!
//! The expanded document is held as [`Item`]s rather than as JSON: each
//! node object, value object and list object keeps the place of every value
//! that wrote one of its terms, so that a refusal can name it and the
//! format-preserving route can rewrite it. Properties are kept in document
//! order without merging two keys that expand to the same IRI, which the
//! RDF reading of the API does not distinguish.

use std::collections::HashMap;
use std::rc::Rc;

use super::json::{Json, Kind, Member};
use super::{JsonLdErrorCode, JsonLdRefusal};
use crate::rdf::turtle::resolve_iri;
use crate::rdf::{is_absolute_iri, is_iri_excluded};

/// Where a term or value of the expanded document comes from.
#[derive(Clone, Debug)]
pub(crate) struct Origin {
    /// The JSON Pointer of the value that wrote it.
    pub(crate) pointer: Rc<str>,
    /// That value's first byte.
    pub(crate) offset: usize,
    /// The token that spells it, when the document writes it with a token
    /// of its own.
    pub(crate) token: Option<usize>,
}

impl Origin {
    /// The place of `json`, written by its own token.
    fn token(json: &Json) -> Self {
        Self {
            pointer: json.pointer.clone(),
            offset: json.offset,
            token: Some(json.token),
        }
    }

    /// The place of `json`, with no token of its own.
    fn implicit(json: &Json) -> Self {
        Self {
            pointer: json.pointer.clone(),
            offset: json.offset,
            token: None,
        }
    }

    /// The key of `member`, which its own token writes; a refusal names the
    /// member's value.
    fn key(member: &Member) -> Self {
        Self {
            pointer: member.value.pointer.clone(),
            offset: member.value.offset,
            token: Some(member.key_token),
        }
    }
}

/// A refusal at a place of the document.
#[derive(Clone, Debug)]
pub(crate) struct Fault {
    pub(crate) refusal: JsonLdRefusal,
    pub(crate) pointer: Rc<str>,
    pub(crate) offset: usize,
}

impl Fault {
    pub(crate) fn at(refusal: JsonLdRefusal, json: &Json) -> Self {
        Self {
            refusal,
            pointer: json.pointer.clone(),
            offset: json.offset,
        }
    }

    pub(crate) fn at_origin(refusal: JsonLdRefusal, origin: &Origin) -> Self {
        Self {
            refusal,
            pointer: origin.pointer.clone(),
            offset: origin.offset,
        }
    }
}

type Res<T> = Result<T, Fault>;

fn error(code: JsonLdErrorCode, json: &Json) -> Fault {
    Fault::at(JsonLdRefusal::Error { code }, json)
}

fn unsupported(feature: &str, json: &Json) -> Fault {
    Fault::at(
        JsonLdRefusal::Unsupported {
            feature: feature.to_owned(),
        },
        json,
    )
}

/// The keywords of JSON-LD 1.1, framing keywords included.
const KEYWORDS: &[&str] = &[
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

/// The keywords a node, value, list or set object of the subset may hold.
const BODY_KEYWORDS: &[&str] = &[
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

pub(crate) fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// Whether `s` has the form of a keyword: `@` followed by one or more ASCII
/// letters (RFC 5234 `"@"1*ALPHA`).
pub(crate) fn has_keyword_form(s: &str) -> bool {
    s.strip_prefix('@')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// Whether `s` is an IRI in the sense of the expansion checks: absolute,
/// with a scheme, and holding no character an IRI excludes.
fn is_iri(s: &str) -> bool {
    is_absolute_iri(s) && !s.chars().any(is_iri_excluded)
}

/// A term's container mapping, within the subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    None,
    List,
    Set,
}

/// A term definition (API Sec 4.1).
#[derive(Clone, Debug, PartialEq, Eq)]
struct TermDef {
    /// The IRI mapping; `None` for a term mapped to `null`.
    iri: Option<String>,
    reverse: bool,
    type_mapping: Option<String>,
    container: Container,
    /// The language mapping: `Some(None)` for an explicit `null`.
    language: Option<Option<String>>,
    prefix: bool,
}

/// An active context.
#[derive(Clone, Debug, Default)]
pub(crate) struct Context {
    base: Option<String>,
    original_base: Option<String>,
    vocab: Option<String>,
    language: Option<String>,
    terms: HashMap<String, TermDef>,
}

impl Context {
    /// The initial context of a document read with base `base`.
    pub(crate) fn initial(base: Option<String>) -> Self {
        Self {
            base: base.clone(),
            original_base: base,
            ..Self::default()
        }
    }
}

/// The local context and `defined` map of one context definition, while it
/// is processed.
struct Local<'j> {
    members: &'j [Member],
    defined: HashMap<String, bool>,
}

impl<'j> Local<'j> {
    fn member(&self, key: &str) -> Option<&'j Member> {
        self.members.iter().find(|m| m.key == key)
    }
}

/// IRI expansion (API Sec 5.2). `None` is the `null` a value of keyword
/// form expands to.
fn expand_iri(
    active: &mut Context,
    value: &str,
    document_relative: bool,
    vocab: bool,
    mut local: Option<&mut Local<'_>>,
) -> Res<Option<String>> {
    if is_keyword(value) {
        return Ok(Some(value.to_owned()));
    }
    if has_keyword_form(value) {
        return Ok(None);
    }
    if let Some(local) = local.as_deref_mut()
        && local.member(value).is_some()
        && local.defined.get(value) != Some(&true)
    {
        create_term_definition(active, local, value)?;
    }
    if let Some(def) = active.terms.get(value) {
        if let Some(iri) = &def.iri
            && is_keyword(iri)
        {
            return Ok(Some(iri.clone()));
        }
        if vocab {
            return Ok(def.iri.clone());
        }
    }
    if let Some(colon) = value.find(':').filter(|&i| i > 0) {
        let (prefix, suffix) = (&value[..colon], &value[colon + 1..]);
        if prefix == "_" || suffix.starts_with("//") {
            return Ok(Some(value.to_owned()));
        }
        if let Some(local) = local.as_deref_mut()
            && local.member(prefix).is_some()
            && local.defined.get(prefix) != Some(&true)
        {
            create_term_definition(active, local, prefix)?;
        }
        if let Some(def) = active.terms.get(prefix)
            && def.prefix
            && let Some(iri) = &def.iri
        {
            return Ok(Some(format!("{iri}{suffix}")));
        }
        if is_iri(value) {
            return Ok(Some(value.to_owned()));
        }
    }
    if vocab && let Some(mapping) = &active.vocab {
        return Ok(Some(format!("{mapping}{value}")));
    }
    if document_relative && let Some(base) = &active.base {
        return Ok(Some(resolve_iri(base, value)));
    }
    Ok(Some(value.to_owned()))
}

/// Whether `iri` ends with a `gen-delims` character of RFC 3986.
fn ends_with_gen_delim(iri: &str) -> bool {
    iri.ends_with([':', '/', '?', '#', '[', ']', '@'])
}

/// The container keywords of `value` when it is a container mapping of the
/// grammar API Sec 4.2 step 21.1 states, in written order.
fn container_keywords(value: &Json) -> Option<Vec<&str>> {
    const ALL: [&str; 7] = [
        "@graph",
        "@id",
        "@index",
        "@language",
        "@list",
        "@set",
        "@type",
    ];
    let keywords: Vec<&str> = match &value.kind {
        Kind::Str(s) => vec![s.as_str()],
        Kind::Array(items) => items.iter().map(Json::as_str).collect::<Option<_>>()?,
        _ => return None,
    };
    let has = |k: &str| keywords.contains(&k);
    let mut sorted = keywords.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != keywords.len() || keywords.iter().any(|k| !ALL.contains(k)) {
        return None;
    }
    let valid = match keywords.len() {
        1 => true,
        _ if has("@graph") && (has("@id") != has("@index")) => keywords
            .iter()
            .all(|k| matches!(*k, "@graph" | "@id" | "@index" | "@set")),
        _ => {
            has("@set")
                && keywords.iter().all(|k| {
                    matches!(
                        *k,
                        "@set" | "@index" | "@graph" | "@id" | "@type" | "@language"
                    )
                })
        }
    };
    valid.then_some(keywords)
}

/// Create Term Definition (API Sec 4.2) for `term` of `local`.
fn create_term_definition(active: &mut Context, local: &mut Local<'_>, term: &str) -> Res<()> {
    let Some(member) = local.member(term) else {
        return Ok(());
    };
    let value = &member.value;
    match local.defined.get(term) {
        Some(true) => return Ok(()),
        Some(false) => return Err(error(JsonLdErrorCode::CyclicIriMapping, value)),
        None => {}
    }
    if term.is_empty() {
        return Err(error(JsonLdErrorCode::InvalidTermDefinition, value));
    }
    local.defined.insert(term.to_owned(), false);
    if term == "@type" {
        type_term_definition(active, value)?;
    } else if is_keyword(term) {
        return Err(error(JsonLdErrorCode::KeywordRedefinition, value));
    } else if !has_keyword_form(term) {
        active.terms.remove(term);
        if let Some(def) = definition(active, local, term, value)? {
            active.terms.insert(term.to_owned(), def);
        }
    }
    local.defined.insert(term.to_owned(), true);
    Ok(())
}

/// The definition of the `@type` term, which may only make `@type` a set
/// (API Sec 4.2 step 4).
fn type_term_definition(active: &mut Context, value: &Json) -> Res<()> {
    let Kind::Object(entries) = &value.kind else {
        return Err(error(JsonLdErrorCode::KeywordRedefinition, value));
    };
    if let Some(protected) = entries.iter().find(|m| m.key == "@protected") {
        return Err(unsupported("@protected", &protected.value));
    }
    let only_set = !entries.is_empty()
        && entries
            .iter()
            .all(|m| m.key == "@container" && m.value.as_str() == Some("@set"));
    if !only_set {
        return Err(error(JsonLdErrorCode::KeywordRedefinition, value));
    }
    active.terms.insert(
        "@type".to_owned(),
        TermDef {
            iri: Some("@type".to_owned()),
            reverse: false,
            type_mapping: None,
            container: Container::Set,
            language: None,
            prefix: false,
        },
    );
    Ok(())
}

/// The entries of a term definition's value: a string or `null` is read as
/// a map holding only `@id`. Also whether the term is simple.
fn definition_entries(value: &Json) -> Res<(Vec<(&str, &Json)>, bool)> {
    match &value.kind {
        Kind::Null => Ok((vec![("@id", value)], false)),
        Kind::Str(_) => Ok((vec![("@id", value)], true)),
        Kind::Object(members) => Ok((
            members.iter().map(|m| (m.key.as_str(), &m.value)).collect(),
            false,
        )),
        _ => Err(error(JsonLdErrorCode::InvalidTermDefinition, value)),
    }
}

fn entry<'j>(entries: &[(&str, &'j Json)], key: &str) -> Option<&'j Json> {
    entries.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// The term definition of `term` (API Sec 4.2 steps 7 to 27), or `None`
/// when a value of keyword form makes the algorithm return early.
fn definition(
    active: &mut Context,
    local: &mut Local<'_>,
    term: &str,
    value: &Json,
) -> Res<Option<TermDef>> {
    let (entries, simple) = definition_entries(value)?;
    let mut def = TermDef {
        iri: None,
        reverse: false,
        type_mapping: None,
        container: Container::None,
        language: None,
        prefix: false,
    };
    if let Some(protected) = entry(&entries, "@protected") {
        return Err(unsupported("@protected", protected));
    }
    if let Some(ty) = entry(&entries, "@type") {
        let Some(text) = ty.as_str() else {
            return Err(error(JsonLdErrorCode::InvalidTypeMapping, ty));
        };
        let expanded = expand_iri(active, text, false, true, Some(local))?;
        match expanded.as_deref() {
            Some("@json") => return Err(unsupported("@json", ty)),
            Some("@id" | "@vocab" | "@none") => {}
            Some(iri) if is_iri(iri) => {}
            _ => return Err(error(JsonLdErrorCode::InvalidTypeMapping, ty)),
        }
        def.type_mapping = expanded;
    }
    if iri_mapping(active, local, term, value, &entries, simple, &mut def)?.is_none() {
        return Ok(None);
    }
    other_entries(term, value, &entries, &mut def)?;
    Ok(Some(def))
}

/// The IRI mapping of `def` (API Sec 4.2 steps 14 to 20), or `None` when a
/// value of keyword form makes the algorithm return early.
fn iri_mapping(
    active: &mut Context,
    local: &mut Local<'_>,
    term: &str,
    value: &Json,
    entries: &[(&str, &Json)],
    simple: bool,
    def: &mut TermDef,
) -> Res<Option<()>> {
    if let Some(reverse) = entry(entries, "@reverse") {
        if entry(entries, "@id").is_some() || entry(entries, "@nest").is_some() {
            return Err(error(JsonLdErrorCode::InvalidReverseProperty, value));
        }
        let Some(text) = reverse.as_str() else {
            return Err(error(JsonLdErrorCode::InvalidIriMapping, reverse));
        };
        if has_keyword_form(text) {
            return Ok(None);
        }
        let iri = expand_iri(active, text, false, true, Some(local))?;
        match iri.as_deref() {
            Some(iri) if is_iri(iri) || iri.starts_with("_:") => {}
            _ => return Err(error(JsonLdErrorCode::InvalidIriMapping, reverse)),
        }
        def.iri = iri;
        def.reverse = true;
    } else if let Some(id) = entry(entries, "@id").filter(|id| id.as_str() != Some(term)) {
        if matches!(id.kind, Kind::Null) {
            return Ok(Some(()));
        }
        let Some(text) = id.as_str() else {
            return Err(error(JsonLdErrorCode::InvalidIriMapping, id));
        };
        if !is_keyword(text) && has_keyword_form(text) {
            return Ok(None);
        }
        let iri = expand_iri(active, text, false, true, Some(local))?;
        match iri.as_deref() {
            Some("@context") => return Err(error(JsonLdErrorCode::InvalidKeywordAlias, id)),
            Some(iri) if is_keyword(iri) || is_iri(iri) || iri.starts_with("_:") => {}
            _ => return Err(error(JsonLdErrorCode::InvalidIriMapping, id)),
        }
        def.iri = iri;
        let inner_colon = term
            .char_indices()
            .any(|(i, c)| c == ':' && i > 0 && i + 1 < term.len());
        if inner_colon || term.contains('/') {
            local.defined.insert(term.to_owned(), true);
            let again = expand_iri(active, term, false, true, Some(local))?;
            if again != def.iri {
                return Err(error(JsonLdErrorCode::InvalidIriMapping, id));
            }
        }
        if !term.contains([':', '/'])
            && simple
            && def
                .iri
                .as_deref()
                .is_some_and(|iri| ends_with_gen_delim(iri) || iri.starts_with("_:"))
        {
            def.prefix = true;
        }
    } else if let Some(colon) = term.find(':').filter(|&i| i > 0) {
        let (prefix, suffix) = (&term[..colon], &term[colon + 1..]);
        if local.member(prefix).is_some() {
            create_term_definition(active, local, prefix)?;
        }
        def.iri = Some(
            match active.terms.get(prefix).and_then(|d| d.iri.as_ref()) {
                Some(iri) => format!("{iri}{suffix}"),
                None => term.to_owned(),
            },
        );
    } else if term.contains('/') {
        let iri = expand_iri(active, term, false, true, None)?;
        if !iri.as_deref().is_some_and(is_iri) {
            return Err(error(JsonLdErrorCode::InvalidIriMapping, value));
        }
        def.iri = iri;
    } else if let Some(mapping) = &active.vocab {
        def.iri = Some(format!("{mapping}{term}"));
    } else {
        return Err(error(JsonLdErrorCode::InvalidIriMapping, value));
    }
    Ok(Some(()))
}

/// The container mapping of `def` (API Sec 4.2 step 21).
fn container_entry(container: &Json, def: &mut TermDef) -> Res<()> {
    if def.reverse {
        return match &container.kind {
            Kind::Null => Ok(()),
            Kind::Str(s) if s == "@set" => {
                def.container = Container::Set;
                Ok(())
            }
            Kind::Str(s) if s == "@index" => Err(unsupported("@container @index", container)),
            _ => Err(error(JsonLdErrorCode::InvalidReverseProperty, container)),
        };
    }
    let Some(keywords) = container_keywords(container) else {
        return Err(error(JsonLdErrorCode::InvalidContainerMapping, container));
    };
    def.container = match keywords.as_slice() {
        ["@list"] => Container::List,
        ["@set"] => Container::Set,
        _ => {
            let first = keywords
                .iter()
                .find(|k| **k != "@set")
                .copied()
                .unwrap_or("@set");
            return Err(unsupported(&format!("@container {first}"), container));
        }
    };
    Ok(())
}

/// The entries of a term definition after its IRI mapping (API Sec 4.2
/// steps 21 to 27).
fn other_entries(
    term: &str,
    value: &Json,
    entries: &[(&str, &Json)],
    def: &mut TermDef,
) -> Res<()> {
    const ENTRIES: [&str; 11] = [
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
    if let Some(container) = entry(entries, "@container") {
        container_entry(container, def)?;
    }
    if let Some(index) = entry(entries, "@index") {
        return Err(unsupported("@index", index));
    }
    if let Some(context) = entry(entries, "@context") {
        return Err(unsupported("scoped context", context));
    }
    if entry(entries, "@type").is_none()
        && let Some(language) = entry(entries, "@language")
    {
        def.language = Some(match &language.kind {
            Kind::Null => None,
            Kind::Str(s) => Some(s.clone()),
            _ => return Err(error(JsonLdErrorCode::InvalidLanguageMapping, language)),
        });
    }
    if let Some(direction) = entry(entries, "@direction") {
        return Err(unsupported("@direction", direction));
    }
    if let Some(nest) = entry(entries, "@nest") {
        return Err(unsupported("@nest", nest));
    }
    if let Some(prefix) = entry(entries, "@prefix") {
        if term.contains([':', '/']) {
            return Err(error(JsonLdErrorCode::InvalidTermDefinition, value));
        }
        let Kind::Bool(flag) = prefix.kind else {
            return Err(error(JsonLdErrorCode::InvalidPrefixValue, prefix));
        };
        def.prefix = flag;
        if flag && def.iri.as_deref().is_some_and(is_keyword) {
            return Err(error(JsonLdErrorCode::InvalidTermDefinition, value));
        }
    }
    if entries.iter().any(|(k, _)| !ENTRIES.contains(k)) {
        return Err(error(JsonLdErrorCode::InvalidTermDefinition, value));
    }
    Ok(())
}

/// Context Processing (API Sec 4.1) of the local context `local` on
/// `active`.
pub(crate) fn process_context(active: &Context, local: &Json) -> Res<Context> {
    let mut result = active.clone();
    let items: Vec<&Json> = match &local.kind {
        Kind::Array(items) => items.iter().collect(),
        _ => vec![local],
    };
    for context in items {
        let members = match &context.kind {
            Kind::Null => {
                result = Context::initial(active.original_base.clone());
                continue;
            }
            Kind::Str(_) => return Err(unsupported("remote context", context)),
            Kind::Object(members) => members,
            _ => return Err(error(JsonLdErrorCode::InvalidLocalContext, context)),
        };
        let get = |key: &str| members.iter().find(|m| m.key == key).map(|m| &m.value);
        if let Some(version) = get("@version") {
            let is_one_one = matches!(&version.kind, Kind::Number(token)
                if token.parse::<f64>().is_ok_and(|v| (v - 1.1).abs() < f64::EPSILON));
            if !is_one_one {
                return Err(error(JsonLdErrorCode::InvalidVersionValue, version));
            }
        }
        for keyword in ["@import", "@direction", "@propagate", "@protected"] {
            if let Some(value) = get(keyword) {
                return Err(unsupported(keyword, value));
            }
        }
        if let Some(value) = get("@base") {
            match &value.kind {
                Kind::Null => result.base = None,
                Kind::Str(s) if is_iri(s) => result.base = Some(s.clone()),
                Kind::Str(s) if result.base.is_some() => {
                    let base = result.base.clone().unwrap_or_default();
                    result.base = Some(resolve_iri(&base, s));
                }
                _ => return Err(error(JsonLdErrorCode::InvalidBaseIri, value)),
            }
        }
        if let Some(value) = get("@vocab") {
            match &value.kind {
                Kind::Null => result.vocab = None,
                Kind::Str(s) => {
                    let expanded = expand_iri(&mut result, s, true, true, None)?;
                    match expanded {
                        Some(v) if is_iri(&v) || v.starts_with("_:") => result.vocab = Some(v),
                        _ => return Err(error(JsonLdErrorCode::InvalidVocabMapping, value)),
                    }
                }
                _ => return Err(error(JsonLdErrorCode::InvalidVocabMapping, value)),
            }
        }
        if let Some(value) = get("@language") {
            match &value.kind {
                Kind::Null => result.language = None,
                Kind::Str(s) => result.language = Some(s.clone()),
                _ => return Err(error(JsonLdErrorCode::InvalidDefaultLanguage, value)),
            }
        }
        let mut local = Local {
            members,
            defined: HashMap::new(),
        };
        for member in members {
            if matches!(
                member.key.as_str(),
                "@base"
                    | "@direction"
                    | "@import"
                    | "@language"
                    | "@propagate"
                    | "@protected"
                    | "@version"
                    | "@vocab"
            ) {
                continue;
            }
            create_term_definition(&mut result, &mut local, &member.key)?;
        }
    }
    Ok(result)
}

// ── The expanded document ─────────────────────────────────────────────

/// An item of the expanded document.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    Node(Box<NodeObject>),
    Value(ValueObject),
    List(ListObject),
}

/// A node object.
#[derive(Clone, Debug)]
pub(crate) struct NodeObject {
    /// The object's place.
    pub(crate) origin: Origin,
    /// `@id`, expanded.
    pub(crate) id: Option<(String, Origin)>,
    /// `@type`, each value expanded.
    pub(crate) types: Vec<(String, Origin)>,
    /// The properties in document order: the expanded IRI, the key's
    /// place, the values.
    pub(crate) properties: Vec<(String, Origin, Vec<Item>)>,
    /// The reverse properties, likewise.
    pub(crate) reverse: Vec<(String, Origin, Vec<Item>)>,
    /// `@graph`, when the object holds one.
    pub(crate) graph: Option<Vec<Item>>,
}

impl NodeObject {
    /// Whether the object holds nothing but `@graph`.
    fn only_graph(&self) -> bool {
        self.graph.is_some()
            && self.id.is_none()
            && self.types.is_empty()
            && self.properties.is_empty()
            && self.reverse.is_empty()
    }
}

/// The scalar of a value object.
#[derive(Clone, Debug)]
pub(crate) enum Scalar {
    Str(String),
    /// A number, by its token.
    Number(String),
    Bool(bool),
}

/// A value object.
#[derive(Clone, Debug)]
pub(crate) struct ValueObject {
    pub(crate) origin: Origin,
    pub(crate) value: Scalar,
    pub(crate) value_origin: Origin,
    pub(crate) datatype: Option<(String, Origin)>,
    pub(crate) language: Option<(String, Origin)>,
}

/// A list object.
#[derive(Clone, Debug)]
pub(crate) struct ListObject {
    pub(crate) origin: Origin,
    pub(crate) items: Vec<Item>,
}

/// What expanding one element gives: `null`, one item, or an array.
enum Ex {
    Null,
    One(Item),
    Many(Vec<Item>),
}

impl Ex {
    fn into_items(self) -> Vec<Item> {
        match self {
            Self::Null => Vec::new(),
            Self::One(item) => vec![item],
            Self::Many(items) => items,
        }
    }
}

/// The active property of an expansion step.
#[derive(Clone, Copy)]
enum Active<'a> {
    /// The top level.
    Top,
    /// `@graph`.
    Graph,
    /// `@reverse`.
    Reverse,
    /// A key.
    Key(&'a str),
}

impl Active<'_> {
    const fn free_floating(self) -> bool {
        matches!(self, Self::Top | Self::Graph)
    }
}

fn term<'c>(context: &'c Context, active: Active<'_>) -> Option<&'c TermDef> {
    match active {
        Active::Key(key) => context.terms.get(key),
        _ => None,
    }
}

/// Value Expansion (API Sec 5.3) of the scalar `value`.
fn expand_value(context: &mut Context, active: Active<'_>, value: &Json) -> Res<Item> {
    let def = term(context, active).cloned();
    let type_mapping = def.as_ref().and_then(|d| d.type_mapping.clone());
    if let Some(text) = value.as_str()
        && let Some(mode @ ("@id" | "@vocab")) = type_mapping.as_deref()
    {
        let id = expand_iri(context, text, true, mode == "@vocab", None)?.ok_or_else(|| {
            Fault::at(
                JsonLdRefusal::ReservedKeyword {
                    value: text.to_owned(),
                },
                value,
            )
        })?;
        return Ok(Item::Node(Box::new(NodeObject {
            origin: Origin::implicit(value),
            id: Some((id, Origin::token(value))),
            types: Vec::new(),
            properties: Vec::new(),
            reverse: Vec::new(),
            graph: None,
        })));
    }
    let scalar = match &value.kind {
        Kind::Str(s) => Scalar::Str(s.clone()),
        Kind::Number(token) => Scalar::Number(token.clone()),
        Kind::Bool(b) => Scalar::Bool(*b),
        _ => unreachable_scalar(value)?,
    };
    let mut result = ValueObject {
        origin: Origin::implicit(value),
        value: scalar,
        value_origin: Origin::token(value),
        datatype: None,
        language: None,
    };
    match type_mapping.as_deref() {
        Some("@id" | "@vocab" | "@none") | None => {
            if let Scalar::Str(_) = result.value {
                let language = match def.as_ref().and_then(|d| d.language.clone()) {
                    Some(mapping) => mapping,
                    None => context.language.clone(),
                };
                result.language = language.map(|tag| (tag, Origin::implicit(value)));
            }
        }
        Some(datatype) => result.datatype = Some((datatype.to_owned(), Origin::implicit(value))),
    }
    Ok(Item::Value(result))
}

/// A non-scalar passed where a scalar is required; the callers never do.
fn unreachable_scalar(value: &Json) -> Res<Scalar> {
    Err(error(JsonLdErrorCode::InvalidValueObjectValue, value))
}

/// The expansion of one document.
pub(crate) struct Expansion;

impl Expansion {
    /// Expand the document's top value with the initial context of `base`,
    /// and return the items of its default graph.
    pub(crate) fn document(document: &Json, base: Option<String>) -> Res<Vec<Item>> {
        let mut context = Context::initial(base);
        let expanded = expand(&mut context, Active::Top, document)?;
        if let (Kind::Object(_), Ex::One(Item::Node(node))) = (&document.kind, &expanded)
            && node.only_graph()
        {
            return Ok(node.graph.clone().unwrap_or_default());
        }
        Ok(expanded.into_items())
    }
}

/// The Expansion algorithm (API Sec 5.1) on `element`.
fn expand(context: &mut Context, active: Active<'_>, element: &Json) -> Res<Ex> {
    match &element.kind {
        Kind::Null => Ok(Ex::Null),
        Kind::Array(items) => {
            let list = term(context, active).is_some_and(|d| d.container == Container::List);
            let mut result = Vec::new();
            for item in items {
                let mut expanded = expand(context, active, item)?;
                if list && let Ex::Many(inner) = expanded {
                    expanded = Ex::One(Item::List(ListObject {
                        origin: Origin::implicit(item),
                        items: inner,
                    }));
                }
                result.extend(expanded.into_items());
            }
            Ok(Ex::Many(result))
        }
        Kind::Object(members) => {
            let mut inner = context.clone();
            expand_object(&mut inner, active, element, members)
        }
        _ => {
            if active.free_floating() {
                return Err(Fault::at(JsonLdRefusal::FreeFloating, element));
            }
            expand_value(context, active, element).map(Ex::One)
        }
    }
}

/// The parts of a map collected by Expansion step 13.
#[derive(Default)]
struct Collected {
    id: Option<(String, Origin)>,
    types: Vec<(String, Origin)>,
    /// Whether `@type` was written as a single string.
    type_single: bool,
    has_type: bool,
    graph: Option<Vec<Item>>,
    value: Option<Json>,
    language: Option<(String, Origin)>,
    list: Option<Vec<Item>>,
    set: Option<Ex>,
    properties: Vec<(String, Origin, Vec<Item>)>,
    reverse: Vec<(String, Origin, Vec<Item>)>,
}

impl Collected {
    fn node_parts(&self) -> bool {
        self.id.is_some()
            || self.graph.is_some()
            || !self.properties.is_empty()
            || !self.reverse.is_empty()
    }
}

fn reserved(value: &str, json: &Json) -> Fault {
    Fault::at(
        JsonLdRefusal::ReservedKeyword {
            value: value.to_owned(),
        },
        json,
    )
}

/// The entry of a map whose key expands to the keyword `property` (API
/// Sec 5.1 step 13.4).
fn keyword_entry(
    context: &mut Context,
    active: Active<'_>,
    property: &str,
    value: &Json,
    c: &mut Collected,
) -> Res<()> {
    match property {
        "@id" => {
            let Some(text) = value.as_str() else {
                return Err(error(JsonLdErrorCode::InvalidIdValue, value));
            };
            let id = expand_iri(context, text, true, false, None)?
                .ok_or_else(|| reserved(text, value))?;
            c.id = Some((id, Origin::token(value)));
        }
        "@type" => {
            let strings: Vec<&Json> = match &value.kind {
                Kind::Str(_) => vec![value],
                Kind::Array(items) if items.iter().all(|i| i.as_str().is_some()) => {
                    items.iter().collect()
                }
                _ => return Err(error(JsonLdErrorCode::InvalidTypeValue, value)),
            };
            c.type_single = !c.has_type && matches!(value.kind, Kind::Str(_));
            c.has_type = true;
            for item in strings {
                let text = item.as_str().unwrap_or_default();
                let ty = expand_iri(context, text, true, true, None)?
                    .ok_or_else(|| reserved(text, item))?;
                c.types.push((ty, Origin::token(item)));
            }
        }
        "@graph" => {
            let expanded = expand(context, Active::Graph, value)?;
            c.graph = Some(expanded.into_items());
        }
        "@value" => {
            if !(value.is_scalar() || matches!(value.kind, Kind::Null)) {
                return Err(error(JsonLdErrorCode::InvalidValueObjectValue, value));
            }
            c.value = Some(value.clone());
        }
        "@language" => {
            let Some(text) = value.as_str() else {
                return Err(error(JsonLdErrorCode::InvalidLanguageTaggedString, value));
            };
            c.language = Some((text.to_owned(), Origin::token(value)));
        }
        "@list" => {
            if active.free_floating() {
                return Err(Fault::at(JsonLdRefusal::FreeFloating, value));
            }
            let expanded = expand(context, active, value)?;
            c.list = Some(expanded.into_items());
        }
        "@set" => {
            c.set = Some(expand(context, active, value)?);
        }
        _ => {
            // `@reverse`, the last keyword the subset holds.
            if !matches!(value.kind, Kind::Object(_)) {
                return Err(error(JsonLdErrorCode::InvalidReverseValue, value));
            }
            let Ex::One(Item::Node(reversed)) = expand(context, Active::Reverse, value)? else {
                return Ok(());
            };
            let reversed = *reversed;
            c.properties.extend(reversed.reverse);
            for (iri, key, items) in reversed.properties {
                for item in &items {
                    if let Item::Value(ValueObject { origin, .. })
                    | Item::List(ListObject { origin, .. }) = item
                    {
                        return Err(Fault::at_origin(
                            JsonLdRefusal::Error {
                                code: JsonLdErrorCode::InvalidReversePropertyValue,
                            },
                            origin,
                        ));
                    }
                }
                c.reverse.push((iri, key, items));
            }
        }
    }
    Ok(())
}

/// Refuse a map with a key expanding to `@type` and a value expanding to
/// `@json`, a JSON literal, before any of its entries is expanded.
fn refuse_json_type(context: &mut Context, members: &[Member]) -> Res<()> {
    for member in members {
        if member.key == "@context"
            || expand_iri(context, &member.key, false, true, None)?.as_deref() != Some("@type")
        {
            continue;
        }
        let values: Vec<&Json> = match &member.value.kind {
            Kind::Array(items) => items.iter().collect(),
            _ => vec![&member.value],
        };
        for value in values {
            if let Some(text) = value.as_str()
                && expand_iri(context, text, true, true, None)?.as_deref() == Some("@json")
            {
                return Err(unsupported("@json", value));
            }
        }
    }
    Ok(())
}

/// Expansion of a map `element` (API Sec 5.1 steps 7 to 20).
fn expand_object(
    context: &mut Context,
    active: Active<'_>,
    element: &Json,
    members: &[Member],
) -> Res<Ex> {
    if let Some(local) = members.iter().find(|m| m.key == "@context") {
        *context = process_context(context, &local.value)?;
    }
    refuse_json_type(context, members)?;
    let mut c = Collected::default();
    let mut seen: Vec<String> = Vec::new();
    for member in members {
        if member.key == "@context" {
            continue;
        }
        let value = &member.value;
        let Some(property) = expand_iri(context, &member.key, false, true, None)? else {
            continue;
        };
        if !property.contains(':') && !is_keyword(&property) {
            continue;
        }
        if is_keyword(&property) {
            if matches!(active, Active::Reverse) {
                return Err(error(JsonLdErrorCode::InvalidReversePropertyMap, value));
            }
            if property != "@type" && seen.contains(&property) {
                return Err(error(JsonLdErrorCode::CollidingKeywords, value));
            }
            if !BODY_KEYWORDS.contains(&property.as_str()) {
                return Err(unsupported(&property, value));
            }
            seen.push(property.clone());
            keyword_entry(context, active, &property, value, &mut c)?;
            continue;
        }
        let def = context.terms.get(&member.key).cloned();
        let expanded = expand(context, Active::Key(&member.key), value)?;
        if matches!(expanded, Ex::Null) {
            continue;
        }
        let expanded = match expanded {
            Ex::One(Item::List(list)) => Ex::One(Item::List(list)),
            other if def.as_ref().is_some_and(|d| d.container == Container::List) => {
                Ex::One(Item::List(ListObject {
                    origin: Origin::implicit(value),
                    items: other.into_items(),
                }))
            }
            other => other,
        };
        let items = expanded.into_items();
        if def.as_ref().is_some_and(|d| d.reverse) {
            for item in &items {
                if let Item::Value(ValueObject { origin, .. })
                | Item::List(ListObject { origin, .. }) = item
                {
                    return Err(Fault::at_origin(
                        JsonLdRefusal::Error {
                            code: JsonLdErrorCode::InvalidReversePropertyValue,
                        },
                        origin,
                    ));
                }
            }
            c.reverse.push((property, Origin::key(member), items));
        } else {
            c.properties.push((property, Origin::key(member), items));
        }
    }
    finish(active, element, c)
}

/// Expansion steps 15 to 20: classify the collected map.
fn finish(active: Active<'_>, element: &Json, mut c: Collected) -> Res<Ex> {
    if let Some(value) = c.value.take() {
        if c.node_parts()
            || c.list.is_some()
            || c.set.is_some()
            || (c.has_type && c.language.is_some())
        {
            return Err(error(JsonLdErrorCode::InvalidValueObject, element));
        }
        let scalar = match &value.kind {
            Kind::Null => return Ok(Ex::Null),
            Kind::Str(s) => Scalar::Str(s.clone()),
            Kind::Number(token) => Scalar::Number(token.clone()),
            Kind::Bool(b) => Scalar::Bool(*b),
            _ => return Err(error(JsonLdErrorCode::InvalidValueObjectValue, &value)),
        };
        if c.language.is_some() && !matches!(scalar, Scalar::Str(_)) {
            return Err(error(JsonLdErrorCode::InvalidLanguageTaggedValue, element));
        }
        let datatype = if c.has_type {
            match c.types.as_slice() {
                [(iri, origin)] if c.type_single && is_iri(iri) && !iri.starts_with("_:") => {
                    Some((iri.clone(), origin.clone()))
                }
                _ => return Err(error(JsonLdErrorCode::InvalidTypedValue, element)),
            }
        } else {
            None
        };
        if active.free_floating() {
            return Err(Fault::at(JsonLdRefusal::FreeFloating, element));
        }
        return Ok(Ex::One(Item::Value(ValueObject {
            origin: Origin::implicit(element),
            value: scalar,
            value_origin: Origin::token(&value),
            datatype,
            language: c.language,
        })));
    }
    if c.list.is_some() || c.set.is_some() {
        if c.node_parts()
            || c.has_type
            || c.language.is_some()
            || (c.list.is_some() && c.set.is_some())
        {
            return Err(error(JsonLdErrorCode::InvalidSetOrListObject, element));
        }
        if let Some(items) = c.list {
            return Ok(Ex::One(Item::List(ListObject {
                origin: Origin::implicit(element),
                items,
            })));
        }
        return Ok(c.set.unwrap_or(Ex::Null));
    }
    if c.language.is_some() {
        if c.node_parts() || c.has_type {
            return Err(unsupported("@language in a node object", element));
        }
        return Ok(Ex::Null);
    }
    let node = NodeObject {
        origin: Origin::implicit(element),
        id: c.id,
        types: c.types,
        properties: c.properties,
        reverse: c.reverse,
        graph: c.graph,
    };
    if active.free_floating()
        && node.types.is_empty()
        && node.properties.is_empty()
        && node.reverse.is_empty()
        && node.graph.is_none()
    {
        return Ok(Ex::Null);
    }
    Ok(Ex::One(Item::Node(Box::new(node))))
}
