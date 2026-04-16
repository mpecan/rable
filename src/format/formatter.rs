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
}

impl Formatter {
    pub(super) const fn new() -> Self {
        Self {
            out: String::new(),
            indent: 0,
        }
    }

    /// Consume the formatter and return its output, trimming trailing
    /// spaces and tabs but **not** newlines — heredoc bodies end in
    /// `\n` and that terminator must survive.
    pub(super) fn finish(mut self) -> String {
        let trimmed_len = self.out.trim_end_matches([' ', '\t']).len();
        self.out.truncate(trimmed_len);
        self.out
    }

    pub(super) fn write_str(&mut self, s: &str) {
        self.out.push_str(s);
    }

    pub(super) fn write_char(&mut self, c: char) {
        self.out.push(c);
    }

    /// Write `n` spaces at the current position.
    pub(super) fn write_indent(&mut self, n: usize) {
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

    /// Read-only view of the accumulated output. Required by
    /// `insert_op_before_heredoc`, which has to `rfind` the heredoc
    /// delimiter before deciding where to splice in a trailing
    /// operator.
    pub(super) fn out(&self) -> &str {
        &self.out
    }

    /// Splice `s` into the output at byte offset `pos`. Used only by
    /// `insert_op_before_heredoc` for the post-hoc trailing-operator
    /// adjustment.
    pub(super) fn insert_at(&mut self, pos: usize, s: &str) {
        self.out.insert_str(pos, s);
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
