use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::claude::WatcherResult;
use crate::marker::Marker;

const CACHE_DIR: &str = ".watcher_knight";
const CACHE_FILE: &str = ".watcher_knight/cache.json";

#[derive(Serialize, Deserialize)]
pub struct CacheEntry {
    pub marker_hash: u64,
    pub file_hashes: HashMap<String, u64>,
    pub is_valid: bool,
    pub reason: Option<String>,
}

pub type Cache = HashMap<String, CacheEntry>;

pub fn load_cache() -> Cache {
    match fs::read_to_string(CACHE_FILE) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_cache(cache: &Cache) {
    fs::create_dir_all(CACHE_DIR).ok();
    let data = serde_json::to_string_pretty(cache).unwrap();
    fs::write(CACHE_FILE, data).ok();
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Hash a marker's instruction and options together so that changing either
/// invalidates the cache.
fn marker_content_hash(marker: &Marker) -> u64 {
    let mut hasher = DefaultHasher::new();
    marker.instruction.hash(&mut hasher);
    let mut opts: Vec<_> = marker.options.iter().collect();
    opts.sort_by_key(|(k, _)| (*k).clone());
    for (k, v) in opts {
        k.hash(&mut hasher);
        v.hash(&mut hasher);
    }
    hasher.finish()
}

fn cache_key(marker: &Marker) -> String {
    format!("{}::{}", marker.name, marker.rel_path)
}

fn hash_watched_files(marker: &Marker, root: &Path) -> HashMap<String, u64> {
    let mut hashes = HashMap::new();
    for file in &marker.files {
        let path = root.join(file);
        if let Ok(contents) = fs::read_to_string(&path) {
            hashes.insert(file.clone(), hash_string(&contents));
        }
    }
    hashes
}

/// Check if a marker's cached result is still valid.
/// Returns None if cache miss, Some(CacheEntry) if hit.
pub fn check_cache<'a>(marker: &Marker, cache: &'a Cache, root: &Path) -> Option<&'a CacheEntry> {
    // Unscoped watchers (no files) always re-run
    if marker.files.is_empty() {
        return None;
    }

    let key = cache_key(marker);
    let entry = cache.get(&key)?;

    // Check marker instruction hash
    if entry.marker_hash != marker_content_hash(marker) {
        return None;
    }

    // Check all watched file hashes
    let current_hashes = hash_watched_files(marker, root);
    if current_hashes != entry.file_hashes {
        return None;
    }

    Some(entry)
}

/// Build a cache entry from a watcher result.
pub fn build_entry(marker: &Marker, result: &WatcherResult, root: &Path) -> (String, CacheEntry) {
    let key = cache_key(marker);
    let entry = CacheEntry {
        marker_hash: marker_content_hash(marker),
        file_hashes: hash_watched_files(marker, root),
        is_valid: result.is_valid,
        reason: result.reason.clone(),
    };
    (key, entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_marker(name: &str, instruction: &str, files: Vec<String>) -> Marker {
        Marker {
            name: name.to_string(),
            rel_path: "src/app.ts".to_string(),
            line: 1,
            instruction: instruction.to_string(),
            files,
            options: HashMap::new(),
        }
    }

    fn make_result(is_valid: bool, reason: Option<&str>) -> WatcherResult {
        WatcherResult {
            name: "test".to_string(),
            location: "f:1".to_string(),
            is_valid,
            reason: reason.map(|s| s.to_string()),
            cached: false,
        }
    }

    // ── hash_string ───────────────────────────────────────────────────────

    #[test]
    fn hash_string_deterministic() {
        assert_eq!(hash_string("hello"), hash_string("hello"));
    }

    #[test]
    fn hash_string_different_inputs() {
        assert_ne!(hash_string("hello"), hash_string("world"));
    }

    #[test]
    fn hash_string_empty() {
        // Should not panic, should produce a valid u64
        let _ = hash_string("");
    }

    // ── marker_content_hash ───────────────────────────────────────────────

    #[test]
    fn marker_content_hash_deterministic() {
        let m = make_marker("w", "Check it", vec![]);
        assert_eq!(marker_content_hash(&m), marker_content_hash(&m));
    }

    #[test]
    fn marker_content_hash_changes_on_instruction_change() {
        let m1 = make_marker("w", "Check A", vec![]);
        let m2 = make_marker("w", "Check B", vec![]);
        assert_ne!(marker_content_hash(&m1), marker_content_hash(&m2));
    }

    #[test]
    fn marker_content_hash_changes_on_options_change() {
        let mut m1 = make_marker("w", "Check it", vec![]);
        let mut m2 = make_marker("w", "Check it", vec![]);
        m1.options.insert("model".to_string(), "haiku".to_string());
        m2.options.insert("model".to_string(), "opus".to_string());
        assert_ne!(marker_content_hash(&m1), marker_content_hash(&m2));
    }

    #[test]
    fn marker_content_hash_options_order_independent() {
        let mut m1 = make_marker("w", "Check it", vec![]);
        m1.options.insert("a".to_string(), "1".to_string());
        m1.options.insert("b".to_string(), "2".to_string());

        let mut m2 = make_marker("w", "Check it", vec![]);
        m2.options.insert("b".to_string(), "2".to_string());
        m2.options.insert("a".to_string(), "1".to_string());

        assert_eq!(marker_content_hash(&m1), marker_content_hash(&m2));
    }

    #[test]
    fn marker_content_hash_ignores_name_and_path() {
        let m1 = make_marker("name1", "Check it", vec![]);
        let mut m2 = make_marker("name2", "Check it", vec![]);
        m2.rel_path = "other.ts".to_string();
        assert_eq!(marker_content_hash(&m1), marker_content_hash(&m2));
    }

    // ── cache_key ─────────────────────────────────────────────────────────

    #[test]
    fn cache_key_format() {
        let m = make_marker("my-watcher", "Check it", vec![]);
        assert_eq!(cache_key(&m), "my-watcher::src/app.ts");
    }

    #[test]
    fn cache_key_unique_per_name() {
        let m1 = make_marker("a", "Check it", vec![]);
        let m2 = make_marker("b", "Check it", vec![]);
        assert_ne!(cache_key(&m1), cache_key(&m2));
    }

    #[test]
    fn cache_key_unique_per_path() {
        let mut m1 = make_marker("w", "Check it", vec![]);
        let mut m2 = make_marker("w", "Check it", vec![]);
        m1.rel_path = "a.ts".to_string();
        m2.rel_path = "b.ts".to_string();
        assert_ne!(cache_key(&m1), cache_key(&m2));
    }

    // ── check_cache ───────────────────────────────────────────────────────

    #[test]
    fn check_cache_miss_empty_cache() {
        let m = make_marker("w", "Check it", vec!["file.ts".to_string()]);
        let cache = Cache::new();
        assert!(check_cache(&m, &cache, Path::new("/repo")).is_none());
    }

    #[test]
    fn check_cache_miss_unscoped_marker() {
        let m = make_marker("w", "Check it", vec![]);
        let mut cache = Cache::new();
        cache.insert(
            "w::src/app.ts".to_string(),
            CacheEntry {
                marker_hash: marker_content_hash(&m),
                file_hashes: HashMap::new(),
                is_valid: true,
                reason: None,
            },
        );
        // Unscoped markers always miss
        assert!(check_cache(&m, &cache, Path::new("/repo")).is_none());
    }

    #[test]
    fn check_cache_hit_valid() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.ts");
        fs::write(&file_path, "content").unwrap();

        let m = make_marker("w", "Check it", vec!["file.ts".to_string()]);
        let content_hash = marker_content_hash(&m);
        let file_hashes = hash_watched_files(&m, dir.path());

        let mut cache = Cache::new();
        cache.insert(
            cache_key(&m),
            CacheEntry {
                marker_hash: content_hash,
                file_hashes,
                is_valid: true,
                reason: None,
            },
        );

        let entry = check_cache(&m, &cache, dir.path()).unwrap();
        assert!(entry.is_valid);
    }

    #[test]
    fn check_cache_miss_instruction_changed() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.ts");
        fs::write(&file_path, "content").unwrap();

        let m_old = make_marker("w", "Old instruction", vec!["file.ts".to_string()]);
        let m_new = make_marker("w", "New instruction", vec!["file.ts".to_string()]);

        let mut cache = Cache::new();
        cache.insert(
            cache_key(&m_old),
            CacheEntry {
                marker_hash: marker_content_hash(&m_old),
                file_hashes: hash_watched_files(&m_old, dir.path()),
                is_valid: true,
                reason: None,
            },
        );

        assert!(check_cache(&m_new, &cache, dir.path()).is_none());
    }

    #[test]
    fn check_cache_miss_file_content_changed() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.ts");
        fs::write(&file_path, "original").unwrap();

        let m = make_marker("w", "Check it", vec!["file.ts".to_string()]);
        let content_hash = marker_content_hash(&m);
        let old_hashes = hash_watched_files(&m, dir.path());

        let mut cache = Cache::new();
        cache.insert(
            cache_key(&m),
            CacheEntry {
                marker_hash: content_hash,
                file_hashes: old_hashes,
                is_valid: true,
                reason: None,
            },
        );

        // Modify the file
        fs::write(&file_path, "modified").unwrap();
        assert!(check_cache(&m, &cache, dir.path()).is_none());
    }

    #[test]
    fn check_cache_miss_file_added_to_scope() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.ts"), "a").unwrap();
        fs::write(dir.path().join("b.ts"), "b").unwrap();

        let m_old = make_marker("w", "Check it", vec!["a.ts".to_string()]);
        let m_new = make_marker(
            "w",
            "Check it",
            vec!["a.ts".to_string(), "b.ts".to_string()],
        );

        let mut cache = Cache::new();
        cache.insert(
            cache_key(&m_old),
            CacheEntry {
                marker_hash: marker_content_hash(&m_old),
                file_hashes: hash_watched_files(&m_old, dir.path()),
                is_valid: true,
                reason: None,
            },
        );

        // m_new has an extra file, so file_hashes won't match
        assert!(check_cache(&m_new, &cache, dir.path()).is_none());
    }

    // ── build_entry ───────────────────────────────────────────────────────

    #[test]
    fn build_entry_valid_result() {
        let m = make_marker("w", "Check it", vec![]);
        let r = make_result(true, None);
        let (key, entry) = build_entry(&m, &r, Path::new("/repo"));
        assert_eq!(key, "w::src/app.ts");
        assert!(entry.is_valid);
        assert!(entry.reason.is_none());
    }

    #[test]
    fn build_entry_failed_result() {
        let m = make_marker("w", "Check it", vec![]);
        let r = make_result(false, Some("broken"));
        let (_, entry) = build_entry(&m, &r, Path::new("/repo"));
        assert!(!entry.is_valid);
        assert_eq!(entry.reason.as_deref(), Some("broken"));
    }

    #[test]
    fn build_entry_file_hashes_populated() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file.ts"), "content").unwrap();

        let m = make_marker("w", "Check it", vec!["file.ts".to_string()]);
        let r = make_result(true, None);
        let (_, entry) = build_entry(&m, &r, dir.path());
        assert!(entry.file_hashes.contains_key("file.ts"));
    }

    // ── load_cache / save_cache ───────────────────────────────────────────
    // These use the hardcoded CACHE_DIR/CACHE_FILE paths so we test
    // the serialization logic directly instead.

    #[test]
    fn cache_serialization_roundtrip() {
        let mut cache = Cache::new();
        cache.insert(
            "w::f.ts".to_string(),
            CacheEntry {
                marker_hash: 12345,
                file_hashes: HashMap::from([("f.ts".to_string(), 67890)]),
                is_valid: true,
                reason: None,
            },
        );

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: Cache = serde_json::from_str(&json).unwrap();
        assert!(loaded.contains_key("w::f.ts"));
        let entry = loaded.get("w::f.ts").unwrap();
        assert_eq!(entry.marker_hash, 12345);
        assert!(entry.is_valid);
    }

    #[test]
    fn cache_deserialization_empty_json() {
        let loaded: Cache = serde_json::from_str("{}").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn cache_deserialization_corrupt_json() {
        let result: Result<Cache, _> = serde_json::from_str("not json");
        assert!(result.is_err());
        // In load_cache, this would fall back to default (empty)
        let fallback: Cache = result.unwrap_or_default();
        assert!(fallback.is_empty());
    }

    #[test]
    fn cache_entry_with_reason() {
        let mut cache = Cache::new();
        cache.insert(
            "w::f.ts".to_string(),
            CacheEntry {
                marker_hash: 1,
                file_hashes: HashMap::new(),
                is_valid: false,
                reason: Some("something broke".to_string()),
            },
        );

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: Cache = serde_json::from_str(&json).unwrap();
        let entry = loaded.get("w::f.ts").unwrap();
        assert!(!entry.is_valid);
        assert_eq!(entry.reason.as_deref(), Some("something broke"));
    }

    // ── hash_watched_files ────────────────────────────────────────────────

    #[test]
    fn hash_watched_files_existing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.ts"), "aaa").unwrap();
        fs::write(dir.path().join("b.ts"), "bbb").unwrap();

        let m = make_marker(
            "w",
            "Check",
            vec!["a.ts".to_string(), "b.ts".to_string()],
        );
        let hashes = hash_watched_files(&m, dir.path());
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains_key("a.ts"));
        assert!(hashes.contains_key("b.ts"));
    }

    #[test]
    fn hash_watched_files_missing_file_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.ts"), "aaa").unwrap();

        let m = make_marker(
            "w",
            "Check",
            vec!["a.ts".to_string(), "nonexistent.ts".to_string()],
        );
        let hashes = hash_watched_files(&m, dir.path());
        assert_eq!(hashes.len(), 1);
        assert!(hashes.contains_key("a.ts"));
        assert!(!hashes.contains_key("nonexistent.ts"));
    }

    #[test]
    fn hash_watched_files_empty_files() {
        let m = make_marker("w", "Check", vec![]);
        let hashes = hash_watched_files(&m, Path::new("/repo"));
        assert!(hashes.is_empty());
    }

    #[test]
    fn hash_watched_files_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("f.ts"), "content").unwrap();

        let m = make_marker("w", "Check", vec!["f.ts".to_string()]);
        let h1 = hash_watched_files(&m, dir.path());
        let h2 = hash_watched_files(&m, dir.path());
        assert_eq!(h1, h2);
    }
}
