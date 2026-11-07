//! List-operator (`;`, `&&`, `||`, `&`) formatting for the reformatter,
//! including operators that follow a command with heredoc bodies.

use crate::ast::{ListItem, ListOperator};

use super::formatter::Formatter;

/// Returns the canonical surface syntax for a list operator, with
/// surrounding spaces as bash's canonical form prescribes.
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
                match items[i - 1].operator {
                    Some(op) => self.write_list_connector(op),
                    None => self.write_str("; "),
                }
            }
            self.format_node(&item.command);
        }
        // Trailing operator on the last item (e.g., `cmd &`)
        if let Some(op) = items.last().and_then(|item| item.operator) {
            self.write_trailing_list_op(op);
        }
    }

    /// Writes the operator between two list items. As bash's
    /// `print_deferred_heredocs` does, the operator comes before any
    /// pending heredoc bodies and a space follows them, two after `&`:
    /// `a <<E && b` → `a <<E && \nx\nE\n b`.
    fn write_list_connector(&mut self, op: ListOperator) {
        match op {
            ListOperator::Semi if self.has_pending_heredocs() => self.write_char('\n'),
            ListOperator::Semi => self.write_list_op(op),
            ListOperator::Background => {
                self.write_before_heredocs(" &");
                self.write_char(' ');
            }
            ListOperator::And | ListOperator::Or => {
                self.write_before_heredocs(list_op_str(op));
            }
        }
    }

    /// Writes the last item's operator. After heredoc bodies `&` keeps
    /// bash's space (`$(cmd <<E &\nx\nE\n )`) and `;` prints nothing.
    fn write_trailing_list_op(&mut self, op: ListOperator) {
        if !self.has_pending_heredocs() {
            self.write_list_op(op);
        } else if op != ListOperator::Semi {
            self.write_before_heredocs(list_op_str(op).trim_end());
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
