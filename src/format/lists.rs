//! List-operator (`;`, `&&`, `||`, `&`) formatting for the reformatter,
//! including the trailing-operator-before-heredoc special case.

use crate::ast::{ListItem, ListOperator};

use super::nodes::format_node;
use super::redirects::has_heredoc_redirect_deep;

pub(super) fn format_list(items: &[ListItem], out: &mut String, indent: usize) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            // Write the operator from the *previous* item
            if let Some(op) = items[i - 1].operator {
                let has_heredoc = has_heredoc_redirect_deep(&items[i - 1].command);
                if op == ListOperator::Semi && has_heredoc {
                    out.push('\n');
                } else {
                    format_list_op(op, out);
                }
            } else {
                out.push_str("; ");
            }
        }
        format_node(&item.command, out, indent);
    }
    // Write trailing operator on the last item (e.g., `cmd &`)
    if let Some(last) = items.last()
        && let Some(op) = last.operator
    {
        if has_heredoc_redirect_deep(&last.command) {
            insert_op_before_heredoc(op, out);
        } else {
            format_list_op(op, out);
        }
    }
}

/// Inserts a trailing operator (like `&`) on the delimiter line
/// before the heredoc content, rather than after it.
fn insert_op_before_heredoc(op: ListOperator, out: &mut String) {
    // The output currently ends with: `<<delim\ncontent\ndelim\n`
    // Find the first `\n` after the `<<` delimiter line to insert the
    // operator there: `<<delim &\ncontent\ndelim\n`
    if let Some(heredoc_pos) = out.rfind("<<")
        && let Some(nl_pos) = out[heredoc_pos..].find('\n')
    {
        let insert_at = heredoc_pos + nl_pos;
        let mut op_str = String::new();
        format_list_op(op, &mut op_str);
        out.insert_str(insert_at, op_str.trim_end());
        return;
    }
    // Fallback: just append
    format_list_op(op, out);
}

fn format_list_op(op: ListOperator, out: &mut String) {
    match op {
        ListOperator::And => out.push_str(" && "),
        ListOperator::Or => out.push_str(" || "),
        ListOperator::Semi => out.push_str("; "),
        ListOperator::Background => out.push_str(" & "),
    }
}
