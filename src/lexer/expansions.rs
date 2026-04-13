use crate::context::CaseTracker;
use crate::error::{RableError, Result};

use super::Lexer;
use super::heredoc::parse_heredoc_delimiter;
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

    /// Reads `$(...)` command substitution content and validates it.
    fn read_command_sub(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let content_start = wb.len();
        self.read_matched_parens(wb, 1)?;
        let content_end = wb.len().saturating_sub(1);
        if content_start < content_end {
            let content = &wb.value[content_start..content_end];
            if !content.trim().is_empty() && crate::parse(content, self.extglob()).is_err() {
                return Err(RableError::parse(
                    "invalid command substitution",
                    self.pos,
                    self.line,
                ));
            }
        }
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
        let mut depth = close_count;
        let mut case = CaseTracker::default();
        let mut word_buf = String::new();
        // Only command substitution (`$(...)`, close_count == 1) queues
        // heredocs; arithmetic `$((...))` (close_count == 2) must keep
        // treating `<<` as the left-shift operator.
        let track_heredocs = close_count == 1;
        let mut pending_heredocs: Vec<(String, bool)> = Vec::new();

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
                    if c == '\n' && !pending_heredocs.is_empty() {
                        let queue = std::mem::take(&mut pending_heredocs);
                        for (delim, strip_tabs) in queue {
                            self.read_heredoc_body_teed(&delim, strip_tabs, true, |ch| {
                                wb.push(ch);
                            });
                        }
                    }
                }
                Some('<') if track_heredocs => {
                    self.advance_char();
                    wb.push('<');
                    if self.peek_char() == Some('<') {
                        // `<<` is a word-terminating operator
                        case.check_word(&word_buf);
                        word_buf.clear();
                        self.advance_char();
                        wb.push('<');
                        if self.peek_char() == Some('<') {
                            // `<<<` herestring — not a heredoc
                            self.advance_char();
                            wb.push('<');
                        } else {
                            // `<<` or `<<-` heredoc
                            self.queue_cmdsub_heredoc(wb, &mut pending_heredocs);
                        }
                    } else {
                        // Bare `<` redirect — match catch-all word_buf behavior
                        word_buf.push('<');
                    }
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

    /// Parses a `<<[-]DELIM` heredoc operator that appears inside a command
    /// substitution, pushing every consumed character verbatim into `wb` and
    /// queuing the normalized delimiter so the body can be drained on the
    /// next newline. The caller has already consumed `<<`.
    fn queue_cmdsub_heredoc(&mut self, wb: &mut WordBuilder, pending: &mut Vec<(String, bool)>) {
        let strip_tabs = self.peek_char() == Some('-');
        if strip_tabs {
            self.advance_char();
            wb.push('-');
        }
        while matches!(self.peek_char(), Some(' ' | '\t')) {
            if let Some(ws) = self.advance_char() {
                wb.push(ws);
            }
        }
        let raw = self.scan_heredoc_delimiter_word(wb);
        if raw.is_empty() {
            return;
        }
        // The `quoted` flag controls whether expansions happen in the body;
        // inside a command substitution we re-emit the body verbatim, so
        // quoting only matters for delimiter normalization (already handled
        // by `parse_heredoc_delimiter`). The flag itself can be dropped.
        let (delim, _quoted) = parse_heredoc_delimiter(&raw);
        if !delim.is_empty() {
            pending.push((delim, strip_tabs));
        }
    }

    /// Scans a heredoc delimiter word from the input, pushing every consumed
    /// character verbatim into `wb` and returning the raw (pre-normalization)
    /// delimiter text so the caller can feed it to `parse_heredoc_delimiter`.
    fn scan_heredoc_delimiter_word(&mut self, wb: &mut WordBuilder) -> String {
        let mut raw = String::new();
        let mut inside_single = false;
        let mut inside_double = false;
        loop {
            match self.peek_char() {
                Some('\'') if !inside_double => {
                    self.advance_char();
                    wb.push('\'');
                    raw.push('\'');
                    inside_single = !inside_single;
                }
                Some('"') if !inside_single => {
                    self.advance_char();
                    wb.push('"');
                    raw.push('"');
                    inside_double = !inside_double;
                }
                Some('\\') if !inside_single => {
                    self.advance_char();
                    wb.push('\\');
                    raw.push('\\');
                    if let Some(nc) = self.advance_char() {
                        wb.push(nc);
                        raw.push(nc);
                    }
                }
                Some(c) if inside_single || inside_double => {
                    self.advance_char();
                    wb.push(c);
                    raw.push(c);
                }
                Some(' ' | '\t' | '\n' | ';' | '|' | '&' | '(' | ')' | '<' | '>') | None => {
                    break;
                }
                Some(c) => {
                    self.advance_char();
                    wb.push(c);
                    raw.push(c);
                }
            }
        }
        raw
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
        loop {
            match self.peek_char() {
                Some('`') => {
                    self.advance_char();
                    wb.push('`');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    wb.push('\\');
                    if let Some(c) = self.advance_char() {
                        wb.push(c);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    wb.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated backtick",
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
