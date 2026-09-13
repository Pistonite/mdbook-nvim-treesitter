//! An mdBook preprocessor that highlights code with nvim-treesitter's queries.
//!
//! The library target exists so the heavier parts can be tested on their own;
//! the binary is the only real consumer. What is public here is what the
//! integration tests in `tests/` drive -- deliberately the same two entry
//! points the binary uses.

/// Arguments in, a transformed book out.
pub mod driver;
/// Downloading, building and caching parsers and queries.
pub mod store;

/// Highlight engine that executes the queries
mod highlight;
pub use highlight::{Highlighted, Highlighter, Overrides, Span, strip_overrides};
/// Neovim compatible tree-sitter query runtime
mod query;
pub use query::QuerySource;
/// Language registry and metadata
mod registry;
mod renderer;
