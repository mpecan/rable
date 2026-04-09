//! Precedence-climbing parser that turns a flat `Vec<Tok>` into an `Arith*`
//! AST tree.

use crate::ast::{Node, NodeKind};
use crate::error::Result;

use super::tokenizer::Tok;
use super::{MAX_ARITH_DEPTH, err, mk};

pub(super) struct ArithParser {
    tokens: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl ArithParser {
    pub(super) const fn new(tokens: Vec<Tok>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    pub(super) const fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    const fn bump(&mut self) {
        self.pos += 1;
    }

    pub(super) fn parse_expression(&mut self) -> Result<Node> {
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
