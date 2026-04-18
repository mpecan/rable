use super::Lexer;

/// Parses a here-document delimiter word, stripping `'...'`, `"..."`, and
/// `\X` quoting. Returns the normalized delimiter and a flag indicating
/// whether any quoting was present (bash uses this to decide whether to
/// expand the body).
pub(crate) fn parse_heredoc_delimiter(raw: &str) -> (String, bool) {
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
pub(crate) struct PendingHereDoc {
    pub(crate) delimiter: String,
    pub(crate) strip_tabs: bool,
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
    pub(crate) fn queue_heredoc(&mut self, delimiter: String, strip_tabs: bool) {
        self.pending_heredocs.push(PendingHereDoc {
            delimiter,
            strip_tabs,
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
    /// fork (which process substitution also uses), a line of the form
    /// `DELIM<tail>` — where `<tail>` is any mix of spaces, tabs and
    /// `)` chars containing at least one `)` — is also accepted as the
    /// delimiter. The tail is rewound back into the input so the outer
    /// scanner / fork grammar can consume the `)` chars as subshell /
    /// cmdsub closers. Mirrors bash's "delimited by end-of-file"
    /// recovery path for heredocs whose terminator is on the same line
    /// as the enclosing construct's closing parens (issue #39).
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
            if !matched && cmdsub_mode {
                matched = self.try_match_sloppy_delimiter(
                    &line,
                    delimiter,
                    strip_tabs,
                    terminated_at_eof,
                );
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

    /// Cmdsub sloppy-delimiter recognition: bash accepts a line of the
    /// form `DELIM<tail>` as a heredoc terminator inside a `$(…)` /
    /// `<(…)` / `>(…)` fork when `<tail>` is whitespace plus close-parens
    /// and contains at least one `)`. The tail's close-parens are
    /// intended to close the enclosing subshell / cmdsub, so we rewind
    /// `self.pos` past the tail (and past the `\n` that terminated the
    /// line, if any) to leave those chars for the outer grammar to
    /// re-tokenize.
    ///
    /// Returns `true` if the line was a sloppy delimiter match. Called
    /// only in `LexerMode::Cmdsub` — top-level heredocs must use the
    /// strict delimiter rule.
    fn try_match_sloppy_delimiter(
        &mut self,
        line: &str,
        delimiter: &str,
        strip_tabs: bool,
        terminated_at_eof: bool,
    ) -> bool {
        let check = if strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line
        };
        let Some(tail) = check.strip_prefix(delimiter) else {
            return false;
        };
        // Tail must be non-empty, contain at least one `)`, and consist
        // only of whitespace or `)` characters. The "at least one `)`"
        // gate prevents pure-whitespace tails from matching — at the top
        // level bash does not accept `DELIM   ` as a terminator.
        if tail.is_empty() || !tail.contains(')') {
            return false;
        }
        if !tail.chars().all(|c| matches!(c, ' ' | '\t' | ')')) {
            return false;
        }
        // Rewind: leave `self.pos` just past the delimiter characters.
        // `read_heredoc_line` consumed `line.chars().count()` chars plus
        // 1 extra if the line was `\n`-terminated.
        let line_chars = line.chars().count();
        let consumed = line_chars + usize::from(!terminated_at_eof);
        let leading_tabs = if strip_tabs {
            line.chars().take_while(|&c| c == '\t').count()
        } else {
            0
        };
        let retain = leading_tabs + delimiter.chars().count();
        let rewind = consumed.saturating_sub(retain);
        self.pos -= rewind;
        if !terminated_at_eof {
            // The `\n` bump from `read_heredoc_line` must be undone: the
            // outer scanner will re-consume the `\n` and bump again.
            self.line = self.line.saturating_sub(1);
        }
        true
    }

    /// Takes the next completed here-doc content, if any.
    pub(crate) fn take_heredoc_content(&mut self) -> Option<String> {
        if self.heredoc_contents.is_empty() {
            None
        } else {
            Some(self.heredoc_contents.remove(0))
        }
    }
}
