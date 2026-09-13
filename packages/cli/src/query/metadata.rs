//! Per-pattern metadata attached by `#set!` and the range/text directives.
//!
//! Directives are not conditions: they never decide whether a pattern matches,
//! they annotate the match. Neovim uses them for the highlight priority, for
//! naming an injected language, and for trimming a capture's range or text
//! before it is used. All of it is static per pattern, so it is read once when
//! the query is compiled rather than on every match.

use std::collections::HashMap;

use regex::bytes::Regex;
use tree_sitter::{Query, QueryPredicateArg};

use crate::query::{lua_pattern, rewrite};

/// Neovim's default highlight priority when a pattern sets none.
pub const DEFAULT_PRIORITY: u32 = 100;

/// Everything the directives on one pattern say about it.
#[derive(Debug, Default)]
pub struct PatternMetadata {
    priority: Option<u32>,
    capture_priority: HashMap<u32, u32>,
    /// Row/column deltas applied to a capture's range by `#offset!`.
    offsets: HashMap<u32, Offset>,
    /// Text substitutions applied to a capture by `#gsub!`.
    gsubs: HashMap<u32, Gsub>,
    /// What the pattern says about the language it injects.
    pub injection: Injection,
}

/// A `#offset!` directive: four deltas on a capture's `(row, column)` bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Offset {
    pub start_row: i32,
    pub start_column: i32,
    pub end_row: i32,
    pub end_column: i32,
}

#[derive(Debug)]
struct Gsub {
    pattern: Regex,
    replacement: String,
}

/// What an injections pattern says about the language it embeds.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Injection {
    /// A fixed language name, from `(#set! injection.language "...")`.
    pub language: Option<String>,
    /// Parse every match of this pattern as one document rather than
    /// separately, so constructs can span the gaps between them.
    pub combined: bool,
    /// Inject the content node whole. Without this the content node's named
    /// children are cut out of the injected region, which is how a template
    /// language keeps its interpolations to itself.
    pub include_children: bool,
    /// Inject the host language into itself.
    pub own_language: bool,
    /// Inject the language of the layer that contains this one.
    pub parent_language: bool,
}

/// The directives of every pattern in a query.
#[derive(Debug)]
pub struct Metadata {
    patterns: Vec<PatternMetadata>,
}

impl Metadata {
    /// Read the directives of every pattern in `query`.
    pub fn compile(query: &Query) -> cu::Result<Self> {
        let patterns = (0..query.pattern_count())
            .map(|index| PatternMetadata::compile(query, index))
            .collect::<cu::Result<Vec<_>>>()?;
        Ok(Self { patterns })
    }

    /// The directives of one pattern.
    pub fn pattern(&self, index: usize) -> &PatternMetadata {
        static EMPTY: std::sync::OnceLock<PatternMetadata> = std::sync::OnceLock::new();
        self.patterns
            .get(index)
            .unwrap_or_else(|| EMPTY.get_or_init(PatternMetadata::default))
    }
}

impl PatternMetadata {
    fn compile(query: &Query, index: usize) -> cu::Result<Self> {
        let mut metadata = Self::default();

        for predicate in query.general_predicates(index) {
            match predicate.operator.as_ref() {
                rewrite::NVIM_SET => metadata.apply_set(&predicate.args),
                "offset!" => {
                    if let Some((capture, offset)) = parse_offset(&predicate.args) {
                        metadata.offsets.insert(capture, offset);
                    }
                }
                "gsub!" => {
                    if let Some((capture, gsub)) = parse_gsub(&predicate.args)? {
                        metadata.gsubs.insert(capture, gsub);
                    }
                }
                _ => {}
            }
        }

        Ok(metadata)
    }

    fn apply_set(&mut self, args: &[QueryPredicateArg]) {
        let Some(set) = parse_set(args) else {
            return;
        };
        match (set.key.as_str(), set.capture) {
            ("priority", None) => self.priority = set.number(),
            ("priority", Some(capture)) => {
                if let Some(priority) = set.number() {
                    self.capture_priority.insert(capture, priority);
                }
            }
            ("injection.language", _) => self.injection.language = set.text(),
            ("injection.combined", _) => self.injection.combined = true,
            ("injection.include-children", _) => self.injection.include_children = true,
            ("injection.self", _) => self.injection.own_language = true,
            ("injection.parent", _) => self.injection.parent_language = true,
            // Everything else is for an editor, not for us: `conceal`, `url`,
            // `bo.commentstring` and friends have no meaning in a static page.
            _ => {}
        }
    }

    /// The priority a capture's highlight is painted with.
    pub fn priority(&self, capture: u32) -> u32 {
        self.capture_priority
            .get(&capture)
            .or(self.priority.as_ref())
            .copied()
            .unwrap_or(DEFAULT_PRIORITY)
    }

    /// The `#offset!` applied to a capture, if any.
    pub fn offset(&self, capture: u32) -> Option<Offset> {
        self.offsets.get(&capture).copied()
    }

    /// Apply any `#gsub!` for a capture to text taken from it.
    pub fn substitute(&self, capture: u32, text: &str) -> String {
        match self.gsubs.get(&capture) {
            None => text.to_string(),
            Some(gsub) => String::from_utf8_lossy(
                &gsub
                    .pattern
                    .replace_all(text.as_bytes(), gsub.replacement.as_bytes()),
            )
            .into_owned(),
        }
    }
}

/// A `#set!` directive: an optional capture it applies to, a key, and an
/// optional value that may itself be a capture.
struct Set {
    capture: Option<u32>,
    key: String,
    value: Option<String>,
}

impl Set {
    fn text(&self) -> Option<String> {
        self.value.clone()
    }

    fn number(&self) -> Option<u32> {
        self.value.as_ref()?.parse().ok()
    }
}

/// Parse the arguments of a `#set!`.
///
/// Neovim allows `(#set! key value)`, `(#set! @capture key value)` and
/// `(#set! @capture key @other)`; the last has no string value, and is only
/// used for editor-only keys, so its value is simply dropped.
fn parse_set(args: &[QueryPredicateArg]) -> Option<Set> {
    let mut capture = None;
    let mut strings = Vec::new();

    for arg in args {
        match arg {
            QueryPredicateArg::Capture(index) if capture.is_none() && strings.is_empty() => {
                capture = Some(*index);
            }
            QueryPredicateArg::Capture(_) => {}
            QueryPredicateArg::String(value) => strings.push(value.to_string()),
        }
    }

    let mut strings = strings.into_iter();
    Some(Set {
        capture,
        key: strings.next()?,
        value: strings.next(),
    })
}

fn parse_offset(args: &[QueryPredicateArg]) -> Option<(u32, Offset)> {
    let [QueryPredicateArg::Capture(capture), rest @ ..] = args else {
        return None;
    };
    let mut deltas = [0i32; 4];
    for (slot, arg) in deltas.iter_mut().zip(rest) {
        let QueryPredicateArg::String(value) = arg else {
            return None;
        };
        *slot = value.parse().ok()?;
    }
    Some((
        *capture,
        Offset {
            start_row: deltas[0],
            start_column: deltas[1],
            end_row: deltas[2],
            end_column: deltas[3],
        },
    ))
}

fn parse_gsub(args: &[QueryPredicateArg]) -> cu::Result<Option<(u32, Gsub)>> {
    let [
        QueryPredicateArg::Capture(capture),
        QueryPredicateArg::String(pattern),
        QueryPredicateArg::String(replacement),
    ] = args
    else {
        return Ok(None);
    };
    Ok(Some((
        *capture,
        Gsub {
            pattern: lua_pattern::compile(pattern)?,
            replacement: lua_replacement_to_regex(replacement),
        },
    )))
}

/// Translate a Lua `gsub` replacement into a regex replacement.
///
/// Lua writes capture references as `%1`; a regex writes them as `${1}`. `$`
/// is ordinary in Lua, so it has to be escaped on the way out.
fn lua_replacement_to_regex(replacement: &str) -> String {
    let bytes = replacement.as_bytes();
    let mut out = String::with_capacity(replacement.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 1 < bytes.len() => {
                let next = bytes[index + 1];
                if next.is_ascii_digit() {
                    out.push_str(&format!("${{{}}}", next as char));
                } else {
                    out.push(next as char);
                }
                index += 2;
            }
            b'$' => {
                out.push_str("$$");
                index += 1;
            }
            _ => {
                let character = replacement[index..].chars().next().expect("boundary");
                out.push(character);
                index += character.len_utf8();
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_lua_capture_references() {
        assert_eq!(lua_replacement_to_regex("%1"), "${1}");
        assert_eq!(lua_replacement_to_regex("a%2b"), "a${2}b");
        assert_eq!(lua_replacement_to_regex("100%%"), "100%");
        assert_eq!(lua_replacement_to_regex("cost: $5"), "cost: $$5");
    }

    #[test]
    fn substitutes_like_lua_gsub() {
        // Taken from nix/injections.scm: pull the language out of `/* sh */`.
        let gsub = Gsub {
            pattern: lua_pattern::compile("/%*%s*([%w%p]+)%s*%*/").unwrap(),
            replacement: lua_replacement_to_regex("%1"),
        };
        let mut metadata = PatternMetadata::default();
        metadata.gsubs.insert(0, gsub);

        assert_eq!(metadata.substitute(0, "/* bash */"), "bash");
        assert_eq!(metadata.substitute(1, "/* bash */"), "/* bash */");
    }

    #[test]
    fn capture_priority_overrides_pattern_priority() {
        let mut metadata = PatternMetadata {
            priority: Some(105),
            ..Default::default()
        };
        metadata.capture_priority.insert(7, 120);

        assert_eq!(metadata.priority(0), 105);
        assert_eq!(metadata.priority(7), 120);
        assert_eq!(PatternMetadata::default().priority(0), DEFAULT_PRIORITY);
    }

    #[test]
    fn parses_every_set_shape_neovim_allows() {
        let set = parse_set(&[
            QueryPredicateArg::String("priority".into()),
            QueryPredicateArg::String("105".into()),
        ])
        .unwrap();
        assert_eq!(
            (set.capture, set.key.as_str(), set.number()),
            (None, "priority", Some(105))
        );

        let set = parse_set(&[
            QueryPredicateArg::Capture(2),
            QueryPredicateArg::String("priority".into()),
            QueryPredicateArg::String("120".into()),
        ])
        .unwrap();
        assert_eq!(set.capture, Some(2));
        assert_eq!(set.number(), Some(120));

        // `(#set! @url url @url)`: a second capture as the value. tree-sitter
        // refuses to parse this, which is why we do it ourselves.
        let set = parse_set(&[
            QueryPredicateArg::Capture(1),
            QueryPredicateArg::String("url".into()),
            QueryPredicateArg::Capture(1),
        ])
        .unwrap();
        assert_eq!(
            (set.capture, set.key.as_str(), set.value),
            (Some(1), "url", None)
        );

        // A flag with no value at all.
        let set = parse_set(&[QueryPredicateArg::String("injection.combined".into())]).unwrap();
        assert_eq!(set.value, None);

        assert!(parse_set(&[]).is_none());
    }

    #[test]
    fn parses_offset_arguments() {
        let args = [
            QueryPredicateArg::Capture(3),
            QueryPredicateArg::String("0".into()),
            QueryPredicateArg::String("1".into()),
            QueryPredicateArg::String("0".into()),
            QueryPredicateArg::String("-1".into()),
        ];
        assert_eq!(
            parse_offset(&args),
            Some((
                3,
                Offset {
                    start_row: 0,
                    start_column: 1,
                    end_row: 0,
                    end_column: -1
                }
            ))
        );
    }

    #[test]
    fn ignores_a_malformed_offset() {
        assert_eq!(parse_offset(&[]), None);
        assert_eq!(parse_offset(&[QueryPredicateArg::String("0".into())]), None);
    }
}
