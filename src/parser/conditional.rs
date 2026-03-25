//! Conditional expression parser for `[[ ... ]]`.

use crate::ast::{Node, NodeKind, Span};
use crate::error::Result;
use crate::token::{Token, TokenType};

use super::{
    Parser,
    helpers::{cond_term_from_token, is_cond_binary_op},
};

impl Parser {
    pub(super) fn parse_cond_command(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::DoubleLeftBracket)?;
        self.lexer.enter_cond_expr();

        let body = self.parse_cond_or()?;

        self.lexer.leave_cond_expr();
        self.expect_cond_close()?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(self.spanned(
            start,
            NodeKind::ConditionalExpr {
                body: Box::new(body),
                redirects,
            },
        ))
    }

    fn parse_cond_or(&mut self) -> Result<Node> {
        let mut left = self.parse_cond_and()?;
        while !self.is_cond_close()? && self.peek_is(TokenType::Or)? {
            self.lexer.next_token()?;
            let right = self.parse_cond_and()?;
            let span = Span::new(left.span.start, right.span.end);
            left = Node::new(
                NodeKind::CondOr {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_cond_and(&mut self) -> Result<Node> {
        let mut left = self.parse_cond_primary()?;
        while !self.is_cond_close()? && self.peek_is(TokenType::And)? {
            self.lexer.next_token()?;
            let right = self.parse_cond_primary()?;
            let span = Span::new(left.span.start, right.span.end);
            left = Node::new(
                NodeKind::CondAnd {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_cond_primary(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        let tok = self.lexer.peek_token()?;

        // Handle ! (negation) — Parable drops it in S-expression output,
        // but we keep it in the AST so the reformatter can preserve it.
        if tok.kind == TokenType::Bang {
            self.lexer.next_token()?;
            let inner = self.parse_cond_primary()?;
            return Ok(self.spanned(
                start,
                NodeKind::CondNot {
                    operand: Box::new(inner),
                },
            ));
        }

        // Handle ( grouped expression )
        if tok.kind == TokenType::LeftParen {
            self.lexer.next_token()?;
            let inner = self.parse_cond_or()?;
            self.expect(TokenType::RightParen)?;
            return Ok(self.spanned(
                start,
                NodeKind::CondParen {
                    inner: Box::new(inner),
                },
            ));
        }

        let first = self.lexer.next_token()?;

        // Check for unary operators: -f, -d, -z, -n, etc.
        if first.value.starts_with('-')
            && first.value.len() <= 3
            && self.peek_cond_term()?.is_some()
        {
            let operand_tok = self.lexer.next_token()?;
            return Ok(self.spanned(
                start,
                NodeKind::UnaryTest {
                    op: first.value,
                    operand: Box::new(cond_term_from_token(&operand_tok)),
                },
            ));
        }

        // Check for binary operators
        if !self.is_cond_close()?
            && !self.peek_is(TokenType::And)?
            && !self.peek_is(TokenType::Or)?
        {
            let op_tok = self.lexer.peek_token()?;
            let is_binary = is_cond_binary_op(&op_tok.value)
                || op_tok.kind == TokenType::Less
                || op_tok.kind == TokenType::Greater;
            if is_binary {
                let op = self.lexer.next_token()?;
                let right = self.lexer.next_token()?;
                return Ok(self.spanned(
                    start,
                    NodeKind::BinaryTest {
                        op: op.value,
                        left: Box::new(cond_term_from_token(&first)),
                        right: Box::new(cond_term_from_token(&right)),
                    },
                ));
            }
        }

        // Bare word: implicit -n test
        Ok(self.spanned(
            start,
            NodeKind::UnaryTest {
                op: "-n".to_string(),
                operand: Box::new(cond_term_from_token(&first)),
            },
        ))
    }

    fn expect_cond_close(&mut self) -> Result<Token> {
        self.expect_closing(TokenType::DoubleRightBracket, "]]")
    }

    fn is_cond_close(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(tok.kind == TokenType::DoubleRightBracket
            || (tok.kind == TokenType::Word && tok.value == "]]"))
    }

    fn peek_cond_term(&mut self) -> Result<Option<()>> {
        if self.is_cond_close()? {
            return Ok(None);
        }
        let tok = self.lexer.peek_token()?;
        if matches!(tok.kind, TokenType::And | TokenType::Or) {
            Ok(None)
        } else {
            Ok(Some(()))
        }
    }
}
