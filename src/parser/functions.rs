//! Subshells, brace groups, function definitions, `coproc`, and arithmetic
//! commands (`(( … ))`).

use crate::ast::{Node, NodeKind};
use crate::error::Result;
use crate::token::{Token, TokenType};

use super::Parser;
use super::helpers::{is_fd_number, word_node_from_token};

impl Parser {
    pub(super) fn parse_subshell(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::LeftParen)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect(TokenType::RightParen)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(self.spanned(
            start,
            NodeKind::Subshell {
                body: Box::new(body),
                redirects,
            },
        ))
    }

    pub(super) fn parse_brace_group(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::LeftBrace)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect_brace_close()?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(self.spanned(
            start,
            NodeKind::BraceGroup {
                body: Box::new(body),
                redirects,
            },
        ))
    }

    pub(super) fn parse_function(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Function)?;
        let name_tok = self.lexer.next_token()?;
        let name = name_tok.value;

        self.lexer.set_command_start();
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
            if self.peek_is(TokenType::RightParen)? {
                // function f() { ... } — empty parens syntax
                self.lexer.next_token()?;
                self.lexer.set_command_start();
            } else {
                // function f ( cmd ) — subshell body
                self.skip_newlines()?;
                let body = self.parse_list()?;
                self.expect(TokenType::RightParen)?;
                let redirects = self.parse_trailing_redirects()?;
                return Ok(self.spanned(
                    start,
                    NodeKind::Function {
                        name,
                        body: Box::new(self.spanned(
                            start,
                            NodeKind::Subshell {
                                body: Box::new(body),
                                redirects,
                            },
                        )),
                    },
                ));
            }
        }

        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(self.spanned(
            start,
            NodeKind::Function {
                name,
                body: Box::new(body),
            },
        ))
    }

    pub(super) fn parse_function_def(&mut self, name_tok: &Token) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::RightParen)?;
        self.lexer.set_command_start();
        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(self.spanned(
            start,
            NodeKind::Function {
                name: name_tok.value.clone(),
                body: Box::new(body),
            },
        ))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn parse_coproc(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Coproc)?;

        let tok = self.lexer.peek_token()?;
        if tok.kind.starts_command()
            && !matches!(
                tok.kind,
                TokenType::Coproc | TokenType::Time | TokenType::Bang
            )
        {
            let command = self.parse_command()?;
            return Ok(self.spanned(
                start,
                NodeKind::Coproc {
                    name: None,
                    command: Box::new(command),
                },
            ));
        }

        let first_tok = self.lexer.next_token()?;
        self.lexer.set_command_start();

        // If first token after coproc is a redirect operator, parse as
        // a command with redirects (no name, no command word)
        if matches!(
            first_tok.kind,
            TokenType::Less
                | TokenType::Greater
                | TokenType::DoubleGreater
                | TokenType::LessAnd
                | TokenType::GreaterAnd
                | TokenType::LessGreater
                | TokenType::GreaterPipe
                | TokenType::AndGreater
                | TokenType::AndDoubleGreater
                | TokenType::DoubleLess
                | TokenType::DoubleLessDash
                | TokenType::TripleLess
        ) {
            let mut redirects = vec![self.build_redirect(first_tok, -1, None)?];
            redirects.extend(self.parse_trailing_redirects()?);
            return Ok(self.spanned(
                start,
                NodeKind::Coproc {
                    name: None,
                    command: Box::new(self.spanned(
                        start,
                        NodeKind::Command {
                            assignments: Vec::new(),
                            words: Vec::new(),
                            redirects,
                        },
                    )),
                },
            ));
        }

        let next = self.lexer.peek_token()?;
        let name = if next.kind.starts_command()
            && !matches!(
                next.kind,
                TokenType::Coproc | TokenType::Time | TokenType::Bang
            ) {
            let n = Some(first_tok.value);
            let command = self.parse_command()?;
            return Ok(self.spanned(
                start,
                NodeKind::Coproc {
                    name: n,
                    command: Box::new(command),
                },
            ));
        } else {
            None
        };
        let mut words = vec![word_node_from_token(first_tok)];
        let mut redirects = Vec::new();
        loop {
            if self.at_end()? {
                break;
            }
            if self.is_redirect_operator()? {
                redirects.push(self.parse_redirect()?);
                continue;
            }
            let tok = self.lexer.peek_token()?;
            if matches!(tok.kind, TokenType::Word | TokenType::Number) {
                let tok = self.lexer.next_token()?;
                if is_fd_number(&tok.value) && self.is_redirect_operator()? {
                    redirects.push(self.parse_redirect_with_fd(&tok)?);
                } else {
                    words.push(word_node_from_token(tok));
                }
            } else {
                break;
            }
        }
        Ok(self.spanned(
            start,
            NodeKind::Coproc {
                name,
                command: Box::new(self.spanned(
                    start,
                    NodeKind::Command {
                        assignments: Vec::new(),
                        words,
                        redirects,
                    },
                )),
            },
        ))
    }

    pub(super) fn parse_arith_command(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::LeftParen)?;
        let content = self.lexer.read_until_double_paren()?;
        let redirects = self.parse_trailing_redirects()?;
        Ok(self.spanned(
            start,
            NodeKind::ArithmeticCommand {
                expression: None,
                redirects,
                raw_content: content,
            },
        ))
    }

    pub(super) fn expect_brace_close(&mut self) -> Result<Token> {
        self.expect_closing(TokenType::RightBrace, "}")
    }
}
