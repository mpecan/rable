use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

mod expansions;
mod heredoc;
mod operators;
mod quotes;
pub mod word_builder;
mod words;

#[cfg(test)]
mod tests;

pub use heredoc::PendingHereDoc;

/// Immutable lexer configuration set at construction time.
#[derive(Debug, Clone, Copy)]
pub struct LexerConfig {
    /// Whether extended glob patterns `@()`, `?()`, `*()`, `+()`, `!()` are enabled.
    pub extglob: bool,
}

/// Mutable context flags the parser uses to inform the lexer.
/// Private — the parser interacts via methods on `Lexer`.
#[derive(Debug, Clone)]
pub struct LexerContext {
    /// At command start position — reserved words are recognized.
    pub command_start: bool,
    /// Inside a `[[ ]]` conditional expression.
    pub cond_expr: bool,
}

impl Default for LexerContext {
    fn default() -> Self {
        Self {
            command_start: true,
            cond_expr: false,
        }
    }
}

/// Hand-written context-sensitive lexer for bash.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    peeked: Option<Token>,
    config: LexerConfig,
    pub(crate) ctx: LexerContext,
    /// Pending here-documents to be read after the next newline.
    pub pending_heredocs: Vec<PendingHereDoc>,
    /// Completed here-document contents (filled after newline).
    pub heredoc_contents: Vec<String>,
    /// End position (char index) of the most recently consumed token.
    last_token_end: usize,
}

impl Lexer {
    // -- State API for parser --

    /// Signal that the next word position is a command start.
    pub const fn set_command_start(&mut self) {
        self.ctx.command_start = true;
    }

    /// Signal entering a `[[ ]]` conditional expression context.
    pub const fn enter_cond_expr(&mut self) {
        self.ctx.cond_expr = true;
    }

    /// Signal leaving a `[[ ]]` conditional expression context.
    pub const fn leave_cond_expr(&mut self) {
        self.ctx.cond_expr = false;
    }

    /// Whether extended glob patterns are enabled.
    pub const fn extglob(&self) -> bool {
        self.config.extglob
    }
}

impl Lexer {
    pub fn new(source: &str, extglob: bool) -> Self {
        Self {
            input: source.chars().collect(),
            pos: 0,
            line: 1,
            peeked: None,
            config: LexerConfig { extglob },
            ctx: LexerContext::default(),
            pending_heredocs: Vec::new(),
            heredoc_contents: Vec::new(),
            last_token_end: 0,
        }
    }

    /// Returns the current position.
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the end position (char index) of the most recently consumed token.
    pub const fn last_token_end(&self) -> usize {
        self.last_token_end
    }

    /// Returns the current line number.
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Returns the total input length.
    pub const fn input_len(&self) -> usize {
        self.input.len()
    }

    /// Returns the character right after the current position (after peeked token).
    /// Used to detect `((` — the first `(` is peeked as `LeftParen`,
    /// and we check if the next raw character is also `(`.
    pub fn char_after_peeked(&self) -> Option<char> {
        // The peeked token consumed `(` at self.pos-1 (or wherever)
        // We need to check the char at the current pos
        self.input.get(self.pos).copied()
    }

    /// Returns true if at end of input.
    pub const fn at_end(&self) -> bool {
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
    pub fn next_token(&mut self) -> Result<Token> {
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
    pub fn peek_token(&mut self) -> Result<&Token> {
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
                // Read any pending here-documents after the newline
                if !self.pending_heredocs.is_empty() {
                    self.read_pending_heredocs();
                }
                Ok(Token::new(TokenType::Newline, "\n", start, line))
            }
            '|' => self.read_pipe_operator(start, line),
            '&' => self.read_ampersand_operator(start, line),
            ';' => self.read_semicolon_operator(start, line),
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
                    self.read_less_operator(start, line)
                }
            }
            '>' => {
                // Check for >( process substitution
                if self.input.get(self.pos + 1) == Some(&'(') {
                    self.read_process_sub_word(start, line)
                } else {
                    self.read_greater_operator(start, line)
                }
            }
            _ => self.read_word_token(start, line),
        }
    }

    /// Reads raw text until `))` for C-style for loops.
    ///
    /// # Errors
    ///
    /// Returns `RableError` if `))` is not found.
    pub fn read_until_double_paren(&mut self) -> Result<String> {
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
