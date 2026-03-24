use std::fmt;

use crate::ast::{CasePattern, Node};
use crate::format;

/// Dispatch formatting to type-specific helpers, keeping the match arms short.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word { value, .. } => {
                write!(f, "(word \"")?;
                write_word_value(f, value)?;
                write!(f, "\")")
            }
            Self::Command { words, redirects } => write_spaced(f, "(command", words, redirects),
            Self::Pipeline { commands } => write_pipeline(f, commands),
            Self::List { parts } => write_list(f, parts),
            Self::Operator { op } => write!(f, "{}", operator_name(op)),
            Self::Empty | Self::PipeBoth => Ok(()),
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
                write_in_list(f, words.as_deref())?;
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
                write!(f, "(arith-for")?;
                write!(f, " (init (word \"{init}\"))")?;
                write!(f, " (test (word \"{cond}\"))")?;
                write!(f, " (step (word \"{incr}\"))")?;
                write!(f, " {body})")?;
                write_redirects(f, redirects)
            }
            Self::Select {
                var,
                words,
                body,
                redirects,
            } => {
                write!(f, "(select (word \"{var}\")")?;
                write_in_list(f, words.as_deref())?;
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
            Self::Redirect { op, target, fd } => write_redirect(f, op, target, *fd),
            Self::HereDoc {
                content,
                strip_tabs,
                ..
            } => {
                let op = if *strip_tabs { "<<-" } else { "<<" };
                // Here-doc content uses literal newlines, not \\n
                write!(f, "(redirect \"{op}\" \"{content}\")")
            }

            // Expansions
            Self::ParamExpansion { param, op, arg } => {
                write_param(f, "${{", param, op.as_deref(), arg.as_deref())
            }
            Self::ParamLength { param } => write!(f, "${{#{param}}}"),
            Self::ParamIndirect { param, op, arg } => {
                write_param(f, "${{!", param, op.as_deref(), arg.as_deref())
            }
            Self::CommandSubstitution { command, brace } => {
                let tag = if *brace { "cmdsub-brace" } else { "cmdsub" };
                write!(f, "({tag} {command})")
            }
            Self::ProcessSubstitution { direction, command } => {
                write!(f, "(procsub {direction} {command})")
            }
            Self::AnsiCQuote { content } => write!(f, "$'{content}'"),
            Self::LocaleString { content } => write!(f, "$\"{content}\""),
            Self::ArithmeticExpansion { expression } => {
                write_arith_wrapper(f, "arith", expression.as_deref())
            }
            Self::ArithmeticCommand {
                redirects,
                raw_content,
                ..
            } => {
                if raw_content.is_empty() {
                    write!(f, "(arith (word \"\"))")?;
                } else {
                    write!(f, "(arith (word \"{raw_content}\"))")?;
                }
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
            Self::CondTerm { value } => {
                // Don't escape quotes — preserve literal quote chars
                write!(f, "(cond-term \"{value}\")")
            }
            Self::UnaryTest { op, operand } => {
                write!(f, "(cond-unary \"{op}\" {operand})")
            }
            Self::BinaryTest { op, left, right } => {
                write!(f, "(cond-binary \"{op}\" {left} {right})")
            }
            Self::CondAnd { left, right } => write!(f, "(cond-and {left} {right})"),
            Self::CondOr { left, right } => write!(f, "(cond-or {left} {right})"),
            Self::CondNot { operand } => write!(f, "(cond-not {operand})"),
            Self::CondParen { inner } => write!(f, "(cond-paren {inner})"),

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

// --- helpers ---

/// Writes a word value with proper escaping/formatting per segment type.
/// `$(...)` content is reformatted and written with literal newlines.
/// ANSI-C and locale prefixes are processed. Regular text is escaped.
fn write_word_value(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    let mut i = 0;
    let chars: Vec<char> = value.chars().collect();

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '\'' {
            // ANSI-C quoting
            i += 2;
            let processed = process_ansi_c_content(&chars, &mut i);
            write_escaped_word(f, "'")?;
            write_escaped_word(f, &processed)?;
            write_escaped_word(f, "'")?;
        } else if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '"' {
            // Locale string — strip $
            i += 1;
            while i < chars.len() {
                write_escaped_char(f, chars[i])?;
                if chars[i] == '"' && i > 0 {
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' {
            // Check for $(( )) arithmetic — don't reformat
            if i + 2 < chars.len() && chars[i + 2] == '(' {
                // Arithmetic expansion: $((...)) — output raw
                write_escaped_char(f, chars[i])?;
                i += 1;
                continue;
            }
            // Command substitution — try reformatting
            write!(f, "$(")?;
            i += 2;
            let content = extract_paren_content(&chars, &mut i);
            if let Some(reformatted) = format::reformat_bash(&content) {
                write_escaped_word(f, &reformatted)?;
            } else {
                let normalized = normalize_cmdsub_content(&content);
                write_escaped_word(f, &normalized)?;
            }
            write!(f, ")")?;
        } else {
            write_escaped_char(f, chars[i])?;
            i += 1;
        }
    }
    Ok(())
}

/// Extracts content from matched parentheses (for command substitution).
fn extract_paren_content(chars: &[char], pos: &mut usize) -> String {
    let mut content = String::new();
    let mut depth = 1;
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '(' {
            depth += 1;
            content.push(c);
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                *pos += 1;
                return content;
            }
            content.push(c);
        } else if c == '\'' {
            // Single-quoted string — read until closing '
            content.push(c);
            *pos += 1;
            while *pos < chars.len() && chars[*pos] != '\'' {
                content.push(chars[*pos]);
                *pos += 1;
            }
            if *pos < chars.len() {
                content.push(chars[*pos]); // closing '
            }
        } else if c == '"' {
            content.push(c);
            *pos += 1;
            while *pos < chars.len() && chars[*pos] != '"' {
                content.push(chars[*pos]);
                if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                    *pos += 1;
                    content.push(chars[*pos]);
                }
                *pos += 1;
            }
            if *pos < chars.len() {
                content.push(chars[*pos]); // closing "
            }
        } else {
            content.push(c);
        }
        *pos += 1;
    }
    content
}

/// Normalizes command substitution content:
/// - Strips leading/trailing whitespace and newlines
/// - Strips trailing semicolons
/// - Adds space after `<` for file reading shortcuts
fn normalize_cmdsub_content(content: &str) -> String {
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

/// Process ANSI-C escape sequences inside `$'...'`.
/// `chars` is the full character array, `pos` points to the first char after `$'`.
/// Returns the processed content (without surrounding quotes).
/// Advances `pos` past the closing `'`.
#[allow(clippy::too_many_lines)]
fn process_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
    let mut out = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '\'' {
            *pos += 1; // skip closing '
            return out;
        }
        if c == '\\' && *pos + 1 < chars.len() {
            *pos += 1;
            let esc = chars[*pos];
            *pos += 1;
            match esc {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'a' => out.push('\x07'),
                'b' => {
                    out.pop(); // backspace removes previous char
                }
                'f' => out.push('\x0C'),
                'v' => out.push('\x0B'),
                'e' | 'E' => out.push('\x1B'),
                '\\' => out.push('\\'),
                '\'' => {
                    // Escaped single quote: output as '\\''
                    out.push('\'');
                    out.push('\\');
                    out.push('\'');
                    out.push('\'');
                    return process_ansi_c_continue(chars, pos, out);
                }
                '"' => out.push('"'),
                'x' => {
                    // Hex escape: \xNN
                    let hex = read_hex(chars, pos, 2);
                    if let Some(ch) = char::from_u32(hex) {
                        out.push(ch);
                    }
                }
                'u' => {
                    // Unicode: \uNNNN
                    let val = read_hex(chars, pos, 4);
                    if let Some(ch) = char::from_u32(val) {
                        out.push(ch);
                    }
                }
                'U' => {
                    // Unicode long: \UNNNNNNNN
                    let val = read_hex(chars, pos, 8);
                    if let Some(ch) = char::from_u32(val) {
                        out.push(ch);
                    }
                }
                '0'..='7' => {
                    // Octal escape
                    let mut val = u32::from(esc as u8 - b'0');
                    for _ in 0..2 {
                        if *pos < chars.len() && chars[*pos] >= '0' && chars[*pos] <= '7' {
                            val = val * 8 + u32::from(chars[*pos] as u8 - b'0');
                            *pos += 1;
                        }
                    }
                    if let Some(ch) = char::from_u32(val) {
                        out.push(ch);
                    }
                }
                _ => {
                    out.push('\\');
                    out.push(esc);
                }
            }
        } else {
            out.push(c);
            *pos += 1;
        }
    }
    out
}

/// Continue processing after an escaped quote split.
fn process_ansi_c_continue(chars: &[char], pos: &mut usize, mut out: String) -> String {
    // After \' we output '\\'' and need to continue in a new quote context
    out.push_str(&process_ansi_c_content(chars, pos));
    out
}

/// Read up to `max` hex digits from chars at pos.
fn read_hex(chars: &[char], pos: &mut usize, max: usize) -> u32 {
    let mut val = 0u32;
    for _ in 0..max {
        if *pos < chars.len() && chars[*pos].is_ascii_hexdigit() {
            val = val * 16 + chars[*pos].to_digit(16).unwrap_or(0);
            *pos += 1;
        } else {
            break;
        }
    }
    val
}

fn operator_name(op: &str) -> &str {
    match op {
        "&&" => "and",
        "||" => "or",
        ";" | "\n" => "semi",
        "&" => "background",
        "|" => "pipe",
        other => other,
    }
}

/// Writes a single character with S-expression escaping.
fn write_escaped_char(f: &mut fmt::Formatter<'_>, ch: char) -> fmt::Result {
    match ch {
        '"' => write!(f, "\\\""),
        '\\' => write!(f, "\\\\"),
        '\n' => write!(f, "\\n"),
        '\t' => write!(f, "\\t"),
        _ => write!(f, "{ch}"),
    }
}

/// Writes a word value with proper escaping for S-expression output.
fn write_escaped_word(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    for ch in value.chars() {
        match ch {
            '"' => write!(f, "\\\"")?,
            '\\' => write!(f, "\\\\")?,
            '\n' => write!(f, "\\n")?,
            '\t' => write!(f, "\\t")?,
            _ => write!(f, "{ch}")?,
        }
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

fn write_spaced(
    f: &mut fmt::Formatter<'_>,
    tag: &str,
    first: &[Node],
    second: &[Node],
) -> fmt::Result {
    write!(f, "{tag}")?;
    for n in first {
        write!(f, " {n}")?;
    }
    for n in second {
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

/// Pipelines are right-nested: `(pipe a (pipe b c))`.
fn write_pipeline(f: &mut fmt::Formatter<'_>, commands: &[Node]) -> fmt::Result {
    let filtered: Vec<_> = commands
        .iter()
        .filter(|c| !matches!(c, Node::PipeBoth))
        .collect();
    if filtered.len() == 1 {
        return write!(f, "{}", filtered[0]);
    }
    // Group commands with their trailing redirects
    let mut groups: Vec<Vec<&Node>> = Vec::new();
    for cmd in &filtered {
        if matches!(cmd, Node::Redirect { .. }) {
            // Attach redirect to the previous group
            if let Some(last) = groups.last_mut() {
                last.push(cmd);
            } else {
                groups.push(vec![cmd]);
            }
        } else {
            groups.push(vec![cmd]);
        }
    }
    write_pipeline_groups(f, &groups, 0)
}

fn write_pipeline_groups(
    f: &mut fmt::Formatter<'_>,
    groups: &[Vec<&Node>],
    idx: usize,
) -> fmt::Result {
    if idx >= groups.len() {
        return Ok(());
    }
    if idx == groups.len() - 1 {
        // Last group: write all elements
        for (j, node) in groups[idx].iter().enumerate() {
            if j > 0 {
                write!(f, " ")?;
            }
            write!(f, "{node}")?;
        }
        return Ok(());
    }
    write!(f, "(pipe ")?;
    for (j, node) in groups[idx].iter().enumerate() {
        if j > 0 {
            write!(f, " ")?;
        }
        write!(f, "{node}")?;
    }
    write!(f, " ")?;
    write_pipeline_groups(f, groups, idx + 1)?;
    write!(f, ")")
}

/// Lists use left-associative nesting: `(and (and a b) c)`.
fn write_list(f: &mut fmt::Formatter<'_>, parts: &[Node]) -> fmt::Result {
    if parts.len() == 1 {
        return write!(f, "{}", parts[0]);
    }
    let mut items: Vec<&Node> = Vec::new();
    let mut ops: Vec<&str> = Vec::new();
    for part in parts {
        if let Node::Operator { op } = part {
            ops.push(op);
        } else {
            items.push(part);
        }
    }
    if items.len() == 1 && ops.is_empty() {
        return write!(f, "{}", items[0]);
    }
    // Left-associative: build from left to right
    write_list_left_assoc(f, &items, &ops)
}

fn write_list_left_assoc(f: &mut fmt::Formatter<'_>, items: &[&Node], ops: &[&str]) -> fmt::Result {
    // Handle trailing unary operator (e.g., "cmd &" → "(background cmd)")
    if items.len() == 1 && ops.len() == 1 {
        let sexp_op = operator_name(ops[0]);
        return write!(f, "({sexp_op} {})", items[0]);
    }
    if items.len() <= 1 && ops.is_empty() {
        if let Some(item) = items.first() {
            return write!(f, "{item}");
        }
        return Ok(());
    }

    // For a trailing background operator with no RHS
    if items.len() == ops.len() {
        // Last op is trailing (e.g., "cmd &")
        let sexp_op = operator_name(ops[ops.len() - 1]);
        write!(f, "({sexp_op} ")?;
        write_list_left_assoc(f, &items[..items.len()], &ops[..ops.len() - 1])?;
        return write!(f, ")");
    }

    // Write left-associatively: ((op1 a b) op2 c) op3 d) ...
    // Open all the parens first, then close them
    for i in (1..ops.len()).rev() {
        write!(f, "({} ", operator_name(ops[i]))?;
    }
    write!(f, "({} {} {})", operator_name(ops[0]), items[0], items[1])?;
    for i in 1..ops.len() {
        write!(f, " {})", items[i + 1])?;
    }
    Ok(())
}

/// Writes a word list wrapped in `(in ...)` for `for`/`select` statements.
fn write_in_list(f: &mut fmt::Formatter<'_>, words: Option<&[Node]>) -> fmt::Result {
    if let Some(ws) = words {
        write!(f, " (in")?;
        for w in ws {
            write!(f, " {w}")?;
        }
        write!(f, ")")?;
    }
    Ok(())
}

fn write_redirect(f: &mut fmt::Formatter<'_>, op: &str, target: &Node, _fd: i32) -> fmt::Result {
    write!(f, "(redirect \"{op}\" ")?;
    if let Node::Word { value, .. } = target {
        // For fd operations (>&, <&, >&-, <&-), output bare number
        let is_fd_op =
            op.starts_with(">&") || op.starts_with("<&") || op.ends_with("&-") || op.ends_with('&');
        if is_fd_op && value.chars().all(|c| c.is_ascii_digit() || c == '-') {
            write!(f, "{value})")
        } else {
            // Strip $" prefix from locale strings
            let val = value
                .strip_prefix('$')
                .filter(|rest| rest.starts_with('"'))
                .unwrap_or(value);
            write!(f, "\"{val}\")")
        }
    } else {
        write!(f, "{target})")
    }
}

fn write_param(
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

fn write_arith_wrapper(
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
