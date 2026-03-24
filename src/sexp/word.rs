//! Word value processing: segment-based state machine.
//!
//! Splits a word's raw value into typed segments, then formats each
//! segment independently. This replaces the monolithic character-by-character
//! `write_word_value` with a testable two-phase pipeline.

use std::fmt;

use crate::format;

use super::{
    ansi_c::process_ansi_c_content, extract_paren_content, normalize_cmdsub_content,
    write_escaped_char, write_escaped_word,
};

/// A typed segment within a word token's raw value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordSegment {
    /// Regular text — escaped when formatted.
    Literal(String),
    /// ANSI-C quoted string `$'...'` — content is raw (with `\` escapes).
    AnsiCQuote(String),
    /// Locale string `$"..."` — content includes the `"..."` delimiters.
    LocaleString(String),
    /// Command substitution `$(...)` — content is between the parens.
    CommandSubstitution(String),
}

/// Parses a word's raw value into typed segments.
pub fn parse_word_segments(value: &str) -> Vec<WordSegment> {
    let chars: Vec<char> = value.chars().collect();
    let mut segments = Vec::new();
    let mut i = 0;
    let mut literal = String::new();

    while i < chars.len() {
        let prev_backslash = i > 0 && chars[i - 1] == '\\';

        if chars[i] == '$' && !prev_backslash && i + 1 < chars.len() {
            match chars[i + 1] {
                '\'' => {
                    flush_literal(&mut literal, &mut segments);
                    i += 2; // skip $'
                    let content = extract_ansi_c_content(&chars, &mut i);
                    segments.push(WordSegment::AnsiCQuote(content));
                }
                '"' => {
                    flush_literal(&mut literal, &mut segments);
                    i += 1; // skip $, keep "
                    let content = extract_locale_content(&chars, &mut i);
                    segments.push(WordSegment::LocaleString(content));
                }
                '(' => {
                    // $(( )) arithmetic — treat as literal (don't reformat)
                    if i + 2 < chars.len() && chars[i + 2] == '(' {
                        literal.push(chars[i]);
                        i += 1;
                        continue;
                    }
                    flush_literal(&mut literal, &mut segments);
                    i += 2; // skip $(
                    let content = extract_paren_content(&chars, &mut i);
                    segments.push(WordSegment::CommandSubstitution(content));
                }
                _ => {
                    literal.push(chars[i]);
                    i += 1;
                }
            }
        } else {
            literal.push(chars[i]);
            i += 1;
        }
    }

    flush_literal(&mut literal, &mut segments);
    segments
}

/// Formats word segments into S-expression output.
pub fn write_word_segments(f: &mut fmt::Formatter<'_>, segments: &[WordSegment]) -> fmt::Result {
    for seg in segments {
        match seg {
            WordSegment::Literal(text) => {
                for ch in text.chars() {
                    write_escaped_char(f, ch)?;
                }
            }
            WordSegment::AnsiCQuote(raw_content) => {
                let chars: Vec<char> = raw_content.chars().collect();
                let mut pos = 0;
                let processed = process_ansi_c_content(&chars, &mut pos);
                write_escaped_word(f, "'")?;
                write_escaped_word(f, &processed)?;
                write_escaped_word(f, "'")?;
            }
            WordSegment::LocaleString(content) => {
                // content includes "..." delimiters
                for ch in content.chars() {
                    write_escaped_char(f, ch)?;
                }
            }
            WordSegment::CommandSubstitution(content) => {
                write!(f, "$(")?;
                if let Some(reformatted) = format::reformat_bash(content) {
                    write_escaped_word(f, &reformatted)?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    write_escaped_word(f, &normalized)?;
                }
                write!(f, ")")?;
            }
        }
    }
    Ok(())
}

/// The public entry point — replaces the old monolithic `write_word_value`.
pub fn write_word_value(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    let segments = parse_word_segments(value);
    write_word_segments(f, &segments)
}

// -- helpers --

fn flush_literal(literal: &mut String, segments: &mut Vec<WordSegment>) {
    if !literal.is_empty() {
        segments.push(WordSegment::Literal(std::mem::take(literal)));
    }
}

/// Extracts ANSI-C content between `'...'` (after `$'` was consumed).
fn extract_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
    let mut content = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '\'' {
            *pos += 1; // skip closing '
            return content;
        }
        if c == '\\' && *pos + 1 < chars.len() {
            content.push('\\');
            *pos += 1;
            content.push(chars[*pos]);
        } else {
            content.push(c);
        }
        *pos += 1;
    }
    content
}

/// Extracts locale string content `"..."` (after `$` was skipped, starting at `"`).
fn extract_locale_content(chars: &[char], pos: &mut usize) -> String {
    let mut content = String::new();
    // Read opening " and content until closing "
    let start = *pos;
    while *pos < chars.len() {
        content.push(chars[*pos]);
        if chars[*pos] == '"' && *pos > start {
            *pos += 1;
            return content;
        }
        *pos += 1;
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_word() {
        let segs = parse_word_segments("hello");
        assert_eq!(segs, vec![WordSegment::Literal("hello".into())]);
    }

    #[test]
    fn ansi_c_quote() {
        let segs = parse_word_segments("$'foo\\nbar'");
        assert_eq!(segs, vec![WordSegment::AnsiCQuote("foo\\nbar".into())]);
    }

    #[test]
    fn locale_string() {
        let segs = parse_word_segments("$\"hello\"");
        assert_eq!(segs, vec![WordSegment::LocaleString("\"hello\"".into())]);
    }

    #[test]
    fn command_substitution() {
        let segs = parse_word_segments("$(date)");
        assert_eq!(segs, vec![WordSegment::CommandSubstitution("date".into())]);
    }

    #[test]
    fn arithmetic_stays_literal() {
        let segs = parse_word_segments("$((1+2))");
        assert_eq!(segs, vec![WordSegment::Literal("$((1+2))".into())]);
    }

    #[test]
    fn mixed_segments() {
        let segs = parse_word_segments("hello$(world)$'foo'");
        assert_eq!(
            segs,
            vec![
                WordSegment::Literal("hello".into()),
                WordSegment::CommandSubstitution("world".into()),
                WordSegment::AnsiCQuote("foo".into()),
            ]
        );
    }

    #[test]
    fn escaped_dollar_not_special() {
        let segs = parse_word_segments("\\$'not-ansi'");
        assert_eq!(segs, vec![WordSegment::Literal("\\$'not-ansi'".into())]);
    }

    #[test]
    fn assignment_with_cmdsub() {
        let segs = parse_word_segments("x=$(cmd)");
        assert_eq!(
            segs,
            vec![
                WordSegment::Literal("x=".into()),
                WordSegment::CommandSubstitution("cmd".into()),
            ]
        );
    }
}
