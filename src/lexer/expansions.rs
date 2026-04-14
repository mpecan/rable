use crate::context::CaseTracker;
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
                    self.read_matched_parens(wb, 2)?;
                    wb.record(span_start, WordSpanKind::ArithmeticSub);
                } else {
                    self.read_command_sub(wb)?;
                    wb.record(span_start, WordSpanKind::CommandSub);
                }
            }
            Some('{') => {
                self.advance_char();
                wb.push('{');
                self.read_param_expansion_braces(wb)?;
                wb.record(span_start, WordSpanKind::ParamExpansion);
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

    /// Reads until matching closing parentheses.
    #[allow(clippy::too_many_lines)]
    pub(super) fn read_matched_parens(
        &mut self,
        wb: &mut WordBuilder,
        close_count: usize,
    ) -> Result<()> {
        wb.enter_context(QuotingContext::CommandSub);
        let result = self.read_matched_parens_inner(wb, close_count);
        wb.leave_context();
        result
    }

    #[allow(clippy::too_many_lines)]
    fn read_matched_parens_inner(
        &mut self,
        wb: &mut WordBuilder,
        close_count: usize,
    ) -> Result<()> {
        // Remaining callers: arithmetic `$((...))` (close_count == 2) and
        // extglob patterns `@(...)`, `?(...)`, etc. (close_count == 1).
        // Neither contains heredocs — `$(...)` and `<(...)`/`>(...)` both
        // fork the real grammar via `parse_paren_body` instead.
        let mut depth = close_count;
        let mut case = CaseTracker::default();
        let mut word_buf = String::new();

        loop {
            match self.peek_char() {
                Some(')') => {
                    // Check keyword before consuming `)`
                    case.check_word(&word_buf);
                    word_buf.clear();

                    self.advance_char();
                    wb.push(')');
                    if case.is_pattern_close() {
                        // This `)` terminates a case pattern — don't decrement depth
                        case.close_pattern();
                    } else {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    }
                }
                Some('(') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    self.advance_char();
                    wb.push('(');
                    // In case pattern mode, `(` is optional pattern prefix — don't increment
                    if !case.is_pattern_open() {
                        depth += 1;
                    }
                }
                Some('\'') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    self.advance_char();
                    wb.push('\'');
                    self.read_single_quoted(wb)?;
                }
                Some('"') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    self.advance_char();
                    wb.push('"');
                    self.read_double_quoted(wb)?;
                }
                Some('\\') => {
                    word_buf.clear();
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
                    word_buf.clear();
                    self.read_dollar(wb)?;
                }
                Some('`') => {
                    word_buf.clear();
                    self.advance_char();
                    wb.push('`');
                    self.read_backtick(wb)?;
                }
                Some('#') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    // # is a comment when preceded by whitespace/newline
                    let prev = wb.value.chars().last().unwrap_or('\n');
                    if prev == '\n' || prev == ' ' || prev == '\t' {
                        // Comment — skip to end of line
                        while let Some(c) = self.peek_char() {
                            if c == '\n' {
                                break;
                            }
                            self.advance_char();
                        }
                    } else {
                        self.advance_char();
                        wb.push('#');
                    }
                }
                Some(';') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    self.advance_char();
                    wb.push(';');
                    // Check for ;; ;& ;;& which resume case pattern mode
                    if self.peek_char() == Some(';') {
                        self.advance_char();
                        wb.push(';');
                        if self.peek_char() == Some('&') {
                            self.advance_char();
                            wb.push('&');
                        }
                        case.resume_pattern();
                    } else if self.peek_char() == Some('&') {
                        self.advance_char();
                        wb.push('&');
                        case.resume_pattern();
                    }
                }
                Some(' ' | '\t' | '\n') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    let c = self.advance_char().unwrap_or(' ');
                    wb.push(c);
                }
                Some('|') => {
                    case.check_word(&word_buf);
                    word_buf.clear();
                    self.advance_char();
                    wb.push('|');
                }
                Some(c) => {
                    word_buf.push(c);
                    self.advance_char();
                    wb.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated parenthesis",
                        self.pos,
                        self.line,
                    ));
                }
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
                        "unterminated parameter expansion",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a backtick command substitution.
    pub(super) fn read_backtick(&mut self, wb: &mut WordBuilder) -> Result<()> {
        wb.enter_context(QuotingContext::Backtick);
        let result = self.read_backtick_inner(wb);
        wb.leave_context();
        result
    }

    fn read_backtick_inner(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let body_start = self.pos;
        let outer_depth = self.parser_depth();
        let (end_pos, end_line) = crate::parser::parse_backtick_body(self, outer_depth)?;
        wb.value
            .extend(self.input[body_start..end_pos].iter().copied());
        self.pos = end_pos;
        self.line = end_line;
        Ok(())
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
