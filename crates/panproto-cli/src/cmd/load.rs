//! One input loader shared by the schema-level commands.
//!
//! Every command that takes a schema on the command line accepts the
//! same four input shapes:
//!
//! 1. a JSON file holding panproto's own serialized [`Schema`];
//! 2. a single schema document written in a protocol's own surface
//!    syntax, or any source file the tree-sitter parser registry
//!    recognizes;
//! 3. a directory backed by a `panproto.toml` manifest, parsed as one
//!    bundle so that references across documents resolve to the real
//!    target vertex rather than an opaque placeholder;
//! 4. a bare directory with no manifest, parsed file by file.
//!
//! Keeping the dispatch here means `add`, `compat` and `diff` agree on
//! what a path denotes, so a project that stages cleanly is a project
//! that can also be diffed and classified.
//!
//! The manifest search walks up from the input, so a package
//! subdirectory or a single document inside one resolves against the
//! manifest at the repository root rather than falling through to the
//! generic parser. The document set is then the intersection of the
//! input path with the manifest's declared packages, which keeps data
//! fixtures and other non-schema JSON that happens to sit beside the
//! packages out of the bundle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    protocols,
    schema::{Edge, Schema},
    vcs,
};

/// How the loader interpreted the path it was handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// A JSON file holding panproto's own serialized `Schema`.
    InternalSchema,
    /// A single schema document parsed by a protocol's document parser.
    ProtocolDocument,
    /// A single source file parsed by the tree-sitter parser registry.
    SourceFile,
    /// A document set whose protocol comes from a `panproto.toml`
    /// manifest, parsed as one cross-referencing bundle.
    ManifestBundle,
    /// A document set with no manifest, parsed as one cross-referencing
    /// bundle in the protocol the caller named.
    ProtocolBundle,
    /// A directory parsed file by file through the tree-sitter registry.
    SourceTree,
}

impl InputKind {
    /// A short human-readable name for this input shape.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::InternalSchema => "internal schema file",
            Self::ProtocolDocument => "schema document",
            Self::SourceFile => "source file",
            Self::ManifestBundle => "manifest-backed project",
            Self::ProtocolBundle => "document bundle",
            Self::SourceTree => "source tree",
        }
    }
}

/// Caller-supplied context for a load.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoadOptions<'a> {
    /// The protocol the caller named on the command line, if any.
    ///
    /// It selects the parser for a raw document or for a directory with
    /// no manifest. When the input is manifest-backed, the manifest is
    /// authoritative and a disagreeing name is an error rather than a
    /// silent override.
    pub protocol: Option<&'a str>,
    /// Report each parsing decision on stderr.
    pub verbose: bool,
}

/// A multi-file project, kept per file so a caller can store one schema
/// tree leaf per source file instead of a single flattened blob.
pub struct ProjectInput {
    source: ProjectSource,
}

enum ProjectSource {
    /// Parsed by one protocol's bundle parser, retaining per-file
    /// provenance and the edges that cross document boundaries.
    Bundle {
        files: HashMap<PathBuf, Schema>,
        protocols: HashMap<PathBuf, String>,
        cross_file_edges: HashMap<PathBuf, Vec<Edge>>,
    },
    /// Parsed file by file through the tree-sitter parser registry.
    Parsed(Box<panproto_project::ProjectBuilder>),
}

impl ProjectInput {
    /// Store this project as a schema tree, one leaf per source file.
    ///
    /// # Errors
    ///
    /// Returns an error if a per-file schema cannot be built or the
    /// store rejects an object write.
    pub fn build_tree<S: vcs::Store>(
        self,
        store: &mut S,
    ) -> Result<panproto_project::ProjectSchemaTree> {
        match self.source {
            ProjectSource::Parsed(builder) => builder.build_tree(store).into_diagnostic(),
            ProjectSource::Bundle {
                files,
                protocols,
                cross_file_edges,
            } => {
                let root_id = panproto_project::build_project_tree(
                    store,
                    &files,
                    &protocols,
                    &cross_file_edges,
                )
                .into_diagnostic()?;
                Ok(panproto_project::ProjectSchemaTree {
                    root_id,
                    protocol_map: protocols,
                })
            }
        }
    }

    /// Flatten this project into the single schema its files
    /// coproduct to, with each file's vertex names prefixed by that
    /// file's path.
    ///
    /// # Errors
    ///
    /// Returns an error if a per-file schema cannot be built or the
    /// coproduct assembly rejects a vertex or edge.
    pub fn build_schema(self) -> Result<Schema> {
        match self.source {
            ProjectSource::Parsed(builder) => Ok(builder.build().into_diagnostic()?.schema),
            bundle @ ProjectSource::Bundle { .. } => {
                let mut store = vcs::MemStore::new();
                let tree = Self { source: bundle }.build_tree(&mut store)?;
                vcs::assemble_schema(&store, &tree.root_id, &vcs::project_coproduct_protocol())
                    .into_diagnostic()
            }
        }
    }
}

/// A loaded input, still in whichever shape the path denoted.
pub enum SchemaInput {
    /// One flat schema, with no per-file provenance to retain.
    Flat(Box<Schema>),
    /// A multi-file project.
    Project(Box<ProjectInput>),
}

/// A loaded input together with what the loader made of it.
pub struct LoadedInput {
    /// The input itself.
    pub input: SchemaInput,
    /// How the path was interpreted.
    pub kind: InputKind,
    /// The protocol the input is in, when the input or the caller names
    /// one. `None` for tree-sitter parses, whose protocol is the
    /// per-file language rather than a schema protocol.
    pub protocol: Option<String>,
}

/// A loaded input flattened to a single schema.
pub struct LoadedSchema {
    /// The flattened schema.
    pub schema: Schema,
    /// How the path was interpreted.
    pub kind: InputKind,
    /// The protocol the schema is in, when known.
    pub protocol: Option<String>,
}

impl LoadedSchema {
    /// The protocol this input names, when it disagrees with
    /// `requested`.
    ///
    /// A manifest-backed input has already been checked against the
    /// requested protocol and can never disagree here, so this reports
    /// only the softer case: a schema file whose own `protocol` field
    /// names something other than the protocol the command was asked to
    /// work in. That is worth reporting, since the schema was written
    /// for one protocol and is about to be read under the rules of
    /// another, but it is not fatal: the requested protocol still
    /// selects the classifier.
    #[must_use]
    pub fn conflicting_protocol(&self, requested: &str) -> Option<&str> {
        let declared = self.protocol.as_deref()?;
        (declared != normalize_protocol(requested)).then_some(declared)
    }
}

impl LoadedInput {
    /// Flatten this input to a single schema.
    ///
    /// A bundle assembles through the project coproduct, which names
    /// the assembled schema after the coproduct rather than after the
    /// protocol its files are all written in. Since every file in a
    /// bundle shares one protocol, the assembled schema is a schema in
    /// that protocol, and is stamped as such here so that downstream
    /// classification and validation resolve the right protocol.
    ///
    /// # Errors
    ///
    /// Returns an error if the project cannot be assembled.
    pub fn into_schema(self) -> Result<LoadedSchema> {
        let Self {
            input,
            kind,
            protocol,
        } = self;
        let mut schema = match input {
            SchemaInput::Flat(schema) => *schema,
            SchemaInput::Project(project) => project.build_schema()?,
        };
        let bundled = matches!(kind, InputKind::ManifestBundle | InputKind::ProtocolBundle);
        if bundled {
            if let Some(ref name) = protocol {
                schema.protocol.clone_from(name);
            }
        }
        Ok(LoadedSchema {
            schema,
            kind,
            protocol,
        })
    }
}

/// Load `path` and flatten it to a single schema.
///
/// # Errors
///
/// Returns an error if the path does not exist, cannot be parsed in any
/// shape the loader accepts, or names a protocol that disagrees with
/// its manifest.
pub fn load_schema(path: &Path, options: &LoadOptions<'_>) -> Result<LoadedSchema> {
    load(path, options)?.into_schema()
}

/// Load `path` in whichever shape it denotes.
///
/// # Errors
///
/// Returns an error if the path does not exist, cannot be parsed in any
/// shape the loader accepts, or names a protocol that disagrees with
/// its manifest.
pub fn load(path: &Path, options: &LoadOptions<'_>) -> Result<LoadedInput> {
    if path.is_dir() {
        load_directory(path, options)
    } else if path.is_file() {
        load_file(path, options)
    } else {
        miette::bail!(
            "path {} does not exist or is not a file/directory",
            path.display()
        )
    }
}

/// Load a directory: a manifest-backed bundle, an explicitly requested
/// bundle, or a tree-sitter parse of every file under it.
fn load_directory(dir: &Path, options: &LoadOptions<'_>) -> Result<LoadedInput> {
    if let Some(manifest) = find_bundle_manifest(dir)? {
        check_requested_protocol(&manifest, options.protocol)?;
        let paths = manifest_bundle_paths(&manifest, dir)?;
        if !paths.is_empty() {
            return bundle_input(
                dir,
                &paths,
                &manifest.protocol,
                InputKind::ManifestBundle,
                options.verbose,
            );
        }
    }

    if let Some(requested) = options.protocol {
        let normalized = normalize_protocol(requested);
        if !is_bundle_protocol(&normalized) {
            miette::bail!(
                "protocol {requested:?} has no bundle parser, so directory {} cannot be \
                 loaded as one; bundle protocols: {:?}",
                dir.display(),
                protocols::bundle_project_protocols()
            );
        }
        let mut paths = Vec::new();
        collect_json_files(dir, &empty_globset()?, &mut paths)?;
        if paths.is_empty() {
            miette::bail!(
                "no .json documents found under {} to parse as {requested}",
                dir.display()
            );
        }
        paths.sort();
        return bundle_input(
            dir,
            &paths,
            &normalized,
            InputKind::ProtocolBundle,
            options.verbose,
        );
    }

    load_source_tree(dir, options.verbose)
}

/// Parse a directory file by file through the tree-sitter registry.
fn load_source_tree(dir: &Path, verbose: bool) -> Result<LoadedInput> {
    let config = panproto_project::config::load_config(dir).into_diagnostic()?;
    let mut builder = match config {
        Some(ref cfg) => {
            panproto_project::ProjectBuilder::with_config(cfg, dir).into_diagnostic()?
        }
        None => panproto_project::ProjectBuilder::new(),
    };
    builder.add_directory(dir).into_diagnostic()?;
    if verbose {
        eprintln!(
            "Scanned {} files under {}",
            builder.file_count(),
            dir.display()
        );
    }
    Ok(LoadedInput {
        input: SchemaInput::Project(Box::new(ProjectInput {
            source: ProjectSource::Parsed(Box::new(builder)),
        })),
        kind: InputKind::SourceTree,
        protocol: None,
    })
}

/// Load a single file: panproto's own schema JSON, one schema document
/// in a named or manifest-declared protocol, or a source file.
fn load_file(path: &Path, options: &LoadOptions<'_>) -> Result<LoadedInput> {
    if path.extension().is_none_or(|ext| ext != "json") {
        return Ok(flat(
            parse_source_file(path, options.verbose)?,
            InputKind::SourceFile,
            None,
        ));
    }

    let value: serde_json::Value = read_json(path)?;
    if let Ok(schema) = serde_json::from_value::<Schema>(value.clone()) {
        if options.verbose {
            eprintln!("Loaded {} as an internal schema", path.display());
        }
        let protocol = normalize_protocol(&schema.protocol);
        return Ok(flat(schema, InputKind::InternalSchema, Some(protocol)));
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let manifest = find_bundle_manifest(parent)?;
    if let Some(ref manifest) = manifest {
        check_requested_protocol(manifest, options.protocol)?;
    }
    let declared = manifest
        .as_ref()
        .filter(|manifest| manifest_owns(manifest, path))
        .map(|manifest| manifest.protocol.clone());
    let Some(protocol) = declared.or_else(|| options.protocol.map(normalize_protocol)) else {
        return Ok(flat(
            parse_source_file(path, options.verbose)?,
            InputKind::SourceFile,
            None,
        ));
    };

    if options.verbose {
        eprintln!("Parsing {} as one {protocol} document", path.display());
    }
    let schema = protocols::parse_schema_document(&protocol, &value)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse {} as {protocol}", path.display()))?;
    Ok(flat(schema, InputKind::ProtocolDocument, Some(protocol)))
}

/// Wrap a flat schema as a [`LoadedInput`].
fn flat(schema: Schema, kind: InputKind, protocol: Option<String>) -> LoadedInput {
    LoadedInput {
        input: SchemaInput::Flat(Box::new(schema)),
        kind,
        protocol,
    }
}

/// Parse a single source file into a schema via tree-sitter.
fn parse_source_file(path: &Path, verbose: bool) -> Result<Schema> {
    let content = std::fs::read(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;
    let registry = panproto_parse::ParserRegistry::new();
    if verbose {
        let language = registry.detect_language(path).unwrap_or("raw_file");
        eprintln!("Parsing {} as {language}", path.display());
    }
    registry
        .parse_file(path, &content)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse {}", path.display()))
}

/// Parse `paths` as one bundle in `protocol`, keyed by each document's
/// path relative to `base`.
fn bundle_input(
    base: &Path,
    paths: &[PathBuf],
    protocol: &str,
    kind: InputKind,
    verbose: bool,
) -> Result<LoadedInput> {
    let mut docs = Vec::with_capacity(paths.len());
    for path in paths {
        let value = read_json(path)?;
        docs.push((relative_key(base, path), value));
    }
    docs.sort_by(|left, right| left.0.cmp(&right.0));
    if verbose {
        eprintln!(
            "Parsing {} document(s) under {} as one {protocol} bundle",
            docs.len(),
            base.display()
        );
    }

    let project = protocols::parse_schema_bundle_project(protocol, &docs)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse {protocol} project at {}", base.display()))?;
    let files: HashMap<PathBuf, Schema> = project.files.into_iter().collect();
    let protocol_map = files
        .keys()
        .map(|path| (path.clone(), protocol.to_owned()))
        .collect();
    Ok(LoadedInput {
        input: SchemaInput::Project(Box::new(ProjectInput {
            source: ProjectSource::Bundle {
                files,
                protocols: protocol_map,
                cross_file_edges: project.cross_file_edges,
            },
        })),
        kind,
        protocol: Some(protocol.to_owned()),
    })
}

/// A `panproto.toml` that declares one bundle protocol for every
/// package it lists.
struct BundleManifest {
    /// Directory the manifest lives in.
    root: PathBuf,
    /// The manifest itself.
    config: panproto_project::ProjectConfig,
    /// The protocol every package declares, normalized.
    protocol: String,
}

/// Find the nearest `panproto.toml` at or above `start` and return it
/// when it declares one bundle protocol for every package.
///
/// The search stops at the first manifest found: a manifest that names
/// no single bundle protocol shadows any manifest above it, since the
/// nearer manifest is the one that describes these files.
fn find_bundle_manifest(start: &Path) -> Result<Option<BundleManifest>> {
    let mut current = Some(start.to_path_buf());
    while let Some(dir) = current {
        let config = panproto_project::config::load_config(&dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read manifest in {}", dir.display()))?;
        if let Some(config) = config {
            return Ok(bundle_protocol(&config).map(|protocol| BundleManifest {
                root: dir,
                config,
                protocol,
            }));
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    Ok(None)
}

/// The single bundle protocol every package in `config` declares, if
/// there is one.
fn bundle_protocol(config: &panproto_project::ProjectConfig) -> Option<String> {
    let first = config.package.first()?.protocol.as_deref()?;
    let normalized = normalize_protocol(first);
    let homogeneous = config.package.iter().all(|package| {
        package
            .protocol
            .as_deref()
            .is_some_and(|other| normalize_protocol(other) == normalized)
    });
    (homogeneous && is_bundle_protocol(&normalized)).then_some(normalized)
}

/// Reject a `--protocol` that disagrees with the manifest.
///
/// The manifest is a declaration about the files themselves, so a
/// command-line name that contradicts it is a mistake in one of the two
/// rather than an override of the manifest.
fn check_requested_protocol(manifest: &BundleManifest, requested: Option<&str>) -> Result<()> {
    let Some(requested) = requested else {
        return Ok(());
    };
    let normalized = normalize_protocol(requested);
    if normalized == manifest.protocol {
        return Ok(());
    }
    miette::bail!(
        "protocol {requested:?} disagrees with {}, which declares {:?}; \
         drop --protocol or fix the manifest",
        manifest.root.join("panproto.toml").display(),
        manifest.protocol
    )
}

/// The documents `manifest` covers that also lie under `input`.
///
/// Every package is intersected with the input path, so pointing at the
/// manifest root loads every package while pointing at one package
/// subdirectory loads only that subdirectory. Files under the input
/// that no package covers, such as data fixtures, stay out of the
/// bundle.
fn manifest_bundle_paths(manifest: &BundleManifest, input: &Path) -> Result<Vec<PathBuf>> {
    let excludes = panproto_project::config::compile_excludes(
        &manifest.root,
        &manifest.config.workspace.exclude,
    )
    .into_diagnostic()?;
    let input_real = canonical(input)?;
    let mut paths = Vec::new();
    for package in &manifest.config.package {
        let package_dir = manifest.root.join(&package.path);
        if !package_dir.is_dir() {
            continue;
        }
        let package_real = canonical(&package_dir)?;
        let root = if package_real.starts_with(&input_real) {
            package_dir
        } else if input_real.starts_with(&package_real) {
            input.to_path_buf()
        } else {
            continue;
        };
        collect_json_files(&root, &excludes, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Whether `manifest` covers the single document at `path`.
fn manifest_owns(manifest: &BundleManifest, path: &Path) -> bool {
    let Ok(path_real) = canonical(path) else {
        return false;
    };
    manifest.config.package.iter().any(|package| {
        canonical(&manifest.root.join(&package.path))
            .is_ok_and(|package_real| path_real.starts_with(&package_real))
    })
}

/// Collect every `.json` file under `root`, skipping dot-prefixed and
/// excluded entries.
fn collect_json_files(
    root: &Path,
    excludes: &globset::GlobSet,
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(root)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read directory {}", root.display()))?;
    for entry in entries {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || excludes.is_match(&path) {
            continue;
        }
        if path.is_dir() {
            collect_json_files(&path, excludes, paths)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            paths.push(path);
        }
    }
    Ok(())
}

/// The key a document is filed under in a bundle: its path relative to
/// the input the caller named, so that two projects loaded from
/// different directories key their documents alike.
fn relative_key(base: &Path, path: &Path) -> PathBuf {
    if base.is_file() {
        return PathBuf::from(path.file_name().unwrap_or(path.as_os_str()));
    }
    let stripped = canonical(base)
        .ok()
        .zip(canonical(path).ok())
        .and_then(|(base, path)| path.strip_prefix(&base).map(Path::to_path_buf).ok());
    stripped.unwrap_or_else(|| path.strip_prefix(base).unwrap_or(path).to_path_buf())
}

/// Read and parse a JSON file.
fn read_json(path: &Path) -> Result<serde_json::Value> {
    let contents = std::fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&contents)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse JSON from {}", path.display()))
}

/// Resolve a path to its canonical form.
fn canonical(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to resolve {}", path.display()))
}

/// An empty exclude set, for a directory with no manifest to take
/// exclusions from.
fn empty_globset() -> Result<globset::GlobSet> {
    globset::GlobSetBuilder::new().build().into_diagnostic()
}

/// Normalize an underscore-keyed protocol name to its canonical
/// hyphenated form, matching the protocol registry's own dispatch.
fn normalize_protocol(name: &str) -> String {
    name.replace('_', "-")
}

/// Whether `name` names a protocol whose bundle parse retains per-file
/// provenance.
fn is_bundle_protocol(name: &str) -> bool {
    protocols::bundle_project_protocols().contains(&name)
}
