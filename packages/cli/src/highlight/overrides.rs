//! Letting an author override a highlight from inside the code.
//!
//! Some distinctions a grammar simply cannot make. In C, a macro invocation
//! and a function call are the same syntax, so no query can tell them apart.
//! A marker comment says which one it is:
//!
//! ```c
//! /* @tree-sitter:function.macro */ASSERT(x > 0);
//! ```
//!
//! The comment is removed before the code is parsed -- so it cannot confuse
//! the grammar, and works even in a language where `/* */` is not a comment --
//! and the highlight it names is applied to whatever follows it.

use std::ops::Range;

use crate::highlight::Span;

/// The marker that introduces a highlight group.
const MARKER: &str = "@tree-sitter:";

/// The comment shapes a marker may be written in.
///
/// Both are *inline* forms, so the override applies to the code right after
/// them on the same line. A line comment would push the target onto the next
/// line, which reads far less clearly at the point of use.
const DELIMITERS: [(&str, &str); 2] = [("/*", "*/"), ("<!--", "-->")];

/// Code with its marker comments removed, and the overrides they asked for.
#[derive(Debug, Default)]
pub struct Overrides {
    code: String,
    marks: Vec<Mark>,
}

#[derive(Debug)]
struct Mark {
    /// Where the comment was, as an offset into the stripped code.
    at: usize,
    group: String,
}

/// Remove every marker comment from `code` and record what it asked for.
pub fn strip_overrides(code: &str) -> Overrides {
    if !code.contains(MARKER) {
        return Overrides {
            code: code.to_string(),
            marks: Vec::new(),
        };
    }

    let mut stripped = String::with_capacity(code.len());
    let mut marks = Vec::new();
    let mut cursor = 0;

    while let Some(found) = find_marker(code, cursor) {
        stripped.push_str(&code[cursor..found.comment.start]);
        marks.push(Mark {
            at: stripped.len(),
            group: found.group,
        });
        cursor = found.comment.end;
    }
    stripped.push_str(&code[cursor..]);

    Overrides {
        code: stripped,
        marks,
    }
}

impl Overrides {
    /// The code to parse and highlight, without the marker comments.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Whether any marker was found.
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// Retag the spans each marker points at.
    ///
    /// A marker applies to the first non-whitespace character after it. If
    /// that character sits inside a longer span, the span is split so only the
    /// part from there on is retagged.
    pub fn apply<'a>(&'a self, spans: Vec<Span<'a>>) -> Vec<Span<'a>> {
        if self.marks.is_empty() {
            return spans;
        }

        let mut spans = spans;
        for mark in &self.marks {
            let Some(target) = self.first_code_byte(mark.at) else {
                continue;
            };
            let Some(index) = spans.iter().position(|span| span.range.contains(&target)) else {
                continue;
            };

            let span = &mut spans[index];
            if span.range.start == target {
                span.group = Some(&mark.group);
                continue;
            }

            let tail = target..span.range.end;
            span.range.end = target;
            spans.insert(
                index + 1,
                Span {
                    range: tail,
                    group: Some(&mark.group),
                },
            );
        }
        spans
    }

    fn first_code_byte(&self, from: usize) -> Option<usize> {
        self.code[from..]
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(offset, _)| from + offset)
    }
}

struct Found {
    /// Byte range of the whole comment in the original code.
    comment: Range<usize>,
    group: String,
}

/// The next marker comment at or after `from`.
fn find_marker(code: &str, from: usize) -> Option<Found> {
    let mut at = from;
    loop {
        let marker = code[at..].find(MARKER)? + at;
        let group_start = marker + MARKER.len();

        // The marker has to sit inside one of the comment forms, and the
        // opening delimiter has to be the one nearest in front of it.
        let candidate = DELIMITERS.iter().find_map(|(open, close)| {
            let open_at = code[from..marker].rfind(open)? + from;
            // Nothing but whitespace may separate the delimiter from the
            // marker, so a comment that merely mentions the syntax in prose is
            // not treated as one.
            code[open_at + open.len()..marker]
                .trim()
                .is_empty()
                .then_some((open_at, *close))
        });

        if let Some((open_at, close)) = candidate {
            let close_at = code[group_start..].find(close).map(|i| i + group_start);
            if let Some(close_at) = close_at {
                let group = code[group_start..close_at].trim();
                if is_group(group) {
                    return Some(Found {
                        comment: open_at..close_at + close.len(),
                        group: group.to_string(),
                    });
                }
            }
        }

        // Not a usable marker; keep looking after it.
        at = group_start;
    }
}

/// Whether a string is shaped like a highlight group, e.g. `function.macro`.
fn is_group(group: &str) -> bool {
    !group.is_empty()
        && group.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::strip_overrides as strip;

    fn span(range: Range<usize>, group: Option<&str>) -> Span<'_> {
        Span { range, group }
    }

    fn shape<'a>(code: &'a str, spans: &'a [Span<'a>]) -> Vec<(&'a str, Option<&'a str>)> {
        spans
            .iter()
            .map(|span| (&code[span.range.clone()], span.group))
            .collect()
    }

    #[test]
    fn code_without_a_marker_is_untouched() {
        let overrides = strip("let x = 1;");
        assert_eq!(overrides.code(), "let x = 1;");
        assert!(overrides.is_empty());
    }

    #[test]
    fn strips_a_block_comment_marker() {
        let overrides = strip("/* @tree-sitter:function.macro */ASSERT(x);");
        assert_eq!(overrides.code(), "ASSERT(x);");
        assert_eq!(overrides.marks.len(), 1);
        assert_eq!(overrides.marks[0].group, "function.macro");
        assert_eq!(overrides.marks[0].at, 0);
    }

    #[test]
    fn strips_an_html_comment_marker() {
        let overrides = strip("<p><!-- @tree-sitter:tag.builtin -->custom</p>");
        assert_eq!(overrides.code(), "<p>custom</p>");
        assert_eq!(overrides.marks[0].group, "tag.builtin");
    }

    #[test]
    fn retags_the_span_that_follows() {
        let overrides = strip("/* @tree-sitter:function.macro */ASSERT(x)");
        let code = overrides.code();
        let spans = vec![span(0..6, Some("function.call")), span(6..9, None)];

        let spans = overrides.apply(spans);
        assert_eq!(
            shape(code, &spans),
            [("ASSERT", Some("function.macro")), ("(x)", None)]
        );
    }

    #[test]
    fn splits_a_span_when_the_target_starts_inside_it() {
        let overrides = strip("a = /* @tree-sitter:constant */B");
        let code = overrides.code();
        assert_eq!(code, "a = B");

        // One unstyled run covering `= B`.
        let spans = vec![span(0..1, Some("variable")), span(1..5, None)];
        let spans = overrides.apply(spans);
        assert_eq!(
            shape(code, &spans),
            [
                ("a", Some("variable")),
                (" = ", None),
                ("B", Some("constant"))
            ]
        );
    }

    #[test]
    fn skips_whitespace_between_the_marker_and_its_target() {
        let overrides = strip("/* @tree-sitter:type */\nPoint p;");
        let code = overrides.code();
        assert_eq!(code, "\nPoint p;");

        let spans = vec![
            span(0..1, None),
            span(1..6, Some("variable")),
            span(6..9, None),
        ];
        let spans = overrides.apply(spans);
        assert_eq!(shape(code, &spans)[1], ("Point", Some("type")));
    }

    #[test]
    fn handles_several_markers() {
        let overrides = strip("/* @tree-sitter:a */x + /* @tree-sitter:b */y");
        assert_eq!(overrides.code(), "x + y");
        assert_eq!(
            overrides
                .marks
                .iter()
                .map(|m| (m.at, m.group.as_str()))
                .collect::<Vec<_>>(),
            [(0, "a"), (4, "b")]
        );
    }

    #[test]
    fn leaves_comments_that_only_mention_the_marker_alone() {
        // Prose in front of the marker: not a directive.
        let code = "/* see @tree-sitter:function.macro for details */x";
        assert_eq!(strip(code).code(), code);

        // No comment at all around it.
        let bare = "let s = \"@tree-sitter:x\";";
        assert_eq!(strip(bare).code(), bare);
    }

    #[test]
    fn ignores_a_marker_with_no_usable_group() {
        for code in [
            "/* @tree-sitter: */x",
            "/* @tree-sitter:has spaces */x",
            "/* @tree-sitter:a..b */x",
            "/* @tree-sitter:unterminated x",
        ] {
            assert_eq!(strip(code).code(), code, "{code:?}");
        }
    }

    #[test]
    fn a_marker_at_the_end_of_the_code_overrides_nothing() {
        let overrides = strip("x;/* @tree-sitter:type */");
        assert_eq!(overrides.code(), "x;");
        let spans = overrides.apply(vec![span(0..2, None)]);
        assert_eq!(shape(overrides.code(), &spans), [("x;", None)]);
    }

    #[test]
    fn recognizes_group_names() {
        assert!(is_group("keyword"));
        assert!(is_group("function.macro"));
        assert!(is_group("markup.heading.1"));
        assert!(!is_group(""));
        assert!(!is_group("a b"));
        assert!(!is_group("a."));
        assert!(!is_group(".a"));
    }
}
