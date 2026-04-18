#![allow(clippy::redundant_pub_crate)]

use std::rc::Rc;

use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

mod brace_expansion;
mod expansions;
pub(crate) mod heredoc;
mod operators;
mod quotes;
pub(super) mod word_builder;
mod words;

#[cfg(test)]
mod backtick_opaque_tests;
#[cfg(test)]
mod tests;

pub(crate) use heredoc::PendingHereDoc;

/// Immutable lexer configuration set at construction time.
#[derive(Debug, Clone, Copy)]
struct LexerConfig {
    /// Whether extended glob patterns `@()`, `?()`, `*()`, `+()`, `!()` are enabled.
    extglob: bool,
}

/// Mutable context flags the parser uses to inform the lexer.
/// Private — the parser interacts via methods on `Lexer`.
#[derive(Debug, Clone)]
pub(crate) struct LexerContext {
    /// At command start position — eligible to begin a new simple command
    /// or to accept an `AssignmentWord`.
    pub(crate) command_start: bool,
    /// Reserved-word recognition is enabled. Distinct from `command_start`:
    /// after a simple command has consumed one or more `AssignmentWord`s,
    /// subsequent words must NOT be classified as reserved words, even
    /// though we are still at command-word position. Re-armed whenever
    /// `command_start` is re-armed (separators, newlines, etc.).
    pub(crate) reserved_words_ok: bool,
    /// Inside a `[[ ]]` conditional expression.
    pub(crate) cond_expr: bool,
}

impl Default for LexerContext {
    fn default() -> Self {
        Self {
            command_start: true,
            reserved_words_ok: true,
            cond_expr: false,
        }
    }
}

/// Hand-written context-sensitive lexer for bash.
pub(crate) struct Lexer {
    input: Rc<[char]>,
    pos: usize,
    line: usize,
    peeked: Option<Token>,
    config: LexerConfig,
    pub(crate) ctx: LexerContext,
    /// Pending here-documents to be read after the next newline.
    pub(crate) pending_heredocs: Vec<PendingHereDoc>,
    /// Completed here-document contents (filled after newline).
    pub(crate) heredoc_contents: Vec<String>,
    /// End position (char index) of the most recently consumed token.
    last_token_end: usize,
    /// Which nested construct, if any, this lexer is a fork of.
    /// See [`LexerMode`] for the per-mode behaviour.
    mode: LexerMode,
    /// Mirror of the owning `Parser::depth`, synced by `Parser::enter`
    /// and `Parser::leave`. Read by `read_command_sub` so forked inner
    /// parsers start at the outer depth — without this, nested `$(...)`
    /// could blow the native stack before hitting `MAX_DEPTH`.
    parser_depth: usize,
}

/// Snapshot of the mutable state the parser needs to roll back when it
/// speculatively tries a parse and has to retry with a different grammar
/// rule — notably `(( … ))` vs. nested subshells (see #42). The fields
/// captured are only those touched by `Parser::parse_arith_command` on the
/// retry path: `pos`, `line`, `peeked`, and `last_token_end`. `ctx`,
/// `pending_heredocs`, and `heredoc_contents` are deliberately omitted
/// because the arithmetic parse does not observe newlines or heredocs.
#[derive(Debug, Clone)]
pub(crate) struct LexerCheckpoint {
    pos: usize,
    line: usize,
    peeked: Option<Token>,
    last_token_end: usize,
}

/// Which nested construct the lexer is parsing, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexerMode {
    /// Top-level lexer, or a fork that is not inside a nested construct.
    Normal,
    /// Fork running inside a `$(...)` command substitution. Makes
    /// `read_heredoc_body` accept a trailing close-paren on the
    /// heredoc delimiter line and rewind it into the input so the
    /// outer scanner or the fork's own grammar can consume it.
    Cmdsub,
    /// Fork running inside a backtick command substitution. Makes
    /// `at_end` report end-of-input on a raw backtick so the forked
    /// parser terminates at the closing delimiter. Backslash escape
    /// pairs inside the body are consumed as two literal chars by
    /// the word reader's existing backslash-escape branch, which
    /// preserves the raw source slice that the outer lexer copies
    /// back into `wb.value`.
    Backtick,
}

impl Lexer {
    // -- State API for parser --

    /// Signal that the next word position is a command start. Also
    /// re-arms reserved-word recognition — a fresh command is always
    /// allowed to begin with a reserved word.
    pub(crate) const fn set_command_start(&mut self) {
        self.ctx.command_start = true;
        self.ctx.reserved_words_ok = true;
    }

    /// Signal entering a `[[ ]]` conditional expression context.
    pub(crate) const fn enter_cond_expr(&mut self) {
        self.ctx.cond_expr = true;
    }

    /// Signal leaving a `[[ ]]` conditional expression context.
    pub(crate) const fn leave_cond_expr(&mut self) {
        self.ctx.cond_expr = false;
    }

    /// Sets the lexer mode on a freshly-constructed lexer. Used by
    /// `reformat_bash` to re-parse cmdsub content with the relaxed
    /// heredoc-terminator rules that the original fork used.
    pub(crate) const fn set_mode(&mut self, mode: LexerMode) {
        self.mode = mode;
    }

    pub(crate) fn checkpoint(&self) -> LexerCheckpoint {
        LexerCheckpoint {
            pos: self.pos,
            line: self.line,
            peeked: self.peeked.clone(),
            last_token_end: self.last_token_end,
        }
    }

    pub(crate) fn restore(&mut self, cp: LexerCheckpoint) {
        self.pos = cp.pos;
        self.line = cp.line;
        self.peeked = cp.peeked;
        self.last_token_end = cp.last_token_end;
    }
}

impl Lexer {
    pub(crate) fn new(source: &str, extglob: bool) -> Self {
        Self {
            input: source.chars().collect::<Vec<_>>().into(),
            pos: 0,
            line: 1,
            peeked: None,
            config: LexerConfig { extglob },
            ctx: LexerContext::default(),
            pending_heredocs: Vec::new(),
            heredoc_contents: Vec::new(),
            last_token_end: 0,
            mode: LexerMode::Normal,
            parser_depth: 0,
        }
    }

    /// Forks a new lexer seeded at the current position, sharing the
    /// source buffer. Fresh token/heredoc state. `mode` selects the
    /// kind of nested construct the fork is parsing.
    pub(crate) fn fork(&self, mode: LexerMode) -> Self {
        Self {
            input: Rc::clone(&self.input),
            pos: self.pos,
            line: self.line,
            peeked: None,
            config: self.config,
            ctx: LexerContext::default(),
            pending_heredocs: Vec::new(),
            heredoc_contents: Vec::new(),
            last_token_end: self.pos,
            mode,
            parser_depth: self.parser_depth,
        }
    }

    pub(crate) const fn parser_depth(&self) -> usize {
        self.parser_depth
    }

    pub(crate) const fn set_parser_depth(&mut self, depth: usize) {
        self.parser_depth = depth;
    }

    /// Consumes the terminating `` ` `` byte at the end of a backtick
    /// fork. Bypasses `at_end` (which returns true on that byte in
    /// backtick mode). Only called by `parse_backtick_body`.
    pub(crate) fn exit_backtick_fork(&mut self) -> Result<()> {
        if self.input.get(self.pos).copied() != Some('`') {
            return Err(RableError::matched_pair(
                "unterminated backtick",
                self.pos,
                self.line,
            ));
        }
        self.pos += 1;
        Ok(())
    }

    /// Returns the current position.
    pub(crate) const fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the end position (char index) of the most recently consumed token.
    pub(crate) const fn last_token_end(&self) -> usize {
        self.last_token_end
    }

    /// Returns the current line number.
    pub(crate) const fn line(&self) -> usize {
        self.line
    }

    /// Returns the total input length.
    pub(crate) fn input_len(&self) -> usize {
        self.input.len()
    }

    /// Returns the character right after the current position (after peeked token).
    /// Used to detect `((` — the first `(` is peeked as `LeftParen`,
    /// and we check if the next raw character is also `(`.
    pub(crate) fn char_after_peeked(&self) -> Option<char> {
        // The peeked token consumed `(` at self.pos-1 (or wherever)
        // We need to check the char at the current pos
        self.input.get(self.pos).copied()
    }

    /// Returns true if at end of input.
    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Peeks at the current character without advancing.
    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Advances one character and returns it.
    fn advance_char(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
            }
        }
        ch
    }

    /// Returns true if the fork is sitting on its closing delimiter
    /// (raw backtick byte in backtick mode). Used by `read_token` to
    /// emit `Eof` and by `read_word_token` to terminate word reading.
    pub(crate) fn at_backtick_terminator(&self) -> bool {
        self.mode == LexerMode::Backtick && self.input.get(self.pos).copied() == Some('`')
    }

    /// Skips blanks (spaces and tabs) and line continuations (`\<newline>`).
    fn skip_blanks(&mut self) {
        loop {
            match self.peek_char() {
                Some(' ' | '\t') => {
                    self.advance_char();
                }
                Some('\\') => {
                    // Line continuation: \<newline> → skip both
                    if self.input.get(self.pos + 1) == Some(&'\n') {
                        self.advance_char(); // skip \
                        self.advance_char(); // skip \n
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    /// Skips a comment (from `#` to end of line).
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            self.advance_char();
        }
    }

    /// Returns the next token, consuming it.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on unterminated quotes or unexpected input.
    pub(crate) fn next_token(&mut self) -> Result<Token> {
        let tok = if let Some(tok) = self.peeked.take() {
            tok
        } else {
            self.read_token()?
        };
        self.last_token_end = tok.pos + tok.value.len();
        Ok(tok)
    }

    /// Peeks at the next token without consuming it.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on unterminated quotes or unexpected input.
    pub(crate) fn peek_token(&mut self) -> Result<&Token> {
        if self.peeked.is_none() {
            let tok = self.read_token()?;
            self.peeked = Some(tok);
        }
        // SAFETY: we just set it above
        self.peeked
            .as_ref()
            .ok_or_else(|| RableError::parse("unexpected end of input", self.pos, self.line))
    }

    /// Core tokenization: reads the next token from input.
    fn read_token(&mut self) -> Result<Token> {
        self.skip_blanks();

        // Backtick fork: the closing `` ` `` is end-of-input for the
        // forked parser's token stream. The byte is consumed by
        // `Lexer::exit_backtick_fork` after `parse_list` returns.
        if self.at_backtick_terminator() {
            return Ok(Token::eof(self.pos, self.line));
        }

        // Skip comments — # starts a comment anywhere after whitespace
        if self.peek_char() == Some('#') {
            self.skip_comment();
            self.skip_blanks();
        }

        if self.at_end() {
            return Ok(Token::eof(self.pos, self.line));
        }

        let start = self.pos;
        let line = self.line;
        let ch = self
            .peek_char()
            .ok_or_else(|| RableError::parse("unexpected end of input", self.pos, self.line))?;

        match ch {
            '\n' => {
                self.advance_char();
                self.ctx.command_start = true;
                self.ctx.reserved_words_ok = true;
                // Read any pending here-documents after the newline
                if !self.pending_heredocs.is_empty() {
                    self.read_pending_heredocs();
                }
                Ok(Token::new(TokenType::Newline, "\n", start, line))
            }
            '|' => Ok(self.read_pipe_operator(start, line)),
            '&' => Ok(self.read_ampersand_operator(start, line)),
            ';' => Ok(self.read_semicolon_operator(start, line)),
            '(' => {
                self.advance_char();
                Ok(Token::new(TokenType::LeftParen, "(", start, line))
            }
            ')' => {
                self.advance_char();
                Ok(Token::new(TokenType::RightParen, ")", start, line))
            }
            '<' => {
                // Check for <( process substitution
                if self.input.get(self.pos + 1) == Some(&'(') {
                    self.read_process_sub_word(start, line)
                } else {
                    Ok(self.read_less_operator(start, line))
                }
            }
            '>' => {
                // Check for >( process substitution
                if self.input.get(self.pos + 1) == Some(&'(') {
                    self.read_process_sub_word(start, line)
                } else {
                    Ok(self.read_greater_operator(start, line))
                }
            }
            _ => self.read_word_token(start, line),
        }
    }

    /// Reads raw text until `))` for arithmetic commands `(( ... ))` and
    /// C-style `for (( ; ; ))` loops.
    ///
    /// Backslash-newline is stripped as a line continuation (matching bash),
    /// other `\<x>` escapes are preserved byte-for-byte.
    ///
    /// # Errors
    ///
    /// Returns `RableError` if `))` is not found.
    pub(crate) fn read_until_double_paren(&mut self) -> Result<String> {
        // Clear any peeked token since we're reading raw
        self.peeked = None;
        let mut result = String::new();
        let mut depth = 0i32;
        loop {
            match self.peek_char() {
                Some(')') if depth == 0 => {
                    self.advance_char();
                    if self.peek_char() == Some(')') {
                        self.advance_char();
                        return Ok(result);
                    }
                    result.push(')');
                }
                Some('(') => {
                    self.advance_char();
                    depth += 1;
                    result.push('(');
                }
                Some(')') => {
                    self.advance_char();
                    depth -= 1;
                    result.push(')');
                }
                Some('\\') => {
                    // `\<newline>` is a line continuation in arithmetic
                    // context — drop both characters. All other `\<x>`
                    // sequences are preserved byte-for-byte so the
                    // arithmetic parser sees the same raw text as bash.
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char();
                    } else {
                        result.push('\\');
                        if let Some(c) = self.advance_char() {
                            result.push(c);
                        }
                    }
                }
                Some(c) => {
                    self.advance_char();
                    result.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated ((",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }
}
