//! Assignment words and compound array assignments: `name=…`,
//! `name+=…`, `name[sub]=…`, and the element list of `name=(…)`.

use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::WordBuilder;

impl Lexer {
    /// Reads array elements from `(...)`, producing normalized content.
    ///
    /// Parses individual elements separated by whitespace directly from
    /// the input stream, stripping comments. Uses the lexer's existing
    /// word-reading facilities for quoting and expansion handling.
    /// The opening `(` must already be consumed and pushed to `wb`.
    pub(super) fn read_array_elements(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let mut need_space = false;
        loop {
            // Skip whitespace between elements
            while matches!(self.peek_char(), Some(' ' | '\t' | '\n' | '\r')) {
                self.advance_char();
            }
            match self.peek_char() {
                Some(')') => {
                    self.advance_char();
                    wb.push(')');
                    return Ok(());
                }
                Some('#') => {
                    // Comment — skip to end of line
                    while matches!(self.peek_char(), Some(c) if c != '\n') {
                        self.advance_char();
                    }
                }
                Some(_) => {
                    if need_space {
                        wb.push(' ');
                    }
                    self.read_array_element(wb)?;
                    need_space = true;
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated array",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a single array element using the same word-reading logic
    /// as `read_word_token`, but with `)` and whitespace as terminators
    /// instead of the standard shell metacharacters.
    fn read_array_element(&mut self, wb: &mut WordBuilder) -> Result<()> {
        while let Some(c) = self.peek_char() {
            match c {
                ' ' | '\t' | '\n' | '\r' | ')' => break,
                '<' | '>' => {
                    if self.input.get(self.pos + 1) == Some(&'(') {
                        self.read_process_sub_into(wb)?;
                    } else {
                        self.advance_char();
                        wb.push(c);
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(wb, c)?;
                }
                _ => {
                    self.advance_char();
                    wb.push(c);
                }
            }
        }
        Ok(())
    }
}

/// Returns true if a word value matches the assignment pattern `NAME=`,
/// `NAME+=`, or `NAME[...]=` / `NAME[...]+=`.
pub(super) fn is_assignment_word(value: &str) -> bool {
    let bytes = value.as_bytes();
    // Must start with [a-zA-Z_]
    if !matches!(bytes.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
        return false;
    }
    let mut i = 1;
    // Continue with [a-zA-Z_0-9]
    while i < bytes.len() {
        match bytes[i] {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => i += 1,
            b'[' => {
                // Skip subscript [...] — reject if it contains whitespace
                // (bash doesn't allow spaces in assignment subscripts)
                i += 1;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b' ' | b'\t' | b'\n' => return false,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'+' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => return true,
            b'=' => return true,
            _ => return false,
        }
    }
    false
}
