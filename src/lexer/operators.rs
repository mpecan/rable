use crate::error::Result;
use crate::token::{Token, TokenType};

use super::Lexer;

impl Lexer {
    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn read_pipe_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '|'
        match self.peek_char() {
            Some('|') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::Or, "||", start, line))
            }
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::PipeBoth, "|&", start, line))
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Pipe, "|", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn read_ampersand_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '&'
        match self.peek_char() {
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::And, "&&", start, line))
            }
            Some('>') => {
                self.advance_char();
                if self.peek_char() == Some('>') {
                    self.advance_char();
                    Ok(Token::new(TokenType::AndDoubleGreater, "&>>", start, line))
                } else {
                    Ok(Token::new(TokenType::AndGreater, "&>", start, line))
                }
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Ampersand, "&", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn read_semicolon_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume ';'
        match self.peek_char() {
            Some(';') => {
                self.advance_char();
                self.state.command_start = true;
                if self.peek_char() == Some('&') {
                    self.advance_char();
                    Ok(Token::new(TokenType::SemiSemiAnd, ";;&", start, line))
                } else {
                    Ok(Token::new(TokenType::DoubleSemi, ";;", start, line))
                }
            }
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::SemiAnd, ";&", start, line))
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Semi, ";", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn read_less_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '<'
        match self.peek_char() {
            Some('<') => {
                self.advance_char();
                match self.peek_char() {
                    Some('-') => {
                        self.advance_char();
                        Ok(Token::new(TokenType::DoubleLessDash, "<<-", start, line))
                    }
                    Some('<') => {
                        self.advance_char();
                        Ok(Token::new(TokenType::TripleLess, "<<<", start, line))
                    }
                    _ => Ok(Token::new(TokenType::DoubleLess, "<<", start, line)),
                }
            }
            Some('&') => {
                self.advance_char();
                Ok(Token::new(TokenType::LessAnd, "<&", start, line))
            }
            Some('>') => {
                self.advance_char();
                Ok(Token::new(TokenType::LessGreater, "<>", start, line))
            }
            _ => Ok(Token::new(TokenType::Less, "<", start, line)),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn read_greater_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '>'
        match self.peek_char() {
            Some('>') => {
                self.advance_char();
                Ok(Token::new(TokenType::DoubleGreater, ">>", start, line))
            }
            Some('&') => {
                self.advance_char();
                Ok(Token::new(TokenType::GreaterAnd, ">&", start, line))
            }
            Some('|') => {
                self.advance_char();
                Ok(Token::new(TokenType::GreaterPipe, ">|", start, line))
            }
            _ => Ok(Token::new(TokenType::Greater, ">", start, line)),
        }
    }
}
