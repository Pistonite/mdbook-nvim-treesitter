//! Converting between byte offsets and `(row, column)` points.
//!
//! tree-sitter tracks both, but the `#offset!` directive is expressed purely
//! in rows and columns, so applying it means going back to bytes afterwards.

use tree_sitter::Point;

/// A byte-offset index of the start of every line in a document.
pub struct LineIndex {
    starts: Vec<usize>,
    length: usize,
}

impl LineIndex {
    /// Index `text`.
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(index, _)| index + 1),
        );
        Self {
            starts,
            length: text.len(),
        }
    }

    /// The byte offset of a point, clamped into the document.
    ///
    /// tree-sitter columns are byte counts within a line, so a point past the
    /// end of its line clamps to the line's end rather than wrapping.
    pub fn offset(&self, point: Point) -> usize {
        let Some(&start) = self.starts.get(point.row) else {
            return self.length;
        };
        let end = self
            .starts
            .get(point.row + 1)
            .map_or(self.length, |next| next - 1);
        (start + point.column).min(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_points_to_offsets() {
        let index = LineIndex::new("ab\ncde\n\nf");
        assert_eq!(index.offset(Point::new(0, 0)), 0);
        assert_eq!(index.offset(Point::new(0, 2)), 2);
        assert_eq!(index.offset(Point::new(1, 0)), 3);
        assert_eq!(index.offset(Point::new(1, 3)), 6);
        assert_eq!(index.offset(Point::new(2, 0)), 7);
        assert_eq!(index.offset(Point::new(3, 1)), 9);
    }

    #[test]
    fn clamps_points_outside_the_document() {
        let index = LineIndex::new("ab\ncd");
        // Past the end of a line stops at the newline, not in the next line.
        assert_eq!(index.offset(Point::new(0, 99)), 2);
        assert_eq!(index.offset(Point::new(99, 0)), 5);
    }

    #[test]
    fn handles_an_empty_document() {
        let index = LineIndex::new("");
        assert_eq!(index.offset(Point::new(0, 0)), 0);
        assert_eq!(index.offset(Point::new(5, 5)), 0);
    }
}
