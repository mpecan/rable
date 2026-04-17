//! Helper functions for the parser.

use crate::ast::{Node, NodeKind};
use crate::token::Token;

/// Creates a `Word` node from a lexer token, moving value and spans.
pub fn word_node_from_token(tok: Token) -> Node {
    let parts = super::word_parts::decompose_word_with_spans(&tok.value, &tok.spans);
    Node::empty(NodeKind::Word {
        parts,
        value: tok.value,
        spans: tok.spans,
    })
}

/// Creates a `Word` node for synthetic values (no lexer token).
pub fn word_node(value: &str) -> Node {
    Node::empty(NodeKind::Word {
        parts: super::word_parts::decompose_word_literal(value),
        value: value.to_string(),
        spans: Vec::new(),
    })
}

/// Creates a `cond-term` node from a lexer token, moving value and spans.
pub(super) fn cond_term_from_token(tok: Token) -> Node {
    Node::empty(NodeKind::CondTerm {
        value: tok.value,
        spans: tok.spans,
    })
}

/// Returns true if the string is a valid file descriptor number.
pub(super) fn is_fd_number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit())
}

/// Returns true if the string is a variable fd reference like `{varname}`.
/// Requires valid bash variable name: starts with letter or `_`, then
/// alphanumeric or `_`.
pub(super) fn is_varfd(s: &str) -> bool {
    s.starts_with('{')
        && s.ends_with('}')
        && s.len() >= 3
        // First char must be letter or underscore (valid variable name start)
        && s.as_bytes()
            .get(1)
            .is_some_and(|&c| c.is_ascii_alphabetic() || c == b'_')
        && s[1..s.len() - 1]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns true if the string is a conditional binary operator.
pub(super) fn is_cond_binary_op(s: &str) -> bool {
    matches!(
        s,
        "==" | "!="
            | "=~"
            | "<"
            | ">"
            | "-eq"
            | "-ne"
            | "-lt"
            | "-le"
            | "-gt"
            | "-ge"
            | "-nt"
            | "-ot"
            | "-ef"
            | "="
    )
}

/// Adds a `(redirect ">&" 1)` to a command node for `|&` pipe-both.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn add_stderr_redirect(node: Option<&mut Node>) -> bool {
    if let Some(Node {
        kind: NodeKind::Command { redirects, .. },
        ..
    }) = node
    {
        redirects.push(make_stderr_redirect());
        true
    } else {
        false
    }
}

/// Creates a `(redirect ">&" 1)` node for pipe-both (|&) expansion.
/// fd=2 so the reformatter outputs `2>&1` (stderr dup to stdout).
pub(super) fn make_stderr_redirect() -> Node {
    Node::empty(NodeKind::Redirect {
        op: ">&".to_string(),
        target: Box::new(Node::empty(NodeKind::Word {
            value: "1".to_string(),
            parts: vec![Node::empty(NodeKind::WordLiteral {
                value: "1".to_string(),
            })],
            spans: Vec::new(),
        })),
        fd: 2,
        varfd: None,
    })
}

/// Walks an AST node and fills in empty `HereDoc` content from the lexer queue.
pub(super) fn fill_heredoc_contents(node: &mut Node, lexer: &mut crate::lexer::Lexer) {
    match &mut node.kind {
        NodeKind::HereDoc { content, .. } if content.is_empty() => {
            if let Some(c) = lexer.take_heredoc_content() {
                *content = c;
            }
        }
        NodeKind::Command {
            assignments,
            words,
            redirects,
        } => fill_command(assignments, words, redirects, lexer),
        NodeKind::Pipeline { commands, .. } => fill_each(commands, lexer),
        NodeKind::List { items } => {
            for item in items {
                fill_heredoc_contents(&mut item.command, lexer);
            }
        }
        NodeKind::If {
            condition,
            then_body,
            else_body,
            redirects,
        } => fill_if(
            condition,
            then_body,
            else_body.as_deref_mut(),
            redirects,
            lexer,
        ),
        NodeKind::While {
            condition,
            body,
            redirects,
        }
        | NodeKind::Until {
            condition,
            body,
            redirects,
        } => fill_cond_body_redirects(condition, body, redirects, lexer),
        NodeKind::Subshell { body, redirects }
        | NodeKind::BraceGroup { body, redirects }
        | NodeKind::For {
            body, redirects, ..
        }
        | NodeKind::Select {
            body, redirects, ..
        } => fill_body_and_redirects(body, redirects, lexer),
        NodeKind::Case {
            patterns,
            redirects,
            ..
        } => fill_case(patterns, redirects, lexer),
        NodeKind::Negation { pipeline } | NodeKind::Time { pipeline, .. } => {
            fill_heredoc_contents(pipeline, lexer);
        }
        NodeKind::Function { body, .. } | NodeKind::Coproc { command: body, .. } => {
            fill_heredoc_contents(body, lexer);
        }
        _ => {}
    }
}

/// Recurses `fill_heredoc_contents` into every node slot.
fn fill_each(nodes: &mut [Node], lexer: &mut crate::lexer::Lexer) {
    for n in nodes {
        fill_heredoc_contents(n, lexer);
    }
}

/// Walks `Command` constituents: assignments, then words, then redirects.
fn fill_command(
    assignments: &mut [Node],
    words: &mut [Node],
    redirects: &mut [Node],
    lexer: &mut crate::lexer::Lexer,
) {
    fill_each(assignments, lexer);
    fill_each(words, lexer);
    fill_each(redirects, lexer);
}

/// Walks `If` constituents: condition, then-body, optional else-body,
/// then redirects.
fn fill_if(
    condition: &mut Node,
    then_body: &mut Node,
    else_body: Option<&mut Node>,
    redirects: &mut [Node],
    lexer: &mut crate::lexer::Lexer,
) {
    fill_heredoc_contents(condition, lexer);
    fill_heredoc_contents(then_body, lexer);
    if let Some(eb) = else_body {
        fill_heredoc_contents(eb, lexer);
    }
    fill_each(redirects, lexer);
}

/// Walks `body` then its trailing redirects. Used by compound commands
/// that carry exactly a body + redirects pair (`Subshell`, `BraceGroup`,
/// `For`, `Select`).
fn fill_body_and_redirects(
    body: &mut Node,
    redirects: &mut [Node],
    lexer: &mut crate::lexer::Lexer,
) {
    fill_heredoc_contents(body, lexer);
    fill_each(redirects, lexer);
}

/// Walks `condition`, `body`, then the trailing redirects. Used by
/// `While` and `Until`.
fn fill_cond_body_redirects(
    condition: &mut Node,
    body: &mut Node,
    redirects: &mut [Node],
    lexer: &mut crate::lexer::Lexer,
) {
    fill_heredoc_contents(condition, lexer);
    fill_heredoc_contents(body, lexer);
    fill_each(redirects, lexer);
}

/// Walks each case pattern's optional body, then the trailing redirects.
fn fill_case(
    patterns: &mut [crate::ast::CasePattern],
    redirects: &mut [Node],
    lexer: &mut crate::lexer::Lexer,
) {
    for p in patterns {
        if let Some(body) = &mut p.body {
            fill_heredoc_contents(body, lexer);
        }
    }
    fill_each(redirects, lexer);
}
