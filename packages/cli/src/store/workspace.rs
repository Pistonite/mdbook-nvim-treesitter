//! Getting a book's parsers and queries into place.
//!
//! Bridges the two caches: the machine-wide one, which holds everything ever
//! built keyed by pinned revision, and the book's own, which holds a flat copy
//! of just what this book uses. The local copy is what the highlighter reads,
//! and it is laid out like a nvim-treesitter install so it can be inspected
//! when a highlight looks wrong.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::registry;
use crate::store::local_cache::LocalCache;
use crate::store::parser::ensure_parser;
use crate::store::queries::ensure_queries;
use crate::store::system_cache::SystemCache;
use crate::store::ts_cli::ensure_tree_sitter_cli;

/// A language that is ready to be loaded.
pub struct Installed {
    /// The parser in the book's cache.
    pub parser: PathBuf,
    /// The grammar name the parser exports, without the `tree_sitter_` prefix.
    pub symbol: String,
}

/// The caches a single book build works against.
pub struct Workspace {
    system: SystemCache,
    local: LocalCache,
    tree_sitter: PathBuf,
    system_queries: PathBuf,
}

impl Workspace {
    /// Prepare the pinned tooling and queries, ready to install languages.
    ///
    /// The machine-wide cache is wherever [`SystemCache::open`] puts it.
    pub fn open(cache_dir: PathBuf) -> cu::Result<Self> {
        Self::open_at(SystemCache::open()?, cache_dir)
    }

    /// As [`Self::open`], but against an explicit machine-wide cache.
    ///
    /// Tests use this to work in a scratch directory. Redirecting through the
    /// `MDBOOK_NVIM_TREESITTER_HOME` environment variable would do the same
    /// job, but it is process-global and the tests run in parallel.
    pub fn open_at(system: SystemCache, cache_dir: PathBuf) -> cu::Result<Self> {
        let tree_sitter = ensure_tree_sitter_cli(&system)?;
        let system_queries = ensure_queries(&system)?;

        Ok(Self {
            system,
            local: LocalCache::at(cache_dir),
            tree_sitter,
            system_queries,
        })
    }

    /// Where to look for query files, in order.
    ///
    /// The book's own copy comes first, so editing a query there takes effect;
    /// the shared checkout backs it up, so a query inherited from a language
    /// this book never installed still resolves.
    pub fn query_roots(&self) -> Vec<PathBuf> {
        vec![
            self.local.root().join("queries"),
            self.system_queries.clone(),
        ]
    }

    /// Install a language's queries and, if it has one, its parser.
    ///
    /// Returns `None` for a language with no grammar of its own -- `ecma` and
    /// `html_tags` exist only to be inherited from -- which is not a failure.
    pub fn install(&self, language: &str) -> cu::Result<Option<Installed>> {
        let Some(info) = registry::get_language(language) else {
            return Ok(None);
        };

        for name in query_dependencies(language) {
            let from = self.system_queries.join(name);
            if from.is_dir() {
                self.local.install_queries(name, &from)?;
            }
        }

        if info.install.is_none() {
            return Ok(None);
        }

        let built = ensure_parser(&self.system, &self.tree_sitter, info)?;
        let parser = self.local.install_parser(language, &built.library)?;

        Ok(Some(Installed {
            parser,
            symbol: built.symbol,
        }))
    }
}

/// A language and every language its queries inherit from, transitively.
///
/// `cpp/highlights.scm` starts with `; inherits: c`, and nvim-treesitter
/// records that as `requires`, so C's query files have to be installed
/// alongside C++'s even though C's *parser* is never needed for them.
fn query_dependencies(language: &str) -> BTreeSet<&'static str> {
    let mut found = BTreeSet::new();
    let mut pending = match registry::get_language(language) {
        Some(info) => vec![info.name],
        None => return found,
    };

    while let Some(name) = pending.pop() {
        if !found.insert(name) {
            continue;
        }
        if let Some(info) = registry::get_language(name) {
            pending.extend(info.requires.iter().copied());
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_always_depends_on_itself() {
        assert_eq!(query_dependencies("rust"), BTreeSet::from(["rust"]));
    }

    #[test]
    fn collects_inherited_query_languages() {
        assert_eq!(query_dependencies("cpp"), BTreeSet::from(["cpp", "c"]));
        assert_eq!(
            query_dependencies("javascript"),
            BTreeSet::from(["javascript", "ecma", "jsx"])
        );
    }

    #[test]
    fn follows_dependencies_transitively() {
        // tsx requires typescript, which in turn requires ecma.
        let dependencies = query_dependencies("tsx");
        assert!(dependencies.contains("typescript"));
        assert!(dependencies.contains("ecma"));
    }

    #[test]
    fn an_unknown_language_has_no_dependencies() {
        assert!(query_dependencies("not-a-language").is_empty());
    }
}
