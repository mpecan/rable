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
//!
//! # Layout
//!
//! | file          | responsibility                                       |
//! |---------------|-------------------------------------------------------|
//! | `tokenizer.rs`| lexes the expression text into `Tok` values          |
//! | `parser.rs`   | `ArithParser` + precedence-climbing cascade           |
//! | `tests.rs`    | unit tests for tokenizer and parser combined          |

mod parser;
mod tokenizer;

use crate::ast::{Node, NodeKind};
use crate::error::{RableError, Result};

use parser::ArithParser;
use tokenizer::tokenize;

/// Maximum recursion depth for `parse_expression`, which is re-entered for
/// parenthesized groups, array subscripts, and ternary `if_true` branches.
/// Bounds the stack on pathological inputs like `((((((x))))))`.
///
/// Each recursive `parse_expression` call cascades through ~15 precedence
/// levels before reaching `parse_primary`, so keep the limit low enough
/// that even debug builds on small test-thread stacks stay safe. Real
/// bash arithmetic expressions never nest deeper than a handful of levels.
pub(super) const MAX_ARITH_DEPTH: usize = 32;

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

/// Shared `Node` constructor used across `mod.rs` and `parser.rs`.
pub(super) const fn mk(kind: NodeKind) -> Node {
    Node::empty(kind)
}

/// Shared error constructor used by both the tokenizer and the parser.
/// Positions are zero because the arithmetic subparser operates on an
/// already-extracted substring; the outer parser attaches real spans when
/// wrapping the returned node into an `ArithmeticExpansion`.
pub(super) fn err(msg: impl Into<String>) -> RableError {
    RableError::parse(msg, 0, 1)
}

#[cfg(test)]
mod tests;
