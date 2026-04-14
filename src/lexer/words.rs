use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::Lexer;
use super::word_builder::{WordBuilder, WordSpanKind};

impl Lexer {
    /// Reads a word token, handling quoting and expansions.
    #[allow(clippy::too_many_lines)]
    pub(super) fn read_word_token(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut wb = WordBuilder::new();

        while let Some(c) = self.peek_char() {
            match c {
                // Metacharacters end a word
                ' ' | '\t' | '\n' | '|' | '&' | ';' | ')' => break,
                // Closing delimiter of a backtick fork ends the word.
                // The byte is consumed later by `exit_backtick_fork`.
                '`' if self.at_backtick_terminator() => break,
                // < and > are metacharacters, but <( and >( are process substitution
                '<' | '>' => {
                    if !wb.is_empty() && self.input.get(self.pos + 1) == Some(&'(') {
                        // Process substitution mid-word: cat<(cmd)
                        self.read_process_sub_into(&mut wb)?;
                    } else {
                        break;
                    }
                }
                // ( is a metacharacter UNLESS preceded by = (array) or extglob prefix
                '(' => {
                    if wb.ends_with('=') {
                        // Array assignment: arr=(...) — parse elements directly
                        self.advance_char();
                        wb.push('(');
                        self.read_array_elements(&mut wb)?;
                    } else if wb.ends_with('@')
                        || wb.ends_with('?')
                        || wb.ends_with('+')
                        || (wb.ends_with('!') && self.config.extglob)
                        || (wb.ends_with('*') && self.config.extglob)
                    {
                        // Extglob: @(...), ?(...), etc.
                        self.advance_char();
                        wb.push('(');
                        self.read_matched_parens(&mut wb, 1)?;
                    } else {
                        break;
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(&mut wb, c)?;
                }
                // Extglob: @(...), ?(...), +(...), !(...)
                // !( is only extglob when extglob is enabled; otherwise ! is Bang
                '@' | '?' | '+' if self.input.get(self.pos + 1) == Some(&'(') => {
                    self.read_extglob(&mut wb, c)?;
                }
                // !( and *( are only extglob when extglob mode is enabled
                '!' | '*' if self.input.get(self.pos + 1) == Some(&'(') && self.config.extglob => {
                    self.read_extglob(&mut wb, c)?;
                }
                // `[` inside a word enters the bracket-subscript context when:
                //   * we're at command-start after a plain identifier
                //     (covers `arr[i]=val` assignments and `foo[...]` calls
                //     where bash absorbs whitespace / metacharacters); or
                //   * we're inside `[[ ]]`, where bash parses regex character
                //     classes like `[[:alpha:][:digit:]]` as a single word.
                // In any other position a bare `[` is an ordinary character.
                '[' if (self.ctx.command_start && is_identifier_prefix(&wb.value))
                    || (self.ctx.cond_expr && !wb.is_empty() && !wb.ends_with('[')) =>
                {
                    self.read_bracket_subscript(&mut wb)?;
                }
                // Regular character
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

        // Check for reserved words at command start, then assignment pattern.
        // Reserved-word classification requires BOTH `command_start` and
        // `reserved_words_ok` (issue #37): once a simple command has
        // consumed an AssignmentWord, bash stops recognizing reserved
        // words in that same command even though we're still at command
        // position. Additionally, `]]` is only a reserved word inside a
        // `[[ ]]` conditional (issue #35); elsewhere it's an ordinary word
        // so the parser doesn't mistake it for a terminator.
        let mut kind = if self.ctx.command_start && self.ctx.reserved_words_ok {
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
        };
        if kind == TokenType::DoubleRightBracket && !self.ctx.cond_expr {
            kind = TokenType::Word;
        }

        // Update context flags for the next token.
        //
        // AssignmentWord keeps `command_start=true` (assignments chain and
        // the eventual command word is still at command position) but clears
        // `reserved_words_ok` — no more reserved words in this simple command.
        //
        // Other words follow the existing rule: command_start re-arms only
        // for keywords that start a new command (`then`, `else`, ...), and
        // `reserved_words_ok` tracks `command_start` directly.
        if kind == TokenType::AssignmentWord {
            self.ctx.command_start = true;
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
                );
            self.ctx.reserved_words_ok = self.ctx.command_start;
        }

        Ok(Token::with_spans(kind, wb.value, start, line, wb.spans))
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
                let start = wb.span_start();
                self.advance_char();
                wb.push('`');
                self.read_backtick(wb)?;
                wb.record(start, WordSpanKind::Backtick);
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
        self.read_matched_parens(wb, 1)?;
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
                _ => {
                    self.advance_char();
                    wb.push(c);
                }
            }
        }
        Ok(())
    }

    /// Reads a process substitution into an existing word value.
    pub(super) fn read_process_sub_into(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let start = wb.span_start();
        let dir = self.advance_char().unwrap_or('<');
        wb.push(dir);
        self.advance_char(); // (
        wb.push('(');
        self.read_paren_body_forked(wb)?;
        wb.record(start, WordSpanKind::ProcessSub(dir));
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
        self.read_paren_body_forked(&mut wb)?;
        wb.record(span_start, WordSpanKind::ProcessSub(dir));
        // Continue reading word chars after the process substitution
        self.continue_word(&mut wb)?;
        self.ctx.command_start = false;
        self.ctx.reserved_words_ok = false;
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

    /// Reads array elements from `(...)`, producing normalized content.
    ///
    /// Parses individual elements separated by whitespace directly from
    /// the input stream, stripping comments. Uses the lexer's existing
    /// word-reading facilities for quoting and expansion handling.
    /// The opening `(` must already be consumed and pushed to `wb`.
    fn read_array_elements(&mut self, wb: &mut WordBuilder) -> Result<()> {
        let mut need_space = false;
        loop {
            // Skip whitespace between elements
            while matches!(self.peek_char(), Some(' ' | '\t' | '\n' | '\r')) {
                self.advance_char();
            }
            match self.peek_char() {
                Some(')') => {
                    self.advance_char();
                    wb.push(')');
                    return Ok(());
                }
                Some('#') => {
                    // Comment — skip to end of line
                    while matches!(self.peek_char(), Some(c) if c != '\n') {
                        self.advance_char();
                    }
                }
                Some(_) => {
                    if need_space {
                        wb.push(' ');
                    }
                    self.read_array_element(wb)?;
                    need_space = true;
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated array",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a single array element using the same word-reading logic
    /// as `read_word_token`, but with `)` and whitespace as terminators
    /// instead of the standard shell metacharacters.
    fn read_array_element(&mut self, wb: &mut WordBuilder) -> Result<()> {
        while let Some(c) = self.peek_char() {
            match c {
                ' ' | '\t' | '\n' | '\r' | ')' => break,
                '<' | '>' => {
                    if self.input.get(self.pos + 1) == Some(&'(') {
                        self.read_process_sub_into(wb)?;
                    } else {
                        self.advance_char();
                        wb.push(c);
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

/// Returns true if a word value matches the assignment pattern `NAME=`,
/// `NAME+=`, or `NAME[...]=` / `NAME[...]+=`.
fn is_assignment_word(value: &str) -> bool {
    let bytes = value.as_bytes();
    // Must start with [a-zA-Z_]
    if !matches!(bytes.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
        return false;
    }
    let mut i = 1;
    // Continue with [a-zA-Z_0-9]
    while i < bytes.len() {
        match bytes[i] {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => i += 1,
            b'[' => {
                // Skip subscript [...] — reject if it contains whitespace
                // (bash doesn't allow spaces in assignment subscripts)
                i += 1;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b' ' | b'\t' | b'\n' => return false,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'+' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => return true,
            b'=' => return true,
            _ => return false,
        }
    }
    false
}
