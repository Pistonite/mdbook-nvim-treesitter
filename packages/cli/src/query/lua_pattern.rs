//! Translating Lua patterns into Rust regexes.
//!
//! `#lua-match?` is by far the most common conditional predicate in
//! nvim-treesitter's queries, and its argument is a [Lua pattern], not a
//! regex. The two look similar enough to be dangerous: `%` escapes instead of
//! `\`, `-` is a *lazy quantifier* rather than a literal, `^` and `$` are
//! anchors only at the very ends, and a backslash is an ordinary character.
//! Feeding one to a regex engine unchanged silently changes which spans get
//! highlighted, so each pattern is translated properly instead.
//!
//! The result is compiled in byte mode with Unicode off, because Lua patterns
//! are defined over bytes, and with `.` matching newlines, which it does in
//! Lua but not by default in a regex.
//!
//! [Lua pattern]: https://www.lua.org/manual/5.4/manual.html#6.4.1

use cu::pre::*;
use regex::bytes::{Regex, RegexBuilder};

/// Compile a Lua pattern into a regex with Lua's matching semantics.
pub fn compile(pattern: &str) -> cu::Result<Regex> {
    let source = translate(pattern)?;
    cu::check!(
        RegexBuilder::new(&source)
            .unicode(false)
            .dot_matches_new_line(true)
            .build(),
        "lua pattern {pattern:?} became invalid regex {source:?}"
    )
}

/// Translate a Lua pattern to equivalent Rust regex source.
pub fn translate(pattern: &str) -> cu::Result<String> {
    let bytes = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len() + 8);

    // `^` and `$` are anchors only at the very start and end; anywhere else
    // they are ordinary characters.
    let mut index = 0;
    if bytes.first() == Some(&b'^') {
        out.push('^');
        index = 1;
    }
    let anchored_end = bytes.len() > index && bytes[bytes.len() - 1] == b'$';
    let end = if anchored_end {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    // A quantifier is only a quantifier when something precedes it that it
    // could repeat -- that is what makes `^-` a literal dash but `data-` a
    // lazily repeated `a`.
    let mut quantifiable = false;

    while index < end {
        match bytes[index] {
            b'%' => {
                let class =
                    *cu::check!(bytes.get(index + 1), "lua pattern ends with a dangling `%`")?;
                out.push_str(&escape_sequence(class, false)?);
                quantifiable = true;
                index += 2;
            }
            b'[' => {
                index = translate_set(bytes, index, end, &mut out)?;
                quantifiable = true;
            }
            b'.' => {
                out.push('.');
                quantifiable = true;
                index += 1;
            }
            b'(' => {
                out.push('(');
                quantifiable = false;
                index += 1;
            }
            b')' => {
                out.push(')');
                quantifiable = true;
                index += 1;
            }
            byte @ (b'*' | b'+' | b'?') if quantifiable => {
                out.push(byte as char);
                quantifiable = false;
                index += 1;
            }
            // Lua's lazy repeat, which a regex spells `*?`.
            b'-' if quantifiable => {
                out.push_str("*?");
                quantifiable = false;
                index += 1;
            }
            byte => {
                push_literal(byte, &mut out);
                quantifiable = true;
                index += 1;
            }
        }
    }

    if anchored_end {
        out.push('$');
    }
    Ok(out)
}

/// Translate `[...]`, returning the index just past the closing bracket.
fn translate_set(bytes: &[u8], open: usize, end: usize, out: &mut String) -> cu::Result<usize> {
    let mut index = open + 1;
    out.push('[');

    if bytes.get(index) == Some(&b'^') {
        out.push('^');
        index += 1;
    }
    // A `]` in the first position is a literal, not the end of the set.
    if bytes.get(index) == Some(&b']') {
        out.push_str("\\]");
        index += 1;
    }

    let close = cu::check!(
        (index..end).find(|&i| bytes[i] == b']'),
        "lua pattern has an unterminated `[` set"
    )?;

    while index < close {
        if bytes[index] == b'%' {
            let class = *cu::check!(bytes.get(index + 1), "lua pattern ends with a dangling `%`")?;
            out.push_str(&escape_sequence(class, true)?);
            index += 2;
            continue;
        }
        // Lua reads a range only when a third character follows the `-` before
        // the closing bracket; otherwise the dash is an ordinary member.
        if bytes.get(index + 1) == Some(&b'-') && index + 2 < close {
            push_set_literal(bytes[index], out);
            out.push('-');
            push_set_literal(bytes[index + 2], out);
            index += 3;
            continue;
        }
        push_set_literal(bytes[index], out);
        index += 1;
    }

    out.push(']');
    Ok(close + 1)
}

/// Translate the character after a `%`.
///
/// `in_set` selects between a standalone class (`[0-9]`) and the bare range
/// body used inside an existing `[...]` (`0-9`).
fn escape_sequence(class: u8, in_set: bool) -> cu::Result<String> {
    let ranges = match class.to_ascii_lowercase() {
        b'a' => "a-zA-Z",
        b'd' => "0-9",
        b'l' => "a-z",
        b'u' => "A-Z",
        b'w' => "0-9a-zA-Z",
        b'x' => "0-9a-fA-F",
        b's' => r" \t\n\x0b\f\r",
        b'p' => r"!-/:-@\[-`{-~",
        b'c' => r"\x00-\x1f\x7f",
        b'g' => r"\x21-\x7e",
        // Not a class letter, so this is a `%`-escaped literal such as `%.`.
        _ => {
            let mut escaped = String::new();
            if in_set {
                push_set_literal(class, &mut escaped);
            } else {
                push_literal(class, &mut escaped);
            }
            return Ok(escaped);
        }
    };

    // An uppercase class letter is the complement of the lowercase one.
    let negated = class.is_ascii_uppercase();
    match (in_set, negated) {
        (false, false) => Ok(format!("[{ranges}]")),
        (false, true) => Ok(format!("[^{ranges}]")),
        (true, false) => Ok(ranges.to_string()),
        // A complemented class has no bare form, so it cannot be folded into a
        // surrounding set. No nvim-treesitter query does this today.
        (true, true) => cu::bail!(
            "lua pattern uses the complemented class `%{}` inside a `[...]` set, \
             which has no regex equivalent",
            class as char
        ),
    }
}

/// Append one byte as a regex literal.
fn push_literal(byte: u8, out: &mut String) {
    const SPECIAL: &[u8] = br".^$*+?()[]{}|\";
    if SPECIAL.contains(&byte) {
        out.push('\\');
        out.push(byte as char);
    } else if byte.is_ascii() {
        out.push(byte as char);
    } else {
        // Unicode is off, so a non-ASCII byte has to be written as a byte
        // escape rather than as a character.
        out.push_str(&format!("\\x{byte:02x}"));
    }
}

/// Append one byte as a literal inside a character class.
fn push_set_literal(byte: u8, out: &mut String) {
    // `&` is escaped because `&&` means set intersection in a regex class.
    const SPECIAL: &[u8] = br"]^-\[&";
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
    fn translates_classes_standalone_and_in_sets() {
        assert_eq!(translate("^[A-Z][A-Z%d_]*$").unwrap(), "^[A-Z][A-Z0-9_]*$");
        assert_eq!(translate("%d+").unwrap(), "[0-9]+");
        assert_eq!(translate("%A+").unwrap(), "[^a-zA-Z]+");
    }

    #[test]
    fn percent_escapes_a_literal() {
        assert_eq!(translate("a%.b").unwrap(), r"a\.b");
        assert_eq!(translate("100%%").unwrap(), "100%");
        assert_eq!(translate("^%[%[").unwrap(), r"^\[\[");
        assert!(matches("a%.b", "a.b"));
        assert!(!matches("a%.b", "axb"));
    }

    #[test]
    fn a_backslash_is_an_ordinary_character() {
        // Lua patterns escape with `%`, so `\` carries no meaning. Passing it
        // through unescaped would turn `\\if` into a regex escape.
        assert_eq!(
            translate(r"^\\if[a-zA-Z@]+$").unwrap(),
            r"^\\\\if[a-zA-Z@]+$"
        );
        assert!(matches(r"^\\if", r"\\ifx"));
        assert!(!matches(r"^\\if", "if"));
    }

    #[test]
    fn dash_is_a_lazy_quantifier_only_after_an_atom() {
        // Leading dash: nothing to repeat, so it is a literal.
        assert_eq!(translate("^-%>[^>]").unwrap(), "^->[^>]");
        assert!(matches("^-%>", "->x"));

        // After an atom it repeats that atom lazily, which is why `^data-`
        // matches anything starting with `dat`.
        assert_eq!(translate("^data-").unwrap(), "^data*?");
        assert!(!matches("^data-", "dtoo"));
        assert!(matches("^data-", "dat"));
    }

    #[test]
    fn quantifiers_without_an_atom_are_literal() {
        // `$` is only an anchor at the end, so here it is a literal repeated
        // by `+`, and the trailing `$` is the anchor.
        assert_eq!(translate("^$+[0-9]+$*$").unwrap(), r"^\$+[0-9]+\$*$");
        assert!(matches("^$+[0-9]+$*$", "$42$"));
        assert!(matches("^$+[0-9]+$*$", "$42"));
    }

    #[test]
    fn anchors_only_bind_at_the_ends() {
        assert_eq!(translate("a^b$c").unwrap(), r"a\^b\$c");
        assert!(matches("a^b$c", "xa^b$cx"));
    }

    #[test]
    fn set_dashes_are_ranges_or_members_like_lua() {
        // `A-Z` is a range; the second dash has no third character before the
        // bracket to span to, so it is a member.
        assert_eq!(translate("^[A-Z-_]").unwrap(), r"^[A-Z\-_]");
        assert!(matches("^[A-Z-_]", "-x"));
        assert!(matches("^[A-Z-_]", "Qx"));

        assert_eq!(translate("^[-][-]").unwrap(), r"^[\-][\-]");
        assert!(matches("^[-][-]", "--x"));
    }

    #[test]
    fn escapes_regex_specials_that_lua_treats_as_literal() {
        assert_eq!(translate("^{[-]|[^|]").unwrap(), r"^\{[\-]\|[^|]");
        assert!(matches("^{[-]|[^|]", "{-|x"));
        assert_eq!(translate(".+[~]$").unwrap(), ".+[~]$");
    }

    #[test]
    fn dot_matches_a_newline_as_it_does_in_lua() {
        assert!(matches("^a.b$", "a\nb"));
    }

    #[test]
    fn groups_and_alternation_free_quantifiers() {
        assert_eq!(
            translate("^[%d]+(%.[%d]+)?$").unwrap(),
            r"^[0-9]+(\.[0-9]+)?$"
        );
        assert!(matches("^[%d]+(%.[%d]+)?$", "3.14"));
        assert!(matches("^[%d]+(%.[%d]+)?$", "3"));
    }

    #[test]
    fn rejects_patterns_it_cannot_translate_faithfully() {
        assert!(translate("abc%").is_err());
        assert!(translate("[abc").is_err());
        assert!(translate("[%D]").is_err());
    }

    #[test]
    fn handles_the_real_corpus_shapes() {
        for pattern in [
            "^[%u][%u%d_]*$",
            "^_*[A-Z][A-Z%d_]*$",
            "^%a*%.*writePy%a*%d*%a*$",
            "^%s*;+%s?query",
            "^;+ *inherits *:",
            "/[*/][!*/]<?[^a-zA-Z]",
            "^[%%/][%l%u][%l%u%d.]*$",
            "^__%?[A-Z_a-z0-9]+%?__$",
            "^[\"']",
            "^%${.*}",
        ] {
            compile(pattern).unwrap_or_else(|e| panic!("{pattern:?}: {e:#}"));
        }
    }
}
