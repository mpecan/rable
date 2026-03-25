//! Decomposes word values into structured AST parts.
//!
//! Reuses the segment parser from `sexp::word` to split a word's raw
//! value into typed segments, then maps each segment to an AST `Node`.

use std::cell::Cell;

use crate::ast::{ListItem, Node, NodeKind, Span};
use crate::sexp::word::{WordSegment, parse_word_segments};

thread_local! {
    static DECOMPOSE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard to prevent infinite recursion when decomposing nested
/// command/process substitutions.
///
/// Depth limit of 2 matches `format::DepthGuard` — allows `$(a $(b))`
/// but stops before unbounded nesting. The counter is separate from
/// the format module's counter since decomposition and formatting
/// are independent recursion paths.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        DECOMPOSE_DEPTH.with(|d| {
            let v = d.get();
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
        DECOMPOSE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Decomposes a word's raw value into a list of typed AST nodes.
///
/// Each segment becomes one of:
/// - `WordLiteral` for plain text
/// - `CommandSubstitution` for `$(...)`
/// - `ProcessSubstitution` for `<(...)` / `>(...)`
/// - `AnsiCQuote` for `$'...'`
/// - `LocaleString` for `$"..."`
pub(super) fn decompose_word(value: &str) -> Vec<Node> {
    let segments = parse_word_segments(value);
    segments.into_iter().map(segment_to_node).collect()
}

fn segment_to_node(seg: WordSegment) -> Node {
    match seg {
        WordSegment::Literal(text) => Node::empty(NodeKind::WordLiteral { value: text }),
        WordSegment::AnsiCQuote(content) => Node::empty(NodeKind::AnsiCQuote { content }),
        WordSegment::LocaleString(content) => Node::empty(NodeKind::LocaleString { content }),
        WordSegment::CommandSubstitution(content) => cmdsub_to_node(&content),
        WordSegment::ProcessSubstitution(direction, content) => {
            procsub_to_node(direction, &content)
        }
    }
}

fn cmdsub_to_node(content: &str) -> Node {
    parse_sub(content, &format!("$({content})"), |cmd| {
        // brace: false — this is $(...) syntax, not ${...} brace expansion
        NodeKind::CommandSubstitution {
            command: cmd,
            brace: false,
        }
    })
}

fn procsub_to_node(direction: char, content: &str) -> Node {
    parse_sub(content, &format!("{direction}({content})"), |cmd| {
        NodeKind::ProcessSubstitution {
            direction: direction.to_string(),
            command: cmd,
        }
    })
}

/// Shared logic for parsing command/process substitution content.
fn parse_sub(
    content: &str,
    fallback_value: &str,
    make_kind: impl FnOnce(Box<Node>) -> NodeKind,
) -> Node {
    let Some(_guard) = DepthGuard::enter() else {
        return literal_fallback(fallback_value);
    };
    crate::parse(content, false).map_or_else(
        |_| literal_fallback(fallback_value),
        |nodes| Node::empty(make_kind(Box::new(wrap_nodes(nodes)))),
    )
}

fn literal_fallback(value: &str) -> Node {
    Node::empty(NodeKind::WordLiteral {
        value: value.to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_word() {
        let parts = decompose_word("echo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "echo"
        ));
    }

    #[test]
    fn simple_variable_stays_literal() {
        let parts = decompose_word("$foo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "$foo"
        ));
    }

    #[test]
    fn command_substitution() {
        let parts = decompose_word("$(date)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::CommandSubstitution { brace: false, .. }
        ));
    }

    #[test]
    fn mixed_segments() {
        let parts = decompose_word("hello$(world)end");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "hello"
        ));
        assert!(matches!(
            &parts[1].kind,
            NodeKind::CommandSubstitution { .. }
        ));
        assert!(matches!(
            &parts[2].kind,
            NodeKind::WordLiteral { value } if value == "end"
        ));
    }

    #[test]
    fn ansi_c_quote() {
        let parts = decompose_word("$'foo\\nbar'");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { content } if content == "foo\\nbar"
        ));
    }

    #[test]
    fn process_substitution() {
        let parts = decompose_word("<(cmd)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ProcessSubstitution { direction, .. }
            if direction == "<"
        ));
    }

    #[test]
    fn locale_string() {
        let parts = decompose_word("$\"hello\"");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { content } if content == "\"hello\""
        ));
    }
}
