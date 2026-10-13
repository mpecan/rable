//! Node span boundaries for constructs whose end (or start) is not the
//! last token the parser consumed: raw-read `(( … ))` bodies, trailing
//! `&` before a newline or here-document, and `name()` function headers.

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

// -- trailing `&` (#79) ----------------------------------------------------

#[test]
fn trailing_background_span_excludes_newline() {
    let source = "ls &\necho started";
    let nodes = parse(source);
    assert_eq!(nodes[0].span, Span::new(0, 4));
    assert_eq!(nodes[1].span, Span::new(5, 17));
}

#[test]
fn trailing_background_span_excludes_heredoc_body() {
    let source = "cat <<EOF &\nrm -rf x\nEOF\necho ok";
    let nodes = parse(source);
    assert_eq!(nodes[0].source_text(source), "cat <<EOF &");
    assert_eq!(nodes[1].source_text(source), "echo ok");
}

#[test]
fn trailing_background_span_excludes_trailing_blanks() {
    let source = "ls &  ";
    assert_eq!(top_level_texts(source), vec!["ls &"]);
}

#[test]
fn trailing_background_inside_brace_group() {
    let source = "{ ls &\n}";
    let nodes = parse(source);
    let NodeKind::BraceGroup { body, .. } = &nodes[0].kind else {
        unreachable!("expected BraceGroup");
    };
    assert_eq!(body.source_text(source), "ls &");
}

#[test]
fn background_and_semicolon_leave_newline_between_nodes() {
    // `&` belongs to the background node; a trailing `;` is a bare
    // terminator and stays outside. Either way the newline separates the
    // two top-level nodes instead of being swallowed by the first.
    for (source, first, gap) in [
        ("ls &\necho started", "ls &", "\n"),
        ("ls;\necho started", "ls", ";\n"),
    ] {
        let nodes = parse(source);
        assert_eq!(nodes.len(), 2, "{source:?}");
        assert_eq!(nodes[0].source_text(source), first, "{source:?}");
        let between: String = source
            .chars()
            .skip(nodes[0].span.end)
            .take(nodes[1].span.start - nodes[0].span.end)
            .collect();
        assert_eq!(between, gap, "{source:?}");
    }
}

#[test]
#[allow(clippy::unwrap_used)]
fn cmdsub_body_keeps_semicolon_newline_as_one_list() {
    // The `$(...)` reformatter re-parses the body with `parse_all`; there a
    // newline after `;` must not split the list, so it renders as `a; b`.
    let nodes = crate::parse("x=$(\n  a;\n  b\n)", false).unwrap();
    assert_eq!(nodes[0].to_string(), r#"(command (word "x=$(a; b)"))"#);
}
