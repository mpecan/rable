//! Pipeline, list, and `(in …)` word-list formatting helpers for the
//! `Display for NodeKind` impl in `mod.rs`.

use std::fmt;

use crate::ast::{ListItem, ListOperator, Node, NodeKind};

/// Pipelines are right-nested: `(pipe a (pipe b c))`.
pub(super) fn write_pipeline(f: &mut fmt::Formatter<'_>, commands: &[Node]) -> fmt::Result {
    if commands.len() == 1 {
        return write!(f, "{}", commands[0]);
    }
    // Group commands with their trailing redirects
    let mut groups: Vec<Vec<&Node>> = Vec::new();
    for cmd in commands {
        if matches!(cmd.kind, NodeKind::Redirect { .. }) {
            // Attach redirect to the previous group
            if let Some(last) = groups.last_mut() {
                last.push(cmd);
            } else {
                groups.push(vec![cmd]);
            }
        } else {
            groups.push(vec![cmd]);
        }
    }
    write_pipeline_groups(f, &groups, 0)
}

fn write_pipeline_groups(
    f: &mut fmt::Formatter<'_>,
    groups: &[Vec<&Node>],
    idx: usize,
) -> fmt::Result {
    if idx >= groups.len() {
        return Ok(());
    }
    if idx == groups.len() - 1 {
        // Last group: write all elements
        for (j, node) in groups[idx].iter().enumerate() {
            if j > 0 {
                write!(f, " ")?;
            }
            write!(f, "{node}")?;
        }
        return Ok(());
    }
    write!(f, "(pipe ")?;
    for (j, node) in groups[idx].iter().enumerate() {
        if j > 0 {
            write!(f, " ")?;
        }
        write!(f, "{node}")?;
    }
    write!(f, " ")?;
    write_pipeline_groups(f, groups, idx + 1)?;
    write!(f, ")")
}

/// Lists use left-associative nesting: `(and (and a b) c)`.
pub(super) fn write_list(f: &mut fmt::Formatter<'_>, items: &[ListItem]) -> fmt::Result {
    if items.len() == 1 && items[0].operator.is_none() {
        return write!(f, "{}", items[0].command);
    }
    let cmds: Vec<&Node> = items.iter().map(|i| &i.command).collect();
    let ops: Vec<ListOperator> = items.iter().filter_map(|i| i.operator).collect();
    write_list_left_assoc(f, &cmds, &ops)
}

fn write_list_left_assoc(
    f: &mut fmt::Formatter<'_>,
    items: &[&Node],
    ops: &[ListOperator],
) -> fmt::Result {
    // Handle trailing unary operator (e.g., "cmd &" → "(background cmd)")
    if items.len() == 1 && ops.len() == 1 {
        let sexp_op = list_op_name(ops[0]);
        return write!(f, "({sexp_op} {})", items[0]);
    }
    if items.len() <= 1 && ops.is_empty() {
        if let Some(item) = items.first() {
            return write!(f, "{item}");
        }
        return Ok(());
    }

    // For a trailing background operator with no RHS
    if items.len() == ops.len() {
        let sexp_op = list_op_name(ops[ops.len() - 1]);
        write!(f, "({sexp_op} ")?;
        write_list_left_assoc(f, items, &ops[..ops.len() - 1])?;
        return write!(f, ")");
    }

    // Write left-associatively: ((op1 a b) op2 c) op3 d) ...
    for i in (1..ops.len()).rev() {
        write!(f, "({} ", list_op_name(ops[i]))?;
    }
    write!(f, "({} {} {})", list_op_name(ops[0]), items[0], items[1])?;
    for i in 1..ops.len() {
        write!(f, " {})", items[i + 1])?;
    }
    Ok(())
}

const fn list_op_name(op: ListOperator) -> &'static str {
    match op {
        ListOperator::And => "and",
        ListOperator::Or => "or",
        ListOperator::Semi => "semi",
        ListOperator::Background => "background",
    }
}

/// Writes a word list wrapped in `(in ...)` for `for`/`select` statements.
pub(super) fn write_in_list(f: &mut fmt::Formatter<'_>, words: Option<&[Node]>) -> fmt::Result {
    if let Some(ws) = words {
        write!(f, " (in")?;
        for w in ws {
            write!(f, " {w}")?;
        }
        write!(f, ")")?;
    }
    Ok(())
}
