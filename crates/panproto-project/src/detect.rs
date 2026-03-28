//! Language detection by file extension, shebang, and configuration.

use std::path::{Path, PathBuf};

use panproto_parse::ParserRegistry;

use crate::error::ProjectError;

/// Detect the panproto protocol name for a file path.
///
/// Delegates to `ParserRegistry::detect_language`, which checks the file
/// extension against all registered grammar parsers. Returns `None` if the
/// file type is not recognized (caller should fall back to `raw_file`).
#[must_use]
pub fn detect_language<'a>(path: &Path, registry: &'a ParserRegistry) -> Option<&'a str> {
    registry.detect_language(path)
}

/// Check if a file should be treated as binary (not parsed as text).
#[must_use]
pub fn is_binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "ico"
                    | "bmp"
                    | "tiff"
                    | "pdf"
                    | "zip"
                    | "tar"
                    | "gz"
                    | "bz2"
                    | "xz"
                    | "7z"
                    | "rar"
                    | "wasm"
                    | "o"
                    | "so"
                    | "dylib"
                    | "dll"
                    | "exe"
                    | "bin"
                    | "class"
                    | "pyc"
                    | "pyo"
            )
        })
}

/// A detected package from filesystem markers.
#[derive(Debug, Clone)]
pub struct DetectedPackage {
    /// Inferred package name.
    pub name: String,
    /// Absolute path to the package root.
    pub path: PathBuf,
    /// Inferred protocol name (e.g., "rust", "typescript", "python").
    pub protocol: String,
}

/// Scan a directory for known package markers and return detected packages.
///
/// Recognized markers:
/// - `Cargo.toml` with `[package]` → Rust
/// - `package.json` with `"name"` → TypeScript/JavaScript
/// - `go.mod` with `module` line → Go
/// - `pyproject.toml` with `[project]` → Python
/// - `build.gradle` or `build.gradle.kts` → Java/Kotlin
/// - `mix.exs` → Elixir
/// - `CMakeLists.txt` → C/C++
///
/// Checks the root directory, then recurses one level into immediate
/// subdirectories and common workspace patterns (`crates/*`, `packages/*`,
/// `cmd/*`, `internal/*`, `sdk/*`, `libs/*`).
///
/// # Errors
///
/// Returns `ProjectError::Io` if directory scanning fails.
pub fn scan_packages(dir: &Path) -> Result<Vec<DetectedPackage>, ProjectError> {
    let mut packages = Vec::new();

    // Check the root directory itself.
    if let Some(pkg) = detect_single_package(dir) {
        packages.push(pkg);
    }

    // Check immediate subdirectories.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(packages);
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }

        // Check this subdirectory for a package marker.
        if let Some(pkg) = detect_single_package(&path) {
            packages.push(pkg);
            continue;
        }

        // Recurse into known workspace patterns.
        let is_workspace_dir = matches!(
            name_str.as_ref(),
            "crates" | "packages" | "cmd" | "internal" | "sdk" | "libs" | "apps"
        );
        if is_workspace_dir {
            let Ok(sub_entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for sub_entry in sub_entries {
                let sub_entry = sub_entry?;
                let sub_path = sub_entry.path();
                if sub_path.is_dir() {
                    if let Some(pkg) = detect_single_package(&sub_path) {
                        packages.push(pkg);
                    }
                }
            }
        }
    }

    packages.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(packages)
}

/// Detect a single package from known marker files in `dir`.
fn detect_single_package(dir: &Path) -> Option<DetectedPackage> {
    detect_rust_package(dir)
        .or_else(|| detect_node_package(dir))
        .or_else(|| detect_go_package(dir))
        .or_else(|| detect_python_package(dir))
        .or_else(|| detect_gradle_package(dir))
        .or_else(|| detect_elixir_package(dir))
        .or_else(|| detect_cmake_package(dir))
}

/// Detect a Rust package from `Cargo.toml`.
fn detect_rust_package(dir: &Path) -> Option<DetectedPackage> {
    let cargo_toml = dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).ok()?;
    let parsed = content.parse::<toml::Table>().ok()?;
    let pkg = parsed.get("package").and_then(toml::Value::as_table)?;
    let name = pkg.get("name").and_then(toml::Value::as_str)?;
    Some(DetectedPackage {
        name: name.to_owned(),
        path: dir.to_path_buf(),
        protocol: "rust".to_owned(),
    })
}

/// Detect a Node.js/TypeScript package from `package.json`.
fn detect_node_package(dir: &Path) -> Option<DetectedPackage> {
    let package_json = dir.join("package.json");
    let content = std::fs::read_to_string(&package_json).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let name = parsed.get("name").and_then(serde_json::Value::as_str)?;
    let protocol = if dir.join("tsconfig.json").exists() {
        "typescript"
    } else {
        "javascript"
    };
    Some(DetectedPackage {
        name: name.to_owned(),
        path: dir.to_path_buf(),
        protocol: protocol.to_owned(),
    })
}

/// Detect a Go module from `go.mod`.
fn detect_go_package(dir: &Path) -> Option<DetectedPackage> {
    let go_mod = dir.join("go.mod");
    let content = std::fs::read_to_string(&go_mod).ok()?;
    for line in content.lines() {
        if let Some(module_path) = line.strip_prefix("module ") {
            let trimmed = module_path.trim();
            let name = trimmed.rsplit('/').next().unwrap_or(trimmed);
            return Some(DetectedPackage {
                name: name.to_owned(),
                path: dir.to_path_buf(),
                protocol: "go".to_owned(),
            });
        }
    }
    None
}

/// Detect a Python package from `pyproject.toml`.
fn detect_python_package(dir: &Path) -> Option<DetectedPackage> {
    let pyproject = dir.join("pyproject.toml");
    let content = std::fs::read_to_string(&pyproject).ok()?;
    let parsed = content.parse::<toml::Table>().ok()?;
    let project = parsed.get("project").and_then(toml::Value::as_table)?;
    let name = project.get("name").and_then(toml::Value::as_str)?;
    Some(DetectedPackage {
        name: name.to_owned(),
        path: dir.to_path_buf(),
        protocol: "python".to_owned(),
    })
}

/// Detect a Gradle project from `build.gradle` or `build.gradle.kts`.
fn detect_gradle_package(dir: &Path) -> Option<DetectedPackage> {
    if !dir.join("build.gradle").exists() && !dir.join("build.gradle.kts").exists() {
        return None;
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_owned();
    let protocol = if dir.join("build.gradle.kts").exists() {
        "kotlin"
    } else {
        "java"
    };
    Some(DetectedPackage {
        name,
        path: dir.to_path_buf(),
        protocol: protocol.to_owned(),
    })
}

/// Detect an Elixir project from `mix.exs`.
fn detect_elixir_package(dir: &Path) -> Option<DetectedPackage> {
    if !dir.join("mix.exs").exists() {
        return None;
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_owned();
    Some(DetectedPackage {
        name,
        path: dir.to_path_buf(),
        protocol: "elixir".to_owned(),
    })
}

/// Detect a `CMake` project from `CMakeLists.txt`.
fn detect_cmake_package(dir: &Path) -> Option<DetectedPackage> {
    if !dir.join("CMakeLists.txt").exists() {
        return None;
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_owned();
    Some(DetectedPackage {
        name,
        path: dir.to_path_buf(),
        protocol: "cpp".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_core_languages() {
        let registry = ParserRegistry::new();
        assert_eq!(
            detect_language(Path::new("lib.py"), &registry),
            Some("python")
        );
        assert_eq!(
            detect_language(Path::new("main.rs"), &registry),
            Some("rust")
        );
        assert_eq!(detect_language(Path::new("main.go"), &registry), Some("go"));
    }

    #[test]
    fn detect_unknown_returns_none() {
        let registry = ParserRegistry::new();
        assert_eq!(detect_language(Path::new("LICENSE"), &registry), None);
        assert_eq!(detect_language(Path::new("Makefile"), &registry), None);
    }

    #[test]
    fn binary_detection() {
        assert!(is_binary_extension(Path::new("photo.png")));
        assert!(is_binary_extension(Path::new("app.wasm")));
        assert!(is_binary_extension(Path::new("archive.zip")));
        assert!(!is_binary_extension(Path::new("main.rs")));
        assert!(!is_binary_extension(Path::new("README.md")));
    }
}
