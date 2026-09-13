//! Building one language's parser from its pinned grammar.

use std::path::{Path, PathBuf};

use cu::pre::*;

use crate::registry::LanguageInfo;
use crate::store::fetch_util::checkout;
use crate::store::system_cache::SystemCache;

/// A compiled parser, ready to be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parser {
    /// The nvim-treesitter language id this parser was built for.
    pub language: String,
    /// The shared library holding the parser.
    pub library: PathBuf,
    /// The exported constructor to look up, without the `tree_sitter_` prefix.
    ///
    /// This is the grammar's own name, which is usually but not always the
    /// nvim-treesitter language id -- so it is recorded at build time from
    /// `grammar.json` rather than guessed at load time.
    pub symbol: String,
}

/// File inside a cache entry recording the grammar name.
const SYMBOL_FILE: &str = "symbol";
/// File inside a cache entry holding the compiled parser.
const LIBRARY_FILE: &str = "parser.so";

/// Build `language`'s parser if it is not cached, and describe how to load it.
pub fn ensure_parser(
    cache: &SystemCache,
    tree_sitter: &Path,
    language: &LanguageInfo,
) -> cu::Result<Parser> {
    let name = language.name;
    let install = cu::check!(
        language.install,
        "`{name}` is a query-only language and has no parser to build"
    )?;

    let key = format!("parser-{name}-{}", install.short_revision());
    let dir = cache.ensure(&key, |dir| {
        let repo = dir.join("repo");
        eprintln!("mdbook-nvim-treesitter: cloning tree-sitter-{name}");
        checkout(install.url, install.revision, &repo, &[])?;

        let grammar = match install.location {
            Some(location) => repo.join(location),
            None => repo.clone(),
        };

        if install.generate {
            eprintln!("mdbook-nvim-treesitter: generating tree-sitter-{name}");
            run(tree_sitter, &grammar, &["generate", "src/grammar.json"])?;
        }

        eprintln!("mdbook-nvim-treesitter: compiling tree-sitter-{name}");
        let library = dir.join(LIBRARY_FILE);
        let library = cu::check!(library.to_str(), "cache path is not valid UTF-8")?;
        run(tree_sitter, &grammar, &["build", "-o", library])?;

        let symbol = grammar_name(&cu::fs::read_string(grammar.join("src/grammar.json"))?)
            .unwrap_or_else(|| name.to_string());
        cu::fs::write(dir.join(SYMBOL_FILE), &symbol)?;

        // The checkout is only needed while building, and grammar repos are
        // big; keeping them would multiply the cache size many times over.
        cu::fs::rec_remove(&repo)?;
        Ok(())
    })?;

    Ok(Parser {
        language: name.to_string(),
        library: dir.join(LIBRARY_FILE),
        symbol: cu::fs::read_string(dir.join(SYMBOL_FILE))?
            .trim()
            .to_string(),
    })
}

fn run(tree_sitter: &Path, cwd: &Path, args: &[&str]) -> cu::Result<()> {
    cu::ensure!(
        cwd.is_dir(),
        "the grammar directory {} does not exist",
        cwd.display()
    )?;
    (tree_sitter)
        .command()
        .args(args)
        .current_dir(cwd)
        // `generate` falls back to a JavaScript grammar when there is no
        // `grammar.json`; the bundled runtime avoids needing Node installed.
        .env("TREE_SITTER_JS_RUNTIME", "native")
        // stdout belongs to mdBook, so only stderr is passed through.
        .stdout_null()
        .stderr_inherit()
        .stdin_null()
        .wait_nz()
}

/// The `name` field of a `grammar.json`.
///
/// `grammar.json` can be many megabytes of rule definitions, and only the very
/// first field is interesting, so this scans for it rather than deserialising
/// the whole document.
fn grammar_name(grammar_json: &str) -> Option<String> {
    let after_key = grammar_json.split_once("\"name\"")?.1;
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let value = after_colon.trim_start().strip_prefix('"')?;
    let name = value.split('"').next()?;

    let is_identifier =
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    is_identifier.then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_grammar_name() {
        assert_eq!(
            grammar_name(r#"{"name":"c_sharp","rules":{}}"#).as_deref(),
            Some("c_sharp")
        );
        assert_eq!(
            grammar_name("{\n  \"name\" : \"rust\" ,\n  \"rules\": {}\n}").as_deref(),
            Some("rust")
        );
    }

    #[test]
    fn rejects_a_name_that_is_not_a_symbol() {
        // A grammar name has to be pasteable into `tree_sitter_<name>`; if it
        // is not, falling back to the language id is the better guess.
        assert_eq!(grammar_name(r#"{"name":"not a symbol"}"#), None);
        assert_eq!(grammar_name(r#"{"name":""}"#), None);
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(grammar_name(""), None);
        assert_eq!(grammar_name(r#"{"rules":{}}"#), None);
        assert_eq!(grammar_name(r#"{"name""#), None);
    }
}
