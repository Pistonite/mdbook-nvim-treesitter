//! Command line surface.
//!
//! With no subcommand the binary is a preprocessor, reading mdBook's JSON on
//! stdin and writing the transformed book to stdout. The subcommands are the
//! parts a human (or mdBook's capability check) invokes directly.

use std::sync::OnceLock;

use cu::pre::*;

use crate::registry;

/// Highlight mdBook code blocks with nvim-treesitter queries.
#[derive(clap::Parser)]
#[command(version, long_version = long_version(), about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[clap(flatten)]
    pub common: cu::cli::Flags,
}

impl Cli {
    /// Keep `cu`'s own logging out of an mdBook run.
    ///
    /// mdBook reads the processed book from our stdout and shows our stderr to
    /// the user, so the only thing either stream should carry is the book and
    /// real progress. `cu` would otherwise add its own framing (the trailing
    /// `finished in ...`, for one). Errors survive `--quiet`, which is what
    /// matters: a failed preprocessor must still say why.
    ///
    /// Passing `-v` opts back in, for debugging a book build.
    pub fn quieten_for_mdbook(&mut self) {
        let mdbook_facing = matches!(self.command, None | Some(Command::Supports { .. }));
        if mdbook_facing && self.common.verbose == 0 {
            self.common.quiet = self.common.quiet.max(1);
        }
    }
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Report whether a renderer is supported. Only `html` is.
    ///
    /// mdBook calls this before running the preprocessor.
    Supports {
        /// The renderer mdBook is about to run.
        renderer: String,
    },
    /// Print the default stylesheet for the generated `ts-*` classes.
    Css,
}

/// The versions this build is pinned to, shown by `--version`.
///
/// Both are baked in: the queries have to match the grammars they were written
/// for, and the CLI has to produce parsers this build can load. Knowing which
/// revisions a book was built with is the first thing to check when a
/// highlight looks wrong.
fn long_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        format!(
            "{}\ntree-sitter CLI: {}\nnvim-treesitter:  {}",
            env!("CARGO_PKG_VERSION"),
            registry::TREE_SITTER_CLI_VERSION,
            registry::NVIM_TREESITTER_REVISION,
        )
    })
}
