//! Everything the preprocessor needs but does not ship: the tree-sitter CLI,
//! the nvim-treesitter queries, and one compiled parser per language.
//!
//! Two layers. The [system cache](SystemCache) is shared by every book on the
//! machine and is keyed by pinned version, so two books -- or two preprocessor
//! versions -- never fight over the same directory. The local cache lives
//! inside one book and holds the subset that book actually uses, laid out
//! exactly like a nvim-treesitter install so it can be inspected by hand.
//!
//! [`Workspace`] is the way in: open one for a book, ask it to install a
//! language, and it says where the parser and queries landed. Nothing outside
//! this module needs to know there are two layers at all.
//!
//! Everything here is deliberately synchronous and process-safe: builds are
//! serialised with an advisory file lock so that several mdBook invocations
//! can run at once without racing on a half-written parser.

/// Downloading and checking out pinned upstream sources.
mod fetch_util;
/// The advisory lock that serialises cache builds across processes.
mod file_lock;
/// One book's copy of the parsers and queries it uses.
mod local_cache;
/// Building one language's parser from its pinned grammar.
mod parser;
/// Filesystem facts that differ between platforms.
mod paths;
/// Installing the pinned nvim-treesitter queries.
mod queries;
/// The machine-wide cache of pinned tooling, queries and parsers.
mod system_cache;
/// Installing the pinned tree-sitter CLI.
mod ts_cli;
/// The driver: what a book build actually talks to.
mod workspace;

pub use system_cache::SystemCache;
pub use workspace::{Installed, Workspace};
