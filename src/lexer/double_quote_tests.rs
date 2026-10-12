//! Expansions inside double quotes: `$'` is literal there (#75).

use crate::ast::{Node, NodeKind};

#[allow(clippy::unwrap_used)]
fn parse_sexp(source: &str) -> String {
    crate::parse(source, false)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the `parts` of word `index` of the single top-level command.
#[allow(clippy::unwrap_used)]
fn word_parts(source: &str, index: usize) -> Vec<Node> {
    let nodes = crate::parse(source, false).unwrap();
    let NodeKind::Command { words, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    let NodeKind::Word { parts, .. } = &words[index].kind else {
        unreachable!("expected Word");
    };
    parts.clone()
}

// -- `$'` inside double quotes (#75) -----------------------------------------

#[test]
fn dollar_single_quote_in_double_quotes_is_literal() {
    assert_eq!(
        parse_sexp(r#"mysql -e "SELECT 'a$'" && rm x"#),
        r#"(and (command (word "mysql") (word "-e") (word "\"SELECT 'a$'\"")) (command (word "rm") (word "x")))"#
    );
}

#[test]
fn dollar_single_quote_pair_in_double_quotes_is_literal() {
    assert_eq!(
        parse_sexp(r#"echo "$'x'" y"#),
        r#"(command (word "echo") (word "\"$'x'\"") (word "y"))"#
    );
}

#[test]
fn dollar_single_quote_in_double_quotes_is_a_literal_part() {
    let parts = word_parts(r#"echo "a$'b""#, 1);
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == r#""a$'b""#
    ));
}

#[test]
fn ansi_c_quote_outside_double_quotes_still_decoded() {
    let parts = word_parts(r"echo $'a\tb'", 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "a\tb"
    ));
}

#[test]
#[allow(clippy::literal_string_with_formatting_args)]
fn ansi_c_quote_in_param_default_inside_double_quotes() {
    // Inside `${…}` the `$'…'` is still special, even within "…".
    assert_eq!(
        parse_sexp(r#"echo "${x:-$'a'}""#),
        r#"(command (word "echo") (word "\"${x:-a}\""))"#
    );
}
