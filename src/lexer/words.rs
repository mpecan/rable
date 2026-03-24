use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::Lexer;

impl Lexer {
    /// Reads a word token, handling quoting and expansions.
    #[allow(clippy::too_many_lines)]
    pub(super) fn read_word_token(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut value = String::new();

        while let Some(c) = self.peek_char() {
            match c {
                // Metacharacters end a word
                ' ' | '\t' | '\n' | '|' | '&' | ';' | ')' => break,
                // < and > are metacharacters, but <( and >( are process substitution
                '<' | '>' => {
                    if !value.is_empty() && self.input.get(self.pos + 1) == Some(&'(') {
                        // Process substitution mid-word: cat<(cmd)
                        self.read_process_sub_into(&mut value)?;
                    } else {
                        break;
                    }
                }
                // ( is a metacharacter UNLESS preceded by = (array) or extglob prefix
                '(' => {
                    if value.ends_with('=') {
                        // Array assignment: arr=(...)
                        self.advance_char();
                        value.push('(');
                        self.read_matched_parens(&mut value, 1)?;
                    } else if value.ends_with('@')
                        || value.ends_with('?')
                        || value.ends_with('+')
                        || (value.ends_with('!') && self.config.extglob)
                        || (value.ends_with('*') && self.config.extglob)
                    {
                        // Extglob: @(...), ?(...), etc.
                        self.advance_char();
                        value.push('(');
                        self.read_matched_parens(&mut value, 1)?;
                    } else {
                        break;
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(&mut value, c)?;
                }
                // Extglob: @(...), ?(...), +(...), !(...)
                // !( is only extglob when extglob is enabled; otherwise ! is Bang
                '@' | '?' | '+' if self.input.get(self.pos + 1) == Some(&'(') => {
                    self.read_extglob(&mut value, c)?;
                }
                // !( and *( are only extglob when extglob mode is enabled
                '!' | '*' if self.input.get(self.pos + 1) == Some(&'(') && self.config.extglob => {
                    self.read_extglob(&mut value, c)?;
                }
                // Regular character
                _ => {
                    self.advance_char();
                    value.push(c);
                }
            }
        }

        if value.is_empty() {
            return Err(RableError::parse("unexpected character", start, line));
        }

        // Check for reserved words at command start
        let kind = if self.ctx.command_start {
            TokenType::reserved_word(&value).unwrap_or(TokenType::Word)
        } else {
            TokenType::Word
        };

        // After a word, we're no longer at command start (unless it's a keyword
        // that expects another command)
        self.ctx.command_start = kind.starts_command()
            || matches!(
                kind,
                TokenType::Then
                    | TokenType::Else
                    | TokenType::Elif
                    | TokenType::Do
                    | TokenType::Semi
            );

        Ok(Token::new(kind, value, start, line))
    }

    /// Reads a quoted string, escape, dollar expansion, or backtick within a word.
    pub(super) fn read_word_special(&mut self, value: &mut String, c: char) -> Result<()> {
        match c {
            '\'' => {
                self.advance_char();
                value.push('\'');
                self.read_single_quoted(value)?;
            }
            '"' => {
                self.advance_char();
                value.push('"');
                self.read_double_quoted(value)?;
            }
            '\\' => {
                self.advance_char();
                if self.peek_char() == Some('\n') {
                    self.advance_char(); // line continuation
                } else {
                    value.push('\\');
                    if let Some(next) = self.advance_char() {
                        value.push(next);
                    }
                }
            }
            '$' => {
                self.read_dollar(value)?;
            }
            '`' => {
                self.advance_char();
                value.push('`');
                self.read_backtick(value)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads an extglob pattern `@(...)`, `?(...)`, etc.
    pub(super) fn read_extglob(&mut self, value: &mut String, prefix: char) -> Result<()> {
        self.advance_char();
        value.push(prefix);
        self.advance_char();
        value.push('(');
        self.read_matched_parens(value, 1)
    }

    /// Reads a process substitution into an existing word value.
    pub(super) fn read_process_sub_into(&mut self, value: &mut String) -> Result<()> {
        let dir = self.advance_char().unwrap_or('<');
        value.push(dir);
        self.advance_char(); // (
        value.push('(');
        self.read_matched_parens(value, 1)
    }

    /// Reads a process substitution `<(...)` or `>(...)` as a word token.
    pub(super) fn read_process_sub_word(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut value = String::new();
        // Read < or >
        let dir = self.advance_char().unwrap_or('<');
        value.push(dir);
        // Read (
        self.advance_char();
        value.push('(');
        // Read until matching )
        self.read_matched_parens(&mut value, 1)?;
        self.ctx.command_start = false;
        Ok(Token::new(TokenType::Word, value, start, line))
    }
}
