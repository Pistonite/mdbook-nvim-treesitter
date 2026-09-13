//! Preparing query source for tree-sitter's query compiler.
//!
//! tree-sitter recognises `#match?` and compiles its argument as a Rust regex
//! while building the query. Neovim's queries mean something else by it -- a
//! Vim regex -- so a pattern like `\c^return$` is a *compile error* that takes
//! the whole file's highlighting with it, and a pattern that happens to parse
//! quietly means the wrong thing.
//!
//! Renaming the predicate before compilation moves it out of tree-sitter's
//! known set and into the general predicates we evaluate ourselves, where
//! [`super::vim_regex`] gives it Neovim's semantics.
//!
//! `#set!` is moved for a different reason. tree-sitter parses it into a
//! key/value property and rejects anything that does not fit that shape, so a
//! Neovim directive that names two captures -- `(#set! @url url @url)` -- is a
//! compile error for the whole file. Reading it ourselves from the raw
//! arguments accepts every shape Neovim does, and ignores the ones that only
//! mean something inside an editor.

/// The predicate renames applied before a query is compiled.
const RENAMES: [(&str, &str); 5] = [
    ("match?", "vim-match?"),
    ("not-match?", "not-vim-match?"),
    ("any-match?", "any-vim-match?"),
    ("any-not-match?", "any-not-vim-match?"),
    ("set!", NVIM_SET),
];

/// The name `#set!` is compiled under, so that we parse it rather than
/// tree-sitter. Shared with [`super::metadata`], which reads it back.
pub const NVIM_SET: &str = "nvim-set!";

/// Rewrite the predicates we want to evaluate ourselves out of tree-sitter's
/// known set.
///
/// Comments and string literals are skipped, so a pattern that merely contains
/// the text `#match?` is left alone.
pub fn for_compilation(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut out = String::with_capacity(query.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b';' => {
                let end = line_end(bytes, index);
                out.push_str(&query[index..end]);
                index = end;
            }
            b'"' => {
                let end = string_end(bytes, index);
                out.push_str(&query[index..end]);
                index = end;
            }
            b'#' => {
                let end = name_end(bytes, index + 1);
                let name = &query[index + 1..end];
                match RENAMES.iter().find(|(from, _)| *from == name) {
                    Some((_, to)) => {
                        out.push('#');
                        out.push_str(to);
                    }
                    None => out.push_str(&query[index..end]),
                }
                index = end;
            }
            _ => {
                let character = query[index..].chars().next().expect("index is a boundary");
                out.push(character);
                index += character.len_utf8();
            }
        }
    }

    out
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    (from..bytes.len())
        .find(|&i| bytes[i] == b'\n')
        .map_or(bytes.len(), |i| i + 1)
}

/// The index just past the closing quote of the string starting at `open`.
fn string_end(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn name_end(bytes: &[u8], from: usize) -> usize {
    (from..bytes.len())
        .find(|&i| !matches!(bytes[i], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'?' | b'!'))
        .unwrap_or(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_the_match_family() {
        assert_eq!(
            for_compilation(r#"((a) @b (#match? @b "\c^x$"))"#),
            r#"((a) @b (#vim-match? @b "\c^x$"))"#
        );
        assert_eq!(
            for_compilation("(#not-match? @b \"x\")"),
            "(#not-vim-match? @b \"x\")"
        );
        assert_eq!(
            for_compilation("(#any-not-match? @b \"x\")"),
            "(#any-not-vim-match? @b \"x\")"
        );
    }

    #[test]
    fn renames_set_directives() {
        assert_eq!(
            for_compilation("((a) @b (#set! priority 105))"),
            "((a) @b (#nvim-set! priority 105))"
        );
        // The shape tree-sitter rejects but Neovim accepts.
        assert_eq!(
            for_compilation("(#set! @url url @url)"),
            "(#nvim-set! @url url @url)"
        );
    }

    #[test]
    fn leaves_other_predicates_alone() {
        for query in [
            r#"((a) @b (#eq? @b "x"))"#,
            r#"((a) @b (#lua-match? @b "^x"))"#,
            "((a) @b (#offset! @b 0 1 0 -1))",
            "(#any-of? @b \"x\" \"y\")",
        ] {
            assert_eq!(for_compilation(query), query);
        }
    }

    #[test]
    fn does_not_rewrite_inside_strings_or_comments() {
        let query = "; see #match? for details\n((a) @b (#eq? @b \"#match?\"))";
        assert_eq!(for_compilation(query), query);
    }

    #[test]
    fn preserves_everything_else_byte_for_byte() {
        let query = "; héllo ünicode\n((identifier) @x)\n";
        assert_eq!(for_compilation(query), query);
    }

    #[test]
    fn survives_unterminated_input() {
        assert_eq!(
            for_compilation("(#eq? @a \"unterminated"),
            "(#eq? @a \"unterminated"
        );
        assert_eq!(for_compilation("#"), "#");
    }
}
