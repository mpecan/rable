use super::*;

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
