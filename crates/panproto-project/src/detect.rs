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
        .and_then(panproto_grammars::extension_to_language)
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
        // group-core: python, javascript, typescript, java, csharp, cpp, php, bash, c, go, rust
        assert_eq!(detect_language(Path::new("lib.py")), Some("python"));
        assert_eq!(detect_language(Path::new("app.js")), Some("javascript"));
        assert_eq!(
            detect_language(Path::new("src/main.ts")),
            Some("typescript")
        );
        assert_eq!(detect_language(Path::new("App.java")), Some("java"));
        assert_eq!(detect_language(Path::new("Program.cs")), Some("csharp"));
        assert_eq!(detect_language(Path::new("stack.cpp")), Some("cpp"));
        assert_eq!(detect_language(Path::new("index.php")), Some("php"));
        assert_eq!(detect_language(Path::new("run.sh")), Some("bash"));
        assert_eq!(detect_language(Path::new("main.c")), Some("c"));
        assert_eq!(detect_language(Path::new("main.go")), Some("go"));
        assert_eq!(detect_language(Path::new("main.rs")), Some("rust"));
    }

    #[test]
    fn detect_unknown_returns_none() {
        assert_eq!(detect_language(Path::new("LICENSE")), None);
        assert_eq!(detect_language(Path::new("Makefile")), None);
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
