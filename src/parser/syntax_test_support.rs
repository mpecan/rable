//! Helpers shared by the parser's syntax test modules.

use super::*;

pub(super) fn try_parse(source: &str) -> Result<Vec<Node>> {
    Parser::new(Lexer::new(source, false)).parse_all()
}

#[allow(clippy::unwrap_used)]
pub(super) fn sexp(source: &str) -> String {
    try_parse(source)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asserts `source` is rejected with a `Parse` error at `pos`.
pub(super) fn assert_syntax_error(source: &str, pos: usize) {
    match try_parse(source) {
        Err(e @ RableError::Parse { .. }) => assert_eq!(e.pos(), pos, "{source:?}: {e}"),
        other => unreachable!("{source:?}: expected parse error, got {other:?}"),
    }
}

/// Asserts `source` is rejected with an unclosed-delimiter error.
pub(super) fn assert_unclosed(source: &str) {
    let result = try_parse(source);
    assert!(
        matches!(result, Err(RableError::MatchedPair { .. })),
        "{source:?}: expected unclosed-delimiter error, got {result:?}"
    );
}

/// Asserts `source` parses to a single command whose words are `words`.
pub(super) fn assert_words(source: &str, words: &[&str]) {
    let escape = |w: &str| w.replace('\\', "\\\\").replace('"', "\\\"");
    let expected: Vec<String> = words
        .iter()
        .map(|w| format!("(word \"{}\")", escape(w)))
        .collect();
    let got = sexp(source);
    assert!(
        got.starts_with(&format!("(command {}", expected.join(" "))),
        "{source:?}: got {got}"
    );
}
