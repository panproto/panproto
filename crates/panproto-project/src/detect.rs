//! Language detection by file extension, shebang, and configuration.

use std::path::Path;

/// Detect the panproto protocol name for a file path.
///
/// Returns `None` if the file type is not recognized (caller should
/// fall back to the `raw_file` protocol).
#[must_use]
pub fn detect_language(path: &Path) -> Option<&'static str> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| language_from_extension(ext))
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

/// Map file extension to protocol name.
fn language_from_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "py" | "pyi" => Some("python"),
        "rs" => Some("rust"),
        "java" => Some("java"),
        "go" => Some("go"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "cs" => Some("csharp"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_common_languages() {
        assert_eq!(
            detect_language(Path::new("src/main.ts")),
            Some("typescript")
        );
        assert_eq!(detect_language(Path::new("app.tsx")), Some("tsx"));
        assert_eq!(detect_language(Path::new("lib.py")), Some("python"));
        assert_eq!(detect_language(Path::new("main.rs")), Some("rust"));
        assert_eq!(detect_language(Path::new("App.java")), Some("java"));
        assert_eq!(detect_language(Path::new("main.go")), Some("go"));
        assert_eq!(detect_language(Path::new("Point.swift")), Some("swift"));
        assert_eq!(detect_language(Path::new("User.kt")), Some("kotlin"));
        assert_eq!(detect_language(Path::new("Program.cs")), Some("csharp"));
        assert_eq!(detect_language(Path::new("main.c")), Some("c"));
        assert_eq!(detect_language(Path::new("stack.cpp")), Some("cpp"));
        assert_eq!(detect_language(Path::new("utils.h")), Some("c"));
        assert_eq!(detect_language(Path::new("utils.hpp")), Some("cpp"));
    }

    #[test]
    fn detect_unknown_returns_none() {
        assert_eq!(detect_language(Path::new("README.md")), None);
        assert_eq!(detect_language(Path::new("LICENSE")), None);
        assert_eq!(detect_language(Path::new(".gitignore")), None);
        assert_eq!(detect_language(Path::new("Makefile")), None);
        assert_eq!(detect_language(Path::new("data.json")), None);
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
