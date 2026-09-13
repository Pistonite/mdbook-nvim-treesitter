//! Reading the nvim-treesitter tables straight from the pinned revision.
//!
//! A full 40-character SHA in a raw.githubusercontent.com path is
//! content-addressed and immutable, so fetching is exactly as reproducible as
//! a checkout would be -- the revision *is* the checksum -- and it removes the
//! one way the generated table could come out wrong: reading a checkout that
//! is not at the revision being stamped into it.
//!
//! Which repository, which revision and which paths all come from
//! [`metadata.json`](crate::metadata). The only thing this module knows on its
//! own is how to turn a GitHub repository url into its raw-file host, which
//! follows from the repository rather than being a separate choice.

use cu::pre::*;

/// Fetch a file from a revision from remote repository
pub fn get_remote(path: &str, repository: &str, revision: &str) -> cu::Result<String> {
    let url = format!("{}/{revision}/{path}", raw_host(repository)?);
    let mut response = cu::check!(ureq::get(&url).call(), "failed to request {url}")?;
    let status = response.status();
    cu::ensure!(
        status.is_success(),
        "{url} responded with HTTP {status}. A 404 here usually means the \
         revision has not been pushed, or the file has moved upstream"
    )?;

    Ok(response.body_mut().read_to_string()?)
}

/// Get a file from local source
pub fn get_local(path: &str, local_source: &str) -> cu::Result<String> {
    // <BY_HUMAN>
    cu::warn!("using local nvim_treesitter: {local_source}/{path}");
    cu::fs::read_string(cu::path!(&(local_source) / path))
}

/// The raw-file host for a github.com repository.
///
/// `metadata.json` is validated to hold exactly this shape, so the error here
/// is unreachable from the committed file -- it exists for the case where
/// someone widens that validation without revisiting this.
fn raw_host(repository: &str) -> cu::Result<String> {
    let path = cu::check!(
        repository.strip_prefix("https://github.com/"),
        "{repository} cannot be read file-by-file"
    )?;
    Ok(format!("https://raw.githubusercontent.com/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{metadata, parse};

    #[test]
    fn derives_the_raw_host_from_the_repository() {
        assert_eq!(
            raw_host("https://github.com/nvim-treesitter/nvim-treesitter").unwrap(),
            "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter"
        );
        assert!(raw_host("https://gitlab.com/owner/repo").is_err());
    }

    /// The test that would have caught every silent parse failure at bump
    /// time. Ignored because it needs the network.
    #[test]
    #[ignore = "fetches from raw.githubusercontent.com"]
    fn the_pinned_revision_parses() {
        let metadata = metadata::read().unwrap();
        let upstream = &metadata.nvim_treesitter;
        let get = |path: &str| get_remote(path, &upstream.repository, &upstream.revision).unwrap();

        let languages = parse::parse_languages(&get(&upstream.paths.parsers)).unwrap();
        let aliases = parse::parse_aliases(&get(&upstream.paths.filetypes)).unwrap();

        assert_eq!(languages.len(), 323);
        assert_eq!(aliases.len(), 75);

        // The number that matters: a language whose `install_info` went
        // unread would show up here, not as a missing highlight months later.
        let with_grammar = languages.iter().filter(|l| l.install.is_some()).count();
        assert_eq!(with_grammar, 320);
    }

    #[test]
    #[ignore = "fetches from raw.githubusercontent.com"]
    fn a_revision_that_does_not_exist_says_so() -> cu::Result<()> {
        let metadata = metadata::read()?;
        let error = get_remote(
            &metadata.nvim_treesitter.paths.parsers,
            &metadata.nvim_treesitter.repository,
            &"0".repeat(40),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("404"), "{error:#}");
        Ok(())
    }
}
