use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

mod expansions;
mod heredoc;
mod operators;
mod quotes;
mod words;

#[cfg(test)]
mod tests;

pub use heredoc::PendingHereDoc;

/// Parser state flags that affect lexer behavior.
///
/// Bash tokenization is context-sensitive — the parser must inform the lexer
/// about the current parsing context.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ParserStateFlags {
    /// Inside a case pattern.
    pub case_pattern: bool,
    /// Inside a command substitution.
    pub command_subst: bool,
    /// Inside a conditional expression `[[ ]]`.
    pub cond_expr: bool,
    /// Inside an arithmetic expression.
    pub arith: bool,
    /// Assignment words are allowed in this position.
    pub assign_ok: bool,
    /// We are at the start of a command (reserved words are recognized).
    pub command_start: bool,
    /// Extglob is enabled.
    pub extglob: bool,
}

/// Hand-written context-sensitive lexer for bash.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    peeked: Option<Token>,
    pub state: ParserStateFlags,
    /// Pending here-documents to be read after the next newline.
    pub pending_heredocs: Vec<PendingHereDoc>,
    /// Completed here-document contents (filled after newline).
    pub heredoc_contents: Vec<String>,
}

impl Lexer {
    pub fn new(source: &str, extglob: bool) -> Self {
        Self {
            input: source.chars().collect(),
            pos: 0,
            line: 1,
            peeked: None,
            state: ParserStateFlags {
                command_start: true,
                extglob,
                ..ParserStateFlags::default()
            },
            pending_heredocs: Vec::new(),
            heredoc_contents: Vec::new(),
        }
    }

    /// Returns the current position.
    pub const fn pos(&self) -> usize {
        self.pos
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
        if let Some(tok) = self.peeked.take() {
            return Ok(tok);
        }
        self.read_token()
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
                self.state.command_start = true;
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
