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

/// Dispatch formatting to type-specific helpers, keeping the match arms short.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word { value, spans, .. } => {
                write!(f, "(word \"")?;
                if spans.is_empty() {
                    // Synthetic word (no lexer token) — escape directly
                    write_escaped_word(f, value)?;
                } else {
                    let segments = word::segments_from_spans(value, spans);
                    word::write_word_segments(f, &segments)?;
                }
                write!(f, "\")")
            }
            Self::WordLiteral { value } => write!(f, "{value}"),
            Self::Command {
                assignments,
                words,
                redirects,
            } => write_spaced3(f, "(command", assignments, words, redirects),
            Self::Pipeline { commands, .. } => commands::write_pipeline(f, commands),
            Self::List { items } => commands::write_list(f, items),
            Self::Empty => write!(f, "(command)"),
            Self::Comment { text } => write!(f, "(comment \"{text}\")"),

            // Compound commands
            Self::If {
                condition,
                then_body,
                else_body,
                redirects,
            } => {
                write!(f, "(if {condition} {then_body}")?;
                write_optional(f, else_body.as_deref())?;
                write!(f, ")")?;
                write_redirects(f, redirects)
            }
            Self::While {
                condition,
                body,
                redirects,
            } => {
                write!(f, "(while {condition} {body})")?;
                write_redirects(f, redirects)
            }
            Self::Until {
                condition,
                body,
                redirects,
            } => {
                write!(f, "(until {condition} {body})")?;
                write_redirects(f, redirects)
            }
            Self::For {
                var,
                words,
                body,
                redirects,
            } => {
                write!(f, "(for (word \"{var}\")")?;
                commands::write_in_list(f, words.as_deref())?;
                write!(f, " {body})")?;
                write_redirects(f, redirects)
            }
            Self::ForArith {
                init,
                cond,
                incr,
                body,
                redirects,
            } => {
                write!(f, "(arith-for (init (word \"")?;
                write_escaped_word(f, init)?;
                write!(f, "\")) (test (word \"")?;
                write_escaped_word(f, cond)?;
                write!(f, "\")) (step (word \"")?;
                write_escaped_word(f, incr)?;
                write!(f, "\")) {body})")?;
                write_redirects(f, redirects)
            }
            Self::Select {
                var,
                words,
                body,
                redirects,
            } => {
                write!(f, "(select (word \"{var}\")")?;
                commands::write_in_list(f, words.as_deref())?;
                write!(f, " {body})")?;
                write_redirects(f, redirects)
            }
            Self::Case {
                word,
                patterns,
                redirects,
            } => {
                write!(f, "(case {word}")?;
                for p in patterns {
                    write!(f, " {p}")?;
                }
                write!(f, ")")?;
                write_redirects(f, redirects)
            }
            Self::Function { name, body } => write!(f, "(function \"{name}\" {body})"),
            Self::Subshell { body, redirects } => {
                write!(f, "(subshell {body})")?;
                write_redirects(f, redirects)
            }
            Self::BraceGroup { body, redirects } => {
                write!(f, "(brace-group {body})")?;
                write_redirects(f, redirects)
            }
            Self::Coproc { name, command } => {
                let n = name.as_deref().unwrap_or("COPROC");
                write!(f, "(coproc \"{n}\" {command})")
            }

            // Redirections
            Self::Redirect { op, target, fd } => redirects::write_redirect(f, op, target, *fd),
            Self::HereDoc {
                content,
                strip_tabs,
                ..
            } => redirects::write_heredoc(f, content, *strip_tabs),

            // Expansions
            Self::ParamExpansion { param, op, arg } => {
                expansions::write_param(f, "${{", param, op.as_deref(), arg.as_deref())
            }
            Self::ParamLength { param } => write!(f, "${{#{param}}}"),
            Self::ParamIndirect { param, op, arg } => {
                expansions::write_param(f, "${{!", param, op.as_deref(), arg.as_deref())
            }
            Self::CommandSubstitution { command, brace } => {
                let tag = if *brace { "cmdsub-brace" } else { "cmdsub" };
                write!(f, "({tag} {command})")
            }
            Self::ProcessSubstitution { direction, command } => {
                write!(f, "(procsub {direction} {command})")
            }
            Self::AnsiCQuote { content, .. } => write!(f, "$'{content}'"),
            Self::LocaleString { content, .. } => write!(f, "$\"{content}\""),
            Self::BraceExpansion { content } => write!(f, "{content}"),
            Self::ArithmeticExpansion { expression } => {
                expansions::write_arith_wrapper(f, "arith", expression.as_deref())
            }
            Self::ArithmeticCommand {
                redirects,
                raw_content,
                ..
            } => {
                redirects::write_arith_command(f, raw_content)?;
                write_redirects(f, redirects)
            }

            // Arithmetic nodes
            Self::ArithNumber { value } => write!(f, "{value}"),
            Self::ArithVar { name } => write!(f, "{name}"),
            Self::ArithBinaryOp { op, left, right } => write!(f, "({op} {left} {right})"),
            Self::ArithUnaryOp { op, operand } => write!(f, "({op} {operand})"),
            Self::ArithPreIncr { operand } => write!(f, "(pre++ {operand})"),
            Self::ArithPostIncr { operand } => write!(f, "(post++ {operand})"),
            Self::ArithPreDecr { operand } => write!(f, "(pre-- {operand})"),
            Self::ArithPostDecr { operand } => write!(f, "(post-- {operand})"),
            Self::ArithAssign { op, target, value } => write!(f, "({op} {target} {value})"),
            Self::ArithTernary {
                condition,
                if_true,
                if_false,
            } => {
                write!(f, "(? {condition}")?;
                write_optional(f, if_true.as_deref())?;
                write_optional(f, if_false.as_deref())?;
                write!(f, ")")
            }
            Self::ArithComma { left, right } => write!(f, "(, {left} {right})"),
            Self::ArithSubscript { array, index } => write!(f, "(subscript {array} {index})"),
            Self::ArithEmpty => write!(f, "(empty)"),
            Self::ArithEscape { ch } => write!(f, "(escape {ch})"),
            Self::ArithDeprecated { expression } => write!(f, "(arith-deprecated {expression})"),
            Self::ArithConcat { parts } => write_tagged_list(f, "concat", parts),

            // Conditional expressions
            Self::ConditionalExpr { body, redirects } => {
                write!(f, "(cond {body})")?;
                write_redirects(f, redirects)
            }
            Self::CondTerm { value, spans } => {
                // Strip $" locale prefix
                let val = if value.starts_with("$\"") {
                    &value[1..]
                } else {
                    value
                };
                if spans.is_empty() {
                    write!(f, "(cond-term \"{val}\")")
                } else {
                    let segments = word::segments_from_spans(val, spans);
                    if segments
                        .iter()
                        .all(|s| matches!(s, word::WordSegment::Literal(_)))
                    {
                        // No expansions needing processing
                        write!(f, "(cond-term \"{val}\")")
                    } else {
                        write!(f, "(cond-term \"")?;
                        // CondTerm uses raw output (like redirect targets)
                        redirects::write_redirect_segments(f, &segments)?;
                        write!(f, "\")")
                    }
                }
            }
            Self::UnaryTest { op, operand } => {
                write!(f, "(cond-unary \"{op}\" {operand})")
            }
            Self::BinaryTest { op, left, right } => {
                write!(f, "(cond-binary \"{op}\" {left} {right})")
            }
            Self::CondAnd { left, right } => write!(f, "(cond-and {left} {right})"),
            Self::CondOr { left, right } => write!(f, "(cond-or {left} {right})"),
            // Parable drops negation in S-expression output — unwrap CondNot
            Self::CondNot { operand } => write!(f, "{operand}"),
            Self::CondParen { inner } => write!(f, "(cond-expr {inner})"),

            // Other
            Self::Negation { pipeline } => write!(f, "(negation {pipeline})"),
            Self::Time { pipeline, posix } => {
                if *posix {
                    write!(f, "(time -p {pipeline})")
                } else {
                    write!(f, "(time {pipeline})")
                }
            }
            Self::Array { elements } => write_tagged_list(f, "array", elements),
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

fn write_optional(f: &mut fmt::Formatter<'_>, node: Option<&Node>) -> fmt::Result {
    if let Some(n) = node {
        write!(f, " {n}")?;
    }
    Ok(())
}

fn write_redirects(f: &mut fmt::Formatter<'_>, redirects: &[Node]) -> fmt::Result {
    for r in redirects {
        write!(f, " {r}")?;
    }
    Ok(())
}

fn write_spaced3(
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

fn write_tagged_list(f: &mut fmt::Formatter<'_>, tag: &str, items: &[Node]) -> fmt::Result {
    write!(f, "({tag}")?;
    for n in items {
        write!(f, " {n}")?;
    }
    write!(f, ")")
}
