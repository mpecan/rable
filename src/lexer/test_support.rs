//! Helpers shared by the lexer's test modules.

use super::{Lexer, TokenType};

/// Lexes `source` to EOF with extglob off, returning each token's kind
/// and value.
pub(super) fn collect_tokens(source: &str) -> Vec<(TokenType, String)> {
    collect_tokens_with(source, false)
}

/// Like [`collect_tokens`], with the extglob setting given explicitly.
#[allow(clippy::unwrap_used)]
pub(super) fn collect_tokens_with(source: &str, extglob: bool) -> Vec<(TokenType, String)> {
    let mut lexer = Lexer::new(source, extglob);
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
