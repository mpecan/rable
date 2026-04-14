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

/// One heredoc body line in normalized form (delimiter-match-ready) plus
/// termination metadata used by the cmdsub-mode rewind path.
struct ReadLine {
    line: String,
    eof_after_backslash: bool,
    /// True if the line ended at EOF with no trailing `\n` — i.e. the
    /// heredoc body ran out of input. The cmdsub-mode rewind only fires
    /// on such lines, because `DELIM)` only butts up against the outer
    /// construct's closing paren when there's no newline between them.
    terminated_at_eof: bool,
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

    /// Reads a here-document body until the delimiter line. In a cmdsub
    /// fork (which process substitution also uses), an EOF line of the
    /// form `DELIM)` (or `DELIM)))`) is also accepted as the delimiter:
    /// the trailing `)` chars are rewound back into the input so the
    /// outer scanner / fork grammar can consume them.
    pub(super) fn read_heredoc_body(&mut self, delimiter: &str, strip_tabs: bool) -> String {
        let cmdsub_mode = self.mode == super::LexerMode::Cmdsub;
        let mut content = String::new();
        loop {
            if self.at_end() {
                break;
            }
            let ReadLine {
                line,
                eof_after_backslash,
                terminated_at_eof,
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
            if !matched && cmdsub_mode && terminated_at_eof {
                matched = self.try_rewind_cmdsub_close(&line, delimiter, strip_tabs);
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

    /// Reads one logical line of heredoc input in normalized form
    /// (line-continuations collapsed, trailing `\n` excluded, backslash-EOF
    /// doubled) plus termination flags.
    fn read_heredoc_line(&mut self) -> ReadLine {
        let mut line = String::new();
        let mut prev_backslash = false;
        let mut eof_after_backslash = false;
        let mut terminated_at_eof = true;
        while let Some(c) = self.peek_char() {
            self.advance_char();
            if c == '\n' {
                terminated_at_eof = false;
                break;
            }
            // Line continuation: \<newline> joins lines (but not \\<newline>)
            if c == '\\' && !prev_backslash && self.peek_char() == Some('\n') {
                self.advance_char();
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
            line,
            eof_after_backslash,
            terminated_at_eof,
        }
    }

    /// Cmdsub edge case: the delimiter line may be followed by the closing
    /// `)` of the outer command construct with no newline between (e.g.
    /// `$(cat <<E\nhello\nE)`). If the line has trailing `)` chars and
    /// stripping them yields the delimiter, rewind `self.pos` past those
    /// `)` chars so the outer scanner consumes them. Returns true if the
    /// rewind happened (and the caller should treat this as the delimiter
    /// line). Caller guarantees the line terminated at EOF, not at `\n`.
    fn try_rewind_cmdsub_close(&mut self, line: &str, delimiter: &str, strip_tabs: bool) -> bool {
        if !line.ends_with(')') {
            return false;
        }
        let n_paren = line.chars().rev().take_while(|&c| c == ')').count();
        // `)` is 1-byte ASCII so char count == byte count.
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
        self.pos -= n_paren;
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
