//! Redirect-node S-expression formatting: regular redirects, heredocs,
//! and the inner segment walker that handles command-sub and ANSI-C content
//! inside redirect targets.

use std::fmt;

use crate::ast::{Node, NodeKind};

use super::ansi_c::process_ansi_c_content;
use super::word::{self, WordSegment};
use super::write_escaped_word;

pub(super) fn write_redirect(
    f: &mut fmt::Formatter<'_>,
    op: &str,
    target: &Node,
    _fd: i32,
) -> fmt::Result {
    write!(f, "(redirect \"{op}\" ")?;
    if let NodeKind::Word { value, spans, .. } = &target.kind {
        let is_fd_op =
            op.starts_with(">&") || op.starts_with("<&") || op.ends_with("&-") || op.ends_with('&');
        // A move (`3<&0-`) dumps like a plain dup, without the `-`.
        let value = if is_fd_op {
            value.strip_suffix('-').unwrap_or(value)
        } else {
            value
        };
        // Numeric fd targets are unquoted, as the number bash parsed (`02` → 2).
        if let Some(n) = is_fd_op.then(|| fd_number(value)).flatten() {
            write!(f, "{n})")
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

/// The fd a digit-only dup target names, if it fits bash's `int`.
pub fn fd_number(value: &str) -> Option<i32> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
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
                write!(f, "{}", crate::format::reformat_or_normalize(content))?;
                write!(f, ")")?;
            }
            WordSegment::FunSub { pipe, content } => {
                write!(f, "{}", crate::format::funsub_text(*pipe, content))?;
            }
            WordSegment::ProcessSubstitution(dir, content) => {
                write!(f, "{dir}(")?;
                write!(f, "{}", crate::format::reformat_or_normalize(content))?;
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

/// Formats redirect-like variants: `Redirect`, `HereDoc`, and
/// `ArithmeticCommand`.
pub(super) fn fmt_redirect_like(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::Redirect { op, target, fd, .. } => write_redirect(f, op, target, *fd),
        NodeKind::HereDoc {
            content,
            strip_tabs,
            ..
        } => write_heredoc(f, content, *strip_tabs),
        NodeKind::ArithmeticCommand {
            redirects,
            raw_content,
            ..
        } => {
            write_arith_command(f, raw_content)?;
            super::write_redirects(f, redirects)
        }
        _ => unreachable!("fmt_redirect_like called with non-redirect-like variant"),
    }
}
