//! Working out which parts of a document belong to an embedded language.
//!
//! An injections query marks a node as `@injection.content`. By default the
//! injected region is that node *minus its named children*, which is how a
//! template language keeps its own interpolations out of the embedded text:
//! the `${...}` holes are cut away and the surrounding literal text is parsed
//! as, say, SQL. `injection.include-children` opts out of the cutting.

use tree_sitter::{Node, Point, Range};

use crate::highlight::lines::LineIndex;
use crate::query::Offset;

/// Adjust a node's range by an `#offset!` directive.
///
/// Offsets are row/column deltas, so the byte bounds are recomputed from the
/// shifted points. An offset that would invert the range is ignored, matching
/// Neovim.
pub fn offset_range(range: Range, offset: Option<Offset>, lines: &LineIndex) -> Range {
    let Some(offset) = offset else {
        return range;
    };

    let start_point = shift(range.start_point, offset.start_row, offset.start_column);
    let end_point = shift(range.end_point, offset.end_row, offset.end_column);
    if (end_point.row, end_point.column) < (start_point.row, start_point.column) {
        return range;
    }

    Range {
        start_point,
        end_point,
        start_byte: lines.offset(start_point),
        end_byte: lines.offset(end_point),
    }
}

fn shift(point: Point, rows: i32, columns: i32) -> Point {
    Point {
        row: point.row.saturating_add_signed(rows as isize),
        column: point.column.saturating_add_signed(columns as isize),
    }
}

/// The ranges of `node` that make up an injected region.
///
/// With `include_children`, that is the whole (offset-adjusted) range.
/// Otherwise the named children are cut out, leaving the text between them.
pub fn content_ranges(node: Node, range: Range, include_children: bool, out: &mut Vec<Range>) {
    if include_children || node.named_child_count() == 0 {
        push_range(out, range);
        return;
    }

    let mut cursor = node.walk();
    let mut start_point = range.start_point;
    let mut start_byte = range.start_byte;

    for child in node.named_children(&mut cursor) {
        let child = child.range();
        if (child.start_point.row, child.start_point.column) > (start_point.row, start_point.column)
        {
            push_range(
                out,
                Range {
                    start_point,
                    start_byte,
                    end_point: child.start_point,
                    end_byte: child.start_byte,
                },
            );
        }
        start_point = child.end_point;
        start_byte = child.end_byte;
    }

    if start_byte < range.end_byte {
        push_range(
            out,
            Range {
                start_point,
                start_byte,
                end_point: range.end_point,
                end_byte: range.end_byte,
            },
        );
    }
}

/// Put `ranges` into the shape tree-sitter demands of included ranges: sorted,
/// non-empty and non-overlapping.
pub fn normalize(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.retain(|range| range.start_byte < range.end_byte);
    ranges.sort_by_key(|range| (range.start_byte, range.end_byte));

    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(previous) if range.start_byte <= previous.end_byte => {
                if range.end_byte > previous.end_byte {
                    previous.end_byte = range.end_byte;
                    previous.end_point = range.end_point;
                }
            }
            _ => merged.push(range),
        }
    }
    merged
}

fn push_range(out: &mut Vec<Range>, range: Range) {
    if range.start_byte < range.end_byte {
        out.push(range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: usize, end: usize) -> Range {
        Range {
            start_byte: start,
            end_byte: end,
            start_point: Point::new(0, start),
            end_point: Point::new(0, end),
        }
    }

    #[test]
    fn trims_a_range_by_columns() {
        let lines = LineIndex::new("`select 1`");
        let trimmed = offset_range(
            range(0, 10),
            Some(Offset {
                start_row: 0,
                start_column: 1,
                end_row: 0,
                end_column: -1,
            }),
            &lines,
        );
        assert_eq!((trimmed.start_byte, trimmed.end_byte), (1, 9));
    }

    #[test]
    fn row_offsets_move_lines_and_keep_columns() {
        let lines = LineIndex::new("<<<\nbody\n>>>");
        let trimmed = offset_range(
            Range {
                start_byte: 0,
                end_byte: 12,
                start_point: Point::new(0, 0),
                end_point: Point::new(2, 3),
            },
            Some(Offset {
                start_row: 1,
                start_column: 0,
                end_row: -1,
                end_column: 0,
            }),
            &lines,
        );
        // Both points move one line inwards; the columns are carried along,
        // so the end lands at column 3 of the middle line rather than its end.
        assert_eq!((trimmed.start_byte, trimmed.end_byte), (4, 7));
    }

    #[test]
    fn ignores_an_offset_that_would_invert_the_range() {
        let lines = LineIndex::new("ab");
        let inverted = offset_range(
            range(0, 2),
            Some(Offset {
                start_row: 0,
                start_column: 5,
                end_row: 0,
                end_column: -5,
            }),
            &lines,
        );
        assert_eq!((inverted.start_byte, inverted.end_byte), (0, 2));
    }

    #[test]
    fn no_offset_leaves_the_range_alone() {
        let lines = LineIndex::new("ab");
        assert_eq!(offset_range(range(0, 2), None, &lines), range(0, 2));
    }

    #[test]
    fn normalizing_sorts_merges_and_drops_empties() {
        let normalized = normalize(vec![range(5, 8), range(0, 3), range(3, 4), range(9, 9)]);
        let bytes: Vec<_> = normalized
            .iter()
            .map(|r| (r.start_byte, r.end_byte))
            .collect();
        assert_eq!(bytes, [(0, 4), (5, 8)]);
    }
}
