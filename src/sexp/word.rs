//! Word value processing: segment-based state machine.
//!
//! Splits a word's raw value into typed segments, then formats each
//! segment independently. This replaces the monolithic character-by-character
//! `write_word_value` with a testable two-phase pipeline.

use std::fmt;

use crate::format;

use super::{
    ansi_c::process_ansi_c_content, normalize_cmdsub_content, write_escaped_char,
    write_escaped_word,
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

/// Collects top-level sexp-relevant spans sorted by start offset.
/// A span is top-level if no other sexp-relevant span fully contains it.
fn collect_top_level_sexp_spans(
    spans: &[crate::lexer::word_builder::WordSpan],
) -> Vec<&crate::lexer::word_builder::WordSpan> {
    // Collect sexp-relevant spans sorted by start offset
    let mut relevant: Vec<_> = spans.iter().filter(|s| is_sexp_relevant(&s.kind)).collect();
    relevant.sort_by_key(|s| s.start);
    // Single pass: skip spans nested inside a previously collected one
    let mut result = Vec::new();
    let mut covered_until: usize = 0;
    for span in relevant {
        if span.start >= covered_until {
            covered_until = span.end;
            result.push(span);
        }
    }
    result
}
