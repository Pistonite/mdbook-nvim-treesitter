//! The preprocessor itself: read a book, highlight its code, write it back.

use std::collections::BTreeSet;
use std::io::{Read, Write};

use cu::pre::*;
use mdbook_preprocessor::book::Book;

use crate::driver::settings::Settings;
use crate::highlight::{Highlighter, strip_overrides};
use crate::query::QuerySource;
use crate::renderer::Replacer;
use crate::renderer::render_html;
use crate::renderer::scan::{self, Filter, HighlightTask};
use crate::store::Workspace;

/// How many times we may discover a new injected language and try again.
///
/// Each round can only add languages, and the language table is finite, so
/// this is a safety net rather than a real limit; two rounds covers every
/// realistic book (a block, plus whatever it injects).
const MAX_ROUNDS: usize = 4;

/// Read mdBook's JSON from `input`, highlight the book, write it to `output`.
pub fn preprocess(input: impl Read, output: impl Write) -> cu::Result<()> {
    let (context, mut book) = mdbook_preprocessor::parse_input(input)?;

    let (settings, complaints) = Settings::load(&context)?;
    for complaint in complaints {
        eprintln!("mdbook-nvim-treesitter: {complaint}");
    }

    let (filter, unknown) = Filter::new(&settings.include, &settings.exclude);
    for tag in unknown {
        eprintln!(
            "mdbook-nvim-treesitter: book.toml names {tag:?}, \
             which is not a language nvim-treesitter knows"
        );
    }

    // Locate everything to highlight before touching the caches, so a book
    // with no code blocks costs nothing.
    let pages = collect(&mut book, &filter);

    if pages.iter().all(|page| page.tasks.is_empty()) {
        return write(book, output);
    }

    let rendered = highlight_pages(&pages, &settings, &context.root)?;
    apply(&mut book, &pages, &rendered)?;
    write(book, output)
}

/// One chapter's worth of work.
struct Page {
    /// The chapter's title, used only to check that the two walks over the
    /// book lined up.
    name: String,
    tasks: Vec<HighlightTask>,
}

/// Scan every chapter, in the order the second pass will visit them.
///
/// `Book` offers two traversals and they are not the same order --
/// `chapters()` yields a parent before its children, `for_each_chapter_mut`
/// after -- so both passes have to use the same one.
fn collect(book: &mut Book, filter: &Filter) -> Vec<Page> {
    let mut pages = Vec::new();
    book.for_each_chapter_mut(|chapter| {
        pages.push(Page {
            name: chapter.name.clone(),
            tasks: scan::tasks(&chapter.content, filter),
        });
    });
    pages
}

/// Highlight every task, installing languages as they turn out to be needed.
///
/// A block's own language is installed up front. Injected languages only
/// become visible once the host language has been parsed -- there is no way to
/// know a markdown block contains Python until markdown's injections query has
/// run over it -- so highlighting reports what it was missing and we go round
/// again. A language we fail to install is reported and its code is left
/// unhighlighted rather than failing the build.
fn highlight_pages(
    pages: &[Page],
    settings: &Settings,
    root: &std::path::Path,
) -> cu::Result<Vec<Vec<String>>> {
    let workspace = Workspace::open(settings.cache_dir(root))?;
    let mut highlighter = Highlighter::new(QuerySource::new(workspace.query_roots()));

    let wanted: BTreeSet<&str> = pages
        .iter()
        .flat_map(|page| &page.tasks)
        .map(|task| task.language.as_str())
        .collect();
    for language in &wanted {
        // A language a code block explicitly asked for must work; a failure
        // here is a real problem with the book or the environment.
        cu::check!(
            install(&workspace, &mut highlighter, language),
            "failed to set up the {language} parser"
        )?;
    }

    let mut attempted: BTreeSet<String> = wanted.iter().map(|l| l.to_string()).collect();
    let mut rendered = Vec::new();

    for _ in 0..MAX_ROUNDS {
        let mut missing = BTreeSet::new();
        rendered = Vec::with_capacity(pages.len());

        for page in pages {
            let mut page_html = Vec::with_capacity(page.tasks.len());
            for task in &page.tasks {
                // Marker comments come out before parsing, so they cannot
                // confuse the grammar, and go back in as highlight groups
                // after.
                let overrides = strip_overrides(&task.code);
                let code = overrides.code();

                let highlighted = highlighter.highlight(&task.language, code)?;
                missing.extend(highlighted.missing);
                let spans = overrides.apply(highlighted.spans);

                page_html.push(if task.kind.is_block() {
                    render_html::block(&task.language, code, &spans)
                } else {
                    render_html::inline(&task.language, code, &spans)
                });
            }
            rendered.push(page_html);
        }

        let fresh: Vec<String> = missing.difference(&attempted).cloned().collect();
        if fresh.is_empty() {
            break;
        }

        for language in &fresh {
            attempted.insert(language.clone());
            // Best effort: an embedded language we cannot build leaves that
            // region plain, which is far better than failing the whole book.
            if let Err(error) = install(&workspace, &mut highlighter, language) {
                eprintln!(
                    "mdbook-nvim-treesitter: leaving embedded {language} unhighlighted: {error:#}"
                );
            }
        }
    }

    for warning in highlighter.warnings() {
        eprintln!("mdbook-nvim-treesitter: {warning}");
    }

    Ok(rendered)
}

fn install(workspace: &Workspace, highlighter: &mut Highlighter, language: &str) -> cu::Result<()> {
    let Some(installed) = workspace.install(language)? else {
        return Ok(());
    };
    highlighter.load(language, &installed.parser, &installed.symbol)
}

/// Splice the rendered HTML back into the book.
///
/// This walks the book exactly as [`collect`] did, so the nth page of results
/// belongs to the nth chapter visited. The chapter name is checked as a guard:
/// if the two walks ever drift apart, ranges from one chapter would be applied
/// to another, and that should fail loudly rather than corrupt the book.
fn apply(book: &mut Book, pages: &[Page], rendered: &[Vec<String>]) -> cu::Result<()> {
    let mut result = Ok(());
    let mut index = 0;

    book.for_each_chapter_mut(|chapter| {
        if result.is_err() {
            return;
        }
        let (Some(page), Some(html)) = (pages.get(index), rendered.get(index)) else {
            result = Err(cu::fmterr!(
                "the book changed shape while it was being processed"
            ));
            return;
        };
        index += 1;

        if page.name != chapter.name {
            result = Err(cu::fmterr!(
                "expected chapter {:?} but found {:?}",
                page.name,
                chapter.name
            ));
            return;
        }
        if page.tasks.is_empty() {
            return;
        }

        let mut replacer = Replacer::default();
        for (task, html) in page.tasks.iter().zip(html) {
            replacer.add(task.range.clone(), html.clone());
        }

        match cu::check!(
            replacer.apply(&chapter.content),
            "in chapter {:?}",
            chapter.name
        ) {
            Ok(content) => chapter.content = content,
            Err(error) => result = Err(error),
        }
    });

    result
}

fn write(book: Book, output: impl Write) -> cu::Result<()> {
    cu::check!(
        cu::json::write(output, &book),
        "failed to write the processed book"
    )
}
