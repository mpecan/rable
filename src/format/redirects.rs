//! Redirect + pipeline formatting. Heredoc bodies are queued on the
//! [`Formatter`] and written after the next connector's operator.

use crate::ast::{Node, NodeKind};

use super::formatter::Formatter;
use super::words::process_word_value;
use crate::sexp::redirects::fd_number;

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
        if op == ">&-" {
            // bash: `N>&-` for every close, whatever its direction.
            self.write_redirect_fd(varfd.as_deref(), *fd, true);
            self.write_str(">&-");
            return;
        }
        if op == ">&" || op == "<&" {
            self.format_dup_redirect(op, target, *fd, varfd.as_deref());
            return;
        }
        let NodeKind::Word { value, spans, .. } = &target.kind else {
            return;
        };
        self.write_redirect_fd(varfd.as_deref(), *fd, *fd != default_fd_for_op(op));
        self.write_str(op);
        self.write_char(' ');
        self.write_str(&process_word_value(value, spans));
    }

    /// `N>&M`, `N<&M`, and the moves `N>&M-` / `N<&M-`, following bash's
    /// `print_redirection`: the fd is always printed for a numeric target
    /// or a move (`>&2` → `1>&2`), and only when not the default for a
    /// word target (`>&$fd`).
    fn format_dup_redirect(&mut self, op: &str, target: &Node, fd: i32, varfd: Option<&str>) {
        let NodeKind::Word { value, spans, .. } = &target.kind else {
            return;
        };
        let moved = value.strip_suffix('-');
        let target = moved.unwrap_or(value);
        let number = fd_number(target);
        let show_fd = moved.is_some() || number.is_some() || fd != default_fd_for_op(op);
        self.write_redirect_fd(varfd, fd, show_fd);
        self.write_str(op);
        match number {
            Some(n) => self.write_str(&n.to_string()),
            None if moved.is_some() => self.write_str(target),
            None => self.write_str(&process_word_value(value, spans)),
        }
        if moved.is_some() {
            self.write_char('-');
        }
    }

    /// Writes a redirect's `{name}` or, when `show_fd`, its fd number.
    fn write_redirect_fd(&mut self, varfd: Option<&str>, fd: i32, show_fd: bool) {
        if let Some(name) = varfd {
            self.write_char('{');
            self.write_str(name);
            self.write_char('}');
        } else if show_fd && fd >= 0 {
            self.write_str(&fd.to_string());
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
            // `|&` after a compound command is a bare `2>&1` stage, as in
            // bash's own dump; bash prints it on the command: `( a ) 2>&1 | b`.
            if matches!(cmd.kind, NodeKind::Redirect { .. }) {
                self.write_char(' ');
                self.format_redirect(cmd);
                continue;
            }
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
