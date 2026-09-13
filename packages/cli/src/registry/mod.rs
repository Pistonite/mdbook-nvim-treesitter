//! The set of languages nvim-treesitter supports, locked to a pinned revision.
//!
//! The table itself is generated (see the `registry-build` package) so that
//! deciding whether a markdown fence tag names a real language is a binary
//! search over static data, with no I/O and no nvim-treesitter checkout needed.
//!
//! That it is generated is an implementation detail: [`db`] and [`pin`] are
//! hand-written, and the generated modules they read are private. This crate
//! has no dependencies at all, which is what makes it free to depend on.
mod info;
pub use info::*;
mod lookup;
pub use lookup::*;
mod util;

mod gen_;

pub use gen_::db::{FILETYPE_ALIASES, LANGUAGES};
pub use gen_::pin::{
    NVIM_TREESITTER_QUERIES_DIR, NVIM_TREESITTER_REPOSITORY, NVIM_TREESITTER_REVISION,
    TREE_SITTER_CLI_VERSION, TREE_SITTER_REPOSITORY,
};

pub fn nvim_treesitter_revision_short() -> &'static str {
    util::short_revision(NVIM_TREESITTER_REVISION)
}
