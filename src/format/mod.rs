//! Canonical bash formatter for command substitution content.
//!
//! Re-parses bash source and produces the canonical indented format
//! that Parable outputs inside `$(...)`.

use std::cell::Cell;

use crate::ast::{CasePattern, Node};

thread_local! {
    static REFORMAT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard for the reformat depth counter.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        REFORMAT_DEPTH.with(|d| {
            let v = d.get();
            if v > 0 {
                return None;
            }
            d.set(v + 1);
            Some(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        REFORMAT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Attempts to reformat bash source into canonical form.
/// Returns `None` if parsing fails (in which case raw text is used).
pub fn reformat_bash(source: &str) -> Option<String> {
    if source.is_empty() || source.len() > 1000 {
        return None;
    }
    let _guard = DepthGuard::enter()?;

    let needs_reformat = source.contains("if ")
        || source.contains("while ")
        || source.contains("until ")
        || source.contains("for ")
        || source.contains("case ")
        || source.contains("function ")
        || source.contains("() ")
        || source.contains('|')
        || source.contains("<<")
        || source.contains(">&")
        || source.contains("<&")
        || source.contains("$\"")
        || source.contains("[[")
        || source.contains("! ")
        || source.contains('>')
        || source.contains('<')
        || has_leading_paren(source);
    if !needs_reformat {
        return None;
    }

    let nodes = crate::parse(source, false).ok()?;
    if nodes.is_empty() {
        return Some(String::new());
    }
    let mut out = String::new();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_node(node, &mut out, 0);
    }
    Some(out)
}

/// Formats a single AST node into canonical bash source.
#[allow(clippy::too_many_lines)]
fn format_node(node: &Node, out: &mut String, indent: usize) {
    match node {
        Node::Word { value, .. } => out.push_str(value),
        Node::Command { words, redirects } => {
            format_command(words, redirects, out);
        }
        Node::Pipeline { commands } => format_pipeline(commands, out, indent),
        Node::List { parts } => format_list(parts, out, indent),
        Node::If {
            condition,
            then_body,
            else_body,
            ..
        } => format_if(condition, then_body, else_body.as_deref(), out, indent),
        Node::While {
            condition,
            body,
            redirects,
            ..
        } => format_while_until("while", condition, body, redirects, out, indent),
        Node::Until {
            condition,
            body,
            redirects,
            ..
        } => format_while_until("until", condition, body, redirects, out, indent),
        Node::For {
            var, words, body, ..
        } => format_for(var, words.as_deref(), body, out, indent),
        Node::ForArith {
            init,
            cond,
            incr,
            body,
            ..
        } => {
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
        Node::Case {
            word,
            patterns,
            redirects,
            ..
        } => format_case(word, patterns, redirects, out, indent),
        Node::Function { name, body } => {
            out.push_str("function ");
            out.push_str(name);
            out.push_str(" () \n");
            format_function_body(body, out, indent);
        }
        Node::Subshell {
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
        Node::BraceGroup {
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
        Node::Negation { pipeline } => {
            out.push_str("! ");
            format_node(pipeline, out, indent);
        }
        Node::Time { pipeline, posix } => {
            if *posix {
                out.push_str("time -p ");
            } else {
                out.push_str("time ");
            }
            format_node(pipeline, out, indent);
        }
        Node::Coproc { name, command } => {
            out.push_str("coproc ");
            if let Some(n) = name {
                out.push_str(n);
                out.push(' ');
            }
            format_node(command, out, indent);
        }
        Node::ConditionalExpr { body, .. } => {
            out.push_str("[[ ");
            format_cond_node(body, out);
            out.push_str(" ]]");
        }
        Node::Empty => {}
        _ => {
            out.push_str(&node.to_string());
        }
    }
}

fn format_command(words: &[Node], redirects: &[Node], out: &mut String) {
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if let Node::Word { value, .. } = w {
            // Strip $"..." locale prefix → "..."
            out.push_str(&strip_locale_prefix(value));
        } else {
            out.push_str(&w.to_string());
        }
    }
    for (i, r) in redirects.iter().enumerate() {
        if !words.is_empty() || i > 0 {
            out.push(' ');
        }
        format_redirect(r, out);
    }
}

fn format_redirect(node: &Node, out: &mut String) {
    if let Node::Redirect { op, target, fd } = node {
        if *fd >= 0 && *fd != default_fd_for_op(op) {
            out.push_str(&fd.to_string());
        }
        out.push_str(op);
        // Dup redirects (>&, <&) don't need a space before target
        let is_dup = op == ">&" || op == "<&";
        if !is_dup {
            out.push(' ');
        }
        if let Node::Word { value, .. } = target.as_ref() {
            out.push_str(value);
        }
    } else if let Node::HereDoc {
        delimiter,
        content,
        strip_tabs,
        ..
    } = node
    {
        let op = if *strip_tabs { "<<-" } else { "<<" };
        out.push_str(op);
        out.push_str(delimiter);
        out.push('\n');
        out.push_str(content);
        out.push_str(delimiter);
        out.push('\n');
    }
}

const fn default_fd_for_op(op: &str) -> i32 {
    match op.as_bytes() {
        b">" | b">>" | b">|" | b">&" => 1,
        b"<" | b"<&" | b"<>" => 0,
        _ => -1,
    }
}

fn format_pipeline(commands: &[Node], out: &mut String, indent: usize) {
    let filtered: Vec<_> = commands
        .iter()
        .filter(|c| !matches!(c, Node::PipeBoth))
        .collect();
    for (i, cmd) in filtered.iter().enumerate() {
        if i > 0 {
            out.push_str(" | ");
        }
        format_node(cmd, out, indent);
    }
}

fn format_list(parts: &[Node], out: &mut String, indent: usize) {
    for (i, part) in parts.iter().enumerate() {
        if let Node::Operator { op } = part {
            match op.as_str() {
                "&&" => out.push_str(" && "),
                "||" => out.push_str(" || "),
                ";" => out.push_str("; "),
                "&" => out.push_str(" & "),
                _ => out.push_str(op),
            }
        } else if i > 0 && !matches!(parts.get(i - 1), Some(Node::Operator { .. })) {
            out.push_str("; ");
            format_node(part, out, indent);
        } else {
            format_node(part, out, indent);
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
    out.push_str("for ");
    out.push_str(var);
    if let Some(ws) = words {
        out.push_str(" in");
        for w in ws {
            out.push(' ');
            if let Node::Word { value, .. } = w {
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
}

fn format_case(
    word: &Node,
    patterns: &[CasePattern],
    redirects: &[Node],
    out: &mut String,
    indent: usize,
) {
    out.push_str("case ");
    if let Node::Word { value, .. } = word {
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
            if let Node::Word { value, .. } = pw {
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
    if let Node::BraceGroup { body: inner, .. } = body {
        out.push_str("{ \n");
        indent_str(out, indent + 4);
        format_node(inner, out, indent + 4);
        out.push('\n');
        indent_str(out, indent);
        out.push('}');
    } else {
        format_node(body, out, indent);
    }
}

/// Formats a conditional expression node as canonical bash source.
fn format_cond_node(node: &Node, out: &mut String) {
    match node {
        Node::UnaryTest { op, operand } => {
            out.push_str(op);
            out.push(' ');
            format_cond_node(operand, out);
        }
        Node::BinaryTest { op, left, right } => {
            format_cond_node(left, out);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            format_cond_node(right, out);
        }
        Node::CondAnd { left, right } => {
            format_cond_node(left, out);
            out.push_str(" && ");
            format_cond_node(right, out);
        }
        Node::CondOr { left, right } => {
            format_cond_node(left, out);
            out.push_str(" || ");
            format_cond_node(right, out);
        }
        Node::CondNot { operand } => {
            out.push_str("! ");
            format_cond_node(operand, out);
        }
        Node::CondTerm { value } => {
            out.push_str(value);
        }
        Node::CondParen { inner } => {
            out.push_str("( ");
            format_cond_node(inner, out);
            out.push_str(" )");
        }
        _ => {
            out.push_str(&node.to_string());
        }
    }
}

fn indent_str(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

/// Check if source starts with `(` (subshell/grouping).
fn has_leading_paren(source: &str) -> bool {
    source.trim_start().starts_with('(')
}

/// Strip `$"..."` locale prefix from word values, turning them into `"..."`.
fn strip_locale_prefix(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '"' {
            // Skip the $ prefix
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}
