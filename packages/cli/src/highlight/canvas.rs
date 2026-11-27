//! Flattening overlapping highlights into a run of non-overlapping spans.
//!
//! Neovim's highlighting is layered: captures are applied as extmarks, and
//! where several cover the same byte they are stacked by priority, then by the
//! order they were applied. The top of the stack decides each attribute it
//! sets, and whatever it leaves unset shows through from the layers below.
//! Queries lean on that -- a broad pattern is written first and a specific one
//! after it, expecting the second to win -- and so do injections: a regex
//! literal keeps its `@string.regexp` colour underneath the regex grammar's
//! own captures.
//!
//! Painting into a per-byte stack reproduces that, and turns the overlapping
//! captures into the flat sequence of spans an HTML renderer needs. Each span
//! keeps its whole stack, so the renderer can nest the groups and let CSS do
//! the falling through.

use std::ops::Range;

/// A contiguous run of source that carries one stack of highlights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<'a> {
    /// Byte range within the highlighted source.
    pub range: Range<usize>,
    /// The highlight groups, e.g. `keyword.function`, from the bottom of the
    /// stack to the top. Empty is unhighlighted.
    pub groups: Vec<&'a str>,
}

impl<'a> Span<'a> {
    /// The group on top of the stack: the one that wins where all of them
    /// style the same thing.
    pub fn group(&self) -> Option<&'a str> {
        self.groups.last().copied()
    }
}

/// One group painted onto a byte.
#[derive(Debug, Clone, Copy)]
struct Layer<'a> {
    group: &'a str,
    priority: u32,
}

/// What has been painted onto one byte.
#[derive(Debug, Default, Clone)]
struct Cell<'a> {
    /// Ordered from bottom to top: by priority, then by paint order.
    layers: Vec<Layer<'a>>,
    /// The priority of the strongest clearing capture so far. Layers below it
    /// are hidden, so they are not kept.
    cleared: Option<u32>,
}

/// A per-byte buffer of the stacked highlights.
pub struct Canvas<'a> {
    cells: Vec<Cell<'a>>,
}

impl<'a> Canvas<'a> {
    /// A blank canvas covering `length` bytes.
    pub fn new(length: usize) -> Self {
        Self {
            cells: vec![Cell::default(); length],
        }
    }

    /// Paint `range` with `group` at `priority`.
    ///
    /// A group goes on top of every layer of lower or equal priority, so a
    /// later capture covers an earlier one -- which is what makes a specific
    /// pattern beat the general one above it -- while the earlier one stays
    /// underneath. `None` clears instead: it removes the layers it covers and
    /// hides any weaker capture painted after it.
    pub fn paint(&mut self, range: Range<usize>, group: Option<&'a str>, priority: u32) {
        let end = range.end.min(self.cells.len());
        let start = range.start.min(end);
        for cell in &mut self.cells[start..end] {
            if cell.cleared.is_some_and(|cleared| priority < cleared) {
                continue;
            }
            match group {
                None => {
                    cell.layers.retain(|layer| layer.priority > priority);
                    cell.cleared = Some(priority);
                }
                Some(group) => {
                    // A group already in the stack moves to its new place
                    // rather than appearing twice.
                    cell.layers.retain(|layer| layer.group != group);
                    let at = cell
                        .layers
                        .iter()
                        .position(|layer| layer.priority > priority)
                        .unwrap_or(cell.layers.len());
                    cell.layers.insert(at, Layer { group, priority });
                }
            }
        }
    }

    /// Collapse the canvas into spans covering all of `source`.
    ///
    /// Spans only ever start and end on character boundaries, so a directive
    /// that shifted a range into the middle of a multi-byte character cannot
    /// produce an unsliceable span.
    pub fn into_spans(self, source: &str) -> Vec<Span<'a>> {
        debug_assert_eq!(self.cells.len(), source.len());
        if source.is_empty() {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut start = 0;

        for index in 1..=self.cells.len() {
            let ends_here = index == self.cells.len()
                || (source.is_char_boundary(index)
                    && !same_groups(&self.cells[index], &self.cells[start]));
            if ends_here {
                spans.push(Span {
                    range: start..index,
                    groups: self.cells[start]
                        .layers
                        .iter()
                        .map(|layer| layer.group)
                        .collect(),
                });
                start = index;
            }
        }

        spans
    }
}

fn same_groups(a: &Cell, b: &Cell) -> bool {
    a.layers.len() == b.layers.len()
        && a.layers
            .iter()
            .zip(&b.layers)
            .all(|(a, b)| a.group == b.group)
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

    fn shape<'a>(spans: &'a [Span<'a>], source: &'a str) -> Vec<(&'a str, Vec<&'a str>)> {
        spans
            .iter()
            .map(|span| (&source[span.range.clone()], span.groups.clone()))
            .collect()
    }

    #[test]
    fn covers_the_whole_source_even_when_nothing_is_painted() {
        let source = "let x";
        let spans = spans(source, &[]);
        assert_eq!(shape(&spans, source), [("let x", vec![])]);
    }

    #[test]
    fn splits_at_the_edges_of_a_painted_range() {
        let source = "let x";
        let spans = spans(source, &[(0..3, Some("keyword"), 100)]);
        assert_eq!(
            shape(&spans, source),
            [("let", vec!["keyword"]), (" x", vec![])]
        );
    }

    #[test]
    fn a_later_capture_goes_on_top_at_equal_priority() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..4, Some("outer"), 100), (1..3, Some("inner"), 100)],
        );
        assert_eq!(
            shape(&spans, source),
            [
                ("a", vec!["outer"]),
                ("bc", vec!["outer", "inner"]),
                ("d", vec!["outer"])
            ]
        );
        assert_eq!(spans[1].group(), Some("inner"));
    }

    #[test]
    fn a_lower_priority_capture_goes_underneath() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..4, Some("strong"), 110), (1..3, Some("weak"), 100)],
        );
        assert_eq!(
            shape(&spans, source),
            [
                ("a", vec!["strong"]),
                ("bc", vec!["weak", "strong"]),
                ("d", vec!["strong"])
            ]
        );
        assert_eq!(spans[1].group(), Some("strong"));
    }

    #[test]
    fn an_injected_layer_keeps_the_host_group_underneath() {
        // `/^a/`: the host paints the pattern as a regex, then the injected
        // regex grammar paints its own captures over parts of it.
        let source = "/^a/";
        let spans = spans(
            source,
            &[
                (1..3, Some("string.regexp"), 100),
                (1..2, Some("punctuation.delimiter"), 100),
            ],
        );
        assert_eq!(
            shape(&spans, source),
            [
                ("/", vec![]),
                ("^", vec!["string.regexp", "punctuation.delimiter"]),
                ("a", vec!["string.regexp"]),
                ("/", vec![]),
            ]
        );
    }

    #[test]
    fn repainting_a_group_moves_it_rather_than_duplicating_it() {
        let source = "ab";
        let spans = spans(
            source,
            &[
                (0..2, Some("x"), 100),
                (0..2, Some("y"), 100),
                (0..2, Some("x"), 100),
            ],
        );
        assert_eq!(shape(&spans, source), [("ab", vec!["y", "x"])]);
    }

    #[test]
    fn none_clears_what_is_underneath() {
        let source = "abcd";
        let spans = spans(source, &[(0..4, Some("string"), 100), (1..3, None, 100)]);
        assert_eq!(
            shape(&spans, source),
            [("a", vec!["string"]), ("bc", vec![]), ("d", vec!["string"])]
        );
    }

    #[test]
    fn none_hides_weaker_captures_painted_after_it() {
        let source = "ab";
        let spans = spans(
            source,
            &[
                (0..2, None, 110),
                (0..2, Some("weak"), 100),
                (1..2, Some("equal"), 110),
            ],
        );
        assert_eq!(shape(&spans, source), [("a", vec![]), ("b", vec!["equal"])]);
    }

    #[test]
    fn none_keeps_stronger_layers() {
        let source = "ab";
        let spans = spans(source, &[(0..2, Some("strong"), 110), (0..2, None, 100)]);
        assert_eq!(shape(&spans, source), [("ab", vec!["strong"])]);
    }

    #[test]
    fn adjacent_runs_of_the_same_stack_merge() {
        let source = "abcd";
        let spans = spans(
            source,
            &[(0..2, Some("same"), 100), (2..4, Some("same"), 100)],
        );
        assert_eq!(shape(&spans, source), [("abcd", vec!["same"])]);
    }

    #[test]
    fn spans_never_split_a_multibyte_character() {
        let source = "añb";
        // Paint only the first byte of the two-byte `ñ`: the whole character
        // takes the group rather than the span cutting it in half.
        let spans = spans(source, &[(1..2, Some("half"), 100)]);
        assert_eq!(
            shape(&spans, source),
            [("a", vec![]), ("ñ", vec!["half"]), ("b", vec![])]
        );
    }

    #[test]
    fn ranges_past_the_end_are_clipped() {
        let source = "ab";
        let spans = spans(source, &[(0..99, Some("x"), 100)]);
        assert_eq!(shape(&spans, source), [("ab", vec!["x"])]);
    }
}
