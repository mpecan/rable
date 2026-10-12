//! Node span boundaries for constructs whose end (or start) is not the
//! last token the parser consumed: raw-read `(( … ))` bodies and `name()`
//! function headers.

use super::*;

#[allow(clippy::unwrap_used)]
fn parse(source: &str) -> Vec<Node> {
    let lexer = Lexer::new(source, false);
    let mut parser = Parser::new(lexer);
    parser.parse_all().unwrap()
}

/// Returns the source text of every top-level node.
fn top_level_texts(source: &str) -> Vec<&str> {
    parse(source)
        .iter()
        .map(|n| n.source_text(source))
        .collect()
}

// -- (( … )) arithmetic command (#78) -------------------------------------

#[test]
fn arith_command_span_covers_closing_parens() {
    let source = "(( arr[$(rm -rf x)] ))";
    let nodes = parse(source);
    assert!(matches!(nodes[0].kind, NodeKind::ArithmeticCommand { .. }));
    assert_eq!(nodes[0].span, Span::new(0, 22));
}

#[test]
fn arith_command_span_in_list() {
    let source = "((x++)) && echo ok";
    let nodes = parse(source);
    let NodeKind::List { items } = &nodes[0].kind else {
        unreachable!("expected List");
    };
    assert_eq!(items[0].command.source_text(source), "((x++))");
    assert_eq!(items[1].command.source_text(source), "echo ok");
}

#[test]
fn arith_command_span_includes_trailing_redirect() {
    let source = "(( x = 1 )) > out";
    assert_eq!(top_level_texts(source), vec![source]);
}

#[test]
fn arith_command_populates_expression() {
    let nodes = parse("(( x + 1 ))");
    let NodeKind::ArithmeticCommand { expression, .. } = &nodes[0].kind else {
        unreachable!("expected ArithmeticCommand");
    };
    let Some(expr) = expression else {
        unreachable!("expected a parsed expression");
    };
    assert!(matches!(&expr.kind, NodeKind::ArithBinaryOp { op, .. } if op == "+"));
}

#[test]
fn arith_command_unparseable_expression_is_none() {
    // The arithmetic sub-parser does not understand `$(…)`; the raw text
    // stays available through `raw_content`.
    let nodes = parse("(( arr[$(rm -rf x)] ))");
    let NodeKind::ArithmeticCommand {
        expression,
        raw_content,
        ..
    } = &nodes[0].kind
    else {
        unreachable!("expected ArithmeticCommand");
    };
    assert!(expression.is_none());
    assert_eq!(raw_content, " arr[$(rm -rf x)] ");
}

#[test]
fn cstyle_for_span_starts_at_for() {
    let source = "for ((i=0; i<3; i++)); do echo $i; done";
    assert_eq!(top_level_texts(source), vec![source]);
}

// -- function definitions (#78) --------------------------------------------

#[test]
fn posix_function_span_starts_at_name() {
    let source = "f() { echo hi; }";
    assert_eq!(top_level_texts(source), vec![source]);
}

#[test]
fn posix_function_span_with_leading_blanks() {
    let source = "  greet () ( echo hi )";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "greet () ( echo hi )");
}
