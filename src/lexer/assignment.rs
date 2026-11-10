//! Assignment words and compound array assignments: `name=…`,
//! `name+=…`, `name[sub]=…`, and the element list of `name=(…)`.

use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::WordBuilder;

impl Lexer {
    /// True if `wb` (ending in `=`) starts a compound array assignment:
    /// a valid assignment prefix, in assignment position (command start,
    /// including after other assignments) or as an argument to an
    /// assignment builtin. `echo a=(x)` and `1a=(x)` are syntax errors.
    pub(super) fn array_assignment_allowed(&self, wb: &WordBuilder) -> bool {
        // The `=` just read must end the prefix: `a=()b=(` is not a second array.
        !self.ctx.redirect_target
            && assignment_prefix_len(&wb.value) == Some(wb.value.len())
            && (self.ctx.command_start || self.ctx.assignment_builtin)
    }

    /// Reads array elements from `(...)`, producing normalized content.
    ///
    /// Parses individual elements separated by whitespace directly from
    /// the input stream, stripping comments. Uses the lexer's existing
    /// word-reading facilities for quoting and expansion handling.
    /// The opening `(` must already be consumed and pushed to `wb`.
    pub(super) fn read_array_elements(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let mut need_space = false;
        loop {
            // Skip whitespace and line continuations between elements
            loop {
                match self.peek_char() {
                    Some(' ' | '\t' | '\n' | '\r') => {
                        self.advance_char();
                    }
                    Some('\\') if self.input.get(self.pos + 1) == Some(&'\n') => {
                        self.advance_char();
                        self.advance_char();
                    }
                    _ => break,
                }
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
    /// as `read_word_token`, but with `)` and whitespace as terminators.
    /// Other unquoted metacharacters are syntax errors, as in bash.
    fn read_array_element(&mut self, wb: &mut WordBuilder) -> Result<()> {
        // A leading `[subscript]` is bracket-matched verbatim, operators
        // and blanks included: `arr=([a;b]=1)` is valid bash.
        if self.peek_char() == Some('[') {
            self.read_bracket_subscript(wb)?;
        }
        while let Some(c) = self.peek_char() {
            match c {
                ' ' | '\t' | '\n' | '\r' | ')' => break,
                '<' | '>' if self.input.get(self.pos + 1) == Some(&'(') => {
                    self.read_process_sub_into(wb)?;
                }
                // Unquoted operators are syntax errors inside `(…)`.
                ';' | '&' | '|' | '(' | '<' | '>' => {
                    return Err(RableError::parse(
                        format!("syntax error near unexpected token '{c}'"),
                        self.pos,
                        self.line,
                    ));
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

/// Builtins whose `name=(…)` arguments bash parses as array assignments.
pub(super) fn is_assignment_builtin(value: &str) -> bool {
    matches!(
        value,
        "alias" | "declare" | "eval" | "export" | "let" | "local" | "readonly" | "typeset"
    )
}

pub(super) fn is_assignment_word(value: &str) -> bool {
    assignment_prefix_len(value).is_some()
}

/// Length in bytes of the `name=`, `name+=` or `name[sub]=` prefix that
/// makes `value` an assignment, or `None` if it is not one.
fn assignment_prefix_len(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    // Must start with [a-zA-Z_]
    if !matches!(bytes.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
        return None;
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
                        b' ' | b'\t' | b'\n' => return None,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'+' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => return Some(i + 2),
            b'=' => return Some(i + 1),
            _ => return None,
        }
    }
    None
}
