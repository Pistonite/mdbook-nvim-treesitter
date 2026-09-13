//! Deciding which languages a book highlights.

use std::collections::BTreeSet;

use crate::registry;

/// The `include` and `exclude` settings from `book.toml`, resolved to
/// canonical language ids.
///
/// This governs *code blocks* only. A language pulled in by an injection --
/// the JavaScript inside an HTML block, say -- is always highlighted, because
/// excluding it would leave the surrounding block half-rendered.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    /// When set, only these languages are highlighted.
    include: Option<BTreeSet<String>>,
    exclude: BTreeSet<String>,
}

impl Filter {
    /// Build a filter from configured tags.
    ///
    /// Tags are resolved the same way a fence tag is, so `exclude = ["sh"]`
    /// excludes bash. Tags that name no known language are returned as
    /// warnings rather than ignored, since a typo would otherwise silently do
    /// nothing.
    pub fn new(include: &[String], exclude: &[String]) -> (Self, Vec<String>) {
        let mut unknown = Vec::new();
        let mut resolve = |tags: &[String]| -> BTreeSet<String> {
            tags.iter()
                .filter_map(|tag| match registry::resolve_language(tag) {
                    Some(language) => Some(language.name.to_string()),
                    None => {
                        unknown.push(tag.clone());
                        None
                    }
                })
                .collect()
        };

        let exclude = resolve(exclude);
        let include = (!include.is_empty()).then(|| resolve(include));

        (Self { include, exclude }, unknown)
    }

    /// Whether code tagged with this language should be highlighted.
    pub fn allows(&self, language: &str) -> bool {
        if self.exclude.contains(language) {
            return false;
        }
        match &self.include {
            None => true,
            Some(include) => include.contains(language),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> Filter {
        let to_owned = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        Filter::new(&to_owned(include), &to_owned(exclude)).0
    }

    #[test]
    fn everything_is_allowed_by_default() {
        let filter = filter(&[], &[]);
        assert!(filter.allows("rust"));
        assert!(filter.allows("python"));
    }

    #[test]
    fn include_restricts_to_a_list() {
        let filter = filter(&["rust", "c"], &[]);
        assert!(filter.allows("rust"));
        assert!(filter.allows("c"));
        assert!(!filter.allows("python"));
    }

    #[test]
    fn exclude_removes_languages() {
        let filter = filter(&[], &["rust"]);
        assert!(!filter.allows("rust"));
        assert!(filter.allows("python"));
    }

    #[test]
    fn exclude_wins_over_include() {
        let filter = filter(&["rust", "c"], &["rust"]);
        assert!(!filter.allows("rust"));
        assert!(filter.allows("c"));
    }

    #[test]
    fn configured_tags_are_resolved_like_fence_tags() {
        let filter = filter(&[], &["sh", "JS"]);
        assert!(!filter.allows("bash"));
        assert!(!filter.allows("javascript"));
    }

    #[test]
    fn unknown_tags_are_reported() {
        let (_, unknown) = Filter::new(&[], &["not-a-language".to_string()]);
        assert_eq!(unknown, ["not-a-language"]);
    }
}
