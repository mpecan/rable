use std::ops::ControlFlow;

use crate::context::CaseTracker;
use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::{QuotingContext, WordBuilder, WordSpanKind};

/// Per-loop state for `read_matched_parens_inner`: paren depth for the
/// matching-close decision, a `CaseTracker` for `case X in pat) ... ;;`
/// pattern recognition inside the parens, and a running word buffer the
/// case tracker peeks at to spot keywords.
struct ParenLoopState {
    depth: usize,
    case: CaseTracker,
    word_buf: String,
}

impl ParenLoopState {
    fn new(close_count: usize) -> Self {
        Self {
            depth: close_count,
            case: CaseTracker::default(),
            word_buf: String::new(),
        }
    }

    /// Close the current word: let the case tracker consume what's been
    /// accumulated, then reset the buffer. Called by every structural
    /// arm that terminates a word (`)`, `(`, `'`, `"`, `#`, `;`,
    /// whitespace, `|`). Backslash escapes, `$` expansions, and
    /// backticks clear the buffer without calling the tracker — they
    /// are transparent to case-pattern keyword recognition.
    fn end_word(&mut self) {
        self.case.check_word(&self.word_buf);
        self.word_buf.clear();
    }
}

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

    fn read_matched_parens_inner(
        &mut self,
        wb: &mut WordBuilder,
        close_count: usize,
    ) -> Result<()> {
        // Remaining callers: arithmetic `$((...))` (close_count == 2) and
        // extglob patterns `@(...)`, `?(...)`, etc. (close_count == 1).
        // Neither contains heredocs — `$(...)` and `<(...)`/`>(...)` both
        // fork the real grammar via `parse_paren_body` instead.
        let mut state = ParenLoopState::new(close_count);
        loop {
            match self.peek_char() {
                Some(')') => {
                    if self.handle_paren_close(wb, &mut state).is_break() {
                        return Ok(());
                    }
                }
                Some('(') => self.handle_paren_open(wb, &mut state),
                Some(c @ ('\'' | '"')) => self.read_paren_quote(wb, &mut state, c)?,
                Some('\\') => self.handle_paren_escape(wb, &mut state),
                Some('$') => {
                    state.word_buf.clear();
                    self.read_dollar(wb)?;
                }
                Some('`') => {
                    state.word_buf.clear();
                    self.advance_char();
                    wb.push('`');
                    self.read_backtick(wb)?;
                }
                Some('#') => self.handle_paren_comment(wb, &mut state),
                Some(';') => self.handle_paren_semi(wb, &mut state),
                Some(c @ (' ' | '\t' | '\n' | '|')) => {
                    self.handle_paren_word_boundary(wb, &mut state, c);
                }
                Some(c) => {
                    state.word_buf.push(c);
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

    /// Handles `)` inside `read_matched_parens_inner`. Returns
    /// `ControlFlow::Break(())` when depth hits zero so the caller
    /// can `return Ok(())` from the outer loop; otherwise
    /// `ControlFlow::Continue(())`.
    ///
    /// The `case.is_pattern_close()` branch is **defensive**: the
    /// current callers (`$((…))` with `close_count=2` and extglob
    /// `@(…)`/`?(…)` with `close_count=1`) cannot contain a valid
    /// `case X in pat) ... ;;` construct, so this branch is not
    /// exercised in practice. It is preserved for symmetry with
    /// `handle_paren_open`/`handle_paren_semi` and in case this
    /// reader is ever reused in a context where case patterns
    /// can legally appear inside the parens.
    fn handle_paren_close(
        &mut self,
        wb: &mut WordBuilder,
        state: &mut ParenLoopState,
    ) -> ControlFlow<()> {
        state.end_word();
        self.advance_char();
        wb.push(')');
        if state.case.is_pattern_close() {
            state.case.close_pattern();
            ControlFlow::Continue(())
        } else {
            state.depth -= 1;
            if state.depth == 0 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }

    /// Handles `(` inside `read_matched_parens_inner`. Increments
    /// depth unless we're opening a case-pattern's optional leading
    /// `(` (e.g. `case x in (a) ...`), which is balanced by its own
    /// closing `)`.
    ///
    /// The `is_pattern_open()` branch is defensive — see the note
    /// on `handle_paren_close`.
    fn handle_paren_open(&mut self, wb: &mut WordBuilder, state: &mut ParenLoopState) {
        state.end_word();
        self.advance_char();
        wb.push('(');
        if !state.case.is_pattern_open() {
            state.depth += 1;
        }
    }

    /// Handles `'` or `"` inside `read_matched_parens_inner`.
    /// Dispatches to the appropriate quoted-string reader.
    fn read_paren_quote(
        &mut self,
        wb: &mut WordBuilder,
        state: &mut ParenLoopState,
        quote: char,
    ) -> Result<()> {
        state.end_word();
        self.advance_char();
        wb.push(quote);
        if quote == '\'' {
            self.read_single_quoted(wb)
        } else {
            self.read_double_quoted(wb)
        }
    }

    /// Handles `\` inside `read_matched_parens_inner`. Backslash-newline
    /// is a line continuation — both chars consumed, neither pushed.
    /// Any other `\<c>` sequence preserves both chars verbatim.
    ///
    /// Unlike most arms this only clears `word_buf` without calling
    /// `case.check_word` — backslash escapes are transparent to the
    /// case-pattern keyword tracker.
    fn handle_paren_escape(&mut self, wb: &mut WordBuilder, state: &mut ParenLoopState) {
        state.word_buf.clear();
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

    /// Handles `#` inside `read_matched_parens_inner`. `#` starts a
    /// comment only when preceded by whitespace (or at the very start
    /// of the content — treated as newline-preceded). Otherwise `#`
    /// is a literal character.
    fn handle_paren_comment(&mut self, wb: &mut WordBuilder, state: &mut ParenLoopState) {
        state.end_word();
        let prev = wb.value.chars().last().unwrap_or('\n');
        if prev == '\n' || prev == ' ' || prev == '\t' {
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

    /// Handles `;` inside `read_matched_parens_inner`, including the
    /// `;;`, `;&`, and `;;&` case-terminator variants. Any of the
    /// three extended forms re-enters case-pattern mode so the next
    /// pattern can be recognized.
    ///
    /// The `resume_pattern()` calls are defensive — see the note
    /// on `handle_paren_close`. The literal-semicolon push happens
    /// unconditionally and IS exercised (e.g. `$((i++; j--))`).
    fn handle_paren_semi(&mut self, wb: &mut WordBuilder, state: &mut ParenLoopState) {
        state.end_word();
        self.advance_char();
        wb.push(';');
        if self.peek_char() == Some(';') {
            self.advance_char();
            wb.push(';');
            if self.peek_char() == Some('&') {
                self.advance_char();
                wb.push('&');
            }
            state.case.resume_pattern();
        } else if self.peek_char() == Some('&') {
            self.advance_char();
            wb.push('&');
            state.case.resume_pattern();
        }
    }

    /// Handles word-boundary characters (space, tab, newline, `|`)
    /// inside `read_matched_parens_inner`. All four terminate the
    /// current word-buf, consume the char, and push it verbatim.
    fn handle_paren_word_boundary(
        &mut self,
        wb: &mut WordBuilder,
        state: &mut ParenLoopState,
        c: char,
    ) {
        state.end_word();
        self.advance_char();
        wb.push(c);
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
        let (end_pos, end_line) = match crate::parser::parse_backtick_body(self, outer_depth) {
            Ok(r) => r,
            Err(_) => self.scan_backtick_opaque(body_start)?,
        };
        wb.value
            .extend(self.input[body_start..end_pos].iter().copied());
        self.pos = end_pos;
        self.line = end_line;
        Ok(())
    }

    /// Raw scan for the closing backtick, used as a fallback when
    /// `parse_backtick_body` rejects the body. Bash treats a backtick
    /// body as a single word token at the initial lexing stage — errors
    /// inside are runtime, not parse, concerns. Issue #38.
    ///
    /// Only recognizes `\<x>` as a two-byte escape (so an escaped
    /// `` ` `` does not terminate). Returns `(end_pos, end_line)` with
    /// `end_pos` one past the closing backtick; errors with
    /// `MatchedPair` when EOF is reached first.
    fn scan_backtick_opaque(&self, body_start: usize) -> Result<(usize, usize)> {
        let mut pos = body_start;
        let mut line = self.line;
        while let Some(c) = self.input.get(pos).copied() {
            match c {
                '\\' => {
                    pos += 1;
                    if let Some(next) = self.input.get(pos).copied() {
                        if next == '\n' {
                            line += 1;
                        }
                        pos += 1;
                    }
                }
                '`' => return Ok((pos + 1, line)),
                '\n' => {
                    line += 1;
                    pos += 1;
                }
                _ => pos += 1,
            }
        }
        Err(RableError::matched_pair("unterminated backtick", pos, line))
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
