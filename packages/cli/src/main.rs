//! An mdBook preprocessor that highlights code with nvim-treesitter's queries.

use mdbook_nvim_treesitter::driver;

#[cu::cli(flags = "common", preprocess = driver::Cli::quieten_for_mdbook)]
fn main(args: driver::Cli) -> cu::Result<()> {
    driver::run(args)
}
