//! The per-book cache, mirroring a nvim-treesitter install.
//!
//! The system cache is keyed by revision and holds everything ever built; this
//! is the flat, human-readable view of just what one book uses:
//!
//! ```text
//! .cache/parsers/<language>.so
//! .cache/queries/<language>/*.scm
//! ```
//!
//! The `.so` extension is used on every platform, matching nvim-treesitter,
//! even though the file is a DLL on Windows and a dylib on macOS -- the loader
//! goes by path, not by extension.

use std::path::{Path, PathBuf};

use cu::pre::*;

/// A book's `.cache` directory.
pub struct LocalCache {
    root: PathBuf,
}

impl LocalCache {
    /// Use `root` as the cache directory; it is created on first write.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The cache directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a language's compiled parser lives.
    pub fn parser_path(&self, language: &str) -> PathBuf {
        self.root.join("parsers").join(format!("{language}.so"))
    }

    /// Where a language's query files live.
    pub fn query_dir(&self, language: &str) -> PathBuf {
        self.root.join("queries").join(language)
    }

    /// Copy a compiled parser in, replacing any previous copy.
    pub fn install_parser(&self, language: &str, from: &Path) -> cu::Result<PathBuf> {
        let to = self.parser_path(language);
        cu::fs::make_dir(to.parent().expect("parser path has a parent"))?;
        displace(&to);
        cu::check!(
            cu::fs::copy(from, &to),
            "failed to install the {language} parser"
        )?;
        Ok(to)
    }

    /// Copy a language's `.scm` files in, replacing any previous copy.
    pub fn install_queries(&self, language: &str, from: &Path) -> cu::Result<PathBuf> {
        let to = self.query_dir(language);
        cu::fs::make_dir_absent_or_empty(&to)?;

        for entry in cu::fs::read_dir(from)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "scm") {
                let name = path.file_name().expect("directory entry has a name");
                cu::check!(
                    cu::fs::copy(&path, to.join(name)),
                    "failed to install the {language} queries"
                )?;
            }
        }

        Ok(to)
    }
}

/// Get an existing parser out of the way, so a fresh one can be copied over it.
///
/// A concurrent build may have this `.so` loaded. On Unix unlinking it is
/// enough: the mapping keeps the inode alive until that process exits. On
/// Windows a mapped file can be neither deleted nor overwritten -- but it
/// *can* be renamed, and a delete after the rename is deferred until the last
/// handle closes. So the fallback moves it aside and asks for it to go when it
/// can. This is the dance nvim-treesitter's own installer does.
///
/// Every step is best effort: nothing here is worth failing a build over, and
/// if the copy afterwards cannot proceed it reports the real error itself.
fn displace(path: &Path) {
    if cu::fs::remove(path).is_ok() || !path.exists() {
        return;
    }

    // Unique per attempt: a second displacement in the same process must not
    // land on a name the first one left behind, still waiting to be deleted.
    static ATTEMPT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let attempt = ATTEMPT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let aside = path.with_file_name(format!("{name}.{}-{attempt}.old", std::process::id()));
    if cu::fs::rename(path, &aside).is_ok() {
        let _ = cu::fs::remove(&aside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nvimts-local-cache-{name}"));
        let _ = cu::fs::rec_remove(&path);
        path
    }

    #[test]
    fn lays_out_paths_like_a_nvim_treesitter_install() {
        let cache = LocalCache::at("/book/.cache");
        assert_eq!(
            cache.parser_path("c_sharp"),
            Path::new("/book/.cache/parsers/c_sharp.so")
        );
        assert_eq!(
            cache.query_dir("c_sharp"),
            Path::new("/book/.cache/queries/c_sharp")
        );
    }

    #[test]
    fn installs_and_replaces_a_parser() {
        let dir = temp_dir("parser");
        let source = dir.join("source");
        cu::fs::make_dir(&source).unwrap();
        cu::fs::write(source.join("parser.so"), "first").unwrap();

        let cache = LocalCache::at(dir.join("cache"));
        cache
            .install_parser("rust", &source.join("parser.so"))
            .unwrap();

        cu::fs::write(source.join("parser.so"), "second").unwrap();
        cache
            .install_parser("rust", &source.join("parser.so"))
            .unwrap();

        assert_eq!(
            cu::fs::read_string(cache.parser_path("rust")).unwrap(),
            "second"
        );
    }

    #[test]
    fn a_parser_that_cannot_be_removed_is_moved_aside_instead() {
        // The Windows case, reachable here only in its shape: `displace` has
        // to leave the path free either way, and must not fail if it cannot.
        let dir = temp_dir("displace");
        cu::fs::make_dir(&dir).unwrap();
        let path = dir.join("rust.so");
        cu::fs::write(&path, "loaded elsewhere").unwrap();

        displace(&path);
        assert!(!path.exists(), "the path must be free for the new parser");

        // And on something that was never there.
        displace(&dir.join("never-existed.so"));
    }

    #[test]
    fn installs_only_scm_files_and_drops_stale_ones() {
        let dir = temp_dir("queries");
        let source = dir.join("source");
        cu::fs::make_dir(&source).unwrap();
        cu::fs::write(source.join("highlights.scm"), "(x) @y").unwrap();
        cu::fs::write(source.join("README.md"), "not a query").unwrap();

        let cache = LocalCache::at(dir.join("cache"));
        cu::fs::make_dir(cache.query_dir("rust")).unwrap();
        cu::fs::write(cache.query_dir("rust").join("stale.scm"), "old").unwrap();

        cache.install_queries("rust", &source).unwrap();

        let installed: Vec<_> = cu::fs::read_dir(cache.query_dir("rust"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(installed, ["highlights.scm"]);
    }
}
