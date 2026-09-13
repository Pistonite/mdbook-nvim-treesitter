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
        // A parser may be loaded by another process; replacing the file is
        // fine, but writing through it is not, so remove first.
        let _ = cu::fs::remove(&to);
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
