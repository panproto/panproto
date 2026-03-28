//! Cross-file import resolution.
//!
//! After coproduct construction, the resolve pass walks the project schema
//! looking for import-like vertices and matches them against export-like
//! vertices in other file schemas. Resolved imports become cross-file edges.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

/// Configuration for import resolution.
#[derive(Debug, Clone)]
pub struct ResolveConfig {
    /// The list of rules to use for matching imports to exports.
    pub rules: Vec<ResolveRule>,
}

/// A rule for matching imports to exports in a specific protocol.
#[derive(Debug, Clone)]
pub struct ResolveRule {
    /// Protocol this rule applies to (e.g., "typescript", "rust", "python").
    pub protocol: String,
    /// Vertex kind that represents an import (e.g., `import_statement`).
    pub import_vertex_kind: String,
    /// Vertex kind that represents an export (e.g., `export_statement`, `function_item`).
    pub export_vertex_kind: String,
    /// Constraint sort on import vertices that holds the source path/module (e.g., "literal-value").
    pub source_constraint_sort: String,
    /// Edge kind to create for resolved imports.
    pub resolved_edge_kind: String,
}

/// A resolved import link.
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    /// The import vertex in the coproduct schema (prefixed with the source file path).
    pub import_vertex: Name,
    /// The export vertex in the coproduct schema (prefixed with the target file path).
    pub export_vertex: Name,
    /// The file containing the import statement.
    pub source_file: PathBuf,
    /// The file containing the export that was resolved to.
    pub target_file: PathBuf,
}

/// Default resolution rules for common languages.
#[must_use]
pub fn default_rules() -> Vec<ResolveRule> {
    let mut rules = Vec::new();

    // TypeScript/JavaScript: import statements link to exported declarations.
    for export_kind in [
        "export_statement",
        "function_declaration",
        "class_declaration",
        "lexical_declaration",
        "type_alias_declaration",
        "interface_declaration",
    ] {
        rules.push(ResolveRule {
            protocol: "typescript".to_owned(),
            import_vertex_kind: "import_statement".to_owned(),
            export_vertex_kind: export_kind.to_owned(),
            source_constraint_sort: "literal-value".to_owned(),
            resolved_edge_kind: "imports".to_owned(),
        });
    }
    // Also handle JS (same AST kinds as TypeScript).
    for export_kind in [
        "export_statement",
        "function_declaration",
        "class_declaration",
    ] {
        rules.push(ResolveRule {
            protocol: "javascript".to_owned(),
            import_vertex_kind: "import_statement".to_owned(),
            export_vertex_kind: export_kind.to_owned(),
            source_constraint_sort: "literal-value".to_owned(),
            resolved_edge_kind: "imports".to_owned(),
        });
    }

    // Python: `import_from_statement` links to top-level definitions.
    for export_kind in [
        "function_definition",
        "class_definition",
        "expression_statement",
    ] {
        rules.push(ResolveRule {
            protocol: "python".to_owned(),
            import_vertex_kind: "import_from_statement".to_owned(),
            export_vertex_kind: export_kind.to_owned(),
            source_constraint_sort: "literal-value".to_owned(),
            resolved_edge_kind: "imports".to_owned(),
        });
    }

    // Rust: `use_declaration` links to public items.
    for export_kind in [
        "function_item",
        "struct_item",
        "enum_item",
        "trait_item",
        "mod_item",
        "type_item",
        "const_item",
        "static_item",
        "macro_definition",
    ] {
        rules.push(ResolveRule {
            protocol: "rust".to_owned(),
            import_vertex_kind: "use_declaration".to_owned(),
            export_vertex_kind: export_kind.to_owned(),
            source_constraint_sort: "literal-value".to_owned(),
            resolved_edge_kind: "imports".to_owned(),
        });
    }

    // Go: import declarations link to exported declarations.
    for export_kind in [
        "function_declaration",
        "type_declaration",
        "const_declaration",
        "var_declaration",
    ] {
        rules.push(ResolveRule {
            protocol: "go".to_owned(),
            import_vertex_kind: "import_declaration".to_owned(),
            export_vertex_kind: export_kind.to_owned(),
            source_constraint_sort: "literal-value".to_owned(),
            resolved_edge_kind: "imports".to_owned(),
        });
    }

    rules
}

/// Normalized import source path: strips quotes and common prefixes, resolves
/// relative segments against the importing file's directory.
fn normalize_import_path(raw: &str, importing_file: &Path) -> Option<PathBuf> {
    // Strip surrounding single or double quotes.
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed);

    if unquoted.is_empty() {
        return None;
    }

    // If the path starts with `.` it is relative to the importing file's directory.
    let candidate = Path::new(unquoted);
    let resolved = if unquoted.starts_with('.') {
        let base_dir = importing_file.parent().unwrap_or_else(|| Path::new(""));
        base_dir.join(candidate)
    } else {
        candidate.to_path_buf()
    };

    // Collapse simple `.` and `..` components without touching the filesystem.
    Some(normalize_components(&resolved))
}

/// Collapse `.` and `..` components in a path (purely lexical, no I/O).
fn normalize_components(path: &Path) -> PathBuf {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str()),
        }
    }
    parts.iter().collect()
}

/// Try common file extensions to find a matching file in the file map.
fn resolve_file_path<S: ::std::hash::BuildHasher>(
    base: &Path,
    file_map: &HashMap<PathBuf, Vec<Name>, S>,
) -> Option<PathBuf> {
    // Exact match first.
    if file_map.contains_key(base) {
        return Some(base.to_path_buf());
    }

    // Try common extensions.
    let extensions = [
        "ts", "tsx", "js", "jsx", "py", "rs", "go", "mts", "mjs", "cts", "cjs",
    ];
    for ext in &extensions {
        let mut with_ext = base.to_path_buf();
        let current_name = with_ext
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        with_ext.set_file_name(format!("{current_name}.{ext}"));
        if file_map.contains_key(&with_ext) {
            return Some(with_ext);
        }
    }

    // Try index files (e.g., `./utils` -> `./utils/index.ts`).
    for ext in &extensions {
        let index_path = base.join(format!("index.{ext}"));
        if file_map.contains_key(&index_path) {
            return Some(index_path);
        }
    }

    None
}

/// Extract the file path prefix from a prefixed vertex name.
///
/// Vertex names in the coproduct schema have the form `"path/to/file.ts::vertex_id"`.
/// Returns the file path portion.
#[must_use]
pub fn file_prefix(vertex_name: &Name) -> Option<PathBuf> {
    let s: &str = vertex_name.as_ref();
    s.find("::").map(|idx| PathBuf::from(&s[..idx]))
}

/// Build an index of export vertices grouped by file path.
///
/// For each file in the project, collects vertices whose kind matches an
/// export vertex kind from any applicable rule.
fn build_export_index<S: ::std::hash::BuildHasher>(
    schema: &Schema,
    file_map: &HashMap<PathBuf, Vec<Name>, S>,
    protocol_map: &HashMap<PathBuf, String, S>,
    rules: &[ResolveRule],
) -> HashMap<PathBuf, Vec<Name>> {
    let mut export_index: HashMap<PathBuf, Vec<Name>> = HashMap::new();

    for (file_path, vertices) in file_map {
        let Some(protocol) = protocol_map.get(file_path) else {
            continue;
        };

        // Collect export vertex kinds that apply to this protocol.
        let export_kinds: Vec<&str> = rules
            .iter()
            .filter(|r| r.protocol == *protocol)
            .map(|r| r.export_vertex_kind.as_str())
            .collect();

        if export_kinds.is_empty() {
            continue;
        }

        for vname in vertices {
            if let Some(vertex) = schema.vertices.get(vname) {
                let kind_str: &str = vertex.kind.as_ref();
                if export_kinds.contains(&kind_str) {
                    export_index
                        .entry(file_path.clone())
                        .or_default()
                        .push(vname.clone());
                }
            }
        }
    }

    export_index
}

/// Insert an edge into a schema, updating both the edge map and all precomputed indices.
///
/// Both `edge.src` and `edge.tgt` must refer to vertices that exist in
/// `schema.vertices`. Violation is a bug in the caller.
fn insert_edge(schema: &mut Schema, edge: Edge) {
    debug_assert!(
        schema.vertices.contains_key(&edge.src),
        "insert_edge: source vertex {:?} does not exist",
        edge.src
    );
    debug_assert!(
        schema.vertices.contains_key(&edge.tgt),
        "insert_edge: target vertex {:?} does not exist",
        edge.tgt
    );

    // Skip duplicate edges.
    if schema.edges.contains_key(&edge) {
        return;
    }

    let kind = edge.kind.clone();
    let src = edge.src.clone();
    let tgt = edge.tgt.clone();

    schema.edges.insert(edge.clone(), kind);

    schema
        .outgoing
        .entry(src.clone())
        .or_default()
        .push(edge.clone());

    schema
        .incoming
        .entry(tgt.clone())
        .or_default()
        .push(edge.clone());

    schema.between.entry((src, tgt)).or_default().push(edge);
}

/// Resolve cross-file imports in the coproduct schema.
///
/// Walks all vertices in the schema, identifies import vertices matching a
/// rule, extracts the source path from the vertex's constraints, and creates
/// edges linking each import vertex to matching export vertices in the target
/// file.
///
/// Search a vertex and all its descendants (via outgoing edges) for a constraint
/// with the given sort, returning the first match's value.
///
/// Tree-sitter attaches `literal-value` constraints to leaf nodes. Import vertices
/// like `import_statement` are parent nodes, so we must walk their children to find
/// the source path string.
fn find_descendant_constraint(schema: &Schema, start: &Name, sort: &str) -> Option<String> {
    // Check the start vertex itself.
    if let Some(constraints) = schema.constraints.get(start) {
        for c in constraints {
            if c.sort.as_ref() == sort {
                return Some(c.value.clone());
            }
        }
    }

    // BFS over outgoing edges to find a descendant with the constraint.
    let mut queue = std::collections::VecDeque::new();
    let mut visited = std::collections::HashSet::new();
    queue.push_back(start.clone());
    visited.insert(start.clone());

    while let Some(current) = queue.pop_front() {
        if let Some(outgoing) = schema.outgoing.get(&current) {
            for edge in outgoing {
                if visited.contains(&edge.tgt) {
                    continue;
                }
                visited.insert(edge.tgt.clone());

                if let Some(constraints) = schema.constraints.get(&edge.tgt) {
                    for c in constraints {
                        if c.sort.as_ref() == sort {
                            return Some(c.value.clone());
                        }
                    }
                }

                queue.push_back(edge.tgt.clone());
            }
        }
    }

    None
}

/// Resolution is best-effort: unresolved imports (e.g., external packages not
/// present in the project) are silently skipped.
pub fn resolve_imports<S: ::std::hash::BuildHasher>(
    schema: &mut Schema,
    file_map: &HashMap<PathBuf, Vec<Name>, S>,
    protocol_map: &HashMap<PathBuf, String, S>,
    rules: &[ResolveRule],
) -> Vec<ResolvedImport> {
    let mut resolved = Vec::new();

    // Build export index: file_path -> [export vertex names].
    let export_index = build_export_index(schema, file_map, protocol_map, rules);

    // Collect import vertices to process. We gather them first to avoid
    // borrowing `schema` mutably while iterating.
    let mut import_candidates: Vec<(Name, PathBuf, String, String)> = Vec::new();

    for (file_path, vertices) in file_map {
        let Some(protocol) = protocol_map.get(file_path) else {
            continue;
        };

        // Collect rules that apply to this protocol.
        let applicable_rules: Vec<&ResolveRule> =
            rules.iter().filter(|r| r.protocol == *protocol).collect();

        if applicable_rules.is_empty() {
            continue;
        }

        for vname in vertices {
            let Some(vertex) = schema.vertices.get(vname) else {
                continue;
            };

            let kind_str: &str = vertex.kind.as_ref();

            for rule in &applicable_rules {
                if kind_str != rule.import_vertex_kind {
                    continue;
                }

                // Look up the source path constraint. Tree-sitter attaches
                // `literal-value` to leaf nodes, not parent nodes like
                // `import_statement`. Walk the import vertex and all its
                // descendants (reachable via outgoing edges) to find the
                // constraint.
                if let Some(source_value) =
                    find_descendant_constraint(schema, vname, &rule.source_constraint_sort)
                {
                    import_candidates.push((
                        vname.clone(),
                        file_path.clone(),
                        source_value,
                        rule.resolved_edge_kind.clone(),
                    ));
                }
            }
        }
    }

    // Deduplicate import candidates: multiple rules may match the same import vertex
    // with the same source value (e.g., 6 TypeScript rules all match import_statement).
    let mut seen = std::collections::HashSet::new();
    import_candidates
        .retain(|(vertex, _, source, _)| seen.insert((vertex.clone(), source.clone())));

    // Now resolve each import candidate.
    for (import_vertex, source_file, raw_source, edge_kind) in import_candidates {
        let Some(target_path) = normalize_import_path(&raw_source, &source_file) else {
            continue;
        };

        let Some(resolved_file) = resolve_file_path(&target_path, file_map) else {
            continue;
        };

        let Some(export_vertices) = export_index.get(&resolved_file) else {
            continue;
        };

        for export_vertex in export_vertices {
            let edge = Edge {
                src: import_vertex.clone(),
                tgt: export_vertex.clone(),
                kind: Name::from(edge_kind.as_str()),
                name: None,
            };

            insert_edge(schema, edge);

            resolved.push(ResolvedImport {
                import_vertex: import_vertex.clone(),
                export_vertex: export_vertex.clone(),
                source_file: source_file.clone(),
                target_file: resolved_file.clone(),
            });
        }
    }

    resolved
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn normalize_import_path_relative() {
        let importing = Path::new("src/components/App.ts");
        let result = normalize_import_path("'./utils'", importing).unwrap();
        assert_eq!(result, PathBuf::from("src/components/utils"));
    }

    #[test]
    fn normalize_import_path_parent_relative() {
        let importing = Path::new("src/components/App.ts");
        let result = normalize_import_path("\"../lib/helpers\"", importing).unwrap();
        assert_eq!(result, PathBuf::from("src/lib/helpers"));
    }

    #[test]
    fn normalize_import_path_bare_module() {
        let importing = Path::new("src/main.ts");
        let result = normalize_import_path("\"lodash\"", importing).unwrap();
        assert_eq!(result, PathBuf::from("lodash"));
    }

    #[test]
    fn normalize_import_path_empty_returns_none() {
        let importing = Path::new("src/main.ts");
        assert!(normalize_import_path("\"\"", importing).is_none());
    }

    #[test]
    fn resolve_file_path_exact() {
        let mut file_map = HashMap::new();
        file_map.insert(PathBuf::from("src/utils.ts"), vec![]);
        let result = resolve_file_path(Path::new("src/utils.ts"), &file_map);
        assert_eq!(result, Some(PathBuf::from("src/utils.ts")));
    }

    #[test]
    fn resolve_file_path_with_extension() {
        let mut file_map = HashMap::new();
        file_map.insert(PathBuf::from("src/utils.ts"), vec![]);
        let result = resolve_file_path(Path::new("src/utils"), &file_map);
        assert_eq!(result, Some(PathBuf::from("src/utils.ts")));
    }

    #[test]
    fn resolve_file_path_index_file() {
        let mut file_map = HashMap::new();
        file_map.insert(PathBuf::from("src/utils/index.ts"), vec![]);
        let result = resolve_file_path(Path::new("src/utils"), &file_map);
        assert_eq!(result, Some(PathBuf::from("src/utils/index.ts")));
    }

    #[test]
    fn resolve_file_path_not_found() {
        let file_map = HashMap::new();
        let result = resolve_file_path(Path::new("src/nonexistent"), &file_map);
        assert!(result.is_none());
    }

    #[test]
    fn file_prefix_extraction() {
        let name = Name::from("src/main.ts::function_foo");
        assert_eq!(file_prefix(&name), Some(PathBuf::from("src/main.ts")));
    }

    #[test]
    fn file_prefix_no_separator() {
        let name = Name::from("just_a_vertex");
        assert_eq!(file_prefix(&name), None);
    }

    #[test]
    fn resolve_two_file_typescript_project() {
        // Build a minimal two-file TypeScript project schema by hand.
        //
        // File A: src/main.ts
        //   - import_statement vertex with literal-value constraint "./utils"
        //
        // File B: src/utils.ts
        //   - export_statement vertex
        //
        // After resolution, there should be an "imports" edge from the import
        // vertex in main.ts to the export vertex in utils.ts.

        let protocol = panproto_schema::Protocol {
            name: "project".into(),
            schema_theory: "ThProjectSchema".into(),
            instance_theory: "ThProjectInstance".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: true,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        };

        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);

        // main.ts vertices
        builder = builder
            .vertex("src/main.ts::import_statement_0", "import_statement", None)
            .unwrap();
        builder = builder.constraint(
            "src/main.ts::import_statement_0",
            "literal-value",
            "'./utils'",
        );
        builder = builder
            .vertex("src/main.ts::program_0", "program", None)
            .unwrap();

        // utils.ts vertices
        builder = builder
            .vertex("src/utils.ts::export_statement_0", "export_statement", None)
            .unwrap();
        builder = builder
            .vertex("src/utils.ts::program_0", "program", None)
            .unwrap();

        let mut schema = builder.build().unwrap();

        let mut file_map: HashMap<PathBuf, Vec<Name>> = HashMap::new();
        file_map.insert(
            PathBuf::from("src/main.ts"),
            vec![
                Name::from("src/main.ts::import_statement_0"),
                Name::from("src/main.ts::program_0"),
            ],
        );
        file_map.insert(
            PathBuf::from("src/utils.ts"),
            vec![
                Name::from("src/utils.ts::export_statement_0"),
                Name::from("src/utils.ts::program_0"),
            ],
        );

        let mut protocol_map: HashMap<PathBuf, String> = HashMap::new();
        protocol_map.insert(PathBuf::from("src/main.ts"), "typescript".to_owned());
        protocol_map.insert(PathBuf::from("src/utils.ts"), "typescript".to_owned());

        let rules = default_rules();
        let resolved = resolve_imports(&mut schema, &file_map, &protocol_map, &rules);

        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].import_vertex,
            Name::from("src/main.ts::import_statement_0")
        );
        assert_eq!(
            resolved[0].export_vertex,
            Name::from("src/utils.ts::export_statement_0")
        );
        assert_eq!(resolved[0].source_file, PathBuf::from("src/main.ts"));
        assert_eq!(resolved[0].target_file, PathBuf::from("src/utils.ts"));

        // Verify edge was added to the schema.
        let edges = schema.edges_between(
            "src/main.ts::import_statement_0",
            "src/utils.ts::export_statement_0",
        );
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].kind, Name::from("imports"));

        // Verify precomputed indices were updated.
        let outgoing = schema.outgoing_edges("src/main.ts::import_statement_0");
        assert!(outgoing.iter().any(|e| e.kind == "imports"));

        let incoming = schema.incoming_edges("src/utils.ts::export_statement_0");
        assert!(incoming.iter().any(|e| e.kind == "imports"));
    }

    #[test]
    fn unresolved_imports_are_skipped() {
        // An import pointing to a file not in the project should be silently skipped.
        let protocol = panproto_schema::Protocol {
            name: "project".into(),
            schema_theory: "ThProjectSchema".into(),
            instance_theory: "ThProjectInstance".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: true,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        };

        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
        builder = builder
            .vertex("src/main.ts::import_statement_0", "import_statement", None)
            .unwrap();
        builder = builder.constraint(
            "src/main.ts::import_statement_0",
            "literal-value",
            "'lodash'",
        );

        let mut schema = builder.build().unwrap();

        let mut file_map: HashMap<PathBuf, Vec<Name>> = HashMap::new();
        file_map.insert(
            PathBuf::from("src/main.ts"),
            vec![Name::from("src/main.ts::import_statement_0")],
        );

        let mut protocol_map: HashMap<PathBuf, String> = HashMap::new();
        protocol_map.insert(PathBuf::from("src/main.ts"), "typescript".to_owned());

        let rules = default_rules();
        let resolved = resolve_imports(&mut schema, &file_map, &protocol_map, &rules);

        // No resolution because "lodash" is not in the project.
        assert!(resolved.is_empty());
        assert_eq!(schema.edge_count(), 0);
    }

    #[test]
    fn multiple_exports_resolved() {
        // An import pointing to a file with multiple exports should create
        // edges to all of them.
        let protocol = panproto_schema::Protocol {
            name: "project".into(),
            schema_theory: "ThProjectSchema".into(),
            instance_theory: "ThProjectInstance".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: true,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        };

        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);

        builder = builder
            .vertex("src/app.ts::import_statement_0", "import_statement", None)
            .unwrap();
        builder = builder.constraint("src/app.ts::import_statement_0", "literal-value", "'./lib'");

        builder = builder
            .vertex("src/lib.ts::export_statement_0", "export_statement", None)
            .unwrap();
        builder = builder
            .vertex("src/lib.ts::export_statement_1", "export_statement", None)
            .unwrap();

        let mut schema = builder.build().unwrap();

        let mut file_map: HashMap<PathBuf, Vec<Name>> = HashMap::new();
        file_map.insert(
            PathBuf::from("src/app.ts"),
            vec![Name::from("src/app.ts::import_statement_0")],
        );
        file_map.insert(
            PathBuf::from("src/lib.ts"),
            vec![
                Name::from("src/lib.ts::export_statement_0"),
                Name::from("src/lib.ts::export_statement_1"),
            ],
        );

        let mut protocol_map: HashMap<PathBuf, String> = HashMap::new();
        protocol_map.insert(PathBuf::from("src/app.ts"), "typescript".to_owned());
        protocol_map.insert(PathBuf::from("src/lib.ts"), "typescript".to_owned());

        let rules = default_rules();
        let resolved = resolve_imports(&mut schema, &file_map, &protocol_map, &rules);

        assert_eq!(resolved.len(), 2);
        assert_eq!(schema.edge_count(), 2);
    }
}
