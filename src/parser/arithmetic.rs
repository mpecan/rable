//! Standalone recursive-descent parser for bash arithmetic expressions.
//!
//! Parses the body of `$((...))` or `((...))` into a typed AST using the
//! existing `NodeKind::Arith*` variants. The public entry point,
//! [`parse_arith_expression`], takes the inner expression text (without the
//! surrounding delimiters) and returns a single [`Node`] representing the
//! parsed tree.
//!
//! Precedence follows the bash manual (lowest → highest): comma, assignment,
//! ternary, logical OR/AND, bitwise OR/XOR/AND, equality, comparison, shift,
//! additive, multiplicative, exponentiation, unary, pre/post increment and
//! decrement, primary.

use crate::ast::{Node, NodeKind};
use crate::error::{RableError, Result};

/// Maximum recursion depth for `parse_expression`, which is re-entered for
/// parenthesized groups, array subscripts, and ternary `if_true` branches.
/// Bounds the stack on pathological inputs like `((((((x))))))`.
///
/// Each recursive `parse_expression` call cascades through ~15 precedence
/// levels before reaching `parse_primary`, so keep the limit low enough
/// that even debug builds on small test-thread stacks stay safe. Real
/// bash arithmetic expressions never nest deeper than a handful of levels.
const MAX_ARITH_DEPTH: usize = 32;

/// Parses a bash arithmetic expression from its inner text.
///
/// # Errors
///
/// Returns [`RableError::Parse`] on malformed input. Callers that want
/// best-effort behavior (e.g. `Word.parts` decomposition) can ignore the
/// error and store `None` in the resulting `ArithmeticExpansion` node.
pub(super) fn parse_arith_expression(source: &str) -> Result<Node> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Ok(mk(NodeKind::ArithEmpty));
    }
    let tokens = tokenize(trimmed)?;
    let mut parser = ArithParser::new(tokens);
    let expr = parser.parse_expression()?;
    if !parser.at_end() {
        return Err(err("trailing tokens in arithmetic expression"));
    }
    Ok(expr)
}

// ------------------------------------------------------------------------
// Tokenizer
// ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Number(String),
    Ident(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Question,
    Colon,
    Comma,
    Bang,
    Tilde,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Power,
    Shl,
    Shr,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    Ne,
    Amp,
    Caret,
    Pipe,
    AmpAmp,
    PipePipe,
    Inc,
    Dec,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ShlAssign,
    ShrAssign,
    AndAssign,
    XorAssign,
    OrAssign,
}

fn tokenize(source: &str) -> Result<Vec<Tok>> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let (tok, next) = if b.is_ascii_digit() {
            tokenize_number(source, i)?
        } else if b.is_ascii_alphabetic() || b == b'_' {
            tokenize_ident(source, i)?
        } else if b == b'$' {
            tokenize_dollar(source, i)?
        } else {
            tokenize_operator(source, i)?
        };
        tokens.push(tok);
        i = next;
    }
    Ok(tokens)
}

fn tokenize_number(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    // Hex: 0x... / 0X...
    if bytes[start] == b'0'
        && start + 1 < bytes.len()
        && (bytes[start + 1] == b'x' || bytes[start + 1] == b'X')
    {
        let mut end = start + 2;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        return slice_number(source, start, end);
    }
    // Decimal (possibly followed by base-N marker `#`)
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'#' {
        end += 1;
        while end < bytes.len() && is_base_digit(bytes[end]) {
            end += 1;
        }
    }
    slice_number(source, start, end)
}

const fn is_base_digit(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'@'
}

fn slice_number(source: &str, start: usize, end: usize) -> Result<(Tok, usize)> {
    let value = source
        .get(start..end)
        .ok_or_else(|| err("invalid number literal"))?;
    Ok((Tok::Number(value.to_string()), end))
}

fn tokenize_ident(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let name = source
        .get(start..end)
        .ok_or_else(|| err("invalid identifier"))?;
    Ok((Tok::Ident(name.to_string()), end))
}

/// `$name` inside an arithmetic expression is equivalent to `name`.
/// Other `$`-expansions (`$(...)`, `${...}`, `$((...))`) are not supported
/// by this lightweight parser — the caller will fall back to `None`.
fn tokenize_dollar(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    let after = start + 1;
    if after >= bytes.len() {
        return Err(err("trailing '$' in arithmetic expression"));
    }
    if bytes[after].is_ascii_alphabetic() || bytes[after] == b'_' {
        return tokenize_ident(source, after);
    }
    Err(err("unsupported $-expansion in arithmetic expression"))
}

fn tokenize_operator(source: &str, start: usize) -> Result<(Tok, usize)> {
    let rest = source
        .get(start..)
        .ok_or_else(|| err("unexpected end of input"))?;
    let bytes = rest.as_bytes();
    if bytes.len() >= 3
        && let Some(t) = match_three(&bytes[..3])
    {
        return Ok((t, start + 3));
    }
    if bytes.len() >= 2
        && let Some(t) = match_two(&bytes[..2])
    {
        return Ok((t, start + 2));
    }
    if let Some(t) = match_one(bytes[0]) {
        return Ok((t, start + 1));
    }
    Err(err(format!(
        "unexpected character '{}' in arithmetic expression",
        bytes[0] as char
    )))
}

fn match_three(pair: &[u8]) -> Option<Tok> {
    Some(match pair {
        b"<<=" => Tok::ShlAssign,
        b">>=" => Tok::ShrAssign,
        _ => return None,
    })
}

fn match_two(pair: &[u8]) -> Option<Tok> {
    Some(match pair {
        b"**" => Tok::Power,
        b"<<" => Tok::Shl,
        b">>" => Tok::Shr,
        b"<=" => Tok::Le,
        b">=" => Tok::Ge,
        b"==" => Tok::EqEq,
        b"!=" => Tok::Ne,
        b"&&" => Tok::AmpAmp,
        b"||" => Tok::PipePipe,
        b"++" => Tok::Inc,
        b"--" => Tok::Dec,
        b"+=" => Tok::AddAssign,
        b"-=" => Tok::SubAssign,
        b"*=" => Tok::MulAssign,
        b"/=" => Tok::DivAssign,
        b"%=" => Tok::ModAssign,
        b"&=" => Tok::AndAssign,
        b"^=" => Tok::XorAssign,
        b"|=" => Tok::OrAssign,
        _ => return None,
    })
}

const fn match_one(c: u8) -> Option<Tok> {
    Some(match c {
        b'+' => Tok::Plus,
        b'-' => Tok::Minus,
        b'*' => Tok::Star,
        b'/' => Tok::Slash,
        b'%' => Tok::Percent,
        b'(' => Tok::LParen,
        b')' => Tok::RParen,
        b'[' => Tok::LBracket,
        b']' => Tok::RBracket,
        b'?' => Tok::Question,
        b':' => Tok::Colon,
        b',' => Tok::Comma,
        b'!' => Tok::Bang,
        b'~' => Tok::Tilde,
        b'<' => Tok::Lt,
        b'>' => Tok::Gt,
        b'&' => Tok::Amp,
        b'^' => Tok::Caret,
        b'|' => Tok::Pipe,
        b'=' => Tok::Assign,
        _ => return None,
    })
}

// ------------------------------------------------------------------------
// Parser
// ------------------------------------------------------------------------

struct ArithParser {
    tokens: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl ArithParser {
    const fn new(tokens: Vec<Tok>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    const fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    const fn bump(&mut self) {
        self.pos += 1;
    }

    fn parse_expression(&mut self) -> Result<Node> {
        if self.at_end() {
            return Ok(mk(NodeKind::ArithEmpty));
        }
        if self.depth >= MAX_ARITH_DEPTH {
            return Err(err("arithmetic expression nested too deeply"));
        }
        self.depth += 1;
        let result = self.parse_comma();
        self.depth -= 1;
        result
    }

    fn parse_comma(&mut self) -> Result<Node> {
        let mut left = self.parse_assign()?;
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.bump();
            let right = self.parse_assign()?;
            left = mk(NodeKind::ArithComma {
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_assign(&mut self) -> Result<Node> {
        let left = self.parse_ternary()?;
        let Some(op) = self.peek().and_then(assign_op) else {
            return Ok(left);
        };
        self.bump();
        let right = self.parse_assign()?;
        Ok(mk(NodeKind::ArithAssign {
            op: op.to_string(),
            target: Box::new(left),
            value: Box::new(right),
        }))
    }

    fn parse_ternary(&mut self) -> Result<Node> {
        let cond = self.parse_logical_or()?;
        if !matches!(self.peek(), Some(Tok::Question)) {
            return Ok(cond);
        }
        self.bump();
        let if_true = if matches!(self.peek(), Some(Tok::Colon)) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };
        if !matches!(self.peek(), Some(Tok::Colon)) {
            return Err(err("expected ':' in ternary arithmetic expression"));
        }
        self.bump();
        let if_false = Some(Box::new(self.parse_assign()?));
        Ok(mk(NodeKind::ArithTernary {
            condition: Box::new(cond),
            if_true,
            if_false,
        }))
    }

    fn parse_logical_or(&mut self) -> Result<Node> {
        let mut left = self.parse_logical_and()?;
        while matches!(self.peek(), Some(Tok::PipePipe)) {
            self.bump();
            let right = self.parse_logical_and()?;
            left = binop("||", left, right);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Node> {
        let mut left = self.parse_bitwise_or()?;
        while matches!(self.peek(), Some(Tok::AmpAmp)) {
            self.bump();
            let right = self.parse_bitwise_or()?;
            left = binop("&&", left, right);
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Node> {
        let mut left = self.parse_bitwise_xor()?;
        while matches!(self.peek(), Some(Tok::Pipe)) {
            self.bump();
            let right = self.parse_bitwise_xor()?;
            left = binop("|", left, right);
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Node> {
        let mut left = self.parse_bitwise_and()?;
        while matches!(self.peek(), Some(Tok::Caret)) {
            self.bump();
            let right = self.parse_bitwise_and()?;
            left = binop("^", left, right);
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Node> {
        let mut left = self.parse_equality()?;
        while matches!(self.peek(), Some(Tok::Amp)) {
            self.bump();
            let right = self.parse_equality()?;
            left = binop("&", left, right);
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Node> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Some(Tok::EqEq) => "==",
                Some(Tok::Ne) => "!=",
                _ => break,
            };
            self.bump();
            let right = self.parse_comparison()?;
            left = binop(op, left, right);
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Node> {
        let mut left = self.parse_shift()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Lt) => "<",
                Some(Tok::Gt) => ">",
                Some(Tok::Le) => "<=",
                Some(Tok::Ge) => ">=",
                _ => break,
            };
            self.bump();
            let right = self.parse_shift()?;
            left = binop(op, left, right);
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Node> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Shl) => "<<",
                Some(Tok::Shr) => ">>",
                _ => break,
            };
            self.bump();
            let right = self.parse_additive()?;
            left = binop(op, left, right);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Node> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => "+",
                Some(Tok::Minus) => "-",
                _ => break,
            };
            self.bump();
            let right = self.parse_multiplicative()?;
            left = binop(op, left, right);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Node> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => "*",
                Some(Tok::Slash) => "/",
                Some(Tok::Percent) => "%",
                _ => break,
            };
            self.bump();
            let right = self.parse_power()?;
            left = binop(op, left, right);
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Node> {
        let left = self.parse_unary()?;
        if matches!(self.peek(), Some(Tok::Power)) {
            self.bump();
            let right = self.parse_power()?; // right-associative
            return Ok(binop("**", left, right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Node> {
        let op = match self.peek() {
            Some(Tok::Bang) => "!",
            Some(Tok::Tilde) => "~",
            Some(Tok::Minus) => "-",
            Some(Tok::Plus) => "+",
            _ => return self.parse_prefix(),
        };
        self.bump();
        let operand = self.parse_unary()?;
        Ok(unop(op, operand))
    }

    fn parse_prefix(&mut self) -> Result<Node> {
        match self.peek() {
            Some(Tok::Inc) => {
                self.bump();
                let operand = self.parse_unary()?;
                Ok(mk(NodeKind::ArithPreIncr {
                    operand: Box::new(operand),
                }))
            }
            Some(Tok::Dec) => {
                self.bump();
                let operand = self.parse_unary()?;
                Ok(mk(NodeKind::ArithPreDecr {
                    operand: Box::new(operand),
                }))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Node> {
        let prim = self.parse_primary()?;
        match self.peek() {
            Some(Tok::Inc) => {
                self.bump();
                Ok(mk(NodeKind::ArithPostIncr {
                    operand: Box::new(prim),
                }))
            }
            Some(Tok::Dec) => {
                self.bump();
                Ok(mk(NodeKind::ArithPostDecr {
                    operand: Box::new(prim),
                }))
            }
            _ => Ok(prim),
        }
    }

    fn parse_primary(&mut self) -> Result<Node> {
        // Inspect the kind without cloning; only take ownership when we
        // need the inner `String` to build the AST node. `Tok::Comma` is a
        // zero-sized sentinel that never reappears after `bump()`.
        match self.peek() {
            None => Err(err("unexpected end of arithmetic expression")),
            Some(Tok::Number(_)) => {
                let Tok::Number(n) = self.take_current() else {
                    unreachable!("peeked Number");
                };
                Ok(mk(NodeKind::ArithNumber { value: n }))
            }
            Some(Tok::Ident(_)) => {
                let Tok::Ident(name) = self.take_current() else {
                    unreachable!("peeked Ident");
                };
                self.maybe_subscript(name)
            }
            Some(Tok::LParen) => {
                self.bump();
                let inner = self.parse_expression()?;
                if !matches!(self.peek(), Some(Tok::RParen)) {
                    return Err(err("expected ')' in arithmetic expression"));
                }
                self.bump();
                Ok(inner)
            }
            _ => Err(err("unexpected token in arithmetic expression")),
        }
    }

    /// Takes ownership of the current token, advancing `pos`. Caller must
    /// have already confirmed via `peek()` that a token is present.
    fn take_current(&mut self) -> Tok {
        let taken = std::mem::replace(&mut self.tokens[self.pos], Tok::Comma);
        self.pos += 1;
        taken
    }

    fn maybe_subscript(&mut self, name: String) -> Result<Node> {
        if !matches!(self.peek(), Some(Tok::LBracket)) {
            return Ok(mk(NodeKind::ArithVar { name }));
        }
        self.bump();
        let index = self.parse_expression()?;
        if !matches!(self.peek(), Some(Tok::RBracket)) {
            return Err(err("expected ']' in array subscript"));
        }
        self.bump();
        Ok(mk(NodeKind::ArithSubscript {
            array: name,
            index: Box::new(index),
        }))
    }
}

const fn assign_op(tok: &Tok) -> Option<&'static str> {
    Some(match tok {
        Tok::Assign => "=",
        Tok::AddAssign => "+=",
        Tok::SubAssign => "-=",
        Tok::MulAssign => "*=",
        Tok::DivAssign => "/=",
        Tok::ModAssign => "%=",
        Tok::ShlAssign => "<<=",
        Tok::ShrAssign => ">>=",
        Tok::AndAssign => "&=",
        Tok::XorAssign => "^=",
        Tok::OrAssign => "|=",
        _ => return None,
    })
}

// ------------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------------

const fn mk(kind: NodeKind) -> Node {
    Node::empty(kind)
}

fn binop(op: &str, left: Node, right: Node) -> Node {
    mk(NodeKind::ArithBinaryOp {
        op: op.to_string(),
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn unop(op: &str, operand: Node) -> Node {
    mk(NodeKind::ArithUnaryOp {
        op: op.to_string(),
        operand: Box::new(operand),
    })
}

fn err(msg: impl Into<String>) -> RableError {
    RableError::parse(msg, 0, 1)
}

// ------------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
