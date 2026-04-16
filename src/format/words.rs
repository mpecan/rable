//! Word-value reconstruction from lexer spans for the reformatter.
//!
//! Distinct from `sexp::word::write_word_segments` which writes the same
//! segments into S-expression output with escaping; this path rebuilds
//! plain bash source so a command substitution body can be re-parsed and
//! laid out canonically.

use crate::lexer::word_builder::WordSpan;
use crate::sexp::word::{WordSegment, segments_from_spans};
use crate::sexp::{normalize_cmdsub_content, process_ansi_c_content};

use super::reformat_bash;

/// Process a word value for canonical bash output using span-based
/// segment extraction. Returns a fresh `String` rather than writing to
/// a `Formatter` because callers embed the result into larger strings
/// (e.g. next to a redirect operator) via `Formatter::write_str`.
pub(super) fn process_word_value(value: &str, spans: &[WordSpan]) -> String {
    let segments = segments_from_spans(value, spans);
    let mut result = String::with_capacity(value.len());

    for seg in &segments {
        match seg {
            WordSegment::Literal(text) => result.push_str(text),
            WordSegment::AnsiCQuote(raw_content) => {
                let chars: Vec<char> = raw_content.chars().collect();
                let mut pos = 0;
                let processed = process_ansi_c_content(&chars, &mut pos);
                result.push('\'');
                result.push_str(&processed);
                result.push('\'');
            }
            WordSegment::LocaleString(content) => {
                // $"..." → "..." (content includes the "..." delimiters)
                result.push_str(content);
            }
            WordSegment::CommandSubstitution(content) => {
                result.push_str("$(");
                if let Some(reformatted) = reformat_bash(content) {
                    result.push_str(&reformatted);
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    result.push_str(&normalized);
                }
                result.push(')');
            }
            WordSegment::ProcessSubstitution(direction, content) => {
                result.push(*direction);
                result.push('(');
                if let Some(reformatted) = reformat_bash(content) {
                    result.push_str(&reformatted);
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    result.push_str(&normalized);
                }
                result.push(')');
            }
            WordSegment::ParamExpansion(text)
            | WordSegment::SimpleVar(text)
            | WordSegment::BraceExpansion(text) => {
                result.push_str(text);
            }
            WordSegment::ArithmeticSub(inner) => {
                // Defensive: `segments_from_spans` uses the sexp filter
                // which excludes `ArithmeticSub`, so this arm is
                // unreachable in practice. Preserve the original text if
                // it ever does fire.
                result.push_str("$((");
                result.push_str(inner);
                result.push_str("))");
            }
        }
    }
    result
}
