//! The right-hand side of `=~` inside `[[ … ]]`.
//!
//! Bash lexes the regex operand in a special mode (`PST_REGEXP`): `(` and
//! `)` are regex grouping, not shell metacharacters, and inside a group
//! blanks are part of the pattern. Parable models this as `WORD_CTX_REGEX`;
//! this reader follows the same termination rules.

use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::Lexer;
use super::word_builder::WordBuilder;

impl Lexer {
    /// Reads the regex operand following `=~` as a single `Word` token.
    ///
    /// Outside any group the word ends at a blank, a `)` that closes no
    /// group, or one of `&`, `;`, `<`, `>` — bash's operator characters,
    /// which then make the conditional a syntax error (except `&&`/`||`).
    /// `<(…)`/`>(…)` is a process substitution. `|` is always a pattern
    /// character, and a `]]` inside the word (`x]]`) is part of the pattern:
    /// it only closes the conditional as a separate token. Quotes, escapes
    /// and `$`/backtick expansions are read as in ordinary words.
    ///
    /// # Errors
    ///
    /// Returns `RableError` if no operand follows `=~` or a quoted or
    /// expanded segment is unterminated.
    pub(crate) fn read_cond_regex_word(&mut self) -> Result<Token> {
        if let Some(tok) = self.peeked.take() {
            // The operand was already tokenized normally; nothing to re-lex.
            self.last_token_end = tok.pos + tok.value.chars().count();
            return Ok(tok);
        }
        self.skip_blanks();
        let (start, line) = (self.pos, self.line);
        let mut wb = WordBuilder::new();
        let mut parens = 0usize;
        while let Some(c) = self.peek_char() {
            let next = self.input.get(self.pos + 1).copied();
            match c {
                '\\' if next == Some('\n') => {
                    self.advance_char();
                    self.advance_char();
                    continue;
                }
                '<' | '>' if parens == 0 && next == Some('(') => {
                    self.read_process_sub_into(&mut wb)?;
                    continue;
                }
                ')' | ' ' | '\t' | '\n' | '&' | ';' | '<' | '>' if parens == 0 => break,
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(&mut wb, c)?;
                    continue;
                }
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
            self.advance_char();
            wb.push(c);
        }
        if wb.is_empty() {
            return Err(RableError::parse(
                "expected regular expression after =~",
                start,
                line,
            ));
        }
        self.last_token_end = self.pos;
        Ok(Token::with_spans(
            TokenType::Word,
            wb.value,
            start,
            line,
            wb.spans,
        ))
    }
}
