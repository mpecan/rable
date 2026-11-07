//! Subshells, brace groups, function definitions, `coproc`, and arithmetic
//! commands (`(( … ))`).

use crate::ast::{Node, NodeKind};
use crate::error::Result;
use crate::token::{Token, TokenType};

use super::arithmetic::parse_arith_expression;
use super::helpers::{is_redirect_op_kind, word_node_from_token};
use super::{Parser, unexpected_token};

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
        let name_tok = self.expect_word()?;
        let name = name_tok.value;

        self.lexer.set_command_start();
        // `function f (( x ))` has an arithmetic body, not a subshell.
        if self.peek_is(TokenType::LeftParen)? && !self.is_double_paren()? {
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
        let body = self.parse_function_body()?;

        Ok(self.spanned(
            start,
            NodeKind::Function {
                name,
                body: Box::new(body),
            },
        ))
    }

    pub(super) fn parse_function_def(&mut self, name_tok: &Token) -> Result<Node> {
        let start = name_tok.pos;
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::RightParen)?;
        self.lexer.set_command_start();
        self.skip_newlines()?;
        let body = self.parse_function_body()?;

        Ok(self.spanned(
            start,
            NodeKind::Function {
                name: name_tok.value.clone(),
                body: Box::new(body),
            },
        ))
    }

    /// Parses a function body, which bash requires to be a compound
    /// command: `f() echo`, `f()` and `f() g() { …; }` are syntax errors.
    fn parse_function_body(&mut self) -> Result<Node> {
        let tok = self.lexer.peek_token()?;
        if !tok.kind.starts_compound_command() {
            return Err(unexpected_token(tok));
        }
        self.parse_command()
    }

    pub(super) fn parse_coproc(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Coproc)?;

        // Path A: `coproc CMD` — no name, body is whatever starts a command.
        if coproc_starts_command(self.lexer.peek_token()?.kind) {
            return self.build_coproc_with_command(start, None);
        }

        let first_tok = self.lexer.next_token()?;

        // Path B: `coproc <redir ...` — synthetic command starting with a
        // redirect. The redirect target must not be re-armed as a command
        // start, so this check comes before `allow_command_after_word`.
        if is_redirect_op_kind(first_tok.kind) {
            return self.parse_coproc_redirect_first(start, first_tok);
        }
        self.lexer.allow_command_after_word();

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
    /// synthetic `Command` (`coproc >&fd cmd args`) wrapped in a nameless
    /// `Coproc`.
    fn parse_coproc_redirect_first(&mut self, start: usize, first_tok: Token) -> Result<Node> {
        let redirects = vec![self.build_redirect(first_tok, -1, None)?];
        let (words, redirects) = self.collect_coproc_words(Vec::new(), redirects)?;
        Ok(self.build_coproc_synthetic_command(start, None, words, redirects))
    }

    /// Path D: loop over words and redirects after `coproc WORD` to collect
    /// the synthetic command's contents.
    fn parse_coproc_word_loop(&mut self, first_tok: Token) -> Result<(Vec<Node>, Vec<Node>)> {
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        // `coproc 2>err cmd`: the first token may itself be an fd prefix.
        match self.try_parse_fd_redirect(&first_tok)? {
            Some(redirect) => redirects.push(redirect),
            None => words.push(word_node_from_token(first_tok)),
        }
        self.collect_coproc_words(words, redirects)
    }

    /// Appends the remaining words (assignment-shaped ones included, as
    /// they are ordinary arguments here) and redirects of a synthetic
    /// coproc command.
    fn collect_coproc_words(
        &mut self,
        mut words: Vec<Node>,
        mut redirects: Vec<Node>,
    ) -> Result<(Vec<Node>, Vec<Node>)> {
        loop {
            if self.at_end()? {
                break;
            }
            if self.is_redirect_operator()? {
                redirects.push(self.parse_redirect()?);
                continue;
            }
            let tok = self.lexer.peek_token()?;
            if !matches!(
                tok.kind,
                TokenType::Word | TokenType::Number | TokenType::AssignmentWord
            ) {
                break;
            }
            let tok = self.lexer.next_token()?;
            match self.try_parse_fd_redirect(&tok)? {
                Some(redirect) => redirects.push(redirect),
                None => words.push(word_node_from_token(tok)),
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
        let expression = parse_arith_expression(&content).ok().map(Box::new);
        Ok(self.spanned(
            start,
            NodeKind::ArithmeticCommand {
                expression,
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
/// `coproc` clause: a compound command or a function definition. Not
/// `coproc`, `time` or `!`, which would re-enter `parse_coproc` or make
/// the negation ambiguous.
const fn coproc_starts_command(kind: TokenType) -> bool {
    kind.starts_compound_command() || matches!(kind, TokenType::Function)
}
