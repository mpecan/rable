//! The [`Formatter`] struct holds the reformatter's output buffer and
//! current indentation level. Per-topic `impl Formatter` blocks live in
//! the sibling files (`nodes.rs`, `compound.rs`, `lists.rs`,
//! `redirects.rs`) — this file only defines the struct and the low-level
//! write/indent primitives they build on.

/// Reformatter state: accumulated output and current indentation.
/// Constructed by [`reformat_bash`](super::reformat_bash); per-topic
/// methods live in sibling files under `impl Formatter`.
pub(super) struct Formatter {
    out: String,
    indent: usize,
    /// Heredoc bodies (content + delimiter + `\n`) of the last command,
    /// not yet written. As in bash's `print_deferred_heredocs`, the next
    /// write flushes them, except that a connector writes its operator
    /// first (`a <<E && b` → `a <<E && \nx\nE\n b`).
    pending_heredocs: Vec<String>,
    /// End offset of a space bash keeps at the very end, e.g. after
    /// heredoc bodies (`$(cat <<E &\nx\nE\n )`) or a bare `!` (`$(! )`),
    /// so `finish` must not trim it.
    kept_space_end: Option<usize>,
    /// bash's `was_heredoc`: heredoc bodies were the last thing printed,
    /// so the next `;` connector is not printed (`a <<E && b; c` →
    /// `a <<E && \nx\nE\n b c`). Reset by a command's redirects and by
    /// the end of a subshell or group, as in `print_cmd.c`.
    was_heredoc: bool,
}

impl Formatter {
    pub(super) const fn new() -> Self {
        Self {
            out: String::new(),
            indent: 0,
            pending_heredocs: Vec::new(),
            kept_space_end: None,
            was_heredoc: false,
        }
    }

    /// Consume the formatter and return its output, trimming trailing
    /// spaces and tabs but **not** newlines — heredoc bodies end in
    /// `\n` and that terminator must survive.
    pub(super) fn finish(mut self) -> String {
        self.flush_heredocs();
        if self.kept_space_end == Some(self.out.len()) {
            return self.out;
        }
        let trimmed_len = self.out.trim_end_matches([' ', '\t']).len();
        self.out.truncate(trimmed_len);
        self.out
    }

    /// Writes `s`, after any pending heredoc bodies.
    pub(super) fn write_str(&mut self, s: &str) {
        self.flush_heredocs();
        self.out.push_str(s);
    }

    /// Writes `c`, after any pending heredoc bodies.
    pub(super) fn write_char(&mut self, c: char) {
        self.flush_heredocs();
        self.out.push(c);
    }

    /// Write `n` spaces at the current position.
    pub(super) fn write_indent(&mut self, n: usize) {
        self.flush_heredocs();
        for _ in 0..n {
            self.out.push(' ');
        }
    }

    /// Write `self.indent` spaces at the current position.
    pub(super) fn indent_here(&mut self) {
        self.write_indent(self.indent);
    }

    /// Run `f` with `indent += delta`, then restore. Guarantees the
    /// increment is always matched by a decrement.
    pub(super) fn with_indent<F: FnOnce(&mut Self)>(&mut self, delta: usize, f: F) {
        self.indent += delta;
        f(self);
        self.indent -= delta;
    }

    /// Queues a heredoc body for the next flush.
    pub(super) fn defer_heredoc(&mut self, content: &str, delimiter: &str) {
        self.pending_heredocs
            .push(format!("{content}{delimiter}\n"));
    }

    /// Whether heredoc bodies are waiting to be written.
    pub(super) const fn has_pending_heredocs(&self) -> bool {
        !self.pending_heredocs.is_empty()
    }

    /// Writes a connector's operator ahead of the pending heredoc bodies,
    /// then the bodies and the space bash prints after them, which is
    /// kept even at the end (`$(a <<E &\nx\nE\n )`). Returns whether
    /// there were bodies.
    pub(super) fn write_before_heredocs(&mut self, op: &str) -> bool {
        self.out.push_str(op);
        let flushed = self.flush_heredocs();
        if flushed {
            self.out.push(' ');
            self.kept_space_end = Some(self.out.len());
        }
        flushed
    }

    /// Writes the pending heredoc bodies: one `\n` ends the command line,
    /// then each body in source order. Returns whether there were any.
    pub(super) fn flush_heredocs(&mut self) -> bool {
        if self.pending_heredocs.is_empty() {
            return false;
        }
        self.out.push('\n');
        for body in self.pending_heredocs.drain(..) {
            self.out.push_str(&body);
        }
        self.was_heredoc = true;
        true
    }

    /// bash's `semicolon()`: writes `;` unless the output already ends a
    /// line (heredoc bodies) or a background command (` &`).
    pub(super) fn semicolon(&mut self) {
        self.flush_heredocs();
        if !(self.out.ends_with('\n') || self.out.ends_with(" &")) {
            self.out.push(';');
        }
    }

    /// Takes bash's `was_heredoc` flag (see the field).
    pub(super) const fn take_was_heredoc(&mut self) -> bool {
        let was = self.was_heredoc;
        self.was_heredoc = false;
        was
    }

    /// Writes a single space that `finish` keeps even at the very end.
    pub(super) fn write_kept_space(&mut self) {
        self.flush_heredocs();
        self.out.push(' ');
        self.kept_space_end = Some(self.out.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_trims_trailing_spaces_and_tabs_but_keeps_newlines() {
        // The trim contract preserves trailing `\n` because heredoc
        // bodies end in a newline the caller relies on.
        let mut f = Formatter::new();
        f.write_str("hello\n   \t  ");
        assert_eq!(f.finish(), "hello\n");

        let mut f = Formatter::new();
        f.write_str("body\n");
        assert_eq!(f.finish(), "body\n");

        let mut f = Formatter::new();
        f.write_str("  ");
        assert_eq!(f.finish(), "");
    }

    #[test]
    fn with_indent_restores_on_normal_return() {
        let mut f = Formatter::new();
        f.with_indent(4, |g| {
            g.indent_here();
            g.write_str("x");
        });
        f.write_str("|after");
        assert_eq!(f.finish(), "    x|after");
    }
}
