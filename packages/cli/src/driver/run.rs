//! Dispatching a command.

use std::io::Write as _;

use crate::driver::args::{Cli, Command};
use crate::driver::pipeline;
use crate::renderer::DEFAULT_STYLESHEET;

/// The only mdBook renderer this preprocessor can support.
const SUPPORTED_RENDERER: &str = "html";

/// Run the command the arguments selected.
pub fn run(args: Cli) -> cu::Result<()> {
    match args.command {
        // mdBook reads the exit status, not the output, so say nothing.
        Some(Command::Supports { renderer }) => {
            if renderer != SUPPORTED_RENDERER {
                std::process::exit(1);
            }
            Ok(())
        }
        Some(Command::Css) => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(DEFAULT_STYLESHEET.as_bytes())?;
            stdout.flush()?;
            Ok(())
        }
        // No subcommand means mdBook is piping a book through us. mdBook shows
        // our stderr and discards our stdout when we fail, and `cu`'s printer
        // goes quiet when neither stream is a terminal -- which is exactly the
        // case under `mdbook build`. Report the failure here so it is always
        // visible, then still fail, so mdBook stops.
        None => pipeline::preprocess(std::io::stdin().lock(), std::io::stdout().lock())
            .inspect_err(|error| eprintln!("mdbook-nvim-treesitter: error: {error:#}")),
    }
}
