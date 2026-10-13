//! `WordSpan` tests: which sub-ranges of a word the lexer records, and
//! with what kind.
//!
//! These spans drive both the S-expression word formatter and `Word.parts`
//! decomposition, so their offsets are load-bearing. Split from `tests.rs`
//! to keep both files under the project's file-length limit.

use super::test_support::collect_tokens;
use super::*;

use super::word_builder::{WordSpan, WordSpanKind};

#[allow(clippy::unwrap_used)]
fn first_word_spans(source: &str) -> (String, Vec<WordSpan>) {
    let mut lexer = Lexer::new(source, false);
    let tok = lexer.next_token().unwrap();
    (tok.value, tok.spans)
}

#[test]
fn span_plain_word_no_spans() {
    let (_, spans) = first_word_spans("echo");
    assert!(spans.is_empty());
}

#[test]
fn span_command_sub() {
    let (val, spans) = first_word_spans("$(cmd)");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_command_sub_mid_word() {
    let (_, spans) = first_word_spans("hello$(world)end");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 5);
    assert_eq!(spans[0].end, 13);
}

#[test]
fn span_arithmetic_sub() {
    let (val, spans) = first_word_spans("$((1+2))");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::ArithmeticSub);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_param_expansion() {
    let (val, spans) = first_word_spans("${var:-default}");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::ParamExpansion);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_simple_var() {
    let (val, spans) = first_word_spans("$HOME");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::SimpleVar);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_ansi_c_quote() {
    let (val, spans) = first_word_spans("$'foo'");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::AnsiCQuote);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_locale_string() {
    let (val, spans) = first_word_spans("$\"hello\"");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::LocaleString);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_single_quoted() {
    let (val, spans) = first_word_spans("'quoted'");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::SingleQuoted);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_double_quoted() {
    let (val, spans) = first_word_spans("\"double\"");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::DoubleQuoted);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_backtick() {
    let (val, spans) = first_word_spans("`cmd`");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::Backtick);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_escape() {
    let (val, spans) = first_word_spans("\\n");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::Escape);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_line_continuation_no_span() {
    // \<newline> is a line continuation — nothing pushed, no span
    let (val, spans) = first_word_spans("hel\\\nlo");
    assert_eq!(val, "hello");
    assert!(spans.is_empty());
}

#[test]
fn span_bare_dollar_no_span() {
    // Bare $ at end — no expansion, no span
    let tokens = collect_tokens("echo $");
    assert_eq!(tokens[1].1, "$");
    // The $ word has no spans (bare dollar)
    let (_, spans) = first_word_spans("$");
    assert!(spans.is_empty());
}

#[test]
fn span_nested_double_quoted_with_cmdsub() {
    // "$(cmd)" — DoubleQuoted span contains CommandSub span
    let (val, spans) = first_word_spans("\"$(cmd)\"");
    assert_eq!(val, "\"$(cmd)\"");
    assert_eq!(spans.len(), 2);
    // CommandSub is recorded first (inside read_dollar, before
    // DoubleQuoted is closed by read_word_special)
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 1);
    assert_eq!(spans[0].end, 7);
    assert_eq!(spans[1].kind, WordSpanKind::DoubleQuoted);
    assert_eq!(spans[1].start, 0);
    assert_eq!(spans[1].end, 8);
}

#[test]
fn span_deprecated_arith() {
    let (val, spans) = first_word_spans("$[1+2]");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::DeprecatedArith);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}
