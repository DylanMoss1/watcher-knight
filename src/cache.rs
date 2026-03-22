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
