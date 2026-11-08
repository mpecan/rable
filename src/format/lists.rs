//! List-operator (`;`, `&&`, `||`, `&`) formatting for the reformatter,
//! including operators that follow a command with heredoc bodies.

use crate::ast::{ListItem, ListOperator, Node, NodeKind};

use super::formatter::Formatter;

/// Returns the canonical surface syntax for a list operator, with
/// surrounding spaces as bash's canonical form prescribes.
pub(super) const fn list_op_str(op: ListOperator) -> &'static str {
    match op {
        ListOperator::And => " && ",
        ListOperator::Or => " || ",
        ListOperator::Semi => "; ",
        ListOperator::Newline => "\n",
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
            ListOperator::Semi => self.write_semi_connector(),
            ListOperator::Newline => self.write_newline_connector(),
            ListOperator::Background => {
                self.write_before_heredocs(" &");
                self.write_char(' ');
            }
            ListOperator::And | ListOperator::Or => {
                self.write_before_heredocs(list_op_str(op));
            }
        }
    }

    /// A `;` connector, as bash prints it: pending heredoc bodies replace
    /// the `;`, and right after bodies (`was_heredoc`) the `;` is dropped,
    /// so `a <<E; b; c` → `a <<E\nx\nE\n b c`. A space always follows.
    fn write_semi_connector(&mut self) {
        if !self.flush_heredocs() && !self.take_was_heredoc() {
            self.write_char(';');
        }
        self.write_char(' ');
    }

    /// A newline connector, which bash keeps inside a command substitution:
    /// pending heredoc bodies, then one newline (`a <<E\nb` →
    /// `a <<E\nx\nE\n\nb`).
    fn write_newline_connector(&mut self) {
        self.flush_heredocs();
        self.take_was_heredoc();
        self.write_char('\n');
    }

    /// Writes the last item's operator. After heredoc bodies `&` keeps
    /// bash's space (`$(cmd <<E &\nx\nE\n )`).
    fn write_trailing_list_op(&mut self, op: ListOperator) {
        match op {
            ListOperator::Semi => self.write_semi_connector(),
            _ if self.has_pending_heredocs() => {
                self.write_before_heredocs(list_op_str(op).trim_end());
            }
            _ => self.write_list_op(op),
        }
    }

    fn write_list_op(&mut self, op: ListOperator) {
        self.write_str(list_op_str(op));
    }

    /// Block-style list formatter for function body brace groups.
    ///
    /// Bash's canonical form only breaks lines on *statement terminators*
    /// — `;` and newlines. `&&`, `||` and `&` stay inline, so
    /// `hi; echo hi` becomes two lines, but `hi && echo hi` and
    /// `hi & echo hi` stay on one line (indented once). A newline
    /// separator leaves a blank line, as bash prints it.
    ///
    /// Distinct from [`Self::format_list`] above, which is the inline
    /// variant used at every non-function-body context.
    pub(super) fn format_list_block(&mut self, items: &[ListItem]) {
        let mut flat = Vec::new();
        flatten_list(items, &mut flat);
        self.indent_here();
        for (i, (command, operator)) in flat.iter().enumerate() {
            self.format_node(command);
            if i + 1 == flat.len() {
                // Only a trailing `&` is printed: `{ a & }` → `a &`.
                if *operator == Some(ListOperator::Background) {
                    self.write_before_heredocs(" &");
                }
                continue;
            }
            match operator {
                Some(ListOperator::Semi) | None => self.write_block_separator(';'),
                Some(ListOperator::Newline) => self.write_block_separator('\n'),
                Some(op) => self.write_list_connector(*op),
            }
        }
    }

    /// A `;` or newline between two commands of a function body. As in
    /// bash, heredoc bodies take the separator's place (and the next
    /// separator is dropped after them, see `was_heredoc`); a line break
    /// follows.
    fn write_block_separator(&mut self, separator: char) {
        if !self.flush_heredocs() && !self.take_was_heredoc() {
            self.write_char(separator);
        }
        self.write_char('\n');
        self.indent_here();
    }
}

/// Flattens the parser's left-nested binary lists (`((a ; b) ; c)`) into
/// one sequence of commands and the operators after them.
fn flatten_list<'a>(items: &'a [ListItem], out: &mut Vec<(&'a Node, Option<ListOperator>)>) {
    for item in items {
        match &item.command.kind {
            // An inner list ending in its own operator (`b &`) can only
            // be flattened where the outer item has none.
            NodeKind::List { items: inner }
                if item.operator.is_none()
                    || inner.last().is_some_and(|last| last.operator.is_none()) =>
            {
                flatten_list(inner, out);
                if let Some(last) = out.last_mut() {
                    last.1 = last.1.or(item.operator);
                }
            }
            _ => out.push((&item.command, item.operator)),
        }
    }
}
