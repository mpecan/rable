//! Loop parsers: `while`, `until`, `for` (word-list and C-style), and the
//! shared `do … done` / `{ … }` body reader.

use crate::ast::{Node, NodeKind};
use crate::error::{RableError, Result};
use crate::lexer::split_top_level;
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
        let parts = split_top_level(&raw, ';')?;
        let [init, cond, incr] = &parts[..] else {
            let message = if parts.len() < 3 {
                "syntax error: arithmetic expression required"
            } else {
                "syntax error: `;' unexpected"
            };
            return Err(RableError::parse(message, start, self.lexer.line()));
        };
        let expr = |part: &str| match part.trim_start() {
            "" => "1".to_string(),
            text => text.to_string(),
        };
        let (init, cond, incr) = (expr(init), expr(cond), expr(incr));

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
