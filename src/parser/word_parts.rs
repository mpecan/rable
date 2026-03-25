//! Decomposes word values into structured AST parts.
//!
//! Reuses the segment parser from `sexp::word` to split a word's raw
//! value into typed segments, then maps each segment to an AST `Node`.

use std::cell::Cell;

use crate::ast::{ListItem, Node, NodeKind, Span};
use crate::lexer::word_builder::WordSpan;
use crate::sexp::word::{WordSegment, segments_from_spans};

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

/// Decomposes a word using lexer spans — the primary path for all
/// token-derived words.
pub(super) fn decompose_word_with_spans(value: &str, spans: &[WordSpan]) -> Vec<Node> {
    let segments = segments_from_spans(value, spans);
    segments.into_iter().map(segment_to_node).collect()
}

/// Creates a single `WordLiteral` for synthetic values (no lexer token).
/// Used for fd numbers (`"0"`, `"1"`), synthetic `"$@"`, etc.
pub(super) fn decompose_word_literal(value: &str) -> Vec<Node> {
    vec![Node::empty(NodeKind::WordLiteral {
        value: value.to_string(),
    })]
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

    /// Helper: lex a single word and decompose it using the real span path.
    #[allow(clippy::unwrap_used)]
    fn decompose(source: &str) -> Vec<Node> {
        let mut lexer = crate::lexer::Lexer::new(source, false);
        let tok = lexer.next_token().unwrap();
        decompose_word_with_spans(&tok.value, &tok.spans)
    }

    #[test]
    fn plain_word() {
        let parts = decompose("echo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "echo"
        ));
    }

    #[test]
    fn simple_variable_stays_literal() {
        let parts = decompose("$foo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "$foo"
        ));
    }

    #[test]
    fn command_substitution() {
        let parts = decompose("$(date)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::CommandSubstitution { brace: false, .. }
        ));
    }

    #[test]
    fn mixed_segments() {
        let parts = decompose("hello$(world)end");
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
        let parts = decompose("$'foo\\nbar'");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { content } if content == "foo\\nbar"
        ));
    }

    #[test]
    fn process_substitution() {
        let parts = decompose("<(cmd)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ProcessSubstitution { direction, .. }
            if direction == "<"
        ));
    }

    #[test]
    fn locale_string() {
        let parts = decompose("$\"hello\"");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { content } if content == "\"hello\""
        ));
    }
}
