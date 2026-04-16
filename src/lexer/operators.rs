use crate::token::{Token, TokenType};

use super::Lexer;

impl Lexer {
    /// Builds a token that terminates the previous simple command — any
    /// list-separator operator (`|`, `||`, `|&`, `&`, `&&`, `;`, `;;`,
    /// `;&`, `;;&`). Sets `command_start` and re-arms reserved-word
    /// recognition for the token that follows. File-redirect operators
    /// (`<<`, `>>`, `<&`, `&>`, etc.) use plain `Token::new` instead
    /// because they do not start a new simple command.
    fn command_token(
        &mut self,
        kind: TokenType,
        text: &'static str,
        start: usize,
        line: usize,
    ) -> Token {
        self.set_command_start();
        Token::new(kind, text, start, line)
    }

    pub(super) fn read_pipe_operator(&mut self, start: usize, line: usize) -> Token {
        self.advance_char(); // consume '|'
        match self.peek_char() {
            Some('|') => {
                self.advance_char();
                self.command_token(TokenType::Or, "||", start, line)
            }
            Some('&') => {
                self.advance_char();
                self.command_token(TokenType::PipeBoth, "|&", start, line)
            }
            _ => self.command_token(TokenType::Pipe, "|", start, line),
        }
    }

    pub(super) fn read_ampersand_operator(&mut self, start: usize, line: usize) -> Token {
        self.advance_char(); // consume '&'
        match self.peek_char() {
            Some('&') => {
                self.advance_char();
                self.command_token(TokenType::And, "&&", start, line)
            }
            Some('>') => {
                self.advance_char();
                if self.peek_char() == Some('>') {
                    self.advance_char();
                    Token::new(TokenType::AndDoubleGreater, "&>>", start, line)
                } else {
                    Token::new(TokenType::AndGreater, "&>", start, line)
                }
            }
            _ => self.command_token(TokenType::Ampersand, "&", start, line),
        }
    }

    pub(super) fn read_semicolon_operator(&mut self, start: usize, line: usize) -> Token {
        self.advance_char(); // consume ';'
        match self.peek_char() {
            Some(';') => {
                self.advance_char();
                if self.peek_char() == Some('&') {
                    self.advance_char();
                    self.command_token(TokenType::SemiSemiAnd, ";;&", start, line)
                } else {
                    self.command_token(TokenType::DoubleSemi, ";;", start, line)
                }
            }
            Some('&') => {
                self.advance_char();
                self.command_token(TokenType::SemiAnd, ";&", start, line)
            }
            _ => self.command_token(TokenType::Semi, ";", start, line),
        }
    }

    pub(super) fn read_less_operator(&mut self, start: usize, line: usize) -> Token {
        self.advance_char(); // consume '<'
        match self.peek_char() {
            Some('<') => {
                self.advance_char();
                match self.peek_char() {
                    Some('-') => {
                        self.advance_char();
                        Token::new(TokenType::DoubleLessDash, "<<-", start, line)
                    }
                    Some('<') => {
                        self.advance_char();
                        Token::new(TokenType::TripleLess, "<<<", start, line)
                    }
                    _ => Token::new(TokenType::DoubleLess, "<<", start, line),
                }
            }
            Some('&') => {
                self.advance_char();
                // <&- is close-fd (complete operator)
                if self.peek_char() == Some('-') {
                    self.advance_char();
                    Token::new(TokenType::LessAnd, "<&-", start, line)
                } else {
                    Token::new(TokenType::LessAnd, "<&", start, line)
                }
            }
            Some('>') => {
                self.advance_char();
                Token::new(TokenType::LessGreater, "<>", start, line)
            }
            _ => Token::new(TokenType::Less, "<", start, line),
        }
    }

    pub(super) fn read_greater_operator(&mut self, start: usize, line: usize) -> Token {
        self.advance_char(); // consume '>'
        match self.peek_char() {
            Some('>') => {
                self.advance_char();
                Token::new(TokenType::DoubleGreater, ">>", start, line)
            }
            Some('&') => {
                self.advance_char();
                // >&- is close-fd (complete operator)
                if self.peek_char() == Some('-') {
                    self.advance_char();
                    Token::new(TokenType::GreaterAnd, ">&-", start, line)
                } else {
                    Token::new(TokenType::GreaterAnd, ">&", start, line)
                }
            }
            Some('|') => {
                self.advance_char();
                Token::new(TokenType::GreaterPipe, ">|", start, line)
            }
            _ => Token::new(TokenType::Greater, ">", start, line),
        }
    }
}
