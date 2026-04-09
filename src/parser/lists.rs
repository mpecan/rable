//! Operator precedence chain: lists (`;`/`\n`), background (`&`),
//! and/or (`&&`/`||`), and pipelines (`|`/`|&`, plus `!` and `time`).

use crate::ast::{ListItem, ListOperator, Node, NodeKind, PipeSep, Span};
use crate::error::Result;
use crate::token::TokenType;

use super::Parser;
use super::helpers::{add_stderr_redirect, make_stderr_redirect, word_node_from_token};

/// Creates a binary list node: `left op right`.
fn make_list(left: Node, op: ListOperator, right: Node) -> Node {
    let span = Span::new(left.span.start, right.span.end);
    Node::new(
        NodeKind::List {
            items: vec![
                ListItem {
                    command: left,
                    operator: Some(op),
                },
                ListItem {
                    command: right,
                    operator: None,
                },
            ],
        },
        span,
    )
}

/// Creates a trailing-operator list node: `left op` (no RHS).
fn make_trailing(left: Node, op: ListOperator, end: usize) -> Node {
    let span = Span::new(left.span.start, end);
    Node::new(
        NodeKind::List {
            items: vec![ListItem {
                command: left,
                operator: Some(op),
            }],
        },
        span,
    )
}

impl Parser {
    /// Like `parse_list` but stops at newlines — used only at the top level so
    /// that newline-separated commands become separate nodes.
    pub(super) fn parse_top_level_list(&mut self) -> Result<Node> {
        self.enter()?;
        let mut left = self.parse_top_level_background()?;

        loop {
            if self.at_end()? {
                break;
            }
            let prev_pos = self.lexer.pos();
            if self.lexer.peek_token()?.kind != TokenType::Semi {
                break;
            }
            self.lexer.next_token()?;
            self.skip_newlines()?;
            if self.at_end()? || self.is_list_terminator()? {
                break;
            }
            let right = self.parse_background()?;
            left = make_list(left, ListOperator::Semi, right);
            if self.lexer.pos() == prev_pos {
                break;
            }
        }

        self.leave();
        Ok(left)
    }

    /// Parses a command list. Precedence: `;`/`\n` < `&` < `&&`/`||` < `|`.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on syntax errors or unclosed delimiters.
    pub fn parse_list(&mut self) -> Result<Node> {
        self.enter()?;
        let mut left = self.parse_background()?;

        loop {
            if self.at_end()? {
                break;
            }
            let prev_pos = self.lexer.pos();
            let kind = self.lexer.peek_token()?.kind;
            if !matches!(kind, TokenType::Semi | TokenType::Newline) {
                break;
            }
            self.lexer.next_token()?;
            self.skip_newlines()?;
            if self.at_end()? || self.is_list_terminator()? {
                break;
            }
            let right = self.parse_background()?;
            left = make_list(left, ListOperator::Semi, right);
            if self.lexer.pos() == prev_pos {
                break;
            }
        }

        self.leave();
        Ok(left)
    }

    /// Top-level variant: does not swallow newlines after `&`, so a `&\n`
    /// leaves the newline for `parse_all` to start the next top-level node.
    fn parse_top_level_background(&mut self) -> Result<Node> {
        self.parse_background_inner(true)
    }

    fn parse_background(&mut self) -> Result<Node> {
        self.parse_background_inner(false)
    }

    /// Shared background-operator loop. `top_level = true` preserves post-`&`
    /// newlines; `false` skips them so `&\n` is treated like `&` followed by
    /// the next command.
    fn parse_background_inner(&mut self, top_level: bool) -> Result<Node> {
        let mut left = self.parse_and_or()?;

        loop {
            if self.at_end()? {
                break;
            }
            if self.lexer.peek_token()?.kind != TokenType::Ampersand {
                break;
            }
            self.lexer.next_token()?;
            if !top_level {
                self.skip_newlines()?;
            }
            if self.at_end()? || self.is_list_terminator()? {
                left = make_trailing(left, ListOperator::Background, self.lexer.pos());
                break;
            }
            // At top level a raw newline after `&` ends the current node;
            // in nested contexts newlines have already been skipped, so only
            // a Semi is left as a "trailing" marker. Matching both is harmless.
            let peek_kind = self.lexer.peek_token()?.kind;
            if matches!(peek_kind, TokenType::Semi | TokenType::Newline) {
                left = make_trailing(left, ListOperator::Background, self.lexer.pos());
                break;
            }
            let right = self.parse_and_or()?;
            left = make_list(left, ListOperator::Background, right);
        }

        Ok(left)
    }

    fn parse_and_or(&mut self) -> Result<Node> {
        let mut left = self.parse_pipeline()?;

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::And => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    let right = self.parse_pipeline()?;
                    left = make_list(left, ListOperator::And, right);
                }
                TokenType::Or => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    let right = self.parse_pipeline()?;
                    left = make_list(left, ListOperator::Or, right);
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        if self.lexer.peek_token()?.kind == TokenType::Bang {
            self.lexer.next_token()?;
            if self.lexer.peek_token()?.kind == TokenType::Bang {
                self.lexer.next_token()?;
                return self.parse_pipeline_inner();
            }
            let inner = self.parse_pipeline()?;
            return Ok(self.spanned(
                start,
                NodeKind::Negation {
                    pipeline: Box::new(inner),
                },
            ));
        }

        if self.lexer.peek_token()?.kind == TokenType::Time {
            self.lexer.next_token()?;
            let posix = if self.check_word("-p")? {
                self.lexer.next_token()?;
                true
            } else {
                false
            };
            if self.lexer.peek_token()?.kind == TokenType::Bang {
                self.lexer.next_token()?;
                let p = self.parse_pipeline_inner()?;
                return Ok(self.spanned(
                    start,
                    NodeKind::Negation {
                        pipeline: Box::new(self.spanned(
                            start,
                            NodeKind::Time {
                                pipeline: Box::new(p),
                                posix,
                            },
                        )),
                    },
                ));
            }
            let inner = self.parse_pipeline_inner()?;
            return Ok(self.spanned(
                start,
                NodeKind::Time {
                    pipeline: Box::new(inner),
                    posix,
                },
            ));
        }

        self.parse_pipeline_inner()
    }

    fn parse_pipeline_inner(&mut self) -> Result<Node> {
        let mut commands = vec![self.parse_command()?];
        let mut separators = Vec::new();

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::Pipe => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    separators.push(PipeSep::Pipe);
                    commands.push(self.parse_pipeline_command()?);
                }
                TokenType::PipeBoth => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    separators.push(PipeSep::PipeBoth);
                    if !add_stderr_redirect(commands.last_mut()) {
                        commands.push(make_stderr_redirect());
                    }
                    commands.push(self.parse_pipeline_command()?);
                }
                _ => break,
            }
        }

        if commands.len() == 1 {
            Ok(commands.remove(0))
        } else {
            let span = Span::new(
                commands.first().map_or(0, |c| c.span.start),
                commands.last().map_or(0, |c| c.span.end),
            );
            Ok(Node::new(
                NodeKind::Pipeline {
                    commands,
                    separators,
                },
                span,
            ))
        }
    }

    fn check_word(&mut self, expected: &str) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(tok.kind == TokenType::Word && tok.value == expected)
    }

    /// Parse a command after `|` in a pipeline — `time` is a regular word here.
    fn parse_pipeline_command(&mut self) -> Result<Node> {
        self.enter()?;
        let start = self.peek_pos()?;
        let tok = self.lexer.peek_token()?;
        let is_time = tok.kind == TokenType::Time;
        let result = if is_time {
            // After |, time is a regular word, not a keyword.
            // Temporarily demote it and all following reserved words to words.
            let time_tok = self.lexer.next_token()?;
            let mut words = vec![word_node_from_token(time_tok)];
            // Check for -p flag (also a word in this context)
            if self.check_word("-p")? {
                let p_tok = self.lexer.next_token()?;
                words.push(word_node_from_token(p_tok));
            }
            // Consume all remaining words (including reserved words as plain words)
            let mut redirects = Vec::new();
            loop {
                if self.at_end()? {
                    break;
                }
                let t = self.lexer.peek_token()?;
                if matches!(
                    t.kind,
                    TokenType::Pipe
                        | TokenType::PipeBoth
                        | TokenType::Semi
                        | TokenType::Newline
                        | TokenType::Ampersand
                        | TokenType::And
                        | TokenType::Or
                        | TokenType::Eof
                        | TokenType::RightParen
                ) {
                    break;
                }
                if self.is_redirect_operator()? {
                    redirects.push(self.parse_redirect()?);
                    continue;
                }
                let t = self.lexer.next_token()?;
                words.push(word_node_from_token(t));
            }
            Ok(self.spanned(
                start,
                NodeKind::Command {
                    assignments: Vec::new(),
                    words,
                    redirects,
                },
            ))
        } else {
            self.parse_command_inner()
        };
        self.leave();
        result
    }
}
