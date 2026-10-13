use super::test_support::collect_tokens;
use super::*;

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
fn arithmetic_double_paren_closes_at_double_right_paren() {
    // `$((1+2))` must tokenize as a single Word with the full
    // `$((1+2))` text preserved. Exercises the `close_count=2`
    // path through `read_matched_parens_inner` — the close arm
    // must decrement twice before returning Break. Regression
    // guard for issue #53.
    let tokens = collect_tokens("$((1+2))");
    assert_eq!(tokens.len(), 1, "tokens = {tokens:?}");
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "$((1+2))");
}

#[test]
fn extglob_with_alternation_is_one_word() {
    // `@(foo|bar|baz)` must tokenize as a single extglob word.
    // The `|` arm inside `read_matched_parens_inner` must treat
    // alternation as a word-boundary character without breaking
    // paren depth. Regression guard for issue #53.
    let tokens = collect_tokens("@(foo|bar|baz)");
    assert_eq!(tokens.len(), 1, "tokens = {tokens:?}");
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "@(foo|bar|baz)");
}

#[test]
fn empty_extglob_closes_at_immediate_right_paren() {
    // `@()` at close_count=1 must tokenize as a single word even
    // with an empty body — `handle_paren_close` hits `depth == 0`
    // immediately and returns `Break`. Regression guard for
    // issue #53.
    let tokens = collect_tokens("@()");
    assert_eq!(tokens.len(), 1, "tokens = {tokens:?}");
    assert_eq!(tokens[0].0, TokenType::Word);
    assert_eq!(tokens[0].1, "@()");
}

#[test]
fn trailing_backslash_in_arith_is_graceful_error() {
    // `\` at EOF inside `$((…))` reaches `handle_paren_escape`'s
    // EOF fallback (the `else { wb.push('\\') }` branch), after
    // which the outer loop hits `peek_char() == None` and must
    // return a matched-pair error rather than panic. Regression
    // guard for issue #53.
    let mut lexer = Lexer::new("$(( \\", false);
    let result = lexer.next_token();
    assert!(result.is_err(), "expected error, got {result:?}");
}

#[test]
fn paren_content_comment_stops_at_newline() {
    // A `#` preceded by whitespace inside `$((…))` starts a
    // comment that runs to the next newline; the closing `))`
    // after the newline must still be recognized. Exercises
    // the comment-gating arm of `read_matched_parens_inner`.
    // Regression guard for issue #53.
    let tokens = collect_tokens("$(( # comment\n 1+2 ))");
    assert_eq!(tokens.len(), 1, "tokens = {tokens:?}");
    assert_eq!(tokens[0].0, TokenType::Word);
    // The comment text is skipped from wb.value; the word value
    // preserves everything else including the newline.
    assert!(
        tokens[0].1.starts_with("$((") && tokens[0].1.ends_with("))"),
        "unexpected word value: {:?}",
        tokens[0].1
    );
    assert!(
        !tokens[0].1.contains("comment"),
        "comment text must be skipped: {:?}",
        tokens[0].1
    );
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
