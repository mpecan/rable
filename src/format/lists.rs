//! List-operator (`;`, `&&`, `||`, `&`) formatting for the reformatter,
//! including the trailing-operator-before-heredoc special case.

use crate::ast::{ListItem, ListOperator};

use super::formatter::Formatter;
use super::redirects::has_heredoc_redirect_deep;

/// Returns the canonical surface syntax for a list operator, with
/// surrounding spaces as bash's canonical form prescribes. Shared
/// between `write_list_op` and the heredoc post-hoc insert in
/// `insert_op_before_heredoc`.
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
                    } else {
                        self.write_list_op(op);
                    }
                } else {
                    self.write_str("; ");
                }
            }
            self.format_node(&item.command);
        }
        // Write trailing operator on the last item (e.g., `cmd &`)
        if let Some(last) = items.last()
            && let Some(op) = last.operator
        {
            if has_heredoc_redirect_deep(&last.command) {
                self.insert_op_before_heredoc(op);
            } else {
                self.write_list_op(op);
            }
        }
    }

    /// Inserts a trailing operator (like `&`) on the delimiter line
    /// before the heredoc content, rather than after it.
    fn insert_op_before_heredoc(&mut self, op: ListOperator) {
        // The output currently ends with: `<<delim\ncontent\ndelim\n`
        // Find the first `\n` after the `<<` delimiter line to insert
        // the operator there: `<<delim &\ncontent\ndelim\n`
        if let Some(heredoc_pos) = self.out().rfind("<<")
            && let Some(nl_pos) = self.out()[heredoc_pos..].find('\n')
        {
            let insert_at = heredoc_pos + nl_pos;
            self.insert_at(insert_at, list_op_str(op).trim_end());
            return;
        }
        // Fallback: just append
        self.write_list_op(op);
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
