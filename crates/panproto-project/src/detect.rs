//! Language detection by file extension, shebang, and configuration.

use std::path::Path;

use panproto_parse::ParserRegistry;

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
