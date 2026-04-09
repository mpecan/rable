//! Redirection parsing: `<`, `>`, `>>`, `<&`, `>&`, `<>`, `>|`,
//! `&>`, `&>>`, `<<`, `<<-`, and `<<<`, plus heredoc queuing.

use crate::ast::{Node, NodeKind};
use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::Parser;
use super::helpers::{
    is_fd_number, is_varfd, parse_heredoc_delimiter, word_node, word_node_from_token,
};

impl Parser {
    pub(super) fn parse_redirect(&mut self) -> Result<Node> {
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, -1)
    }

    pub(super) fn parse_redirect_with_fd(&mut self, fd_tok: &Token) -> Result<Node> {
        let fd: i32 = fd_tok
            .value
            .parse()
            .map_err(|_| RableError::parse("invalid fd number", fd_tok.pos, fd_tok.line))?;
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, fd)
    }

    pub(super) fn build_redirect(&mut self, op_tok: Token, fd: i32) -> Result<Node> {
        let start = op_tok.pos;
        if op_tok.kind == TokenType::DoubleLess || op_tok.kind == TokenType::DoubleLessDash {
            let delim_tok = self.lexer.next_token()?;
            let strip_tabs = op_tok.kind == TokenType::DoubleLessDash;
            let (delimiter, quoted) = parse_heredoc_delimiter(&delim_tok.value);
            self.lexer
                .queue_heredoc(delimiter.clone(), strip_tabs, quoted);
            return Ok(self.spanned(
                start,
                NodeKind::HereDoc {
                    delimiter,
                    content: String::new(),
                    strip_tabs,
                    quoted,
                    fd,
                    complete: true,
                },
            ));
        }

        // >&- and <&- are complete close-fd operators (no target needed)
        if op_tok.value == ">&-" || op_tok.value == "<&-" {
            return Ok(self.spanned(
                start,
                NodeKind::Redirect {
                    op: ">&-".to_string(),
                    target: Box::new(word_node("0")),
                    fd,
                },
            ));
        }

        let target_tok = self.lexer.next_token()?;
        let is_dup = op_tok.kind == TokenType::GreaterAnd || op_tok.kind == TokenType::LessAnd;

        if is_dup && target_tok.value == "-" {
            return Ok(self.spanned(
                start,
                NodeKind::Redirect {
                    op: ">&-".to_string(),
                    target: Box::new(word_node("0")),
                    fd: -1,
                },
            ));
        }
        if is_dup && target_tok.value.ends_with('-') {
            let fd_str = &target_tok.value[..target_tok.value.len() - 1];
            return Ok(self.spanned(
                start,
                NodeKind::Redirect {
                    op: op_tok.value,
                    target: Box::new(word_node(fd_str)),
                    fd: -1,
                },
            ));
        }

        Ok(self.spanned(
            start,
            NodeKind::Redirect {
                op: op_tok.value,
                target: Box::new(word_node_from_token(target_tok)),
                fd,
            },
        ))
    }

    pub(super) fn parse_trailing_redirects(&mut self) -> Result<Vec<Node>> {
        let mut redirects = Vec::new();
        loop {
            if self.at_end()? {
                break;
            }
            if self.is_redirect_operator()? {
                redirects.push(self.parse_redirect()?);
            } else {
                let tok = self.lexer.peek_token()?;
                if tok.kind == TokenType::Word || tok.kind == TokenType::Number {
                    if is_fd_number(&tok.value) {
                        let tok = self.lexer.next_token()?;
                        if self.is_redirect_operator()? {
                            redirects.push(self.parse_redirect_with_fd(&tok)?);
                            continue;
                        }
                        break;
                    }
                    if is_varfd(&tok.value) {
                        let _varfd = self.lexer.next_token()?;
                        if self.is_redirect_operator()? {
                            redirects.push(self.parse_redirect()?);
                            continue;
                        }
                        break;
                    }
                }
                break;
            }
        }
        Ok(redirects)
    }

    /// Returns true if the next token is `&>` or `&>>` (which never take fd prefixes).
    pub(super) fn is_and_redirect(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(matches!(
            tok.kind,
            TokenType::AndGreater | TokenType::AndDoubleGreater
        ))
    }

    pub(super) fn is_redirect_operator(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(matches!(
            tok.kind,
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
        ))
    }
}
