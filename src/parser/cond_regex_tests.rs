//! The `=~` operand inside `[[ … ]]` is lexed as a regex: `(`, `)` and `|`
//! are pattern characters, and blanks are allowed inside groups. Expected
//! output is bash-oracle's.

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

/// Asserts `[[ a =~ REGEX ]]` keeps `regex` as a single `cond-term`.
fn assert_regex_term(regex: &str) {
    let source = format!("[[ a =~ {regex} ]]");
    // `cond-term` prints its text verbatim (no quote escaping).
    let expected = format!("(cond (cond-binary \"=~\" (cond-term \"a\") (cond-term \"{regex}\")))");
    assert_eq!(sexp(&source), expected, "{source:?}");
}

#[test]
fn group() {
    assert_regex_term("^(a)$");
    assert_regex_term("(a)");
}

#[test]
fn alternation_outside_group() {
    assert_regex_term("a|b");
    assert_regex_term("x||$");
}

#[test]
fn blanks_and_metacharacters_inside_group() {
    assert_regex_term("(a b)");
    assert_regex_term("( a )");
    assert_regex_term("(< >)");
    assert_regex_term("(x||y)");
    assert_regex_term("(^|;)update($|;)");
}

#[test]
fn consecutive_groups_and_classes() {
    assert_regex_term("([a-z]+)([0-9]+)");
    assert_regex_term("[[:space:]]+");
    assert_regex_term("[[:alpha:][:digit:]]");
}

#[test]
fn group_after_quote_or_expansion() {
    assert_regex_term("\"a b\"(c)");
    assert_regex_term("$(echo x)(y)");
    assert_regex_term("$f(x)");
    assert_regex_term("${x}|z");
}

#[test]
fn double_bracket_inside_word_is_pattern() {
    assert_regex_term("x]]");
}

#[test]
fn escaped_parens_unchanged() {
    assert_regex_term(r"\(a\)");
}

#[test]
fn newline_inside_group() {
    assert_eq!(
        sexp("[[ a =~ (x\ny) ]]"),
        "(cond (cond-binary \"=~\" (cond-term \"a\") (cond-term \"(x\ny)\")))"
    );
}

#[test]
fn and_or_still_end_the_regex() {
    assert_eq!(
        sexp("[[ a =~ x&&y ]]"),
        "(cond (cond-and (cond-binary \"=~\" (cond-term \"a\") (cond-term \"x\")) \
         (cond-unary \"-n\" (cond-term \"y\"))))"
    );
    assert_eq!(
        sexp("[[ a =~ x ||y ]]"),
        "(cond (cond-or (cond-binary \"=~\" (cond-term \"a\") (cond-term \"x\")) \
         (cond-unary \"-n\" (cond-term \"y\"))))"
    );
}

#[test]
fn regex_conditional_in_list() {
    assert_eq!(
        sexp("[[ $x =~ ^(a|b)$ ]] && echo y"),
        "(and (cond (cond-binary \"=~\" (cond-term \"$x\") (cond-term \"^(a|b)$\"))) \
         (command (word \"echo\") (word \"y\")))"
    );
}

#[test]
fn missing_regex_is_an_error() {
    assert!(
        Parser::new(Lexer::new("[[ a =~ ]]", false))
            .parse_all()
            .is_err()
    );
}
