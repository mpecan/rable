//! Subshells, brace groups, function definitions, `coproc`, and arithmetic
//! commands (`(( … ))`).

use crate::ast::{Node, NodeKind};
use crate::error::Result;
use crate::token::{Token, TokenType};

use super::Parser;
use super::helpers::{is_fd_number, is_redirect_op_kind, word_node_from_token};

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

    pub(super) fn parse_coproc(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Coproc)?;

        // Path A: `coproc CMD` — no name, body is whatever starts a command.
        if coproc_starts_command(self.lexer.peek_token()?.kind) {
            return self.build_coproc_with_command(start, None);
        }

        let first_tok = self.lexer.next_token()?;
        self.lexer.set_command_start();

        // Path B: `coproc <redir ...` — synthetic command with only redirects.
        if is_redirect_op_kind(first_tok.kind) {
            return self.parse_coproc_redirect_only(start, first_tok);
        }

        // Path C: `coproc NAME CMD` — named coproc, `first_tok` is the name.
        if coproc_starts_command(self.lexer.peek_token()?.kind) {
            return self.build_coproc_with_command(start, Some(first_tok.value));
        }

        // Path D: `coproc WORD WORD... [redirs]` — synthetic command with
        // `first_tok` as the first word.
        let (words, redirects) = self.parse_coproc_word_loop(first_tok)?;
        Ok(self.build_coproc_synthetic_command(start, None, words, redirects))
    }

    /// Parses the body of a `coproc [NAME] CMD` form by delegating to
    /// `parse_command`. Returns the wrapped `Coproc` node.
    fn build_coproc_with_command(&mut self, start: usize, name: Option<String>) -> Result<Node> {
        let command = self.parse_command()?;
        Ok(self.spanned(
            start,
            NodeKind::Coproc {
                name,
                command: Box::new(command),
            },
        ))
    }

    /// Path B: first token after `coproc` is a redirect operator. Build a
    /// synthetic `Command { redirects }` wrapped in a nameless `Coproc`.
    fn parse_coproc_redirect_only(&mut self, start: usize, first_tok: Token) -> Result<Node> {
        let mut redirects = vec![self.build_redirect(first_tok, -1, None)?];
        redirects.extend(self.parse_trailing_redirects()?);
        Ok(self.build_coproc_synthetic_command(start, None, Vec::new(), redirects))
    }

    /// Path D: loop over words and redirects after `coproc WORD` to collect
    /// the synthetic command's contents.
    fn parse_coproc_word_loop(&mut self, first_tok: Token) -> Result<(Vec<Node>, Vec<Node>)> {
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
            if !matches!(tok.kind, TokenType::Word | TokenType::Number) {
                break;
            }
            let tok = self.lexer.next_token()?;
            if is_fd_number(&tok.value) && self.is_redirect_operator()? {
                redirects.push(self.parse_redirect_with_fd(&tok)?);
            } else {
                words.push(word_node_from_token(tok));
            }
        }
        Ok((words, redirects))
    }

    /// Wraps a synthetic `Command { assignments: [], words, redirects }` in
    /// a `Coproc { name, command }` at `start`.
    fn build_coproc_synthetic_command(
        &self,
        start: usize,
        name: Option<String>,
        words: Vec<Node>,
        redirects: Vec<Node>,
    ) -> Node {
        let command = self.spanned(
            start,
            NodeKind::Command {
                assignments: Vec::new(),
                words,
                redirects,
            },
        );
        self.spanned(
            start,
            NodeKind::Coproc {
                name,
                command: Box::new(command),
            },
        )
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

/// Returns true when `kind` can start a command at the body position of a
/// `coproc` clause. Excludes `coproc`, `time`, and `!` since they would
/// cause re-entry into `parse_coproc` or ambiguous negation.
const fn coproc_starts_command(kind: TokenType) -> bool {
    kind.starts_command() && !matches!(kind, TokenType::Coproc | TokenType::Time | TokenType::Bang)
}
