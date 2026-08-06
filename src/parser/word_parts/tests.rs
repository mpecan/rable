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

/// Walks into a `CommandSubstitution` and returns the decomposed parts of
/// the last word of the command it wraps.
fn last_word_parts(sub: &Node) -> &[Node] {
    let NodeKind::CommandSubstitution { command, .. } = &sub.kind else {
        unreachable!("expected command substitution, got {:?}", sub.kind);
    };
    let NodeKind::Command { words, .. } = &command.kind else {
        unreachable!("expected command, got {:?}", command.kind);
    };
    let Some(last) = words.last() else {
        unreachable!("expected at least one word");
    };
    let NodeKind::Word { parts, .. } = &last.kind else {
        unreachable!("expected word, got {:?}", last.kind);
    };
    parts
}

#[test]
fn nested_substitution_decomposes_to_the_depth_limit() {
    let outer = decompose("$(a $(b $(c)))");
    let level2 = last_word_parts(&outer[0]);
    assert!(matches!(
        &level2[0].kind,
        NodeKind::CommandSubstitution { .. }
    ));
    // The budget is two levels; beyond that the text is kept verbatim
    // rather than decomposed further.
    let level3 = last_word_parts(&level2[0]);
    assert!(matches!(
        &level3[0].kind,
        NodeKind::WordLiteral { value } if value == "$(c)"
    ));
}

#[test]
fn depth_budget_is_released_between_words() {
    // The counter is a thread-local shared across calls. If it were not
    // released on the way out, the third substitution in a row would be
    // treated as too deep and fall back to a literal.
    for i in 0..5 {
        let parts = decompose("$(a)");
        assert!(
            matches!(&parts[0].kind, NodeKind::CommandSubstitution { .. }),
            "substitution {i} decomposed as {:?}",
            parts[0].kind
        );
    }
}

#[test]
fn quoted_runs_are_not_decomposed() {
    // Quoting is structural, not an expansion: a fully quoted word stays a
    // single literal (quotes included) rather than becoming its own part.
    for source in ["\"abc\"", "'abc'"] {
        let parts = decompose(source);
        assert_eq!(parts.len(), 1, "for input {source:?}");
        assert!(
            matches!(&parts[0].kind, NodeKind::WordLiteral { value } if value == source),
            "for input {source:?}, got {:?}",
            parts[0].kind
        );
    }
}

#[test]
fn expansions_inside_double_quotes_still_decompose() {
    // The quotes themselves stay in the surrounding literals, but the
    // expansion between them is a part of its own.
    let parts = decompose("a\"b$c\"d");
    assert_eq!(parts.len(), 3);
    assert!(matches!(
        &parts[0].kind,
        NodeKind::WordLiteral { value } if value == "a\"b"
    ));
    assert!(matches!(
        &parts[1].kind,
        NodeKind::ParamExpansion { param, .. } if param == "c"
    ));
    assert!(matches!(
        &parts[2].kind,
        NodeKind::WordLiteral { value } if value == "\"d"
    ));
}
