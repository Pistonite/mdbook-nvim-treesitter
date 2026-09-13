//! Installing the pinned nvim-treesitter queries.
//!
//! Only `runtime/queries` is needed -- the Lua plugin itself is irrelevant
//! here, and its parser registry is already baked into the `registry` module --
//! so the checkout is sparse. That takes a ~100 MiB repository down to a few
//! megabytes of `.scm` files.

use std::path::PathBuf;

use crate::registry::{
    self, NVIM_TREESITTER_QUERIES_DIR, NVIM_TREESITTER_REPOSITORY, NVIM_TREESITTER_REVISION,
};
use crate::store::fetch_util::checkout;
use crate::store::system_cache::SystemCache;

/// Install the pinned queries if needed and return the directory holding one
/// sub-directory per language.
pub fn ensure_queries(cache: &SystemCache) -> cu::Result<PathBuf> {
    let short = registry::nvim_treesitter_revision_short();
    let key = format!("nvim-treesitter-{short}");

    let dir = cache.ensure(&key, |dir| {
        eprintln!("mdbook-nvim-treesitter: setting up nvim-treesitter {short}");
        checkout(
            NVIM_TREESITTER_REPOSITORY,
            NVIM_TREESITTER_REVISION,
            &dir.join("checkout"),
            &[NVIM_TREESITTER_QUERIES_DIR],
        )
    })?;

    let queries = cu::path!(&dir / "checkout" / NVIM_TREESITTER_QUERIES_DIR);
    cu::ensure!(
        queries.is_dir(),
        "the nvim-treesitter cache at {} is missing its queries",
        dir.display()
    )?;

    Ok(queries)
}
