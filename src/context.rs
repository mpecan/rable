//! Shared context types for tracking quoting and nesting state.
//!
//! These types are used by both the lexer (when reading matched parens)
//! and the S-expression formatter (when extracting paren content from
//! word values). Centralizing them ensures consistent behavior.

/// Tracks `case`/`in`/`esac` nesting inside parenthesis matching.
///
/// In bash, `)` inside case patterns doesn't close `$(...)`. This state
/// machine tracks whether we're inside a case block and whether the
/// current `)` is a pattern terminator.
#[derive(Debug, Clone, Default)]
pub struct CaseTracker {
    /// Nesting depth of case statements.
    depth: usize,
    /// Whether we're in pattern-matching position (after `in` or `;;`).
    in_pattern: bool,
}

impl CaseTracker {
    /// Process a completed word at a word boundary.
    /// Call this when whitespace, `|`, or other delimiters are encountered.
    pub fn check_word(&mut self, word: &str) {
        match word {
            "case" => self.depth += 1,
            "in" if self.depth > 0 => self.in_pattern = true,
            "esac" if self.depth > 0 => {
                self.depth -= 1;
                if self.depth == 0 {
                    self.in_pattern = false;
                }
            }
            _ => {}
        }
    }

    /// Returns true if `)` at this position is a case pattern terminator
    /// (and should NOT decrement paren depth).
    pub const fn is_pattern_close(&self) -> bool {
        self.depth > 0 && self.in_pattern
    }

    /// Called after a `)` that was identified as a pattern close.
    pub const fn close_pattern(&mut self) {
        self.in_pattern = false;
    }

    /// Called after `;;`, `;&`, or `;;&` — resumes pattern matching.
    pub const fn resume_pattern(&mut self) {
        if self.depth > 0 {
            self.in_pattern = true;
        }
    }

    /// Returns true if `(` should NOT increment paren depth
    /// (optional leading `(` in case patterns).
    pub const fn is_pattern_open(&self) -> bool {
        self.depth > 0 && self.in_pattern
    }
}

/// Skips a single-quoted region in a char array. Reads from `pos` (which
/// should point AT the opening `'`) through the closing `'`.
/// Pushes all chars (including quotes) into `out`.
pub fn skip_single_quoted(chars: &[char], pos: &mut usize, out: &mut String) {
    out.push(chars[*pos]); // opening '
    *pos += 1;
    while *pos < chars.len() && chars[*pos] != '\'' {
        out.push(chars[*pos]);
        *pos += 1;
    }
    if *pos < chars.len() {
        out.push(chars[*pos]); // closing '
        *pos += 1;
    }
}

/// Skips a double-quoted region in a char array. Reads from `pos` (which
/// should point AT the opening `"`) through the closing `"`.
/// Handles backslash escapes inside. Pushes all chars into `out`.
pub fn skip_double_quoted(chars: &[char], pos: &mut usize, out: &mut String) {
    out.push(chars[*pos]); // opening "
    *pos += 1;
    while *pos < chars.len() && chars[*pos] != '"' {
        if chars[*pos] == '\\' && *pos + 1 < chars.len() {
            out.push(chars[*pos]);
            *pos += 1;
        }
        out.push(chars[*pos]);
        *pos += 1;
    }
    if *pos < chars.len() {
        out.push(chars[*pos]); // closing "
        *pos += 1;
    }
}

/// Skips a backtick region in a char array. Reads from `pos` (which
/// should point AT the opening `` ` ``) through the closing `` ` ``.
/// Handles backslash escapes inside. Pushes all chars into `out`.
pub fn skip_backtick(chars: &[char], pos: &mut usize, out: &mut String) {
    out.push(chars[*pos]); // opening `
    *pos += 1;
    while *pos < chars.len() && chars[*pos] != '`' {
        if chars[*pos] == '\\' && *pos + 1 < chars.len() {
            out.push(chars[*pos]);
            *pos += 1;
        }
        out.push(chars[*pos]);
        *pos += 1;
    }
    if *pos < chars.len() {
        out.push(chars[*pos]); // closing `
        *pos += 1;
    }
}

/// Counts consecutive backslashes before a position to determine
/// if a character is escaped.
///
/// Even count → not escaped. Odd count → escaped.
pub fn is_backslash_escaped(chars: &[char], pos: usize) -> bool {
    let mut count = 0;
    let mut j = pos;
    while j > 0 && chars[j - 1] == '\\' {
        count += 1;
        j -= 1;
    }
    count % 2 != 0
}
