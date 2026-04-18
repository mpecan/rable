//! Tests for the opaque-backtick fallback (issue #38).
//!
//! When `parse_backtick_body` rejects a backtick body, `read_backtick_inner`
//! falls back to a raw byte-level scan for the closing `` ` ``, matching
//! bash's lexing rule that a backtick body is a single word token whose
//! errors (if any) are runtime concerns, not parse-time.

use super::Lexer;
use crate::error::RableError;
use crate::token::TokenType;

#[allow(clippy::unwrap_used)]
fn collect_tokens(source: &str) -> Vec<(TokenType, String)> {
    let mut lexer = Lexer::new(source, false);
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token().unwrap();
        if tok.kind == TokenType::Eof {
            break;
        }
        tokens.push((tok.kind, tok.value));
    }
    tokens
}

#[test]
fn invalid_body_becomes_opaque_word() {
    // `else echo` — fork fails because `else` at command start is a
    // reserved word and cannot begin a simple command. The fallback
    // scanner must emit the whole backtick as one Word token.
    let tokens = collect_tokens("`else echo`");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "`else echo`");
}

#[test]
fn escape_does_not_terminate() {
    // Inside an opaque backtick body, `\<x>` consumes two bytes,
    // so an escaped `` ` `` does not falsely terminate the body.
    let tokens = collect_tokens("`else \\`then\\` echo`");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "`else \\`then\\` echo`");
}

#[test]
fn literal_newline_escape_consumes_two_bytes() {
    // `\n` inside an opaque body is literal backslash-then-n, not a
    // newline. The scanner's two-byte escape rule must consume both
    // without touching the line counter.
    let tokens = collect_tokens("`else a\\nb`");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "`else a\\nb`");
}

#[test]
fn trailing_backslash_at_eof_surfaces_error() {
    // A lone trailing `\` with no following byte must still produce
    // a MatchedPair error — the inner `if let` is a no-op, the outer
    // loop sees EOF, and the scanner reports unterminated backtick.
    let mut lexer = Lexer::new("`else\\", false);
    assert!(matches!(
        lexer.next_token(),
        Err(RableError::MatchedPair { .. }),
    ));
}

#[test]
fn unterminated_body_surfaces_error() {
    // Invalid body with no closing backtick must surface a
    // MatchedPair error rather than silently consuming input.
    let mut lexer = Lexer::new("`else echo", false);
    assert!(matches!(
        lexer.next_token(),
        Err(RableError::MatchedPair { .. }),
    ));
}

#[test]
#[allow(clippy::unwrap_used)]
fn newlines_in_body_advance_line_counter() {
    // Newlines inside an opaque backtick body must advance the
    // line counter so subsequent tokens report the correct line.
    let mut lexer = Lexer::new("`else\necho\n`\nok", false);
    let bt = lexer.next_token().unwrap();
    assert_eq!(bt.kind, TokenType::Word);
    assert_eq!(bt.value, "`else\necho\n`");
    let nl = lexer.next_token().unwrap();
    assert_eq!(nl.kind, TokenType::Newline);
    let ok = lexer.next_token().unwrap();
    assert_eq!(ok.value, "ok");
    assert_eq!(ok.line, 4);
}
