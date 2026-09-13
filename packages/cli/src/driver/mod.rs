//! The preprocessor as mdBook sees it: arguments in, a transformed book out.
//!
//! Everything below this module is a component that knows nothing about
//! mdBook. This is where they are wired together.

mod args;
pub use args::Cli;
mod pipeline;
mod run;
pub use run::run;
mod settings;
