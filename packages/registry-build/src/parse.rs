//! Reading the two nvim-treesitter Lua tables we depend on.
//!
//! Both files are formatted with [StyLua] -- the bot that bumps grammar
//! revisions rewrites `parsers.lua` wholesale and pipes it through StyLua, and
//! the hand-written pull requests that make up most of the edits are gated on
//! `stylua --check`. StyLua preserves whatever layout it is given, so
//! "stylua-clean" is the only invariant that actually holds: an `install_info`
//! may be written on one line, a value may be double-quoted, a key may be
//! bracketed, and a comment may sit at the end of any line.
//!
//! So the tables are parsed with `full_moon`, which is StyLua's own parser.
//! Anything StyLua accepts, this accepts. A scan over the text could not say
//! the same, and its failure mode is the dangerous one: a missed
//! `install_info` is indistinguishable from a language that genuinely has no
//! grammar (see [`Language::install`]), and would silently drop that
//! language's highlighting.
//!
//! Unrecognised keys are a hard error rather than an ignored field, for the
//! same reason: `parsers.lua` gains keys from time to time, and finding out at
//! the next revision bump is much cheaper than finding out from a book that
//! stopped highlighting.
//!
//! [StyLua]: https://github.com/JohnnyMorganz/StyLua

use cu::pre::*;
use full_moon::ast::{Ast, Expression, Field, LastStmt, Stmt, TableConstructor};
use full_moon::tokenizer::{Symbol, TokenType};

pub fn parse_db(parsers_lua: &str, filetypes_lua: &str) -> cu::Result<(Vec<Language>, Vec<Alias>)> {
    // <BY_HUMAN>
    let languages = cu::check!(
        parse_languages(parsers_lua),
        "failed to parse languages from parsers.lua"
    )?;
    cu::info!("found {} languages", languages.len());
    // parse raw aliases, then collect them from the language file types
    let mut aliases = cu::check!(
        parse_aliases(filetypes_lua),
        "failed to parse aliases from filetypes.lua"
    )?;
    collect_aliases(&languages, &mut aliases);
    cu::info!("found {} filetype aliases", aliases.len());

    Ok((languages, aliases))
}

/// Every alias, from both places nvim-treesitter declares them.
///
/// `filetypes.lua` is the usual one. A language entry may also carry a
/// `filetype` key -- `powershell` does -- and while that one is currently
/// declared in both places, the next to use it might not be.
fn collect_aliases(languages: &[Language], aliases: &mut Vec<Alias>) {
    for language in languages {
        let Some(filetype) = &language.filetype else {
            continue;
        };
        let already = aliases
            .iter()
            .any(|a| &a.alias == filetype && a.language == language.name);
        if !already {
            aliases.push(Alias {
                alias: filetype.clone(),
                language: language.name.clone(),
            });
        }
    }

    // filetypes.lua occasionally keeps an alias for a language that has since
    // left parsers.lua. Such an alias can never resolve, so drop it here
    // rather than shipping a dangling entry.
    aliases.retain(|alias| {
        let known = languages.iter().any(|l| l.name == alias.language);
        if !known {
            cu::warn!(
                "dropping alias {:?}: no such language {:?}",
                alias.alias,
                alias.language
            );
        }
        known
    });

    aliases.sort_by(|a, b| a.alias.cmp(&b.alias));
}

/// One entry of `lua/nvim-treesitter/parsers.lua`.
#[derive(Debug, PartialEq)]
pub struct Language {
    pub name: String,
    /// `None` for query-only languages such as `ecma` and `html_tags`, which
    /// exist purely to be pulled in by a `; inherits:` modeline.
    ///
    /// This is a meaningful value, not a parse failure, which is exactly why
    /// the parsing above has to be exact.
    pub install: Option<Install>,
    /// Languages whose *queries* this language's queries depend on.
    pub requires: Vec<String>,
    pub tier: u8,
    /// A Neovim filetype declared inline, as `powershell` does.
    ///
    /// `filetypes.lua` is the usual place for these; this is a second one, and
    /// the only entry using it today also appears there. Collected so that the
    /// next entry to use it alone does not lose its alias.
    pub filetype: Option<String>,
}

/// The `install_info` sub-table: where the grammar comes from.
#[derive(Debug, PartialEq)]
pub struct Install {
    pub url: String,
    pub revision: String,
    pub branch: Option<String>,
    /// Sub-directory of the repo holding `grammar.js`, for monorepos.
    pub location: Option<String>,
    /// The repo ships no pre-generated `src/parser.c`; it must be generated.
    pub generate: bool,
}

/// One filetype alias: `alias` is what a user writes, `language` is the
/// nvim-treesitter language it resolves to.
#[derive(Debug, PartialEq)]
pub struct Alias {
    pub alias: String,
    pub language: String,
}

/// Keys we read from a language entry. Anything else is an error.
const LANGUAGE_KEYS: &[&str] = &[
    "install_info",
    "requires",
    "tier",
    "filetype",
    // Prose for nvim-treesitter's own README; nothing to do with us.
    "readme_name",
    "readme_note",
    "maintainers",
    "experimental",
];

/// Keys we read from an `install_info`. Anything else is an error.
const INSTALL_KEYS: &[&str] = &["url", "revision", "branch", "location", "generate"];

/// Parse `lua/nvim-treesitter/parsers.lua` into a name-sorted language list.
pub fn parse_languages(source: &str) -> cu::Result<Vec<Language>> {
    let ast = parse_lua(source, "parsers.lua")?;

    let Some(LastStmt::Return(returned)) = ast.nodes().last_stmt() else {
        cu::bail!("parsers.lua does not end in a `return`");
    };
    let table = cu::check!(
        returned.returns().iter().next().and_then(table_of),
        "parsers.lua does not return a table"
    )?;

    let mut languages = Vec::new();
    for (name, value) in entries(table, "parsers.lua")? {
        languages.push(cu::check!(
            parse_language(&name, value),
            "invalid entry for `{name}`"
        )?);
    }

    cu::ensure!(!languages.is_empty(), "no languages found in parsers.lua")?;
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(languages)
}

fn parse_language(name: &str, value: &Expression) -> cu::Result<Language> {
    let table = cu::check!(table_of(value), "expected a table")?;

    let mut language = Language {
        name: name.to_string(),
        install: None,
        requires: Vec::new(),
        tier: 2,
        filetype: None,
    };

    for (key, value) in entries(table, name)? {
        cu::ensure!(
            LANGUAGE_KEYS.contains(&key.as_str()),
            "unrecognised key `{key}`; if nvim-treesitter added it, decide \
             whether we need it and add it to LANGUAGE_KEYS"
        )?;
        match key.as_str() {
            "install_info" => language.install = Some(parse_install(value)?),
            "requires" => {
                language.requires =
                    cu::check!(string_list(value), "`requires` is not a string list")?
            }
            "tier" => language.tier = cu::check!(number(value), "`tier` is not a number")?,
            "filetype" => {
                language.filetype = Some(cu::check!(string(value), "`filetype` is not a string")?)
            }
            _ => {}
        }
    }

    Ok(language)
}

fn parse_install(value: &Expression) -> cu::Result<Install> {
    let table = cu::check!(table_of(value), "`install_info` is not a table")?;

    let mut url = None;
    let mut revision = None;
    let mut branch = None;
    let mut location = None;
    let mut generate = false;

    for (key, value) in entries(table, "install_info")? {
        cu::ensure!(
            INSTALL_KEYS.contains(&key.as_str()),
            "unrecognised `install_info` key `{key}`. nvim-treesitter's schema \
             also defines `path`, `generate_from_json` and `queries`, none of \
             which we implement -- handle it rather than ignoring it"
        )?;
        match key.as_str() {
            "url" => url = Some(cu::check!(string(value), "`url` is not a string")?),
            "revision" => revision = Some(cu::check!(string(value), "`revision` is not a string")?),
            "branch" => branch = Some(cu::check!(string(value), "`branch` is not a string")?),
            "location" => location = Some(cu::check!(string(value), "`location` is not a string")?),
            "generate" => generate = cu::check!(boolean(value), "`generate` is not a boolean")?,
            _ => {}
        }
    }

    Ok(Install {
        url: cu::check!(url, "`install_info` has no url")?
            .trim_end_matches(".git")
            .to_string(),
        revision: cu::check!(revision, "`install_info` has no revision")?,
        branch,
        location,
        generate,
    })
}

/// Parse `plugin/filetypes.lua` into alias -> language pairs, sorted by alias.
pub fn parse_aliases(source: &str) -> cu::Result<Vec<Alias>> {
    let ast = parse_lua(source, "filetypes.lua")?;

    // The file is `local filetypes = { … }` followed by a registration loop.
    let table = ast
        .nodes()
        .stmts()
        .find_map(|stmt| match stmt {
            Stmt::LocalAssignment(assignment) => {
                assignment.expressions().iter().next().and_then(table_of)
            }
            _ => None,
        })
        .ok_or_else(|| cu::fmterr!("filetypes.lua has no `local filetypes = {{ … }}`"))?;

    let mut aliases = Vec::new();
    for (language, value) in entries(table, "filetypes.lua")? {
        let filetypes = cu::check!(
            string_list(value),
            "`{language}` is not a list of filetypes"
        )?;
        for alias in filetypes {
            aliases.push(Alias {
                alias,
                language: language.clone(),
            });
        }
    }

    cu::ensure!(!aliases.is_empty(), "no aliases found in filetypes.lua")?;
    aliases.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(aliases)
}

fn parse_lua(source: &str, what: &str) -> cu::Result<Ast> {
    full_moon::parse(source).map_err(|errors| {
        let first = errors
            .first()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unknown error".to_string());
        cu::fmterr!("{what} is not valid Lua: {first}")
    })
}

/// The `key = value` fields of a table, in source order.
///
/// Both `name = …` and `['name'] = …` are accepted; `vim.inspect` emits the
/// bracketed form for any key that is not a bare identifier. A positional
/// field would mean the file is not the shape we think it is, so it is an
/// error rather than a skip.
fn entries<'a>(
    table: &'a TableConstructor,
    what: &str,
) -> cu::Result<Vec<(String, &'a Expression)>> {
    table
        .fields()
        .iter()
        .map(|field| match field {
            Field::NameKey { key, value, .. } => Ok((key.token().to_string(), value)),
            Field::ExpressionKey { key, value, .. } => {
                let key = cu::check!(string(key), "{what} has a non-string bracketed key")?;
                Ok((key, value))
            }
            _ => cu::bail!("{what} has a field with no key"),
        })
        .collect()
}

fn table_of(expr: &Expression) -> Option<&TableConstructor> {
    match expr {
        Expression::TableConstructor(table) => Some(table),
        _ => None,
    }
}

/// The contents of a string literal, with its quotes already removed.
///
/// Quote style therefore does not matter: `'a'`, `"a"` and `[[a]]` all read
/// the same.
fn string(expr: &Expression) -> Option<String> {
    match expr {
        Expression::String(token) => match token.token_type() {
            TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn boolean(expr: &Expression) -> Option<bool> {
    match expr {
        Expression::Symbol(token) => match token.token_type() {
            TokenType::Symbol {
                symbol: Symbol::True,
            } => Some(true),
            TokenType::Symbol {
                symbol: Symbol::False,
            } => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn number(expr: &Expression) -> Option<u8> {
    match expr {
        Expression::Number(token) => match token.token_type() {
            TokenType::Number { text } => text.parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

/// A Lua sequence of strings, `{ 'a', 'b' }`.
fn string_list(expr: &Expression) -> Option<Vec<String>> {
    table_of(expr)?
        .fields()
        .iter()
        .map(|field| match field {
            Field::NoKey(value) => string(value),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape the bot emits: one key per line, single quotes.
    const PARSERS: &str = "---@type nvim-ts.parsers
return {
  c = {
    install_info = {
      revision = 'b780e47f',
      url = 'https://github.com/tree-sitter/tree-sitter-c.git',
    },
    tier = 1,
  },
  cpp = {
    install_info = {
      location = 'cpp',
      generate = true,
      revision = '8b5b49eb',
      url = 'https://github.com/tree-sitter/tree-sitter-cpp',
    },
    requires = { 'c' },
    tier = 2,
  },
  ecma = {
    readme_note = 'queries required by javascript',
    tier = 2,
  },
  sql = {
    install_info = {
      branch = 'gh-pages',
      revision = '593a5ecc',
      url = 'https://github.com/derekstride/tree-sitter-sql',
    },
    tier = 3,
  },
}
";

    fn language<'a>(languages: &'a [Language], name: &str) -> &'a Language {
        languages
            .iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("no `{name}` in {languages:?}"))
    }

    #[test]
    fn parses_every_entry() {
        let languages = parse_languages(PARSERS).unwrap();
        let names: Vec<_> = languages.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["c", "cpp", "ecma", "sql"]);
    }

    #[test]
    fn strips_git_suffix_from_url() {
        let languages = parse_languages(PARSERS).unwrap();
        let c = language(&languages, "c").install.as_ref().unwrap();
        assert_eq!(c.url, "https://github.com/tree-sitter/tree-sitter-c");
        assert_eq!(c.revision, "b780e47f");
        assert!(!c.generate);
        assert_eq!(c.location, None);
        assert_eq!(language(&languages, "c").tier, 1);
    }

    #[test]
    fn parses_optional_install_fields() {
        let languages = parse_languages(PARSERS).unwrap();
        let cpp = language(&languages, "cpp").install.as_ref().unwrap();
        assert_eq!(cpp.location.as_deref(), Some("cpp"));
        assert!(cpp.generate);
        assert_eq!(language(&languages, "cpp").requires, ["c"]);

        let sql = language(&languages, "sql").install.as_ref().unwrap();
        assert_eq!(sql.branch.as_deref(), Some("gh-pages"));
    }

    #[test]
    fn query_only_language_has_no_install_info() {
        // `install: None` is a real value, not a parse failure -- these are the
        // languages that exist only to be inherited from. Every test below
        // that checks a layout variant is really checking that a *grammar*
        // never turns into one of these by accident.
        let languages = parse_languages(PARSERS).unwrap();
        assert!(language(&languages, "ecma").install.is_none());
        assert!(language(&languages, "c").install.is_some());
    }

    #[test]
    fn reads_an_install_info_written_on_one_line() {
        // StyLua leaves this alone when it fits in 100 columns, so a
        // hand-written entry can look like this and pass upstream CI.
        let languages = parse_languages(
            "return {
  robot = { install_info = { revision = 'v0.2.0', url = 'https://example.com/r' }, tier = 1 },
}
",
        )
        .unwrap();
        let robot = language(&languages, "robot").install.as_ref().unwrap();
        assert_eq!(robot.revision, "v0.2.0");
    }

    #[test]
    fn ignores_comments() {
        let languages = parse_languages(
            "-- a header comment
return {
  apex = {
    install_info = {
      location = 'apex', -- monorepo
      revision = 'da568eee',
      url = 'https://example.com/a',
    },
    tier = 2, --[[ inline ]]
  },
}
",
        )
        .unwrap();
        let apex = language(&languages, "apex").install.as_ref().unwrap();
        assert_eq!(apex.location.as_deref(), Some("apex"));
    }

    #[test]
    fn reads_any_quote_style() {
        let languages = parse_languages(
            "return {
  a = { install_info = { revision = \"aaaa\", url = [[https://example.com/a]] } },
}
",
        )
        .unwrap();
        let a = language(&languages, "a").install.as_ref().unwrap();
        assert_eq!(a.revision, "aaaa");
        assert_eq!(a.url, "https://example.com/a");
    }

    #[test]
    fn reads_a_bracketed_key() {
        // `vim.inspect` emits this form for any key that is not a bare
        // identifier, so it can appear the next time one is added.
        let languages = parse_languages(
            "return {
  ['c-plus-plus'] = { install_info = { revision = 'aaaa', url = 'https://example.com/c' } },
}
",
        )
        .unwrap();
        assert_eq!(languages[0].name, "c-plus-plus");
    }

    #[test]
    fn reads_an_inline_filetype() {
        let languages = parse_languages(
            "return {
  powershell = {
    filetype = 'ps1',
    install_info = { revision = 'aaaa', url = 'https://example.com/p' },
  },
}
",
        )
        .unwrap();
        assert_eq!(languages[0].filetype.as_deref(), Some("ps1"));
    }

    #[test]
    fn an_unrecognised_language_key_is_an_error() {
        // Loud beats silent: a key we drop without a word is how a language
        // quietly stops working at the next revision bump.
        let error = parse_languages(
            "return { a = { install_info = { revision = 'a', url = 'u' }, brand_new_key = 1 } }",
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("brand_new_key"), "{message}");
    }

    #[test]
    fn an_unrecognised_install_key_is_an_error() {
        for key in ["path", "generate_from_json", "queries"] {
            let source = format!(
                "return {{ a = {{ install_info = {{ revision = 'a', url = 'u', {key} = 1 }} }} }}"
            );
            let error = parse_languages(&source).unwrap_err();
            assert!(format!("{error:#}").contains(key), "{key} was accepted");
        }
    }

    #[test]
    fn an_install_info_without_a_url_is_an_error() {
        let error =
            parse_languages("return { a = { install_info = { revision = 'a' } } }").unwrap_err();
        assert!(format!("{error:#}").contains("no url"), "{error:#}");
    }

    #[test]
    fn parses_aliases_flattened_and_sorted() {
        let aliases = parse_aliases(
            "local filetypes = {
  bash = { 'sh' },
  javascript = { 'jsx', 'js' },
}

for lang, fts in pairs(filetypes) do
  vim.treesitter.language.register(lang, fts)
end
",
        )
        .unwrap();
        let pairs: Vec<_> = aliases
            .iter()
            .map(|a| (a.alias.as_str(), a.language.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [("js", "javascript"), ("jsx", "javascript"), ("sh", "bash")]
        );
    }

    #[test]
    fn rejects_input_that_is_not_the_table_we_expect() {
        assert!(parse_languages("return {}").is_err());
        assert!(parse_languages("local x = 1").is_err());
        assert!(parse_languages("return { !!! }").is_err());
        assert!(parse_aliases("local filetypes = {}").is_err());
        assert!(parse_aliases("return {}").is_err());
    }
}
