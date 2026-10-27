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

/// Fold the positional base directives preceding one occurrence.
///
/// An absolute directive replaces the environment; a relative directive is
/// resolved against the environment immediately before it. A relative first
/// directive, or a later unresolvable directive, yields `None`.
#[must_use]
pub fn fold_uri_environment(directives: &[String]) -> Option<String> {
    directives.iter().try_fold(None, |base, directive| {
        if parse(directive).scheme.is_some() {
            Some(Some(directive.clone()))
        } else {
            Some(Some(resolve_uri_reference(base.as_deref()?, directive)?))
        }
    })?
}

/// Resolve against the nearest present inherited base.
#[must_use]
pub fn resolve_inherited_uri_base(
    ancestor_bases: &[Option<String>],
    reference: &str,
) -> Option<String> {
    ancestor_bases
        .iter()
        .rev()
        .flatten()
        .next()
        .and_then(|base| resolve_uri_reference(base, reference))
}
