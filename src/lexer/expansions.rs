use crate::error::{RableError, Result};

use super::Lexer;

/// Checks if a completed word is a case-related keyword and updates tracking state.
fn check_case_keyword(word: &str, case_depth: &mut usize, in_pattern: &mut bool) {
    match word {
        "case" => {
            *case_depth += 1;
        }
        "in" if *case_depth > 0 => {
            *in_pattern = true;
        }
        "esac" if *case_depth > 0 => {
            *case_depth -= 1;
            if *case_depth == 0 {
                *in_pattern = false;
            }
        }
        _ => {}
    }
}

impl Lexer {
    /// Reads a dollar expansion into the value string.
    pub(super) fn read_dollar(&mut self, value: &mut String) -> Result<()> {
        self.advance_char(); // consume '$'
        value.push('$');

        match self.peek_char() {
            Some('(') => {
                self.advance_char();
                value.push('(');
                if self.peek_char() == Some('(') {
                    // $(( ... )) arithmetic expansion
                    self.advance_char();
                    value.push('(');
                    self.read_matched_parens(value, 2)?;
                } else {
                    // $( ... ) command substitution
                    let content_start = value.len();
                    self.read_matched_parens(value, 1)?;
                    // Validate content is parseable bash
                    let content_end = value.len().saturating_sub(1);
                    if content_start < content_end {
                        let content: String = value[content_start..content_end].to_string();
                        if !content.trim().is_empty()
                            && crate::parse(&content, self.extglob()).is_err()
                        {
                            return Err(RableError::parse(
                                "invalid command substitution",
                                self.pos,
                                self.line,
                            ));
                        }
                    }
                }
            }
            Some('{') => {
                self.advance_char();
                value.push('{');
                // Parameter expansion: don't count unquoted { as depth increase
                self.read_param_expansion_braces(value)?;
            }
            Some('\'') => {
                // $'...' ANSI-C quoting — handles \ escapes unlike regular ''
                self.advance_char();
                value.push('\'');
                self.read_ansi_c_quoted(value)?;
            }
            Some('"') => {
                // $"..." locale string
                self.advance_char();
                value.push('"');
                self.read_double_quoted(value)?;
            }
            Some('[') => {
                // $[...] deprecated arithmetic
                self.advance_char();
                value.push('[');
                self.read_until_char(value, ']')?;
            }
            Some(c) if is_dollar_start(c) => self.read_variable_name(value, c),
            _ => {} // Bare $ at end of word — just leave it
        }
        Ok(())
    }

    /// Reads a variable name after `$` for simple expansions.
    pub(super) fn read_variable_name(&mut self, value: &mut String, first: char) {
        if first.is_ascii_alphabetic() || first == '_' {
            while let Some(nc) = self.peek_char() {
                if nc.is_ascii_alphanumeric() || nc == '_' {
                    self.advance_char();
                    value.push(nc);
                } else {
                    break;
                }
            }
        } else {
            self.advance_char();
            value.push(first);
        }
    }

    /// Reads until matching closing parentheses.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_lines)]
    pub(super) fn read_matched_parens(
        &mut self,
        value: &mut String,
        close_count: usize,
    ) -> Result<()> {
        let mut depth = close_count;
        // Case-awareness: track case/esac nesting so `)` in case patterns
        // doesn't close the command substitution prematurely.
        let mut case_depth = 0usize;
        let mut in_case_pattern = false;
        let mut word_buf = String::new();

        loop {
            match self.peek_char() {
                Some(')') => {
                    // Check keyword before consuming `)`
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();

                    self.advance_char();
                    value.push(')');
                    if case_depth > 0 && in_case_pattern {
                        // This `)` terminates a case pattern — don't decrement depth
                        in_case_pattern = false; // now in pattern body
                    } else {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    }
                }
                Some('(') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    self.advance_char();
                    value.push('(');
                    // In case pattern mode, `(` is optional pattern prefix — don't increment
                    if !(case_depth > 0 && in_case_pattern) {
                        depth += 1;
                    }
                }
                Some('\'') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    self.advance_char();
                    value.push('\'');
                    self.read_single_quoted(value)?;
                }
                Some('"') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    self.advance_char();
                    value.push('"');
                    self.read_double_quoted(value)?;
                }
                Some('\\') => {
                    word_buf.clear();
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char(); // line continuation
                    } else {
                        value.push('\\');
                        if let Some(c) = self.advance_char() {
                            value.push(c);
                        }
                    }
                }
                Some('$') => {
                    word_buf.clear();
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    word_buf.clear();
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some('#') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    // # is a comment when preceded by whitespace/newline
                    let prev = value.chars().last().unwrap_or('\n');
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
                        value.push('#');
                    }
                }
                Some(';') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    self.advance_char();
                    value.push(';');
                    // Check for ;; ;& ;;& which resume case pattern mode
                    if case_depth > 0 {
                        if self.peek_char() == Some(';') {
                            self.advance_char();
                            value.push(';');
                            // ;;&
                            if self.peek_char() == Some('&') {
                                self.advance_char();
                                value.push('&');
                            }
                            in_case_pattern = true;
                        } else if self.peek_char() == Some('&') {
                            self.advance_char();
                            value.push('&');
                            in_case_pattern = true;
                        }
                    }
                }
                Some(' ' | '\t' | '\n') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    let c = self.advance_char().unwrap_or(' ');
                    value.push(c);
                }
                Some('|') => {
                    check_case_keyword(&word_buf, &mut case_depth, &mut in_case_pattern);
                    word_buf.clear();
                    self.advance_char();
                    value.push('|');
                }
                Some(c) => {
                    word_buf.push(c);
                    self.advance_char();
                    value.push(c);
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

    /// Reads a parameter expansion `${...}` allowing unbalanced inner `{`.
    pub(super) fn read_param_expansion_braces(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('}') => {
                    self.advance_char();
                    value.push('}');
                    return Ok(());
                }
                Some('\'') => {
                    self.advance_char();
                    value.push('\'');
                    self.read_single_quoted(value)?;
                }
                Some('"') => {
                    self.advance_char();
                    value.push('"');
                    self.read_double_quoted(value)?;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char(); // line continuation
                    } else {
                        value.push('\\');
                        if let Some(c) = self.advance_char() {
                            value.push(c);
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
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
    pub(super) fn read_backtick(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    value.push('\\');
                    if let Some(c) = self.advance_char() {
                        value.push(c);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
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

    /// Reads until the given closing character.
    pub(super) fn read_until_char(&mut self, value: &mut String, close: char) -> Result<()> {
        loop {
            match self.advance_char() {
                Some(c) if c == close => {
                    value.push(c);
                    return Ok(());
                }
                Some(c) => value.push(c),
                None => {
                    return Err(RableError::matched_pair(
                        format!("unterminated '{close}'"),
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
