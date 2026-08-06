//! Helpers shared by the lexer's test modules.

use super::{Lexer, TokenType};

/// Lexes `source` to EOF, returning each token's kind and value.
#[allow(clippy::unwrap_used)]
pub(super) fn collect_tokens(source: &str) -> Vec<(TokenType, String)> {
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
