//! Incremental parsing cache for project assembly.
//!
//! Caches per-file schema parse results in `.panproto/cache/file_schemas.json`.
//! Files are invalidated when their mtime or size changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use panproto_schema::Schema;

use crate::error::ProjectError;

/// Cache of per-file schema parse results.
///
/// Stores parsed schemas keyed by file path, along with file metadata
/// used for invalidation (mtime, size, content hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCache {
    /// Per-file cache entries keyed by file path.
    pub entries: HashMap<PathBuf, CacheEntry>,
}

/// A single cached parse result for one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// File modification time as seconds since the Unix epoch.
    pub mtime_secs: u64,
    /// File size in bytes.
    pub size: u64,
    /// Blake3 hash of the file contents (hex-encoded).
    pub content_hash: String,
    /// The parsed schema for this file.
    pub schema: Schema,
    /// The protocol name used to parse this file.
    pub protocol: String,
}

impl FileCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl Default for FileCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Load the file cache from `.panproto/cache/file_schemas.json`.
///
/// Returns an empty cache if the file does not exist. Returns an error
/// only on actual I/O failures (permission denied, corrupt JSON, etc.).
///
/// # Errors
///
/// Returns [`ProjectError::Io`] on filesystem errors or
/// [`ProjectError::ParseFailed`] if the JSON is malformed.
pub fn load_cache(panproto_dir: &Path) -> Result<FileCache, ProjectError> {
    let cache_path = panproto_dir.join("cache").join("file_schemas.json");
    if !cache_path.exists() {
        return Ok(FileCache::new());
    }
    let data = std::fs::read_to_string(&cache_path)?;
    let cache: FileCache = serde_json::from_str(&data).map_err(|e| ProjectError::ParseFailed {
        path: cache_path.display().to_string(),
        reason: format!("cache JSON: {e}"),
    })?;
    Ok(cache)
}

/// Save the file cache to `.panproto/cache/file_schemas.json`.
///
/// Creates the `cache/` subdirectory if it does not already exist.
///
/// # Errors
///
/// Returns [`ProjectError::Io`] on filesystem errors.
pub fn save_cache(panproto_dir: &Path, cache: &FileCache) -> Result<(), ProjectError> {
    let cache_dir = panproto_dir.join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = cache_dir.join("file_schemas.json");
    let data = serde_json::to_string_pretty(cache).map_err(|e| ProjectError::ParseFailed {
        path: cache_path.display().to_string(),
        reason: format!("cache serialize: {e}"),
    })?;
    std::fs::write(&cache_path, data)?;
    Ok(())
}

/// Check whether a cache entry is still valid for the file at `path`.
///
/// Performs a two-level check:
/// 1. Fast path: if mtime (seconds) and size both match, the entry is valid.
/// 2. Slow path: if mtime or size differ, re-hash the file content and compare
///    against the stored content hash. This handles cases where `git checkout`
///    changes content without changing mtime in the same second.
///
/// If the file cannot be read, the entry is considered invalid.
#[must_use]
pub fn is_valid(entry: &CacheEntry, path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let size = metadata.len();

    // Fast path: metadata matches exactly.
    if entry.mtime_secs == mtime_secs && entry.size == size {
        return true;
    }

    // Slow path: size changed means content definitely changed.
    if entry.size != size {
        return false;
    }

    // Same size, different mtime: re-hash content to check.
    let Ok(content) = std::fs::read(path) else {
        return false;
    };
    let current_hash = blake3::hash(&content).to_string();
    current_hash == entry.content_hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    /// Build a minimal valid schema for testing.
    fn test_schema() -> Schema {
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThTestInst".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        };
        SchemaBuilder::new(&protocol)
            .vertex("root", "node", None)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn empty_cache_round_trip() {
        let dir = std::env::temp_dir().join("panproto_cache_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let cache = FileCache::new();
        save_cache(&dir, &cache).unwrap();
        let loaded = load_cache(&dir).unwrap();
        assert!(loaded.entries.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = std::env::temp_dir().join("panproto_cache_test_noexist");
        let _ = std::fs::remove_dir_all(&dir);

        let cache = load_cache(&dir).unwrap();
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn is_valid_matches_real_file() {
        let dir = std::env::temp_dir().join("panproto_cache_test_valid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, b"hello world").unwrap();

        let metadata = std::fs::metadata(&file_path).unwrap();
        let mtime_secs = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let size = metadata.len();

        let entry = CacheEntry {
            mtime_secs,
            size,
            content_hash: blake3::hash(b"hello world").to_string(),
            schema: test_schema(),
            protocol: "raw_file".to_owned(),
        };

        assert!(is_valid(&entry, &file_path));

        // Mutate the file so size changes.
        std::fs::write(&file_path, b"hello world, updated content!").unwrap();
        assert!(!is_valid(&entry, &file_path));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_valid_missing_file() {
        let entry = CacheEntry {
            mtime_secs: 0,
            size: 0,
            content_hash: String::new(),
            schema: test_schema(),
            protocol: "raw_file".to_owned(),
        };
        assert!(!is_valid(
            &entry,
            Path::new("/nonexistent/path/to/file.txt")
        ));
    }

    #[test]
    fn cache_with_entries_round_trip() {
        let dir = std::env::temp_dir().join("panproto_cache_test_entries");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut cache = FileCache::new();
        cache.entries.insert(
            PathBuf::from("src/main.rs"),
            CacheEntry {
                mtime_secs: 1_700_000_000,
                size: 42,
                content_hash: "abc123".to_owned(),
                schema: test_schema(),
                protocol: "rust".to_owned(),
            },
        );

        save_cache(&dir, &cache).unwrap();
        let loaded = load_cache(&dir).unwrap();
        assert_eq!(loaded.entries.len(), 1);

        let entry = loaded.entries.get(Path::new("src/main.rs")).unwrap();
        assert_eq!(entry.mtime_secs, 1_700_000_000);
        assert_eq!(entry.size, 42);
        assert_eq!(entry.content_hash, "abc123");
        assert_eq!(entry.protocol, "rust");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_impl() {
        let cache = FileCache::default();
        assert!(cache.entries.is_empty());
    }
}
