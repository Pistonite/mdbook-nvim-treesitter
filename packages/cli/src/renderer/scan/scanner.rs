//! Locating highlightable code in a chapter.
//!
//! Byte ranges come from pulldown-cmark -- the same parser mdBook itself uses
//! -- so what we replace is exactly what mdBook would have rendered.
//!
//! Three shapes are recognised:
//!
//! 1. a fenced block at column 0;
//! 2. a fenced block inside a list item or block quote, where the reported
//!    range starts at the backticks and so leaves the container prefix on the
//!    line intact;
//! 3. an inline span written ``` ```lang`code``` ```, which a markdown parser
//!    reads as a code span whose content is ``lang`code``.

use mdbook_markdown::pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use mdbook_markdown::{MarkdownOptions, new_cmark_parser};

use crate::renderer::scan::{Filter, HighlightTask, TaskKind};

use crate::registry;

/// Info-string token that leaves one block to mdBook.
///
/// Useful for blocks that need mdBook's own treatment, such as a Rust
/// playground example whose hidden `#` lines are stripped by mdBook's
/// JavaScript.
pub const OPT_OUT: &str = "no-treesitter";

/// Every piece of code in `content` that `filter` allows, in source order.
pub fn tasks(content: &str, filter: &Filter) -> Vec<HighlightTask> {
    let mut tasks = Vec::new();
    let mut open: Option<OpenBlock> = None;

    let parser = new_cmark_parser(content, &MarkdownOptions::default());
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                open = Some(OpenBlock {
                    start: range.start,
                    language: fence_language(&info, filter),
                    code: String::new(),
                });
            }
            // An indented code block has no language to go on.
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => open = None,
            Event::Text(text) => {
                if let Some(block) = open.as_mut() {
                    block.code.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = open.take()
                    && let Some(language) = block.language
                {
                    tasks.push(HighlightTask {
                        kind: if at_line_start(content, block.start) {
                            TaskKind::TopLevelFenced
                        } else {
                            TaskKind::NestedFenced
                        },
                        range: block.start..range.end,
                        language,
                        code: block.code,
                    });
                }
            }
            Event::Code(text) => {
                if let Some((language, code)) = inline_language(&text, filter) {
                    tasks.push(HighlightTask {
                        range,
                        language,
                        code,
                        kind: TaskKind::Inline,
                    });
                }
            }
            _ => {}
        }
    }

    tasks
}

/// A fenced block being accumulated between its start and end events.
struct OpenBlock {
    start: usize,
    /// `None` when the block is not one we highlight.
    language: Option<String>,
    code: String,
}

/// The language a fence info string selects, if we highlight it.
fn fence_language(info: &str, filter: &Filter) -> Option<String> {
    let mut tokens = info
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty());

    let tag = tokens.next()?;
    if tokens.any(|token| token == OPT_OUT) {
        return None;
    }

    let language = registry::resolve_language(tag)?;
    filter
        .allows(language.name)
        .then(|| language.name.to_string())
}

/// Split an inline code span into its language tag and its code.
///
/// ``` ```rust`fn f()``` ``` reaches us as the code span ``rust`fn f()``, so
/// the tag is everything before the first backtick. A span with no backtick,
/// or whose prefix is not a language, is ordinary inline code.
fn inline_language(text: &str, filter: &Filter) -> Option<(String, String)> {
    let (tag, code) = text.split_once('`')?;
    let language = registry::resolve_language(tag)?;
    filter
        .allows(language.name)
        .then(|| (language.name.to_string(), code.to_string()))
}

fn at_line_start(content: &str, offset: usize) -> bool {
    offset == 0 || content.as_bytes()[offset - 1] == b'\n'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(content: &str) -> Vec<HighlightTask> {
        tasks(content, &Filter::default())
    }

    /// The exact text each task will replace.
    fn replaced<'a>(content: &'a str, tasks: &[HighlightTask]) -> Vec<&'a str> {
        tasks.iter().map(|t| &content[t.range.clone()]).collect()
    }

    #[test]
    fn finds_a_top_level_fenced_block() {
        let content = "intro\n\n```rust\nfn f() {}\n```\n";
        let tasks = scan(content);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind, TaskKind::TopLevelFenced);
        assert_eq!(tasks[0].language, "rust");
        assert_eq!(tasks[0].code, "fn f() {}\n");
        assert_eq!(replaced(content, &tasks), ["```rust\nfn f() {}\n```"]);
    }

    #[test]
    fn a_nested_fence_keeps_its_container_prefix_out_of_the_range() {
        let content = "- item\n\n  ```rust\n  fn f() {}\n  ```\n";
        let tasks = scan(content);

        assert_eq!(tasks[0].kind, TaskKind::NestedFenced);
        // The two spaces of list indentation stay in the document.
        assert!(replaced(content, &tasks)[0].starts_with("```rust"));
        // The code itself arrives without them.
        assert_eq!(tasks[0].code, "fn f() {}\n");
    }

    #[test]
    fn a_block_quoted_fence_is_nested_too() {
        let content = "> quote\n>\n> ```rust\n> fn f() {}\n> ```\n";
        let tasks = scan(content);

        assert_eq!(tasks[0].kind, TaskKind::NestedFenced);
        assert!(replaced(content, &tasks)[0].starts_with("```rust"));
        assert_eq!(tasks[0].code, "fn f() {}\n");
    }

    #[test]
    fn finds_the_inline_syntax() {
        let content = "call ```rust`fn f()``` now";
        let tasks = scan(content);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind, TaskKind::Inline);
        assert_eq!(tasks[0].language, "rust");
        assert_eq!(tasks[0].code, "fn f()");
        assert_eq!(replaced(content, &tasks), ["```rust`fn f()```"]);
    }

    #[test]
    fn leaves_ordinary_inline_code_alone() {
        assert_eq!(scan("a `plain` span"), []);
        // A backtick is there, but `notalanguage` is not a language.
        assert_eq!(scan("a ``notalanguage`x`` span"), []);
    }

    #[test]
    fn resolves_fence_tags_through_aliases() {
        assert_eq!(scan("```sh\nls\n```\n")[0].language, "bash");
        assert_eq!(scan("```JS\nlet x\n```\n")[0].language, "javascript");
        assert_eq!(scan("```c++\nint x;\n```\n")[0].language, "cpp");
    }

    #[test]
    fn ignores_blocks_it_cannot_highlight() {
        assert_eq!(scan("```\nno language\n```\n"), []);
        assert_eq!(scan("```nonsense\nx\n```\n"), []);
        assert_eq!(scan("    indented code\n"), []);
    }

    #[test]
    fn honours_extra_info_string_tokens() {
        // mdBook and rustdoc annotations follow the language.
        assert_eq!(scan("```rust,no_run\nfn f() {}\n```\n")[0].language, "rust");
        assert_eq!(scan("```rust ignore\nfn f() {}\n```\n")[0].language, "rust");
        // ...unless one of them opts the block out.
        assert_eq!(scan("```rust,no-treesitter\nfn f() {}\n```\n"), []);
    }

    #[test]
    fn applies_the_filter() {
        let (filter, _) = Filter::new(&[], &["rust".to_string()]);
        assert_eq!(tasks("```rust\nfn f() {}\n```\n", &filter), []);
        assert_eq!(tasks("x ```rust`f()``` y", &filter), []);
        assert_eq!(tasks("```c\nint x;\n```\n", &filter).len(), 1);
    }

    #[test]
    fn reports_tasks_in_source_order_without_overlaps() {
        let content = "```rust\na\n```\n\ntext ```c`int x``` more\n\n```python\nb\n```\n";
        let tasks = scan(content);

        assert_eq!(
            tasks
                .iter()
                .map(|t| t.language.as_str())
                .collect::<Vec<_>>(),
            ["rust", "c", "python"]
        );
        assert!(tasks.windows(2).all(|w| w[0].range.end <= w[1].range.start));
    }

    #[test]
    fn handles_a_chapter_with_no_code() {
        assert_eq!(scan(""), []);
        assert_eq!(scan("# Title\n\nJust prose.\n"), []);
    }
}
