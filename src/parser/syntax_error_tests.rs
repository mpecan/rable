//! Invalid input must surface as an error rather than an AST with empty
//! `(command)` nodes or silently dropped text (#77).

use super::*;

fn try_parse(source: &str) -> Result<Vec<Node>> {
    Parser::new(Lexer::new(source, false)).parse_all()
}

#[allow(clippy::unwrap_used)]
fn sexp(source: &str) -> String {
    try_parse(source)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asserts `source` is rejected with a `Parse` error at `pos`.
fn assert_syntax_error(source: &str, pos: usize) {
    match try_parse(source) {
        Err(e @ RableError::Parse { .. }) => assert_eq!(e.pos(), pos, "{source:?}: {e}"),
        other => unreachable!("{source:?}: expected parse error, got {other:?}"),
    }
}

/// Asserts `source` is rejected with an unclosed-delimiter error.
fn assert_unclosed(source: &str) {
    let result = try_parse(source);
    assert!(
        matches!(result, Err(RableError::MatchedPair { .. })),
        "{source:?}: expected unclosed-delimiter error, got {result:?}"
    );
}

// -- empty commands ---------------------------------------------------------

#[test]
fn double_semi_outside_case() {
    assert_syntax_error("ls ;; rm x", 3);
}

#[test]
fn case_terminators_outside_case() {
    assert_syntax_error("ls ;& rm x", 3);
    assert_syntax_error("ls ;;& rm x", 3);
}

#[test]
fn repeated_and_operator() {
    assert_syntax_error("ls && && rm x", 6);
}

#[test]
fn repeated_or_operator() {
    assert_syntax_error("ls || || rm x", 6);
}

#[test]
fn leading_semicolon() {
    assert_syntax_error("; echo hi", 0);
}

#[test]
fn repeated_semicolon() {
    assert_syntax_error("ls ; ; rm x", 5);
}

#[test]
fn repeated_ampersand() {
    assert_syntax_error("ls & & rm x", 5);
}

#[test]
fn leading_ampersand() {
    assert_syntax_error("& rm x", 0);
}

#[test]
fn and_operator_at_end_of_input() {
    assert_syntax_error("ls &&", 5);
}

#[test]
fn pipe_without_left_command() {
    assert_syntax_error("| rm x", 0);
}

#[test]
fn pipe_without_middle_command() {
    assert_syntax_error("ls | | rm x", 5);
    assert_syntax_error("ls |& | rm x", 6);
}

#[test]
fn pipe_at_end_of_input() {
    assert_syntax_error("ls |", 4);
}

#[test]
fn empty_command_in_compound_body() {
    assert_syntax_error("if ; then x; fi", 3);
    assert_syntax_error("while true; do ; done", 15);
}

#[test]
fn error_message_names_the_unexpected_token() {
    let message = |source: &str| match try_parse(source) {
        Err(e) => e.message().to_string(),
        Ok(nodes) => unreachable!("{source:?}: expected error, got {nodes:?}"),
    };
    assert_eq!(
        message("ls && && rm x"),
        "syntax error near unexpected token '&&'"
    );
    assert_eq!(message("ls &&"), "syntax error: unexpected end of input");
}

#[test]
fn stray_close_paren() {
    assert_syntax_error("ls )", 3);
}

#[test]
fn bare_negation_and_time_are_valid() {
    // bash accepts `!` and `time` without a pipeline; the bash-oracle
    // corpus pins these as `(negation (command))` / `(time (command))`.
    assert_eq!(
        sexp("x && ! && y"),
        r#"(and (and (command (word "x")) (negation (command))) (command (word "y")))"#
    );
    assert_eq!(sexp("time"), "(time (command))");
}

#[test]
fn valid_lists_still_parse() {
    assert_eq!(
        sexp("ls &\n\nrm x"),
        "(background (command (word \"ls\")))\n(command (word \"rm\") (word \"x\"))"
    );
    assert_eq!(
        sexp("a &&\n\nb"),
        r#"(and (command (word "a")) (command (word "b")))"#
    );
    assert_eq!(
        sexp("a | b |& c"),
        r#"(pipe (command (word "a")) (pipe (command (word "b") (redirect ">&" 1)) (command (word "c"))))"#
    );
}

// -- unterminated words -------------------------------------------------------

#[test]
fn unterminated_double_quote() {
    assert_unclosed("echo \"unterminated");
}

#[test]
fn unterminated_single_quote() {
    assert_unclosed("echo 'unterminated");
}

#[test]
fn unterminated_ansi_c_quote() {
    assert_unclosed("echo $'unterminated");
}

#[test]
fn unterminated_backtick() {
    assert_unclosed("echo `unterminated");
}

#[test]
fn unterminated_word_after_command_is_not_dropped() {
    // The failing word follows a command word, which is the position
    // where the fd-redirect lookahead used to swallow the lexer error.
    assert_unclosed("ls -l \"x && rm y");
}
