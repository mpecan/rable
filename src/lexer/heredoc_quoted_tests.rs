//! A quoted here-document delimiter (`<<'E'`, `<<"E"`, `<<\E`) makes the
//! body literal: `\<newline>` is kept and is not a line continuation.
//! Expected bodies are bash-oracle's.

use crate::ast::NodeKind;

/// Returns the body of the single here-document in `source`.
#[allow(clippy::unwrap_used)]
fn heredoc_body(source: &str) -> String {
    let nodes = crate::parse(source, false).unwrap();
    let NodeKind::Command { redirects, .. } = &nodes[0].kind else {
        unreachable!("expected Command");
    };
    let NodeKind::HereDoc { content, .. } = &redirects[0].kind else {
        unreachable!("expected HereDoc");
    };
    content.clone()
}

#[test]
fn quoted_delimiter_keeps_backslash_newline() {
    assert_eq!(heredoc_body("cat <<'E'\na\\\nb\nE"), "a\\\nb\n");
}

#[test]
fn double_quoted_and_escaped_delimiters_are_quoted_too() {
    assert_eq!(heredoc_body("cat <<\"E\"\na\\\nb\nE"), "a\\\nb\n");
    assert_eq!(heredoc_body("cat <<\\E\na\\\nb\nE"), "a\\\nb\n");
}

#[test]
fn unquoted_delimiter_still_joins_lines() {
    assert_eq!(heredoc_body("cat <<E\na\\\nb\nE"), "ab\n");
}

#[test]
fn lone_backslash_line() {
    assert_eq!(heredoc_body("cat <<'E'\n\\\nE"), "\\\n");
}

#[test]
fn delimiter_after_backslash_line_terminates_body() {
    // `a\` does not swallow the next line, so `E` closes the body and the
    // following `E` is a separate command.
    let source = "cat <<'E'\na\\\nE\nE";
    assert_eq!(heredoc_body(source), "a\\\n");
    assert_eq!(crate::parse(source, false).map(|n| n.len()).ok(), Some(2));
}

#[test]
fn backslash_before_delimiter_text_is_not_the_delimiter() {
    assert_eq!(heredoc_body("cat <<'E'\nE\\\nE"), "E\\\n");
}

#[test]
fn trailing_backslash_at_eof() {
    assert_eq!(heredoc_body("cat <<'E'\nx\\"), "x\\\n");
    assert_eq!(heredoc_body("cat <<E\nx\\"), "x\\\\");
}

#[test]
fn strip_tabs_applies_per_physical_line() {
    assert_eq!(heredoc_body("cat <<-'E'\n\ta\\\n\tb\n\tE"), "a\\\nb\n");
}

#[test]
fn quoted_heredoc_inside_command_substitution() {
    let nodes = crate::parse("x=$(cat <<'E'\na\\\nb\nE\n)", false).ok();
    let sexp = nodes.map(|n| n[0].to_string());
    assert_eq!(
        sexp.as_deref(),
        // Word S-expressions escape `\n` and `\\`.
        Some(r#"(command (word "x=$(cat <<'E'\na\\\nb\nE\n)"))"#)
    );
}
