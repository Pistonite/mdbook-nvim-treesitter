//! End-to-end tests of the store's driver.
//!
//! These hit the network and run a C compiler, so they are ignored by default:
//! `cargo test -- --ignored`. Set `MNT_TEST_CACHE` to put the scratch caches
//! somewhere other than the temp directory.

use std::path::PathBuf;

use mdbook_nvim_treesitter::store::{SystemCache, Workspace};

/// A workspace over a scratch machine-wide cache and a scratch book cache.
///
/// The machine-wide half is shared between tests so grammars are built once;
/// the book half is per-test, so one test cannot see another's installs.
fn workspace(name: &str) -> Workspace {
    let root: PathBuf = std::env::var_os("MNT_TEST_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("mnt-integration-cache"));
    Workspace::open_at(
        SystemCache::at(root.join("shared")),
        root.join("books").join(name),
    )
    .unwrap()
}

#[test]
#[ignore = "downloads the tree-sitter CLI and clones nvim-treesitter"]
fn opening_a_workspace_installs_the_pinned_tooling() {
    // Opening is what fetches the CLI and the queries; if either were missing
    // the shared query root would not exist.
    let workspace = workspace("open");
    let shared = workspace.query_roots().pop().unwrap();

    assert!(shared.join("rust").join("highlights.scm").is_file());
}

#[test]
#[ignore = "clones and compiles a grammar"]
fn installing_a_language_puts_a_parser_in_the_book_cache() {
    let workspace = workspace("parser");

    let installed = workspace.install("rust").unwrap().unwrap();

    assert_eq!(installed.symbol, "rust");
    assert!(installed.parser.is_file());
    assert!(installed.parser.ends_with("parsers/rust.so"));
}

#[test]
#[ignore = "clones nvim-treesitter"]
fn installing_a_language_brings_its_inherited_queries() {
    // C++ queries open with `; inherits: c`, so C's `.scm` files have to land
    // in the book cache too -- even though C's parser never does.
    let workspace = workspace("inherited");
    workspace.install("cpp").unwrap();

    let local = workspace.query_roots().remove(0);
    assert!(local.join("cpp").join("highlights.scm").is_file());
    assert!(local.join("c").join("highlights.scm").is_file());
}

#[test]
#[ignore = "clones nvim-treesitter"]
fn a_query_only_language_installs_no_parser() {
    let workspace = workspace("query-only");

    // `ecma` exists so JavaScript and TypeScript can inherit from it; it has
    // no grammar of its own, and that is not a failure.
    assert!(workspace.install("ecma").unwrap().is_none());
    assert!(workspace.install("not-a-language").unwrap().is_none());
}
