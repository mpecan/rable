use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::{QuotingContext, WordBuilder, WordSpanKind};

impl Lexer {
    /// Reads a dollar expansion into the value string.
    pub(super) fn read_dollar(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let span_start = wb.span_start();
        self.advance_char(); // consume '$'
        wb.push('$');

        match self.peek_char() {
            Some('(') => {
                self.advance_char();
                wb.push('(');
                if self.peek_char() == Some('(') {
                    self.advance_char();
                    wb.push('(');
                    self.read_matched_parens(wb, 2, true)?;
                    wb.record(span_start, WordSpanKind::ArithmeticSub);
                } else {
                    self.read_command_sub(wb)?;
                    wb.record(span_start, WordSpanKind::CommandSub);
                }
            }
            Some('{') => {
                self.advance_char();
                wb.push('{');
                // `${ cmd; }` / `${| cmd; }`: bash 5.3 command substitution.
                if let Some(marker @ (' ' | '\t' | '\n' | '|')) = self.peek_char() {
                    if marker == '|' {
                        self.advance_char();
                        wb.push('|');
                    }
                    self.read_funsub_body(wb)?;
                    wb.record(span_start, WordSpanKind::FunSub(marker == '|'));
                } else {
                    self.read_param_expansion_braces(wb)?;
                    wb.record(span_start, WordSpanKind::ParamExpansion);
                }
            }
            Some('\'') => {
                self.advance_char();
                wb.push('\'');
                self.read_ansi_c_quoted(wb)?;
                wb.record(span_start, WordSpanKind::AnsiCQuote);
            }
            Some('"') => {
                self.advance_char();
                wb.push('"');
                self.read_double_quoted(wb)?;
                wb.record(span_start, WordSpanKind::LocaleString);
            }
            Some('[') => {
                self.advance_char();
                wb.push('[');
                self.read_deprecated_arith(wb)?;
                wb.record(span_start, WordSpanKind::DeprecatedArith);
            }
            Some(c) if is_dollar_start(c) => {
                self.read_variable_name(wb, c);
                wb.record(span_start, WordSpanKind::SimpleVar);
            }
            _ => {} // Bare $ — no span
        }
        Ok(())
    }

    /// Reads `$(...)` by forking a fresh parser over the shared source
    /// buffer. The fork consumes up to and including the matching `)`;
    /// the consumed range is then copied into `wb.value`. Precondition:
    /// `read_dollar` has already pushed `$(`.
    fn read_command_sub(&mut self, wb: &mut WordBuilder) -> Result<()> {
        self.read_paren_body_forked(wb)
    }

    /// Reads a parenthesized command-list body (terminated by `)`) using
    /// fork-and-merge, extending `wb.value` with the consumed source range
    /// including the closing `)`. Used by both `$(...)` (command
    /// substitution) and `<(...)`/`>(...)` (process substitution).
    /// Precondition: the opening `(` has just been consumed.
    pub(super) fn read_paren_body_forked(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let body_start = self.pos;
        let outer_depth = self.parser_depth();
        let (end_pos, end_line) = crate::parser::parse_paren_body(self, outer_depth)?;
        wb.value
            .extend(self.input[body_start..end_pos].iter().copied());
        self.pos = end_pos;
        self.line = end_line;
        Ok(())
    }

    /// Reads the body of `${ …; }` / `${| …; }` by forking the real
    /// grammar, like `$(…)`, up to and including its closing `}`.
    fn read_funsub_body(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let body_start = self.pos;
        let outer_depth = self.parser_depth();
        let (end_pos, end_line) = crate::parser::parse_funsub_body(self, outer_depth)?;
        wb.value
            .extend(self.input[body_start..end_pos].iter().copied());
        self.pos = end_pos;
        self.line = end_line;
        Ok(())
    }

    /// Reads a variable name after `$` for simple expansions.
    pub(super) fn read_variable_name(&mut self, wb: &mut WordBuilder, first: char) {
        if first.is_ascii_alphabetic() || first == '_' {
            while let Some(nc) = self.peek_char() {
                if nc.is_ascii_alphanumeric() || nc == '_' {
                    self.advance_char();
                    wb.push(nc);
                } else {
                    break;
                }
            }
        } else {
            self.advance_char();
            wb.push(first);
        }
    }

    /// Reads up to the matching closing parenthesis (`close_count` of them
    /// for `$((`), as bash's `parse_matched_pair` does for `$(( … ))`
    /// (`arith`) and extglob patterns: only parentheses are counted, and
    /// `#` is an ordinary character, never a comment. Quotes, escapes,
    /// `$…` expansions and backticks are read whole, except that inside
    /// arithmetic `${` opens an expansion only as a `${ …; }` / `${| …; }`
    /// command substitution, as bash's `P_ARITH` scan does (`$((${x))` is
    /// a valid word).
    pub(super) fn read_matched_parens(
        &mut self,
        wb: &mut WordBuilder,
        close_count: usize,
        arith: bool,
    ) -> Result<()> {
        wb.enter_context(QuotingContext::CommandSub);
        let result = self.read_matched_parens_inner(wb, close_count, arith);
        wb.leave_context();
        result
    }

    fn read_matched_parens_inner(
        &mut self,
        wb: &mut WordBuilder,
        close_count: usize,
        arith: bool,
    ) -> Result<()> {
        let mut depth = close_count;
        while let Some(c) = self.peek_char() {
            match c {
                '\\' => self.read_paren_escape(wb),
                '\'' | '"' => {
                    self.advance_char();
                    wb.push(c);
                    if c == '\'' {
                        self.read_single_quoted(wb)?;
                    } else {
                        self.read_double_quoted(wb)?;
                    }
                }
                '$' if !(arith && self.at_plain_dollar_brace()) => self.read_dollar(wb)?,
                '`' => self.read_backtick_sub(wb)?,
                _ => {
                    self.advance_char();
                    wb.push(c);
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Err(RableError::matched_pair(
            "unterminated parenthesis",
            self.pos,
            self.line,
        ))
    }

    /// True at a `${` that is not a `${ …; }` / `${| …; }` command
    /// substitution (bash's `FUNSUB_CHAR`).
    fn at_plain_dollar_brace(&self) -> bool {
        self.input.get(self.pos + 1) == Some(&'{')
            && !matches!(self.input.get(self.pos + 2), Some(' ' | '\t' | '\n' | '|'))
    }

    /// Reads `\<c>` verbatim; a backslash-newline is a line continuation.
    fn read_paren_escape(&mut self, wb: &mut WordBuilder) {
        self.advance_char();
        if self.peek_char() == Some('\n') {
            self.advance_char();
        } else {
            wb.push('\\');
            if let Some(c) = self.advance_char() {
                wb.push(c);
            } else {
                wb.push('\\');
            }
        }
    }

    /// Reads a parameter expansion `${...}` allowing unbalanced inner `{`.
    pub(super) fn read_param_expansion_braces(&mut self, wb: &mut WordBuilder) -> Result<()> {
        wb.enter_context(QuotingContext::ParamExpansion);
        let result = self.read_param_expansion_inner(wb);
        wb.leave_context();
        result
    }

    fn read_param_expansion_inner(&mut self, wb: &mut WordBuilder) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('}') => {
                    self.advance_char();
                    wb.push('}');
                    return Ok(());
                }
                Some('\'') => {
                    self.advance_char();
                    wb.push('\'');
                    self.read_single_quoted(wb)?;
                }
                Some('"') => {
                    self.advance_char();
                    wb.push('"');
                    self.read_double_quoted(wb)?;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char(); // line continuation
                    } else {
                        wb.push('\\');
                        if let Some(c) = self.advance_char() {
                            wb.push(c);
                        } else {
                            wb.push('\\');
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(wb)?;
                }
                Some('`') => {
                    self.read_backtick_sub(wb)?;
                }
                Some(c) => {
                    self.advance_char();
                    wb.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated parameter expansion",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads deprecated `$[...]` arithmetic with bracket depth tracking.
    fn read_deprecated_arith(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let mut depth = 1;
        loop {
            match self.advance_char() {
                Some('[') => {
                    depth += 1;
                    wb.push('[');
                }
                Some(']') => {
                    depth -= 1;
                    wb.push(']');
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(c) => wb.push(c),
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated '$['",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }
}

/// Returns true if the character can follow `$` to start a variable expansion.
pub(super) const fn is_dollar_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '*' | '#' | '?' | '-' | '$' | '!')
}
