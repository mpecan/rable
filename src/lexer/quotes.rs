use crate::error::{RableError, Result};

use super::Lexer;

impl Lexer {
    /// Reads contents of a single-quoted string (after the opening `'`).
    pub(super) fn read_single_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.advance_char() {
                Some('\'') => {
                    value.push('\'');
                    return Ok(());
                }
                Some(c) => value.push(c),
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated single quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads contents of an ANSI-C quoted string `$'...'` (after the opening `'`).
    /// Unlike regular single quotes, `\` is an escape character here.
    pub(super) fn read_ansi_c_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.advance_char();
                    value.push('\'');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    value.push('\\');
                    if let Some(next) = self.advance_char() {
                        value.push(next);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated ANSI-C quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads contents of a double-quoted string (after the opening `"`).
    pub(super) fn read_double_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance_char();
                    value.push('"');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    // Line continuation: \<newline> is removed in double quotes
                    if self.peek_char() == Some('\n') {
                        self.advance_char();
                    } else {
                        value.push('\\');
                        if let Some(next) = self.advance_char() {
                            value.push(next);
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated double quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }
}
