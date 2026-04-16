//! S-expression rendering for AST nodes via `Display`.
//!
//! The implementation is split across sibling files by concern:
//!
//! | file            | responsibility                                         |
//! |-----------------|--------------------------------------------------------|
//! | `mod.rs`        | `Display` impls, module glue, low-level escape helpers |
//! | `commands.rs`   | pipeline / list / `(in …)` formatting                  |
//! | `redirects.rs`  | `(redirect …)` and arithmetic-command wrappers         |
//! | `expansions.rs` | parameter-expansion and `(arith …)` wrappers           |
//! | `ansi_c.rs`     | `$'…'` escape walker used here and by `format`         |
//! | `word.rs`       | segment parser + word-segment formatter                |

pub(crate) mod ansi_c;
pub(crate) mod commands;
pub(crate) mod expansions;
pub(crate) mod redirects;
pub(crate) mod word;

use std::fmt;

use crate::ast::{CasePattern, Node, NodeKind};

pub(crate) use ansi_c::process_ansi_c_content;

/// `Node` delegates `Display` to its inner `NodeKind`.
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

/// Dispatch formatting to per-category helpers in sibling submodules.
///
/// The compiler enforces exhaustiveness: adding a new `NodeKind` variant
/// without a dispatch entry is a compile error.
impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word { .. } | Self::WordLiteral { .. } | Self::CondTerm { .. } => {
                word::fmt_word_like(f, self)
            }
            Self::Command { .. }
            | Self::Pipeline { .. }
            | Self::List { .. }
            | Self::Empty
            | Self::Comment { .. }
            | Self::Negation { .. }
            | Self::Time { .. }
            | Self::Array { .. } => commands::fmt_command_like(f, self),
            Self::If { .. }
            | Self::While { .. }
            | Self::Until { .. }
            | Self::For { .. }
            | Self::ForArith { .. }
            | Self::Select { .. }
            | Self::Case { .. }
            | Self::Function { .. }
            | Self::Subshell { .. }
            | Self::BraceGroup { .. }
            | Self::Coproc { .. } => commands::fmt_compound(f, self),
            Self::ConditionalExpr { .. }
            | Self::UnaryTest { .. }
            | Self::BinaryTest { .. }
            | Self::CondAnd { .. }
            | Self::CondOr { .. }
            | Self::CondNot { .. }
            | Self::CondParen { .. } => commands::fmt_conditional(f, self),
            Self::Redirect { .. } | Self::HereDoc { .. } | Self::ArithmeticCommand { .. } => {
                redirects::fmt_redirect_like(f, self)
            }
            Self::ParamExpansion { .. }
            | Self::ParamLength { .. }
            | Self::ParamIndirect { .. }
            | Self::CommandSubstitution { .. }
            | Self::ProcessSubstitution { .. }
            | Self::AnsiCQuote { .. }
            | Self::LocaleString { .. }
            | Self::BraceExpansion { .. }
            | Self::ArithmeticExpansion { .. } => expansions::fmt_expansion(f, self),
            Self::ArithNumber { .. }
            | Self::ArithVar { .. }
            | Self::ArithBinaryOp { .. }
            | Self::ArithUnaryOp { .. }
            | Self::ArithPreIncr { .. }
            | Self::ArithPostIncr { .. }
            | Self::ArithPreDecr { .. }
            | Self::ArithPostDecr { .. }
            | Self::ArithAssign { .. }
            | Self::ArithTernary { .. }
            | Self::ArithComma { .. }
            | Self::ArithSubscript { .. }
            | Self::ArithEmpty
            | Self::ArithEscape { .. }
            | Self::ArithDeprecated { .. }
            | Self::ArithConcat { .. } => expansions::fmt_arith(f, self),
        }
    }
}

impl fmt::Display for CasePattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(pattern (")?;
        for (i, p) in self.patterns.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{p}")?;
        }
        write!(f, ")")?;
        match &self.body {
            Some(body) => write!(f, " {body}")?,
            None => write!(f, " ()")?,
        }
        write!(f, ")")
    }
}

// --- shared helpers used by mod.rs and submodules --------------------------

/// Normalizes command substitution content:
/// - Strips leading/trailing whitespace and newlines
/// - Strips trailing semicolons
/// - Adds space after `<` for file reading shortcuts
pub(crate) fn normalize_cmdsub_content(content: &str) -> String {
    let trimmed = content.trim();
    let stripped = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    // Normalize $(<file) to $(< file)
    if let Some(rest) = stripped.strip_prefix('<')
        && !rest.starts_with(['<', ' '])
    {
        return format!("< {rest}");
    }
    stripped.to_string()
}

/// Writes a single character with S-expression escaping.
pub(super) fn write_escaped_char(f: &mut fmt::Formatter<'_>, ch: char) -> fmt::Result {
    match ch {
        '"' => write!(f, "\\\""),
        '\\' => write!(f, "\\\\"),
        '\n' => write!(f, "\\n"),
        '\t' => write!(f, "\\t"),
        _ => write!(f, "{ch}"),
    }
}

/// Writes a word value with proper escaping for S-expression output.
pub(super) fn write_escaped_word(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    for ch in value.chars() {
        write_escaped_char(f, ch)?;
    }
    Ok(())
}

pub(super) fn write_optional(f: &mut fmt::Formatter<'_>, node: Option<&Node>) -> fmt::Result {
    if let Some(n) = node {
        write!(f, " {n}")?;
    }
    Ok(())
}

pub(super) fn write_redirects(f: &mut fmt::Formatter<'_>, redirects: &[Node]) -> fmt::Result {
    for r in redirects {
        write!(f, " {r}")?;
    }
    Ok(())
}

pub(super) fn write_spaced3(
    f: &mut fmt::Formatter<'_>,
    tag: &str,
    first: &[Node],
    second: &[Node],
    third: &[Node],
) -> fmt::Result {
    write!(f, "{tag}")?;
    for n in first {
        write!(f, " {n}")?;
    }
    for n in second {
        write!(f, " {n}")?;
    }
    for n in third {
        write!(f, " {n}")?;
    }
    write!(f, ")")
}

pub(super) fn write_tagged_list(
    f: &mut fmt::Formatter<'_>,
    tag: &str,
    items: &[Node],
) -> fmt::Result {
    write!(f, "({tag}")?;
    for n in items {
        write!(f, " {n}")?;
    }
    write!(f, ")")
}
