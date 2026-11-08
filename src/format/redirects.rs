//! Redirect + pipeline formatting. Heredoc bodies are queued on the
//! [`Formatter`] and written after the next connector's operator.

use crate::ast::{Node, NodeKind};

use super::formatter::Formatter;
use super::words::process_word_value;

impl Formatter {
    /// Emits a redirect after a compound command (`done <<E`), queueing a
    /// heredoc's body like [`Self::format_command`] does.
    pub(super) fn format_redirect(&mut self, node: &Node) {
        self.format_redirect_inline(node);
        if let NodeKind::HereDoc {
            delimiter, content, ..
        } = &node.kind
        {
            self.defer_heredoc(content, delimiter);
        }
    }

    /// Emits a redirect as it should appear on the command line: the full
    /// `op target` pair for a regular redirect, or just `<<DELIM` for a
    /// heredoc. The heredoc's body and closing delimiter are NOT emitted
    /// here — callers queue them with [`Self::defer_heredoc`] so
    /// multi-heredoc commands group all ops before any bodies (bash's
    /// canonical form).
    pub(super) fn format_redirect_inline(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::Redirect { .. } => self.format_regular_redirect(node),
            NodeKind::HereDoc { .. } => self.format_heredoc_open(node),
            _ => {}
        }
    }

    /// Emits a regular (non-heredoc) redirect: `[fd]op target`. Handles
    /// the `>&-` close-fd special case where the target fd is written
    /// before the operator (e.g. `2>&-`), the var-fd form `{name}`, and
    /// the no-space-before-target convention for dup redirects (`>&`,
    /// `<&`).
    fn format_regular_redirect(&mut self, node: &Node) {
        let NodeKind::Redirect {
            op,
            target,
            fd,
            varfd,
        } = &node.kind
        else {
            return;
        };
        // Close-fd redirects: >&- with target fd → output as "fd>&-"
        if op == ">&-" {
            if let Some(name) = varfd {
                self.write_char('{');
                self.write_str(name);
                self.write_char('}');
            } else if let NodeKind::Word { value, .. } = &target.kind {
                self.write_str(value);
            }
            self.write_str(">&-");
            return;
        }
        if let Some(name) = varfd {
            self.write_char('{');
            self.write_str(name);
            self.write_char('}');
        } else if *fd >= 0 && *fd != default_fd_for_op(op) {
            self.write_str(&fd.to_string());
        }
        self.write_str(op);
        // Dup redirects (>&, <&) don't need a space before target
        let is_dup = op == ">&" || op == "<&";
        if !is_dup {
            self.write_char(' ');
        }
        if let NodeKind::Word { value, spans, .. } = &target.kind {
            self.write_str(&process_word_value(value, spans));
        }
    }

    /// Emits just the `<<DELIM` opening of a heredoc (or `<<-DELIM`
    /// with tab-stripping). The body + closing delimiter are emitted
    /// separately via [`Self::defer_heredoc`] so multi-heredoc
    /// commands can group all inline ops before any body.
    fn format_heredoc_open(&mut self, node: &Node) {
        let NodeKind::HereDoc {
            delimiter,
            strip_tabs,
            quoted,
            ..
        } = &node.kind
        else {
            return;
        };
        let op = if *strip_tabs { "<<-" } else { "<<" };
        self.write_str(op);
        self.write_heredoc_delimiter(delimiter, *quoted);
    }

    /// Emits a heredoc opening delimiter, wrapping it in single quotes
    /// when the source used any quoting form (`<<'EOF'`, `<<"EOF"`,
    /// `<<\EOF`).
    pub(super) fn write_heredoc_delimiter(&mut self, delimiter: &str, quoted: bool) {
        if quoted {
            self.write_char('\'');
            self.write_str(delimiter);
            self.write_char('\'');
        } else {
            self.write_str(delimiter);
        }
    }

    pub(super) fn format_pipeline(&mut self, commands: &[Node]) {
        for (i, cmd) in commands.iter().enumerate() {
            if i > 0 {
                // ` |`, then the previous stage's heredoc bodies and their
                // space, then one more: `a <<E |\nx\nE\n  b`, or `a | b`.
                self.write_before_heredocs(" |");
                self.write_char(' ');
            }
            self.format_node(cmd);
        }
    }

    /// Writes trailing redirects (e.g. after `done` on a while/for loop,
    /// or after `esac` on a case) as space-separated `format_redirect`
    /// emissions. Each redirect is preceded by a single space. Used by
    /// compound-construct formatters that need to appendix a redirect
    /// list to their terminator keyword.
    pub(super) fn write_trailing_redirects(&mut self, redirects: &[Node]) {
        for r in redirects {
            self.write_char(' ');
            self.format_redirect(r);
        }
    }
}

const fn default_fd_for_op(op: &str) -> i32 {
    match op.as_bytes() {
        b">" | b">>" | b">|" | b">&" => 1,
        b"<" | b"<&" | b"<>" => 0,
        _ => -1,
    }
}
