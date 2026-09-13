//! Translating Vim regexes into Rust regexes.
//!
//! `#match?` in a Neovim query is *not* a standard tree-sitter `#match?`: it
//! is evaluated with `vim.regex()`, and Neovim prepends `\v` ("very magic")
//! unless the pattern already opens with a magic prefix. Very magic mode is
//! close to PCRE, but not identical -- `<` and `>` are word boundaries, `=`
//! is a quantifier, `%(` opens a non-capturing group, and `\c` switches the
//! whole pattern to case-insensitive wherever it appears.
//!
//! tree-sitter would otherwise compile these as Rust regexes at query-build
//! time, where a leading `\c` is simply a syntax error that takes the entire
//! `highlights.scm` down with it. Rewriting `#match?` into a predicate we
//! evaluate ourselves (see [`super::rewrite`]) and translating here keeps both
//! the compilation and the semantics right.

use cu::pre::*;
use regex::bytes::{Regex, RegexBuilder};

/// Compile a Vim regex, as Neovim's query predicates would.
pub fn compile(pattern: &str) -> cu::Result<Regex> {
    let (source, case_insensitive) = translate(pattern)?;
    cu::check!(
        RegexBuilder::new(&source)
            .unicode(false)
            .case_insensitive(case_insensitive)
            .build(),
        "vim regex {pattern:?} became invalid regex {source:?}"
    )
}

/// Translate a Vim regex to Rust regex source, plus whether `\c` was seen.
pub fn translate(pattern: &str) -> cu::Result<(String, bool)> {
    // Neovim only leaves the magic level alone when the pattern sets it
    // explicitly; everything else is forced to very magic.
    let body = match pattern.get(..2) {
        Some(r"\v") => &pattern[2..],
        Some(r"\m" | r"\M" | r"\V") => cu::bail!(
            "vim regex {pattern:?} selects a magic level other than very magic, \
             which is not supported"
        ),
        _ => pattern,
    };

    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 4);
    let mut case_insensitive = false;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let escaped = *cu::check!(
                    bytes.get(index + 1),
                    "vim regex ends with a dangling backslash"
                )?;
                index += 2;
                match escaped {
                    // Case sensitivity applies to the whole pattern, wherever
                    // the flag appears.
                    b'c' => case_insensitive = true,
                    b'C' => case_insensitive = false,
                    b'<' | b'>' => out.push_str(r"\b"),
                    b'w' => out.push_str("[0-9A-Za-z_]"),
                    b'W' => out.push_str("[^0-9A-Za-z_]"),
                    b'd' => out.push_str("[0-9]"),
                    b'D' => out.push_str("[^0-9]"),
                    b'a' => out.push_str("[A-Za-z]"),
                    b'A' => out.push_str("[^A-Za-z]"),
                    b'l' => out.push_str("[a-z]"),
                    b'L' => out.push_str("[^a-z]"),
                    b'u' => out.push_str("[A-Z]"),
                    b'U' => out.push_str("[^A-Z]"),
                    b'x' => out.push_str("[0-9A-Fa-f]"),
                    b'X' => out.push_str("[^0-9A-Fa-f]"),
                    b'h' => out.push_str("[A-Za-z_]"),
                    b'H' => out.push_str("[^A-Za-z_]"),
                    b'o' => out.push_str("[0-7]"),
                    b'O' => out.push_str("[^0-7]"),
                    b's' => out.push_str("[ \\t]"),
                    b'S' => out.push_str("[^ \\t]"),
                    b'n' => out.push_str(r"\n"),
                    b't' => out.push_str(r"\t"),
                    b'r' => out.push_str(r"\r"),
                    b'e' => out.push_str(r"\x1b"),
                    // In very magic, a backslash before anything else makes it
                    // an ordinary character.
                    other => push_literal(other, &mut out),
                }
            }
            // Word boundaries, not comparison operators.
            b'<' | b'>' => {
                out.push_str(r"\b");
                index += 1;
            }
            // Vim's second spelling of `?`.
            b'=' => {
                out.push('?');
                index += 1;
            }
            // `%(` is the non-capturing group; a bare `%` is a literal.
            b'%' if bytes.get(index + 1) == Some(&b'(') => {
                out.push_str("(?:");
                index += 2;
            }
            b'[' => {
                index = translate_set(bytes, index, &mut out)?;
            }
            b'@' | b'&' | b'~' => cu::bail!(
                "vim regex {pattern:?} uses `{}`, which has no regex equivalent",
                bytes[index] as char
            ),
            byte @ (b'^' | b'$' | b'.' | b'*' | b'+' | b'?' | b'(' | b')' | b'|' | b'{' | b'}') => {
                out.push(byte as char);
                index += 1;
            }
            byte => {
                push_literal(byte, &mut out);
                index += 1;
            }
        }
    }

    Ok((out, case_insensitive))
}

/// Copy `[...]` across, returning the index just past the closing bracket.
///
/// Character classes mean the same thing in both dialects, so the contents are
/// passed through; only `\` escapes are normalised.
fn translate_set(bytes: &[u8], open: usize, out: &mut String) -> cu::Result<usize> {
    let mut index = open + 1;
    out.push('[');

    if bytes.get(index) == Some(&b'^') {
        out.push('^');
        index += 1;
    }
    if bytes.get(index) == Some(&b']') {
        out.push_str(r"\]");
        index += 1;
    }

    while index < bytes.len() && bytes[index] != b']' {
        if bytes[index] == b'\\' {
            let escaped = *cu::check!(
                bytes.get(index + 1),
                "vim regex ends with a dangling backslash"
            )?;
            out.push('\\');
            out.push(escaped as char);
            index += 2;
            continue;
        }
        if bytes[index] == b'&' {
            // `&&` is set intersection in a regex class but nothing special
            // in Vim.
            out.push_str(r"\&");
        } else if bytes[index].is_ascii() {
            out.push(bytes[index] as char);
        } else {
            out.push_str(&format!("\\x{:02x}", bytes[index]));
        }
        index += 1;
    }

    cu::ensure!(index < bytes.len(), "vim regex has an unterminated `[` set")?;
    out.push(']');
    Ok(index + 1)
}

fn push_literal(byte: u8, out: &mut String) {
    const SPECIAL: &[u8] = br".^$*+?()[]{}|\";
    if SPECIAL.contains(&byte) {
        out.push('\\');
        out.push(byte as char);
    } else if byte.is_ascii() {
        out.push(byte as char);
    } else {
        out.push_str(&format!("\\x{byte:02x}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        compile(pattern).unwrap().is_match(text.as_bytes())
    }

    #[test]
    fn very_magic_groups_and_alternation_need_no_backslashes() {
        assert_eq!(translate("^(a|b)+$").unwrap().0, "^(a|b)+$");
        assert!(matches("^(a|b)+$", "abab"));
    }

    #[test]
    fn backslash_c_makes_the_whole_pattern_case_insensitive() {
        let (source, case_insensitive) = translate(r"\c^return$").unwrap();
        assert_eq!(source, "^return$");
        assert!(case_insensitive);
        assert!(matches(r"\c^return$", "RETURN"));
        assert!(!matches("^return$", "RETURN"));
    }

    #[test]
    fn angle_brackets_are_word_boundaries() {
        assert_eq!(translate("<word>").unwrap().0, r"\bword\b");
        assert!(matches("<word>", "a word here"));
        assert!(!matches("<word>", "awordhere"));
    }

    #[test]
    fn equals_is_the_optional_quantifier() {
        assert_eq!(translate("^ab=c$").unwrap().0, "^ab?c$");
        assert!(matches("^ab=c$", "ac"));
    }

    #[test]
    fn backslash_makes_a_special_character_ordinary() {
        assert_eq!(
            translate(r"^(:|v-bind|v-|\@)").unwrap().0,
            "^(:|v-bind|v-|@)"
        );
        assert!(matches(r"^(:|v-bind|v-|\@)", "@click"));
        assert_eq!(
            translate(r"^(!?\=|-[a-zA-Z]+)$").unwrap().0,
            r"^(!?=|-[a-zA-Z]+)$"
        );
    }

    #[test]
    fn percent_paren_is_a_non_capturing_group() {
        assert_eq!(translate("%(ab)+").unwrap().0, "(?:ab)+");
        assert!(matches("^%(ab)+$", "abab"));
    }

    #[test]
    fn character_classes_pass_through() {
        assert_eq!(translate(r"^[-+]?\d+$").unwrap().0, "^[-+]?[0-9]+$");
        assert!(matches(r"^[-+]?\d+$", "-42"));
    }

    #[test]
    fn rejects_constructs_with_no_equivalent() {
        assert!(translate(r"(foo)@=bar").is_err());
        assert!(translate(r"\Mliteral").is_err());
        assert!(translate("[abc").is_err());
        assert!(translate("trailing\\").is_err());
    }

    #[test]
    fn handles_the_real_corpus_shapes() {
        for pattern in [
            r"\c^(1|on|yes|true|y|0|off|no|false|n|ignore|notfound|.*-notfound)$",
            "^[A-Z][A-Z_]+",
            "^[A-Z][A-Z0-9_]+$|^[a-z]{2,3}[A-Z].+$",
            r"^assert[A-Za-z_0-9]*|error|info|debug|print|warning|warning_once$",
            r"^((GPVAL|MOUSE|FIT)_|ARG)\w+$",
            r"\c^(DefaultValue|LowLimit|HighLimit|SubNumber)$",
            "^([+*-+=<>]|<=|>=|/=)$",
            r"/\*!([a-zA-Z]+:)?re2c",
            "^[eE][nN][dD]([dD][oO])?$",
        ] {
            compile(pattern).unwrap_or_else(|e| panic!("{pattern:?}: {e:#}"));
        }
    }
}
