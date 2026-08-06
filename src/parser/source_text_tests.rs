//! `Node::source_text` span-fidelity tests.
//!
//! Every node must map back to the exact source it came from, including
//! across multibyte and astral-plane input where byte and character
//! offsets diverge. Split from `tests.rs` to keep both files under the
//! project's file-length limit.

use super::*;

#[allow(clippy::unwrap_used)]
fn parse(source: &str) -> Vec<Node> {
    let lexer = Lexer::new(source, false);
    let mut parser = Parser::new(lexer);
    parser.parse_all().unwrap()
}

/// Flatten a (possibly nested) `List`/`Pipeline` into its leaf command nodes,
/// in source order. Test helper for span-boundary assertions.
fn collect_leaf_commands<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    match &node.kind {
        NodeKind::List { items } => {
            for item in items {
                collect_leaf_commands(&item.command, out);
            }
        }
        NodeKind::Pipeline { commands, .. } => {
            for cmd in commands {
                collect_leaf_commands(cmd, out);
            }
        }
        _ => out.push(node),
    }
}

/// Slice `source` by **character** offsets, mirroring how `Node::source_text`
/// resolves spans. Test helper for asserting inter-span gaps.
fn char_slice(source: &str, start: usize, end: usize) -> String {
    source.chars().skip(start).take(end - start).collect()
}

#[test]
fn source_text_simple_command() {
    let source = "echo hello";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "echo hello");
    let NodeKind::Command { words, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    assert_eq!(words[0].source_text(source), "echo");
    assert_eq!(words[1].source_text(source), "hello");
}

#[test]
fn source_text_pipeline() {
    let source = "ls | grep foo";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "ls | grep foo");
    let NodeKind::Pipeline { commands, .. } = &nodes[0].kind else {
        unreachable!("expected Pipeline");
    };
    assert_eq!(commands[0].source_text(source), "ls");
    assert_eq!(commands[1].source_text(source), "grep foo");
}

#[test]
fn source_text_list() {
    let source = "a && b";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "a && b");
}

#[test]
fn source_text_synthetic_node_empty() {
    let node = crate::ast::Node::empty(NodeKind::Empty);
    assert_eq!(node.source_text("anything"), "");
}

#[test]
fn source_text_span_past_source_end() {
    use crate::ast::Span;
    // Span that starts beyond source length
    let node = crate::ast::Node::new(NodeKind::Empty, Span::new(100, 200));
    assert_eq!(node.source_text("short"), "");
}

#[test]
fn source_text_multibyte_utf8() {
    // Source with multi-byte characters: "é" is 2 bytes but 1 char
    let source = "echo café";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "echo café");
    let NodeKind::Command { words, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    assert_eq!(words[0].source_text(source), "echo");
    assert_eq!(words[1].source_text(source), "café");
}

#[test]
fn source_text_multibyte_non_last_word() {
    // Regression (tokf #383): a multibyte word that is NOT the last token.
    // Before the char-count span fix, the word's span end was computed as
    // `pos + byte_len`, overshooting into the following token — here `café`
    // would bleed into ` bar`. The previous `source_text_multibyte_utf8`
    // test missed this because its multibyte word was last, so the
    // overshooting span clamped harmlessly to the source end.
    let source = "echo café bar";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "echo café bar");
    let NodeKind::Command { words, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    assert_eq!(words[0].source_text(source), "echo");
    assert_eq!(words[1].source_text(source), "café");
    assert_eq!(words[2].source_text(source), "bar");
}

#[test]
fn source_text_multibyte_compound_segments() {
    // Regression (tokf #383): multibyte in non-final segments of a compound
    // list. Each leaf command's source_text must be exact, and the spans
    // must leave the operator gaps intact for a consumer that slices
    // `source[prev.span.end .. next.span.start]` as the separator.
    let source = "echo \"✓ ok\" || echo \"✗ no\"; ls";
    let nodes = parse(source);
    let mut leaves = Vec::new();
    collect_leaf_commands(&nodes[0], &mut leaves);
    let texts: Vec<&str> = leaves.iter().map(|n| n.source_text(source)).collect();
    assert_eq!(texts, vec!["echo \"✓ ok\"", "echo \"✗ no\"", "ls"]);
    // The gap between the first two leaves must be exactly the `||` operator
    // (converting the character spans to byte offsets, as source_text does).
    let gap = char_slice(source, leaves[0].span.end, leaves[1].span.start);
    assert_eq!(gap, " || ");
}

#[test]
fn source_text_multibyte_pipeline() {
    // Regression (tokf #383): multibyte in the left command of a pipeline.
    let source = "echo \"✓\" | grep x";
    let nodes = parse(source);
    let NodeKind::Pipeline { commands, .. } = &nodes[0].kind else {
        unreachable!("expected Pipeline");
    };
    assert_eq!(commands[0].source_text(source), "echo \"✓\"");
    assert_eq!(commands[1].source_text(source), "grep x");
}

#[test]
fn source_text_astral_plane_emoji() {
    // Regression (tokf #383): a 4-byte (astral-plane) char before a newline
    // separator. Previously `echo 🎉` overshot its span and the second
    // command's slice underflowed, panicking on a non-char-boundary index.
    let source = "echo 🎉\nls";
    let nodes = parse(source);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].source_text(source), "echo 🎉");
    assert_eq!(nodes[1].source_text(source), "ls");
}

#[test]
fn source_text_if_compound() {
    let source = "if true; then echo yes; fi";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::If { .. }));
}

#[test]
fn source_text_while_compound() {
    let source = "while true; do echo x; done";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::While { .. }));
}

#[test]
fn source_text_for_loop() {
    let source = "for x in a b c; do echo $x; done";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::For { .. }));
}

#[test]
fn source_text_case_statement() {
    let source = "case $x in a) echo a;; esac";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::Case { .. }));
}

#[test]
fn source_text_function_def() {
    let source = "function foo { echo bar; }";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::Function { .. }));
}

#[test]
fn source_text_subshell() {
    let source = "(echo hello)";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::Subshell { .. }));
}

#[test]
fn source_text_brace_group() {
    let source = "{ echo hello; }";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::BraceGroup { .. }));
}

#[test]
fn source_text_conditional_expr() {
    let source = "[[ -f file ]]";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::ConditionalExpr { .. }));
}

#[test]
fn source_text_negation() {
    let source = "! true";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), source);
    assert!(matches!(nodes[0].kind, NodeKind::Negation { .. }));
}

#[test]
fn source_text_redirect() {
    let source = "echo hello > file.txt";
    let nodes = parse(source);
    let NodeKind::Command { redirects, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    assert_eq!(redirects.len(), 1);
    assert_eq!(redirects[0].source_text(source), "> file.txt");
}

#[test]
fn source_text_multiline() {
    let source = "echo a\necho b";
    let nodes = parse(source);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].source_text(source), "echo a");
    assert_eq!(nodes[1].source_text(source), "echo b");
}
