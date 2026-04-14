//! Command dispatch: compound-vs-simple routing and simple command parsing.

use crate::ast::{Node, NodeKind, Span};
use crate::error::{RableError, Result};
use crate::token::TokenType;

use super::Parser;
use super::helpers::{is_fd_number, is_varfd};
use super::word_parts;

impl Parser {
    pub(super) fn parse_command(&mut self) -> Result<Node> {
        self.enter()?;
        let result = self.parse_command_inner();
        self.leave();
        result
    }

    pub(super) fn parse_command_inner(&mut self) -> Result<Node> {
        let tok = self.lexer.peek_token()?;
        match tok.kind {
            TokenType::If => self.parse_if(),
            TokenType::While => self.parse_while(),
            TokenType::Until => self.parse_until(),
            TokenType::For => self.parse_for(),
            TokenType::Case => self.parse_case(),
            TokenType::Select => self.parse_select(),
            TokenType::LeftParen => {
                if self.lexer.pos() + 1 < self.lexer.input_len() && self.is_double_paren()? {
                    // Bash resolves `((` ambiguity by trying arithmetic first
                    // and falling back to nested subshells `( ( … ) )` when
                    // the body is not a valid arithmetic expression (#42).
                    // Only a `MatchedPair` error from `read_until_double_paren`
                    // (i.e. no balanced `))`) triggers the fallback — any
                    // other arith-side failure is a real error worth reporting.
                    let cp = self.lexer.checkpoint();
                    match self.parse_arith_command() {
                        Ok(node) => Ok(node),
                        Err(RableError::MatchedPair { .. }) => {
                            self.lexer.restore(cp);
                            self.parse_subshell()
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    self.parse_subshell()
                }
            }
            TokenType::LeftBrace => self.parse_brace_group(),
            TokenType::Function => self.parse_function(),
            TokenType::Coproc => self.parse_coproc(),
            TokenType::DoubleLeftBracket => self.parse_cond_command(),
            // Closing reserved words that cannot start a command
            TokenType::Fi | TokenType::Done | TokenType::Esac => {
                let tok = self.lexer.peek_token()?;
                Err(RableError::parse(
                    format!("unexpected reserved word '{}'", tok.value),
                    tok.pos,
                    tok.line,
                ))
            }
            _ => self.parse_simple_command(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_simple_command(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        let mut assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        let mut saw_command_word = false;

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
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
                | TokenType::TripleLess => {
                    redirects.push(self.parse_redirect()?);
                }
                TokenType::Word | TokenType::AssignmentWord | TokenType::Number => {
                    let is_assignment = tok.kind == TokenType::AssignmentWord;
                    let tok = self.lexer.next_token()?;
                    // fd numbers before redirects — only when adjacent (no space)
                    // and never before &> or &>>
                    let adjacent = self
                        .lexer
                        .peek_token()
                        .map(|next| tok.adjacent_to(next))
                        .unwrap_or(false);
                    if adjacent
                        && is_fd_number(&tok.value)
                        && self.is_redirect_operator()?
                        && !self.is_and_redirect()?
                    {
                        redirects.push(self.parse_redirect_with_fd(&tok)?);
                    } else if adjacent && is_varfd(&tok.value) && self.is_redirect_operator()? {
                        redirects.push(self.parse_redirect()?);
                    } else {
                        if !saw_command_word
                            && assignments.is_empty()
                            && words.is_empty()
                            && self.peek_is(TokenType::LeftParen)?
                        {
                            return self.parse_function_def(&tok);
                        }
                        let word_span = Span::new(tok.pos, tok.pos + tok.value.len());
                        let parts = word_parts::decompose_word_with_spans(&tok.value, &tok.spans);
                        let node = Node::new(
                            NodeKind::Word {
                                value: tok.value,
                                parts,
                                spans: tok.spans,
                            },
                            word_span,
                        );
                        if is_assignment && !saw_command_word {
                            assignments.push(node);
                        } else {
                            saw_command_word = true;
                            words.push(node);
                        }
                    }
                }
                _ => break,
            }
        }

        if assignments.is_empty() && words.is_empty() && redirects.is_empty() {
            return Ok(self.spanned(start, NodeKind::Empty));
        }

        Ok(self.spanned(
            start,
            NodeKind::Command {
                assignments,
                words,
                redirects,
            },
        ))
    }
}
