//! A `coproc` whose body is not a compound command builds a synthetic
//! simple command; every remaining word belongs to it, including
//! assignment-shaped arguments and words after a leading redirect.
//! Expected output is bash-oracle's.

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

#[test]
fn assignment_shaped_arguments() {
    assert_eq!(
        sexp("coproc cat * input=t.txt"),
        "(coproc \"COPROC\" (command (word \"cat\") (word \"*\") (word \"input=t.txt\")))"
    );
    assert_eq!(
        sexp("coproc echo -n pwd=; pwd"),
        "(semi (coproc \"COPROC\" (command (word \"echo\") (word \"-n\") (word \"pwd=\"))) \
         (command (word \"pwd\")))"
    );
}

#[test]
fn words_after_leading_redirect() {
    assert_eq!(
        sexp("coproc>f e"),
        "(coproc \"COPROC\" (command (word \"e\") (redirect \">\" \"f\")))"
    );
    assert_eq!(
        sexp("coproc >&MYFD cat file"),
        "(coproc \"COPROC\" (command (word \"cat\") (word \"file\") (redirect \">&\" \"MYFD\")))"
    );
}

#[test]
fn leading_redirect_target_and_next_word_are_not_reserved() {
    assert_eq!(
        sexp("coproc >esac if"),
        "(coproc \"COPROC\" (command (word \"if\") (redirect \">\" \"esac\")))"
    );
}

#[test]
fn redirect_only() {
    assert_eq!(
        sexp("coproc >o"),
        "(coproc \"COPROC\" (command (redirect \">\" \"o\")))"
    );
}

#[test]
fn named_and_compound_forms_unchanged() {
    assert_eq!(
        sexp("coproc NAME { cat; }"),
        "(coproc \"NAME\" (brace-group (command (word \"cat\"))))"
    );
    assert_eq!(
        sexp("coproc cat"),
        "(coproc \"COPROC\" (command (word \"cat\")))"
    );
}
