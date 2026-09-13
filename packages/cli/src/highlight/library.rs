//! Loading a compiled tree-sitter parser from a shared library.

use std::path::Path;

use cu::pre::*;
use libloading::{Library, Symbol};
use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

/// A dynamically loaded parser.
///
/// The [`Language`] handed out by [`Self::language`] points into this
/// library's memory, so the library has to outlive every tree and query built
/// from it -- which is why it is kept rather than dropped after loading.
pub struct ParserLibrary {
    language: Language,
    _library: Library,
}

impl ParserLibrary {
    /// Load `path` and resolve the parser constructor for `symbol`.
    ///
    /// `symbol` is the grammar's name, without the `tree_sitter_` prefix.
    pub fn load(path: &Path, symbol: &str) -> cu::Result<Self> {
        let export = format!("tree_sitter_{}", symbol.replace('-', "_"));

        // SAFETY: the file is a tree-sitter parser we compiled ourselves with
        // the pinned CLI, so the named export has the standard signature
        // `extern "C" fn() -> *const ()`. Loading it runs the library's
        // initialisers, which for a generated parser do nothing.
        let library = unsafe { Library::new(path) };
        let library = cu::check!(library, "failed to load the parser at {}", path.display())?;

        let constructor = unsafe {
            let constructor: Symbol<unsafe extern "C" fn() -> *const ()> = cu::check!(
                library.get(export.as_bytes()),
                "{} does not export {export}",
                path.display()
            )?;
            LanguageFn::from_raw(*constructor)
        };
        let language = Language::from(constructor);

        // A parser built by a newer CLI than our runtime cannot be used, and
        // the failure otherwise shows up much later as an empty parse.
        let abi_compatible = tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION <= language.abi_version()
            && language.abi_version() <= tree_sitter::LANGUAGE_VERSION;
        if !abi_compatible {
            cu::bail!(
                "the parser at {} has ABI version {}, but this build supports {}..={}",
                path.display(),
                language.abi_version(),
                tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
                tree_sitter::LANGUAGE_VERSION
            );
        }

        Ok(Self {
            language,
            _library: library,
        })
    }

    /// The loaded grammar.
    pub fn language(&self) -> &Language {
        &self.language
    }
}
