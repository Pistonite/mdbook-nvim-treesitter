//! Generates the pinned data the `registry` crate serves.
//!
//! Two inputs: `metadata.json`, which is hand-written and holds everything
//! this build pins, and the nvim-treesitter revision it names, whose parser
//! table and filetype aliases are fetched at that revision. Two outputs, one
//! per input.
//!
//! The whole thing is therefore a pure function of `metadata.json` -- there is
//! nothing else to pass it -- which is what makes `task codegen-check`
//! meaningful.

use cu::pre::*;

mod emit;
mod fetch;
mod metadata;
mod parse;

#[derive(clap::Parser)]
struct Cli {
    /// Where to write the module generated from `metadata.json`
    #[clap(long)]
    out_dir: Option<String>,

    /// Print the pinned revision information and exit
    #[clap(long)]
    print_revision: bool,

    /// Read a local nvim-treesitter checkout instead of fetching the pinned revision.
    /// Useful for local testing.
    #[clap(short, long)]
    local_source: Option<String>,

    #[clap(flatten)]
    common: cu::cli::Flags,
}

#[cu::cli(flags = "common")]
fn main(args: Cli) -> cu::Result<()> {
    // <BY_HUMAN>
    cu::lv::disable_print_time();
    let metadata = metadata::read()?;

    if args.print_revision {
        println!("nvim-treesitter {}", metadata.nvim_treesitter.revision);
        println!("tree-sitter {}", metadata.tree_sitter.cli_version);
        return Ok(());
    }
    let Some(out_dir) = &args.out_dir else {
        cu::bail!("--out-dir must be specified");
    };

    cu::check!(
        emit::emit_pin(&metadata, out_dir.as_ref()),
        "failed to emit pin metadata"
    )?;
    let (parsers, filetypes) = match &args.local_source {
        None => {
            let repository = &metadata.nvim_treesitter.repository;
            let revision = &metadata.nvim_treesitter.revision;
            let parsers = cu::check!(
                fetch::get_remote(
                    &metadata.nvim_treesitter.paths.parsers,
                    repository,
                    revision
                ),
                "failed to fetch nvim-treesitter parsers file"
            )?;
            let filetypes = cu::check!(
                fetch::get_remote(
                    &metadata.nvim_treesitter.paths.filetypes,
                    repository,
                    revision
                ),
                "failed to fetch nvim-treesitter filetypes file"
            )?;
            (parsers, filetypes)
        }
        Some(local) => {
            let parsers = cu::check!(
                fetch::get_local(&metadata.nvim_treesitter.paths.parsers, local),
                "failed to get local nvim-treesitter parsers file"
            )?;
            let filetypes = cu::check!(
                fetch::get_local(&metadata.nvim_treesitter.paths.filetypes, local),
                "failed to get local nvim-treesitter filetypes file"
            )?;
            (parsers, filetypes)
        }
    };

    let (languages, aliases) = parse::parse_db(&parsers, &filetypes)?;
    cu::check!(
        emit::emit_db(&languages, &aliases, out_dir.as_ref()),
        "failed to emit language db"
    )?;

    Ok(())
}
