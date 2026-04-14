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
