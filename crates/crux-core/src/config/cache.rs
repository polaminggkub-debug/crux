//! rkyv-based filter discovery cache for fast startup.
//!
//! Stores resolved filter manifests so repeated invocations skip directory
//! scanning. The cache is invalidated when any source directory or the binary
//! itself has a newer mtime than what was recorded.

#[cfg(feature = "cache")]
use rkyv::{Archive, Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[cfg(feature = "cache")]
#[derive(Archive, Serialize, Deserialize, Debug)]
#[archive(check_bytes)]
pub struct CacheManifest {
    /// Nanosecond timestamps of source directories at cache time.
    pub dir_mtimes: Vec<(String, u64)>,
    /// Binary mtime at cache time.
    pub binary_mtime: u64,
    /// Cached filter commands and their config TOML strings.
    pub entries: Vec<CacheEntry>,
}

#[cfg(feature = "cache")]
#[derive(Archive, Serialize, Deserialize, Debug)]
#[archive(check_bytes)]
pub struct CacheEntry {
    pub command: String,
    pub toml_content: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the cache file path: `$XDG_CACHE_HOME/crux/manifest.bin`
/// or `~/.cache/crux/manifest.bin`.
#[cfg(feature = "cache")]
pub fn cache_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(unix)]
            {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".cache"))
            }
            #[cfg(not(unix))]
            {
                None
            }
        })?;
    Some(base.join("crux").join("manifest.bin"))
}

/// Load and validate the cache against the current directory mtimes.
///
/// Returns `Some(manifest)` when every recorded directory mtime still matches
/// the filesystem; returns `None` if the cache is missing, corrupt, or stale.
#[cfg(feature = "cache")]
pub fn load_cache(search_dirs: &[&Path]) -> Option<CacheManifest> {
    let path = cache_path()?;
    let bytes = std::fs::read(&path).ok()?;
    let archived = rkyv::check_archived_root::<CacheManifest>(&bytes).ok()?;

    // Validate: every recorded dir must have the same mtime now.
    for (dir_str, recorded) in archived.dir_mtimes.iter() {
        let current = dir_mtime_nanos(Path::new(dir_str.as_str()));
        if current != *recorded {
            return None;
        }
    }

    let manifest: CacheManifest = archived.deserialize(&mut rkyv::Infallible).ok()?;

    // Extra check: search_dirs count must match.
    if manifest.dir_mtimes.len() != search_dirs.len() {
        return None;
    }

    Some(manifest)
}

/// Serialize and persist the manifest to disk.
#[cfg(feature = "cache")]
pub fn save_cache(manifest: &CacheManifest) -> anyhow::Result<()> {
    let path = cache_path().ok_or_else(|| anyhow::anyhow!("cannot determine cache path"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = rkyv::to_bytes::<_, 256>(manifest).map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::write(&path, &bytes)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Directory mtime as nanoseconds since the Unix epoch.
#[cfg(feature = "cache")]
fn dir_mtime_nanos(dir: &Path) -> u64 {
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(feature = "cache")]
mod tests {
    use super::*;

    #[test]
    fn cache_path_returns_valid_path() {
        let p = cache_path().expect("cache_path should return Some");
        assert!(p.ends_with("crux/manifest.bin"));
    }

    #[test]
    fn dir_mtime_nanos_nonzero_for_existing_dir() {
        let tmp = std::env::temp_dir();
        let ns = dir_mtime_nanos(&tmp);
        assert!(ns > 0, "mtime nanos should be > 0 for temp dir");
    }

    #[test]
    fn round_trip_save_load() {
        // Two temp dirs: one for the cache store, one as a stable "search dir".
        let cache_root = tempfile::tempdir().expect("create cache tempdir");
        let search_dir = tempfile::tempdir().expect("create search tempdir");

        // Record mtime of search_dir *before* any writes to cache_root.
        let mtime = dir_mtime_nanos(search_dir.path());

        // Point cache_path at cache_root via XDG_CACHE_HOME.
        std::env::set_var("XDG_CACHE_HOME", cache_root.path());

        let manifest = CacheManifest {
            dir_mtimes: vec![(search_dir.path().to_string_lossy().into_owned(), mtime)],
            binary_mtime: 123_456_789,
            entries: vec![CacheEntry {
                command: "git status".into(),
                toml_content: "[filter]\nname = \"git-status\"".into(),
            }],
        };

        save_cache(&manifest).expect("save_cache should succeed");

        let dirs: Vec<&Path> = vec![search_dir.path()];
        let loaded = load_cache(&dirs).expect("load_cache should return Some for fresh cache");

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].command, "git status");
        assert_eq!(loaded.binary_mtime, 123_456_789);
    }
}
