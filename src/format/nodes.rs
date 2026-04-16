//! Node-kind dispatch for the bash reformatter: `format_node` plus the
//! compound-construct formatters (if / while / for / case / function /
//! conditional expression).

use crate::ast::{CasePattern, Node, NodeKind};

use super::lists::format_list;
use super::redirects::{
    format_pipeline, format_redirect, format_redirect_inline, write_heredoc_body,
};
use super::words::{indent_str, process_word_value};

/// Formats a single AST node into canonical bash source.
#[allow(clippy::too_many_lines)]
pub(super) fn format_node(node: &Node, out: &mut String, indent: usize) {
    match &node.kind {
        NodeKind::Word { value, .. } => out.push_str(value),
        NodeKind::Command {
            assignments,
            words,
            redirects,
        } => {
            format_command(assignments, words, redirects, out);
        }
        NodeKind::Pipeline { commands, .. } => format_pipeline(commands, out, indent),
        NodeKind::List { items } => format_list(items, out, indent),
        NodeKind::If {
            condition,
            then_body,
            else_body,
            ..
        } => format_if(condition, then_body, else_body.as_deref(), out, indent),
        NodeKind::While {
            condition,
            body,
            redirects,
            ..
        } => format_while_until("while", condition, body, redirects, out, indent),
        NodeKind::Until {
            condition,
            body,
            redirects,
            ..
        } => format_while_until("until", condition, body, redirects, out, indent),
        NodeKind::For {
            var, words, body, ..
        } => format_for(var, words.as_deref(), body, out, indent),
        NodeKind::Select {
            var,
            words,
            body,
            redirects,
        } => format_select(var, words.as_deref(), body, redirects, out, indent),
        NodeKind::ForArith {
            init,
            cond,
            incr,
            body,
            ..
        } => format_for_arith(init, cond, incr, body, out, indent),
        NodeKind::Case {
            word,
            patterns,
            redirects,
            ..
        } => format_case(word, patterns, redirects, out, indent),
        NodeKind::Function { name, body } => {
            out.push_str("function ");
            out.push_str(name);
            out.push_str(" () \n");
            format_function_body(body, out, indent);
        }
        NodeKind::Subshell {
            body, redirects, ..
        } => {
            out.push_str("( ");
            format_node(body, out, indent);
            out.push_str(" )");
            for r in redirects {
                out.push(' ');
                format_redirect(r, out);
            }
        }
        NodeKind::BraceGroup {
            body, redirects, ..
        } => {
            out.push_str("{ ");
            format_node(body, out, indent);
            out.push_str("; }");
            for r in redirects {
                out.push(' ');
                format_redirect(r, out);
            }
        }
        NodeKind::Negation { pipeline } => {
            out.push_str("! ");
            format_node(pipeline, out, indent);
        }
        NodeKind::Time { pipeline, posix } => {
            if *posix {
                out.push_str("time -p ");
            } else {
                out.push_str("time ");
            }
            format_node(pipeline, out, indent);
        }
        NodeKind::Coproc { name, command } => {
            out.push_str("coproc ");
            if let Some(n) = name {
                out.push_str(n);
                out.push(' ');
            }
            format_node(command, out, indent);
        }
        NodeKind::ConditionalExpr { body, .. } => {
            out.push_str("[[ ");
            format_cond_node(body, out);
            out.push_str(" ]]");
        }
        NodeKind::Empty => {}
        _ => {
            out.push_str(&node.to_string());
        }
    }
}

pub(super) fn format_command(
    assignments: &[Node],
    words: &[Node],
    redirects: &[Node],
    out: &mut String,
) {
    format_command_words(assignments, words, out);
    // Phase 1: all redirect heads inline on the command line.
    // For heredocs this is just the `<<DELIM` part — the bodies are
    // deferred so bash's canonical form can put every op on the same
    // line regardless of heredoc ordering (see #39).
    for (i, r) in redirects.iter().enumerate() {
        if !assignments.is_empty() || !words.is_empty() || i > 0 {
            out.push(' ');
        }
        format_redirect_inline(r, out);
    }
    // Phase 2: concatenate heredoc bodies in source order after the
    // command line. Exactly one `\n` separates the command line from
    // the first body; consecutive bodies have none between them (each
    // body already ends with its delimiter's trailing `\n`).
    let mut first_heredoc = true;
    for r in redirects {
        if let NodeKind::HereDoc {
            delimiter, content, ..
        } = &r.kind
        {
            if first_heredoc {
                out.push('\n');
                first_heredoc = false;
            }
            write_heredoc_body(out, content, delimiter);
        }
    }
}

/// Writes assignments and command words as space-separated bash tokens.
pub(super) fn format_command_words(assignments: &[Node], words: &[Node], out: &mut String) {
    for (i, w) in assignments.iter().chain(words.iter()).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if let NodeKind::Word { value, spans, .. } = &w.kind {
            out.push_str(&process_word_value(value, spans));
        } else {
            out.push_str(&w.to_string());
        }
    }
}

fn format_if(
    condition: &Node,
    then_body: &Node,
    else_body: Option<&Node>,
    out: &mut String,
    indent: usize,
) {
    out.push_str("if ");
    format_node(condition, out, indent);
    out.push_str("; then\n");
    indent_str(out, indent + 4);
    format_node(then_body, out, indent + 4);
    out.push_str(";\n");
    if let Some(eb) = else_body {
        indent_str(out, indent);
        out.push_str("else\n");
        indent_str(out, indent + 4);
        format_node(eb, out, indent + 4);
        out.push_str(";\n");
    }
    indent_str(out, indent);
    out.push_str("fi");
}

#[allow(clippy::too_many_arguments)]
fn format_while_until(
    keyword: &str,
    condition: &Node,
    body: &Node,
    redirects: &[Node],
    out: &mut String,
    indent: usize,
) {
    out.push_str(keyword);
    out.push(' ');
    format_node(condition, out, indent);
    out.push_str("; do\n");
    indent_str(out, indent + 4);
    format_node(body, out, indent + 4);
    out.push_str(";\n");
    indent_str(out, indent);
    out.push_str("done");
    for r in redirects {
        out.push(' ');
        format_redirect(r, out);
    }
}

fn format_for(var: &str, words: Option<&[Node]>, body: &Node, out: &mut String, indent: usize) {
    format_for_or_select("for", var, words, body, &[], out, indent);
}

#[allow(clippy::too_many_arguments)]
fn format_select(
    var: &str,
    words: Option<&[Node]>,
    body: &Node,
    redirects: &[Node],
    out: &mut String,
    indent: usize,
) {
    format_for_or_select("select", var, words, body, redirects, out, indent);
}

/// Shared layout for `for` and `select` loops. `redirects` is only
/// non-empty for `select`, which lexes a trailing redirect list.
#[allow(clippy::too_many_arguments)]
fn format_for_or_select(
    keyword: &str,
    var: &str,
    words: Option<&[Node]>,
    body: &Node,
    redirects: &[Node],
    out: &mut String,
    indent: usize,
) {
    out.push_str(keyword);
    out.push(' ');
    out.push_str(var);
    if let Some(ws) = words {
        out.push_str(" in");
        for w in ws {
            out.push(' ');
            if let NodeKind::Word { value, .. } = &w.kind {
                out.push_str(value);
            }
        }
    }
    out.push_str(";\n");
    indent_str(out, indent);
    out.push_str("do\n");
    indent_str(out, indent + 4);
    format_node(body, out, indent + 4);
    out.push_str(";\n");
    indent_str(out, indent);
    out.push_str("done");
    for r in redirects {
        out.push(' ');
        format_redirect(r, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn format_for_arith(
    init: &str,
    cond: &str,
    incr: &str,
    body: &Node,
    out: &mut String,
    indent: usize,
) {
    out.push_str("for ((");
    out.push_str(init);
    out.push_str("; ");
    out.push_str(cond);
    out.push_str("; ");
    out.push_str(incr);
    out.push_str("))\n");
    indent_str(out, indent);
    out.push_str("do\n");
    indent_str(out, indent + 4);
    format_node(body, out, indent + 4);
    out.push_str(";\n");
    indent_str(out, indent);
    out.push_str("done");
}

fn format_case(
    word: &Node,
    patterns: &[CasePattern],
    redirects: &[Node],
    out: &mut String,
    indent: usize,
) {
    out.push_str("case ");
    if let NodeKind::Word { value, .. } = &word.kind {
        out.push_str(value);
    }
    out.push_str(" in ");
    for (i, p) in patterns.iter().enumerate() {
        if i > 0 {
            out.push('\n');
            indent_str(out, indent + 4);
        }
        for (j, pw) in p.patterns.iter().enumerate() {
            if j > 0 {
                out.push_str(" | ");
            }
            if let NodeKind::Word { value, .. } = &pw.kind {
                out.push_str(value);
            }
        }
        out.push_str(")\n");
        indent_str(out, indent + 8);
        if let Some(body) = &p.body {
            format_node(body, out, indent + 8);
        }
        out.push('\n');
        indent_str(out, indent + 4);
        out.push_str(&p.terminator);
    }
    out.push('\n');
    indent_str(out, indent);
    out.push_str("esac");
    for r in redirects {
        out.push(' ');
        format_redirect(r, out);
    }
}

fn format_function_body(body: &Node, out: &mut String, indent: usize) {
    // Bash's canonical form always wraps a function body in braces,
    // even when the source used a parens-only body like `function f ( … )`.
    out.push_str("{ \n");
    let inner = if let NodeKind::BraceGroup { body: inner, .. } = &body.kind {
        inner.as_ref()
    } else {
        body
    };
    if let NodeKind::List { items } = &inner.kind {
        super::lists::format_list_block(items, out, indent + 4);
    } else {
        indent_str(out, indent + 4);
        format_node(inner, out, indent + 4);
    }
    out.push('\n');
    indent_str(out, indent);
    out.push('}');
}

/// Formats a conditional expression node as canonical bash source.
fn format_cond_node(node: &Node, out: &mut String) {
    match &node.kind {
        NodeKind::UnaryTest { op, operand } => {
            out.push_str(op);
            out.push(' ');
            format_cond_node(operand, out);
        }
        NodeKind::BinaryTest { op, left, right } => {
            format_cond_node(left, out);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            format_cond_node(right, out);
        }
        NodeKind::CondAnd { left, right } => {
            format_cond_node(left, out);
            out.push_str(" && ");
            format_cond_node(right, out);
        }
        NodeKind::CondOr { left, right } => {
            format_cond_node(left, out);
            out.push_str(" || ");
            format_cond_node(right, out);
        }
        NodeKind::CondNot { operand } => {
            out.push_str("! ");
            format_cond_node(operand, out);
        }
        NodeKind::CondTerm { value, .. } => {
            out.push_str(value);
        }
        NodeKind::CondParen { inner } => {
            out.push_str("( ");
            format_cond_node(inner, out);
            out.push_str(" )");
        }
        _ => {
            out.push_str(&node.to_string());
        }
    }
}
