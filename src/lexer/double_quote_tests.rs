//! Expansions inside double quotes: `$'` is literal there (#75), and a
//! backtick substitution is exposed as a `CommandSubstitution` part just
//! like `$(…)` (#76).

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

/// Returns the S-expression of each `CommandSubstitution` part's command.
fn cmdsub_commands(parts: &[Node]) -> Vec<String> {
    parts
        .iter()
        .filter_map(|p| match &p.kind {
            NodeKind::CommandSubstitution { command, .. } => Some(command.to_string()),
            _ => None,
        })
        .collect()
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

// -- backticks inside double quotes (#76) ------------------------------------

#[test]
fn backtick_in_double_quotes_is_command_substitution() {
    let parts = word_parts(r#"echo "`rm -rf x`""#, 1);
    assert_eq!(parts.len(), 3);
    assert!(matches!(&parts[0].kind, NodeKind::WordLiteral { value } if value == "\""));
    assert_eq!(
        cmdsub_commands(&parts),
        vec![r#"(command (word "rm") (word "-rf") (word "x"))"#]
    );
    assert!(matches!(&parts[2].kind, NodeKind::WordLiteral { value } if value == "\""));
}

#[test]
fn backtick_and_dollar_paren_in_double_quotes_match() {
    let backtick = word_parts(r#"echo "a `ls` b""#, 1);
    let dollar = word_parts(r#"echo "a $(ls) b""#, 1);
    assert_eq!(cmdsub_commands(&backtick), cmdsub_commands(&dollar));
    assert_eq!(backtick.len(), dollar.len());
}

#[test]
fn multiple_backticks_in_double_quotes() {
    let parts = word_parts(r#"echo "`a`-`b`""#, 1);
    assert_eq!(
        cmdsub_commands(&parts),
        vec![r#"(command (word "a"))"#, r#"(command (word "b"))"#]
    );
}

#[test]
fn backtick_in_double_quotes_sexp_unchanged() {
    // The S-expression keeps the backtick text verbatim (Parable parity).
    assert_eq!(
        parse_sexp(r#"echo "`rm -rf x`""#),
        r#"(command (word "echo") (word "\"`rm -rf x`\""))"#
    );
}
