//! Decomposes word values into structured AST parts.
//!
//! Reuses the segment parser from `sexp::word` to split a word's raw
//! value into typed segments, then maps each segment to an AST `Node`.
//!
//! # Layout
//!
//! | file                  | responsibility                                 |
//! |-----------------------|------------------------------------------------|
//! | `ansi_c.rs`           | `$'…'` escape decoding, locale-string trimming |
//! | `param_expansion.rs`  | `$var` / `${…}` parsing including subscripts   |
//! | `substitution.rs`     | `$(…)`, `<(…)`, `$((…))` with depth guarding   |
//! | `tests.rs`            | integration tests for all segment kinds        |

mod ansi_c;
mod param_expansion;
mod substitution;

use std::cell::Cell;

use crate::ast::{Node, NodeKind};
use crate::lexer::word_builder::WordSpan;
use crate::sexp::word::{WordSegment, segments_with_params};

thread_local! {
    static DECOMPOSE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard to prevent infinite recursion when decomposing nested
/// command/process substitutions.
///
/// Depth limit of 2 matches `format::DepthGuard` — allows `$(a $(b))`
/// but stops before unbounded nesting. The counter is separate from
/// the format module's counter since decomposition and formatting
/// are independent recursion paths.
pub(super) struct DepthGuard;

impl DepthGuard {
    pub(super) fn enter() -> Option<Self> {
        DECOMPOSE_DEPTH.with(|d| {
            let v = d.get();
            if v >= 2 {
                return None;
            }
            d.set(v + 1);
            Some(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DECOMPOSE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Shared `WordLiteral` constructor used for synthetic values and error
/// recovery across submodules.
pub(super) fn literal_fallback(value: &str) -> Node {
    Node::empty(NodeKind::WordLiteral {
        value: value.to_string(),
    })
}

/// Decomposes a word using lexer spans — the primary path for all
/// token-derived words.
pub(super) fn decompose_word_with_spans(value: &str, spans: &[WordSpan]) -> Vec<Node> {
    let segments = segments_with_params(value, spans);
    segments.into_iter().map(segment_to_node).collect()
}

/// Creates a single `WordLiteral` for synthetic values (no lexer token).
/// Used for fd numbers (`"0"`, `"1"`), synthetic `"$@"`, etc.
pub(super) fn decompose_word_literal(value: &str) -> Vec<Node> {
    vec![literal_fallback(value)]
}

fn segment_to_node(seg: WordSegment) -> Node {
    match seg {
        WordSegment::Literal(text) => Node::empty(NodeKind::WordLiteral { value: text }),
        WordSegment::AnsiCQuote(content) => {
            let decoded = ansi_c::ansi_c_decode(&content);
            Node::empty(NodeKind::AnsiCQuote { content, decoded })
        }
        WordSegment::LocaleString(content) => {
            let inner = ansi_c::strip_locale_quotes(&content);
            Node::empty(NodeKind::LocaleString { content, inner })
        }
        WordSegment::ArithmeticSub(inner) => substitution::arithmetic_sub_to_node(&inner),
        WordSegment::CommandSubstitution(content) => substitution::cmdsub_to_node(&content),
        WordSegment::ProcessSubstitution(direction, content) => {
            substitution::procsub_to_node(direction, &content)
        }
        WordSegment::SimpleVar(text) => param_expansion::parse_simple_var(&text),
        WordSegment::ParamExpansion(text) => param_expansion::parse_braced_param(&text),
        WordSegment::BraceExpansion(text) => {
            Node::empty(NodeKind::BraceExpansion { content: text })
        }
    }
}

#[cfg(test)]
mod tests;
