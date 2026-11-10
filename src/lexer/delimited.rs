//! Splitting a standalone string at top-level delimiters, as bash's
//! `skip_to_delim` does for the `for ((init; test; step))` header
//! (`make_arith_for_command`): quotes, escapes, backticks and `$…`
//! expansions are read whole by the lexer's own readers, so a `;` inside
//! `"$(printf ";")"` or a nested `case … ;; esac` is not a separator.

use crate::error::Result;

use super::Lexer;
use super::word_builder::WordBuilder;

/// Splits `text` at each `delim` outside quotes, escapes and expansions.
///
/// # Errors
///
/// Returns `RableError` if a quote or expansion in `text` is unterminated.
pub(crate) fn split_top_level(text: &str, delim: char) -> Result<Vec<String>> {
    Lexer::new(text, false).split_at_top_level(delim)
}

impl Lexer {
    fn split_at_top_level(&mut self, delim: char) -> Result<Vec<String>> {
        let mut parts = Vec::new();
        let mut start = self.pos;
        while let Some(c) = self.peek_char() {
            match c {
                '\\' | '\'' | '"' | '$' | '`' => {
                    self.read_word_special(&mut WordBuilder::new(), c)?;
                }
                _ if c == delim => {
                    parts.push(self.input[start..self.pos].iter().collect());
                    self.advance_char();
                    start = self.pos;
                }
                _ => {
                    self.advance_char();
                }
            }
        }
        parts.push(self.input[start..].iter().collect());
        Ok(parts)
    }
}
