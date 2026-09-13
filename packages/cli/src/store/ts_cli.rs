//! Installing the pinned tree-sitter CLI.
//!
//! The CLI is what compiles grammars into loadable parsers. Upstream publishes
//! a prebuilt binary for every platform we support, so there is no need to
//! build it -- which also keeps the version exactly in step with the
//! tree-sitter runtime this preprocessor links against.

use std::path::PathBuf;

use crate::registry::{TREE_SITTER_CLI_VERSION, TREE_SITTER_REPOSITORY};

use crate::store::fetch_util::download_gzipped;
use crate::store::paths::{executable_name, make_executable};
use crate::store::system_cache::SystemCache;

/// The name of the executable inside the release archive.
const TREE_SITTER: &str = "tree-sitter";

/// Install the pinned CLI if needed and return the path to the executable.
pub fn ensure_tree_sitter_cli(cache: &SystemCache) -> cu::Result<PathBuf> {
    let key = format!("tree-sitter-cli-{TREE_SITTER_CLI_VERSION}");
    let platform = platform()?;

    let dir = cache.ensure(&key, |dir| {
        eprintln!("mdbook-nvim-treesitter: downloading tree-sitter {TREE_SITTER_CLI_VERSION} for {platform}");
        // GitHub's release-asset convention, not something we choose -- so it
        // stays here rather than in the metadata.
        let url = format!(
            "{TREE_SITTER_REPOSITORY}/releases/download/v{TREE_SITTER_CLI_VERSION}/tree-sitter-{platform}.gz",
        );
        let binary = dir.join(executable_name(TREE_SITTER));
        download_gzipped(&url, &binary)?;
        make_executable(&binary)
    })?;

    Ok(dir.join(executable_name(TREE_SITTER)))
}

/// The platform slug used in tree-sitter's release asset names.
fn platform() -> cu::Result<&'static str> {
    let slug = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        (os, arch) => cu::bail!(
            "no prebuilt tree-sitter CLI for {os}-{arch}; \
             install tree-sitter {TREE_SITTER_CLI_VERSION} manually and put it on PATH",
        ),
    };
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_platform_has_a_prebuilt_cli() {
        // If this fails on a new target, the fix is to add the slug above (or
        // to accept that the target needs a manually installed CLI).
        assert!(platform().is_ok());
    }
}
