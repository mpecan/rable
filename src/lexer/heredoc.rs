use super::Lexer;

/// Pending here-document to be read after the current line.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PendingHereDoc {
    pub delimiter: String,
    pub strip_tabs: bool,
    pub quoted: bool,
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

    /// Reads a here-document body until the delimiter line.
    pub(super) fn read_heredoc_body(&mut self, delimiter: &str, strip_tabs: bool) -> String {
        let mut content = String::new();
        loop {
            if self.at_end() {
                break;
            }
            // Read a line
            let mut line = String::new();
            let mut prev_backslash = false;
            while let Some(c) = self.peek_char() {
                self.advance_char();
                if c == '\n' {
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
                } else {
                    prev_backslash = c == '\\' && !prev_backslash;
                    line.push(c);
                }
            }
            // Check if this line matches the delimiter
            let check_line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };
            // Match delimiter exactly, or with trailing whitespace
            // (bash allows trailing spaces on the delimiter line)
            if check_line == delimiter || check_line.trim_end() == delimiter {
                break;
            }
            if strip_tabs {
                content.push_str(line.trim_start_matches('\t'));
            } else {
                content.push_str(&line);
            }
            content.push('\n');
        }
        content
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
