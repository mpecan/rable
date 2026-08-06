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
fn arith_command_with_inner_parens() {
    // Balanced inner parens still parse as arithmetic.
    let nodes = parse("(( (1+2) ))");
    let output = format!("{}", nodes[0]);
    assert_eq!(output, r#"(arith (word " (1+2) "))"#);
}

#[test]
fn double_paren_falls_back_to_nested_subshells() {
    // Bash resolves `((x\n> 0)\n)` as two nested subshells because the
    // body is not a valid arithmetic expression. See #42.
    let nodes = parse("((x\n> 0)\n)\n");
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        r#"(subshell (subshell (semi (command (word "x")) (command (redirect ">" "0")))))"#
    );
}

#[test]
fn double_paren_fallback_preserves_following_command() {
    // After a fallback-parsed `((…))`, the lexer must be positioned
    // correctly so the next command parses normally.
    let nodes = parse("((x\n> 0)\n)\necho done");
    assert_eq!(nodes.len(), 2);
    let second = format!("{}", nodes[1]);
    assert_eq!(second, r#"(command (word "echo") (word "done"))"#);
}

#[test]
fn double_paren_fallback_inside_if() {
    // The fallback must work when dispatched from inside a compound
    // construct like `if`, not just at top level.
    let nodes = parse("if ((x\n> 0)\n); then echo hi; fi");
    let output = format!("{}", nodes[0]);
    assert!(
        output.contains("(subshell (subshell"),
        "expected nested-subshell fallback inside if, got: {output}"
    );
    assert!(output.contains(r#"(command (word "echo") (word "hi"))"#));
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
fn list_trailing_background() {
    let nodes = parse("cmd &");
    assert_eq!(nodes.len(), 1);
    let NodeKind::List { items } = &nodes[0].kind else {
        unreachable!("expected List");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].operator, Some(ListOperator::Background));
}

#[test]
fn varfd_redirect_populates_ast_field() {
    // Pins the `Redirect.varfd` AST field (added for #40). The sexp
    // layer intentionally drops the varfd name to stay compatible with
    // Parable's corpus, so only an AST-level assertion catches a
    // regression where the parser forgets to populate the field.
    let nodes = parse("exec {fd}>file.txt");
    assert_eq!(nodes.len(), 1);
    let NodeKind::Command { redirects, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    assert_eq!(redirects.len(), 1);
    let NodeKind::Redirect { op, varfd, .. } = &redirects[0].kind else {
        unreachable!("expected Redirect");
    };
    assert_eq!(op, ">");
    assert_eq!(varfd.as_deref(), Some("fd"));
}

#[test]
fn plain_fd_redirect_has_no_varfd() {
    let nodes = parse("exec 3>file.txt");
    let NodeKind::Command { redirects, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    let NodeKind::Redirect { fd, varfd, .. } = &redirects[0].kind else {
        unreachable!("expected Redirect");
    };
    assert_eq!(*fd, 3);
    assert!(varfd.is_none());
}

#[test]
fn heredoc_in_cmdsub_with_trailing_parens() {
    // Issue #39 case 2: the heredoc terminator line `EOF ) )` sits on
    // the same line as the enclosing subshell + cmdsub closers. Rable
    // must parse this (the sloppy-delimiter path in the heredoc body
    // reader rewinds the trailing ` ) )` back into the input) and the
    // reformatter must emit bash's canonical form.
    let src = "$( ( cat <<EOF >>o fi utput.txt\ncontent\nEOF ) )\n";
    let nodes = parse(src);
    assert_eq!(nodes.len(), 1);
    let output = format!("{}", nodes[0]);
    assert_eq!(
        output,
        "(command (word \"$( ( cat fi utput.txt <<EOF >> o\\ncontent\\nEOF\\n ))\"))"
    );
}
