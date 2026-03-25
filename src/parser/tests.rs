use super::*;
use crate::ast::{ListOperator, PipeSep};

#[allow(clippy::unwrap_used)]
fn parse(source: &str) -> Vec<Node> {
    let lexer = Lexer::new(source, false);
    let mut parser = Parser::new(lexer);
    parser.parse_all().unwrap()
}

#[test]
fn simple_command() {
    let nodes = parse("echo hello");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(command (word "echo") (word "hello"))"#);
}

#[test]
fn pipeline() {
    let nodes = parse("ls | grep foo");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(pipe (command (word "ls")) (command (word "grep") (word "foo")))"#
    );
}

#[test]
fn and_list() {
    let nodes = parse("a && b");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(and (command (word "a")) (command (word "b")))"#);
}

#[test]
fn or_list() {
    let nodes = parse("a || b");
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(or (command (word "a")) (command (word "b")))"#);
}

#[test]
fn redirect_output() {
    let nodes = parse("echo hello > file.txt");
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(command (word "echo") (word "hello") (redirect ">" "file.txt"))"#
    );
}

#[test]
fn if_then_fi() {
    let nodes = parse("if true; then echo yes; fi");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(if "));
}

#[test]
fn while_loop() {
    let nodes = parse("while true; do echo yes; done");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(while "));
}

#[test]
fn for_loop() {
    let nodes = parse("for x in a b c; do echo $x; done");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(for "));
}

#[test]
fn subshell() {
    let nodes = parse("(echo hello)");
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(subshell "));
}

#[test]
fn brace_group() {
    let nodes = parse("{ echo hello; }");
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(brace-group "));
}

#[test]
fn negation() {
    let nodes = parse("! true");
    let output = format!("{}", nodes[0]);
    assert!(output.starts_with("(negation "));
}

#[test]
fn cstyle_for() {
    let nodes = parse("for ((i=0; i<10; i++)); do echo $i; done");
    let output = format!("{}", nodes[0]);
    let expected = r#"(arith-for (init (word "i=0")) (test (word "i<10")) (step (word "i++")) (command (word "echo") (word "$i")))"#;
    assert_eq!(output, expected);
}

#[test]
fn background() {
    let nodes = parse("echo foo &");
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(background (command (word "echo") (word "foo")))"#
    );
}

#[test]
fn conditional_expr() {
    let nodes = parse("[[ -f file ]]");
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(cond (cond-unary "-f" (cond-term "file")))"#);
}

#[test]
fn cmdsub_while_reformat() {
    let nodes = parse("echo $(while false; do echo x; done)");
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(command (word "echo") (word "$(while false; do\n    echo x;\ndone)"))"#,
    );
}

#[test]
fn cmdsub_if_else_reformat() {
    let nodes = parse("echo $(if true; then echo yes; else echo no; fi)");
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(command (word "echo") (word "$(if true; then\n    echo yes;\nelse\n    echo no;\nfi)"))"#,
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn extglob_star() {
    let lexer = Lexer::new("*(a|b)", true);
    let mut parser = Parser::new(lexer);
    let nodes = parser.parse_all().unwrap();
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(command (word "*(a|b)"))"#);
}

#[test]
#[allow(clippy::unwrap_used)]
fn extglob_star_in_case() {
    let nodes = crate::parse("# @extglob\ncase $x in *(a|b|c)) echo match;; esac", true).unwrap();
    let output = format!("{}", nodes[0]);
    assert!(
        output.contains(r#"(word "*(a|b|c)")"#),
        "expected extglob word, got: {output}"
    );
}

#[test]
fn arith_command() {
    let nodes = parse("((x = 5))");
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(arith (word "x = 5"))"#);
}

#[test]
fn comment_after_command() {
    let nodes = parse("echo hi # comment");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(command (word "echo") (word "hi"))"#);
}

#[test]
fn hash_inside_word_not_comment() {
    let nodes = parse("echo ${#var}");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert!(output.contains("${#var}"), "got: {output}");
}

#[test]
fn line_continuation() {
    let nodes = parse("echo hel\\\nlo");
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(command (word "echo") (word "hello"))"#);
}

#[test]
fn command_has_assignments_field() {
    let nodes = parse("FOO=bar cmd arg");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.len() == 1 && words.len() == 2
    ));
    // S-expression output is unchanged (assignments and words are merged)
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(command (word "FOO=bar") (word "cmd") (word "arg"))"#
    );
}

#[test]
fn multiple_assignments_before_command() {
    let nodes = parse("a=1 b=2 cmd");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.len() == 2 && words.len() == 1
    ));
}

#[test]
fn assignment_after_command_word_stays_in_words() {
    let nodes = parse("cmd FOO=bar");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.is_empty() && words.len() == 2
    ));
}

#[test]
fn plus_equals_assignment() {
    let nodes = parse("FOO+=bar cmd");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.len() == 1 && words.len() == 1
    ));
}

#[test]
fn array_assignment() {
    let nodes = parse("arr=(a b c) cmd");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.len() == 1 && words.len() == 1
    ));
}

#[test]
fn bare_assignment_no_command() {
    let nodes = parse("FOO=bar");
    assert_eq!(nodes.len(), 1);
    assert!(matches!(
        &nodes[0].kind,
        NodeKind::Command { assignments, words, .. }
        if assignments.len() == 1 && words.is_empty()
    ));
}

#[test]
fn list_items_structured() {
    let nodes = parse("a && b; c");
    assert_eq!(nodes.len(), 1);
    let NodeKind::List { items } = &nodes[0].kind else {
        unreachable!("expected List");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].operator, Some(ListOperator::Semi));
    assert_eq!(items[1].operator, None);
    let NodeKind::List { items: inner } = &items[0].command.kind else {
        unreachable!("expected inner List");
    };
    assert_eq!(inner.len(), 2);
    assert_eq!(inner[0].operator, Some(ListOperator::And));
    assert_eq!(inner[1].operator, None);
}

#[test]
fn pipeline_separators() {
    let nodes = parse("a | b");
    assert_eq!(nodes.len(), 1);
    let NodeKind::Pipeline {
        commands,
        separators,
    } = &nodes[0].kind
    else {
        unreachable!("expected Pipeline");
    };
    assert_eq!(commands.len(), 2);
    assert_eq!(separators.len(), 1);
    assert_eq!(separators[0], PipeSep::Pipe);
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

#[test]
fn list_trailing_background() {
    let nodes = parse("cmd &");
    assert_eq!(nodes.len(), 1);
    let NodeKind::List { items } = &nodes[0].kind else {
        unreachable!("expected List");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].operator, Some(ListOperator::Background));
}
