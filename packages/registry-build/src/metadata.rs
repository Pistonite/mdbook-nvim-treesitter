//! Types for `metadata.json`

use cu::pre::*;

const METADATA: &str = include_str!("../metadata.json");

/// Read and validate `metadata.json`.
pub fn read() -> cu::Result<Metadata> {
    let manifest_raw = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"));
    let manifest: toml::Value = cu::check!(
        toml::parse(manifest_raw),
        "failed to parse the workspace Cargo.toml"
    )?;

    let tree_sitter = manifest
        .get("workspace")
        .and_then(|x| x.get("dependencies"))
        .and_then(|x| x.get("tree-sitter"));
    let tree_sitter_version = cu::check!(
        tree_sitter.and_then(|x| x.as_str()),
        "workspace.dependencies.tree-sitter doesn't exist or is not a version string"
    )?;
    let mut metadata = json::parse::<Metadata>(METADATA)?;
    metadata.tree_sitter.cli_version = tree_sitter_version.to_string();
    cu::check!(metadata.validate(), "invalid metadata.json")?;
    Ok(metadata)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    /// handle and ignore the comment in the file
    #[serde(rename = "//", default)]
    #[allow(dead_code)]
    comment: Vec<String>,
    pub nvim_treesitter: NvimTreesitter,
    pub tree_sitter: TreeSitter,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NvimTreesitter {
    /// The repository the queries and the parser table come from.
    pub repository: String,
    /// The commit this build is locked to.
    ///
    /// Always a full SHA: it is used as a cache key and as a
    /// content-addressed fetch path, and both need to be immutable.
    pub revision: String,
    pub paths: Paths,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// The parser table, within the nvim-treesitter repository.
    pub parsers: String,
    /// The filetype aliases, within the nvim-treesitter repository.
    pub filetypes: String,
    /// The directory holding one query directory per language.
    pub queries: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeSitter {
    /// The repository the CLI is released from.
    pub repository: String,
    /// The CLI release this build is locked to. Read directly from workspace Cargo.toml
    #[serde(skip)]
    pub cli_version: String,
}

impl Metadata {
    fn validate(&self) -> cu::Result<()> {
        let revision = &self.nvim_treesitter.revision;
        if !(revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())) {
            cu::bail!(
                "nvim_treesitter.revision must be a full 40-character commit SHA, got {revision:?}."
            );
        }

        let version = &self.tree_sitter.cli_version;
        let is_version_valid = version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
        if !is_version_valid {
            cu::bail!("tree_sitter.cli_version must be `x.y.z`, got {version:?}");
        }

        check_repository(&self.nvim_treesitter.repository)?;
        check_repository(&self.tree_sitter.repository)?;

        let paths = &self.nvim_treesitter.paths;
        for path in [&paths.parsers, &paths.filetypes, &paths.queries] {
            cu::ensure!(
                !path.starts_with('/') && !path.split('/').any(|part| part == ".."),
                "paths must be relative and stay inside the repository, got {path:?}"
            )?;
        }

        Ok(())
    }
}

/// Repositories are github.com URLs, deliberately narrowly.
///
/// The raw-file host in [`super::fetch`] is derived from this, and only works
/// for github.com; a `.git` suffix or a trailing slash would 404 there. A
/// clear error at codegen time beats a confusing one from `ureq`.
fn check_repository(repository: &str) -> cu::Result<()> {
    let path = cu::check!(
        repository.strip_prefix("https://github.com/"),
        "{repository:?} is not an https github.com url"
    )?;

    let is_path_valid =
        path.split('/').count() == 2 && path.split('/').all(|part| !part.is_empty());
    if !is_path_valid {
        cu::bail!("{repository:?} should be https://github.com/<owner>/<repo>");
    }
    cu::ensure!(
        !repository.ends_with(".git"),
        "{repository:?} should not end in .git"
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_metadata_is_valid() {
        read().unwrap();
    }

    /// Parse and validate, with the CLI version filled in the way [`read`]
    /// does -- it comes from the workspace manifest, not from the JSON.
    fn parse(json: &str, version: &str) -> cu::Result<Metadata> {
        let mut metadata: Metadata = cu::json::parse(json)?;
        metadata.tree_sitter.cli_version = version.to_string();
        metadata.validate()?;
        Ok(metadata)
    }

    fn document(revision: &str, repository: &str) -> String {
        format!(
            r#"{{"nvim_treesitter":{{"repository":"{repository}","revision":"{revision}",
               "paths":{{"parsers":"a.lua","filetypes":"b.lua","queries":"runtime/queries"}}}},
               "tree_sitter":{{"repository":"https://github.com/tree-sitter/tree-sitter"}}}}"#
        )
    }

    const SHA: &str = "9a168f6357ed21c3a636e1727bc7d382abc451b8";
    const REPO: &str = "https://github.com/nvim-treesitter/nvim-treesitter";

    #[test]
    fn accepts_a_well_formed_document() {
        let metadata = parse(&document(SHA, REPO), "0.27.0").unwrap();
        assert_eq!(metadata.nvim_treesitter.revision, SHA);
        assert_eq!(metadata.tree_sitter.cli_version, "0.27.0");
    }

    #[test]
    fn the_cli_version_is_not_read_from_the_json() {
        // It comes from `workspace.dependencies.tree-sitter`, so that the
        // version this build links and the CLI that compiles its parsers
        // cannot disagree. Writing it here would recreate the duplication.
        let json =
            document(SHA, REPO).replacen('{', "{\"tree_sitter\":{\"cli_version\":\"9.9.9\"},", 1);
        assert!(parse(&json, "0.27.0").is_err());
    }

    #[test]
    fn a_mistyped_key_is_an_error_rather_than_a_default() {
        let json = document(SHA, REPO).replace("revision", "revisoin");
        assert!(parse(&json, "0.27.0").is_err());
    }

    #[test]
    fn the_comment_key_is_allowed() {
        let json = document(SHA, REPO).replacen('{', "{\"//\":[\"a note\"],", 1);
        assert!(parse(&json, "0.27.0").is_ok());
    }

    #[test]
    fn rejects_a_revision_that_is_not_a_full_sha() {
        for revision in ["v0.9.3", "main", "9a168f63", ""] {
            let error = parse(&document(revision, REPO), "0.27.0").unwrap_err();
            assert!(format!("{error:#}").contains("40-character"), "{revision}");
        }
    }

    #[test]
    fn rejects_a_version_that_is_not_three_numbers() {
        // The version comes from the manifest, so this guards against a
        // workspace dependency written as `{ version = "..." }` or a range.
        for version in ["0.27", "v0.27.0", "latest", "0.27.x"] {
            assert!(parse(&document(SHA, REPO), version).is_err(), "{version}");
        }
    }

    #[test]
    fn rejects_a_repository_the_raw_url_cannot_be_derived_from() {
        for repository in [
            "https://github.com/nvim-treesitter/nvim-treesitter.git",
            "https://github.com/nvim-treesitter/nvim-treesitter/",
            "https://gitlab.com/owner/repo",
            "git@github.com:owner/repo",
            "https://github.com/owner",
        ] {
            assert!(
                parse(&document(SHA, repository), "0.27.0").is_err(),
                "{repository}"
            );
        }
    }

    #[test]
    fn rejects_a_path_that_escapes_the_repository() {
        let json = document(SHA, REPO).replace("\"a.lua\"", "\"../a.lua\"");
        assert!(parse(&json, "0.27.0").is_err());
        let json = document(SHA, REPO).replace("\"a.lua\"", "\"/etc/passwd\"");
        assert!(parse(&json, "0.27.0").is_err());
    }
}
