//! Getting pinned upstream sources onto disk.
//!
//! Two ways in, because upstream offers two shapes: a git repository at a
//! pinned revision (every grammar, and nvim-treesitter itself), and a gzipped
//! release binary (the tree-sitter CLI).
//!
//! Grammar repositories are large and we only ever want one commit out of one,
//! so instead of cloning we create an empty repository and fetch exactly the
//! revision we are pinned to at depth 1. Optionally only part of the tree is
//! checked out (`sparse`), which is how the nvim-treesitter queries are
//! obtained without the rest of the plugin.

use std::io::Write as _;
use std::path::Path;

use cu::pre::*;

/// Check `revision` of `url` out into `into`, which must not already exist.
///
/// `sparse` restricts the working tree to the given directories; an empty
/// slice checks out everything. The `.git` directory is removed afterwards --
/// nothing downstream needs history, and it is by far the largest part of what
/// was fetched.
pub fn checkout(url: &str, revision: &str, into: &Path, sparse: &[&str]) -> cu::Result<()> {
    cu::fs::make_dir(into)?;

    git(into, &["init", "--quiet"])?;
    git(into, &["remote", "add", "origin", url])?;

    if !sparse.is_empty() {
        git(into, &["sparse-checkout", "init", "--cone"])?;
        let mut args = vec!["sparse-checkout", "set"];
        args.extend_from_slice(sparse);
        git(into, &args)?;
    }

    // Fetching a bare SHA needs the server to allow it. GitHub does, and every
    // grammar in the table is hosted there; a tag or branch name works anywhere.
    cu::check!(
        git(
            into,
            &["fetch", "--quiet", "--depth", "1", "origin", revision]
        ),
        "failed to fetch {revision} from {url}"
    )?;
    git(into, &["checkout", "--quiet", "FETCH_HEAD"])?;

    cu::fs::rec_remove(into.join(".git"))?;
    Ok(())
}

fn git(cwd: &Path, args: &[&str]) -> cu::Result<()> {
    cu::check!(
        cu::which("git"),
        "git is required to download grammars, but was not found on PATH"
    )?
    .command()
    .args(args)
    .current_dir(cwd)
    // stdout belongs to mdBook; git's progress output goes to stderr where
    // the user can see it.
    .stdout_null()
    .stderr_inherit()
    .stdin_null()
    .wait_nz()
}
/// Upper bound on any single artifact we download.
///
/// The largest thing we fetch is a ~10 MiB tree-sitter binary; the cap exists
/// so a redirect to something unexpected cannot fill the disk.
const SIZE_LIMIT: u64 = 256 * 1024 * 1024;

/// Download a gzip-compressed `url` and write the decompressed bytes to `dest`.
///
/// tree-sitter publishes its CLI as a bare gzipped executable rather than an
/// archive, so there is nothing to unpack beyond the gzip frame.
pub fn download_gzipped(url: &str, dest: &Path) -> cu::Result<()> {
    write_body(url, dest, |body, file| {
        std::io::copy(&mut flate2::read::GzDecoder::new(body), file)
    })
}

fn write_body<F>(url: &str, dest: &Path, copy: F) -> cu::Result<()>
where
    F: FnOnce(&mut dyn std::io::Read, &mut std::fs::File) -> std::io::Result<u64>,
{
    let mut response = cu::check!(ureq::get(url).call(), "failed to request {url}")?;
    let status = response.status();
    cu::ensure!(status.is_success(), "{url} responded with HTTP {status}")?;

    if let Some(parent) = dest.parent() {
        cu::fs::make_dir(parent)?;
    }
    let mut file = cu::fs::writer(dest)?;

    let mut body = response.body_mut().with_config().limit(SIZE_LIMIT).reader();
    cu::check!(copy(&mut body, &mut file), "failed to save {url}")?;
    cu::check!(file.flush(), "failed to flush {}", dest.display())?;

    Ok(())
}
