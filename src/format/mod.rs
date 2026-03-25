//! Canonical bash formatter for command substitution content.
//!
//! Re-parses bash source and produces the canonical indented format
//! that Parable outputs inside `$(...)`.

use std::cell::Cell;

use crate::ast::{CasePattern, ListItem, ListOperator, Node, NodeKind};

thread_local! {
    static REFORMAT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard for the reformat depth counter.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        REFORMAT_DEPTH.with(|d| {
            let v = d.get();
            // Allow up to depth 2 for nested command substitutions
            if v >= 2 {
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

    // Always try to reformat if the content has any operators or special syntax.
    // The DepthGuard prevents recursion, and the 1000-char limit handles performance.
    let dominated_by_words = source
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.' || c == '/');
    if dominated_by_words {
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
    // Trim trailing spaces/tabs but NOT newlines (heredocs need those)
    let trimmed = out.trim_end_matches([' ', '\t']);
    Some(trimmed.to_string())
}

/// Formats a single AST node into canonical bash source.
#[allow(clippy::too_many_lines)]
fn format_node(node: &Node, out: &mut String, indent: usize) {
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
        NodeKind::ForArith {
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

fn format_command(assignments: &[Node], words: &[Node], redirects: &[Node], out: &mut String) {
    format_command_words(assignments, words, out);
    for (i, r) in redirects.iter().enumerate() {
        if !assignments.is_empty() || !words.is_empty() || i > 0 {
            out.push(' ');
        }
        format_redirect(r, out);
    }
}

/// Writes assignments and command words as space-separated bash tokens.
fn format_command_words(assignments: &[Node], words: &[Node], out: &mut String) {
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

fn format_redirect(node: &Node, out: &mut String) {
    if let NodeKind::Redirect { op, target, fd } = &node.kind {
        // Close-fd redirects: >&- with target fd → output as "fd>&-"
        if op == ">&-" {
            if let NodeKind::Word { value, .. } = &target.kind {
                out.push_str(value);
            }
            out.push_str(">&-");
            return;
        }
        if *fd >= 0 && *fd != default_fd_for_op(op) {
            out.push_str(&fd.to_string());
        }
        out.push_str(op);
        // Dup redirects (>&, <&) don't need a space before target
        let is_dup = op == ">&" || op == "<&";
        if !is_dup {
            out.push(' ');
        }
        if let NodeKind::Word { value, spans, .. } = &target.kind {
            out.push_str(&process_word_value(value, spans));
        }
    } else if let NodeKind::HereDoc {
        delimiter,
        content,
        strip_tabs,
        ..
    } = &node.kind
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
    for (i, cmd) in commands.iter().enumerate() {
        if i > 0 {
            // Check if previous command had a heredoc — pipe placement differs
            let prev_has_heredoc = has_heredoc_redirect_deep(&commands[i - 1]);
            if prev_has_heredoc {
                // Pipe was already placed on the heredoc delimiter line
                out.push_str("  ");
                format_node(cmd, out, indent);
                continue;
            }
            out.push_str(" | ");
        }
        // Check if this command has a heredoc redirect AND is not the last in pipeline
        if i + 1 < commands.len() && has_heredoc_redirect_deep(cmd) {
            format_command_with_heredoc_pipe(cmd, out);
        } else {
            format_node(cmd, out, indent);
        }
    }
}

/// Format a command that has a heredoc redirect, with ` |` placed on the delimiter line.
fn format_command_with_heredoc_pipe(node: &Node, out: &mut String) {
    if let NodeKind::Command {
        assignments,
        words,
        redirects,
    } = &node.kind
    {
        format_command_words(assignments, words, out);
        for r in redirects {
            if let NodeKind::HereDoc {
                delimiter,
                content,
                strip_tabs,
                ..
            } = &r.kind
            {
                let op = if *strip_tabs { " <<-" } else { " <<" };
                out.push_str(op);
                out.push_str(delimiter);
                out.push_str(" |\n"); // pipe on delimiter line
                out.push_str(content);
                out.push_str(delimiter);
                out.push('\n');
            } else {
                out.push(' ');
                format_redirect(r, out);
            }
        }
    }
}

fn format_list(items: &[ListItem], out: &mut String, indent: usize) {
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
    // Strategy: find the position of the first \n after the last `<<`
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

/// Check if a node (or its last sub-command) has heredoc redirects.
fn has_heredoc_redirect_deep(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Command { redirects, .. } => redirects
            .iter()
            .any(|r| matches!(r.kind, NodeKind::HereDoc { .. })),
        NodeKind::Pipeline { commands, .. } => {
            commands.last().is_some_and(has_heredoc_redirect_deep)
        }
        _ => false,
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
    if let NodeKind::BraceGroup { body: inner, .. } = &body.kind {
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

fn indent_str(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

/// Process a word value for canonical bash output using span-based
/// segment extraction.
fn process_word_value(value: &str, spans: &[crate::lexer::word_builder::WordSpan]) -> String {
    use crate::sexp::word::{WordSegment, segments_from_spans};

    let segments = segments_from_spans(value, spans);
    let mut result = String::with_capacity(value.len());

    for seg in &segments {
        match seg {
            WordSegment::Literal(text) => result.push_str(text),
            WordSegment::AnsiCQuote(raw_content) => {
                let chars: Vec<char> = raw_content.chars().collect();
                let mut pos = 0;
                let processed = crate::sexp::process_ansi_c_content(&chars, &mut pos);
                result.push('\'');
                result.push_str(&processed);
                result.push('\'');
            }
            WordSegment::LocaleString(content) => {
                // $"..." → "..." (content includes the "..." delimiters)
                result.push_str(content);
            }
            WordSegment::CommandSubstitution(content) => {
                result.push_str("$(");
                if let Some(reformatted) = reformat_bash(content) {
                    result.push_str(&reformatted);
                } else {
                    let normalized = crate::sexp::normalize_cmdsub_content(content);
                    result.push_str(&normalized);
                }
                result.push(')');
            }
            WordSegment::ProcessSubstitution(direction, content) => {
                result.push(*direction);
                result.push('(');
                if let Some(reformatted) = reformat_bash(content) {
                    result.push_str(&reformatted);
                } else {
                    let normalized = crate::sexp::normalize_cmdsub_content(content);
                    result.push_str(&normalized);
                }
                result.push(')');
            }
        }
    }
    result
}
