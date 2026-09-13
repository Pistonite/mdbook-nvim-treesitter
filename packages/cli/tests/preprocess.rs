//! Driving the binary the way mdBook does: a JSON book on stdin, a JSON book
//! on stdout.
//!
//! The tests that actually highlight something need parsers, so they build
//! grammars the first time they run and are ignored by default. Run them with
//! `cargo test -- --ignored`. Set `MNT_TEST_CACHE` to put the scratch book
//! cache somewhere other than the temp directory.

use std::io::Write as _;
use std::process::{Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_mdbook-nvim-treesitter");

/// A preprocessor input: `[context, book]`, with one chapter per string.
fn input(chapters: &[&str]) -> String {
    let items: Vec<String> = chapters
        .iter()
        .enumerate()
        .map(|(index, content)| {
            format!(
                r#"{{"Chapter":{{"name":"Chapter {index}","content":{},"number":[{}],
                   "sub_items":[],"path":"ch{index}.md","source_path":"ch{index}.md",
                   "parent_names":[]}}}}"#,
                json_string(content),
                index + 1
            )
        })
        .collect();

    format!(
        r#"[{{"root":"/book","config":{{"book":{{"title":"t"}},
             "preprocessor":{{"nvim-treesitter":{{"cache_dir":{}}}}}}},
           "renderer":"html","mdbook_version":"0.5.0"}},
          {{"items":[{}]}}]"#,
        json_string(&cache_dir()),
        items.join(",")
    )
}

/// Where the test book keeps its parsers. Shared between tests so grammars are
/// built once.
fn cache_dir() -> String {
    let root = std::env::var_os("MNT_TEST_CACHE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join("books")
        .join("preprocess")
        .to_string_lossy()
        .into_owned()
}

fn json_string(value: &str) -> String {
    let escaped: String = value
        .chars()
        .flat_map(|c| {
            match c {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                '\n' => "\\n".to_string(),
                '\t' => "\\t".to_string(),
                c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32),
                c => c.to_string(),
            }
            .chars()
            .collect::<Vec<_>>()
        })
        .collect();
    format!("\"{escaped}\"")
}

/// Run the preprocessor and return its stdout, or its stderr on failure.
fn preprocess(chapters: &[&str]) -> Result<String, String> {
    let mut child = Command::new(BINARY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the preprocessor binary should be runnable");

    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input(chapters).as_bytes())
        .expect("the preprocessor should accept its input");

    let output = child
        .wait_with_output()
        .expect("the preprocessor should exit");
    if output.status.success() {
        Ok(String::from_utf8(output.stdout).expect("output is UTF-8"))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

#[test]
fn a_book_without_code_passes_straight_through() {
    // No code means no cache and no network, so this runs anywhere.
    let out = preprocess(&["# Title\n\nJust prose.\n"]).unwrap();
    assert!(out.contains("Just prose."), "{out}");
    assert!(!out.contains("tree-sitter-block"), "{out}");
}

#[test]
fn an_unsupported_renderer_is_reported_by_exit_status() {
    let status = Command::new(BINARY)
        .args(["supports", "epub"])
        .status()
        .unwrap();
    assert!(!status.success());

    let status = Command::new(BINARY)
        .args(["supports", "html"])
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn the_stylesheet_is_printed_to_stdout() {
    let output = Command::new(BINARY).arg("css").output().unwrap();
    assert!(output.status.success());
    let css = String::from_utf8(output.stdout).unwrap();
    assert!(css.contains(".ts-keyword"), "{css}");
}

/// Run the preprocessor over a raw input document, returning exit status,
/// stdout and stderr.
fn run_raw(input: &str) -> (bool, String, String) {
    let mut child = Command::new(BINARY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn config_only(table: &str) -> String {
    format!(
        r#"[{{"root":"/book","config":{{"preprocessor":{{"nvim-treesitter":{table}}}}},
           "renderer":"html","mdbook_version":"0.5.0"}},{{"items":[]}}]"#
    )
}

#[test]
fn mdbooks_own_preprocessor_keys_are_accepted() {
    // A book.toml that pins the executable and the ordering is entirely
    // normal, and mdBook puts those keys in the same table as ours.
    let (ok, _, stderr) = run_raw(&config_only(
        r#"{"command":"mdbook-nvim-treesitter","before":["links"],"after":["index"],
            "optional":false,"renderers":["html"]}"#,
    ));

    assert!(ok, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn an_unknown_setting_warns_but_still_builds() {
    let (ok, _, stderr) = run_raw(&config_only(r#"{"excludes":["python"]}"#));

    assert!(ok, "{stderr}");
    assert!(stderr.contains("excludes"), "{stderr}");
}

#[test]
fn a_setting_of_the_wrong_type_fails_with_an_explanation() {
    let (ok, _, stderr) = run_raw(&config_only(r#"{"exclude":"python"}"#));

    assert!(!ok, "{stderr}");
    assert!(stderr.contains("book.toml"), "{stderr}");
}

#[test]
#[ignore = "builds parsers from source"]
fn highlights_a_fenced_block() {
    let out = preprocess(&["```rust\nfn main() {}\n```\n"]).unwrap();

    assert!(out.contains("tree-sitter-block"), "{out}");
    assert!(out.contains("tree-sitter-language-rust"), "{out}");
    assert!(out.contains("ts-keyword ts-keyword-function"), "{out}");
    // highlight.js must be kept away from spans we already produced.
    assert!(out.contains("no-highlight language-none"), "{out}");
}

#[test]
#[ignore = "builds parsers from source"]
fn leaves_a_nested_fence_inside_its_container() {
    let out = preprocess(&["- item\n\n  ```rust\n  fn f() {}\n  ```\n"]).unwrap();

    // The list indentation is still in front of the replacement, and the
    // replacement itself is a single line.
    assert!(
        out.contains(r#"  <pre class=\"tree-sitter-block\">"#),
        "{out}"
    );
    let block_start = out.find("<pre class=").unwrap();
    let block_end = out.find("</pre>").unwrap();
    assert!(!out[block_start..block_end].contains(r"\n"), "{out}");
}

#[test]
#[ignore = "builds parsers from source"]
fn highlights_the_inline_syntax() {
    let out = preprocess(&["see ```rust`let x = 1;``` here\n"]).unwrap();

    assert!(out.contains("tree-sitter-inline"), "{out}");
    assert!(!out.contains("tree-sitter-block"), "{out}");
}

#[test]
#[ignore = "builds parsers from source"]
fn excluded_languages_are_left_to_mdbook() {
    let chapters = ["```rust\nfn f() {}\n```\n"];
    let with_exclude =
        input(&chapters).replace(r#""cache_dir""#, r#""exclude":["rust"],"cache_dir""#);

    let mut child = Command::new(BINARY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(with_exclude.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(!out.contains("tree-sitter-block"), "{out}");
}

#[test]
#[ignore = "builds parsers from source"]
fn a_marker_comment_overrides_a_highlight() {
    let out = preprocess(&[
        "```c\nint f(int x) { /* @tree-sitter:function.macro */ASSERT(x); return 0; }\n```\n",
    ])
    .unwrap();

    assert!(out.contains("ts-function ts-function-macro"), "{out}");
    // The marker itself is gone from the rendered code.
    assert!(!out.contains("@tree-sitter:"), "{out}");
}
