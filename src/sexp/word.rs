//! Word value processing: segment-based state machine.
//!
//! Splits a word's raw value into typed segments, then formats each
//! segment independently. This replaces the monolithic character-by-character
//! `write_word_value` with a testable two-phase pipeline.

use std::fmt;

use crate::ast::NodeKind;
use crate::format;
use crate::lexer::word_builder::WordSpan;

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
    /// Arithmetic substitution `$((...))` — content is the inner expression,
    /// i.e. the text between `$((` and `))`. Only emitted by
    /// [`segments_with_params`] (the `Word.parts` path); the sexp path
    /// leaves `$((...))` as literal text for backwards compatibility.
    ArithmeticSub(String),
    /// Command substitution `$(...)` — content is between the parens.
    CommandSubstitution(String),
    /// Process substitution `>(...)` or `<(...)` — direction + content.
    ProcessSubstitution(char, String),
    /// Parameter expansion `${...}` — raw text includes `${` and `}`.
    ParamExpansion(String),
    /// Simple variable `$var` — raw text includes `$` prefix.
    SimpleVar(String),
    /// Brace expansion `{a,b,c}` or `{1..10}` — raw text includes braces.
    BraceExpansion(String),
}

/// Formats word segments into S-expression output.
pub fn write_word_segments(f: &mut fmt::Formatter<'_>, segments: &[WordSegment]) -> fmt::Result {
    for seg in segments {
        write_one_segment(f, seg)?;
    }
    Ok(())
}

fn write_one_segment(f: &mut fmt::Formatter<'_>, seg: &WordSegment) -> fmt::Result {
    match seg {
        WordSegment::Literal(text)
        | WordSegment::ParamExpansion(text)
        | WordSegment::SimpleVar(text)
        | WordSegment::BraceExpansion(text) => {
            for ch in text.chars() {
                write_escaped_char(f, ch)?;
            }
            Ok(())
        }
        WordSegment::AnsiCQuote(raw_content) => {
            let chars: Vec<char> = raw_content.chars().collect();
            let mut pos = 0;
            let processed = process_ansi_c_content(&chars, &mut pos);
            write_escaped_word(f, "'")?;
            write_escaped_word(f, &processed)?;
            write_escaped_word(f, "'")
        }
        WordSegment::LocaleString(content) => {
            // content includes "..." delimiters
            for ch in content.chars() {
                write_escaped_char(f, ch)?;
            }
            Ok(())
        }
        WordSegment::ArithmeticSub(inner) => {
            // Defensive: the sexp filter excludes ArithmeticSub so this
            // branch is unreachable under normal use. If it ever does run,
            // reproduce the original `$((...))` text so output stays sane.
            write!(f, "$((")?;
            for ch in inner.chars() {
                write_escaped_char(f, ch)?;
            }
            write!(f, "))")
        }
        WordSegment::CommandSubstitution(content) => write_cmdsub_segment(f, content),
        WordSegment::ProcessSubstitution(direction, content) => {
            write_procsub_segment(f, *direction, content)
        }
    }
}

fn write_cmdsub_segment(f: &mut fmt::Formatter<'_>, content: &str) -> fmt::Result {
    write!(f, "$(")?;
    if let Some(reformatted) = format::reformat_bash(content) {
        // Add space if content starts with ( to avoid $((
        if reformatted.starts_with('(') {
            write!(f, " ")?;
        }
        write_escaped_word(f, &reformatted)?;
    } else {
        let normalized = normalize_cmdsub_content(content);
        if normalized.starts_with('(') {
            write!(f, " ")?;
        }
        write_escaped_word(f, &normalized)?;
    }
    write!(f, ")")
}

fn write_procsub_segment(
    f: &mut fmt::Formatter<'_>,
    direction: char,
    content: &str,
) -> fmt::Result {
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
    write!(f, ")")
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
    build_segments(value, spans, is_sexp_relevant)
}

/// Like `segments_from_spans` but also decomposes parameter expansions
/// and simple variables into their own segments (for `Word.parts`).
pub fn segments_with_params(
    value: &str,
    spans: &[crate::lexer::word_builder::WordSpan],
) -> Vec<WordSegment> {
    build_segments(value, spans, is_decomposable)
}

fn build_segments(
    value: &str,
    spans: &[crate::lexer::word_builder::WordSpan],
    filter: fn(&crate::lexer::word_builder::WordSpanKind) -> bool,
) -> Vec<WordSegment> {
    let top_level = collect_filtered_spans(spans, filter);
    let mut segments = Vec::new();
    let mut pos = 0;
    for span in &top_level {
        if span.start > pos
            && let Some(text) = value.get(pos..span.start)
        {
            segments.push(WordSegment::Literal(text.to_string()));
        }
        span_to_segment(&mut segments, value, span);
        pos = span.end;
    }
    if pos < value.len()
        && let Some(text) = value.get(pos..)
    {
        segments.push(WordSegment::Literal(text.to_string()));
    }
    segments
}

/// Converts a single span into the appropriate `WordSegment` and appends it.
fn span_to_segment(
    segments: &mut Vec<WordSegment>,
    value: &str,
    span: &crate::lexer::word_builder::WordSpan,
) {
    use crate::lexer::word_builder::{QuotingContext, WordSpanKind};
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
            push_ansi_c_span(segments, value, span);
        }
        WordSpanKind::ArithmeticSub => {
            // `$(( ... ))`: skip `$((` (3 bytes) at the start and `))`
            // (2 bytes) at the end to extract the inner expression text.
            if span.end >= span.start + 5
                && let Some(inner) = value.get(span.start + 3..span.end - 2)
            {
                segments.push(WordSegment::ArithmeticSub(inner.to_string()));
            }
        }
        WordSpanKind::LocaleString => {
            match span.context {
                QuotingContext::DoubleQuote => {
                    // $"..." inside "..." is literal (not a locale string)
                    if let Some(text) = value.get(span.start..span.end) {
                        push_literal(segments, text);
                    }
                }
                _ => {
                    if let Some(c) = value.get(span.start + 1..span.end) {
                        segments.push(WordSegment::LocaleString(c.to_string()));
                    }
                }
            }
        }
        WordSpanKind::ParamExpansion | WordSpanKind::SimpleVar | WordSpanKind::BraceExpansion => {
            if let Some(text) = value.get(span.start..span.end) {
                let seg = match &span.kind {
                    WordSpanKind::ParamExpansion => WordSegment::ParamExpansion,
                    WordSpanKind::SimpleVar => WordSegment::SimpleVar,
                    WordSpanKind::BraceExpansion => WordSegment::BraceExpansion,
                    _ => unreachable!(),
                };
                segments.push(seg(text.to_string()));
            }
        }
        WordSpanKind::Backtick => {
            if span.end > span.start + 1
                && let Some(c) = value.get(span.start + 1..span.end - 1)
            {
                segments.push(WordSegment::CommandSubstitution(c.to_string()));
            }
        }
        _ => {} // filtered out by span filter
    }
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

/// Sexp-relevant spans plus parameter expansions, simple variables,
/// brace expansions, and arithmetic substitutions.
///
/// `ArithmeticSub` is included here but intentionally NOT in
/// [`is_sexp_relevant`] — this keeps `$((...))` as literal text in the
/// S-expression word output (preserving the Parable corpus expectations)
/// while still exposing a typed `ArithmeticExpansion` node in `Word.parts`.
const fn is_decomposable(kind: &crate::lexer::word_builder::WordSpanKind) -> bool {
    use crate::lexer::word_builder::WordSpanKind;
    if is_sexp_relevant(kind) {
        return true;
    }
    matches!(
        kind,
        WordSpanKind::ParamExpansion
            | WordSpanKind::SimpleVar
            | WordSpanKind::BraceExpansion
            | WordSpanKind::ArithmeticSub
            | WordSpanKind::Backtick
    )
}

/// Collects top-level spans matching `filter`, sorted by start offset.
/// A span is top-level if no other matching span fully contains it.
fn collect_filtered_spans(
    spans: &[crate::lexer::word_builder::WordSpan],
    filter: fn(&crate::lexer::word_builder::WordSpanKind) -> bool,
) -> Vec<&crate::lexer::word_builder::WordSpan> {
    let mut relevant: Vec<_> = spans.iter().filter(|s| filter(&s.kind)).collect();
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

/// Formats word-like variants: `Word`, `WordLiteral`, and `CondTerm`.
pub(super) fn fmt_word_like(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::Word { value, spans, .. } => fmt_word(f, value, spans),
        NodeKind::WordLiteral { value } => write!(f, "{value}"),
        NodeKind::CondTerm { value, spans } => fmt_cond_term(f, value, spans),
        _ => unreachable!("fmt_word_like called with non-word-like variant"),
    }
}

fn fmt_word(f: &mut fmt::Formatter<'_>, value: &str, spans: &[WordSpan]) -> fmt::Result {
    write!(f, "(word \"")?;
    if spans.is_empty() {
        // Synthetic word (no lexer token) — escape directly
        write_escaped_word(f, value)?;
    } else {
        let segments = segments_from_spans(value, spans);
        write_word_segments(f, &segments)?;
    }
    write!(f, "\")")
}

fn fmt_cond_term(f: &mut fmt::Formatter<'_>, value: &str, spans: &[WordSpan]) -> fmt::Result {
    // Strip `$"` locale prefix
    let val = if value.starts_with("$\"") {
        &value[1..]
    } else {
        value
    };
    if spans.is_empty() {
        return write!(f, "(cond-term \"{val}\")");
    }
    let segments = segments_from_spans(val, spans);
    if segments
        .iter()
        .all(|s| matches!(s, WordSegment::Literal(_)))
    {
        // No expansions needing processing
        return write!(f, "(cond-term \"{val}\")");
    }
    // CondTerm uses raw output (like redirect targets)
    write!(f, "(cond-term \"")?;
    super::redirects::write_redirect_segments(f, &segments)?;
    write!(f, "\")")
}
