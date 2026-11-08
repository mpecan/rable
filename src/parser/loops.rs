//! Loop parsers: `while`, `until`, `for` (word-list and C-style), and the
//! shared `do … done` / `{ … }` body reader.

use crate::ast::{Node, NodeKind};
use crate::error::{RableError, Result};
use crate::token::TokenType;

use super::Parser;
use super::helpers::word_node;

impl Parser {
    pub(super) fn parse_while(&mut self) -> Result<Node> {
        self.parse_while_until(TokenType::While, true)
    }

    pub(super) fn parse_until(&mut self) -> Result<Node> {
        self.parse_while_until(TokenType::Until, false)
    }

    fn parse_while_until(&mut self, keyword: TokenType, is_while: bool) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(keyword)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.lexer.set_command_start();
        self.expect(TokenType::Do)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect(TokenType::Done)?;
        let redirects = self.parse_trailing_redirects()?;

        let condition = Box::new(condition);
        let body = Box::new(body);
        if is_while {
            Ok(self.spanned(
                start,
                NodeKind::While {
                    condition,
                    body,
                    redirects,
                },
            ))
        } else {
            Ok(self.spanned(
                start,
                NodeKind::Until {
                    condition,
                    body,
                    redirects,
                },
            ))
        }
    }

    pub(super) fn parse_for(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::For)?;

        if self.peek_is(TokenType::LeftParen)? {
            return self.parse_for_arith(start);
        }

        let var_tok = self.lexer.next_token()?;
        let var = var_tok.value;

        self.lexer.set_command_start();
        self.skip_newlines()?;
        let words = if self.peek_is(TokenType::In)? {
            self.lexer.next_token()?;
            Some(self.parse_in_word_list()?)
        } else {
            Some(vec![word_node("\"$@\"")])
        };

        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.set_command_start();
        let (body, redirects) = self.parse_loop_body()?;

        Ok(self.spanned(
            start,
            NodeKind::For {
                var,
                words,
                body: Box::new(body),
                redirects,
            },
        ))
    }

    fn parse_for_arith(&mut self, start: usize) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::LeftParen)?;

        let raw = self.lexer.read_until_double_paren()?;
        // bash's make_arith_for_command: exactly three `;`-separated
        // expressions, each without leading blanks, an empty one being `1`.
        let parts = split_arith_for(&raw);
        if parts.len() != 3 {
            let message = if parts.len() < 3 {
                "syntax error: arithmetic expression required"
            } else {
                "syntax error: `;' unexpected"
            };
            return Err(RableError::parse(message, start, self.lexer.line()));
        }
        let expr = |part: &str| match part.trim_start() {
            "" => "1".to_string(),
            text => text.to_string(),
        };
        let (init, cond, incr) = (expr(parts[0]), expr(parts[1]), expr(parts[2]));

        self.skip_newlines()?;
        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.set_command_start();
        let (body, redirects) = self.parse_loop_body()?;

        Ok(self.spanned(
            start,
            NodeKind::ForArith {
                init,
                cond,
                incr,
                body: Box::new(body),
                redirects,
            },
        ))
    }

    /// Shared do/done or {/} loop body parsing.
    pub(super) fn parse_loop_body(&mut self) -> Result<(Node, Vec<Node>)> {
        if self.peek_is(TokenType::LeftBrace)? {
            let bg = self.parse_brace_group()?;
            let redirects = self.parse_trailing_redirects()?;
            if let NodeKind::BraceGroup { body, .. } = bg.kind {
                Ok((*body, redirects))
            } else {
                Ok((bg, redirects))
            }
        } else {
            self.expect(TokenType::Do)?;
            self.skip_newlines()?;
            let body = self.parse_list()?;
            self.expect(TokenType::Done)?;
            let redirects = self.parse_trailing_redirects()?;
            Ok((body, redirects))
        }
    }
}

/// Splits a `for ((…))` header at its top-level `;`s, as bash's
/// `skip_to_delim` does: quotes, backslash escapes, backticks and
/// `$(…)`, `${…}`, `$[…]` are skipped over, so `$((1;2))` and `"a;b"`
/// stay inside one expression.
fn split_arith_for(raw: &str) -> Vec<&str> {
    let bytes = raw.as_bytes();
    let mut parts = Vec::new();
    let (mut start, mut i) = (0, 0);
    let mut closers: Vec<u8> = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            quote @ (b'\'' | b'"' | b'`') => {
                i = skip_quoted(bytes, i, quote);
            }
            b'$' if matches!(bytes.get(i + 1), Some(b'(' | b'{' | b'[')) => {
                closers.push(match bytes[i + 1] {
                    b'(' => b')',
                    b'{' => b'}',
                    _ => b']',
                });
                i += 1;
            }
            b'(' if !closers.is_empty() => closers.push(b')'),
            c if closers.last() == Some(&c) => {
                closers.pop();
            }
            b';' if closers.is_empty() => {
                parts.push(&raw[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&raw[start..]);
    parts
}

/// Index of the quote closing the one at `open` (or the end), honouring
/// backslash escapes except inside single quotes.
fn skip_quoted(bytes: &[u8], open: usize, quote: u8) -> usize {
    let mut i = open + 1;
    while i < bytes.len() && bytes[i] != quote {
        if bytes[i] == b'\\' && quote != b'\'' {
            i += 1;
        }
        i += 1;
    }
    i
}
