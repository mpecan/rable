//! Parameter and arithmetic-expansion helpers for `Display for NodeKind`.

use std::fmt;

use crate::ast::Node;

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
