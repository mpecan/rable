//! List-operator (`;`, `&&`, `||`, `&`) formatting for the reformatter,
//! including operators that follow a command with heredoc bodies.

use crate::ast::{ListItem, ListOperator};

use super::formatter::Formatter;
use super::redirects::has_heredoc_redirect_deep;

/// Returns the canonical surface syntax for a list operator, with
/// surrounding spaces as bash's canonical form prescribes. Shared
/// between `write_list_op` and the heredoc splice in
/// `write_op_around_heredoc`.
pub(super) const fn list_op_str(op: ListOperator) -> &'static str {
    match op {
        ListOperator::And => " && ",
        ListOperator::Or => " || ",
        ListOperator::Semi => "; ",
        ListOperator::Background => " & ",
    }
}

impl Formatter {
    /// Inline list formatter — joins items with `; `, ` && `, ` || `,
    /// ` & ` on a single line. Used by the default `format_node`
    /// dispatch for `NodeKind::List` at every context except a function
    /// body's brace group (where [`Self::format_list_block`] produces
    /// the multi-line form).
    pub(super) fn format_list(&mut self, items: &[ListItem]) {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                // Write the operator from the *previous* item
                if let Some(op) = items[i - 1].operator {
                    let has_heredoc = has_heredoc_redirect_deep(&items[i - 1].command);
                    if op == ListOperator::Semi && has_heredoc {
                        self.write_char('\n');
                    } else if has_heredoc {
                        self.write_op_around_heredoc(op, true);
                    } else {
                        self.write_list_op(op);
                    }
                } else {
                    self.write_str("; ");
                }
            }
            self.take_heredoc_bodies_at();
            self.format_node(&item.command);
        }
        // Write trailing operator on the last item (e.g., `cmd &`)
        if let Some(last) = items.last()
            && let Some(op) = last.operator
        {
            if has_heredoc_redirect_deep(&last.command) {
                self.write_op_around_heredoc(op, false);
            } else {
                self.write_list_op(op);
            }
        }
    }

    /// Writes a list operator after a command whose heredoc bodies are
    /// already in the output, as bash's `print_deferred_heredocs` does:
    /// the operator goes on the delimiter line, and a space follows the
    /// bodies — two after `&` when a command follows. So `a <<E && b`
    /// becomes `a <<E && \nx\nE\n b`, and a trailing `cmd <<E &` ends in
    /// `E\n ` (bash's `$(… )`). A trailing `;` prints nothing.
    fn write_op_around_heredoc(&mut self, op: ListOperator, has_next: bool) {
        if op == ListOperator::Semi {
            return;
        }
        let Some(at) = self.take_heredoc_bodies_at() else {
            self.write_list_op(op);
            return;
        };
        let text = if op == ListOperator::Background {
            " &"
        } else {
            list_op_str(op)
        };
        self.insert_at(at, text);
        match (has_next, op) {
            (true, ListOperator::Background) => self.write_str("  "),
            (true, _) => self.write_char(' '),
            (false, _) => self.write_kept_space(),
        }
    }

    fn write_list_op(&mut self, op: ListOperator) {
        self.write_str(list_op_str(op));
    }

    /// Block-style list formatter for function body brace groups.
    ///
    /// Bash's canonical form only breaks lines on *statement terminators*
    /// — `;` and `&`. The short-circuit separators `&&` / `||` stay
    /// inline because they form a single logical command with their
    /// operands. So `hi; echo hi` becomes two lines, but `hi && echo hi`
    /// stays on one line (indented once).
    ///
    /// Distinct from [`Self::format_list`] above, which is the inline
    /// variant used at every non-function-body context.
    pub(super) fn format_list_block(&mut self, items: &[ListItem]) {
        for (i, item) in items.iter().enumerate() {
            if i == 0 {
                self.indent_here();
            }
            self.format_node(&item.command);
            if i + 1 >= items.len() {
                continue;
            }
            match item.operator {
                Some(ListOperator::And) => self.write_str(" && "),
                Some(ListOperator::Or) => self.write_str(" || "),
                Some(ListOperator::Background) => {
                    self.write_str(" &\n");
                    self.indent_here();
                }
                Some(ListOperator::Semi) | None => {
                    self.write_str(";\n");
                    self.indent_here();
                }
            }
        }
    }
}
