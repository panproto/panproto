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

    /// Access the per-file parsed schemas without consuming the builder.
    #[must_use]
    pub const fn file_schemas(&self) -> &FxHashMap<PathBuf, Schema> {
        &self.file_schemas
    }

    /// Access the per-file protocol names without consuming the builder.
    #[must_use]
    pub const fn protocol_map_ref(&self) -> &FxHashMap<PathBuf, String> {
        &self.protocol_map
    }

    /// Build the project as a Merkle tree of per-file schemas rather
    /// than a flat coproduct schema.
    ///
    /// The tree is stored incrementally in `store`; see
    /// [`build_project_tree`] for the standalone function form.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::CoproductFailed`] if the underlying
    /// store rejects an object write.
    pub fn build_tree<S>(self, store: &mut S) -> Result<ProjectSchemaTree, ProjectError>
    where
        S: panproto_vcs::Store,
    {
        // Mirror the flat build path: resolve cross-file imports so
        // the per-file schemas persisted under the tree carry the
        // same edges the flat coproduct would. Without this step,
        // tree-built projects silently drop every cross-file import
        // edge that the flat build path records.
        let cross_file_edges = resolve_per_file_imports(&self.file_schemas, &self.protocol_map)?;
        let root_id = build_project_tree(
            store,
            &self.file_schemas,
            &self.protocol_map,
            &cross_file_edges,
        )?;
        let protocol_map: HashMap<PathBuf, String> = self.protocol_map.into_iter().collect();
        Ok(ProjectSchemaTree {
            root_id,
            protocol_map,
        })
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

/// A project schema stored as a Merkle tree of per-file schemas.
///
/// The root [`panproto_vcs::ObjectId`] points at a
/// [`panproto_vcs::SchemaTreeObject`] whose leaves are
/// [`panproto_vcs::FileSchemaObject`]s. Consumers that want the
/// assembled flat [`Schema`] call
/// [`panproto_vcs::assemble_schema`] with this root.
#[derive(Debug, Clone)]
pub struct ProjectSchemaTree {
    /// Object ID of the root [`panproto_vcs::SchemaTreeObject`].
    pub root_id: panproto_vcs::ObjectId,
    /// Mapping from file path to the protocol used to parse it.
    pub protocol_map: HashMap<PathBuf, String>,
}

/// Run the cross-file import resolver against the flat coproduct of
/// the given per-file schemas and bucket the resulting import edges
/// by the file that owns the importing vertex.
///
/// Returned edges carry vertex names already prefixed with their
/// owning file's project path (`<path>::<name>`), matching the
/// convention used by [`ProjectBuilder::build`], so that flat
/// assembly (`panproto_vcs::assemble_schema`) can add them verbatim
/// without re-prefixing.
fn resolve_per_file_imports<H1, H2>(
    file_schemas: &std::collections::HashMap<PathBuf, panproto_schema::Schema, H1>,
    protocol_map: &std::collections::HashMap<PathBuf, String, H2>,
) -> Result<std::collections::HashMap<PathBuf, Vec<panproto_schema::Edge>>, ProjectError>
where
    H1: std::hash::BuildHasher,
    H2: std::hash::BuildHasher,
{
    if file_schemas.len() <= 1 {
        return Ok(HashMap::new());
    }

    // Rebuild the flat coproduct just like `ProjectBuilder::build`
    // does. The duplicated loop body is what makes the flat and tree
    // paths agree on edge composition; factoring it out across the
    // two would tangle `build_tree`'s lifetime with `build`'s.
    let coproduct_protocol = panproto_schema::Protocol {
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

    let mut builder = panproto_schema::SchemaBuilder::new(&coproduct_protocol);
    let mut file_map: HashMap<PathBuf, Vec<panproto_gat::Name>> = HashMap::new();

    for (path, schema) in file_schemas {
        let prefix = path.display().to_string();
        let mut file_vertices = Vec::new();
        for (name, vertex) in &schema.vertices {
            let prefixed_name = format!("{prefix}::{name}");
            builder = builder
                .vertex(&prefixed_name, vertex.kind.as_ref(), None)
                .map_err(|e| ProjectError::CoproductFailed {
                    reason: format!("vertex {prefixed_name}: {e}"),
                })?;
            file_vertices.push(panproto_gat::Name::from(prefixed_name.as_str()));
            if let Some(constraints) = schema.constraints.get(name) {
                for c in constraints {
                    builder = builder.constraint(&prefixed_name, c.sort.as_ref(), &c.value);
                }
            }
        }
        for edge in schema.edges.keys() {
            let prefixed_src = format!("{prefix}::{}", edge.src);
            let prefixed_tgt = format!("{prefix}::{}", edge.tgt);
            let edge_name = edge.name.as_ref().map(|n| format!("{prefix}::{n}"));
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
    let protocols: HashMap<PathBuf, String> = protocol_map
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let before: std::collections::HashSet<panproto_schema::Edge> =
        schema.edges.keys().cloned().collect();
    let rules = resolve::default_rules();
    resolve::resolve_imports(&mut schema, &file_map, &protocols, &rules);

    let new_edges: Vec<panproto_schema::Edge> = schema
        .edges
        .keys()
        .filter(|e| !before.contains(*e))
        .cloned()
        .collect();
    bucket_new_edges(&new_edges, &file_map)
}

/// Bucket newly synthesized cross-file import edges by the file whose
/// vertex list contains each edge's `src`. Surfaces
/// [`ProjectError::OrphanImportEdge`] if any edge's src matches no file.
fn bucket_new_edges<H>(
    new_edges: &[panproto_schema::Edge],
    file_map: &HashMap<PathBuf, Vec<panproto_gat::Name>, H>,
) -> Result<HashMap<PathBuf, Vec<panproto_schema::Edge>>, ProjectError>
where
    H: std::hash::BuildHasher,
{
    let mut by_file: HashMap<PathBuf, Vec<panproto_schema::Edge>> = HashMap::new();
    for edge in new_edges {
        let Some(owner) = file_map
            .iter()
            .find(|(_, verts)| verts.iter().any(|v| v == &edge.src))
            .map(|(path, _)| path.clone())
        else {
            return Err(ProjectError::OrphanImportEdge {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
            });
        };
        by_file.entry(owner).or_default().push(edge.clone());
    }
    Ok(by_file)
}

/// Build a project schema tree and store it in `store`.
///
/// Each `(path, schema)` pair in `files` becomes a
/// [`panproto_vcs::FileSchemaObject`]; the path components induce a
/// directory hierarchy of
/// [`panproto_vcs::SchemaTreeObject`]s stored bottom up. The returned
/// [`panproto_vcs::ObjectId`] is stable across input-order
/// permutations because sibling entries are sorted lexicographically.
///
/// `protocols` carries the per-file protocol names so the stored
/// leaves can record how each file was parsed; missing entries fall
/// back to `"raw_file"`. `cross_file_edges` carries the already-
/// prefixed cross-file import edges rooted at each file; pass an
/// empty map to skip cross-file resolution.
///
/// # Errors
///
/// Returns [`ProjectError::CoproductFailed`] if the underlying store
/// rejects an object write.
pub fn build_project_tree<S, H1, H2, H3>(
    store: &mut S,
    files: &std::collections::HashMap<PathBuf, panproto_schema::Schema, H1>,
    protocols: &std::collections::HashMap<PathBuf, String, H2>,
    cross_file_edges: &std::collections::HashMap<PathBuf, Vec<panproto_schema::Edge>, H3>,
) -> Result<panproto_vcs::ObjectId, ProjectError>
where
    S: panproto_vcs::Store,
    H1: std::hash::BuildHasher,
    H2: std::hash::BuildHasher,
    H3: std::hash::BuildHasher,
{
    let mut leaves: Vec<(PathBuf, panproto_vcs::FileSchemaObject)> = files
        .iter()
        .map(|(path, schema)| {
            let protocol = protocols
                .get(path)
                .cloned()
                .unwrap_or_else(|| "raw_file".to_owned());
            let mut cross = cross_file_edges.get(path).cloned().unwrap_or_default();
            // Canonicalize wire order: sort so that the emitted bytes
            // are stable across input permutations. `hash_file_schema`
            // also sorts before hashing, but canonicalizing here makes
            // the stored wire bytes themselves deterministic.
            cross.sort();
            let file = panproto_vcs::FileSchemaObject {
                path: path.display().to_string(),
                protocol,
                schema: schema.clone(),
                cross_file_edges: cross,
            };
            (path.clone(), file)
        })
        .collect();
    leaves.sort_by(|a, b| a.0.cmp(&b.0));

    panproto_vcs::build_schema_tree(store, leaves).map_err(|e| ProjectError::CoproductFailed {
        reason: format!("build_schema_tree: {e}"),
    })
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
    fn build_tree_stable_across_insertion_order() {
        use panproto_vcs::MemStore;

        let build = |paths: Vec<(&str, &[u8])>| -> panproto_vcs::ObjectId {
            let mut builder = ProjectBuilder::new();
            for (p, c) in paths {
                builder.add_file(Path::new(p), c).unwrap();
            }
            let mut store = MemStore::new();
            let tree = builder.build_tree(&mut store).unwrap();
            tree.root_id
        };

        let forward = build(vec![
            ("src/a.rs", b"pub fn a() {}"),
            ("src/b.rs", b"pub fn b() {}"),
        ]);
        let reverse = build(vec![
            ("src/b.rs", b"pub fn b() {}"),
            ("src/a.rs", b"pub fn a() {}"),
        ]);
        assert_eq!(forward, reverse);
    }

    #[test]
    fn build_tree_preserves_cross_file_imports() {
        use panproto_vcs::MemStore;

        // Two-file TypeScript project with an import: assembled flat
        // schema from the tree must have the same edge count as the
        // flat `build()` result.
        let build_flat = || -> Schema {
            let mut builder = ProjectBuilder::new();
            builder
                .add_file(
                    Path::new("src/utils.ts"),
                    b"export function add(a: number, b: number): number { return a + b; }\n",
                )
                .unwrap();
            builder
                .add_file(
                    Path::new("src/main.ts"),
                    b"import { add } from './utils';\nadd(1, 2);\n",
                )
                .unwrap();
            builder.build().unwrap().schema
        };

        let build_tree_flat = || -> Schema {
            let mut builder = ProjectBuilder::new();
            builder
                .add_file(
                    Path::new("src/utils.ts"),
                    b"export function add(a: number, b: number): number { return a + b; }\n",
                )
                .unwrap();
            builder
                .add_file(
                    Path::new("src/main.ts"),
                    b"import { add } from './utils';\nadd(1, 2);\n",
                )
                .unwrap();
            let mut store = MemStore::new();
            let tree = builder.build_tree(&mut store).unwrap();
            let proto = panproto_vcs::project_coproduct_protocol();
            panproto_vcs::assemble_schema(&store, &tree.root_id, &proto).unwrap()
        };

        let flat = build_flat();
        let assembled = build_tree_flat();
        assert_eq!(
            flat.edges.len(),
            assembled.edges.len(),
            "tree-built project drops edges; cross-file imports are likely missing"
        );
    }

    #[test]
    fn build_tree_assembles_back_to_flat_schema() {
        use panproto_vcs::MemStore;

        let mut builder = ProjectBuilder::new();
        builder
            .add_file(Path::new("x.rs"), b"pub fn x() {}")
            .unwrap();
        builder
            .add_file(Path::new("y.rs"), b"pub fn y() {}")
            .unwrap();
        let flat = builder.build().unwrap().schema;

        let mut builder = ProjectBuilder::new();
        builder
            .add_file(Path::new("x.rs"), b"pub fn x() {}")
            .unwrap();
        builder
            .add_file(Path::new("y.rs"), b"pub fn y() {}")
            .unwrap();
        let mut store = MemStore::new();
        let tree = builder.build_tree(&mut store).unwrap();
        let proto = panproto_vcs::project_coproduct_protocol();
        let assembled = panproto_vcs::assemble_schema(&store, &tree.root_id, &proto).unwrap();

        // Match on vertex and edge counts; the assembled form must
        // carry the same structural content as the flat build.
        assert_eq!(flat.vertices.len(), assembled.vertices.len());
        assert_eq!(flat.edges.len(), assembled.edges.len());
    }

    #[test]
    fn cross_file_edges_wire_bytes_are_deterministic() {
        use panproto_gat::Name;
        use panproto_schema::Edge;
        use panproto_vcs::FileSchemaObject;

        // Two synthetic edges rooted at the same vertex but ordered
        // differently in the input Vec. Emitted wire bytes must match.
        let e1 = Edge {
            src: Name::from("src/main.ts::importStmt"),
            tgt: Name::from("src/a.ts::exportA"),
            kind: Name::from("imports"),
            name: None,
        };
        let e2 = Edge {
            src: Name::from("src/main.ts::importStmt"),
            tgt: Name::from("src/b.ts::exportB"),
            kind: Name::from("imports"),
            name: None,
        };

        let mut files_a = HashMap::new();
        let tiny = panproto_schema::SchemaBuilder::new(&panproto_schema::Protocol {
            name: "project".into(),
            ..Default::default()
        })
        .vertex("x", "record", None)
        .unwrap()
        .build()
        .unwrap();
        files_a.insert(PathBuf::from("src/main.ts"), tiny);
        let mut protocols = HashMap::new();
        protocols.insert(PathBuf::from("src/main.ts"), "typescript".to_owned());
        let mut ce_forward = HashMap::new();
        ce_forward.insert(PathBuf::from("src/main.ts"), vec![e1.clone(), e2.clone()]);
        let mut ce_reverse = HashMap::new();
        ce_reverse.insert(PathBuf::from("src/main.ts"), vec![e2, e1]);

        let mut store_a = panproto_vcs::MemStore::new();
        let mut store_b = panproto_vcs::MemStore::new();
        let id_a = build_project_tree(&mut store_a, &files_a, &protocols, &ce_forward).unwrap();
        let id_b = build_project_tree(&mut store_b, &files_a, &protocols, &ce_reverse).unwrap();
        assert_eq!(
            id_a, id_b,
            "FileSchemaObject wire order must be deterministic"
        );

        // Stronger check: walk each store, find the wrapped
        // FileSchemaObject, and compare raw serialized bytes.
        let collect_bytes = |store: &panproto_vcs::MemStore, root: panproto_vcs::ObjectId| {
            let mut bytes: Vec<u8> = Vec::new();
            panproto_vcs::walk_tree(store, &root, |_, file: &FileSchemaObject| {
                bytes = serde_json::to_vec(file).unwrap();
                Ok(())
            })
            .unwrap();
            bytes
        };
        assert_eq!(collect_bytes(&store_a, id_a), collect_bytes(&store_b, id_b));
    }

    #[test]
    fn orphan_import_edge_is_surfaced() {
        use panproto_gat::Name;
        use panproto_schema::Edge;

        // Forge a new edge whose `src` is not in any file's vertex
        // list. The bucketing helper must surface OrphanImportEdge
        // rather than silently drop the edge.
        let mut file_map: HashMap<PathBuf, Vec<Name>> = HashMap::new();
        file_map.insert(
            PathBuf::from("src/a.ts"),
            vec![Name::from("src/a.ts::real")],
        );

        let orphan = Edge {
            src: Name::from("unknown::ghost"),
            tgt: Name::from("src/a.ts::real"),
            kind: Name::from("imports"),
            name: None,
        };

        let err = bucket_new_edges(&[orphan], &file_map).unwrap_err();
        match err {
            ProjectError::OrphanImportEdge { src, tgt } => {
                assert!(src.contains("ghost"));
                assert!(tgt.contains("real"));
            }
            other => panic!("expected OrphanImportEdge, got {other:?}"),
        }
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
