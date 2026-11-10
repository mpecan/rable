//! Invalid input must surface as an error rather than an AST with empty
//! `(command)` nodes or silently dropped text (#77).

use super::syntax_test_support::*;

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
    // bash accepts `!` and `time` without a pipeline when a list
    // terminator follows: `;`, `&`, a newline or end of input. The
    // bash-oracle corpus pins these as `(negation (command))` /
    // `(time (command))`.
    assert_eq!(sexp("time"), "(time (command))");
    assert_eq!(
        sexp("x && !"),
        r#"(and (command (word "x")) (negation (command)))"#
    );
    assert_eq!(
        sexp("! ; x"),
        r#"(semi (negation (command)) (command (word "x")))"#
    );
    assert_eq!(sexp("! &"), "(background (negation (command)))");
    assert_eq!(
        sexp("{ x && !; }"),
        r#"(brace-group (and (command (word "x")) (negation (command))))"#
    );
}

#[test]
fn bare_negation_and_time_need_a_list_terminator() {
    assert_syntax_error("! && true", 2);
    assert_syntax_error("x && ! && y", 7);
    assert_syntax_error("x || time || y", 10);
    assert_syntax_error("time -p && x", 8);
    assert_syntax_error("x && ! | y", 7);
    assert_syntax_error("{ x; ! }", 7);
    assert_syntax_error("( ! )", 4);
    assert_syntax_error("case x in a) ! ;; esac", 15);
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

// -- separators between top-level commands -----------------------------------

#[test]
fn command_after_subshell_needs_separator() {
    assert_syntax_error("(a) b", 4);
    assert_syntax_error("(a) { b; }", 4);
}

#[test]
fn subshell_after_words_needs_separator() {
    assert_syntax_error("echo a (x)", 7);
}

#[test]
fn compound_after_compound_needs_separator() {
    assert_syntax_error("case x in a) ;; esac case y in b) ;; esac", 21);
}

#[test]
fn separated_top_level_commands_still_parse() {
    assert_eq!(
        sexp("(a)\nb"),
        "(subshell (command (word \"a\")))\n(command (word \"b\"))"
    );
    assert_eq!(
        sexp("(a) >o; b"),
        r#"(semi (subshell (command (word "a"))) (redirect ">" "o") (command (word "b")))"#
    );
}

#[test]
fn fd_number_after_compound_is_not_dropped() {
    assert_syntax_error("(a)1", 3);
    assert_syntax_error("(a) 12", 4);
    assert_syntax_error("! (a)1", 5);
    assert_syntax_error("{ a; } 1", 7);
    assert_syntax_error("if a; then b; fi 1", 17);
    assert_syntax_error("(a){fd}", 3);
    assert_eq!(
        sexp("(a)1>f"),
        "(subshell (command (word \"a\"))) (redirect \">\" \"f\")"
    );
}

// -- WORD required by the grammar ------------------------------------------

#[test]
fn redirect_target_must_be_a_word() {
    assert_syntax_error("f>", 2);
    assert_syntax_error("cmd >;", 5);
    assert_syntax_error("cmd> &-", 5);
    assert_syntax_error("cat <<", 6);
    assert_eq!(
        sexp("echo > if"),
        "(command (word \"echo\") (redirect \">\" \"if\"))"
    );
}

#[test]
fn case_pattern_is_words_separated_by_pipes() {
    assert_syntax_error("case $x in a||q) :;; esac", 12);
    assert_syntax_error("case $x in do a) :;; esac", 14);
    assert_syntax_error("case $x in ) :;; esac", 11);
    assert_syntax_error("case x in a\n) :;; esac", 11);
    assert_eq!(
        sexp("case x in (a | b) :;; esac"),
        "(case (word \"x\") (pattern ((word \"a\") (word \"b\")) (command (word \":\"))))"
    );
    assert_eq!(
        sexp("case x in if|esac) :;; esac"),
        "(case (word \"x\") (pattern ((word \"if\") (word \"esac\")) (command (word \":\"))))"
    );
}

#[test]
fn for_in_list_rejects_operators() {
    assert_syntax_error("for i in 1 2|| 3; do :; done", 12);
    assert_eq!(
        sexp("for i in if then fi; do :; done"),
        "(for (word \"i\") (in (word \"if\") (word \"then\") (word \"fi\")) (command (word \":\")))"
    );
}
