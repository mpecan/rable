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
#[allow(clippy::too_many_lines)]
pub fn parse_word_segments(value: &str) -> Vec<WordSegment> {
    let chars: Vec<char> = value.chars().collect();
    let mut segments = Vec::new();
    let mut i = 0;
    let mut literal = String::new();
    let mut brace_depth = 0usize; // Track ${...} nesting
    let mut in_double_quote = false; // Track "..." context

    while i < chars.len() {
        let prev_backslash = crate::context::is_backslash_escaped(&chars, i);

        // Track double-quote context (not inside ${...} where quotes nest differently)
        if chars[i] == '"' && !prev_backslash && brace_depth == 0 {
            in_double_quote = !in_double_quote;
            literal.push(chars[i]);
            i += 1;
            continue;
        }

        // Track closing braces for ${...}
        if chars[i] == '}' && brace_depth > 0 {
            brace_depth -= 1;
            literal.push(chars[i]);
            i += 1;
            continue;
        }

        // Single quotes are opaque — nothing is special inside '...'
        if chars[i] == '\'' && !prev_backslash && !in_double_quote {
            literal.push(chars[i]);
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                literal.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                literal.push(chars[i]); // closing '
                i += 1;
            }
            continue;
        }

        // Backticks are opaque — don't process $'...' or $() inside them
        if chars[i] == '`' && !prev_backslash {
            literal.push(chars[i]);
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    literal.push(chars[i]);
                    i += 1;
                }
                literal.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                literal.push(chars[i]); // closing `
                i += 1;
            }
            continue;
        }

        if chars[i] == '$' && !prev_backslash && i + 1 < chars.len() {
            match chars[i + 1] {
                '\'' => {
                    if in_double_quote && brace_depth == 0 {
                        // $'...' is NOT special inside double quotes — literal
                        // (but IS special inside ${...} even when double-quoted)
                        literal.push(chars[i]);
                        i += 1;
                        continue;
                    }
                    if brace_depth > 0 {
                        // ANSI-C inside ${...} — process escapes, output without quotes
                        i += 2; // skip $'
                        let content = extract_ansi_c_content(&chars, &mut i);
                        let ac_chars: Vec<char> = content.chars().collect();
                        let mut pos = 0;
                        let processed = super::process_ansi_c_content(&ac_chars, &mut pos);
                        literal.push_str(&processed);
                    } else {
                        flush_literal(&mut literal, &mut segments);
                        i += 2; // skip $'
                        let content = extract_ansi_c_content(&chars, &mut i);
                        segments.push(WordSegment::AnsiCQuote(content));
                    }
                }
                '"' => {
                    if in_double_quote {
                        // $" inside double quotes is literal (not a locale string)
                        literal.push(chars[i]);
                        i += 1;
                    } else {
                        flush_literal(&mut literal, &mut segments);
                        i += 1; // skip $, keep "
                        let content = extract_locale_content(&chars, &mut i);
                        segments.push(WordSegment::LocaleString(content));
                    }
                }
                '{' => {
                    literal.push(chars[i]);
                    literal.push(chars[i + 1]);
                    i += 2;
                    brace_depth += 1;
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
            && !in_double_quote
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

/// The public entry point — formats a word value via segment parsing.
/// Used as fallback when spans are not available (synthetic word nodes).
pub fn write_word_value(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    let segments = parse_word_segments(value);
    write_word_segments(f, &segments)
}

/// Converts lexer spans to segments without re-parsing the value.
///
/// Filters to top-level sexp-relevant spans (not contained within
/// another sexp-relevant span) and treats everything else as literal.
/// Uses span context to handle context-sensitive constructs like
/// `$'...'` inside `${...}` vs inside `"..."`.
pub fn segments_from_spans(
    value: &str,
    spans: &[crate::lexer::word_builder::WordSpan],
) -> Vec<WordSegment> {
    use crate::lexer::word_builder::{QuotingContext, WordSpanKind};
    let top_level = collect_top_level_sexp_spans(spans);
    let mut segments = Vec::new();
    let mut pos = 0;
    for span in &top_level {
        if span.start > pos
            && let Some(text) = value.get(pos..span.start)
        {
            segments.push(WordSegment::Literal(text.to_string()));
        }
        match &span.kind {
            WordSpanKind::CommandSub => {
                if let Some(c) = value.get(span.start + 2..span.end - 1) {
                    segments.push(WordSegment::CommandSubstitution(c.to_string()));
                }
            }
            WordSpanKind::ProcessSub(dir) => {
                if let Some(c) = value.get(span.start + 2..span.end - 1) {
                    segments.push(WordSegment::ProcessSubstitution(*dir, c.to_string()));
                }
            }
            WordSpanKind::AnsiCQuote => {
                push_ansi_c_span(&mut segments, value, span);
            }
            WordSpanKind::LocaleString => {
                match span.context {
                    QuotingContext::DoubleQuote => {
                        // $"..." inside "..." is literal (not a locale string)
                        if let Some(text) = value.get(span.start..span.end) {
                            push_literal(&mut segments, text);
                        }
                    }
                    _ => {
                        if let Some(c) = value.get(span.start + 1..span.end) {
                            segments.push(WordSegment::LocaleString(c.to_string()));
                        }
                    }
                }
            }
            _ => {} // filtered out by is_sexp_relevant
        }
        pos = span.end;
    }
    if pos < value.len()
        && let Some(text) = value.get(pos..)
    {
        segments.push(WordSegment::Literal(text.to_string()));
    }
    segments
}

/// Handles `$'...'` spans with context-sensitive behavior:
/// - Inside `"..."`: not special, treat as literal `$` + `'...'`
/// - Inside `${...}`: process escapes, absorb into literal (no quotes)
/// - Otherwise: normal `AnsiCQuote` segment
fn push_ansi_c_span(
    segments: &mut Vec<WordSegment>,
    value: &str,
    span: &crate::lexer::word_builder::WordSpan,
) {
    use crate::lexer::word_builder::QuotingContext;
    match span.context {
        QuotingContext::DoubleQuote => {
            // $'...' is NOT special inside "..." — treat as literal
            if let Some(text) = value.get(span.start..span.end) {
                push_literal(segments, text);
            }
        }
        QuotingContext::ParamExpansion => {
            // $'...' inside ${...} — process escapes, no quotes in output
            if let Some(raw) = value.get(span.start + 2..span.end - 1) {
                let chars: Vec<char> = raw.chars().collect();
                let mut pos = 0;
                let processed = super::process_ansi_c_content(&chars, &mut pos);
                push_literal(segments, &processed);
            }
        }
        _ => {
            // Top-level or inside $() / `` — normal ANSI-C quote
            if let Some(c) = value.get(span.start + 2..span.end - 1) {
                segments.push(WordSegment::AnsiCQuote(c.to_string()));
            }
        }
    }
}

/// Appends text to the last `Literal` segment if possible, or creates a new one.
fn push_literal(segments: &mut Vec<WordSegment>, text: &str) {
    if let Some(WordSegment::Literal(last)) = segments.last_mut() {
        last.push_str(text);
    } else {
        segments.push(WordSegment::Literal(text.to_string()));
    }
}

/// Returns true if the word value requires the value-based formatting
/// path. Currently always returns false — all cases are handled by
/// the span-based path or by lex-time normalization.
pub const fn needs_value_path(_value: &str) -> bool {
    false
}

const fn is_sexp_relevant(kind: &crate::lexer::word_builder::WordSpanKind) -> bool {
    use crate::lexer::word_builder::WordSpanKind;
    matches!(
        kind,
        WordSpanKind::CommandSub
            | WordSpanKind::AnsiCQuote
            | WordSpanKind::LocaleString
            | WordSpanKind::ProcessSub(_)
    )
}

/// Collects sexp-relevant spans that are not nested inside another
/// sexp-relevant span. Sorted by start offset.
fn collect_top_level_sexp_spans(
    spans: &[crate::lexer::word_builder::WordSpan],
) -> Vec<&crate::lexer::word_builder::WordSpan> {
    let relevant: Vec<_> = spans.iter().filter(|s| is_sexp_relevant(&s.kind)).collect();
    relevant
        .iter()
        .filter(|s| {
            // A span is top-level if no other relevant span contains it
            !relevant.iter().any(|outer| {
                outer.start <= s.start && outer.end >= s.end && !std::ptr::eq(*outer, **s)
            })
        })
        .copied()
        .collect()
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
