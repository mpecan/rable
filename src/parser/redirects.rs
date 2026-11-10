//! Redirection parsing: `<`, `>`, `>>`, `<&`, `>&`, `<>`, `>|`,
//! `&>`, `&>>`, `<<`, `<<-`, and `<<<`, plus heredoc queuing.

use crate::ast::{Node, NodeKind, RedirectKind};
use crate::error::{RableError, Result};
use crate::lexer::default_fd_for_op;
use crate::lexer::heredoc::parse_heredoc_delimiter;
use crate::token::{Token, TokenType};

use super::helpers::{
    is_fd_number, is_redirect_op_kind, is_varfd, word_node, word_node_from_token,
};
use super::{Parser, unexpected_token};

impl Parser {
    pub(super) fn parse_redirect(&mut self) -> Result<Node> {
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(&op_tok, -1, None)
    }

    pub(super) fn parse_redirect_with_fd(&mut self, fd_tok: &Token) -> Result<Node> {
        let fd: i32 = fd_tok
            .value
            .parse()
            .map_err(|_| RableError::parse("invalid fd number", fd_tok.pos, fd_tok.line))?;
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(&op_tok, fd, None)
    }

    /// Parses a `{name}op target` redirect. `varfd_tok.value` is the
    /// literal `{name}` text from the lexer; the braces are stripped
    /// before the name is stored on the AST.
    pub(super) fn parse_redirect_with_varfd(&mut self, varfd_tok: &Token) -> Result<Node> {
        let raw = varfd_tok.value.as_str();
        let name = raw
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(raw)
            .to_string();
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(&op_tok, -1, Some(name))
    }

    pub(super) fn build_redirect(
        &mut self,
        op_tok: &Token,
        fd: i32,
        varfd: Option<String>,
    ) -> Result<Node> {
        if op_tok.kind == TokenType::DoubleLess || op_tok.kind == TokenType::DoubleLessDash {
            return self.build_heredoc_redirect(op_tok, fd);
        }

        let start = op_tok.pos;
        let is_dup = op_tok.kind == TokenType::GreaterAnd || op_tok.kind == TokenType::LessAnd;
        // `>&-` / `<&-` are one token; the operator is `>&` / `<&`.
        let op = op_tok.value.trim_end_matches('-').to_string();
        // Dup, move and close redirects always act on a definite fd, which
        // bash prints (`>&2` is `1>&2`): resolve the default here.
        let fd = if is_dup && fd == -1 {
            default_fd_for_op(&op)
        } else {
            fd
        };
        let redirect = |target: Node, kind| NodeKind::Redirect {
            op,
            target: Box::new(target),
            fd,
            varfd,
            kind,
        };
        if op_tok.value.ends_with('-') {
            return Ok(self.spanned(start, redirect(word_node("-"), RedirectKind::Close)));
        }
        let target_tok = self.expect_word()?;
        let kind = match target_tok.value.as_str() {
            _ if !is_dup => RedirectKind::File,
            "-" => RedirectKind::Close,
            // `N<&M-` moves M to N; the target is `M`.
            value if value.ends_with('-') => {
                let moved = word_node(&value[..value.len() - 1]);
                return Ok(self.spanned(start, redirect(moved, RedirectKind::Move)));
            }
            _ => RedirectKind::Dup,
        };
        Ok(self.spanned(start, redirect(word_node_from_token(target_tok), kind)))
    }

    fn build_heredoc_redirect(&mut self, op_tok: &Token, fd: i32) -> Result<Node> {
        let start = op_tok.pos;
        let delim_tok = self.expect_word()?;
        let strip_tabs = op_tok.kind == TokenType::DoubleLessDash;
        let (delimiter, quoted) = parse_heredoc_delimiter(&delim_tok.value);
        self.lexer
            .queue_heredoc(delimiter.clone(), strip_tabs, quoted);
        Ok(self.spanned(
            start,
            NodeKind::HereDoc {
                delimiter,
                content: String::new(),
                strip_tabs,
                quoted,
                fd,
                complete: true,
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
                    // The fd prefix is consumed to look past it; a word after a
                    // compound command is a syntax error, never droppable text.
                    if is_fd_number(&tok.value) {
                        let tok = self.lexer.next_token()?;
                        if !self.is_redirect_operator()? || self.is_and_redirect()? {
                            return Err(unexpected_token(&tok));
                        }
                        redirects.push(self.parse_redirect_with_fd(&tok)?);
                        continue;
                    }
                    if is_varfd(&tok.value) {
                        let varfd_tok = self.lexer.next_token()?;
                        if !self.is_redirect_operator()? || self.is_and_redirect()? {
                            return Err(unexpected_token(&varfd_tok));
                        }
                        redirects.push(self.parse_redirect_with_varfd(&varfd_tok)?);
                        continue;
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
        Ok(is_redirect_op_kind(tok.kind))
    }
}
