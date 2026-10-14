//! A reserved word may directly follow the keyword or token that closes a
//! compound command (`fi`, `done`, `esac`, `}`, `)`, `]]`), as in bash's
//! `reserved_word_acceptable`. Expected output is bash-oracle's.

use super::*;

fn try_sexp(source: &str) -> Result<String> {
    Ok(Parser::new(Lexer::new(source, false))
        .parse_all()?
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" "))
}

#[allow(clippy::unwrap_used)]
fn sexp(source: &str) -> String {
    try_sexp(source).unwrap()
}

/// `for x in a; do BODY done` parses with BODY as the loop body.
fn assert_loop_body(body: &str, expected: &str) {
    let source = format!("for x in a; do {body} done");
    assert_eq!(
        sexp(&source),
        format!("(for (word \"x\") (in (word \"a\")) {expected})"),
        "{source:?}"
    );
}

#[test]
fn done_after_fi() {
    assert_loop_body(
        "if t; then b; fi",
        "(if (command (word \"t\")) (command (word \"b\")))",
    );
}

#[test]
fn done_after_brace_group() {
    assert_loop_body("{ a; }", "(brace-group (command (word \"a\")))");
}

#[test]
fn done_after_subshell() {
    assert_loop_body("(a)", "(subshell (command (word \"a\")))");
}

#[test]
fn done_after_conditional() {
    assert_loop_body("[[ a ]]", "(cond (cond-unary \"-n\" (cond-term \"a\")))");
}

#[test]
fn done_after_esac() {
    assert_loop_body(
        "case y in z) c;; esac",
        "(case (word \"y\") (pattern ((word \"z\")) (command (word \"c\"))))",
    );
}

#[test]
fn done_after_done() {
    assert_eq!(
        sexp("while a; do for b in c; do d; done done"),
        "(while (command (word \"a\")) (for (word \"b\") (in (word \"c\")) (command (word \"d\"))))"
    );
}

#[test]
fn fi_after_fi() {
    assert_eq!(
        sexp("if a; then if b; then c; fi fi"),
        "(if (command (word \"a\")) (if (command (word \"b\")) (command (word \"c\"))))"
    );
}

#[test]
fn brace_close_after_fi() {
    assert_eq!(
        sexp("f() { if a; then b; fi }"),
        "(function \"f\" (brace-group (if (command (word \"a\")) (command (word \"b\")))))"
    );
}

#[test]
fn stray_closer_after_compound_is_an_error() {
    // The same rule makes a closer with nothing to close a syntax error
    // instead of a command word.
    assert!(try_sexp("if a; then b; fi fi").is_err());
    assert!(try_sexp("[[ a ]] fi").is_err());
}

#[test]
fn closers_as_arguments_stay_words() {
    assert_eq!(
        sexp("echo fi done esac }"),
        "(command (word \"echo\") (word \"fi\") (word \"done\") (word \"esac\") (word \"}\"))"
    );
    assert_eq!(
        sexp("x=$(a) done"),
        "(command (word \"x=$(a)\") (word \"done\"))"
    );
}

#[test]
fn redirect_after_compound_still_parses() {
    assert_eq!(
        sexp("if a; then b; fi >out; c"),
        "(semi (if (command (word \"a\")) (command (word \"b\"))) (redirect \">\" \"out\") \
         (command (word \"c\")))"
    );
}

#[test]
fn redirect_target_is_never_a_reserved_word() {
    // `esac` is the file name, so the following `while` is not a command
    // start either: bash reports a syntax error at `do`.
    assert!(try_sexp(">esac while true; do x; done").is_err());
    assert_eq!(
        sexp(">out if"),
        "(command (word \"if\") (redirect \">\" \"out\"))"
    );
    assert_eq!(
        sexp("x=1 >o if"),
        "(command (word \"x=1\") (word \"if\") (redirect \">\" \"o\"))"
    );
}
