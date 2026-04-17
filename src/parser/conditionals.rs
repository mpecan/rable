//! Conditional statements: `if`/`elif`/`else`/`fi` and `[[ … ]]` expressions.

use crate::ast::{Node, NodeKind, Span};
use crate::error::Result;
use crate::token::{Token, TokenType};

use super::Parser;
use super::helpers::{cond_term_from_token, is_cond_binary_op};

impl Parser {
    pub(super) fn parse_if(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::If)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.expect(TokenType::Then)?;
        self.skip_newlines()?;
        let then_body = self.parse_list()?;

        let else_body = if self.peek_is(TokenType::Elif)? {
            Some(Box::new(self.parse_elif()?))
        } else if self.peek_is(TokenType::Else)? {
            self.lexer.next_token()?;
            self.skip_newlines()?;
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };

        self.expect(TokenType::Fi)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(self.spanned(
            start,
            NodeKind::If {
                condition: Box::new(condition),
                then_body: Box::new(then_body),
                else_body,
                redirects,
            },
        ))
    }

    fn parse_elif(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.enter()?;
        self.expect(TokenType::Elif)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.expect(TokenType::Then)?;
        self.skip_newlines()?;
        let then_body = self.parse_list()?;

        let else_body = if self.peek_is(TokenType::Elif)? {
            Some(Box::new(self.parse_elif()?))
        } else if self.peek_is(TokenType::Else)? {
            self.lexer.next_token()?;
            self.skip_newlines()?;
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };

        self.leave();
        Ok(self.spanned(
            start,
            NodeKind::If {
                condition: Box::new(condition),
                then_body: Box::new(then_body),
                else_body,
                redirects: Vec::new(),
            },
        ))
    }

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

    fn parse_cond_primary(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        let kind = self.lexer.peek_token()?.kind;

        if let Some(node) = self.try_parse_cond_negation(start, kind)? {
            return Ok(node);
        }
        if let Some(node) = self.try_parse_cond_group(start, kind)? {
            return Ok(node);
        }

        let first = self.lexer.next_token()?;
        self.parse_cond_operand(start, first)
    }

    /// `! expr` — Parable drops the negation in S-expression output, but we
    /// keep it in the AST so the reformatter can preserve it.
    fn try_parse_cond_negation(&mut self, start: usize, kind: TokenType) -> Result<Option<Node>> {
        if kind != TokenType::Bang {
            return Ok(None);
        }
        self.lexer.next_token()?;
        let inner = self.parse_cond_primary()?;
        Ok(Some(self.spanned(
            start,
            NodeKind::CondNot {
                operand: Box::new(inner),
            },
        )))
    }

    /// `( expr )` — a grouped expression inside `[[ … ]]`.
    fn try_parse_cond_group(&mut self, start: usize, kind: TokenType) -> Result<Option<Node>> {
        if kind != TokenType::LeftParen {
            return Ok(None);
        }
        self.lexer.next_token()?;
        let inner = self.parse_cond_or()?;
        self.expect(TokenType::RightParen)?;
        Ok(Some(self.spanned(
            start,
            NodeKind::CondParen {
                inner: Box::new(inner),
            },
        )))
    }

    /// Parse `-f EXPR` (unary), `EXPR OP EXPR` (binary), or a bare word
    /// (`[-n] EXPR`). `first` is the already-consumed leading token.
    fn parse_cond_operand(&mut self, start: usize, first: Token) -> Result<Node> {
        // Unary operators: -f, -d, -z, -n, etc.
        if first.value.starts_with('-')
            && first.value.len() <= 3
            && self.peek_cond_term()?.is_some()
        {
            let operand_tok = self.lexer.next_token()?;
            return Ok(self.spanned(
                start,
                NodeKind::UnaryTest {
                    op: first.value,
                    operand: Box::new(cond_term_from_token(operand_tok)),
                },
            ));
        }

        // Binary operators: ==, !=, =~, <, >, -eq, -ne, ...
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
                        left: Box::new(cond_term_from_token(first)),
                        right: Box::new(cond_term_from_token(right)),
                    },
                ));
            }
        }

        // Bare word: implicit -n test
        Ok(self.spanned(
            start,
            NodeKind::UnaryTest {
                op: "-n".to_string(),
                operand: Box::new(cond_term_from_token(first)),
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
