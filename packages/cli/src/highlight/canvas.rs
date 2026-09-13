//! Flattening overlapping highlights into a run of non-overlapping spans.
//!
//! Neovim's highlighting is flat: captures are applied as extmarks, and where
//! two cover the same byte the winner is the one with the higher priority, or,
//! at equal priority, the one applied later. Queries lean on that -- a broad
//! pattern is written first and a specific one after it, expecting the second
//! to win.
//!
//! Painting into a per-byte buffer reproduces that rule exactly, and turns the
//! overlapping captures into the flat sequence of spans an HTML renderer needs.

use std::ops::Range;

/// A contiguous run of source that carries one highlight (or none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<'a> {
    /// Byte range within the highlighted source.
    pub range: Range<usize>,
    /// The highlight group, e.g. `keyword.function`. `None` is unhighlighted.
    pub group: Option<&'a str>,
}

/// A per-byte buffer of the winning highlight.
pub struct Canvas<'a> {
    groups: Vec<Option<&'a str>>,
    priorities: Vec<u32>,
}

impl<'a> Canvas<'a> {
    /// A blank canvas covering `length` bytes.
    pub fn new(length: usize) -> Self {
        Self {
            groups: vec![None; length],
            priorities: vec![0; length],
        }
    }

    /// Paint `range` with `group`, if nothing stronger is already there.
    ///
    /// Equal priority wins, so a later capture overrides an earlier one --
    /// which is what makes a specific pattern beat the general one above it.
    pub fn paint(&mut self, range: Range<usize>, group: Option<&'a str>, priority: u32) {
        let end = range.end.min(self.groups.len());
        for index in range.start.min(end)..end {
            if priority >= self.priorities[index] {
                self.groups[index] = group;
                self.priorities[index] = priority;
            }
        }
    }

    /// Collapse the canvas into spans covering all of `source`.
    ///
    /// Spans only ever start and end on character boundaries, so a directive
    /// that shifted a range into the middle of a multi-byte character cannot
    /// produce an unsliceable span.
    pub fn into_spans(self, source: &str) -> Vec<Span<'a>> {
        debug_assert_eq!(self.groups.len(), source.len());
        if source.is_empty() {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut start = 0;
        let mut current = self.groups[0];

        for index in 1..=self.groups.len() {
            let ends_here = index == self.groups.len()
                || (source.is_char_boundary(index) && self.groups[index] != current);
            if ends_here {
                spans.push(Span {
                    range: start..index,
                    group: current,
                });
                start = index;
                current = self.groups.get(index).copied().flatten();
            }
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans<'a>(source: &str, painted: &[(Range<usize>, Option<&'a str>, u32)]) -> Vec<Span<'a>> {
        let mut canvas = Canvas::new(source.len());
        for (range, group, priority) in painted {
            canvas.paint(range.clone(), *group, *priority);
        }
        canvas.into_spans(source)
    }

    fn shape<'a>(spans: &'a [Span<'a>], source: &'a str) -> Vec<(&'a str, Option<&'a str>)> {
        spans
            .iter()
            .map(|span| (&source[span.range.clone()], span.group))
            .collect()
    }

    #[test]
    fn covers_the_whole_source_even_when_nothing_is_painted() {
        let source = "let x";
        let spans = spans(source, &[]);
        assert_eq!(shape(&spans, source), [("let x", None)]);
    }

    #[test]
    fn splits_at_the_edges_of_a_painted_range() {
        let source = "let x";
        let spans = spans(source, &[(0..3, Some("keyword"), 100)]);
        assert_eq!(
            shape(&spans, source),
            [("let", Some("keyword")), (" x", None)]
        );
    }

    #[test]
    fn a_later_capture_wins_at_equal_priority() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..4, Some("outer"), 100), (1..3, Some("inner"), 100)],
        );
        assert_eq!(
            shape(&spans, source),
            [
                ("a", Some("outer")),
                ("bc", Some("inner")),
                ("d", Some("outer"))
            ]
        );
    }

    #[test]
    fn a_lower_priority_capture_cannot_overwrite() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..4, Some("strong"), 110), (1..3, Some("weak"), 100)],
        );
        assert_eq!(shape(&spans, source), [("abcd", Some("strong"))]);
    }

    #[test]
    fn none_clears_what_is_underneath() {
        let source = "abcd";
        let spans = spans(source, &[(0..4, Some("string"), 100), (1..3, None, 100)]);
        assert_eq!(
            shape(&spans, source),
            [("a", Some("string")), ("bc", None), ("d", Some("string"))]
        );
    }

    #[test]
    fn adjacent_runs_of_the_same_group_merge() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..2, Some("same"), 100), (2..4, Some("same"), 100)],
        );
        assert_eq!(shape(&spans, source), [("abcd", Some("same"))]);
    }

    #[test]
    fn spans_never_split_a_multibyte_character() {
        let source = "añb";
        // Paint only the first byte of the two-byte `ñ`: the whole character
        // takes the group rather than the span cutting it in half.
        let spans = spans(source, &[(1..2, Some("half"), 100)]);
        assert_eq!(
            shape(&spans, source),
            [("a", None), ("ñ", Some("half")), ("b", None)]
        );
    }

    #[test]
    fn ranges_past_the_end_are_clipped() {
        let source = "ab";
        let spans = spans(source, &[(0..99, Some("x"), 100)]);
        assert_eq!(shape(&spans, source), [("ab", Some("x"))]);
    }
}
