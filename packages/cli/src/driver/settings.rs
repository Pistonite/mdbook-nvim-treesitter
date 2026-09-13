//! Reading `[preprocessor.nvim-treesitter]` out of `book.toml`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cu::pre::*;
use mdbook_preprocessor::PreprocessorContext;
use serde::de::IgnoredAny;

/// The preprocessor's table in `book.toml`.
///
/// ```toml
/// [preprocessor.nvim-treesitter]
/// include = []          # only these languages, if given
/// exclude = []          # never these languages
/// cache_dir = ".cache"  # relative to book.toml
/// ```
#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// When non-empty, only code tagged with these languages is highlighted.
    pub include: Vec<String>,
    /// Code tagged with these languages is left to mdBook.
    pub exclude: Vec<String>,
    /// Where the book keeps its copy of the parsers and queries.
    pub cache_dir: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            cache_dir: PathBuf::from(".cache"),
        }
    }
}

/// The keys we read.
const OURS: &[&str] = &["include", "exclude", "cache_dir"];

/// Keys in our table that belong to mdBook, not to us.
///
/// mdBook reads these out of the same table to decide how and when to run the
/// preprocessor, so they are entirely normal to find here and must not be
/// reported as mistakes.
const MDBOOK: &[&str] = &["command", "before", "after", "optional", "renderers"];

impl Settings {
    /// The key the settings live under in `book.toml`.
    const KEY: &'static str = "preprocessor.nvim-treesitter";

    /// Read the settings for this run, along with anything odd about them.
    ///
    /// An absent table is not an error: mdBook only runs us because the table
    /// exists, but it may hold nothing but mdBook's own keys, and every field
    /// of ours has a default.
    ///
    /// A key we do not recognise is a warning rather than an error. It is
    /// worth saying something, because a misspelled `exclude` would otherwise
    /// do nothing at all -- but refusing to build would mean any key mdBook
    /// adds in future breaks every book using this preprocessor until it is
    /// released again, which is far worse than a stray line on stderr.
    pub fn load(context: &PreprocessorContext) -> cu::Result<(Self, Vec<String>)> {
        let settings = cu::check!(
            context.config.get::<Self>(Self::KEY),
            "invalid [{}] in book.toml",
            Self::KEY
        )?
        .unwrap_or_default();

        Ok((settings, unrecognized_keys(context)?))
    }

    /// The cache directory, resolved against the book root.
    pub fn cache_dir(&self, root: &std::path::Path) -> PathBuf {
        root.join(&self.cache_dir)
    }
}

/// Complaints about keys in our table that nothing will read.
fn unrecognized_keys(context: &PreprocessorContext) -> cu::Result<Vec<String>> {
    // Only the key names matter, so the values are deserialized into nothing.
    // That also means a key whose value we could not have parsed anyway --
    // mdBook's, or a future one of ours -- cannot fail this.
    let table = cu::check!(
        context
            .config
            .get::<BTreeMap<String, IgnoredAny>>(Settings::KEY),
        "invalid [{}] in book.toml",
        Settings::KEY
    )?
    .unwrap_or_default();

    Ok(table
        .into_keys()
        .filter(|key| !OURS.contains(&key.as_str()) && !MDBOOK.contains(&key.as_str()))
        .map(|key| {
            format!(
                "book.toml sets [{}] {key}, which is not a setting this preprocessor has \
                 (it reads {})",
                Settings::KEY,
                OURS.join(", ")
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdbook_preprocessor::config::Config;

    fn load(toml: &str) -> cu::Result<(Settings, Vec<String>)> {
        let mut config = Config::default();
        for (key, value) in toml_pairs(toml) {
            config.set(key, value).unwrap();
        }
        let context = PreprocessorContext::new(PathBuf::from("/book"), config, "html".to_string());
        Settings::load(&context)
    }

    fn settings(toml: &str) -> Settings {
        load(toml).unwrap().0
    }

    fn warnings(toml: &str) -> Vec<String> {
        load(toml).unwrap().1
    }

    /// Minimal `key = json` pairs, to avoid pulling in a TOML parser here.
    fn toml_pairs(spec: &str) -> Vec<(String, cu::json::Value)> {
        spec.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (key, value) = line.split_once('=').expect("key = value");
                (
                    key.trim().to_string(),
                    cu::json::parse(value.trim()).expect("valid json value"),
                )
            })
            .collect()
    }

    #[test]
    fn an_empty_table_uses_the_defaults() {
        let settings = settings("");
        assert!(settings.include.is_empty());
        assert!(settings.exclude.is_empty());
        assert_eq!(settings.cache_dir, PathBuf::from(".cache"));
        assert_eq!(warnings(""), [] as [String; 0]);
    }

    #[test]
    fn reads_every_field() {
        let settings = settings(
            r#"
            preprocessor.nvim-treesitter.include = ["rust", "c"]
            preprocessor.nvim-treesitter.exclude = ["python"]
            preprocessor.nvim-treesitter.cache_dir = "build/ts"
            "#,
        );

        assert_eq!(settings.include, ["rust", "c"]);
        assert_eq!(settings.exclude, ["python"]);
        assert_eq!(settings.cache_dir, PathBuf::from("build/ts"));
    }

    #[test]
    fn resolves_the_cache_dir_against_the_book_root() {
        let settings = Settings::default();
        assert_eq!(
            settings.cache_dir(std::path::Path::new("/book")),
            PathBuf::from("/book/.cache")
        );
    }

    #[test]
    fn mdbook_own_keys_are_accepted_in_silence() {
        // Every key mdBook reads out of this same table. None of them is our
        // business, and all of them are ordinary things to find in a book.toml.
        let toml = r#"
            preprocessor.nvim-treesitter.command = "mdbook-nvim-treesitter"
            preprocessor.nvim-treesitter.before = ["links"]
            preprocessor.nvim-treesitter.after = ["index"]
            preprocessor.nvim-treesitter.optional = true
            preprocessor.nvim-treesitter.renderers = ["html"]
            preprocessor.nvim-treesitter.exclude = ["rust"]
            "#;

        assert_eq!(warnings(toml), [] as [String; 0]);
        assert_eq!(settings(toml).exclude, ["rust"]);
    }

    #[test]
    fn a_misspelled_key_warns_and_still_builds() {
        let warnings = warnings(r#"preprocessor.nvim-treesitter.excludes = ["python"]"#);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("excludes"), "{warnings:?}");
        // The warning has to say what the real settings are, or it does not
        // help anyone find the typo.
        assert!(warnings[0].contains("exclude"), "{warnings:?}");
    }

    #[test]
    fn an_unknown_key_does_not_stop_the_rest_being_read() {
        let toml = r#"
            preprocessor.nvim-treesitter.future_mdbook_key = "whatever"
            preprocessor.nvim-treesitter.include = ["rust"]
            "#;

        assert_eq!(settings(toml).include, ["rust"]);
        assert_eq!(warnings(toml).len(), 1);
    }

    #[test]
    fn a_setting_of_the_wrong_type_is_still_an_error() {
        // Warning on an unknown key must not turn into shrugging at a key we
        // do read: `exclude = "rust"` is a mistake we can and should catch.
        let error = load(r#"preprocessor.nvim-treesitter.exclude = "rust""#).unwrap_err();
        assert!(format!("{error:#}").contains("book.toml"), "{error:#}");
    }
}
