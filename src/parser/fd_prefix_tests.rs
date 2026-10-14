//! An all-digit word directly before a redirect operator is an fd prefix
//! when it fits in an `int` (`100>file`, `007<in`), including at the start
//! of a `coproc` command and after `| time`. Expected output is
//! bash-oracle's.

use super::helpers::is_fd_number;
use super::*;

#[allow(clippy::unwrap_used)]
fn sexp(source: &str) -> String {
    Parser::new(Lexer::new(source, false))
        .parse_all()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns the `fd` of the first redirect of the single top-level command.
#[allow(clippy::unwrap_used)]
fn first_redirect_fd(source: &str) -> i32 {
    let nodes = Parser::new(Lexer::new(source, false)).parse_all().unwrap();
    let NodeKind::Command { redirects, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    match &redirects[0].kind {
        NodeKind::Redirect { fd, .. } | NodeKind::HereDoc { fd, .. } => *fd,
        other => unreachable!("expected redirect, got {other:?}"),
    }
}

#[test]
fn fd_number_accepts_any_int() {
    assert!(is_fd_number("1"));
    assert!(is_fd_number("100"));
    assert!(is_fd_number("007"));
    assert!(is_fd_number("2147483647"));
}

#[test]
fn fd_number_rejects_overflow_and_non_digits() {
    assert!(!is_fd_number(""));
    assert!(!is_fd_number("2147483648"));
    assert!(!is_fd_number("99999999999"));
    assert!(!is_fd_number("-1"));
    assert!(!is_fd_number("+1"));
    assert!(!is_fd_number("1a"));
}

#[test]
fn multi_digit_fd_prefix() {
    assert_eq!(sexp("100>1"), "(command (redirect \">\" \"1\"))");
    assert_eq!(
        sexp("echo 212>3"),
        "(command (word \"echo\") (redirect \">\" \"3\"))"
    );
    assert_eq!(first_redirect_fd("exec 100<>file"), 100);
    assert_eq!(first_redirect_fd("007>x"), 7);
    assert_eq!(first_redirect_fd("cat 100<<E\nx\nE"), 100);
}

#[test]
fn largest_int_fd_prefix() {
    assert_eq!(first_redirect_fd("2147483647>x"), i32::MAX);
}

#[test]
fn overflowing_number_stays_a_word() {
    assert_eq!(
        sexp("2147483648>x"),
        "(command (word \"2147483648\") (redirect \">\" \"x\"))"
    );
}

#[test]
fn separated_number_stays_a_word() {
    assert_eq!(
        sexp("echo 100 >x"),
        "(command (word \"echo\") (word \"100\") (redirect \">\" \"x\"))"
    );
}

#[test]
fn coproc_leading_fd_prefix() {
    assert_eq!(
        sexp("coproc 2>1"),
        "(coproc \"COPROC\" (command (redirect \">\" \"1\")))"
    );
    assert_eq!(
        sexp("coproc 22>1 a"),
        "(coproc \"COPROC\" (command (word \"a\") (redirect \">\" \"1\")))"
    );
}

#[test]
fn coproc_fd_prefix_requires_adjacency() {
    assert_eq!(
        sexp("coproc a 2>x"),
        "(coproc \"COPROC\" (command (word \"a\") (redirect \">\" \"x\")))"
    );
    assert_eq!(
        sexp("coproc a 2 >x"),
        "(coproc \"COPROC\" (command (word \"a\") (word \"2\") (redirect \">\" \"x\")))"
    );
}

#[test]
fn fd_prefix_after_piped_time() {
    assert_eq!(
        sexp("x | time 2>1"),
        "(pipe (command (word \"x\")) (command (word \"time\") (redirect \">\" \"1\")))"
    );
    assert_eq!(
        sexp("x | time a 10 >1"),
        "(pipe (command (word \"x\")) (command (word \"time\") (word \"a\") (word \"10\") \
         (redirect \">\" \"1\")))"
    );
}

#[test]
fn multi_digit_fd_after_compound_command() {
    assert_eq!(
        sexp("while a; do b; done 10>x"),
        "(while (command (word \"a\")) (command (word \"b\"))) (redirect \">\" \"x\")"
    );
}
