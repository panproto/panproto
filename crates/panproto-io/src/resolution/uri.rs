//! URI-reference resolution shared by resolution providers.
//!
//! These functions implement RFC 3986 section 5.2 without additional
//! normalization. In particular, they preserve the spelling of schemes,
//! authorities, percent escapes, repeated slashes, queries and fragments;
//! only the algorithm's dot-segment removal changes a path.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Components<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn parse(text: &str) -> Components<'_> {
    let (without_fragment, fragment) = text
        .split_once('#')
        .map_or((text, None), |(head, tail)| (head, Some(tail)));
    let (without_query, query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, None), |(head, tail)| (head, Some(tail)));
    let scheme_end = without_query.find(':').filter(|at| {
        let candidate = &without_query[..*at];
        !candidate.is_empty()
            && candidate.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_alphabetic()
                    || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
            })
    });
    let (scheme, rest) = scheme_end.map_or((None, without_query), |at| {
        (Some(&without_query[..at]), &without_query[at + 1..])
    });
    let (authority, path) = rest.strip_prefix("//").map_or((None, rest), |after| {
        after.find('/').map_or((Some(after), ""), |slash| {
            (Some(&after[..slash]), &after[slash..])
        })
    });
    Components {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

fn remove_last_segment(output: &mut String) {
    output.truncate(output.rfind('/').unwrap_or(0));
}

fn remove_dot_segments(path: &str) -> String {
    let mut input = path;
    let mut output = String::new();
    while !input.is_empty() {
        if let Some(rest) = input
            .strip_prefix("../")
            .or_else(|| input.strip_prefix("./"))
        {
            input = rest;
        } else if input.starts_with("/./") {
            input = &input[2..];
        } else if input == "/." {
            input = "/";
        } else if input.starts_with("/../") {
            input = &input[3..];
            remove_last_segment(&mut output);
        } else if input == "/.." {
            input = "/";
            remove_last_segment(&mut output);
        } else if matches!(input, "." | "..") {
            input = "";
        } else {
            let end = input.strip_prefix('/').map_or_else(
                || input.find('/').unwrap_or(input.len()),
                |after_slash| after_slash.find('/').map_or(input.len(), |at| at + 1),
            );
            output.push_str(&input[..end]);
            input = &input[end..];
        }
    }
    output
}

fn merge(base: Components<'_>, reference: &str) -> String {
    if base.authority.is_some() && base.path.is_empty() {
        return format!("/{reference}");
    }
    base.path.rfind('/').map_or_else(
        || reference.to_owned(),
        |slash| format!("{}{reference}", &base.path[..=slash]),
    )
}

/// Resolve one URI reference by RFC 3986 section 5.2.
///
/// `None` means that `base` has no scheme and therefore cannot be an RFC
/// 3986 base URI. No case, percent-escape or repeated-slash normalization is
/// performed.
#[must_use]
pub fn resolve_uri_reference(base: &str, reference: &str) -> Option<String> {
    let base = parse(base);
    let reference = parse(reference);
    let base_scheme = base.scheme?;
    let (scheme, authority, path, query) = reference.scheme.map_or_else(
        || {
            if reference.authority.is_some() {
                (
                    base_scheme,
                    reference.authority,
                    remove_dot_segments(reference.path),
                    reference.query,
                )
            } else if reference.path.is_empty() {
                (
                    base_scheme,
                    base.authority,
                    base.path.to_owned(),
                    reference.query.or(base.query),
                )
            } else {
                let path = if reference.path.starts_with('/') {
                    remove_dot_segments(reference.path)
                } else {
                    remove_dot_segments(&merge(base, reference.path))
                };
                (base_scheme, base.authority, path, reference.query)
            }
        },
        |scheme| {
            (
                scheme,
                reference.authority,
                remove_dot_segments(reference.path),
                reference.query,
            )
        },
    );
    let mut result = format!("{scheme}:");
    if let Some(authority) = authority {
        result.push_str("//");
        result.push_str(authority);
    }
    result.push_str(&path);
    if let Some(query) = query {
        result.push('?');
        result.push_str(query);
    }
    if let Some(fragment) = reference.fragment {
        result.push('#');
        result.push_str(fragment);
    }
    Some(result)
}

/// Resolve `reference` against an optional base.
///
/// A reference with a scheme is resolved without a base (RFC 3986 section
/// 5.2.2 then reads only the reference); a relative reference with no base
/// is `None`, never resolved against a guessed base.
#[must_use]
pub fn resolve_uri_against(base: Option<&str>, reference: &str) -> Option<String> {
    if parse(reference).scheme.is_some() {
        resolve_uri_reference(reference, reference)
    } else {
        resolve_uri_reference(base?, reference)
    }
}

/// One positional directive of a URI environment, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UriDirective {
    /// A base directive (Turtle's `@base`).
    Base(String),
    /// A prefix binding (Turtle's `@prefix label: <iri>`).
    Prefix {
        /// The bound label.
        label: String,
        /// The written IRI, resolved against the base then in scope.
        iri: String,
    },
}

/// The base and prefix bindings in scope at one occurrence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UriEnvironment {
    /// The base in scope, if any.
    pub base: Option<String>,
    /// Each bound label's resolved IRI.
    pub prefixes: std::collections::BTreeMap<String, String>,
}

impl UriEnvironment {
    /// Resolve one written form: `a` is `rdf:type`, a prefixed name under a
    /// bound label expands to the label's IRI followed by its local part,
    /// and any other form is an IRI reference resolved against the base.
    #[must_use]
    pub fn resolve(&self, written: &str) -> Option<String> {
        if written == "a" {
            return Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned());
        }
        if let Some((label, local)) = written.split_once(':')
            && let Some(namespace) = self.prefixes.get(label)
        {
            return Some(format!("{namespace}{local}"));
        }
        resolve_uri_against(self.base.as_deref(), written)
    }
}

/// Fold the positional directives preceding one occurrence.
///
/// Starting from no base and no bindings, a base directive sets the base to
/// its IRI resolved against the previous base (no base when that resolution
/// is undefined), and a prefix directive binds its label to its IRI resolved
/// against the base then in scope, replacing an earlier binding; a binding
/// whose IRI is undefined leaves the label unbound.
#[must_use]
pub fn fold_uri_environment(directives: &[UriDirective]) -> UriEnvironment {
    directives
        .iter()
        .fold(UriEnvironment::default(), |mut environment, directive| {
            match directive {
                UriDirective::Base(iri) => {
                    environment.base = resolve_uri_against(environment.base.as_deref(), iri);
                }
                UriDirective::Prefix { label, iri } => {
                    match resolve_uri_against(environment.base.as_deref(), iri) {
                        Some(resolved) => {
                            environment.prefixes.insert(label.clone(), resolved);
                        }
                        None => {
                            environment.prefixes.remove(label);
                        }
                    }
                }
            }
            environment
        })
}

/// Resolve against the base inherited along an ancestor-or-self chain.
///
/// `ancestor_bases` lists the chain root first and the holder last. As XML
/// Base section 4.2 states, the base is the left fold of the present values,
/// each resolved against the base so far, from no base; a relative value
/// met with no base gives no base. The reference is then resolved against
/// the folded base.
#[must_use]
pub fn resolve_inherited_uri_base(
    ancestor_bases: &[Option<String>],
    reference: &str,
) -> Option<String> {
    let base = ancestor_bases
        .iter()
        .flatten()
        .fold(None, |base: Option<String>, value| {
            resolve_uri_against(base.as_deref(), value)
        });
    resolve_uri_against(base.as_deref(), reference)
}
