use super::*;

#[allow(clippy::unwrap_used)]
fn collect_tokens(source: &str) -> Vec<(TokenType, String)> {
    let mut lexer = Lexer::new(source, false);
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token().unwrap();
        if tok.kind == TokenType::Eof {
            break;
        }
        tokens.push((tok.kind, tok.value));
    }
    tokens
}

#[test]
fn simple_command() {
    let tokens = collect_tokens("echo hello world");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], (TokenType::Word, "echo".to_string()));
    assert_eq!(tokens[1], (TokenType::Word, "hello".to_string()));
    assert_eq!(tokens[2], (TokenType::Word, "world".to_string()));
}

#[test]
fn pipeline() {
    let tokens = collect_tokens("ls | grep foo");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].1, "ls");
    assert_eq!(tokens[1], (TokenType::Pipe, "|".to_string()));
    assert_eq!(tokens[2].1, "grep");
    assert_eq!(tokens[3].1, "foo");
}

#[test]
fn redirections() {
    let tokens = collect_tokens("echo hello > file.txt");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[2], (TokenType::Greater, ">".to_string()));
}

#[test]
fn reserved_words() {
    // if(0) true(1) ;(2) then(3) echo(4) yes(5) ;(6) fi(7)
    let tokens = collect_tokens("if true; then echo yes; fi");
    assert_eq!(tokens[0].0, TokenType::If);
    assert_eq!(tokens[2].0, TokenType::Semi);
    assert_eq!(tokens[3].0, TokenType::Then);
    assert_eq!(tokens[6].0, TokenType::Semi);
    assert_eq!(tokens[7].0, TokenType::Fi);
}

#[test]
fn single_quoted() {
    let tokens = collect_tokens("echo 'hello world'");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[1].1, "'hello world'");
}

#[test]
fn double_quoted() {
    let tokens = collect_tokens("echo \"hello $name\"");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[1].1, "\"hello $name\"");
}

#[test]
#[allow(clippy::literal_string_with_formatting_args)]
fn dollar_expansion() {
    let tokens = collect_tokens("echo ${foo:-bar}");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[1].1, "${foo:-bar}");
}

#[test]
fn command_substitution() {
    let tokens = collect_tokens("echo $(date)");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[1].1, "$(date)");
}

#[test]
fn and_or() {
    let tokens = collect_tokens("a && b || c");
    assert_eq!(tokens[1], (TokenType::And, "&&".to_string()));
    assert_eq!(tokens[3], (TokenType::Or, "||".to_string()));
}

#[test]
fn assignment_word_simple() {
    let tokens = collect_tokens("FOO=bar");
    assert_eq!(
        tokens[0],
        (TokenType::AssignmentWord, "FOO=bar".to_string())
    );
}

#[test]
fn assignment_word_plus_equals() {
    let tokens = collect_tokens("FOO+=bar");
    assert_eq!(
        tokens[0],
        (TokenType::AssignmentWord, "FOO+=bar".to_string())
    );
}

#[test]
fn assignment_word_array() {
    let tokens = collect_tokens("arr=(a b)");
    assert_eq!(
        tokens[0],
        (TokenType::AssignmentWord, "arr=(a b)".to_string())
    );
}

#[test]
fn assignment_word_subscript() {
    let tokens = collect_tokens("arr[0]=val");
    assert_eq!(
        tokens[0],
        (TokenType::AssignmentWord, "arr[0]=val".to_string())
    );
}

#[test]
fn not_assignment_no_name() {
    let tokens = collect_tokens("=value");
    assert_eq!(tokens[0].0, TokenType::Word);
}

#[test]
fn not_assignment_regular_word() {
    let tokens = collect_tokens("echo");
    assert_eq!(tokens[0].0, TokenType::Word);
}

#[test]
fn assignment_before_command_keeps_command_start() {
    // Assignment tokens should keep command_start=true so the
    // command word after is still recognized
    let tokens = collect_tokens("FOO=bar echo hello");
    assert_eq!(tokens[0].0, TokenType::AssignmentWord);
    assert_eq!(tokens[1].0, TokenType::Word);
    assert_eq!(tokens[2].0, TokenType::Word);
}

#[test]
fn reserved_word_after_assignment_is_plain_word() {
    // Issue #37: after an AssignmentWord is consumed in a simple command,
    // subsequent words must not be classified as reserved words.
    // `bash -n -c 'foo=bar for'` is valid — `for` is a plain word there.
    let tokens = collect_tokens("foo= for x");
    assert_eq!(tokens[0].0, TokenType::AssignmentWord);
    assert_eq!(tokens[1], (TokenType::Word, "for".to_string()));
    assert_eq!(tokens[2], (TokenType::Word, "x".to_string()));

    let tokens = collect_tokens("arr[0]=$fo do o");
    assert_eq!(tokens[0].0, TokenType::AssignmentWord);
    assert_eq!(tokens[1], (TokenType::Word, "do".to_string()));

    let tokens = collect_tokens("x=$ then bar");
    assert_eq!(tokens[0].0, TokenType::AssignmentWord);
    assert_eq!(tokens[1], (TokenType::Word, "then".to_string()));
}

#[test]
fn reserved_word_re_armed_after_separator() {
    // After a command separator (`;`, `|`, newline, etc.), reserved-word
    // recognition must re-arm even if the previous simple command had
    // consumed an AssignmentWord.
    let tokens = collect_tokens("foo=bar baz; do");
    assert_eq!(tokens[0].0, TokenType::AssignmentWord);
    // tokens: AssignmentWord, Word(baz), Semi, Do
    assert_eq!(tokens[3].0, TokenType::Do);

    let tokens = collect_tokens("foo= for | do");
    // tokens: AssignmentWord, Word(for), Pipe, Do
    assert_eq!(tokens[1], (TokenType::Word, "for".to_string()));
    assert_eq!(tokens[3].0, TokenType::Do);
}

#[test]
fn list_separators_re_arm_reserved_words() {
    // Every list-separator operator must re-arm reserved-word
    // recognition so a following `for` is classified as a reserved
    // word even after `foo=` has cleared the flag. This is the
    // invariant centralised by `Lexer::command_token` (see #51).
    for src in [
        "foo= |& for",
        "foo= && for",
        "foo= || for",
        "foo= ;; for",
        "foo= ;& for",
        "foo= ;;& for",
    ] {
        let tokens = collect_tokens(src);
        assert_eq!(tokens.len(), 3, "`{src}`: unexpected token count");
        assert_eq!(
            tokens[2].0,
            TokenType::For,
            "`{src}`: list separator must re-arm reserved-word recognition"
        );
    }
}

#[test]
fn file_redirects_do_not_re_arm_reserved_words() {
    // File-redirect operators must NOT re-arm reserved-word
    // recognition — they are not command separators. After `foo=`
    // clears the flag, `for` following a redirect must stay a plain
    // `Word`. Guards against a future edit routing a redirect branch
    // through `command_token` (see #51).
    for src in [
        "foo= > f for",
        "foo= >> f for",
        "foo= &> f for",
        "foo= < f for",
        "foo= <<< f for",
    ] {
        let tokens = collect_tokens(src);
        assert_eq!(tokens.len(), 4, "`{src}`: unexpected token count");
        assert_eq!(
            tokens[3],
            (TokenType::Word, "for".to_string()),
            "`{src}`: file redirect must not re-arm reserved words"
        );
    }
}

#[test]
fn extglob_prefix_without_paren_is_ordinary_char() {
    // `is_extglob_trigger` (see read_word_token dispatch) fires only
    // when the prefix char is immediately followed by `(`. Bare
    // `@`, `?`, `+`, `!`, `*` without a following `(` must be
    // consumed as ordinary word characters. Regression guard for
    // issue #52: centralising the extglob-arm split.
    for src in ["foo@bar", "foo?bar", "foo+bar", "foo!bar", "foo*bar"] {
        let tokens = collect_tokens(src);
        assert_eq!(tokens.len(), 1, "`{src}`: unexpected token count");
        assert_eq!(
            tokens[0],
            (TokenType::Word, src.to_string()),
            "`{src}`: prefix char without `(` must stay an ordinary word char"
        );
    }
}

#[test]
fn extglob_disabled_does_not_absorb_paren() {
    // With `extglob=false`, the config-gated extglob prefixes `!(…)`
    // and `*(…)` must NOT tokenize as single extglob words. The `(`
    // must appear as a distinct `LeftParen` token instead of being
    // absorbed into the word. Guards the `config.extglob` branch of
    // `is_extglob_trigger`, which is otherwise only covered by
    // integration tests that enable extglob.
    for src in ["!(cmd)", "foo*(bar)"] {
        let tokens = collect_tokens(src);
        let has_left_paren = tokens.iter().any(|(k, _)| *k == TokenType::LeftParen);
        assert!(
            has_left_paren,
            "`{src}` with extglob=false: expected a LeftParen token, got {tokens:?}"
        );
    }
}

// -- Span recording tests --

use super::word_builder::{WordSpan, WordSpanKind};

#[allow(clippy::unwrap_used)]
fn first_word_spans(source: &str) -> (String, Vec<WordSpan>) {
    let mut lexer = Lexer::new(source, false);
    let tok = lexer.next_token().unwrap();
    (tok.value, tok.spans)
}

#[test]
fn span_plain_word_no_spans() {
    let (_, spans) = first_word_spans("echo");
    assert!(spans.is_empty());
}

#[test]
fn span_command_sub() {
    let (val, spans) = first_word_spans("$(cmd)");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_command_sub_mid_word() {
    let (_, spans) = first_word_spans("hello$(world)end");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 5);
    assert_eq!(spans[0].end, 13);
}

#[test]
fn span_arithmetic_sub() {
    let (val, spans) = first_word_spans("$((1+2))");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::ArithmeticSub);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_param_expansion() {
    let (val, spans) = first_word_spans("${var:-default}");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::ParamExpansion);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_simple_var() {
    let (val, spans) = first_word_spans("$HOME");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::SimpleVar);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_ansi_c_quote() {
    let (val, spans) = first_word_spans("$'foo'");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::AnsiCQuote);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_locale_string() {
    let (val, spans) = first_word_spans("$\"hello\"");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::LocaleString);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_single_quoted() {
    let (val, spans) = first_word_spans("'quoted'");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::SingleQuoted);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_double_quoted() {
    let (val, spans) = first_word_spans("\"double\"");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::DoubleQuoted);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_backtick() {
    let (val, spans) = first_word_spans("`cmd`");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::Backtick);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_escape() {
    let (val, spans) = first_word_spans("\\n");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::Escape);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
fn span_line_continuation_no_span() {
    // \<newline> is a line continuation — nothing pushed, no span
    let (val, spans) = first_word_spans("hel\\\nlo");
    assert_eq!(val, "hello");
    assert!(spans.is_empty());
}

#[test]
fn span_bare_dollar_no_span() {
    // Bare $ at end — no expansion, no span
    let tokens = collect_tokens("echo $");
    assert_eq!(tokens[1].1, "$");
    // The $ word has no spans (bare dollar)
    let (_, spans) = first_word_spans("$");
    assert!(spans.is_empty());
}

#[test]
fn span_nested_double_quoted_with_cmdsub() {
    // "$(cmd)" — DoubleQuoted span contains CommandSub span
    let (val, spans) = first_word_spans("\"$(cmd)\"");
    assert_eq!(val, "\"$(cmd)\"");
    assert_eq!(spans.len(), 2);
    // CommandSub is recorded first (inside read_dollar, before
    // DoubleQuoted is closed by read_word_special)
    assert_eq!(spans[0].kind, WordSpanKind::CommandSub);
    assert_eq!(spans[0].start, 1);
    assert_eq!(spans[0].end, 7);
    assert_eq!(spans[1].kind, WordSpanKind::DoubleQuoted);
    assert_eq!(spans[1].start, 0);
    assert_eq!(spans[1].end, 8);
}

#[test]
fn span_deprecated_arith() {
    let (val, spans) = first_word_spans("$[1+2]");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, WordSpanKind::DeprecatedArith);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, val.len());
}

#[test]
#[allow(clippy::expect_used)]
fn arith_command_strips_line_continuation() {
    // `((` is already consumed before read_until_double_paren runs,
    // so the input here is the arithmetic body plus the closing `))`.
    let mut lexer = Lexer::new("1 + \\\n2))", false);
    let raw = lexer
        .read_until_double_paren()
        .expect("read_until_double_paren should succeed");
    assert_eq!(raw, "1 + 2");
}

#[test]
#[allow(clippy::expect_used)]
fn arith_command_preserves_other_backslash_escapes() {
    let mut lexer = Lexer::new("a\\b))", false);
    let raw = lexer
        .read_until_double_paren()
        .expect("read_until_double_paren should succeed");
    assert_eq!(raw, "a\\b");
}

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
