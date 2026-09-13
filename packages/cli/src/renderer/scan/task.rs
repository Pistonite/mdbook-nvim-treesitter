//! What the scanner found.

use std::ops::Range;

/// One piece of code to highlight and the text it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightTask {
    /// Byte range in the chapter that the rendered HTML replaces.
    ///
    /// For a fenced block this starts at the opening backticks -- *not* at the
    /// start of the line -- so any list indentation or block-quote marker in
    /// front of the fence survives the replacement.
    pub range: Range<usize>,
    /// The canonical nvim-treesitter language id.
    pub language: String,
    /// The code itself, with container prefixes already stripped.
    pub code: String,
    /// Where the code was found.
    pub kind: TaskKind,
}

/// The kind of construct a task came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// An inline code span written as ``` ```lang`code``` ```.
    ///
    /// Rendered without a `<pre>`, since it sits inside a paragraph.
    Inline,
    /// A fenced block whose opening fence is at column 0.
    TopLevelFenced,
    /// A fenced block inside a list item or block quote.
    ///
    /// No extra state is carried: the rendered HTML is a single line, so it
    /// simply inherits the container prefix already on the fence's line. The
    /// distinction is kept because it is what makes that rendering choice load
    /// bearing rather than incidental.
    NestedFenced,
}

impl TaskKind {
    /// Whether the task needs a `<pre>` wrapper when rendered.
    pub fn is_block(&self) -> bool {
        !matches!(self, TaskKind::Inline)
    }
}
