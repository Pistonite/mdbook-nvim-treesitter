/// A short, filesystem-safe form of a pinned revision, for cache keys.
///
/// Commit SHAs are truncated to 8 characters; anything else -- nvim-treesitter
/// pins a handful of grammars to a tag such as `v1.2.3` -- is kept whole, since
/// truncating it would collide across versions.
pub fn short_revision(revision: &'static str) -> &'static str {
    let is_sha = revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit());
    if is_sha { &revision[..8] } else { revision }
}

/// Reduce a tag to the form the lookup tables are keyed by.
///
/// Tags arrive from three places that all have to agree -- a markdown fence, an
/// `include`/`exclude` entry in `book.toml`, and the language an injections
/// query names -- so the normalisation lives in one place rather than at each
/// call site.
pub fn normalize_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_a_commit_sha() {
        assert_eq!(
            short_revision("77a3747266f4d621d0757825e6b11edcbf991ca5"),
            "77a37472"
        );
    }

    #[test]
    fn keeps_a_tag_whole() {
        // `v0.2` and `v0.25.0` would truncate to the same eight characters.
        assert_eq!(short_revision("v0.25.0"), "v0.25.0");
        assert_eq!(short_revision("main"), "main");
    }

    #[test]
    fn leaves_anything_that_is_not_a_sha_alone() {
        // Right length, wrong alphabet.
        let not_hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert_eq!(short_revision(not_hex), not_hex);
    }

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(normalize_tag("  Rust "), "rust");
        assert_eq!(normalize_tag("JS"), "js");
        assert_eq!(normalize_tag("c++"), "c++");
    }

    #[test]
    fn leaves_an_empty_tag_empty() {
        assert_eq!(normalize_tag(""), "");
        assert_eq!(normalize_tag("   "), "");
    }
}
