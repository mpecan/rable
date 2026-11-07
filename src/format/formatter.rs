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
    /// Offset of the newline that starts the most recent command's
    /// heredoc bodies: where a following list operator is spliced in.
    heredoc_bodies_at: Option<usize>,
    /// End offset of a space written after heredoc bodies. Bash keeps it
    /// before the closing `)` (`$(cat <<E &\nx\nE\n )`), so `finish`
    /// must not trim it.
    kept_space_end: Option<usize>,
}

impl Formatter {
    pub(super) const fn new() -> Self {
        Self {
            out: String::new(),
            indent: 0,
            heredoc_bodies_at: None,
            kept_space_end: None,
        }
    }

    /// Consume the formatter and return its output, trimming trailing
    /// spaces and tabs but **not** newlines — heredoc bodies end in
    /// `\n` and that terminator must survive.
    pub(super) fn finish(mut self) -> String {
        if self.kept_space_end == Some(self.out.len()) {
            return self.out;
        }
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

    /// Records that the current command's heredoc bodies start here
    /// (just before the newline that ends its command line).
    pub(super) const fn mark_heredoc_bodies(&mut self) {
        self.heredoc_bodies_at = Some(self.out.len());
    }

    /// Takes the offset recorded by [`Self::mark_heredoc_bodies`].
    pub(super) const fn take_heredoc_bodies_at(&mut self) -> Option<usize> {
        self.heredoc_bodies_at.take()
    }

    /// Writes a single space that `finish` keeps even at the very end.
    pub(super) fn write_kept_space(&mut self) {
        self.out.push(' ');
        self.kept_space_end = Some(self.out.len());
    }

    /// Splice `s` into the output at byte offset `pos`. Used to put a
    /// list operator on a heredoc's delimiter line after its bodies were
    /// written.
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
