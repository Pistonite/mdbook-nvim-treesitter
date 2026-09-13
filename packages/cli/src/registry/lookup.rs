//! Resolving a user-written tag to a language in the table.

use crate::registry::{FILETYPE_ALIASES, LANGUAGES, LanguageInfo, util};

/// Aliases that are conventional in markdown fences but are not Neovim
/// filetypes, so nvim-treesitter's own alias table does not carry them.
///
/// Kept sorted by alias; only add a tag here when it is unambiguous.
const FENCE_ALIASES: &[(&str, &str)] = &[
    ("c#", "c_sharp"),
    ("c++", "cpp"),
    ("cc", "cpp"),
    ("golang", "go"),
    ("hpp", "cpp"),
    ("htm", "html"),
    ("kt", "kotlin"),
    ("md", "markdown"),
    ("rs", "rust"),
    ("sh", "bash"),
    ("shell", "bash"),
    ("yml", "yaml"),
    ("zsh", "bash"),
];

/// Look a canonical language id up in the table.
pub fn get_language(name: &str) -> Option<&'static LanguageInfo> {
    let index = LANGUAGES.binary_search_by(|l| l.name.cmp(name)).ok()?;
    Some(&LANGUAGES[index])
}

/// Resolve a markdown fence tag, filetype, or language id to a language that
/// can actually be highlighted.
///
/// Tags are matched case-insensitively, and `-` is accepted where the language
/// id uses `_` (fences say `c-sharp`, nvim-treesitter says `c_sharp`), except
/// where the tag is an alias in its own right.
///
/// **A query-only language is never the answer.** `ecma`, `jsx` and
/// `html_tags` are language ids in the table, but they have no grammar -- they
/// exist so that other languages can inherit their queries. Returning one
/// would name a language that can never highlight anything, and because
/// "has no parser" is a legitimate state further down, nothing would report
/// it: the block would come out unhighlighted with no error anywhere.
///
/// This matters for real fence tags. ```` ```jsx ```` and ```` ```ecma ````
/// are both the name of a query-only language *and* a Neovim filetype alias
/// for `javascript`, so skipping the parser-less entry is what makes them fall
/// through to the language the author meant. Use [`get`] when you want the
/// table entry for an exact id, query-only ones included.
pub fn resolve_language(tag: &str) -> Option<&'static LanguageInfo> {
    let tag = util::normalize_tag(tag);
    if tag.is_empty() {
        return None;
    }

    // Exact id first: a language id always wins over an alias, so that a tag
    // like `json` cannot be redirected by an alias someone adds later.
    if let Some(language) = get_language(&tag).filter(|l| l.has_parser()) {
        return Some(language);
    }
    if let Some(language) = alias_of(FILETYPE_ALIASES, &tag)
        .and_then(get_language)
        .filter(|l| l.has_parser())
    {
        return Some(language);
    }
    if let Some(language) = alias_of(FENCE_ALIASES, &tag)
        .and_then(get_language)
        .filter(|l| l.has_parser())
    {
        return Some(language);
    }
    get_language(&tag.replace('-', "_")).filter(|l| l.has_parser())
}

fn alias_of(table: &'static [(&'static str, &'static str)], tag: &str) -> Option<&'static str> {
    let index = table.binary_search_by(|(alias, _)| alias.cmp(&tag)).ok()?;
    Some(table[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_so_binary_search_is_valid() {
        assert!(LANGUAGES.windows(2).all(|w| w[0].name < w[1].name));
        assert!(FILETYPE_ALIASES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(FENCE_ALIASES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn every_alias_points_at_a_real_language() {
        for (alias, language) in FILETYPE_ALIASES.iter().chain(FENCE_ALIASES) {
            assert!(
                get_language(language).is_some(),
                "{alias} -> missing {language}"
            );
        }
    }

    #[test]
    fn every_required_language_is_in_the_table() {
        for language in LANGUAGES {
            for required in language.requires {
                assert!(
                    get_language(required).is_some(),
                    "{} requires missing {required}",
                    language.name
                );
            }
        }
    }

    #[test]
    fn resolves_ids_aliases_and_fence_spellings() {
        assert_eq!(resolve_language("rust").unwrap().name, "rust");
        assert_eq!(resolve_language("  Rust ").unwrap().name, "rust");
        assert_eq!(resolve_language("sh").unwrap().name, "bash");
        assert_eq!(resolve_language("js").unwrap().name, "javascript");
        assert_eq!(resolve_language("c++").unwrap().name, "cpp");
        assert_eq!(resolve_language("c-sharp").unwrap().name, "c_sharp");
        assert_eq!(resolve_language("c_sharp").unwrap().name, "c_sharp");
    }

    #[test]
    fn rejects_tags_that_are_not_languages() {
        assert!(resolve_language("").is_none());
        assert!(resolve_language("   ").is_none());
        assert!(resolve_language("definitely-not-a-language").is_none());
    }

    #[test]
    fn query_only_languages_have_no_parser() {
        assert!(!get_language("ecma").unwrap().has_parser());
        assert!(!get_language("html_tags").unwrap().has_parser());
        assert!(get_language("rust").unwrap().has_parser());
    }

    #[test]
    fn a_query_only_language_never_wins_a_tag() {
        // `jsx` and `ecma` are query-only language ids *and* filetype aliases
        // for javascript. Matching the id first would hand back a language
        // with no grammar, and the block would silently come out plain.
        assert_eq!(resolve_language("jsx").unwrap().name, "javascript");
        assert_eq!(resolve_language("ecma").unwrap().name, "javascript");

        // `html_tags` has no alias to fall through to, so it resolves to
        // nothing rather than to something unhighlightable.
        assert!(resolve_language("html_tags").is_none());

        // Whatever `resolve` returns can always be highlighted.
        for language in LANGUAGES {
            if let Some(resolved) = resolve_language(language.name) {
                assert!(
                    resolved.has_parser(),
                    "{} -> {}",
                    language.name,
                    resolved.name
                );
            }
        }
        for (alias, _) in FILETYPE_ALIASES.iter().chain(FENCE_ALIASES) {
            if let Some(resolved) = resolve_language(alias) {
                assert!(resolved.has_parser(), "{alias} -> {}", resolved.name);
            }
        }
    }

    #[test]
    fn get_still_finds_query_only_languages() {
        // The store needs them: installing `cpp` has to install `c`'s queries,
        // and installing `javascript` has to install `ecma`'s.
        assert_eq!(get_language("ecma").unwrap().name, "ecma");
        assert_eq!(get_language("jsx").unwrap().name, "jsx");
        assert_eq!(get_language("html_tags").unwrap().name, "html_tags");
    }
}
