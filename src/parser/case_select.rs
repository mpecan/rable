//! `case … esac` and `select … do … done` parsers.

use crate::ast::{CasePattern, Node, NodeKind, Span};
use crate::error::Result;
use crate::token::TokenType;

use super::Parser;
use super::helpers::{word_node, word_node_from_token};
use super::word_parts;

impl Parser {
    pub(super) fn parse_case(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Case)?;
        let word_tok = self.lexer.next_token()?;
        let word = Box::new(Node::new(
            NodeKind::Word {
                parts: word_parts::decompose_word_with_spans(&word_tok.value, &word_tok.spans),
                value: word_tok.value.clone(),
                spans: word_tok.spans,
            },
            Span::new(word_tok.pos, word_tok.pos + word_tok.value.len()),
        ));

        self.lexer.set_command_start();
        self.skip_newlines()?;
        self.expect(TokenType::In)?;
        self.skip_newlines()?;

        let mut patterns = Vec::new();
        self.lexer.set_command_start();
        while !self.peek_is(TokenType::Esac)? && !self.at_end()? {
            patterns.push(self.parse_case_pattern()?);
            self.lexer.set_command_start();
            self.skip_newlines()?;
        }

        self.expect(TokenType::Esac)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(self.spanned(
            start,
            NodeKind::Case {
                word,
                patterns,
                redirects,
            },
        ))
    }

    fn parse_case_pattern(&mut self) -> Result<CasePattern> {
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
        }

        let mut pattern_words = Vec::new();
        loop {
            let tok = self.lexer.next_token()?;
            if tok.kind == TokenType::RightParen || tok.kind == TokenType::Eof {
                break;
            }
            if tok.kind == TokenType::Pipe {
                continue;
            }
            pattern_words.push(word_node_from_token(tok));
        }

        self.skip_newlines()?;

        let body = if self.peek_is(TokenType::DoubleSemi)?
            || self.peek_is(TokenType::SemiAnd)?
            || self.peek_is(TokenType::SemiSemiAnd)?
            || self.peek_is(TokenType::Esac)?
        {
            None
        } else {
            Some(self.parse_list()?)
        };

        let terminator = if self.peek_is(TokenType::DoubleSemi)? {
            self.lexer.next_token()?;
            ";;".to_string()
        } else if self.peek_is(TokenType::SemiAnd)? {
            self.lexer.next_token()?;
            ";&".to_string()
        } else if self.peek_is(TokenType::SemiSemiAnd)? {
            self.lexer.next_token()?;
            ";;&".to_string()
        } else {
            ";;".to_string()
        };

        Ok(CasePattern::new(pattern_words, body, terminator))
    }

    pub(super) fn parse_select(&mut self) -> Result<Node> {
        let start = self.peek_pos()?;
        self.expect(TokenType::Select)?;
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
            NodeKind::Select {
                var,
                words,
                body: Box::new(body),
                redirects,
            },
        ))
    }
}
