//! Assembling a language's query text the way Neovim's runtime does.
//!
//! nvim-treesitter queries are not self-contained. A file may open with a
//! modeline such as
//!
//! ```scm
//! ; inherits: c
//! ```
//!
//! which means "prepend the whole of C's query of the same kind". Without it,
//! `cpp/highlights.scm` highlights almost nothing, because everything C-shaped
//! lives in C's file. Inherited text goes *before* the file's own, so the
//! file's patterns are matched later and therefore win where they overlap.
//!
//! A language in parentheses -- `; inherits: (foo)` -- is only pulled in when
//! the file is being read directly, not when it is itself being inherited.

use std::collections::HashSet;
use std::path::PathBuf;

/// The query files a language can define.
pub const HIGHLIGHTS: &str = "highlights";
/// Injected-language definitions.
pub const INJECTIONS: &str = "injections";

/// Reads query files from a search path of nvim-treesitter-shaped directories.
///
/// Roots are searched in order, so a book's own cache can shadow the shared
/// one -- the same precedence Neovim gives the runtimepath.
pub struct QuerySource {
    roots: Vec<PathBuf>,
}

impl QuerySource {
    /// Search `roots` in order. Each root holds `<language>/<kind>.scm`.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }

    /// The assembled text of `language`'s `kind` query, empty if it has none.
    pub fn load(&self, language: &str, kind: &str) -> cu::Result<String> {
        let mut text = String::new();
        let mut visiting = HashSet::new();
        self.append(language, kind, false, &mut text, &mut visiting)?;
        Ok(text)
    }

    /// Whether any root defines `language`'s `kind` query.
    pub fn has(&self, language: &str, kind: &str) -> bool {
        self.locate(language, kind).is_some()
    }

    fn append(
        &self,
        language: &str,
        kind: &str,
        inherited: bool,
        out: &mut String,
        visiting: &mut HashSet<String>,
    ) -> cu::Result<()> {
        // nvim-treesitter's own queries are acyclic, but a cycle here would be
        // an unbounded recursion, so refuse to revisit.
        if !visiting.insert(language.to_string()) {
            return Ok(());
        }

        let text = match self.locate(language, kind) {
            Some(path) => cu::fs::read_string(&path)?,
            // A missing inherited query is normal: not every language defines
            // every kind.
            None => {
                visiting.remove(language);
                return Ok(());
            }
        };

        for base in parse_inherits(&text) {
            if base.optional && inherited {
                continue;
            }
            self.append(&base.language, kind, true, out, visiting)?;
        }
        visiting.remove(language);

        out.push_str(&text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        Ok(())
    }

    fn locate(&self, language: &str, kind: &str) -> Option<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.join(language).join(format!("{kind}.scm")))
            .find(|path| path.is_file())
    }
}

/// One entry of an `; inherits:` modeline.
#[derive(Debug, PartialEq, Eq)]
struct Inherit {
    language: String,
    /// Written as `(lang)`: skipped when this file is itself being inherited.
    optional: bool,
}

/// The languages named by the `inherits` modelines at the top of `text`.
///
/// Only the leading run of comment lines is inspected, matching Neovim, so an
/// `inherits` written further down is an ordinary comment.
fn parse_inherits(text: &str) -> Vec<Inherit> {
    let mut inherits = Vec::new();

    for line in text.lines() {
        if !line.starts_with(';') {
            break;
        }
        let Some(list) = line
            .trim_start_matches(';')
            .trim()
            .strip_prefix("inherits")
            .map(|rest| rest.trim_start().trim_start_matches(':').trim())
        else {
            continue;
        };

        for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let optional = entry.starts_with('(') && entry.ends_with(')');
            let language = if optional {
                &entry[1..entry.len() - 1]
            } else {
                entry
            };
            if !language.is_empty() {
                inherits.push(Inherit {
                    language: language.to_string(),
                    optional,
                });
            }
        }
    }

    inherits
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn write(root: &Path, language: &str, kind: &str, text: &str) {
        let dir = root.join(language);
        cu::fs::make_dir(&dir).unwrap();
        cu::fs::write(dir.join(format!("{kind}.scm")), text).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nvimts-query-source-{name}"));
        let _ = cu::fs::rec_remove(&path);
        cu::fs::make_dir(&path).unwrap();
        path
    }

    #[test]
    fn reads_inherits_modelines() {
        assert_eq!(
            parse_inherits("; inherits: typescript,jsx\n(x) @y"),
            [
                Inherit {
                    language: "typescript".into(),
                    optional: false
                },
                Inherit {
                    language: "jsx".into(),
                    optional: false
                },
            ]
        );
        assert_eq!(
            parse_inherits(";; inherits c\n"),
            [Inherit {
                language: "c".into(),
                optional: false
            }]
        );
        assert_eq!(
            parse_inherits("; inherits: (html)\n"),
            [Inherit {
                language: "html".into(),
                optional: true
            }]
        );
    }

    #[test]
    fn stops_looking_after_the_leading_comments() {
        assert_eq!(parse_inherits("(x) @y\n; inherits: c\n"), []);
        assert_eq!(parse_inherits("(x) @y\n"), []);
    }

    #[test]
    fn inherited_text_comes_first() {
        let root = temp_dir("order");
        write(&root, "c", "highlights", "(c_pattern) @a");
        write(
            &root,
            "cpp",
            "highlights",
            "; inherits: c\n(cpp_pattern) @b",
        );

        let source = QuerySource::new([root]);
        assert_eq!(
            source.load("cpp", HIGHLIGHTS).unwrap(),
            "(c_pattern) @a\n; inherits: c\n(cpp_pattern) @b\n"
        );
    }

    #[test]
    fn inheritance_is_transitive() {
        let root = temp_dir("transitive");
        write(&root, "a", "highlights", "(a) @a");
        write(&root, "b", "highlights", "; inherits: a\n(b) @b");
        write(&root, "c", "highlights", "; inherits: b\n(c) @c");

        let text = QuerySource::new([root]).load("c", HIGHLIGHTS).unwrap();
        let order: Vec<_> = ["(a) @a", "(b) @b", "(c) @c"]
            .iter()
            .map(|needle| text.find(needle).expect(needle))
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{text:?}");
    }

    #[test]
    fn optional_inherits_only_apply_at_the_top_level() {
        let root = temp_dir("optional");
        write(&root, "extra", "highlights", "(extra) @e");
        write(
            &root,
            "base",
            "highlights",
            "; inherits: (extra)\n(base) @b",
        );
        write(&root, "leaf", "highlights", "; inherits: base\n(leaf) @l");

        let source = QuerySource::new([root]);
        assert!(
            source
                .load("base", HIGHLIGHTS)
                .unwrap()
                .contains("(extra) @e")
        );
        // `base` is inherited here, so its optional inherit is skipped.
        assert!(
            !source
                .load("leaf", HIGHLIGHTS)
                .unwrap()
                .contains("(extra) @e")
        );
    }

    #[test]
    fn earlier_roots_shadow_later_ones() {
        let first = temp_dir("shadow-first");
        let second = temp_dir("shadow-second");
        write(&first, "rust", "highlights", "(first) @a");
        write(&second, "rust", "highlights", "(second) @a");

        let source = QuerySource::new([first, second]);
        assert_eq!(source.load("rust", HIGHLIGHTS).unwrap(), "(first) @a\n");
    }

    #[test]
    fn falls_back_to_a_later_root_for_an_inherited_language() {
        let local = temp_dir("fallback-local");
        let shared = temp_dir("fallback-shared");
        write(&local, "cpp", "highlights", "; inherits: c\n(cpp) @b");
        write(&shared, "c", "highlights", "(c) @a");

        let text = QuerySource::new([local, shared])
            .load("cpp", HIGHLIGHTS)
            .unwrap();
        assert!(text.contains("(c) @a"), "{text:?}");
    }

    #[test]
    fn a_missing_query_is_empty_rather_than_an_error() {
        let root = temp_dir("missing");
        let source = QuerySource::new([root]);
        assert_eq!(source.load("nothing", HIGHLIGHTS).unwrap(), "");
        assert!(!source.has("nothing", HIGHLIGHTS));
    }

    #[test]
    fn a_cycle_terminates() {
        let root = temp_dir("cycle");
        write(&root, "a", "highlights", "; inherits: b\n(a) @a");
        write(&root, "b", "highlights", "; inherits: a\n(b) @b");

        let text = QuerySource::new([root]).load("a", HIGHLIGHTS).unwrap();
        assert!(text.contains("(a) @a") && text.contains("(b) @b"));
    }
}
