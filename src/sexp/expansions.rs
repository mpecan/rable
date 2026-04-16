//! Parameter expansion, arithmetic expansion, and arithmetic-node formatting
//! helpers for `Display for NodeKind`.

use std::fmt;

use crate::ast::{Node, NodeKind};

use super::{write_optional, write_tagged_list};

pub(super) fn write_param(
    f: &mut fmt::Formatter<'_>,
    prefix: &str,
    param: &str,
    op: Option<&str>,
    arg: Option<&str>,
) -> fmt::Result {
    if op.is_some() || arg.is_some() {
        write!(f, "{prefix}{param}")?;
        if let Some(o) = op {
            write!(f, "{o}")?;
        }
        if let Some(a) = arg {
            write!(f, "{a}")?;
        }
        write!(f, "}}")
    } else {
        write!(f, "${param}")
    }
}

pub(super) fn write_arith_wrapper(
    f: &mut fmt::Formatter<'_>,
    tag: &str,
    expression: Option<&Node>,
) -> fmt::Result {
    write!(f, "({tag}")?;
    if let Some(expr) = expression {
        write!(f, " {expr}")?;
    }
    write!(f, ")")
}

/// Formats shell expansion variants: parameter, command, process,
/// ANSI-C, locale, brace, and arithmetic substitution.
pub(super) fn fmt_expansion(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::ParamExpansion { param, op, arg } => {
            write_param(f, "${{", param, op.as_deref(), arg.as_deref())
        }
        NodeKind::ParamLength { param } => write!(f, "${{#{param}}}"),
        NodeKind::ParamIndirect { param, op, arg } => {
            write_param(f, "${{!", param, op.as_deref(), arg.as_deref())
        }
        NodeKind::CommandSubstitution { command, brace } => {
            let tag = if *brace { "cmdsub-brace" } else { "cmdsub" };
            write!(f, "({tag} {command})")
        }
        NodeKind::ProcessSubstitution { direction, command } => {
            write!(f, "(procsub {direction} {command})")
        }
        NodeKind::AnsiCQuote { content, .. } => write!(f, "$'{content}'"),
        NodeKind::LocaleString { content, .. } => write!(f, "$\"{content}\""),
        NodeKind::BraceExpansion { content } => write!(f, "{content}"),
        NodeKind::ArithmeticExpansion { expression } => {
            write_arith_wrapper(f, "arith", expression.as_deref())
        }
        _ => unreachable!("fmt_expansion called with non-expansion variant"),
    }
}

/// Formats arithmetic-tree nodes produced by the arithmetic parser.
pub(super) fn fmt_arith(f: &mut fmt::Formatter<'_>, kind: &NodeKind) -> fmt::Result {
    match kind {
        NodeKind::ArithNumber { value } => write!(f, "{value}"),
        NodeKind::ArithVar { name } => write!(f, "{name}"),
        NodeKind::ArithBinaryOp { op, left, right } => write!(f, "({op} {left} {right})"),
        NodeKind::ArithUnaryOp { op, operand } => write!(f, "({op} {operand})"),
        NodeKind::ArithPreIncr { operand } => write!(f, "(pre++ {operand})"),
        NodeKind::ArithPostIncr { operand } => write!(f, "(post++ {operand})"),
        NodeKind::ArithPreDecr { operand } => write!(f, "(pre-- {operand})"),
        NodeKind::ArithPostDecr { operand } => write!(f, "(post-- {operand})"),
        NodeKind::ArithAssign { op, target, value } => write!(f, "({op} {target} {value})"),
        NodeKind::ArithTernary {
            condition,
            if_true,
            if_false,
        } => {
            write!(f, "(? {condition}")?;
            write_optional(f, if_true.as_deref())?;
            write_optional(f, if_false.as_deref())?;
            write!(f, ")")
        }
        NodeKind::ArithComma { left, right } => write!(f, "(, {left} {right})"),
        NodeKind::ArithSubscript { array, index } => write!(f, "(subscript {array} {index})"),
        NodeKind::ArithEmpty => write!(f, "(empty)"),
        NodeKind::ArithEscape { ch } => write!(f, "(escape {ch})"),
        NodeKind::ArithDeprecated { expression } => write!(f, "(arith-deprecated {expression})"),
        NodeKind::ArithConcat { parts } => write_tagged_list(f, "concat", parts),
        _ => unreachable!("fmt_arith called with non-arithmetic variant"),
    }
}
