use crate::ast::{Node, NodeKind};

use super::parse_arith_expression;

#[allow(clippy::expect_used)]
fn parse(source: &str) -> Node {
    parse_arith_expression(source).expect("expected successful arith parse")
}

fn parse_err(source: &str) {
    assert!(
        parse_arith_expression(source).is_err(),
        "expected error for {source:?}"
    );
}

/// Asserts the failure message, not just that a failure happened — several
/// distinct rejection paths would otherwise be interchangeable.
#[allow(clippy::expect_used)]
fn parse_err_msg(source: &str, expected: &str) {
    let Err(err) = parse_arith_expression(source) else {
        unreachable!("expected error for {source:?}");
    };
    assert_eq!(err.message(), expected, "for input {source:?}");
}

/// Extracts the operator of a binary node, panicking with context otherwise.
fn binop_of(source: &str) -> String {
    let n = parse(source);
    let NodeKind::ArithBinaryOp { op, .. } = &n.kind else {
        unreachable!("expected binop for {source:?}, got {:?}", n.kind);
    };
    op.clone()
}

/// Extracts the operator of a unary node.
fn unop_of(source: &str) -> String {
    let n = parse(source);
    let NodeKind::ArithUnaryOp { op, .. } = &n.kind else {
        unreachable!("expected unary for {source:?}, got {:?}", n.kind);
    };
    op.clone()
}

/// Extracts the operator of an assignment node.
fn assign_of(source: &str) -> String {
    let n = parse(source);
    let NodeKind::ArithAssign { op, .. } = &n.kind else {
        unreachable!("expected assign for {source:?}, got {:?}", n.kind);
    };
    op.clone()
}

#[test]
fn empty_expression() {
    assert!(matches!(parse("").kind, NodeKind::ArithEmpty));
    assert!(matches!(parse("   ").kind, NodeKind::ArithEmpty));
}

#[test]
fn decimal_number() {
    let n = parse("42");
    assert!(matches!(&n.kind, NodeKind::ArithNumber { value } if value == "42"));
}

#[test]
fn hex_number() {
    let n = parse("0xFF");
    assert!(matches!(&n.kind, NodeKind::ArithNumber { value } if value == "0xFF"));
}

#[test]
fn octal_number() {
    let n = parse("0755");
    assert!(matches!(&n.kind, NodeKind::ArithNumber { value } if value == "0755"));
}

#[test]
fn base_n_number() {
    let n = parse("64#abc");
    assert!(matches!(&n.kind, NodeKind::ArithNumber { value } if value == "64#abc"));
}

#[test]
fn simple_variable() {
    let n = parse("x");
    assert!(matches!(&n.kind, NodeKind::ArithVar { name } if name == "x"));
}

#[test]
fn dollar_prefixed_variable() {
    let n = parse("$x");
    assert!(matches!(&n.kind, NodeKind::ArithVar { name } if name == "x"));
}

#[test]
fn addition() {
    let n = parse("1 + 2");
    let NodeKind::ArithBinaryOp { op, left, right } = &n.kind else {
        unreachable!("expected binop, got {:?}", n.kind);
    };
    assert_eq!(op, "+");
    assert!(matches!(&left.kind, NodeKind::ArithNumber { value } if value == "1"));
    assert!(matches!(&right.kind, NodeKind::ArithNumber { value } if value == "2"));
}

#[test]
fn precedence_mul_over_add() {
    // 1 + 2 * 3 → 1 + (2 * 3)
    let n = parse("1 + 2 * 3");
    let NodeKind::ArithBinaryOp { op, left: _, right } = &n.kind else {
        unreachable!("expected binop");
    };
    assert_eq!(op, "+");
    assert!(matches!(
        &right.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "*"
    ));
}

#[test]
fn left_associative_subtraction() {
    // 1 - 2 - 3 → (1 - 2) - 3
    let n = parse("1 - 2 - 3");
    let NodeKind::ArithBinaryOp { op, left, right: _ } = &n.kind else {
        unreachable!("expected binop");
    };
    assert_eq!(op, "-");
    assert!(matches!(
        &left.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "-"
    ));
}

#[test]
fn right_associative_power() {
    // 2 ** 3 ** 2 → 2 ** (3 ** 2)
    let n = parse("2 ** 3 ** 2");
    let NodeKind::ArithBinaryOp { op, left: _, right } = &n.kind else {
        unreachable!("expected binop");
    };
    assert_eq!(op, "**");
    assert!(matches!(
        &right.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "**"
    ));
}

#[test]
fn parenthesized_expression() {
    // (1 + 2) * 3 → multiplication at top
    let n = parse("(1 + 2) * 3");
    let NodeKind::ArithBinaryOp { op, left, .. } = &n.kind else {
        unreachable!("expected binop");
    };
    assert_eq!(op, "*");
    assert!(matches!(
        &left.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "+"
    ));
}

#[test]
fn unary_minus() {
    let n = parse("-x");
    let NodeKind::ArithUnaryOp { op, operand } = &n.kind else {
        unreachable!("expected unary");
    };
    assert_eq!(op, "-");
    assert!(matches!(&operand.kind, NodeKind::ArithVar { .. }));
}

#[test]
fn logical_negation() {
    let n = parse("!x");
    assert!(matches!(
        &n.kind,
        NodeKind::ArithUnaryOp { op, .. } if op == "!"
    ));
}

#[test]
fn pre_increment() {
    let n = parse("++x");
    assert!(matches!(&n.kind, NodeKind::ArithPreIncr { .. }));
}

#[test]
fn post_increment() {
    let n = parse("x++");
    assert!(matches!(&n.kind, NodeKind::ArithPostIncr { .. }));
}

#[test]
fn pre_decrement() {
    let n = parse("--x");
    assert!(matches!(&n.kind, NodeKind::ArithPreDecr { .. }));
}

#[test]
fn post_decrement() {
    let n = parse("x--");
    assert!(matches!(&n.kind, NodeKind::ArithPostDecr { .. }));
}

#[test]
fn assignment_right_associative() {
    // a = b = c → a = (b = c)
    let n = parse("a = b = c");
    let NodeKind::ArithAssign { op, value, .. } = &n.kind else {
        unreachable!("expected assign");
    };
    assert_eq!(op, "=");
    assert!(matches!(&value.kind, NodeKind::ArithAssign { .. }));
}

#[test]
fn compound_assignment() {
    let n = parse("x += 5");
    let NodeKind::ArithAssign { op, .. } = &n.kind else {
        unreachable!("expected assign");
    };
    assert_eq!(op, "+=");
}

#[test]
fn ternary_operator() {
    let n = parse("a ? b : c");
    assert!(matches!(&n.kind, NodeKind::ArithTernary { .. }));
}

#[test]
fn ternary_with_empty_if_true() {
    // Bash allows `cond ?: false` (empty if_true).
    let n = parse("a ?: c");
    let NodeKind::ArithTernary {
        if_true, if_false, ..
    } = &n.kind
    else {
        unreachable!("expected ternary");
    };
    assert!(if_true.is_none());
    assert!(if_false.is_some());
}

#[test]
fn comma_operator() {
    let n = parse("a, b");
    assert!(matches!(&n.kind, NodeKind::ArithComma { .. }));
}

#[test]
fn array_subscript() {
    let n = parse("arr[i + 1]");
    let NodeKind::ArithSubscript { array, index } = &n.kind else {
        unreachable!("expected subscript, got {:?}", n.kind);
    };
    assert_eq!(array, "arr");
    assert!(matches!(
        &index.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "+"
    ));
}

#[test]
fn comparison_operators() {
    for (src, expected) in [
        ("a < b", "<"),
        ("a > b", ">"),
        ("a <= b", "<="),
        ("a >= b", ">="),
        ("a == b", "=="),
        ("a != b", "!="),
    ] {
        let n = parse(src);
        let NodeKind::ArithBinaryOp { op, .. } = &n.kind else {
            unreachable!("expected binop for {src}");
        };
        assert_eq!(op, expected, "for input {src}");
    }
}

#[test]
fn logical_and_or() {
    let n = parse("a && b || c");
    // || is lower precedence → top-level is ||
    let NodeKind::ArithBinaryOp { op, left, .. } = &n.kind else {
        unreachable!("expected binop");
    };
    assert_eq!(op, "||");
    assert!(matches!(
        &left.kind,
        NodeKind::ArithBinaryOp { op, .. } if op == "&&"
    ));
}

#[test]
fn bitwise_operators() {
    for (src, expected) in [
        ("a & b", "&"),
        ("a | b", "|"),
        ("a ^ b", "^"),
        ("a << 2", "<<"),
        ("a >> 2", ">>"),
    ] {
        let n = parse(src);
        let NodeKind::ArithBinaryOp { op, .. } = &n.kind else {
            unreachable!("expected binop for {src}");
        };
        assert_eq!(op, expected);
    }
}

#[test]
fn error_on_trailing_tokens() {
    parse_err("1 2");
}

#[test]
fn error_on_unmatched_paren() {
    parse_err("(1 + 2");
}

#[test]
fn error_on_unsupported_dollar_expansion() {
    parse_err("$(cmd)");
}

#[test]
fn error_on_extreme_paren_nesting() {
    // Guard against stack overflow on pathological input.
    let input = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    parse_err(&input);
}

// --- operator table coverage ----------------------------------------------
//
// Every operator the tokenizer and parser claim to support gets an assertion
// on the resulting node's operator string. Without these, whole arms of
// `match_one` / `match_two` / `match_three` / `assign_op` can be deleted
// without any test noticing.

#[test]
fn multiplicative_operators() {
    for (src, expected) in [("a * b", "*"), ("a / b", "/"), ("a % b", "%")] {
        assert_eq!(binop_of(src), expected, "for input {src:?}");
    }
}

#[test]
fn unary_operators() {
    for (src, expected) in [("-x", "-"), ("+x", "+"), ("!x", "!"), ("~x", "~")] {
        assert_eq!(unop_of(src), expected, "for input {src:?}");
    }
}

#[test]
fn all_compound_assignments() {
    for (src, expected) in [
        ("x = 5", "="),
        ("x += 5", "+="),
        ("x -= 5", "-="),
        ("x *= 5", "*="),
        ("x /= 5", "/="),
        ("x %= 5", "%="),
        ("x &= 5", "&="),
        ("x ^= 5", "^="),
        ("x |= 5", "|="),
    ] {
        assert_eq!(assign_of(src), expected, "for input {src:?}");
    }
}

#[test]
fn shift_assignments_without_surrounding_space() {
    // Deliberately unspaced: `match_three` consumes three bytes and the
    // resume offset must be `start + 3`. With spaces around the operator a
    // wrong offset can still land on the next token by luck.
    assert_eq!(assign_of("a<<=2"), "<<=");
    assert_eq!(assign_of("a>>=2"), ">>=");
}

// --- number tokenization --------------------------------------------------

#[test]
fn uppercase_hex_number() {
    let n = parse("0XFF");
    assert!(matches!(&n.kind, NodeKind::ArithNumber { value } if value == "0XFF"));
}

#[test]
fn digit_followed_by_letters_is_not_one_token() {
    // `1x` is a number then an identifier — two tokens, hence trailing input.
    // `0abc` must not be mistaken for a hex literal: only `0x`/`0X` starts one.
    parse_err("1x");
    parse_err("0abc");
}

#[test]
fn base_n_digits_include_underscore_and_at() {
    // bash uses `_` and `@` as base-N digits 62 and 63.
    for src in ["64#a_b", "64#a@b"] {
        let n = parse(src);
        assert!(
            matches!(&n.kind, NodeKind::ArithNumber { value } if value == src),
            "for input {src:?}, got {:?}",
            n.kind
        );
    }
}

#[test]
fn base_n_literal_stops_at_operator() {
    // A base-N literal must not swallow the following operator.
    assert_eq!(binop_of("64#abc+1"), "+");
}

#[test]
fn error_on_dollar_followed_by_digit() {
    // `$1` is a positional parameter; this lightweight parser rejects it so
    // the caller can fall back to an undecomposed expression.
    parse_err("$1");
}

// --- error identity -------------------------------------------------------

#[test]
fn error_messages_distinguish_failure_modes() {
    parse_err_msg("1 +", "unexpected end of arithmetic expression");
    parse_err_msg("1 2", "trailing tokens in arithmetic expression");
    parse_err_msg("(1 + 2", "expected ')' in arithmetic expression");
    parse_err_msg("a[1", "expected ']' in array subscript");
    parse_err_msg("a $", "trailing '$' in arithmetic expression");
    parse_err_msg("@", "unexpected character '@' in arithmetic expression");
}

#[test]
fn error_carries_position_and_line() {
    #[allow(clippy::expect_used)]
    let err = parse_arith_expression("1 2").expect_err("expected error");
    // The arithmetic subparser works on an already-extracted substring, so
    // it reports position 0 / line 1 and lets the caller attach real spans.
    assert_eq!(err.pos(), 0);
    assert_eq!(err.line(), 1);
}

// --- depth accounting -----------------------------------------------------

#[test]
fn sequential_groups_do_not_accumulate_depth() {
    // `parse_expression` is re-entered once per parenthesized group. Depth
    // must unwind on the way out, otherwise a long *flat* chain of groups
    // would trip `MAX_ARITH_DEPTH` even though nesting never exceeds two.
    let src = vec!["(1)"; super::MAX_ARITH_DEPTH + 8].join("+");
    let n = parse(&src);
    assert!(matches!(&n.kind, NodeKind::ArithBinaryOp { op, .. } if op == "+"));
}
