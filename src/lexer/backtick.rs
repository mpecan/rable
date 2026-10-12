//! Backtick command substitution (`` `…` ``): forks the real parser over
//! the body, falling back to an opaque scan when the body does not parse.

use crate::error::{RableError, Result};

use super::Lexer;
use super::word_builder::{QuotingContext, WordBuilder, WordSpanKind};

impl Lexer {
    /// Reads a backtick command substitution starting at the opening
    /// `` ` `` and records a `Backtick` span covering the whole `` `…` ``,
    /// so `Word.parts` exposes it as a `CommandSubstitution` in every
    /// context (bare, inside `"…"`, `${…}`, `$((…))`).
    pub(super) fn read_backtick_sub(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let start = wb.span_start();
        self.advance_char();
        wb.push('`');
        self.read_backtick(wb)?;
        wb.record(start, WordSpanKind::Backtick);
        Ok(())
    }

    /// Reads the body of a backtick command substitution (after the opening `` ` ``).
    fn read_backtick(&mut self, wb: &mut WordBuilder) -> Result<()> {
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
}
