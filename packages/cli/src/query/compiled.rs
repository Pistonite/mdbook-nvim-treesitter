//! A query, ready to run, with everything Neovim attaches to it.

use tree_sitter::{Language, Query, QueryMatch};

use crate::query::{Metadata, PatternMetadata, Predicates, rewrite};

/// What a capture means when it is painted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureRole {
    /// Not a highlight: a helper capture, or an editor-only concern such as
    /// spell checking. Leaves whatever is underneath alone.
    Ignored,
    /// `@none`: explicitly *no* highlight, clearing anything underneath.
    Cleared,
    /// A highlight group, e.g. `keyword.function`.
    Group(String),
}

/// A compiled query together with the predicates and directives that Neovim
/// applies to it.
pub struct CompiledQuery {
    query: Query,
    predicates: Predicates,
    metadata: Metadata,
    roles: Vec<CaptureRole>,
}

impl CompiledQuery {
    /// Compile `source` against `language`.
    ///
    /// An empty source compiles to an empty query, which simply never matches.
    pub fn compile(language: &Language, source: &str) -> cu::Result<Self> {
        let prepared = rewrite::for_compilation(source);
        let query = Query::new(language, &prepared).map_err(|error| {
            // tree-sitter reports a row within the assembled text, which spans
            // several inherited files; quoting the offending line is far more
            // useful than the number on its own.
            let line = prepared.lines().nth(error.row).unwrap_or_default();
            cu::fmterr!(
                "invalid query at line {}: {}\n  {line}",
                error.row + 1,
                error.message
            )
        })?;

        let roles = query.capture_names().iter().map(|n| role_of(n)).collect();
        Ok(Self {
            predicates: Predicates::compile(&query),
            metadata: Metadata::compile(&query)?,
            roles,
            query,
        })
    }

    /// The underlying tree-sitter query.
    pub fn raw(&self) -> &Query {
        &self.query
    }

    /// Whether the query has no patterns, and so can be skipped entirely.
    pub fn is_empty(&self) -> bool {
        self.query.pattern_count() == 0
    }

    /// Whether every general predicate on this match's pattern holds.
    pub fn accepts(&self, matched: &QueryMatch, source: &[u8]) -> bool {
        self.predicates.accepts(matched, source)
    }

    /// The directives attached to a pattern.
    pub fn pattern(&self, index: usize) -> &PatternMetadata {
        self.metadata.pattern(index)
    }

    /// What a capture index means when painted.
    pub fn role(&self, capture: u32) -> &CaptureRole {
        self.roles
            .get(capture as usize)
            .unwrap_or(&CaptureRole::Ignored)
    }

    /// The index of a capture by name.
    pub fn capture_index(&self, name: &str) -> Option<u32> {
        self.query.capture_index_for_name(name)
    }

    /// Predicates in this query that could not be compiled, and so never hold.
    pub fn unsupported_predicates(&self) -> &[String] {
        self.predicates.unsupported()
    }
}

/// Captures that exist to support a pattern rather than to colour anything.
///
/// `@spell` and `@nospell` drive Neovim's spell checker, `@conceal` its
/// concealing, and a leading `_` is the query convention for a helper capture
/// referenced only by predicates.
const NOT_HIGHLIGHTS: &[&str] = &["spell", "nospell", "conceal", "conceal_lines"];

fn role_of(capture: &str) -> CaptureRole {
    if capture.starts_with('_')
        || NOT_HIGHLIGHTS.contains(&capture)
        || capture.starts_with("injection.")
        || capture.starts_with("local.")
    {
        return CaptureRole::Ignored;
    }
    if capture == "none" {
        return CaptureRole::Cleared;
    }
    CaptureRole::Group(capture.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_capture_names() {
        assert_eq!(
            role_of("keyword.function"),
            CaptureRole::Group("keyword.function".into())
        );
        assert_eq!(role_of("none"), CaptureRole::Cleared);
        assert_eq!(role_of("_helper"), CaptureRole::Ignored);
        assert_eq!(role_of("spell"), CaptureRole::Ignored);
        assert_eq!(role_of("nospell"), CaptureRole::Ignored);
        assert_eq!(role_of("conceal"), CaptureRole::Ignored);
        assert_eq!(role_of("injection.content"), CaptureRole::Ignored);
    }
}
