//! The machine-wide cache of pinned tooling, queries and parsers.
//!
//! Every entry lives at `<root>/<key>/`, where the key embeds the pinned
//! version of whatever it holds (`tree-sitter-cli-0.27.0`,
//! `parser-rust-77a37472`). Because the version is in the key, upgrading the
//! preprocessor never invalidates an entry in place and two versions can share
//! a cache without corrupting each other.
//!
//! An entry is only trusted once a `.ready` marker exists inside it, so a build
//! interrupted halfway is re-done rather than silently used.

use std::path::{Path, PathBuf};

use cu::pre::*;

use crate::store::file_lock::FileLock;
use crate::store::paths::home_dir;

/// Environment variable that relocates the cache, on every platform.
pub const HOME_ENV: &str = "MDBOOK_NVIM_TREESITTER_HOME";

/// Marker file written inside an entry once it is complete.
const READY_MARKER: &str = ".ready";

/// A directory of version-keyed cache entries shared by every book.
pub struct SystemCache {
    root: PathBuf,
}

impl SystemCache {
    /// Open the cache at its default location, or wherever
    /// `MDBOOK_NVIM_TREESITTER_HOME` points.
    pub fn open() -> cu::Result<Self> {
        if let Some(root) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::at(PathBuf::from(root)));
        }
        let home = cu::check!(
            home_dir(),
            "cannot find your home directory -- set {HOME_ENV} to choose a cache location"
        )?;
        Ok(Self::at(home.join(".cache").join("mdbook-nvim-treesitter")))
    }

    /// Open the cache rooted at an explicit directory.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory holding all entries.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory an entry occupies, whether or not it exists yet.
    pub fn path_of(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    /// The lock file guarding `key`.
    ///
    /// A sibling of the entry directory rather than a file inside it, so that
    /// wiping a half-built entry never deletes the lock being held on it.
    fn lock_path_for(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.lock"))
    }

    /// Whether `key` holds a complete entry.
    pub fn is_ready(&self, key: &str) -> bool {
        self.path_of(key).join(READY_MARKER).is_file()
    }

    /// Return the directory for `key`, building it with `build` if needed.
    ///
    /// `build` is handed an empty directory to populate and is called at most
    /// once across all processes on the machine: the key is locked first, and
    /// the readiness check is repeated under the lock so that a build that
    /// finished while we waited is picked up instead of redone.
    pub fn ensure<F>(&self, key: &str, build: F) -> cu::Result<PathBuf>
    where
        F: FnOnce(&Path) -> cu::Result<()>,
    {
        let path = self.path_of(key);
        if self.is_ready(key) {
            return Ok(path);
        }

        let _lock = FileLock::acquire(self.lock_path_for(key))?;
        if self.is_ready(key) {
            return Ok(path);
        }

        // Anything already here is the debris of an interrupted build.
        cu::fs::make_dir_absent_or_empty(&path)?;
        cu::check!(build(&path), "failed to populate cache entry `{key}`")?;
        cu::fs::write(path.join(READY_MARKER), key)?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nvimts-system-cache-{name}"));
        let _ = cu::fs::rec_remove(&path);
        path
    }

    #[test]
    fn entry_paths_are_the_key_under_the_root() {
        let cache = SystemCache::at("/cache");
        assert_eq!(
            cache.path_of("parser-rust-77a37472").as_path(),
            Path::new("/cache/parser-rust-77a37472")
        );
    }

    #[test]
    fn builds_an_entry_then_reuses_it() {
        let cache = SystemCache::at(temp_dir("reuse"));
        let mut builds = 0;

        for _ in 0..2 {
            cache
                .ensure("thing", |dir| {
                    builds += 1;
                    cu::fs::write(dir.join("payload"), "hello")
                })
                .unwrap();
        }

        assert_eq!(builds, 1);
        assert_eq!(
            cu::fs::read_string(cache.path_of("thing").join("payload")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn a_failed_build_is_not_marked_ready() {
        let cache = SystemCache::at(temp_dir("failed"));
        let error = cache
            .ensure("thing", |dir| {
                cu::fs::write(dir.join("half-written"), "oops")?;
                cu::bail!("build blew up")
            })
            .unwrap_err();

        assert!(format!("{error:#}").contains("build blew up"));
        assert!(!cache.is_ready("thing"));
    }

    #[test]
    fn a_partial_entry_is_wiped_before_rebuilding() {
        let cache = SystemCache::at(temp_dir("partial"));
        let _ = cache.ensure("thing", |dir| {
            cu::fs::write(dir.join("stale"), "junk")?;
            cu::bail!("interrupted")
        });

        cache
            .ensure("thing", |dir| cu::fs::write(dir.join("good"), "ok"))
            .unwrap();

        assert!(!cache.path_of("thing").join("stale").exists());
        assert!(cache.is_ready("thing"));
    }
}
