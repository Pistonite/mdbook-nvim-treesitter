//! Turning highlighted spans into HTML for mdBook.
//!
//! Highlights become CSS classes rather than inline colours, so a book themes
//! code the same way it themes everything else. A dotted capture contributes
//! one class per prefix -- `keyword.function` becomes
//! `ts-keyword ts-keyword-function` -- which gives a stylesheet both the broad
//! group and the specific kind to target, and reproduces Neovim's rule that an
//! unstyled `@keyword.function` falls back to `@keyword`.
//!
//! The output is always a *single line*, with newlines written as `&#10;`.
//! That matters because a code block may sit inside a list item or a block
//! quote, where every physical line needs the container's prefix; keeping the
//! replacement to one line means it inherits the prefix already on the opening
//! fence's line and cannot break out of the container.

use crate::highlight::Span;

/// Prefix on every generated class.
const PREFIX: &str = "ts";

/// Render a fenced code block.
pub fn block(language: &str, source: &str, spans: &[Span]) -> String {
    format!(
        "<pre class=\"tree-sitter-block\"><code class=\"{}\">{}</code></pre>",
        code_classes(language, false),
        render(source, spans)
    )
}

/// Render an inline code span.
pub fn inline(language: &str, source: &str, spans: &[Span]) -> String {
    format!(
        "<code class=\"{}\">{}</code>",
        code_classes(language, true),
        render(source, spans)
    )
}

/// Render just the highlighted content, without a wrapper.
fn render(source: &str, spans: &[Span]) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    for span in spans {
        let Some(text) = source.get(span.range.clone()) else {
            continue;
        };
        match span.group {
            None => escape_into(text, &mut out),
            Some(group) => {
                out.push_str("<span class=\"");
                out.push_str(&class_attribute(group));
                out.push_str("\">");
                escape_into(text, &mut out);
                out.push_str("</span>");
            }
        }
    }
    out
}

/// The classes on the `<code>` element.
///
/// mdBook runs highlight.js over *every* `<code>` it renders, with no way to
/// turn that off from a preprocessor, and a second pass over spans we already
/// produced would wreck them. `language-none` is the lever that works:
/// highlight.js prefers a `language-*` class, finds no such language, and
/// falls back to not highlighting. `no-highlight` says the same thing for the
/// path where no `language-*` class is seen at all.
fn code_classes(language: &str, is_inline: bool) -> String {
    let kind = if is_inline {
        "tree-sitter-inline"
    } else {
        "tree-sitter-code"
    };
    format!(
        "no-highlight language-none {kind} tree-sitter-language-{}",
        sanitize(language)
    )
}

/// The class list for a highlight group.
fn class_attribute(group: &str) -> String {
    let mut classes = String::new();
    let mut current = String::from(PREFIX);
    for part in group.split('.') {
        current.push('-');
        current.push_str(&sanitize(part));
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(&current);
    }
    classes
}

/// Reduce a name to what is safe and conventional in a CSS class.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn escape_into(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Kept as an entity so the whole element stays on one line; a
            // browser renders it as a line break inside `<pre>` all the same.
            '\n' => out.push_str("&#10;"),
            // A lone carriage return would be a second line break in the
            // rendered output, so drop it and let the newline stand alone.
            '\r' => {}
            _ => out.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(range: std::ops::Range<usize>, group: Option<&str>) -> Span<'_> {
        Span { range, group }
    }

    #[test]
    fn dotted_groups_become_nested_classes() {
        assert_eq!(class_attribute("keyword"), "ts-keyword");
        assert_eq!(
            class_attribute("keyword.function"),
            "ts-keyword ts-keyword-function"
        );
        assert_eq!(
            class_attribute("variable.parameter.builtin"),
            "ts-variable ts-variable-parameter ts-variable-parameter-builtin"
        );
    }

    #[test]
    fn class_names_are_sanitized() {
        assert_eq!(
            class_attribute("markup.raw.block"),
            "ts-markup ts-markup-raw ts-markup-raw-block"
        );
        assert_eq!(class_attribute("Odd Name"), "ts-odd-name");
    }

    #[test]
    fn wraps_only_the_highlighted_spans() {
        let source = "let x";
        let spans = [span(0..3, Some("keyword")), span(3..5, None)];
        assert_eq!(
            render(source, &spans),
            "<span class=\"ts-keyword\">let</span> x"
        );
    }

    #[test]
    fn escapes_markup_characters() {
        let spans = [span(0..13, None)];
        assert_eq!(render("a<b>&c\"d\'e", &spans[..0]), "");
        assert_eq!(
            render("a<b>&\"c", &[span(0..7, None)]),
            "a&lt;b&gt;&amp;&quot;c"
        );
    }

    #[test]
    fn newlines_become_entities_so_the_output_is_one_line() {
        let rendered = render("a\nb", &[span(0..3, None)]);
        assert_eq!(rendered, "a&#10;b");
        assert!(!rendered.contains('\n'));

        assert_eq!(render("a\r\nb", &[span(0..4, None)]), "a&#10;b");
    }

    #[test]
    fn a_block_is_one_line_with_the_highlightjs_opt_out() {
        let html = block(
            "rust",
            "fn f\n",
            &[span(0..2, Some("keyword")), span(2..5, None)],
        );
        assert!(!html.contains('\n'), "{html}");
        assert!(html.starts_with("<pre class=\"tree-sitter-block\"><code class=\""));
        assert!(html.contains("no-highlight language-none"));
        assert!(html.contains("tree-sitter-language-rust"));
        assert!(html.ends_with("</code></pre>"));
    }

    #[test]
    fn an_inline_span_has_no_pre_wrapper() {
        let html = inline("rust", "x", &[span(0..1, None)]);
        assert_eq!(
            html,
            "<code class=\"no-highlight language-none tree-sitter-inline tree-sitter-language-rust\">x</code>"
        );
    }

    #[test]
    fn a_language_id_is_safe_in_a_class_name() {
        assert!(block("c++", "x", &[span(0..1, None)]).contains("tree-sitter-language-c--"));
    }

    #[test]
    fn out_of_range_spans_are_skipped_rather_than_panicking() {
        assert_eq!(render("ab", &[span(0..99, None)]), "");
    }
}
