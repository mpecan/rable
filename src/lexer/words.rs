use std::ops::ControlFlow;

use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::Lexer;
use super::assignment::{is_assignment_builtin, is_assignment_word};
use super::word_builder::{WordBuilder, WordSpanKind};

impl Lexer {
    /// Reads a word token, handling quoting and expansions.
    ///
    /// Structured as three phases: character-by-character assembly via
    /// the `while let`/`match` loop, classification of the accumulated
    /// value (`classify_word`), and context-flag update for the next
    /// token (`advance_word_context`).
    pub(super) fn read_word_token(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut wb = WordBuilder::new();
        while let Some(c) = self.peek_char() {
            match c {
                // Metacharacters end a word.
                ' ' | '\t' | '\n' | '|' | '&' | ';' | ')' => break,
                // bash: the `}` closing `${ …; }` is a token of its own even
                // when more word characters follow it (`${ a;}${ b;}`).
                '}' if wb.is_empty()
                    && self.ctx.funsub
                    && self.ctx.command_start
                    && self.ctx.reserved_words_ok =>
                {
                    self.advance_char();
                    wb.push('}');
                    break;
                }
                // Closing delimiter of a backtick fork ends the word.
                // The byte is consumed later by `exit_backtick_fork`.
                '`' if self.at_backtick_terminator() => break,
                '<' | '>' => {
                    if self.read_angle_bracket_word(&mut wb)?.is_break() {
                        break;
                    }
                }
                '(' => {
                    if self.read_open_paren_word(&mut wb)?.is_break() {
                        break;
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(&mut wb, c)?;
                }
                c if self.is_extglob_trigger(c) => {
                    self.read_extglob(&mut wb, c)?;
                }
                '[' if self.word_enters_bracket_subscript(&wb) => {
                    self.read_bracket_subscript(&mut wb)?;
                }
                _ => {
                    self.advance_char();
                    wb.push(c);
                }
            }
        }

        super::brace_expansion::detect_brace_expansions(&mut wb);

        if wb.is_empty() {
            return Err(RableError::parse("unexpected character", start, line));
        }

        let kind = self.classify_word(&wb);
        // A redirect fd prefix: `2>`, `{fd}>`.
        let fd_prefix = matches!(self.peek_char(), Some('<' | '>'))
            && (is_fd_number(&wb.value) || is_varfd(&wb.value));
        self.advance_word_context(kind, &wb.value, fd_prefix);
        Ok(Token::with_spans(kind, wb.value, start, line, wb.spans))
    }

    // -- word-assembly dispatch helpers --

    /// Handles a `<` or `>` encountered mid-word. If followed by `(` and
    /// the word so far is non-empty, reads a process substitution
    /// (`cat<(cmd)`) into `wb` and returns `Continue`. Otherwise the
    /// character is a metacharacter that terminates the word, and we
    /// return `Break`.
    fn read_angle_bracket_word(&mut self, wb: &mut WordBuilder) -> Result<ControlFlow<()>> {
        if !wb.is_empty() && self.input.get(self.pos + 1) == Some(&'(') {
            self.read_process_sub_into(wb)?;
            Ok(ControlFlow::Continue(()))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Handles `(` encountered mid-word. Three outcomes:
    ///
    /// * preceded by `name=` / `name+=` / `name[sub]=` where an assignment
    ///   is allowed — array assignment `arr=(…)`; consume the
    ///   parenthesised element list and keep reading.
    /// * preceded by an extglob prefix (`@ ? + ! *`) when extglob is on
    ///   — extglob pattern `@(…)`, `*(…)`, etc.
    /// * otherwise `(` is a metacharacter that terminates the word.
    fn read_open_paren_word(&mut self, wb: &mut WordBuilder) -> Result<ControlFlow<()>> {
        if wb.ends_with('=') && self.array_assignment_allowed(wb) {
            self.advance_char();
            wb.push('(');
            self.read_array_elements(wb)?;
            Ok(ControlFlow::Continue(()))
        } else if self.config.extglob && wb.value.ends_with(['@', '?', '+', '!', '*']) {
            self.advance_char();
            wb.push('(');
            self.read_matched_parens(wb, false)?;
            Ok(ControlFlow::Continue(()))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Returns true if `c` at the current position opens an extglob
    /// pattern: a prefix character (`@ ? + ! *`) followed immediately by
    /// `(`, when extglob is on (see also `Lexer::next_pattern_token`).
    fn is_extglob_trigger(&self, c: char) -> bool {
        if self.input.get(self.pos + 1) != Some(&'(') {
            return false;
        }
        self.config.extglob && matches!(c, '@' | '?' | '+' | '!' | '*')
    }

    /// Returns true if a `[` at the current position should open a
    /// bracket-subscript absorption (`arr[i]=val`, `arr[i]` at command
    /// start, or regex character classes inside `[[ ]]`).
    ///
    /// In any other position a bare `[` is just an ordinary character
    /// so the word reader falls through to the `_` arm.
    fn word_enters_bracket_subscript(&self, wb: &WordBuilder) -> bool {
        // A redirect target is never an assignment: `<exp[ 5` is `exp[`.
        (self.ctx.command_start && !self.ctx.redirect_target && is_identifier_prefix(&wb.value))
            || (self.ctx.cond_expr && !wb.is_empty() && !wb.ends_with('['))
    }

    // -- word finalisation --

    /// Classifies an assembled word value into a `TokenType`.
    ///
    /// Reserved-word classification requires BOTH `command_start` and
    /// `reserved_words_ok` (issue #37): once a simple command has
    /// consumed an `AssignmentWord`, bash stops recognising reserved
    /// words in that same command even though we're still at command
    /// position. Like bash, `]]` is a reserved word in that position too,
    /// so ` ]] foo` is rejected; as an argument (`echo ]]`) it stays an
    /// ordinary word (issue #35).
    fn classify_word(&self, wb: &WordBuilder) -> TokenType {
        if self.ctx.command_start && self.ctx.reserved_words_ok {
            TokenType::reserved_word(&wb.value).unwrap_or_else(|| {
                if is_assignment_word(&wb.value) {
                    TokenType::AssignmentWord
                } else {
                    TokenType::Word
                }
            })
        } else if is_assignment_word(&wb.value) {
            TokenType::AssignmentWord
        } else {
            TokenType::Word
        }
    }

    /// Updates `command_start` / `reserved_words_ok` for the token
    /// that follows the word just emitted.
    ///
    /// Redirect fd prefixes and targets leave the context untouched.
    ///
    /// `AssignmentWord` keeps `command_start` as it was — still true in
    /// a leading chain (`a=1 b=(x) cmd`), still false for an argument
    /// (`cmd a=1`) — but clears `reserved_words_ok`: no more reserved
    /// words in this simple command.
    ///
    /// Other words follow the existing rule: `command_start` re-arms
    /// for keywords that start a new command (`then`, `else`, `elif`,
    /// `do`, `;`) and after keywords that close a compound command
    /// (`fi`, `done`, `esac`, `}`), where bash's
    /// `reserved_word_acceptable` also recognises reserved words — so
    /// `…; fi done` closes the enclosing loop. `reserved_words_ok`
    /// tracks `command_start` directly.
    fn advance_word_context(&mut self, kind: TokenType, value: &str, fd_prefix: bool) {
        // Redirect fd prefixes (`2>`, `{fd}>`) and targets are not part of
        // the command's words, so they leave its context unchanged.
        if self.ctx.redirect_target {
            self.ctx.redirect_target = false;
            return;
        }
        if fd_prefix {
            return;
        }
        // The first word of a command decides whether later `name=(…)`
        // arguments are arrays; leading assignments defer the decision.
        if self.ctx.command_start && kind != TokenType::AssignmentWord {
            self.ctx.assignment_builtin = kind == TokenType::Word && is_assignment_builtin(value);
        }
        if kind == TokenType::AssignmentWord {
            // Assignments chain at command start (`a=1 b=(x) cmd`) but an
            // assignment-shaped argument (`cmd a=1`) does not re-arm it.
            self.ctx.reserved_words_ok = false;
        } else {
            self.ctx.command_start = kind.starts_command()
                || matches!(
                    kind,
                    TokenType::Then
                        | TokenType::Else
                        | TokenType::Elif
                        | TokenType::Do
                        | TokenType::Semi
                        | TokenType::Fi
                        | TokenType::Done
                        | TokenType::Esac
                        | TokenType::RightBrace
                );
            self.ctx.reserved_words_ok = self.ctx.command_start;
        }
        // Assignments and command words count; a keyword that starts a
        // new command (`then`, `do`, …) does not.
        self.ctx.command_has_word = kind == TokenType::AssignmentWord || !self.ctx.command_start;
    }

    /// Reads a quoted string, escape, dollar expansion, or backtick within a word.
    pub(super) fn read_word_special(&mut self, wb: &mut WordBuilder, c: char) -> Result<()> {
        match c {
            '\'' => {
                let start = wb.span_start();
                self.advance_char();
                wb.push('\'');
                self.read_single_quoted(wb)?;
                wb.record(start, WordSpanKind::SingleQuoted);
            }
            '"' => {
                let start = wb.span_start();
                self.advance_char();
                wb.push('"');
                self.read_double_quoted(wb)?;
                wb.record(start, WordSpanKind::DoubleQuoted);
            }
            '\\' => {
                self.advance_char();
                if self.peek_char() == Some('\n') {
                    self.advance_char(); // line continuation — no span
                } else {
                    let start = wb.span_start();
                    wb.push('\\');
                    if let Some(next) = self.advance_char() {
                        wb.push(next);
                    } else {
                        // Trailing \ at EOF — bash keeps it as literal \\
                        wb.push('\\');
                    }
                    wb.record(start, WordSpanKind::Escape);
                }
            }
            '$' => {
                self.read_dollar(wb)?;
            }
            '`' => {
                self.read_backtick_sub(wb)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads an extglob pattern `@(...)`, `?(...)`, etc.
    pub(super) fn read_extglob(&mut self, wb: &mut WordBuilder, prefix: char) -> Result<()> {
        let start = wb.span_start();
        self.advance_char();
        wb.push(prefix);
        self.advance_char();
        wb.push('(');
        self.read_matched_parens(wb, false)?;
        wb.record(start, WordSpanKind::Extglob(prefix));
        Ok(())
    }

    /// Reads a bracket subscript `[...]` inside a word.
    ///
    /// Called only when the word so far is a plain identifier and we are at
    /// command-start position — i.e. an `arr[idx]=value` assignment or an
    /// `arr[idx]` command invocation. In this position bash's parser is
    /// permissive about whitespace and metacharacters inside `[...]`, so
    /// we absorb them into the word verbatim until the matching `]`.
    pub(super) fn read_bracket_subscript(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let start = wb.span_start();
        self.advance_char(); // consume [
        wb.push('[');
        let mut depth = 1;
        while let Some(c) = self.peek_char() {
            match c {
                '[' => {
                    depth += 1;
                    self.advance_char();
                    wb.push(c);
                }
                ']' => {
                    depth -= 1;
                    self.advance_char();
                    wb.push(c);
                    if depth == 0 {
                        wb.record(start, WordSpanKind::BracketSubscript);
                        return Ok(());
                    }
                }
                '\'' => {
                    self.advance_char();
                    wb.push('\'');
                    self.read_single_quoted(wb)?;
                }
                '"' => {
                    self.advance_char();
                    wb.push('"');
                    self.read_double_quoted(wb)?;
                }
                '\\' => {
                    self.advance_char();
                    wb.push('\\');
                    if let Some(nc) = self.advance_char() {
                        wb.push(nc);
                    }
                }
                '$' => {
                    self.read_dollar(wb)?;
                }
                '`' => {
                    self.read_word_special(wb, c)?;
                }
                _ => {
                    self.advance_char();
                    wb.push(c);
                }
            }
        }
        // bash matches the subscript to its `]` across blanks and lines,
        // so `foo[0` and `foo[0 bar` hit end of input.
        Err(RableError::matched_pair(
            "unterminated subscript",
            self.pos,
            self.line,
        ))
    }

    /// Reads a process substitution into an existing word value.
    pub(super) fn read_process_sub_into(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let start = wb.span_start();
        let dir = self.advance_char().unwrap_or('<');
        wb.push(dir);
        self.advance_char(); // (
        wb.push('(');
        let body = self.read_paren_body_forked(wb)?;
        wb.record_body(start, WordSpanKind::ProcessSub(dir), body);
        Ok(())
    }

    /// Reads a process substitution `<(...)` or `>(...)` as a word token.
    /// Continues reading word characters after the closing `)`.
    pub(super) fn read_process_sub_word(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut wb = WordBuilder::new();
        let span_start = wb.span_start();
        // Read < or >
        let dir = self.advance_char().unwrap_or('<');
        wb.push(dir);
        // Read (
        self.advance_char();
        wb.push('(');
        // Read until matching ) via fork-and-merge
        let body = self.read_paren_body_forked(&mut wb)?;
        wb.record_body(span_start, WordSpanKind::ProcessSub(dir), body);
        // Continue reading word chars after the process substitution
        self.continue_word(&mut wb)?;
        self.advance_word_context(TokenType::Word, &wb.value, false);
        Ok(Token::with_spans(
            TokenType::Word,
            wb.value,
            start,
            line,
            wb.spans,
        ))
    }

    /// Continue reading word characters after a special construct.
    fn continue_word(&mut self, wb: &mut WordBuilder) -> Result<()> {
        while let Some(c) = self.peek_char() {
            match c {
                ' ' | '\t' | '\n' | '|' | '&' | ';' | ')' | '(' => break,
                '<' | '>' => {
                    if self.input.get(self.pos + 1) == Some(&'(') {
                        self.read_process_sub_into(wb)?;
                    } else {
                        break;
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(wb, c)?;
                }
                _ => {
                    self.advance_char();
                    wb.push(c);
                }
            }
        }
        Ok(())
    }
}

/// Returns true if `value` is a non-empty plain bash identifier
/// (`[a-zA-Z_][a-zA-Z_0-9]*`) with no other characters.
///
/// Used to gate bracket-subscript absorption: `arr[i]=x` and `arr[i]`
/// only behave specially when the word so far is a bare identifier.
/// Words like `][[`, `[c[`, or `foo^` must fall through to ordinary
/// tokenization so `[...]` contents do not absorb whitespace.
fn is_identifier_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    if !matches!(first, b'a'..=b'z' | b'A'..=b'Z' | b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

/// Returns true if the string is a valid file descriptor number: all
/// digits and within `i32` range, as bash accepts any `int` fd prefix
/// (`100>file`, `007<in`); larger numbers stay ordinary words.
pub(crate) fn is_fd_number(s: &str) -> bool {
    fd_number(s).is_some()
}

/// The fd a digit-only word names, if it fits bash's `int`: `02` is 2.
pub(crate) fn fd_number(s: &str) -> Option<i32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// The fd a redirect operator acts on when none is written: 1 for output
/// (`>`, `>&`, …), 0 for input; -1 for operators without one.
pub(crate) const fn default_fd_for_op(op: &str) -> i32 {
    match op.as_bytes() {
        b">" | b">>" | b">|" | b">&" => 1,
        b"<" | b"<&" | b"<>" => 0,
        _ => -1,
    }
}

/// Returns true if the string is a variable fd reference like `{varname}`.
/// Requires valid bash variable name: starts with letter or `_`, then
/// alphanumeric or `_`.
pub(crate) fn is_varfd(s: &str) -> bool {
    s.starts_with('{')
        && s.ends_with('}')
        && s.len() >= 3
        // First char must be letter or underscore (valid variable name start)
        && s.as_bytes()
            .get(1)
            .is_some_and(|&c| c.is_ascii_alphabetic() || c == b'_')
        && s[1..s.len() - 1]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}
