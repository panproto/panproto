//! Output codecs never serialize Rust's debug representation as data.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

const FILES: &[&str] = &[
    "src/cst_extract.rs",
    "src/byte_tabular.rs",
    "src/json_codec.rs",
    "src/xml_codec.rs",
    "src/tabular_codec.rs",
    "src/json_pathway.rs",
    "src/xml_pathway.rs",
    "src/toml_pathway.rs",
    "src/tabular_pathway.rs",
];

/// Each entry permits one `format!` call with exactly this whitespace-normalized
/// text in that file, and each constructs an error message rather than output.
/// An entry is consumed by one occurrence, so a repeated macro needs one entry
/// per occurrence; entries name the macro text, not its line, so unrelated
/// edits above an allowed site do not invalidate the list.
const ERROR_ONLY_ALLOWLIST: &[(&str, &str)] = &[
    (
        "src/cst_extract.rs",
        r#"format!( "invalid TOML basic string token at byte {:?}: {error}", cst_byte_offset(cst, name) )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "invalid XML namespace declaration {key:?}" )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "invalid XML namespace declaration {key:?}" )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("element has invalid QName {tag:?}")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "element QName {tag:?} uses an unbound or reserved namespace prefix" )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("attribute has invalid QName {key:?}")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("namespace declaration {key:?} is not a string")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("invalid XML namespace declaration {key:?}")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "attribute QName {key:?} uses an unbound or reserved namespace prefix" )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("element has invalid QName {tag:?}")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!("attribute has invalid QName {key:?}")"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "XML element {element_vertex:?} has no start tag for xsi:nil" )"#,
    ),
    (
        "src/cst_extract.rs",
        r#"format!( "data row {} at byte {:?} has {} fields but {} header fields are declared", row_idx + 1, cst_byte_offset(cst, row_name), fields.len(), headers.len() )"#,
    ),
    (
        "src/json_pathway.rs",
        r#"format!("root vertex {root_vertex:?} does not exist")"#,
    ),
    (
        "src/json_pathway.rs",
        r#"format!("duplicate emitted JSON object key {key:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("XML element node {node_id} has invalid QName {tag:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("XML element QName {tag:?} uses an unbound namespace prefix")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!( "XML tag {tag:?} would not reparse to node {node_id} at schema anchor '{}'", node.anchor )"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("XML attribute has invalid QName {key:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("invalid XML namespace declaration {key:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("invalid XML namespace declaration {key:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("XML attribute QName {key:?} uses an unbound namespace prefix")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("XML element {tag:?} uses an invalid or unbound namespace prefix")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!("invalid XML namespace declaration {key:?}")"#,
    ),
    (
        "src/xml_pathway.rs",
        r#"format!( "XML attribute {key:?} uses unbound namespace prefix {:?}", String::from_utf8_lossy(&prefix) )"#,
    ),
    (
        "src/toml_pathway.rs",
        r#"format!("duplicate emitted TOML table key {key:?}")"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "{format} header {column:?} contains a reserved row or field delimiter" )"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "CoNLL-U can carry only sentence and token tables, not {:?}", tables.iter().map(AsRef::as_ref).collect::<Vec<_>>() )"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!("token row {} has unsupported column {key:?}", row_id.0)"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "sentence row {} has metadata key {key:?} that reparses differently", row_id.0 )"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "sentence row {} has metadata value for {key:?} whose surrounding whitespace would be trimmed", row_id.0 )"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "sentence sent_idx values {declared:?} do not exactly match token sent_idx values {used:?}" )"#,
    ),
    (
        "src/tabular_pathway.rs",
        r#"format!( "canonical emission selects table '{table_vertex}', but the non-empty table set is {:?}", tables.iter().map(AsRef::as_ref).collect::<Vec<_>>() )"#,
    ),
];

fn format_macro_spans(text: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut search_from = 0usize;
    while let Some(relative) = text[search_from..].find("format!") {
        let start = search_from + relative;
        let Some(open_relative) = text[start..].find('(') else {
            break;
        };
        let open = start + open_relative;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = text.len();
        for (offset, character) in text[open..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    in_string = false;
                }
                continue;
            }
            match character {
                '"' => in_string = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset + character.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
        spans.push((start, &text[start..end]));
        search_from = end;
    }
    spans
}

/// Whether `span` holds a placeholder whose format spec selects `Debug`.
///
/// A placeholder is `{argument:spec}` with no brace inside; `{{` and `}}` are
/// escaped braces, not placeholders. Under `std::fmt`'s grammar the spec ends
/// in its type, and the `Debug` types are `?`, `x?` and `X?`, so a placeholder
/// selects `Debug` exactly when its spec ends in `?`, whatever fill, alignment,
/// sign, `#`, width or precision precede it.
fn has_debug_spec(span: &str) -> bool {
    let bytes = span.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => index += 2,
            b'}' if bytes.get(index + 1) == Some(&b'}') => index += 2,
            b'{' => {
                let Some(length) = span[index + 1..].find(['{', '}']) else {
                    return false;
                };
                let inner = &span[index + 1..index + 1 + length];
                if bytes[index + 1 + length] == b'}'
                    && inner
                        .split_once(':')
                        .is_some_and(|(_, spec)| spec.ends_with('?'))
                {
                    return true;
                }
                index += 1 + length;
            }
            _ => index += 1,
        }
    }
    false
}

/// Macro texts whose format string selects `Debug` in some placeholder.
const DEBUG_SPEC_SPANS: &[&str] = &[
    r#"format!("{:?}", x)"#,
    r#"format!("{x:?}")"#,
    r#"format!("{x:#?}")"#,
    r#"format!("{x:x?}")"#,
    r#"format!("{x:X?}")"#,
    r#"format!("{x:#x?}")"#,
    r#"format!("{x:>10?}")"#,
    r#"format!("{x:.3?}")"#,
    r#"format!("{0:width$?}", x, width = 4)"#,
    r#"format!("{{{x:#?}}}")"#,
];

/// Macro texts whose placeholders select no `Debug`, escaped braces included.
const NON_DEBUG_SPEC_SPANS: &[&str] = &[
    r#"format!("{x}")"#,
    r#"format!("{x:>10}")"#,
    r#"format!("{:x}", x)"#,
    r#"format!("{{x:?}}")"#,
    r#"format!("{{:?}}{x}")"#,
    r#"format!("what? {x}")"#,
];

#[test]
fn debug_spec_detection_covers_every_debug_format_spec() {
    for debug in DEBUG_SPEC_SPANS {
        assert!(has_debug_spec(debug), "{debug} selects Debug");
    }
    for display in NON_DEBUG_SPEC_SPANS {
        assert!(!has_debug_spec(display), "{display} selects no Debug");
    }
}

#[test]
fn no_debug_formatting_reaches_emitted_output() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    let mut allowlist_hits = vec![false; ERROR_ONLY_ALLOWLIST.len()];

    for relative in FILES {
        let text =
            std::fs::read_to_string(manifest.join(relative)).expect("read fixed source file");
        for (start, span) in format_macro_spans(&text) {
            if !has_debug_spec(span) {
                continue;
            }
            let line = text[..start].bytes().filter(|byte| *byte == b'\n').count() + 1;
            let normalized = span.split_whitespace().collect::<Vec<_>>().join(" ");
            let allowlist_index = ERROR_ONLY_ALLOWLIST.iter().enumerate().position(
                |(index, (file, allowed_span))| {
                    !allowlist_hits[index] && file == relative && *allowed_span == normalized
                },
            );
            if let Some(index) = allowlist_index {
                allowlist_hits[index] = true;
            } else {
                violations.push(format!("{relative}:{line}: {span}"));
            }
        }
    }

    for (index, hit) in allowlist_hits.into_iter().enumerate() {
        if !hit {
            let (file, span) = ERROR_ONLY_ALLOWLIST[index];
            violations.push(format!(
                "{file}: stale or missing error-only allowlist entry: {span}"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Rust debug formatting reached a codec output path:\n{}",
        violations.join("\n")
    );
}

/// The shared value path's library code, scanned for panicking macros.
const LIBRARY_FILES: &[&str] = &[
    "src/cst_extract.rs",
    "src/byte_tabular.rs",
    "src/json_codec.rs",
    "src/xml_codec.rs",
    "src/tabular_codec.rs",
    "src/unified_codec.rs",
    "src/json_pathway.rs",
    "src/xml_pathway.rs",
    "src/toml_pathway.rs",
    "src/tabular_pathway.rs",
    "src/error.rs",
];

#[test]
fn codec_library_code_has_no_panicking_macros() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    for relative in LIBRARY_FILES {
        let text =
            std::fs::read_to_string(manifest.join(relative)).expect("read fixed source file");
        // The unit-test module is the last item of each file.
        let library = text
            .find("\nmod tests {")
            .map_or(text.as_str(), |end| &text[..end]);
        for (index, line) in library.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for name in ["unreachable!", "panic!", "todo!", "unimplemented!"] {
                if code.contains(name) {
                    violations.push(format!("{relative}:{}: {}", index + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "panicking macros in codec library code:\n{}",
        violations.join("\n")
    );
}

/// No documentation or manifest names `simd-json` as a parser in use: the JSON
/// pathway reads numerals exactly with its own lexer, and nothing depends on it.
#[test]
fn json_pathway_docs_and_manifests_name_no_simd_json_parser() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = [
        crate_dir.join("src/lib.rs"),
        crate_dir.join("Cargo.toml"),
        crate_dir.join("../../Cargo.toml"),
    ];
    let mut stale = Vec::new();
    for path in &sources {
        let text = std::fs::read_to_string(path).unwrap();
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("simd-json")
                || lower.contains("simd_json")
                || lower.contains("simd-accelerated json")
                || lower.contains("simd-accelerated parsing")
            {
                stale.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "stale simd-json claims:\n{}",
        stale.join("\n")
    );
}
