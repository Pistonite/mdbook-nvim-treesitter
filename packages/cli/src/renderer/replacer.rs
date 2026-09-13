//! Replacing byte ranges in a document.

use std::ops::Range;

use cu::pre::*;

#[derive(Default)]
pub struct Replacer {
    replacements: Vec<(Range<usize>, String)>,
}
impl Replacer {
    pub fn add(&mut self, range: Range<usize>, content: String) {
        self.replacements.push((range, content));
    }
    /// Replace each range of `content` with its replacement text.
    ///
    /// Ranges may be given in any order. Overlapping ranges are rejected rather
    /// than resolved: an overlap means two highlight tasks claimed the same code,
    /// which is a bug upstream, and silently picking one would hide it.
    pub fn apply(self, content: &str) -> cu::Result<String> {
        let replacements = self.replacements;
        if replacements.is_empty() {
            return Ok(content.to_string());
        }

        let mut replacements = replacements;
        replacements.sort_by_key(|(range, _)| (range.start, range.end));

        let mut out = String::with_capacity(content.len());
        let mut cursor = 0;

        for (range, replacement) in replacements {
            cu::ensure!(
                range.start >= cursor,
                "overlapping replacements at byte {} (previous ended at {cursor})",
                range.start
            )?;
            cu::ensure!(
                range.end <= content.len() && range.start <= range.end,
                "replacement range {}..{} is outside the document ({} bytes)",
                range.start,
                range.end,
                content.len()
            )?;

            out.push_str(cu::check!(
                content.get(cursor..range.start),
                "replacement does not start on a character boundary"
            )?);
            out.push_str(&replacement);
            cursor = range.end;
        }

        out.push_str(cu::check!(
            content.get(cursor..),
            "replacement does not end on a character boundary"
        )?);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(content: &str, replacements: Vec<(Range<usize>, String)>) -> cu::Result<String> {
        let mut replacer = Replacer::default();
        replacements
            .into_iter()
            .for_each(|x| replacer.add(x.0, x.1));
        replacer.apply(content)
    }

    #[test]
    fn an_empty_list_returns_the_original() {
        assert_eq!(apply("hello", vec![]).unwrap(), "hello");
    }

    #[test]
    fn replaces_in_source_order_regardless_of_input_order() {
        let out = apply(
            "one two three",
            vec![(8..13, "THREE".into()), (0..3, "ONE".into())],
        )
        .unwrap();
        assert_eq!(out, "ONE two THREE");
    }

    #[test]
    fn handles_replacements_at_the_edges() {
        assert_eq!(apply("abc", vec![(0..3, "x".into())]).unwrap(), "x");
        assert_eq!(apply("abc", vec![(3..3, "!".into())]).unwrap(), "abc!");
        assert_eq!(apply("abc", vec![(0..0, "!".into())]).unwrap(), "!abc");
    }

    #[test]
    fn adjacent_replacements_are_fine() {
        let out = apply("abcd", vec![(0..2, "X".into()), (2..4, "Y".into())]).unwrap();
        assert_eq!(out, "XY");
    }

    #[test]
    fn rejects_overlapping_ranges() {
        let error = apply("abcdef", vec![(0..3, "X".into()), (2..5, "Y".into())]).unwrap_err();
        assert!(format!("{error:#}").contains("overlapping"), "{error:#}");
    }

    #[test]
    fn rejects_ranges_outside_the_document() {
        assert!(apply("abc", vec![(0..99, "X".into())]).is_err());

        // An inverted range: built by hand, since the literal `2..1` is a
        // compile-time lint.
        let inverted = Range { start: 2, end: 1 };
        assert!(apply("abc", vec![(inverted, "X".into())]).is_err());
    }

    #[test]
    fn rejects_ranges_that_split_a_character() {
        assert!(apply("añb", vec![(1..2, "X".into())]).is_err());
    }
}
