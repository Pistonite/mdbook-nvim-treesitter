//! The highlighter: parse, run the queries, paint, recurse into injections.
//!
//! This follows Neovim's `LanguageTree` and `TSHighlighter` rather than
//! tree-sitter's own `tree-sitter-highlight`, because nvim-treesitter's
//! queries are written against Neovim's semantics: inherited query files,
//! Lua-pattern and Vim-regex predicates, `#set! priority`, and injections that
//! cut out their content node's children. Reproducing those here is what lets
//! the queries be used exactly as shipped.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use cu::pre::*;
use tree_sitter::{Language, Parser, QueryCursor, Range, StreamingIterator as _, Tree};

use crate::query::{CaptureRole, CompiledQuery, HIGHLIGHTS, INJECTIONS, QuerySource};
use crate::registry;

use crate::highlight::ParserLibrary;
use crate::highlight::canvas::{Canvas, Span};
use crate::highlight::injection;
use crate::highlight::lines::LineIndex;

/// How deep injections may nest before we stop descending.
///
/// A query can inject a language into itself, so some bound is needed; eight
/// is far beyond anything real (markdown in a docstring in HTML is three).
const MAX_INJECTION_DEPTH: usize = 8;

/// The capture an injections query uses for the embedded text.
const INJECTION_CONTENT: &str = "injection.content";
/// The capture an injections query uses to name the embedded language.
const INJECTION_LANGUAGE: &str = "injection.language";

/// The result of highlighting one piece of code.
#[derive(Debug)]
pub struct Highlighted<'a> {
    /// Spans covering the whole input, in order.
    pub spans: Vec<Span<'a>>,
    /// Languages an injection asked for that were not loaded.
    ///
    /// The corresponding regions are left unhighlighted. Callers can use this
    /// to install the missing parsers and highlight again.
    pub missing: BTreeSet<String>,
}

/// A loaded language: its parser and its queries.
///
/// Field order matters: the queries and the [`Language`] borrow from the
/// library's memory, so the library must be dropped last.
struct Loaded {
    highlights: CompiledQuery,
    injections: CompiledQuery,
    content_capture: Option<u32>,
    language_capture: Option<u32>,
    language: Language,
    _library: ParserLibrary,
}

/// Highlights code using nvim-treesitter's queries.
pub struct Highlighter {
    queries: QuerySource,
    languages: HashMap<String, Loaded>,
    warnings: Vec<String>,
}

impl Highlighter {
    /// Create a highlighter that reads queries from `queries`.
    pub fn new(queries: QuerySource) -> Self {
        Self {
            queries,
            languages: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// Load a language's parser and compile its queries.
    ///
    /// `symbol` is the grammar name exported by the library, without the
    /// `tree_sitter_` prefix.
    pub fn load(&mut self, language: &str, parser: &Path, symbol: &str) -> cu::Result<()> {
        if self.languages.contains_key(language) {
            return Ok(());
        }

        let library = ParserLibrary::load(parser, symbol)?;
        let compile = |kind: &str| -> cu::Result<CompiledQuery> {
            let source = self.queries.load(language, kind)?;
            cu::check!(
                CompiledQuery::compile(library.language(), &source),
                "in the {language} {kind} query"
            )
        };

        let highlights = compile(HIGHLIGHTS)?;
        let injections = compile(INJECTIONS)?;

        let mut warnings = Vec::new();
        for query in [&highlights, &injections] {
            for unsupported in query.unsupported_predicates() {
                warnings.push(format!("{language}: {unsupported}"));
            }
        }
        self.warnings.append(&mut warnings);

        self.languages.insert(
            language.to_string(),
            Loaded {
                content_capture: injections.capture_index(INJECTION_CONTENT),
                language_capture: injections.capture_index(INJECTION_LANGUAGE),
                highlights,
                injections,
                language: library.language().clone(),
                _library: library,
            },
        );
        Ok(())
    }

    /// Whether a language's parser and queries are loaded.
    pub fn is_loaded(&self, language: &str) -> bool {
        self.languages.contains_key(language)
    }

    /// Problems found while compiling queries, worth showing once per run.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Highlight `code` as `language`.
    ///
    /// An unloaded language is not an error: the code comes back as a single
    /// unhighlighted span and the language is reported in
    /// [`Highlighted::missing`].
    pub fn highlight<'a>(&'a self, language: &str, code: &str) -> cu::Result<Highlighted<'a>> {
        let mut run = Run {
            canvas: Canvas::new(code.len()),
            lines: LineIndex::new(code),
            missing: BTreeSet::new(),
            code,
        };
        self.paint(&mut run, language, None, None, 0)?;

        Ok(Highlighted {
            spans: run.canvas.into_spans(code),
            missing: run.missing,
        })
    }

    fn paint<'a>(
        &'a self,
        run: &mut Run<'a, '_>,
        language: &str,
        parent: Option<&str>,
        ranges: Option<&[Range]>,
        depth: usize,
    ) -> cu::Result<()> {
        let Some(loaded) = self.languages.get(language) else {
            run.missing.insert(language.to_string());
            return Ok(());
        };

        let Some(tree) = parse(&loaded.language, run.code, ranges)? else {
            // A parser can decline a document (for instance when included
            // ranges leave nothing to read). Nothing to highlight, but the
            // surrounding layers are unaffected.
            return Ok(());
        };

        self.paint_highlights(run, loaded, &tree);

        if depth < MAX_INJECTION_DEPTH {
            for region in self.find_injections(run, loaded, &tree, language, parent) {
                self.paint(
                    run,
                    &region.language,
                    Some(language),
                    Some(&region.ranges),
                    depth + 1,
                )?;
            }
        }

        Ok(())
    }

    fn paint_highlights<'a>(&'a self, run: &mut Run<'a, '_>, loaded: &'a Loaded, tree: &Tree) {
        if loaded.highlights.is_empty() {
            return;
        }

        let source = run.code.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(loaded.highlights.raw(), tree.root_node(), source);

        while let Some((matched, capture_index)) = captures.next() {
            if !loaded.highlights.accepts(matched, source) {
                // Drop the whole match, so its other captures are skipped too.
                matched.remove();
                continue;
            }

            let capture = matched.captures()[*capture_index];
            let group = match loaded.highlights.role(capture.index) {
                CaptureRole::Ignored => continue,
                CaptureRole::Cleared => None,
                CaptureRole::Group(group) => Some(group.as_str()),
            };

            let pattern = loaded.highlights.pattern(matched.pattern_index);
            let range = injection::offset_range(
                capture.node.range(),
                pattern.offset(capture.index),
                &run.lines,
            );
            run.canvas.paint(
                range.start_byte..range.end_byte,
                group,
                pattern.priority(capture.index),
            );
        }
    }

    fn find_injections(
        &self,
        run: &mut Run<'_, '_>,
        loaded: &Loaded,
        tree: &Tree,
        language: &str,
        parent: Option<&str>,
    ) -> Vec<Region> {
        let Some(content_capture) = loaded.content_capture else {
            return Vec::new();
        };
        if loaded.injections.is_empty() {
            return Vec::new();
        }

        let source = run.code.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(loaded.injections.raw(), tree.root_node(), source);

        let mut regions = Vec::new();
        // `injection.combined` asks for every match of one pattern to be
        // parsed as a single document, so those accumulate per pattern.
        let mut combined: BTreeMap<(usize, String), Vec<Range>> = BTreeMap::new();

        while let Some(matched) = matches.next() {
            if !loaded.injections.accepts(matched, source) {
                continue;
            }
            let pattern = loaded.injections.pattern(matched.pattern_index);

            let Some(injected) = self.injected_language(run, loaded, matched, language, parent)
            else {
                continue;
            };

            let mut ranges = Vec::new();
            for capture in matched
                .captures()
                .iter()
                .filter(|c| c.index == content_capture)
            {
                let range = injection::offset_range(
                    capture.node.range(),
                    pattern.offset(capture.index),
                    &run.lines,
                );
                injection::content_ranges(
                    capture.node,
                    range,
                    pattern.injection.include_children,
                    &mut ranges,
                );
            }
            if ranges.is_empty() {
                continue;
            }

            if pattern.injection.combined {
                combined
                    .entry((matched.pattern_index, injected))
                    .or_default()
                    .append(&mut ranges);
            } else {
                regions.push(Region {
                    language: injected,
                    ranges: injection::normalize(ranges),
                });
            }
        }

        regions.extend(combined.into_iter().map(|((_, language), ranges)| Region {
            language,
            ranges: injection::normalize(ranges),
        }));
        regions.retain(|region| !region.ranges.is_empty());
        regions
    }

    /// Work out which language a match injects, as a canonical language id.
    fn injected_language(
        &self,
        run: &Run<'_, '_>,
        loaded: &Loaded,
        matched: &tree_sitter::QueryMatch,
        language: &str,
        parent: Option<&str>,
    ) -> Option<String> {
        let pattern = loaded.injections.pattern(matched.pattern_index);
        let injection = &pattern.injection;

        let named = if injection.own_language {
            Some(language.to_string())
        } else if injection.parent_language {
            Some(parent.unwrap_or(language).to_string())
        } else if let Some(fixed) = &injection.language {
            Some(fixed.clone())
        } else {
            // Otherwise the language is whatever text the `@injection.language`
            // capture covers -- a fenced block's info string, say.
            let capture = loaded.language_capture?;
            let node = matched.nodes_for_capture_index(capture).next()?;
            let text = run.code.get(node.byte_range())?;
            Some(pattern.substitute(capture, text))
        }?;

        // Queries name languages the way a user would (`sh`, `js`), so the
        // same alias table that resolves a markdown fence tag applies here.
        Some(
            registry::resolve_language(&named)
                .map(|language| language.name.to_string())
                .unwrap_or(named),
        )
    }
}

/// Mutable state for one `highlight` call.
struct Run<'a, 'code> {
    canvas: Canvas<'a>,
    lines: LineIndex,
    missing: BTreeSet<String>,
    code: &'code str,
}

/// One embedded region to highlight with another language.
struct Region {
    language: String,
    ranges: Vec<Range>,
}

fn parse(language: &Language, code: &str, ranges: Option<&[Range]>) -> cu::Result<Option<Tree>> {
    let mut parser = Parser::new();
    cu::check!(
        parser.set_language(language),
        "the parser is not compatible with this tree-sitter build"
    )?;
    if let Some(ranges) = ranges {
        // Ranges come from `injection::normalize`, so this only fails if a
        // grammar hands back something pathological.
        if parser.set_included_ranges(ranges).is_err() {
            return Ok(None);
        }
    }
    Ok(parser.parse(code, None))
}
