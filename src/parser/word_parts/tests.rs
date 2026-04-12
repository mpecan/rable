use crate::ast::{Node, NodeKind};

use super::decompose_word_with_spans;

/// Helper: lex a single word and decompose it using the real span path.
#[allow(clippy::unwrap_used)]
fn decompose(source: &str) -> Vec<Node> {
    let mut lexer = crate::lexer::Lexer::new(source, false);
    let tok = lexer.next_token().unwrap();
    decompose_word_with_spans(&tok.value, &tok.spans)
}

#[test]
fn plain_word() {
    let parts = decompose("echo");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "echo"
    ));
}

#[test]
fn simple_variable_expansion() {
    let parts = decompose("$foo");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "foo" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn braced_variable_expansion() {
    let parts = decompose("${foo}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "foo" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn param_with_default() {
    let parts = decompose("${foo:-default}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "foo"
            && op.as_deref() == Some(":-")
            && arg.as_deref() == Some("default")
    ));
}

#[test]
fn param_length() {
    let parts = decompose("${#foo}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamLength { param } if param == "foo"
    ));
}

#[test]
fn param_indirect() {
    let parts = decompose("${!foo}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamIndirect { param, op, arg }
        if param == "foo" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn special_param_question_mark() {
    let parts = decompose("$?");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "?" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn positional_param() {
    let parts = decompose("$1");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "1" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn multi_digit_positional() {
    let parts = decompose("${10}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "10" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn prefix_removal_operator() {
    let parts = decompose("${foo##pattern}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "foo"
            && op.as_deref() == Some("##")
            && arg.as_deref() == Some("pattern")
    ));
}

#[test]
fn special_param_hash_braced() {
    // ${#} is the special param "#", NOT ParamLength
    let parts = decompose("${#}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "#" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn mixed_text_and_param() {
    let parts = decompose("hello${world}end");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "hello"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::ParamExpansion { param, .. } if param == "world"
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == "end"
    ));
}

#[test]
fn array_subscript() {
    let parts = decompose("${arr[@]}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamExpansion { param, op, arg }
        if param == "arr[@]" && op.is_none() && arg.is_none()
    ));
}

#[test]
fn indirect_with_operator() {
    let parts = decompose("${!foo:-bar}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ParamIndirect { param, op, arg }
        if param == "foo"
            && op.as_deref() == Some(":-")
            && arg.as_deref() == Some("bar")
    ));
}

#[test]
fn command_substitution() {
    let parts = decompose("$(date)");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::CommandSubstitution { brace: false, .. }
    ));
}

#[test]
fn mixed_segments() {
    let parts = decompose("hello$(world)end");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "hello"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::CommandSubstitution { .. }
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == "end"
    ));
}

#[test]
fn ansi_c_quote() {
    let parts = decompose("$'foo\\nbar'");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { content, .. } if content == "foo\\nbar"
    ));
}

#[test]
fn process_substitution() {
    let parts = decompose("<(cmd)");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ProcessSubstitution { direction, .. }
        if direction == "<"
    ));
}

#[test]
fn locale_string() {
    let parts = decompose("$\"hello\"");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::LocaleString { content, .. } if content == "\"hello\""
    ));
}

#[test]
fn brace_expansion_comma() {
    let parts = decompose("{a,b,c}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::BraceExpansion { content } if content == "{a,b,c}"
    ));
}

#[test]
fn brace_expansion_range() {
    let parts = decompose("{1..10}");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::BraceExpansion { content } if content == "{1..10}"
    ));
}

#[test]
fn brace_expansion_mid_word() {
    let parts = decompose("file{1,2}.txt");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "file"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::BraceExpansion { content } if content == "{1,2}"
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == ".txt"
    ));
}

#[test]
fn arithmetic_expansion_decomposed() {
    let parts = decompose("$((1+2))");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ArithmeticExpansion {
            expression: Some(_)
        }
    ));
}

#[test]
fn arithmetic_with_variable() {
    let parts = decompose("$((x*2))");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::ArithmeticExpansion { .. }
    ));
}

#[test]
fn arithmetic_in_mixed_word() {
    let parts = decompose("file_$((n+1)).txt");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "file_"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::ArithmeticExpansion {
            expression: Some(_)
        }
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == ".txt"
    ));
}

#[test]
fn arithmetic_expression_tree_shape() {
    // `1+2` should parse as `(+ 1 2)` — verifies we're actually
    // producing a typed sub-AST, not an opaque string blob.
    let parts = decompose("$((1+2))");
    let NodeKind::ArithmeticExpansion {
        expression: Some(expr),
    } = &parts[0].kind
    else {
        unreachable!("expected parsed arithmetic expression");
    };
    let NodeKind::ArithBinaryOp { op, left, right } = &expr.kind else {
        unreachable!("expected binop, got {:?}", expr.kind);
    };
    assert_eq!(op, "+");
    assert!(matches!(&left.kind, NodeKind::ArithNumber { value } if value == "1"));
    assert!(matches!(&right.kind, NodeKind::ArithNumber { value } if value == "2"));
}

#[test]
fn ansi_c_decodes_hex() {
    let parts = decompose("$'\\x41'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
    ));
}

#[test]
fn ansi_c_decodes_newline() {
    let parts = decompose("$'line1\\nline2'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "line1\nline2"
    ));
}

#[test]
fn ansi_c_decodes_octal() {
    // octal 101 = 65 = 'A'
    let parts = decompose("$'\\101'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
    ));
}

#[test]
fn ansi_c_decodes_unicode() {
    let parts = decompose("$'\\u0041'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
    ));
}

#[test]
fn ansi_c_decodes_control_char() {
    // Ctrl-A = 0x01
    let parts = decompose("$'\\cA'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "\u{01}"
    ));
}

#[test]
fn ansi_c_unknown_escape_passthrough() {
    let parts = decompose("$'\\z'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { decoded, .. } if decoded == "\\z"
    ));
}

#[test]
fn ansi_c_preserves_raw_content() {
    // Legacy `content` field must remain byte-identical so the
    // Parable corpus keeps passing.
    let parts = decompose("$'foo\\nbar'");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::AnsiCQuote { content, .. } if content == "foo\\nbar"
    ));
}

#[test]
fn locale_string_strips_quotes() {
    let parts = decompose("$\"hello\"");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::LocaleString { inner, .. } if inner == "hello"
    ));
}

#[test]
fn locale_string_empty() {
    let parts = decompose("$\"\"");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::LocaleString { inner, .. } if inner.is_empty()
    ));
}

#[test]
fn locale_string_preserves_raw_content() {
    // Legacy `content` field keeps the surrounding quotes so the
    // Parable corpus keeps passing.
    let parts = decompose("$\"hello\"");
    assert!(matches!(
        &parts[0].kind,
        NodeKind::LocaleString { content, .. } if content == "\"hello\""
    ));
}

#[test]
fn backtick_command_substitution() {
    let parts = decompose("`date`");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::CommandSubstitution { brace: false, .. }
    ));
}

#[test]
fn backtick_with_args() {
    let parts = decompose("`ls -la`");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::CommandSubstitution { .. }
    ));
}

#[test]
fn backtick_in_mixed_word() {
    let parts = decompose("prefix`pwd`suffix");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "prefix"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::CommandSubstitution { .. }
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == "suffix"
    ));
}

#[test]
fn backtick_empty() {
    let parts = decompose("``");
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::CommandSubstitution { .. }
    ));
}

#[test]
fn backtick_and_dollar_paren_both_decompose() {
    let parts = decompose("`date`$(pwd)");
    assert_eq!(parts.len(), 2);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::CommandSubstitution { .. }
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::CommandSubstitution { .. }
    ));
}
