//! # panproto-project
//!
//! Multi-file project assembly via schema coproduct for panproto.
//!
//! Orchestrates parsing all files in a project directory into a unified
//! project-level schema. The project schema is the coproduct (disjoint union)
//! of per-file schemas, with cross-file edges for imports and type references.
//!
//! ## Two-pass approach
//!
//! 1. **Parse pass**: For each file, detect language, parse via
//!    `ParserRegistry`, prefix vertex IDs
//!    with the file path.
//! 2. **Resolve pass**: Walk `import` vertices, match against exports
//!    in other file schemas, emit `imports` edges connecting them.
//!
//! ## Coproduct construction
//!
//! The schema-level coproduct prefixes each file's vertex names with the file
//! path. Edges within a file retain their local structure. The result is a
//! single [`Schema`] spanning the entire project.
//!
//! The coproduct is universal: any morphism out of the project schema restricts
//! to per-file morphisms. This means per-file diffs compose into project-level
//! diffs automatically.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::GlobSet;
use panproto_parse::ParserRegistry;
use panproto_protocols::raw_file;
use panproto_schema::Schema;
use rustc_hash::FxHashMap;

/// Incremental parsing cache for project assembly.
pub mod cache;

/// Project manifest (`panproto.toml`) loading and generation.
pub mod config;

/// Language detection by file extension and package detection.
pub mod detect;

/// Error types for project assembly.
pub mod error;

/// Cross-file import resolution.
pub mod resolve;

pub use config::ProjectConfig;
pub use detect::DetectedPackage;
pub use error::ProjectError;

/// A parsed project containing a unified schema and per-file metadata.
#[derive(Debug, Clone)]
pub struct ProjectSchema {
    /// The unified coproduct schema spanning all files.
    pub schema: Schema,
    /// Mapping from file path to the root vertex IDs belonging to that file.
    pub file_map: HashMap<PathBuf, Vec<panproto_gat::Name>>,
    /// Mapping from file path to the protocol used to parse it.
    pub protocol_map: HashMap<PathBuf, String>,
}

/// Builder for assembling a multi-file project into a unified schema.
///
/// Files are added one at a time (or by scanning a directory), then assembled
/// into a [`ProjectSchema`] via coproduct construction.
pub struct ProjectBuilder {
    /// The parser registry for all supported languages.
    registry: ParserRegistry,
    /// Per-file parsed schemas, keyed by file path.
    file_schemas: FxHashMap<PathBuf, Schema>,
    /// Per-file protocol names.
    protocol_map: FxHashMap<PathBuf, String>,
    /// Compiled exclude patterns from config (if any).
    excludes: Option<GlobSet>,
    /// Per-path protocol overrides from package config.
    protocol_overrides: FxHashMap<PathBuf, String>,
    /// Optional incremental parsing cache for skipping unchanged files.
    cache: Option<cache::FileCache>,
}

impl ProjectBuilder {
    /// Create a new project builder with the default parser registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: ParserRegistry::new(),
            file_schemas: FxHashMap::default(),
            protocol_map: FxHashMap::default(),
            excludes: None,
            protocol_overrides: FxHashMap::default(),
            cache: None,
        }
    }

    /// Create a new project builder with a custom parser registry.
    #[must_use]
    pub fn with_registry(registry: ParserRegistry) -> Self {
        Self {
            registry,
            file_schemas: FxHashMap::default(),
            protocol_map: FxHashMap::default(),
            excludes: None,
            protocol_overrides: FxHashMap::default(),
            cache: None,
        }
    }

    /// Create a project builder configured from a [`ProjectConfig`].
    ///
    /// Compiles exclude patterns and builds the per-package protocol override map.
    ///
    /// # Errors
    ///
    /// Returns `ProjectError::InvalidPattern` if a glob pattern is malformed.
    pub fn with_config(cfg: &ProjectConfig, base_dir: &Path) -> Result<Self, ProjectError> {
        let excludes = config::compile_excludes(base_dir, &cfg.workspace.exclude)?;
        let mut protocol_overrides = FxHashMap::default();
        for pkg in &cfg.package {
            if let Some(ref proto) = pkg.protocol {
                protocol_overrides.insert(base_dir.join(&pkg.path), proto.clone());
            }
        }
        Ok(Self {
            registry: ParserRegistry::new(),
            file_schemas: FxHashMap::default(),
            protocol_map: FxHashMap::default(),
            excludes: Some(excludes),
            protocol_overrides,
            cache: None,
        })
    }

    /// Create a project builder configured from a [`ProjectConfig`] with an
    /// incremental parsing cache.
    ///
    /// Behaves like [`with_config`](Self::with_config) but attaches a
    /// [`FileCache`](cache::FileCache) so that unchanged files are not
    /// re-parsed.
    ///
    /// # Errors
    ///
    /// Returns `ProjectError::InvalidPattern` if a glob pattern is malformed.
    pub fn with_config_and_cache(
        cfg: &ProjectConfig,
        base_dir: &Path,
        file_cache: cache::FileCache,
    ) -> Result<Self, ProjectError> {
        let mut builder = Self::with_config(cfg, base_dir)?;
        builder.cache = Some(file_cache);
        Ok(builder)
    }

    /// Extract the cache from the builder (e.g., for saving after build).
    ///
    /// Returns `None` if no cache was attached.
    pub const fn take_cache(&mut self) -> Option<cache::FileCache> {
        self.cache.take()
    }

    /// Add a single file to the project.
    ///
    /// The file's language is detected from its path. If the language is
    /// recognized, the file is parsed via tree-sitter. Otherwise, it is
    /// parsed as a raw file (text or binary).
    ///
    /// If a cache is attached and the file's mtime and size match the
    /// cached entry, the cached schema is used without re-parsing.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::ParseFailed`] if parsing fails.
    pub fn add_file(&mut self, path: &Path, content: &[u8]) -> Result<(), ProjectError> {
        // Check cache first.
        if let Some(ref mut file_cache) = self.cache {
            if let Some(entry) = file_cache.entries.get(path) {
                if cache::is_valid(entry, path) {
                    self.file_schemas
                        .insert(path.to_owned(), entry.schema.clone());
                    self.protocol_map
                        .insert(path.to_owned(), entry.protocol.clone());
                    return Ok(());
                }
            }
        }

        let path_str = path.display().to_string();

        // Check per-package protocol override first.
        let override_protocol = self
            .protocol_overrides
            .iter()
            .find(|(pkg_path, _)| path.starts_with(pkg_path))
            .map(|(_, proto)| proto.clone());

        // Detect language and parse.
        let (schema, protocol_name) = if let Some(proto) = override_protocol {
            if let Ok(schema) = self
                .registry
                .parse_with_protocol(&proto, content, &path_str)
            {
                (schema, proto)
            } else {
                // Fall back to raw file if overridden protocol fails.
                let text = std::str::from_utf8(content).map_err(|e| ProjectError::ParseFailed {
                    path: path_str.clone(),
                    reason: format!("UTF-8 decode: {e}"),
                })?;
                let schema = raw_file::parse_text(text, &path_str).map_err(|e| {
                    ProjectError::ParseFailed {
                        path: path_str.clone(),
                        reason: e.to_string(),
                    }
                })?;
                (schema, "raw_file".to_owned())
            }
        } else if let Some(protocol) = detect::detect_language(path, &self.registry) {
            if let Ok(schema) = self
                .registry
                .parse_with_protocol(protocol, content, &path_str)
            {
                (schema, protocol.to_owned())
            } else {
                // Fall back to raw file parsing if the language parser fails
                // (e.g., Kotlin's tree-sitter grammar is ABI-incompatible).
                let text = std::str::from_utf8(content).map_err(|e| ProjectError::ParseFailed {
                    path: path_str.clone(),
                    reason: format!("UTF-8 decode: {e}"),
                })?;
                let schema = raw_file::parse_text(text, &path_str).map_err(|e| {
                    ProjectError::ParseFailed {
                        path: path_str.clone(),
                        reason: e.to_string(),
                    }
                })?;
                (schema, "raw_file".to_owned())
            }
        } else if detect::is_binary_extension(path) {
            let schema = raw_file::parse_binary(&path_str, content).map_err(|e| {
                ProjectError::ParseFailed {
                    path: path_str.clone(),
                    reason: e.to_string(),
                }
            })?;
            (schema, "raw_file".to_owned())
        } else {
            // Parse as text raw file.
            let text = std::str::from_utf8(content).map_err(|e| ProjectError::ParseFailed {
                path: path_str.clone(),
                reason: format!("UTF-8 decode: {e}"),
            })?;
            let schema =
                raw_file::parse_text(text, &path_str).map_err(|e| ProjectError::ParseFailed {
                    path: path_str.clone(),
                    reason: e.to_string(),
                })?;
            (schema, "raw_file".to_owned())
        };

        // Update cache entry for this file.
        if let Some(ref mut file_cache) = self.cache {
            let metadata = std::fs::metadata(path).ok();
            let mtime_secs = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let size = metadata.map_or(0, |m| m.len());
            let content_hash = blake3::hash(content).to_string();
            file_cache.entries.insert(
                path.to_owned(),
                cache::CacheEntry {
                    mtime_secs,
                    size,
                    content_hash,
                    schema: schema.clone(),
                    protocol: protocol_name.clone(),
                },
            );
        }

        self.file_schemas.insert(path.to_owned(), schema);
        self.protocol_map.insert(path.to_owned(), protocol_name);
        Ok(())
    }

    /// Add all files in a directory (recursively).
    ///
    /// Skips hidden directories (starting with `.`) and common build/output
    /// directories (`target`, `node_modules`, `__pycache__`, `.git`, etc.).
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] if any file fails to read or parse.
    pub fn add_directory(&mut self, dir: &Path) -> Result<(), ProjectError> {
        self.walk_directory(dir)
    }

    /// Recursively walk a directory, adding all files.
    fn walk_directory(&mut self, dir: &Path) -> Result<(), ProjectError> {
        let entries = std::fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Always skip hidden files/directories.
            if name_str.starts_with('.') {
                continue;
            }

            // Check against compiled excludes (config-driven) or hardcoded defaults.
            if let Some(ref excludes) = self.excludes {
                if excludes.is_match(&path) {
                    continue;
                }
            } else if matches!(
                name_str.as_ref(),
                "target" | "node_modules" | "__pycache__" | "build" | "dist" | "vendor" | "Pods"
            ) {
                continue;
            }

            if path.is_dir() {
                self.walk_directory(&path)?;
            } else if path.is_file() {
                let content = std::fs::read(&path)?;
                self.add_file(&path, &content)?;
            }
        }

        Ok(())
    }

    /// Get the number of files added to the builder.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.file_schemas.len()
    }

    /// Build the project schema by constructing the coproduct of all file schemas.
    ///
    /// Each file's vertices are prefixed with the file path to ensure uniqueness
    /// in the coproduct. Edges within a file retain their local structure.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::CoproductFailed`] if construction fails.
    pub fn build(self) -> Result<ProjectSchema, ProjectError> {
        if self.file_schemas.is_empty() {
            return Err(ProjectError::CoproductFailed {
                reason: "no files added to project".to_owned(),
            });
        }

        // For single-file projects, return the schema as-is.
        if self.file_schemas.len() == 1 {
            let (path, schema) = self.file_schemas.into_iter().next().ok_or_else(|| {
                ProjectError::CoproductFailed {
                    reason: "internal error: empty after length check".to_owned(),
                }
            })?;

            let root_vertices: Vec<panproto_gat::Name> = schema.vertices.keys().cloned().collect();
            let mut file_map = HashMap::new();
            file_map.insert(path, root_vertices);

            let protocol_map: HashMap<PathBuf, String> = self.protocol_map.into_iter().collect();

            return Ok(ProjectSchema {
                schema,
                file_map,
                protocol_map,
            });
        }

        // Multi-file coproduct: build a new schema containing all vertices/edges
        // from all file schemas, with path-prefixed names.
        //
        // We use the "raw_file" protocol for the coproduct since it's the most
        // permissive (empty obj_kinds = open protocol). The coproduct schema
        // contains vertices from multiple protocols.
        let coproduct_protocol = panproto_schema::Protocol {
            name: "project".into(),
            schema_theory: "ThProjectSchema".into(),
            instance_theory: "ThProjectInstance".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![], // Open protocol.
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

        let mut builder = panproto_schema::SchemaBuilder::new(&coproduct_protocol);
        let mut file_map: HashMap<PathBuf, Vec<panproto_gat::Name>> = HashMap::new();

        for (path, schema) in &self.file_schemas {
            let prefix = path.display().to_string();
            let mut file_vertices = Vec::new();

            // Copy vertices with path prefix.
            for (name, vertex) in &schema.vertices {
                let prefixed_name = format!("{prefix}::{name}");
                builder = builder
                    .vertex(&prefixed_name, vertex.kind.as_ref(), None)
                    .map_err(|e| ProjectError::CoproductFailed {
                        reason: format!("vertex {prefixed_name}: {e}"),
                    })?;
                file_vertices.push(panproto_gat::Name::from(prefixed_name.as_str()));

                // Copy constraints.
                if let Some(constraints) = schema.constraints.get(name) {
                    for c in constraints {
                        builder = builder.constraint(&prefixed_name, c.sort.as_ref(), &c.value);
                    }
                }
            }

            // Copy edges with prefixed source and target.
            for edge in schema.edges.keys() {
                let prefixed_src = format!("{prefix}::{}", edge.src);
                let prefixed_tgt = format!("{prefix}::{}", edge.tgt);
                let edge_name = edge.name.as_ref().map(|n| {
                    let prefixed = format!("{prefix}::{n}");
                    prefixed
                });
                builder = builder
                    .edge(
                        &prefixed_src,
                        &prefixed_tgt,
                        edge.kind.as_ref(),
                        edge_name.as_deref(),
                    )
                    .map_err(|e| ProjectError::CoproductFailed {
                        reason: format!("edge {prefixed_src} -> {prefixed_tgt}: {e}"),
                    })?;
            }

            file_map.insert(path.clone(), file_vertices);
        }

        let mut schema = builder.build().map_err(|e| ProjectError::CoproductFailed {
            reason: format!("build: {e}"),
        })?;

        let protocol_map: HashMap<PathBuf, String> = self.protocol_map.into_iter().collect();

        // Resolve cross-file imports using default rules.
        let rules = resolve::default_rules();
        let _resolved = resolve::resolve_imports(&mut schema, &file_map, &protocol_map, &rules);

        Ok(ProjectSchema {
            schema,
            file_map,
            protocol_map,
        })
    }
}

impl Default for ProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn single_file_project() {
        let mut builder = ProjectBuilder::new();
        builder
            .add_file(
                Path::new("main.ts"),
                b"function hello(): string { return 'Hello'; }",
            )
            .unwrap();

        assert_eq!(builder.file_count(), 1);

        let project = builder.build().unwrap();
        assert!(!project.schema.vertices.is_empty());
        assert_eq!(project.file_map.len(), 1);
        assert_eq!(project.protocol_map.len(), 1);
        assert_eq!(
            project.protocol_map.get(Path::new("main.ts")),
            Some(&"typescript".to_owned())
        );
    }

    #[test]
    fn multi_file_project() {
        let mut builder = ProjectBuilder::new();

        builder
            .add_file(
                Path::new("src/main.ts"),
                b"function main(): void { console.log('hello'); }",
            )
            .unwrap();

        builder
            .add_file(
                Path::new("src/utils.ts"),
                b"export function add(a: number, b: number): number { return a + b; }",
            )
            .unwrap();

        assert_eq!(builder.file_count(), 2);

        let project = builder.build().unwrap();
        assert!(project.schema.vertices.len() > 5);
        assert_eq!(project.file_map.len(), 2);
    }

    #[test]
    fn raw_file_fallback() {
        let mut builder = ProjectBuilder::new();

        builder
            .add_file(Path::new("README.md"), b"# Hello\n\nThis is a project.\n")
            .unwrap();

        let project = builder.build().unwrap();
        assert_eq!(
            project.protocol_map.get(Path::new("README.md")),
            Some(&"raw_file".to_owned())
        );
    }

    #[test]
    fn mixed_languages() {
        let mut builder = ProjectBuilder::new();

        builder
            .add_file(Path::new("main.py"), b"def main():\n    print('hello')\n")
            .unwrap();

        builder
            .add_file(
                Path::new("lib.rs"),
                b"pub fn add(a: i32, b: i32) -> i32 { a + b }",
            )
            .unwrap();

        builder
            .add_file(Path::new("README.md"), b"# Mixed project\n")
            .unwrap();

        assert_eq!(builder.file_count(), 3);

        let project = builder.build().unwrap();
        assert_eq!(project.file_map.len(), 3);
        assert_eq!(
            project.protocol_map.get(Path::new("main.py")),
            Some(&"python".to_owned())
        );
        assert_eq!(
            project.protocol_map.get(Path::new("lib.rs")),
            Some(&"rust".to_owned())
        );
        assert_eq!(
            project.protocol_map.get(Path::new("README.md")),
            Some(&"raw_file".to_owned())
        );
    }

    #[test]
    fn empty_project_errors() {
        let builder = ProjectBuilder::new();
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn language_detection() {
        let registry = ParserRegistry::new();
        assert_eq!(
            detect::detect_language(Path::new("a.ts"), &registry),
            Some("typescript")
        );
        assert_eq!(
            detect::detect_language(Path::new("b.py"), &registry),
            Some("python")
        );
        assert_eq!(
            detect::detect_language(Path::new("c.rs"), &registry),
            Some("rust")
        );
        assert_eq!(detect::detect_language(Path::new("d.md"), &registry), None);
    }
}
