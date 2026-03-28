//! Project manifest (`panproto.toml`) loading, generation, and serialization.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::detect;
use crate::error::ProjectError;

/// Root manifest structure, deserialized from `panproto.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Workspace-level settings.
    pub workspace: WorkspaceConfig,
    /// Package declarations.
    #[serde(default)]
    pub package: Vec<PackageConfig>,
}

/// Workspace-level settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Human-readable workspace name.
    pub name: String,
    /// Glob patterns for files/directories to exclude from parsing.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// A package within the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    /// Package name (e.g., "panproto-gat").
    pub name: String,
    /// Path to the package root, relative to the workspace root.
    pub path: PathBuf,
    /// Protocol override for all files in this package.
    /// If absent, language detection proceeds as normal.
    #[serde(default)]
    pub protocol: Option<String>,
}

/// Load a `ProjectConfig` from a `panproto.toml` file in `dir`.
///
/// Returns `Ok(None)` if the file does not exist.
///
/// # Errors
///
/// Returns `ProjectError::InvalidManifest` if the file exists but cannot be parsed.
pub fn load_config(dir: &Path) -> Result<Option<ProjectConfig>, ProjectError> {
    let manifest_path = dir.join("panproto.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&manifest_path)?;
    let config: ProjectConfig =
        toml::from_str(&content).map_err(|e| ProjectError::InvalidManifest {
            path: manifest_path.display().to_string(),
            reason: e.to_string(),
        })?;
    Ok(Some(config))
}

/// Compile exclude patterns from the manifest into a `GlobSet` for efficient matching.
///
/// Patterns are resolved relative to `base` so that `"target"` matches
/// `<base>/target` and `"grammars/*/src/parser.c"` matches the expected paths.
///
/// # Errors
///
/// Returns `ProjectError::InvalidPattern` if a glob pattern is malformed.
pub fn compile_excludes(base: &Path, patterns: &[String]) -> Result<GlobSet, ProjectError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let full_pattern = base.join(pattern).display().to_string();
        let glob = Glob::new(&full_pattern).map_err(|e| ProjectError::InvalidPattern {
            pattern: pattern.clone(),
            reason: e.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|e| ProjectError::InvalidPattern {
        pattern: "<composite>".to_owned(),
        reason: e.to_string(),
    })
}

/// Generate a `ProjectConfig` by scanning `dir` for known package markers.
///
/// # Errors
///
/// Returns `ProjectError` if directory scanning fails.
pub fn generate_config(dir: &Path, name: &str) -> Result<ProjectConfig, ProjectError> {
    let packages = detect::scan_packages(dir)?;
    let package_configs: Vec<PackageConfig> = packages
        .into_iter()
        .map(|pkg| {
            let relative_path = pkg
                .path
                .strip_prefix(dir)
                .unwrap_or(&pkg.path)
                .to_path_buf();
            PackageConfig {
                name: pkg.name,
                path: relative_path,
                protocol: Some(pkg.protocol),
            }
        })
        .collect();

    Ok(ProjectConfig {
        workspace: WorkspaceConfig {
            name: name.to_owned(),
            exclude: vec![
                "target".to_owned(),
                "node_modules".to_owned(),
                "__pycache__".to_owned(),
                "build".to_owned(),
                "dist".to_owned(),
                ".git".to_owned(),
            ],
        },
        package: package_configs,
    })
}

/// Serialize a `ProjectConfig` to a TOML string.
///
/// # Errors
///
/// Returns `ProjectError::InvalidManifest` if serialization fails.
pub fn serialize_config(config: &ProjectConfig) -> Result<String, ProjectError> {
    toml::to_string_pretty(config).map_err(|e| ProjectError::InvalidManifest {
        path: "panproto.toml".to_owned(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_config() {
        let config = ProjectConfig {
            workspace: WorkspaceConfig {
                name: "test-project".to_owned(),
                exclude: vec!["target".to_owned(), "node_modules".to_owned()],
            },
            package: vec![
                PackageConfig {
                    name: "core".to_owned(),
                    path: PathBuf::from("crates/core"),
                    protocol: Some("rust".to_owned()),
                },
                PackageConfig {
                    name: "sdk".to_owned(),
                    path: PathBuf::from("sdk/typescript"),
                    protocol: Some("typescript".to_owned()),
                },
            ],
        };

        let toml_str = serialize_config(&config).unwrap();
        let parsed: ProjectConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.workspace.name, "test-project");
        assert_eq!(parsed.package.len(), 2);
        assert_eq!(parsed.package[0].name, "core");
        assert_eq!(parsed.package[1].protocol.as_deref(), Some("typescript"));
    }

    #[test]
    fn compile_excludes_builds_globset() {
        let base = Path::new("/tmp/project");
        let patterns = vec!["target".to_owned(), "**/*.log".to_owned()];
        let globset = compile_excludes(base, &patterns).unwrap();
        assert!(globset.is_match("/tmp/project/target"));
        assert!(globset.is_match("/tmp/project/logs/debug.log"));
        assert!(!globset.is_match("/tmp/project/src/main.rs"));
    }

    #[test]
    fn load_config_missing_file() {
        let result = load_config(Path::new("/nonexistent/path")).unwrap();
        assert!(result.is_none());
    }
}
