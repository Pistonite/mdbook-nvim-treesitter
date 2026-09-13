//! Rendering the two generated modules.
//!
//! They are emitted separately because they have different provenance and
//! different update paths: `pin` is what *we* chose, `db` is what upstream
//! says. Bumping a revision then produces a one-line diff in one and a large
//! one in the other, and a reviewer can see at a glance which is which.
//!
//! Neither is formatted afterwards. `registry`'s `gen_/mod.rs` declares them
//! with `#[rustfmt::skip]`, so what is written here is what is compiled.

use std::fmt::Write as _;
use std::path::Path;

use crate::metadata::Metadata;
use crate::parse::{Alias, Language};

/// Render the constants derived from `metadata.json`.
pub fn emit_pin(metadata: &Metadata, out_dir: &Path) -> cu::Result<()> {
    // <BY_HUMAN>
    let mut out = make_header(
        "What this build pins.",
        "`packages/registry-build/metadata.json`",
    );

    let _ = writeln!(
        out,
        "/// The nvim-treesitter repository the queries come from.\n\
         pub const NVIM_TREESITTER_REPOSITORY: &str = {:?};\n\n\
         /// The nvim-treesitter commit this build is locked to.\n\
         pub const NVIM_TREESITTER_REVISION: &str = {:?};\n\n\
         /// The sub-directory of that repository holding one query directory\n\
         /// per language.\n\
         pub const NVIM_TREESITTER_QUERIES_DIR: &str = {:?};\n\n\
         /// The tree-sitter repository the CLI is released from.\n\
         pub const TREE_SITTER_REPOSITORY: &str = {:?};\n\n\
         /// The tree-sitter CLI release this build is locked to.\n\
         ///\n\
         /// Must stay in step with the `tree-sitter` runtime dependency: the CLI\n\
         /// decides the ABI version of every parser we compile, and the runtime\n\
         /// decides which ABI versions it can load.\n\
         pub const TREE_SITTER_CLI_VERSION: &str = {:?};",
        metadata.nvim_treesitter.repository,
        metadata.nvim_treesitter.revision,
        metadata.nvim_treesitter.paths.queries,
        metadata.tree_sitter.repository,
        metadata.tree_sitter.cli_version,
    );

    wrapped_write("pin.gen.rs", &out_dir.join("pin.gen.rs"), out)?;
    Ok(())
}
/// Render the language table read from nvim-treesitter.
pub fn emit_db(languages: &[Language], aliases: &[Alias], out_dir: &Path) -> cu::Result<()> {
    let mut out = make_header(
        "The pinned nvim-treesitter language table.",
        "nvim-treesitter's `parsers.lua` and `filetypes.lua`",
    );
    out.push_str("use crate::registry::{Install, LanguageInfo};\n\n");

    let _ = writeln!(
        out,
        "/// Every language nvim-treesitter ships queries for, sorted by name.\n\
         pub static LANGUAGES: &[LanguageInfo] = &["
    );
    for language in languages {
        let _ = writeln!(out, "{}", language_literal(language));
    }
    out.push_str("];\n\n");

    let _ = writeln!(
        out,
        "/// Neovim filetype aliases, as `(alias, language)` sorted by alias.\n\
         pub static FILETYPE_ALIASES: &[(&str, &str)] = &["
    );
    for alias in aliases {
        let _ = writeln!(
            out,
            "    ({:?}, {:?}),",
            alias.alias.as_str(),
            alias.language.as_str()
        );
    }
    out.push_str("];\n");

    wrapped_write("db.gen.rs", &out_dir.join("db.gen.rs"), out)?;
    Ok(())
}

fn language_literal(language: &Language) -> String {
    let install = match &language.install {
        None => "None".to_string(),
        Some(install) => format!(
            "Some(Install {{ url: {:?}, revision: {:?}, branch: {}, location: {}, generate: {} }})",
            install.url,
            install.revision,
            option_literal(install.branch.as_deref()),
            option_literal(install.location.as_deref()),
            install.generate,
        ),
    };
    let requires = language
        .requires
        .iter()
        .map(|r| format!("{r:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "    LanguageInfo {{ name: {:?}, install: {install}, requires: &[{requires}], tier: {} }},",
        language.name, language.tier,
    )
}

fn option_literal(value: Option<&str>) -> String {
    format!("{value:?}")
}

fn wrapped_write(what: &str, path: &Path, content: String) -> cu::Result<()> {
    // <BY_HUMAN>
    if let Ok(current) = cu::fs::read_string(path) {
        // normalize LE
        let current_normalized: Vec<_> = current.trim().lines().map(|x| x.trim_end()).collect();
        let new_normalized: Vec<_> = content.trim().lines().map(|x| x.trim_end()).collect();
        if current_normalized == new_normalized {
            cu::hint!("skipped {what}: {} is up-to-date", path.display());
            return Ok(());
        }
    }
    cu::fs::write(path, content)?;
    cu::info!("written {what}: {}", path.display());
    Ok(())
}

/// Header on every generated file, so no LLM edits one by hand.
fn make_header(subject: &str, source: &str) -> String {
    // <BY_HUMAN>
    format!(
        "//! {subject}\n\
         //!\n\
         //! @generated from {source} -- do not edit by hand,\n\
         //! see registry-build package for re-generation.\n\n"
    )
}
