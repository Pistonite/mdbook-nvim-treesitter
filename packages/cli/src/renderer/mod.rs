//! Finding the code in an mdBook chapter and putting highlighted HTML back.
//!
//! Three steps, in the order a chapter goes through them: [`scan`] locates
//! every piece of code worth highlighting and reports it as a byte range,
//! [`render_html`] turns the highlighted spans into the markup that replaces
//! it, and [`Replacer`] puts that markup back. In between sits the
//! highlighter, which knows nothing about markdown.

mod replacer;
pub use replacer::*;
pub mod render_html;
pub mod scan;

pub const DEFAULT_STYLESHEET: &str = include_str!("default.css");
