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
