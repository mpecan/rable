//! Helper functions for the parser.

use crate::ast::Node;

/// Creates a `Word` node with no parts.
pub fn word_node(value: &str) -> Node {
    Node::Word {
        value: value.to_string(),
        parts: Vec::new(),
    }
}

/// Creates a `cond-term` node for conditional expressions.
pub(super) fn cond_term(value: &str) -> Node {
    Node::CondTerm {
        value: value.to_string(),
    }
}

/// Returns true if the string is a valid file descriptor number.
pub(super) fn is_fd_number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit())
}

/// Returns true if the string is a variable fd reference like `{varname}`.
pub(super) fn is_varfd(s: &str) -> bool {
    s.starts_with('{')
        && s.ends_with('}')
        && s.len() >= 3
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
    if let Some(Node::Command { redirects, .. }) = node {
        redirects.push(make_stderr_redirect());
        true
    } else {
        false
    }
}

/// Creates a `(redirect ">&" 1)` node for pipe-both (|&) expansion.
/// fd=2 so the reformatter outputs `2>&1` (stderr dup to stdout).
pub(super) fn make_stderr_redirect() -> Node {
    Node::Redirect {
        op: ">&".to_string(),
        target: Box::new(Node::Word {
            value: "1".to_string(),
            parts: Vec::new(),
        }),
        fd: 2,
    }
}

/// Parses a here-document delimiter, stripping quotes if present.
pub(super) fn parse_heredoc_delimiter(raw: &str) -> (String, bool) {
    let mut result = String::new();
    let mut quoted = false;
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    result.push(c);
                }
            }
            '"' => {
                quoted = true;
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    result.push(c);
                }
            }
            '\\' => {
                quoted = true;
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(c),
        }
    }
    (result, quoted)
}

/// Walks an AST node and fills in empty `HereDoc` content from the lexer queue.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub(super) fn fill_heredoc_contents(node: &mut Node, lexer: &mut crate::lexer::Lexer) {
    match node {
        Node::HereDoc { content, .. } if content.is_empty() => {
            if let Some(c) = lexer.take_heredoc_content() {
                *content = c;
            }
        }
        Node::Command { words, redirects } => {
            for w in words {
                fill_heredoc_contents(w, lexer);
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Pipeline { commands } => {
            for c in commands {
                fill_heredoc_contents(c, lexer);
            }
        }
        Node::List { parts } => {
            for p in parts {
                fill_heredoc_contents(p, lexer);
            }
        }
        Node::If {
            condition,
            then_body,
            else_body,
            redirects,
        } => {
            fill_heredoc_contents(condition, lexer);
            fill_heredoc_contents(then_body, lexer);
            if let Some(eb) = else_body {
                fill_heredoc_contents(eb, lexer);
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::While {
            condition,
            body,
            redirects,
        }
        | Node::Until {
            condition,
            body,
            redirects,
        } => {
            fill_heredoc_contents(condition, lexer);
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Subshell { body, redirects } | Node::BraceGroup { body, redirects } => {
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::For {
            body, redirects, ..
        }
        | Node::Select {
            body, redirects, ..
        } => {
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Case {
            patterns,
            redirects,
            ..
        } => {
            for p in patterns {
                if let Some(body) = &mut p.body {
                    fill_heredoc_contents(body, lexer);
                }
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Negation { pipeline } | Node::Time { pipeline, .. } => {
            fill_heredoc_contents(pipeline, lexer);
        }
        Node::Function { body, .. } | Node::Coproc { command: body, .. } => {
            fill_heredoc_contents(body, lexer);
        }
        _ => {}
    }
}
