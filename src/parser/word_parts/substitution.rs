//! Command, process, and arithmetic substitution (`$(…)`, `<(…)`, `$((…))`).
//! Depth-guarded via [`super::DepthGuard`] to prevent runaway recursion on
//! inputs like `$($($($($(…))))))`.

use crate::ast::{ListItem, Node, NodeKind, Span};
use crate::sexp::word::Substitution;

use super::{DepthGuard, literal_fallback};

/// Decomposes the inner text of a `$((...))` arithmetic substitution
/// into a typed `ArithmeticExpansion` node. On parse failure the
/// `expression` field is left as `None`, matching the existing best-effort
/// semantics used elsewhere in word decomposition.
pub(super) fn arithmetic_sub_to_node(inner: &str) -> Node {
    let expression = crate::parser::arithmetic::parse_arith_expression(inner)
        .ok()
        .map(Box::new);
    Node::empty(NodeKind::ArithmeticExpansion { expression })
}

pub(super) fn cmdsub_to_node(sub: &Substitution) -> Node {
    let content = &sub.content;
    sub_to_node(sub, &format!("$({content})"), |cmd| {
        // brace: false — this is $(...) syntax, not ${...} brace expansion
        NodeKind::CommandSubstitution {
            command: cmd,
            brace: false,
            reply: false,
        }
    })
}

/// `${ …; }` / `${| …; }`: a command substitution with `brace: true`,
/// and `reply` for `${| …; }`.
pub(super) fn funsub_to_node(pipe: bool, sub: &Substitution) -> Node {
    let open = crate::sexp::word::funsub_opener(pipe);
    let content = &sub.content;
    sub_to_node(sub, &format!("{open}{content}}}"), |cmd| {
        NodeKind::CommandSubstitution {
            command: cmd,
            brace: true,
            reply: pipe,
        }
    })
}

pub(super) fn procsub_to_node(direction: char, sub: &Substitution) -> Node {
    let content = &sub.content;
    sub_to_node(sub, &format!("{direction}({content})"), |cmd| {
        NodeKind::ProcessSubstitution {
            direction: direction.to_string(),
            command: cmd,
        }
    })
}

/// Builds a substitution node from the command list the lexer parsed for
/// it; backticks, which have none, are parsed from their text.
fn sub_to_node(
    sub: &Substitution,
    fallback_value: &str,
    make_kind: impl FnOnce(Box<Node>) -> NodeKind,
) -> Node {
    if let Some(body) = &sub.body {
        return Node::empty(make_kind(Box::new(Node::clone(body))));
    }
    let Some(_guard) = DepthGuard::enter() else {
        return literal_fallback(fallback_value);
    };
    crate::parse(&sub.content, false).map_or_else(
        |_| literal_fallback(fallback_value),
        |nodes| Node::empty(make_kind(Box::new(wrap_nodes(nodes)))),
    )
}

/// Wraps a list of parsed nodes into a single node.
fn wrap_nodes(mut nodes: Vec<Node>) -> Node {
    match nodes.len() {
        0 => Node::empty(NodeKind::Empty),
        1 => nodes.swap_remove(0),
        _ => Node::new(
            NodeKind::List {
                items: nodes
                    .into_iter()
                    .map(|n| ListItem {
                        command: n,
                        operator: None,
                    })
                    .collect(),
            },
            Span::empty(),
        ),
    }
}
