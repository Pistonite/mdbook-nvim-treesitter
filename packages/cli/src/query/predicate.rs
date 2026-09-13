//! The conditional predicates tree-sitter does not evaluate itself.
//!
//! tree-sitter handles `#eq?`, `#any-of?` and (after [`super::rewrite`] has
//! moved Vim's regexes out of the way) nothing else that Neovim's queries use.
//! Everything left over arrives as a "general predicate", which tree-sitter
//! parses but never checks -- and an unchecked predicate is not a no-op, it
//! makes its pattern fire *unconditionally*. A rule meant for `SCREAMING_CASE`
//! identifiers would then colour every identifier as a constant, so each of
//! these has to be evaluated here.
//!
//! Names follow Neovim's scheme: a base predicate `foo?` also exists as
//! `not-foo?` (negated), `any-foo?` (satisfied by one node of a multi-node
//! capture) and `any-not-foo?`.

use regex::bytes::Regex;
use tree_sitter::{Node, Query, QueryMatch, QueryPredicateArg};

use crate::query::{lua_pattern, vim_regex};

/// Every general predicate of every pattern in a query.
#[derive(Debug)]
pub struct Predicates {
    patterns: Vec<Vec<Predicate>>,
    /// Predicates we did not recognise, reported once per query.
    unsupported: Vec<String>,
}

#[derive(Debug)]
struct Predicate {
    capture: u32,
    test: Test,
    /// False for the `not-` spellings.
    positive: bool,
    /// True for the `any-` spellings: one node is enough.
    any: bool,
}

#[derive(Debug)]
enum Test {
    /// The capture's text matches a Lua pattern.
    LuaMatch(Regex),
    /// The capture's text matches a Vim regex.
    VimMatch(Regex),
    /// The capture's text contains one of these substrings.
    Contains(Vec<String>),
    /// The captured node's type is one of these.
    Kind(Vec<String>),
    /// Some ancestor of the captured node has one of these types.
    Ancestor(Vec<String>),
    /// The captured node's immediate parent has one of these types.
    Parent(Vec<String>),
    /// A predicate we could not compile. It never holds, so a pattern relying
    /// on it under-highlights rather than highlighting everything.
    Unsupported,
}

impl Predicates {
    /// Compile the general predicates of every pattern in `query`.
    pub fn compile(query: &Query) -> Self {
        let mut unsupported = Vec::new();
        let patterns = (0..query.pattern_count())
            .map(|index| {
                query
                    .general_predicates(index)
                    .iter()
                    .filter_map(|predicate| {
                        let operator = predicate.operator.as_ref();
                        // Directives annotate a match rather than filtering it;
                        // they are read by `super::metadata`.
                        if operator.ends_with('!') {
                            return None;
                        }
                        match Predicate::compile(operator, &predicate.args) {
                            Ok(compiled) => Some(compiled),
                            Err(error) => {
                                if !unsupported.iter().any(|seen| seen == &error) {
                                    unsupported.push(error);
                                }
                                Some(Predicate {
                                    capture: 0,
                                    test: Test::Unsupported,
                                    positive: true,
                                    any: false,
                                })
                            }
                        }
                    })
                    .collect()
            })
            .collect();

        Self {
            patterns,
            unsupported,
        }
    }

    /// Descriptions of predicates that could not be compiled.
    pub fn unsupported(&self) -> &[String] {
        &self.unsupported
    }

    /// Whether every predicate on this match's pattern holds.
    pub fn accepts(&self, matched: &QueryMatch, source: &[u8]) -> bool {
        self.patterns
            .get(matched.pattern_index)
            .is_none_or(|predicates| predicates.iter().all(|p| p.holds(matched, source)))
    }
}

impl Predicate {
    fn compile(operator: &str, args: &[QueryPredicateArg]) -> Result<Self, String> {
        let (base, positive, any) = split_name(operator);

        let [QueryPredicateArg::Capture(capture), rest @ ..] = args else {
            return Err(format!("#{operator} does not start with a capture"));
        };

        let test = match base {
            "lua-match" => Test::LuaMatch(
                lua_pattern::compile(&single_string(operator, rest)?)
                    .map_err(|e| format!("#{operator}: {e:#}"))?,
            ),
            "vim-match" => Test::VimMatch(
                vim_regex::compile(&single_string(operator, rest)?)
                    .map_err(|e| format!("#{operator}: {e:#}"))?,
            ),
            "contains" => Test::Contains(strings(operator, rest)?),
            "kind-eq" => Test::Kind(strings(operator, rest)?),
            "has-ancestor" => Test::Ancestor(strings(operator, rest)?),
            "has-parent" => Test::Parent(strings(operator, rest)?),
            _ => return Err(format!("#{operator} is not a predicate we know")),
        };

        Ok(Self {
            capture: *capture,
            test,
            positive,
            any,
        })
    }

    fn holds(&self, matched: &QueryMatch, source: &[u8]) -> bool {
        if matches!(self.test, Test::Unsupported) {
            return false;
        }

        let mut nodes = matched.nodes_for_capture_index(self.capture).peekable();
        // Neovim treats a predicate on a capture that matched nothing as
        // satisfied, so an optional capture does not veto its pattern.
        if nodes.peek().is_none() {
            return true;
        }

        if self.any {
            nodes.any(|node| self.test.holds(node, source) == self.positive)
        } else {
            nodes.all(|node| self.test.holds(node, source) == self.positive)
        }
    }
}

impl Test {
    fn holds(&self, node: Node, source: &[u8]) -> bool {
        match self {
            Test::LuaMatch(regex) | Test::VimMatch(regex) => regex.is_match(text(node, source)),
            Test::Contains(needles) => {
                let haystack = text(node, source);
                needles
                    .iter()
                    .any(|needle| contains(haystack, needle.as_bytes()))
            }
            Test::Kind(kinds) => kinds.iter().any(|kind| kind == node.kind()),
            Test::Ancestor(kinds) => std::iter::successors(node.parent(), Node::parent)
                .any(|ancestor| kinds.iter().any(|kind| kind == ancestor.kind())),
            Test::Parent(kinds) => node
                .parent()
                .is_some_and(|parent| kinds.iter().any(|kind| kind == parent.kind())),
            Test::Unsupported => false,
        }
    }
}

/// Split `any-not-lua-match?` into `("lua-match", positive, any)`.
fn split_name(operator: &str) -> (&str, bool, bool) {
    let base = operator.strip_suffix('?').unwrap_or(operator);
    let (base, any) = match base.strip_prefix("any-") {
        Some(rest) => (rest, true),
        None => (base, false),
    };
    match base.strip_prefix("not-") {
        Some(rest) => (rest, false, any),
        None => (base, true, any),
    }
}

fn single_string(operator: &str, args: &[QueryPredicateArg]) -> Result<String, String> {
    match strings(operator, args)?.as_slice() {
        [only] => Ok(only.clone()),
        other => Err(format!(
            "#{operator} takes one string argument, got {}",
            other.len()
        )),
    }
}

fn strings(operator: &str, args: &[QueryPredicateArg]) -> Result<Vec<String>, String> {
    args.iter()
        .map(|arg| match arg {
            QueryPredicateArg::String(value) => Ok(value.to_string()),
            QueryPredicateArg::Capture(_) => {
                Err(format!("#{operator} takes string arguments, got a capture"))
            }
        })
        .collect()
}

fn text<'a>(node: Node, source: &'a [u8]) -> &'a [u8] {
    source.get(node.byte_range()).unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_any_and_not_prefixes() {
        assert_eq!(split_name("lua-match?"), ("lua-match", true, false));
        assert_eq!(split_name("not-lua-match?"), ("lua-match", false, false));
        assert_eq!(split_name("any-lua-match?"), ("lua-match", true, true));
        assert_eq!(split_name("any-not-lua-match?"), ("lua-match", false, true));
        assert_eq!(split_name("has-ancestor?"), ("has-ancestor", true, false));
        assert_eq!(split_name("not-has-parent?"), ("has-parent", false, false));
    }

    #[test]
    fn rejects_predicates_it_does_not_know() {
        let error = Predicate::compile("cl-standard-function?", &[QueryPredicateArg::Capture(0)])
            .unwrap_err();
        assert!(error.contains("not a predicate we know"), "{error}");
    }

    #[test]
    fn rejects_predicates_with_the_wrong_shape() {
        assert!(Predicate::compile("lua-match?", &[]).is_err());
        assert!(
            Predicate::compile(
                "lua-match?",
                &[QueryPredicateArg::Capture(0), QueryPredicateArg::Capture(1)]
            )
            .is_err()
        );
        assert!(
            Predicate::compile(
                "lua-match?",
                &[
                    QueryPredicateArg::Capture(0),
                    QueryPredicateArg::String("a".into()),
                    QueryPredicateArg::String("b".into()),
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn compiles_the_predicates_the_corpus_uses() {
        let capture = || QueryPredicateArg::Capture(0);
        let string = |s: &str| QueryPredicateArg::String(s.into());
        for (operator, args) in [
            ("lua-match?", vec![capture(), string("^[A-Z][A-Z%d_]*$")]),
            ("not-lua-match?", vec![capture(), string("^_")]),
            ("vim-match?", vec![capture(), string(r"\c^return$")]),
            (
                "has-ancestor?",
                vec![capture(), string("call"), string("macro")],
            ),
            ("not-has-parent?", vec![capture(), string("block")]),
            ("kind-eq?", vec![capture(), string("identifier")]),
            ("contains?", vec![capture(), string("TODO")]),
        ] {
            Predicate::compile(operator, &args).unwrap_or_else(|e| panic!("{operator}: {e}"));
        }
    }

    #[test]
    fn substring_search_matches_lua_semantics() {
        assert!(contains(b"hello world", b"o w"));
        assert!(!contains(b"hello", b"world"));
        assert!(contains(b"hello", b""));
        assert!(!contains(b"hi", b"hello"));
    }
}
