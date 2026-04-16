//! Formatting helpers for command-like, compound, and conditional `NodeKind`
//! variants dispatched from the `Display for NodeKind` impl in `mod.rs`.

use std::fmt;

use crate::ast::{ListItem, ListOperator, Node, NodeKind};

use super::{
    write_escaped_word, write_optional, write_redirects, write_spaced3, write_tagged_list,
};

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

/// Formats simple command-like variants: `Command`, `Pipeline`, `List`,
/// `Empty`, `Comment`, `Negation`, `Time`, and `Array`.
pub(super) fn fmt_command_like(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::Command {
            assignments,
            words,
            redirects,
        } => write_spaced3(f, "(command", assignments, words, redirects),
        NodeKind::Pipeline { commands, .. } => write_pipeline(f, commands),
        NodeKind::List { items } => write_list(f, items),
        NodeKind::Empty => write!(f, "(command)"),
        NodeKind::Comment { text } => write!(f, "(comment \"{text}\")"),
        NodeKind::Negation { pipeline } => write!(f, "(negation {pipeline})"),
        NodeKind::Time { pipeline, posix } => {
            let tag = if *posix { "(time -p " } else { "(time " };
            write!(f, "{tag}{pipeline})")
        }
        NodeKind::Array { elements } => write_tagged_list(f, "array", elements),
        _ => unreachable!("fmt_command_like called with non-command-like variant"),
    }
}

/// Formats compound commands: `If`, `While`, `Until`, `For`, `ForArith`,
/// `Select`, `Case`, `Function`, `Subshell`, `BraceGroup`, and `Coproc`.
///
/// Dispatches to sub-helpers grouped by structure to keep each one small.
pub(super) fn fmt_compound(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::If { .. } | NodeKind::While { .. } | NodeKind::Until { .. } => {
            fmt_cond_body(f, kind)
        }
        NodeKind::For { .. }
        | NodeKind::ForArith { .. }
        | NodeKind::Select { .. }
        | NodeKind::Case { .. } => fmt_loop(f, kind),
        NodeKind::Function { .. }
        | NodeKind::Subshell { .. }
        | NodeKind::BraceGroup { .. }
        | NodeKind::Coproc { .. } => fmt_group(f, kind),
        _ => unreachable!("fmt_compound called with non-compound variant"),
    }
}

/// `if` / `while` / `until` — all take a condition plus body.
fn fmt_cond_body(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::If {
            condition,
            then_body,
            else_body,
            redirects,
        } => {
            write!(f, "(if {condition} {then_body}")?;
            write_optional(f, else_body.as_deref())?;
            write!(f, ")")?;
            write_redirects(f, redirects)
        }
        NodeKind::While {
            condition,
            body,
            redirects,
        } => {
            write!(f, "(while {condition} {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::Until {
            condition,
            body,
            redirects,
        } => {
            write!(f, "(until {condition} {body})")?;
            write_redirects(f, redirects)
        }
        _ => unreachable!("fmt_cond_body called with unexpected variant"),
    }
}

/// `for` / `arith-for` / `select` / `case` — iterate over a word list or
/// a set of patterns.
fn fmt_loop(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::For {
            var,
            words,
            body,
            redirects,
        } => {
            write!(f, "(for (word \"{var}\")")?;
            write_in_list(f, words.as_deref())?;
            write!(f, " {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::ForArith {
            init,
            cond,
            incr,
            body,
            redirects,
        } => {
            write!(f, "(arith-for (init (word \"")?;
            write_escaped_word(f, init)?;
            write!(f, "\")) (test (word \"")?;
            write_escaped_word(f, cond)?;
            write!(f, "\")) (step (word \"")?;
            write_escaped_word(f, incr)?;
            write!(f, "\")) {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::Select {
            var,
            words,
            body,
            redirects,
        } => {
            write!(f, "(select (word \"{var}\")")?;
            write_in_list(f, words.as_deref())?;
            write!(f, " {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::Case {
            word,
            patterns,
            redirects,
        } => {
            write!(f, "(case {word}")?;
            for p in patterns {
                write!(f, " {p}")?;
            }
            write!(f, ")")?;
            write_redirects(f, redirects)
        }
        _ => unreachable!("fmt_loop called with unexpected variant"),
    }
}

/// `function` / `subshell` / `brace-group` / `coproc` — grouped bodies.
fn fmt_group(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::Function { name, body } => write!(f, "(function \"{name}\" {body})"),
        NodeKind::Subshell { body, redirects } => {
            write!(f, "(subshell {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::BraceGroup { body, redirects } => {
            write!(f, "(brace-group {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::Coproc { name, command } => {
            let n = name.as_deref().unwrap_or("COPROC");
            write!(f, "(coproc \"{n}\" {command})")
        }
        _ => unreachable!("fmt_group called with unexpected variant"),
    }
}

/// Formats conditional expressions: `ConditionalExpr`, `UnaryTest`,
/// `BinaryTest`, `CondAnd`, `CondOr`, `CondNot`, and `CondParen`.
pub(super) fn fmt_conditional(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::ConditionalExpr { body, redirects } => {
            write!(f, "(cond {body})")?;
            write_redirects(f, redirects)
        }
        NodeKind::UnaryTest { op, operand } => write!(f, "(cond-unary \"{op}\" {operand})"),
        NodeKind::BinaryTest { op, left, right } => {
            write!(f, "(cond-binary \"{op}\" {left} {right})")
        }
        NodeKind::CondAnd { left, right } => write!(f, "(cond-and {left} {right})"),
        NodeKind::CondOr { left, right } => write!(f, "(cond-or {left} {right})"),
        // Parable drops negation in S-expression output — unwrap CondNot
        NodeKind::CondNot { operand } => write!(f, "{operand}"),
        NodeKind::CondParen { inner } => write!(f, "(cond-expr {inner})"),
        _ => unreachable!("fmt_conditional called with non-conditional variant"),
    }
}
