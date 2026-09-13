//! The shape of one language table entry.

use crate::registry::util;

/// A language nvim-treesitter ships queries for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageInfo {
    /// The canonical nvim-treesitter language id, e.g. `c_sharp`.
    pub name: &'static str,
    /// Where the grammar comes from.
    ///
    /// `None` for query-only languages such as `ecma`, `jsx` and `html_tags`.
    /// Those have no grammar of their own; they exist so that other languages
    /// can pull their queries in with a `; inherits:` modeline.
    pub install: Option<Install>,
    /// Languages whose *queries* this language's queries inherit from.
    ///
    /// These need their query files present, but never their parsers: an
    /// inherited query is concatenated and compiled against *this* language's
    /// grammar.
    pub requires: &'static [&'static str],
    /// nvim-treesitter's maintenance tier. 1 is a core parser, higher is less
    /// closely maintained.
    pub tier: u8,
}

impl LanguageInfo {
    /// Whether this language has a grammar that can be compiled into a parser.
    pub fn has_parser(&self) -> bool {
        self.install.is_some()
    }
}

/// Where a grammar comes from and how to build it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Install {
    /// Repository URL, without a trailing `.git`.
    pub url: &'static str,
    /// The pinned revision: usually a full commit SHA, occasionally a tag.
    pub revision: &'static str,
    /// Branch to fetch, when the revision is not on the default branch.
    pub branch: Option<&'static str>,
    /// Sub-directory of the repo containing the grammar, for monorepos.
    pub location: Option<&'static str>,
    /// The repo ships no pre-generated `src/parser.c`, so `tree-sitter
    /// generate` has to run before the parser can be built.
    pub generate: bool,
}

impl Install {
    /// A short, filesystem-safe form of [`Self::revision`], for cache keys.
    ///
    /// Commit SHAs are truncated to 8 characters; tags (`v1.2.3`) are kept
    /// whole, since truncating them would collide across minor versions.
    pub fn short_revision(&self) -> &'static str {
        util::short_revision(self.revision)
    }
}
