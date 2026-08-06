//! Issue #35: `]]` is only the `DoubleRightBracket` reserved word inside a
//! `[[ … ]]` conditional. Anywhere else it is an ordinary word, so the
//! parser does not mistake it for a list terminator.
//!
//! Split from `tests.rs` to keep both files under the project's
//! file-length limit.

use super::test_support::collect_tokens;
use super::*;

// --- #35: `]]` outside `[[ ]]` is an ordinary word ---------------------

#[test]
fn rbracket_outside_cond_is_plain_word() {
    // `]]` at command-start must NOT be classified as DoubleRightBracket.
    let tokens = collect_tokens("]] foo");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0], (TokenType::Word, "]]".to_string()));
    assert_eq!(tokens[1], (TokenType::Word, "foo".to_string()));
}

#[test]
fn rbracket_in_middle_of_command_is_plain_word() {
    // Regression for rbracket_outside_cond 3: `Cdeclare -n ref=t ]] arget`.
    // `]]` as a mid-command argument must stay a Word and not terminate
    // the command list.
    let tokens = collect_tokens("Cdeclare -n ref=t ]] arget");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["Cdeclare", "-n", "ref=t", "]]", "arget"]);
    // Ensure `]]` didn't get reclassified as a reserved word here.
    let rbracket = tokens.iter().find(|(_, v)| v == "]]").map(|(k, _)| *k);
    assert_eq!(rbracket, Some(TokenType::Word));
}

#[test]
fn rbracket_inside_cond_is_reserved() {
    // Sanity check: inside `[[ ]]`, `]]` must still be the reserved
    // DoubleRightBracket token. `cond_expr` is set by the parser when
    // it consumes the opening `[[`, so we set it manually here to
    // isolate the lexer-level classification.
    let mut lexer = Lexer::new("]]", false);
    lexer.enter_cond_expr();
    #[allow(clippy::unwrap_used)]
    let tok = lexer.next_token().unwrap();
    assert_eq!(tok.kind, TokenType::DoubleRightBracket);
    assert_eq!(tok.value, "]]");
}

#[test]
fn leading_rbracket_bracket_bracket_stays_own_word() {
    // rbracket_outside_cond 1: `][[ "$file" == *.txt ]]`
    // The leading `][[` must split from the following word even though
    // it starts with `]` and contains `[`.
    let tokens = collect_tokens("][[ \"$file\" == *.txt ]]");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["][[", "\"$file\"", "==", "*.txt", "]]"],);
}

#[test]
fn leading_bracket_letter_bracket_stays_own_word() {
    // rbracket_outside_cond 2: `[c[ $x =~ ]+[a-z] ]]`
    // `[c[` starts with `[` (not an identifier char), so the inner `[`
    // must not enter subscript absorption. The space after `[c[` splits
    // the word, and the final `]]` stands alone.
    let tokens = collect_tokens("[c[ $x =~ ]+[a-z] ]]");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["[c[", "$x", "=~", "]+[a-z]", "]]"]);
}

// --- #36 side-effect: `[...]` on a non-identifier prefix splits --------

#[test]
fn pipe_inside_brackets_on_non_identifier_splits_command() {
    // bracket_op_split 1: `echo ho $$[a||b]` must tokenize as
    // `echo`, `ho`, `$$[a`, `||`, `b]` (the `||` inside `[...]` is a
    // control operator because `$$` is not an identifier prefix).
    let tokens = collect_tokens("echo ho $$[a||b]");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["echo", "ho", "$$[a", "||", "b]"]);
}

#[test]
fn amp_inside_brackets_on_reserved_word_prefix_splits() {
    // bracket_op_split 2: `case[a&&b]` in non-first-word position —
    // `case` is a reserved word but here it's just an argument, and
    // `&&` inside `[...]` must still split because the bracket word
    // is not an assignment.
    let tokens = collect_tokens("Decho $ case[a&&b]");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["Decho", "$", "case[a", "&&", "b]"]);
}

#[test]
fn caret_prefix_brackets_split_on_space() {
    // bracket_op_split 3: `foo^[a-echo ${foo^[a-z]}` — `foo^` is not a
    // bare identifier (contains `^`), so the `[` must not enter
    // subscript absorption; the space splits the word.
    let tokens = collect_tokens("foo^[a-echo ${foo^[a-z]}");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(values, vec!["foo^[a-echo", "${foo^[a-z]}"]);
}

// --- #37 case 5 side-effect: `[ $"yes" = yes9 ][ $; then ...` ----------

#[test]
fn bare_bracket_test_then_bracket_splits_on_semi() {
    // reserved_word_as_word 5: `if [ $"yes" = yes9 ][ $; then echo ok; fi`
    // The `][` in the middle is a plain word; the `;` after `$` must
    // terminate the command (no bracket absorption on `[ `).
    let tokens = collect_tokens("if [ $\"yes\" = yes9 ][ $; then echo ok; fi");
    let values: Vec<&str> = tokens.iter().map(|(_, v)| v.as_str()).collect();
    assert_eq!(
        values,
        vec![
            "if", "[", "$\"yes\"", "=", "yes9", "][", "$", ";", "then", "echo", "ok", ";", "fi",
        ],
    );
}

// --- Regression guard: identifier-prefixed brackets still absorb -------

#[test]
fn arr_subscript_absorbs_space() {
    // `arr[0 foo]` must stay a single word — the subscript absorbs the
    // space because `arr` is a bare identifier at command-start. This
    // guards the existing `arr[...]` invocation path from the #35 fix.
    let tokens = collect_tokens("arr[0 foo]");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].1, "arr[0 foo]");
}

#[test]
fn regex_char_class_inside_cond_stays_one_word() {
    // Inside `[[ ]]`, a regex RHS containing a character class with
    // `||` must stay a single word (not be split as a control operator).
    // We simulate the cond-expr context the parser sets when it
    // consumes `[[`, so we can exercise the lexer branch in isolation.
    let mut lexer = Lexer::new("$x =~ [[:alpha:][:dig||it:]] ]]", false);
    lexer.enter_cond_expr();
    let mut values = Vec::new();
    loop {
        #[allow(clippy::unwrap_used)]
        let tok = lexer.next_token().unwrap();
        if tok.kind == TokenType::Eof {
            break;
        }
        values.push(tok.value);
    }
    assert_eq!(values, vec!["$x", "=~", "[[:alpha:][:dig||it:]]", "]]"],);
}
