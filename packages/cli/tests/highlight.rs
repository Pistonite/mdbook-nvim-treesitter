//! Highlighting with the real pinned parsers and queries.
//!
//! This is the test that says the nvim-treesitter queries actually work as
//! shipped: inheritance resolved, Lua-pattern predicates evaluated, injections
//! descended into. It builds parsers from source the first time it runs, so it
//! is ignored by default: `cargo test -- --ignored`.

use std::path::PathBuf;
use std::sync::OnceLock;

use mdbook_nvim_treesitter::store::{SystemCache, Workspace};
use mdbook_nvim_treesitter::{Highlighter, QuerySource};

/// The shared scratch workspace, so grammars are built once for the whole file.
fn workspace() -> &'static Workspace {
    static WORKSPACE: OnceLock<Workspace> = OnceLock::new();
    WORKSPACE.get_or_init(|| {
        let root: PathBuf = std::env::var_os("MNT_TEST_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("mnt-integration-cache"));
        Workspace::open_at(
            SystemCache::at(root.join("shared")),
            root.join("books").join("highlight"),
        )
        .unwrap()
    })
}

/// A highlighter with `languages` loaded from the shared scratch workspace.
fn highlighter(languages: &[&str]) -> Highlighter {
    let workspace = workspace();
    let mut highlighter = Highlighter::new(QuerySource::new(workspace.query_roots()));
    for name in languages {
        let installed = workspace.install(name).unwrap().unwrap();
        highlighter
            .load(name, &installed.parser, &installed.symbol)
            .unwrap();
    }
    highlighter
}

/// The highlight groups covering `needle`, for readable assertions.
fn group_of(
    highlighter: &Highlighter,
    language: &str,
    code: &str,
    needle: &str,
) -> Vec<Option<String>> {
    let at = code.find(needle).expect("needle is in the code");
    let wanted = at..at + needle.len();
    highlighter
        .highlight(language, code)
        .unwrap()
        .spans
        .into_iter()
        .filter(|span| span.range.start < wanted.end && wanted.start < span.range.end)
        .map(|span| span.group.map(str::to_string))
        .collect()
}

#[test]
#[ignore = "builds parsers from source"]
fn highlights_rust() {
    let highlighter = highlighter(&["rust"]);
    let code = "// a note\nfn main() {\n    let total = 1 + 2;\n}\n";

    assert_eq!(
        group_of(&highlighter, "rust", code, "// a note"),
        [Some("comment".into())]
    );
    assert_eq!(
        group_of(&highlighter, "rust", code, "fn"),
        [Some("keyword.function".into())]
    );
    assert_eq!(
        group_of(&highlighter, "rust", code, "main"),
        [Some("function".into())]
    );
    assert_eq!(
        group_of(&highlighter, "rust", code, "let"),
        [Some("keyword".into())]
    );
    assert_eq!(
        group_of(&highlighter, "rust", code, "1"),
        [Some("number".into())]
    );
}

#[test]
#[ignore = "builds parsers from source"]
fn spans_cover_the_input_exactly() {
    let highlighter = highlighter(&["rust"]);
    let code = "fn main() {}\n";
    let spans = highlighter.highlight("rust", code).unwrap().spans;

    assert_eq!(spans.first().unwrap().range.start, 0);
    assert_eq!(spans.last().unwrap().range.end, code.len());
    assert!(spans.windows(2).all(|w| w[0].range.end == w[1].range.start));
}

#[test]
#[ignore = "builds parsers from source"]
fn inherited_queries_reach_cpp() {
    // `cpp/highlights.scm` opens with `; inherits: c`, so without inheritance
    // a plain C declaration inside C++ gets almost nothing.
    let highlighter = highlighter(&["cpp"]);
    let code = "int main(void) { return 0; }";

    assert_eq!(
        group_of(&highlighter, "cpp", code, "return"),
        [Some("keyword.return".into())]
    );
    assert_eq!(
        group_of(&highlighter, "cpp", code, "int"),
        [Some("type.builtin".into())]
    );
}

#[test]
#[ignore = "builds parsers from source"]
fn lua_match_predicates_are_evaluated() {
    // C's queries mark SCREAMING_CASE identifiers as constants via
    // `#lua-match?`. If the predicate were ignored, `value` would be one too.
    let highlighter = highlighter(&["c"]);
    let code = "int f(void) { return LIMIT + value; }";

    assert_eq!(
        group_of(&highlighter, "c", code, "LIMIT"),
        [Some("constant".into())]
    );
    assert_eq!(
        group_of(&highlighter, "c", code, "value"),
        [Some("variable".into())]
    );
}

#[test]
#[ignore = "builds parsers from source"]
fn injections_highlight_javascript_inside_html() {
    let highlighter = highlighter(&["html", "javascript"]);
    let code = "<script>const x = 1;</script>";

    assert_eq!(
        group_of(&highlighter, "html", code, "const"),
        [Some("keyword".into())]
    );
    assert_eq!(
        group_of(&highlighter, "html", code, "script"),
        [Some("tag".into())]
    );
}

#[test]
#[ignore = "builds parsers from source"]
fn a_missing_injected_language_is_reported_not_fatal() {
    let highlighter = highlighter(&["html"]);
    let highlighted = highlighter
        .highlight("html", "<script>const x = 1;</script>")
        .unwrap();

    assert!(
        highlighted.missing.contains("javascript"),
        "{:?}",
        highlighted.missing
    );
    // The HTML around it is still highlighted.
    assert!(highlighted.spans.iter().any(|s| s.group.is_some()));
}

#[test]
#[ignore = "builds parsers from source"]
fn an_unloaded_language_yields_plain_text() {
    let highlighter = highlighter(&[]);
    let highlighted = highlighter.highlight("rust", "fn main() {}").unwrap();

    assert!(highlighted.spans.iter().all(|s| s.group.is_none()));
    assert!(highlighted.missing.contains("rust"));
}

#[test]
#[ignore = "builds parsers from source"]
fn every_shipped_query_compiles() {
    // A predicate or node type that no longer exists would break a whole
    // language silently, so check the languages the example book uses.
    let names = [
        "rust",
        "c",
        "cpp",
        "python",
        "javascript",
        "html",
        "css",
        "markdown",
        "bash",
        "toml",
    ];
    let highlighter = highlighter(&names);
    for name in names {
        assert!(highlighter.is_loaded(name), "{name} failed to load");
    }
    assert_eq!(highlighter.warnings(), &[] as &[String]);
}
