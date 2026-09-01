// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

//! The one reader for a quoted literal, and the one rule for what a backslash
//! means inside it.
//!
//! # Why this module exists
//!
//! Quoted text used to be read in ten places, with three different escape rules
//! and two different answers for an unterminated literal. The lexer processed no
//! escapes at all; the delegate-argument splitter let a backslash protect the
//! next character; the pipe finder skipped two bytes after one. Two of the
//! splitters treated an unterminated quote as a hard error and two silently
//! swallowed the rest of the line. Paren matching ignored quotes entirely, so
//! `deny("blocked (see policy)")` was refused as a malformed call while
//! `regex(")` compiled to a pattern matching one quote character.
//!
//! Every site now goes through [`read_literal`] or [`skip_literal`], so a quote
//! means the same thing wherever it appears and a backslash means the same thing
//! inside one.
//!
//! # The escape set
//!
//! Exactly `\\`, `\'`, and `\"`. That is the minimum that closes the rule:
//! without it there is no way to write a quote inside a literal delimited by that
//! quote, which is the gap that made two of the scanners above disagree.
//!
//! `\n` and `\t` are deliberately excluded. A deny reason rides in a violation
//! field a host renders, so a multi-line reason there is a display problem rather
//! than a missing capability. An unrecognized escape is an error naming the
//! character, so a pattern that needs a literal backslash (a regex character
//! class, say) says so by doubling it rather than by relying on passthrough.

/// What went wrong reading a literal, and where.
///
/// `at` is a **character** offset into the source, not a byte offset. Callers
/// render it directly, and a byte offset lands to the right of the real column
/// once any multi-byte character precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiteralError {
    /// Character offset the fault is reported at.
    pub(crate) at: usize,
    /// What the reader refused, phrased to name the construct.
    pub(crate) msg: String,
}

/// A literal read off the front of `src[open..]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Literal {
    /// The content with escapes resolved.
    pub(crate) value: String,
    /// Byte index just past the closing quote.
    pub(crate) end: usize,
}

/// Whether `b` opens a literal. Both quote styles are accepted, and the closing
/// quote must be the same character, so `"it's"` and `'say "hi"'` each carry the
/// other quote as content.
pub(crate) const fn is_quote(b: u8) -> bool {
    b == b'"' || b == b'\''
}

/// The character offset of byte index `at` in `src`.
///
/// Used for every position this module reports, and by the lexer for its own, so
/// a non-ASCII identifier is named at the character a reader can count to.
pub(crate) fn char_offset(src: &str, at: usize) -> usize {
    src.get(..at).map_or(at, |head| head.chars().count())
}

/// Read the literal opening at byte index `open`, resolving escapes.
///
/// `open` must index a quote character; the caller has already dispatched on it.
///
/// # Errors
///
/// Returns [`LiteralError`] when the literal is unterminated, or when a
/// backslash is followed by anything outside the escape set.
pub(crate) fn read_literal(src: &str, open: usize) -> Result<Literal, LiteralError> {
    let bytes = src.as_bytes();
    let quote = match bytes.get(open) {
        Some(&b) if is_quote(b) => b,
        // Unreachable through the lexer, which dispatches on the quote first.
        // Reported rather than asserted so a new caller gets a diagnosis instead
        // of a panic.
        _ => {
            return Err(LiteralError {
                at: char_offset(src, open),
                msg: "expected a quoted literal here".to_owned(),
            });
        },
    };

    let mut value = String::new();
    let mut i = open + 1;
    while let Some(&b) = bytes.get(i) {
        if b == quote {
            return Ok(Literal { value, end: i + 1 });
        }
        if b == b'\\' {
            let Some(&esc) = bytes.get(i + 1) else {
                return Err(unterminated(src, open));
            };
            match esc {
                b'\\' => value.push('\\'),
                b'\'' => value.push('\''),
                b'"' => value.push('"'),
                other => {
                    return Err(LiteralError {
                        at: char_offset(src, i),
                        msg: format!(
                            "unrecognized escape `\\{}` in a quoted literal; only `\\\\`, `\\'` \
                             and `\\\"` are escapes, so write `\\\\` for a literal backslash",
                            other as char
                        ),
                    });
                },
            }
            i += 2;
            continue;
        }
        // Copy one whole character, so a multi-byte character survives intact and
        // `i` never lands mid-character.
        let rest = src.get(i..).ok_or_else(|| LiteralError {
            at: char_offset(src, i),
            msg: "a quoted literal is cut mid-character".to_owned(),
        })?;
        let ch = rest.chars().next().ok_or_else(|| unterminated(src, open))?;
        value.push(ch);
        i += ch.len_utf8();
    }
    Err(unterminated(src, open))
}

/// The index just past the literal opening at `open`, without building its value.
///
/// For the scanners that only need to step over a literal to find a delimiter
/// outside one. It shares [`read_literal`]'s escape rule, which is the point: a
/// splitter that disagreed with the lexer about what closes a literal is how a
/// colon inside quoted text used to split a rule.
///
/// # Errors
///
/// As [`read_literal`].
pub(crate) fn skip_literal(src: &str, open: usize) -> Result<usize, LiteralError> {
    read_literal(src, open).map(|lit| lit.end)
}

/// Read `src` as exactly one literal, or as a bare word.
///
/// `Some(value)` when `src` is one complete literal. `None` when it does not open
/// with a quote, which is the bare form a stage argument may still use
/// (`enum(low, medium)`).
///
/// This replaces a `strip_prefix` / `strip_suffix` pair that stripped the
/// outermost matching quotes without reading a literal at all, so
/// `"a" == "b"` came back as `a" == "b`: two literals spliced through their
/// inner quotes. Requiring the literal to consume the whole string is what
/// refuses that.
///
/// # Errors
///
/// Returns [`LiteralError`] when `src` opens a literal that is unterminated,
/// carries a bad escape, or ends before the string does (trailing text).
pub(crate) fn read_whole_literal(src: &str) -> Result<Option<String>, LiteralError> {
    let trimmed = src.trim();
    let Some(&first) = trimmed.as_bytes().first() else {
        return Ok(None);
    };
    if !is_quote(first) {
        return Ok(None);
    }
    // Offsets are reported against `src`, so account for what `trim` removed.
    let lead = src.len() - src.trim_start().len();
    let lit = read_literal(trimmed, 0).map_err(|e| LiteralError {
        at: e.at + char_offset(src, lead),
        msg: e.msg,
    })?;
    if lit.end != trimmed.len() {
        return Err(LiteralError {
            at: char_offset(src, lead + lit.end),
            msg: "trailing text after a quoted literal; a value is one literal or one bare word"
                .to_owned(),
        });
    }
    Ok(Some(lit.value))
}

fn unterminated(src: &str, open: usize) -> LiteralError {
    LiteralError {
        at: char_offset(src, open),
        msg: "unterminated string literal".to_owned(),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code, per the crate's test conventions"
)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_literal_reads_its_content() {
        let lit = read_literal("'abc' rest", 0).expect("a closed literal");
        assert_eq!(lit.value, "abc");
        assert_eq!(lit.end, 5, "just past the closing quote");
    }

    #[test]
    fn the_other_quote_is_content() {
        assert_eq!(read_literal(r#""it's""#, 0).unwrap().value, "it's");
        assert_eq!(
            read_literal(r#"'say "hi"'"#, 0).unwrap().value,
            r#"say "hi""#
        );
    }

    #[test]
    fn each_escape_in_the_set_resolves_to_one_character() {
        for (src, want) in [
            (r"'it\'s'", "it's"),
            (r#""say \"hi\"""#, r#"say "hi""#),
            (r"'c:\\tmp'", r"c:\tmp"),
        ] {
            assert_eq!(read_literal(src, 0).expect("escapes resolve").value, want);
        }
    }

    #[test]
    fn an_escaped_quote_does_not_close_the_literal() {
        let lit = read_literal(r"'a\'b'", 0).expect("the escape protects the quote");
        assert_eq!(lit.value, "a'b");
        assert_eq!(lit.end, 6, "the literal runs to the last quote");
    }

    #[test]
    fn an_unrecognized_escape_names_the_character_and_the_fix() {
        let e = read_literal(r"'x\qy'", 0).expect_err("`\\q` is not an escape");
        assert!(e.msg.contains("`\\q`"), "{}", e.msg);
        assert!(
            e.msg.contains(r"\\"),
            "and says how to write one: {}",
            e.msg
        );
    }

    #[test]
    fn an_unterminated_literal_is_named_as_one() {
        for src in ["'abc", r#""abc"#, r"'abc\"] {
            let e = read_literal(src, 0).expect_err("no closing quote");
            assert!(e.msg.contains("unterminated"), "{}", e.msg);
        }
    }

    #[test]
    fn a_position_is_a_character_offset_not_a_byte_offset() {
        // `café` is five characters and six bytes, so a byte offset would report
        // one past the character a reader counts to.
        let e = read_literal(r"'café\q'", 0).expect_err("bad escape");
        assert_eq!(e.at, 5, "the backslash is the sixth character");
    }

    #[test]
    fn a_multibyte_character_survives_intact() {
        assert_eq!(read_literal("'café'", 0).unwrap().value, "café");
    }

    #[test]
    fn a_whole_literal_must_consume_the_whole_string() {
        assert_eq!(read_whole_literal("'abc'").unwrap().as_deref(), Some("abc"));
        assert_eq!(
            read_whole_literal("  'abc'  ").unwrap().as_deref(),
            Some("abc")
        );
        // The splice the old strip_prefix/strip_suffix pair allowed.
        let e = read_whole_literal(r#""a" == "b""#).expect_err("two literals are not one value");
        assert!(e.msg.contains("trailing text"), "{}", e.msg);
    }

    #[test]
    fn a_bare_word_is_not_a_literal_and_not_an_error() {
        assert_eq!(read_whole_literal("medium").unwrap(), None);
        assert_eq!(read_whole_literal("").unwrap(), None);
    }

    #[test]
    fn skipping_lands_where_reading_does() {
        let src = r"'a\'b' & c";
        assert_eq!(
            skip_literal(src, 0).unwrap(),
            read_literal(src, 0).unwrap().end
        );
    }
}
