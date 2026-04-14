use super::Lexer;

/// Parses a here-document delimiter word, stripping `'...'`, `"..."`, and
/// `\X` quoting. Returns the normalized delimiter and a flag indicating
/// whether any quoting was present (bash uses this to decide whether to
/// expand the body).
pub fn parse_heredoc_delimiter(raw: &str) -> (String, bool) {
    let mut result = String::new();
    let mut quoted = false;
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    result.push(c);
                }
            }
            '"' => {
                quoted = true;
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    result.push(c);
                }
            }
            '\\' => {
                quoted = true;
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(c),
        }
    }
    (result, quoted)
}

/// Pending here-document to be read after the current line.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PendingHereDoc {
    pub delimiter: String,
    pub strip_tabs: bool,
    pub quoted: bool,
}

/// One heredoc body line, captured both verbatim (for teeing) and in
/// normalized form (for delimiter matching).
struct ReadLine {
    raw: String,
    line: String,
    eof_after_backslash: bool,
}

impl Lexer {
    /// Queues a here-document to be read after the next newline.
    pub fn queue_heredoc(&mut self, delimiter: String, strip_tabs: bool, quoted: bool) {
        self.pending_heredocs.push(PendingHereDoc {
            delimiter,
            strip_tabs,
            quoted,
        });
    }

    /// Reads all pending here-document bodies after a newline.
    pub(super) fn read_pending_heredocs(&mut self) {
        let pending: Vec<_> = self.pending_heredocs.drain(..).collect();
        for hd in pending {
            let content = self.read_heredoc_body(&hd.delimiter, hd.strip_tabs);
            self.heredoc_contents.push(content);
        }
    }

    /// Reads a here-document body until the delimiter line. On a cmdsub
    /// fork (`in_cmdsub`), a trailing `)` on the delimiter line is
    /// accepted and rewound for the outer fork to consume.
    pub(super) fn read_heredoc_body(&mut self, delimiter: &str, strip_tabs: bool) -> String {
        self.read_heredoc_body_teed(delimiter, strip_tabs, self.in_cmdsub, |_| {})
    }

    /// Reads a here-document body like `read_heredoc_body`, but also streams
    /// every consumed character to `tee` so callers needing the raw, verbatim
    /// input (e.g. the command-substitution scanner) can capture it without
    /// duplicating the line/delimiter/backslash logic.
    ///
    /// When `cmdsub_mode` is true, an EOF line of the form `DELIM)` (or
    /// `DELIM)))`) is also accepted as the delimiter: the trailing `)` chars
    /// are rewound back into the input so the outer command-substitution
    /// scanner can consume them as the closing paren.
    pub(super) fn read_heredoc_body_teed<F: FnMut(char)>(
        &mut self,
        delimiter: &str,
        strip_tabs: bool,
        cmdsub_mode: bool,
        mut tee: F,
    ) -> String {
        let mut content = String::new();
        loop {
            if self.at_end() {
                break;
            }
            let ReadLine {
                mut raw,
                line,
                eof_after_backslash,
            } = self.read_heredoc_line();
            // Check if this line matches the delimiter
            let check_line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            // Match delimiter exactly, or with trailing whitespace
            // (bash allows trailing spaces on the delimiter line)
            let mut matched = check_line == delimiter || check_line.trim_end() == delimiter;
            if !matched && cmdsub_mode {
                matched = self.try_rewind_cmdsub_close(&mut raw, &line, delimiter, strip_tabs);
            }
            // Tee the raw consumed chars (after any rewind)
            for c in raw.chars() {
                tee(c);
            }
            if matched {
                break;
            }
            if strip_tabs {
                content.push_str(line.trim_start_matches('\t'));
            } else {
                content.push_str(&line);
            }
            // Trailing \ at EOF consumes the implicit newline
            if !eof_after_backslash {
                content.push('\n');
            }
        }
        content
    }

    /// Reads one logical line of heredoc input, returning the raw verbatim
    /// form (for teeing) and the normalized form (for delimiter matching).
    fn read_heredoc_line(&mut self) -> ReadLine {
        // raw  — verbatim chars consumed
        // line — normalized form: line continuations collapsed, trailing `\n`
        //        excluded, backslash-EOF doubled (matches pre-refactor
        //        `read_heredoc_body` semantics)
        let mut raw = String::new();
        let mut line = String::new();
        let mut prev_backslash = false;
        let mut eof_after_backslash = false;
        while let Some(c) = self.peek_char() {
            self.advance_char();
            raw.push(c);
            if c == '\n' {
                break;
            }
            // Line continuation: \<newline> joins lines (but not \\<newline>)
            if c == '\\' && !prev_backslash && self.peek_char() == Some('\n') {
                self.advance_char();
                raw.push('\n');
                prev_backslash = false;
                continue;
            }
            if c == '\\' && !prev_backslash && self.peek_char().is_none() {
                // Trailing \ at EOF — treat as literal \\
                line.push('\\');
                line.push('\\');
                prev_backslash = false;
                eof_after_backslash = true;
            } else {
                prev_backslash = c == '\\' && !prev_backslash;
                line.push(c);
            }
        }
        ReadLine {
            raw,
            line,
            eof_after_backslash,
        }
    }

    /// Cmdsub edge case: the delimiter line may be followed by the closing
    /// `)` of the command substitution with no newline between (e.g.
    /// `$(cat <<E\nhello\nE)`). If the line has trailing `)` chars and
    /// stripping them yields the delimiter, rewind the `)` chars out of
    /// both `raw` and `self.pos` so the outer scanner consumes them.
    /// Returns true if the rewind happened (and the caller should treat
    /// the current line as the delimiter line).
    fn try_rewind_cmdsub_close(
        &mut self,
        raw: &mut String,
        line: &str,
        delimiter: &str,
        strip_tabs: bool,
    ) -> bool {
        if raw.ends_with('\n') || !raw.ends_with(')') {
            return false;
        }
        let n_paren = raw.chars().rev().take_while(|&c| c == ')').count();
        // `)` is 1-byte ASCII so char count == byte count; trimming
        // `n_paren` bytes from `line` is safe because `line` ends with
        // exactly those `)` chars (`raw` and `line` diverge only on line
        // continuations and trailing `\n`).
        let trimmed_len = line.len().saturating_sub(n_paren);
        let without = &line[..trimmed_len];
        let without_check = if strip_tabs {
            without.trim_start_matches('\t')
        } else {
            without
        };
        if without_check != delimiter && without_check.trim_end() != delimiter {
            return false;
        }
        for _ in 0..n_paren {
            self.pos -= 1;
            raw.pop();
        }
        true
    }

    /// Takes the next completed here-doc content, if any.
    pub fn take_heredoc_content(&mut self) -> Option<String> {
        if self.heredoc_contents.is_empty() {
            None
        } else {
            Some(self.heredoc_contents.remove(0))
        }
    }
}
