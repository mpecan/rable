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
    /// Process substitution `>(...)` or `<(...)` — direction + content.
    ProcessSubstitution(char, String),
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
        } else if (chars[i] == '>' || chars[i] == '<')
            && !prev_backslash
            && i + 1 < chars.len()
            && chars[i + 1] == '('
        {
            let direction = chars[i];
            flush_literal(&mut literal, &mut segments);
            i += 2; // skip >( or <(
            let content = extract_paren_content(&chars, &mut i);
            segments.push(WordSegment::ProcessSubstitution(direction, content));
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
                    // Add space if content starts with ( to avoid $((
                    if reformatted.starts_with('(') {
                        write!(f, " ")?;
                    }
                    write_escaped_word(f, &reformatted)?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    // Add space if content starts with ( to avoid $((
                    if normalized.starts_with('(') {
                        write!(f, " ")?;
                    }
                    write_escaped_word(f, &normalized)?;
                }
                write!(f, ")")?;
            }
            WordSegment::ProcessSubstitution(direction, content) => {
                write!(f, "{direction}(")?;
                let trimmed = content.trim();
                // Don't reformat content that starts with ( (subshell inside procsub)
                if trimmed.starts_with('(') {
                    write_escaped_word(f, trimmed)?;
                } else if let Some(reformatted) = format::reformat_bash(content) {
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
    // Check for array assignment pattern: name=(...) — normalize content
    if let Some(normalized) = try_normalize_array(value) {
        // Process word segments so $() inside arrays gets reformatted
        let segments = parse_word_segments(&normalized);
        return write_word_segments(f, &segments);
    }
    let segments = parse_word_segments(value);
    write_word_segments(f, &segments)
}

/// Detects `name=(...)` array patterns and normalizes whitespace/comments.
fn try_normalize_array(value: &str) -> Option<String> {
    // Find the `=(` pattern
    let eq_paren = value.find("=(")?;
    let prefix = &value[..=eq_paren]; // includes `=`
    let rest = &value[eq_paren + 1..]; // starts with `(`

    // Must start with `(` and end with `)`
    if !rest.starts_with('(') || !rest.ends_with(')') {
        return None;
    }

    // Only normalize if content has whitespace/comments that need it
    let inner = &rest[1..rest.len() - 1];
    if !inner.contains('\n') && !inner.contains('\t') && !inner.contains('#') {
        return None;
    }

    let normalized = normalize_array_content(inner);
    // Process command substitutions inside the array
    let result = format!("{prefix}({normalized})");
    // Now process word segments on the result to handle $() inside arrays
    Some(result)
}

/// Normalizes array content: collapses whitespace, strips comments.
#[allow(clippy::too_many_lines)]
fn normalize_array_content(inner: &str) -> String {
    let mut elements = Vec::new();
    let chars: Vec<char> = inner.chars().collect();
    let mut i = 0;
    let mut current = String::new();

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' | '\r' => {
                if !current.is_empty() {
                    elements.push(std::mem::take(&mut current));
                }
                i += 1;
            }
            '#' => {
                // Skip comment until end of line
                if !current.is_empty() {
                    elements.push(std::mem::take(&mut current));
                }
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\'' => {
                // Single-quoted string
                current.push(chars[i]);
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    current.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    current.push(chars[i]);
                    i += 1;
                }
            }
            '"' => {
                // Double-quoted string
                current.push(chars[i]);
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    current.push(chars[i]);
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        current.push(chars[i]);
                    }
                    i += 1;
                }
                if i < chars.len() {
                    current.push(chars[i]);
                    i += 1;
                }
            }
            '$' if i + 1 < chars.len() && chars[i + 1] == '(' => {
                // Command substitution — read matched parens
                current.push(chars[i]);
                current.push(chars[i + 1]);
                i += 2;
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '(' {
                        depth += 1;
                    } else if chars[i] == ')' {
                        depth -= 1;
                    }
                    current.push(chars[i]);
                    i += 1;
                }
            }
            '$' if i + 1 < chars.len() && chars[i + 1] == '{' => {
                // Parameter expansion — read matched braces
                current.push(chars[i]);
                current.push(chars[i + 1]);
                i += 2;
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '{' {
                        depth += 1;
                    } else if chars[i] == '}' {
                        depth -= 1;
                    }
                    current.push(chars[i]);
                    i += 1;
                }
            }
            _ => {
                current.push(chars[i]);
                i += 1;
            }
        }
    }
    if !current.is_empty() {
        elements.push(current);
    }

    elements.join(" ")
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
