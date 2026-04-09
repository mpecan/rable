//! Redirect-node S-expression formatting: regular redirects, heredocs,
//! and the inner segment walker that handles command-sub and ANSI-C content
//! inside redirect targets.

use std::fmt;

use crate::ast::{Node, NodeKind};

use super::ansi_c::process_ansi_c_content;
use super::word::{self, WordSegment};
use super::{normalize_cmdsub_content, write_escaped_word};

pub(super) fn write_redirect(
    f: &mut fmt::Formatter<'_>,
    op: &str,
    target: &Node,
    _fd: i32,
) -> fmt::Result {
    write!(f, "(redirect \"{op}\" ")?;
    if let NodeKind::Word { value, spans, .. } = &target.kind {
        // For fd dup operations (>&, <&), bare digit-only targets are unquoted
        let is_fd_op =
            op.starts_with(">&") || op.starts_with("<&") || op.ends_with("&-") || op.ends_with('&');
        if is_fd_op && !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
            write!(f, "{value})")
        } else if !spans.is_empty() {
            // Use span-based formatting — redirect targets use literal
            // output (no S-expression escaping of quotes)
            write!(f, "\"")?;
            let segments = word::segments_from_spans(value, spans);
            write_redirect_segments(f, &segments)?;
            write!(f, "\")")
        } else {
            // Synthetic nodes (fd strings) — output as-is
            write!(f, "\"{value}\")")
        }
    } else {
        write!(f, "{target})")
    }
}

pub(super) fn write_heredoc(
    f: &mut fmt::Formatter<'_>,
    content: &str,
    strip_tabs: bool,
) -> fmt::Result {
    let op = if strip_tabs { "<<-" } else { "<<" };
    // Here-doc content uses literal newlines, not \\n
    write!(f, "(redirect \"{op}\" \"{content}\")")
}

/// Formats word segments for redirect targets — uses literal output
/// (no S-expression escaping) for text, but processes ANSI-C escapes
/// and reformats command substitutions.
pub(super) fn write_redirect_segments(
    f: &mut fmt::Formatter<'_>,
    segments: &[WordSegment],
) -> fmt::Result {
    for seg in segments {
        match seg {
            WordSegment::Literal(text) => write!(f, "{text}")?,
            WordSegment::AnsiCQuote(raw) => {
                let chars: Vec<char> = raw.chars().collect();
                let mut pos = 0;
                let processed = process_ansi_c_content(&chars, &mut pos);
                write!(f, "'{processed}'")?;
            }
            WordSegment::LocaleString(content) => {
                // $"..." → "..." (content already includes "...")
                write!(f, "{content}")?;
            }
            WordSegment::CommandSubstitution(content) => {
                write!(f, "$(")?;
                if let Some(reformatted) = crate::format::reformat_bash(content) {
                    write!(f, "{reformatted}")?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    write!(f, "{normalized}")?;
                }
                write!(f, ")")?;
            }
            WordSegment::ProcessSubstitution(dir, content) => {
                write!(f, "{dir}(")?;
                if let Some(reformatted) = crate::format::reformat_bash(content) {
                    write!(f, "{reformatted}")?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    write!(f, "{normalized}")?;
                }
                write!(f, ")")?;
            }
            WordSegment::ParamExpansion(text)
            | WordSegment::SimpleVar(text)
            | WordSegment::BraceExpansion(text) => {
                write!(f, "{text}")?;
            }
            WordSegment::ArithmeticSub(inner) => {
                // Defensive: redirect target segments come from the sexp
                // filter which excludes `ArithmeticSub`. If it ever does
                // arrive here, reproduce the original `$((...))` text.
                write!(f, "$(({inner}))")?;
            }
        }
    }
    Ok(())
}

/// Writes a `ArithmeticCommand` node body with raw expression text wrapped
/// in the `(arith (word "…"))` form.
pub(super) fn write_arith_command(f: &mut fmt::Formatter<'_>, raw_content: &str) -> fmt::Result {
    write!(f, "(arith (word \"")?;
    write_escaped_word(f, raw_content)?;
    write!(f, "\"))")
}
