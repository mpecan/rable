pub(crate) mod ansi_c;
pub(crate) mod word;

use std::fmt;

use crate::ast::{CasePattern, ListItem, ListOperator, Node, NodeKind};

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
            Self::Pipeline { commands, .. } => write_pipeline(f, commands),
            Self::List { items } => write_list(f, items),
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
            Self::AnsiCQuote { content, .. } => write!(f, "$'{content}'"),
            Self::LocaleString { content, .. } => write!(f, "$\"{content}\""),
            Self::BraceExpansion { content } => write!(f, "{content}"),
            Self::ArithmeticExpansion { expression } => {
                write_arith_wrapper(f, "arith", expression.as_deref())
            }
            Self::ArithmeticCommand {
                redirects,
                raw_content,
                ..
            } => {
                write!(f, "(arith (word \"")?;
                write_escaped_word(f, raw_content)?;
                write!(f, "\"))")?;
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
                        write_redirect_segments(f, &segments)?;
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

// --- helpers ---

// Old write_word_value replaced by word::write_word_value (segment-based)

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

/// Process ANSI-C escape sequences inside `$'...'`.
/// `chars` is the full character array, `pos` points to the first char after `$'`.
/// Returns the processed content (without surrounding quotes).
/// Advances `pos` past the closing `'`.
#[allow(clippy::too_many_lines)]
pub(crate) fn process_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
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
                'b' => out.push('\x08'),
                'f' => out.push('\x0C'),
                'v' => out.push('\x0B'),
                'e' | 'E' => out.push('\x1B'),
                '\\' => out.push('\\'),
                'c' => {
                    // Control character: \cX → chr(X & 0x1F)
                    if *pos < chars.len() {
                        let ctrl = chars[*pos];
                        *pos += 1;
                        let val = (ctrl as u32) & 0x1F;
                        if val > 0
                            && let Some(ch) = char::from_u32(val)
                        {
                            out.push(ch);
                        }
                        // \c@ or val==0 → NUL, which is dropped
                    } else {
                        // \c at end of string — output literal \c
                        out.push('\\');
                        out.push('c');
                    }
                }
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
                    // Hex escape: \xNN — if no valid hex digits, output literal \x
                    let before = *pos;
                    let hex = read_hex(chars, pos, 2);
                    if *pos == before {
                        // No hex digits consumed — output literal \x
                        out.push('\\');
                        out.push('x');
                    } else if hex == 0 {
                        // NUL byte truncates the string
                        while *pos < chars.len() && chars[*pos] != '\'' {
                            *pos += 1;
                        }
                        if *pos < chars.len() {
                            *pos += 1;
                        }
                        return out;
                    } else if hex > 0x7F {
                        // High bytes are invalid standalone UTF-8 — replacement char
                        out.push('\u{FFFD}');
                    } else if let Some(ch) = char::from_u32(hex) {
                        // Bash prefixes CTLESC (0x01) and CTLNUL (0x7F) with
                        // CTLESC in its internal representation
                        if ch == '\x01' || ch == '\x7F' {
                            out.push('\x01');
                        }
                        out.push(ch);
                    }
                }
                'u' => {
                    // Unicode: \uNNNN — if no hex digits, output literal \u
                    let before = *pos;
                    let val = read_hex(chars, pos, 4);
                    if *pos == before {
                        out.push('\\');
                        out.push('u');
                    } else if val > 0
                        && let Some(ch) = char::from_u32(val)
                    {
                        out.push(ch);
                    }
                    // val==0 with digits → NUL, truncate
                    else if val == 0 && *pos > before {
                        while *pos < chars.len() && chars[*pos] != '\'' {
                            *pos += 1;
                        }
                        if *pos < chars.len() {
                            *pos += 1;
                        }
                        return out;
                    }
                }
                'U' => {
                    // Unicode long: \UNNNNNNNN — if no hex digits, output literal \U
                    let before = *pos;
                    let val = read_hex(chars, pos, 8);
                    if *pos == before {
                        out.push('\\');
                        out.push('U');
                    } else if val > 0
                        && let Some(ch) = char::from_u32(val)
                    {
                        out.push(ch);
                    }
                    // val==0 with digits → NUL, truncate
                    else if val == 0 && *pos > before {
                        while *pos < chars.len() && chars[*pos] != '\'' {
                            *pos += 1;
                        }
                        if *pos < chars.len() {
                            *pos += 1;
                        }
                        return out;
                    }
                }
                '0'..='7' => {
                    // Octal escape — NUL terminates the string (bash behavior)
                    let mut val = u32::from(esc as u8 - b'0');
                    for _ in 0..2 {
                        if *pos < chars.len() && chars[*pos] >= '0' && chars[*pos] <= '7' {
                            val = val * 8 + u32::from(chars[*pos] as u8 - b'0');
                            *pos += 1;
                        }
                    }
                    if val == 0 {
                        // NUL byte truncates the string in bash
                        // Skip to closing quote
                        while *pos < chars.len() && chars[*pos] != '\'' {
                            *pos += 1;
                        }
                        if *pos < chars.len() {
                            *pos += 1; // skip closing '
                        }
                        return out;
                    }
                    if let Some(ch) = char::from_u32(val) {
                        if ch == '\x01' || ch == '\x7F' {
                            out.push('\x01');
                        }
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

/// Pipelines are right-nested: `(pipe a (pipe b c))`.
fn write_pipeline(f: &mut fmt::Formatter<'_>, commands: &[Node]) -> fmt::Result {
    if commands.len() == 1 {
        return write!(f, "{}", commands[0]);
    }
    // Group commands with their trailing redirects
    let mut groups: Vec<Vec<&Node>> = Vec::new();
    for cmd in commands {
        if matches!(cmd.kind, NodeKind::Redirect { .. }) {
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
fn write_list(f: &mut fmt::Formatter<'_>, items: &[ListItem]) -> fmt::Result {
    if items.len() == 1 && items[0].operator.is_none() {
        return write!(f, "{}", items[0].command);
    }
    let cmds: Vec<&Node> = items.iter().map(|i| &i.command).collect();
    let ops: Vec<ListOperator> = items.iter().filter_map(|i| i.operator).collect();
    write_list_left_assoc(f, &cmds, &ops)
}

fn write_list_left_assoc(
    f: &mut fmt::Formatter<'_>,
    items: &[&Node],
    ops: &[ListOperator],
) -> fmt::Result {
    // Handle trailing unary operator (e.g., "cmd &" → "(background cmd)")
    if items.len() == 1 && ops.len() == 1 {
        let sexp_op = list_op_name(ops[0]);
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
        let sexp_op = list_op_name(ops[ops.len() - 1]);
        write!(f, "({sexp_op} ")?;
        write_list_left_assoc(f, items, &ops[..ops.len() - 1])?;
        return write!(f, ")");
    }

    // Write left-associatively: ((op1 a b) op2 c) op3 d) ...
    for i in (1..ops.len()).rev() {
        write!(f, "({} ", list_op_name(ops[i]))?;
    }
    write!(f, "({} {} {})", list_op_name(ops[0]), items[0], items[1])?;
    for i in 1..ops.len() {
        write!(f, " {})", items[i + 1])?;
    }
    Ok(())
}

const fn list_op_name(op: ListOperator) -> &'static str {
    match op {
        ListOperator::And => "and",
        ListOperator::Or => "or",
        ListOperator::Semi => "semi",
        ListOperator::Background => "background",
    }
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
    if let NodeKind::Word { value, spans, .. } = &target.kind {
        // For fd dup operations (>&, <&), bare digit-only targets are unquoted
        let is_fd_op =
            op.starts_with(">&") || op.starts_with("<&") || op.ends_with("&-") || op.ends_with('&');
        if is_fd_op && !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
            write!(f, "{value})")
        } else if !spans.is_empty() {
            // Use span-based formatting — redirect targets use literal
            // output (no S-expression escaping of quotes)
            write!(f, "\"")?;
            let segments = word::segments_from_spans(value, spans);
            write_redirect_segments(f, &segments)?;
            write!(f, "\")")
        } else {
            // Synthetic nodes (fd strings) — output as-is
            write!(f, "\"{value}\")")
        }
    } else {
        write!(f, "{target})")
    }
}

/// Formats word segments for redirect targets — uses literal output
/// (no S-expression escaping) for text, but processes ANSI-C escapes
/// and reformats command substitutions.
fn write_redirect_segments(
    f: &mut fmt::Formatter<'_>,
    segments: &[word::WordSegment],
) -> fmt::Result {
    for seg in segments {
        match seg {
            word::WordSegment::Literal(text) => write!(f, "{text}")?,
            word::WordSegment::AnsiCQuote(raw) => {
                let chars: Vec<char> = raw.chars().collect();
                let mut pos = 0;
                let processed = process_ansi_c_content(&chars, &mut pos);
                write!(f, "'{processed}'")?;
            }
            word::WordSegment::LocaleString(content) => {
                // $"..." → "..." (content already includes "...")
                write!(f, "{content}")?;
            }
            word::WordSegment::CommandSubstitution(content) => {
                write!(f, "$(")?;
                if let Some(reformatted) = crate::format::reformat_bash(content) {
                    write!(f, "{reformatted}")?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    write!(f, "{normalized}")?;
                }
                write!(f, ")")?;
            }
            word::WordSegment::ProcessSubstitution(dir, content) => {
                write!(f, "{dir}(")?;
                if let Some(reformatted) = crate::format::reformat_bash(content) {
                    write!(f, "{reformatted}")?;
                } else {
                    let normalized = normalize_cmdsub_content(content);
                    write!(f, "{normalized}")?;
                }
                write!(f, ")")?;
            }
            word::WordSegment::ParamExpansion(text)
            | word::WordSegment::SimpleVar(text)
            | word::WordSegment::BraceExpansion(text) => {
                write!(f, "{text}")?;
            }
            word::WordSegment::ArithmeticSub(inner) => {
                // Defensive: redirect target segments come from the sexp
                // filter which excludes `ArithmeticSub`. If it ever does
                // arrive here, reproduce the original `$((...))` text.
                write!(f, "$(({inner}))")?;
            }
        }
    }
    Ok(())
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
