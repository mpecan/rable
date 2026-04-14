//! Redirect + pipeline formatting, including the heredoc/pipe interaction
//! logic used when a pipe command has a heredoc redirect.

use crate::ast::{Node, NodeKind};

use super::nodes::{format_command_words, format_node};
use super::words::process_word_value;

pub(super) fn format_redirect(node: &Node, out: &mut String) {
    if let NodeKind::Redirect {
        op,
        target,
        fd,
        varfd,
    } = &node.kind
    {
        // Close-fd redirects: >&- with target fd → output as "fd>&-"
        if op == ">&-" {
            if let Some(name) = varfd {
                out.push('{');
                out.push_str(name);
                out.push('}');
            } else if let NodeKind::Word { value, .. } = &target.kind {
                out.push_str(value);
            }
            out.push_str(">&-");
            return;
        }
        if let Some(name) = varfd {
            out.push('{');
            out.push_str(name);
            out.push('}');
        } else if *fd >= 0 && *fd != default_fd_for_op(op) {
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
        quoted,
        ..
    } = &node.kind
    {
        let op = if *strip_tabs { "<<-" } else { "<<" };
        out.push_str(op);
        write_heredoc_delimiter(out, delimiter, *quoted);
        out.push('\n');
        out.push_str(content);
        out.push_str(delimiter);
        out.push('\n');
    }
}

/// Emits a heredoc opening delimiter, wrapping it in single quotes when
/// the source used any quoting form (`<<'EOF'`, `<<"EOF"`, `<<\EOF`).
fn write_heredoc_delimiter(out: &mut String, delimiter: &str, quoted: bool) {
    if quoted {
        out.push('\'');
        out.push_str(delimiter);
        out.push('\'');
    } else {
        out.push_str(delimiter);
    }
}

const fn default_fd_for_op(op: &str) -> i32 {
    match op.as_bytes() {
        b">" | b">>" | b">|" | b">&" => 1,
        b"<" | b"<&" | b"<>" => 0,
        _ => -1,
    }
}

pub(super) fn format_pipeline(commands: &[Node], out: &mut String, indent: usize) {
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
                quoted,
                ..
            } = &r.kind
            {
                let op = if *strip_tabs { " <<-" } else { " <<" };
                out.push_str(op);
                write_heredoc_delimiter(out, delimiter, *quoted);
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

/// Check if a node (or its last sub-command) has heredoc redirects.
pub(super) fn has_heredoc_redirect_deep(node: &Node) -> bool {
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
