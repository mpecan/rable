use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::{QuotingContext, WordBuilder};

impl Lexer {
    /// Reads contents of a single-quoted string (after the opening `'`).
    pub(super) fn read_single_quoted(&mut self, wb: &mut WordBuilder) -> Result<()> {
        loop {
            match self.advance_char() {
                Some('\'') => {
                    wb.push('\'');
                    return Ok(());
                }
                Some(c) => wb.push(c),
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
    pub(super) fn read_ansi_c_quoted(&mut self, wb: &mut WordBuilder) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.advance_char();
                    wb.push('\'');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    wb.push('\\');
                    if let Some(next) = self.advance_char() {
                        wb.push(next);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    wb.push(c);
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
    pub(super) fn read_double_quoted(&mut self, wb: &mut WordBuilder) -> Result<()> {
        wb.enter_context(QuotingContext::DoubleQuote);
        let result = self.read_double_quoted_inner(wb);
        wb.leave_context();
        result
    }

    fn read_double_quoted_inner(&mut self, wb: &mut WordBuilder) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance_char();
                    wb.push('"');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    // Line continuation: \<newline> is removed in double quotes
                    if self.peek_char() == Some('\n') {
                        self.advance_char();
                    } else {
                        wb.push('\\');
                        if let Some(next) = self.advance_char() {
                            wb.push(next);
                        }
                    }
                }
                Some('$') => {
                    // Inside double quotes, $" is NOT a locale string — it's
                    // bare $ followed by closing ". Don't let read_dollar consume it.
                    if self.input.get(self.pos + 1) == Some(&'"') {
                        self.advance_char();
                        wb.push('$');
                    } else {
                        self.read_dollar(wb)?;
                    }
                }
                Some('`') => {
                    self.advance_char();
                    wb.push('`');
                    self.read_backtick(wb)?;
                }
                Some(c) => {
                    self.advance_char();
                    wb.push(c);
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
