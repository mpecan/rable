//! Decomposes word values into structured AST parts.
//!
//! Reuses the segment parser from `sexp::word` to split a word's raw
//! value into typed segments, then maps each segment to an AST `Node`.

use std::cell::Cell;

use crate::ast::{ListItem, Node, NodeKind, Span};
use crate::lexer::word_builder::WordSpan;
use crate::sexp::word::{WordSegment, segments_with_params};

thread_local! {
    static DECOMPOSE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard to prevent infinite recursion when decomposing nested
/// command/process substitutions.
///
/// Depth limit of 2 matches `format::DepthGuard` — allows `$(a $(b))`
/// but stops before unbounded nesting. The counter is separate from
/// the format module's counter since decomposition and formatting
/// are independent recursion paths.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        DECOMPOSE_DEPTH.with(|d| {
            let v = d.get();
            if v >= 2 {
                return None;
            }
            d.set(v + 1);
            Some(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DECOMPOSE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Decomposes a word using lexer spans — the primary path for all
/// token-derived words.
pub(super) fn decompose_word_with_spans(value: &str, spans: &[WordSpan]) -> Vec<Node> {
    let segments = segments_with_params(value, spans);
    segments.into_iter().map(segment_to_node).collect()
}

/// Creates a single `WordLiteral` for synthetic values (no lexer token).
/// Used for fd numbers (`"0"`, `"1"`), synthetic `"$@"`, etc.
pub(super) fn decompose_word_literal(value: &str) -> Vec<Node> {
    vec![Node::empty(NodeKind::WordLiteral {
        value: value.to_string(),
    })]
}

fn segment_to_node(seg: WordSegment) -> Node {
    match seg {
        WordSegment::Literal(text) => Node::empty(NodeKind::WordLiteral { value: text }),
        WordSegment::AnsiCQuote(content) => {
            let decoded = ansi_c_decode(&content);
            Node::empty(NodeKind::AnsiCQuote { content, decoded })
        }
        WordSegment::LocaleString(content) => {
            let inner = strip_locale_quotes(&content);
            Node::empty(NodeKind::LocaleString { content, inner })
        }
        WordSegment::ArithmeticSub(inner) => arithmetic_sub_to_node(&inner),
        WordSegment::CommandSubstitution(content) => cmdsub_to_node(&content),
        WordSegment::ProcessSubstitution(direction, content) => {
            procsub_to_node(direction, &content)
        }
        WordSegment::SimpleVar(text) => parse_simple_var(&text),
        WordSegment::ParamExpansion(text) => parse_braced_param(&text),
        WordSegment::BraceExpansion(text) => {
            Node::empty(NodeKind::BraceExpansion { content: text })
        }
    }
}

/// Strips the outer pair of double quotes from a locale-string body.
///
/// Locale strings are parsed with the surrounding quotes included (for
/// backwards-compatible S-expression output); `LocaleString.inner` is
/// the same text with that outer pair removed so consumers see just the
/// translatable message.
fn strip_locale_quotes(content: &str) -> String {
    content
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .map_or_else(|| content.to_string(), ToString::to_string)
}

/// Decomposes the inner text of a `$((...))` arithmetic substitution
/// into a typed `ArithmeticExpansion` node. On parse failure the
/// `expression` field is left as `None`, matching the existing best-effort
/// semantics used elsewhere in word decomposition.
fn arithmetic_sub_to_node(inner: &str) -> Node {
    let expression = crate::parser::arithmetic::parse_arith_expression(inner)
        .ok()
        .map(Box::new);
    Node::empty(NodeKind::ArithmeticExpansion { expression })
}

/// Decodes ANSI-C quoted content per the bash manual.
///
/// Handles control-char escapes (`\n`, `\t`, etc.), hex/octal/Unicode
/// byte escapes, `\cX` control characters, and backslash-escaped quote
/// / backslash / question mark. Unknown escapes pass through as a
/// backslash followed by the character (matching bash behavior).
fn ansi_c_decode(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let next = chars[i + 1];
        if let Some(simple) = decode_simple_escape(next) {
            out.push(simple);
            i += 2;
            continue;
        }
        if let Some((ch, consumed)) = decode_numeric_escape(&chars, i + 1) {
            out.push(ch);
            i += 1 + consumed;
            continue;
        }
        // Unknown escape — pass through as `\X`.
        out.push('\\');
        out.push(next);
        i += 2;
    }
    out
}

/// Handles `\a \b \e \E \f \n \r \t \v \\ \' \" \?` per the bash(1) manual.
const fn decode_simple_escape(next: char) -> Option<char> {
    Some(match next {
        'a' => '\u{07}',
        'b' => '\u{08}',
        'e' | 'E' => '\u{1B}',
        'f' => '\u{0C}',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'v' => '\u{0B}',
        '\\' => '\\',
        '\'' => '\'',
        '"' => '"',
        '?' => '?',
        _ => return None,
    })
}

/// Numeric / control-character escapes: `\NNN`, `\xHH`, `\uHHHH`,
/// `\UHHHHHHHH`, `\cX`. Returns the decoded character and the number
/// of characters consumed *after* the leading backslash.
fn decode_numeric_escape(chars: &[char], start: usize) -> Option<(char, usize)> {
    let first = *chars.get(start)?;
    match first {
        'x' => take_hex_escape(chars, start + 1, 2).map(|(ch, n)| (ch, n + 1)),
        'u' => take_hex_escape(chars, start + 1, 4).map(|(ch, n)| (ch, n + 1)),
        'U' => take_hex_escape(chars, start + 1, 8).map(|(ch, n)| (ch, n + 1)),
        'c' => take_control_escape(chars, start + 1).map(|(ch, n)| (ch, n + 1)),
        '0'..='7' => take_octal_escape(chars, start),
        _ => None,
    }
}

fn take_hex_escape(chars: &[char], start: usize, max: usize) -> Option<(char, usize)> {
    let mut value: u32 = 0;
    let mut consumed = 0;
    while consumed < max {
        let Some(c) = chars.get(start + consumed) else {
            break;
        };
        let Some(digit) = c.to_digit(16) else { break };
        value = value * 16 + digit;
        consumed += 1;
    }
    if consumed == 0 {
        return None;
    }
    let ch = char::from_u32(value)?;
    Some((ch, consumed))
}

fn take_octal_escape(chars: &[char], start: usize) -> Option<(char, usize)> {
    let mut value: u32 = 0;
    let mut consumed = 0;
    while consumed < 3 {
        let Some(c) = chars.get(start + consumed) else {
            break;
        };
        let Some(digit) = c.to_digit(8) else { break };
        value = value * 8 + digit;
        consumed += 1;
    }
    if consumed == 0 {
        return None;
    }
    let ch = char::from_u32(value)?;
    Some((ch, consumed))
}

fn take_control_escape(chars: &[char], start: usize) -> Option<(char, usize)> {
    let c = *chars.get(start)?;
    if !c.is_ascii() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let byte = (c as u32) & 0x1F;
    let ch = char::from_u32(byte)?;
    Some((ch, 1))
}

fn cmdsub_to_node(content: &str) -> Node {
    parse_sub(content, &format!("$({content})"), |cmd| {
        // brace: false — this is $(...) syntax, not ${...} brace expansion
        NodeKind::CommandSubstitution {
            command: cmd,
            brace: false,
        }
    })
}

fn procsub_to_node(direction: char, content: &str) -> Node {
    parse_sub(content, &format!("{direction}({content})"), |cmd| {
        NodeKind::ProcessSubstitution {
            direction: direction.to_string(),
            command: cmd,
        }
    })
}

/// Shared logic for parsing command/process substitution content.
fn parse_sub(
    content: &str,
    fallback_value: &str,
    make_kind: impl FnOnce(Box<Node>) -> NodeKind,
) -> Node {
    let Some(_guard) = DepthGuard::enter() else {
        return literal_fallback(fallback_value);
    };
    crate::parse(content, false).map_or_else(
        |_| literal_fallback(fallback_value),
        |nodes| Node::empty(make_kind(Box::new(wrap_nodes(nodes)))),
    )
}

fn literal_fallback(value: &str) -> Node {
    Node::empty(NodeKind::WordLiteral {
        value: value.to_string(),
    })
}

/// Wraps a list of parsed nodes into a single node.
fn wrap_nodes(mut nodes: Vec<Node>) -> Node {
    match nodes.len() {
        0 => Node::empty(NodeKind::Empty),
        1 => nodes.swap_remove(0),
        _ => Node::new(
            NodeKind::List {
                items: nodes
                    .into_iter()
                    .map(|n| ListItem {
                        command: n,
                        operator: None,
                    })
                    .collect(),
            },
            Span::empty(),
        ),
    }
}

/// Parses `$var` — strip leading `$`, remainder is the param name.
fn parse_simple_var(text: &str) -> Node {
    let param = text.get(1..).unwrap_or("");
    Node::empty(NodeKind::ParamExpansion {
        param: param.to_string(),
        op: None,
        arg: None,
    })
}

/// Parses `${...}` — strip `${` and `}`, dispatch on inner content.
fn parse_braced_param(text: &str) -> Node {
    let inner = match text.get(2..text.len().saturating_sub(1)) {
        Some(s) if !s.is_empty() => s,
        _ => return literal_fallback(text),
    };
    parse_param_inner(inner)
}

/// Dispatches based on the first character of the inner `${...}` content.
fn parse_param_inner(inner: &str) -> Node {
    let first = inner.as_bytes().first().copied();
    match first {
        // ${#...} — length prefix, unless inner is just "#" (special param)
        Some(b'#') if inner.len() > 1 => parse_length_prefix(&inner[1..]),
        // ${!...} — indirect prefix, unless inner is just "!" (special param)
        Some(b'!') if inner.len() > 1 => parse_indirect_prefix(&inner[1..]),
        _ => parse_plain_param(inner),
    }
}

/// Parses `${#var}` — the content after `#` is the param name.
fn parse_length_prefix(after_hash: &str) -> Node {
    let (param, _) = split_param_and_rest(after_hash);
    Node::empty(NodeKind::ParamLength {
        param: param.to_string(),
    })
}

/// Parses `${!var}` or `${!var:-default}`.
fn parse_indirect_prefix(after_bang: &str) -> Node {
    let (param, op, arg) = extract_param_op_arg(after_bang);
    Node::empty(NodeKind::ParamIndirect { param, op, arg })
}

/// Parses `${var}` or `${var:-default}`.
fn parse_plain_param(inner: &str) -> Node {
    let (param, op, arg) = extract_param_op_arg(inner);
    Node::empty(NodeKind::ParamExpansion { param, op, arg })
}

/// Shared extraction: splits param name, operator, and argument.
fn extract_param_op_arg(s: &str) -> (String, Option<String>, Option<String>) {
    let (param, rest) = split_param_and_rest(s);
    let (op, arg) = parse_op_and_arg(rest);
    (param.to_string(), op, arg)
}

/// Splits a parameter name from the remainder. Handles identifiers,
/// special single-char params, digits, and array subscripts.
fn split_param_and_rest(s: &str) -> (&str, &str) {
    if s.is_empty() {
        return ("", "");
    }
    let bytes = s.as_bytes();
    // Special single-char params: @, *, ?, -, $, !, #
    if matches!(bytes[0], b'@' | b'*' | b'?' | b'-' | b'$' | b'!' | b'#') {
        return (&s[..1], &s[1..]);
    }
    // Digit-only positional params (e.g., "0", "10", "100")
    if bytes[0].is_ascii_digit() {
        let end = bytes
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(s.len());
        return maybe_with_subscript(s, end);
    }
    // Identifier: [a-zA-Z_][a-zA-Z0-9_]*
    if bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' {
        let end = bytes
            .iter()
            .position(|b| !b.is_ascii_alphanumeric() && *b != b'_')
            .unwrap_or(s.len());
        return maybe_with_subscript(s, end);
    }
    // Unknown — treat entire string as param
    (s, "")
}

/// If the character at `name_end` is `[`, extend to include the subscript.
fn maybe_with_subscript(s: &str, name_end: usize) -> (&str, &str) {
    if s.as_bytes().get(name_end) == Some(&b'[')
        && let Some(bracket_len) = find_matching_bracket(&s[name_end..])
    {
        let end = name_end + bracket_len + 1;
        return (&s[..end], &s[end..]);
    }
    (&s[..name_end], &s[name_end..])
}

/// Finds the matching `]` for a `[` at position 0, returning the index
/// of `]` relative to the input. Handles nested brackets.
fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Known parameter expansion operators, longest-match-first.
const OPERATORS: &[&str] = &[
    ":-", ":=", ":+", ":?", // colon-prefixed defaults
    "-", "=", "+", "?", // plain defaults
    "##", "#", // prefix removal
    "%%", "%", // suffix removal
    "//", "/#", "/%", "/", // substitution
    "^^", "^", // uppercase
    ",,", ",", // lowercase
    "@", // transformation
    ":", // substring
];

/// Extracts operator and argument from the remainder after the param name.
fn parse_op_and_arg(s: &str) -> (Option<String>, Option<String>) {
    if s.is_empty() {
        return (None, None);
    }
    for op in OPERATORS {
        if let Some(arg) = s.strip_prefix(op) {
            let arg_opt = if arg.is_empty() {
                None
            } else {
                Some(arg.to_string())
            };
            return (Some((*op).to_string()), arg_opt);
        }
    }
    // Unknown operator — treat entire remainder as op
    (Some(s.to_string()), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: lex a single word and decompose it using the real span path.
    #[allow(clippy::unwrap_used)]
    fn decompose(source: &str) -> Vec<Node> {
        let mut lexer = crate::lexer::Lexer::new(source, false);
        let tok = lexer.next_token().unwrap();
        decompose_word_with_spans(&tok.value, &tok.spans)
    }

    #[test]
    fn plain_word() {
        let parts = decompose("echo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "echo"
        ));
    }

    #[test]
    fn simple_variable_expansion() {
        let parts = decompose("$foo");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "foo" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn braced_variable_expansion() {
        let parts = decompose("${foo}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "foo" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn param_with_default() {
        let parts = decompose("${foo:-default}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "foo"
                && op.as_deref() == Some(":-")
                && arg.as_deref() == Some("default")
        ));
    }

    #[test]
    fn param_length() {
        let parts = decompose("${#foo}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamLength { param } if param == "foo"
        ));
    }

    #[test]
    fn param_indirect() {
        let parts = decompose("${!foo}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamIndirect { param, op, arg }
            if param == "foo" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn special_param_question_mark() {
        let parts = decompose("$?");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "?" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn positional_param() {
        let parts = decompose("$1");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "1" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn multi_digit_positional() {
        let parts = decompose("${10}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "10" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn prefix_removal_operator() {
        let parts = decompose("${foo##pattern}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "foo"
                && op.as_deref() == Some("##")
                && arg.as_deref() == Some("pattern")
        ));
    }

    #[test]
    fn special_param_hash_braced() {
        // ${#} is the special param "#", NOT ParamLength
        let parts = decompose("${#}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "#" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn mixed_text_and_param() {
        let parts = decompose("hello${world}end");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "hello"
        ));
        assert!(matches!(
            &parts[1].kind,
            NodeKind::ParamExpansion { param, .. } if param == "world"
        ));
        assert!(matches!(
            &parts[2].kind,
            NodeKind::WordLiteral { value } if value == "end"
        ));
    }

    #[test]
    fn array_subscript() {
        let parts = decompose("${arr[@]}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamExpansion { param, op, arg }
            if param == "arr[@]" && op.is_none() && arg.is_none()
        ));
    }

    #[test]
    fn indirect_with_operator() {
        let parts = decompose("${!foo:-bar}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ParamIndirect { param, op, arg }
            if param == "foo"
                && op.as_deref() == Some(":-")
                && arg.as_deref() == Some("bar")
        ));
    }

    #[test]
    fn command_substitution() {
        let parts = decompose("$(date)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::CommandSubstitution { brace: false, .. }
        ));
    }

    #[test]
    fn mixed_segments() {
        let parts = decompose("hello$(world)end");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "hello"
        ));
        assert!(matches!(
            &parts[1].kind,
            NodeKind::CommandSubstitution { .. }
        ));
        assert!(matches!(
            &parts[2].kind,
            NodeKind::WordLiteral { value } if value == "end"
        ));
    }

    #[test]
    fn ansi_c_quote() {
        let parts = decompose("$'foo\\nbar'");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { content, .. } if content == "foo\\nbar"
        ));
    }

    #[test]
    fn process_substitution() {
        let parts = decompose("<(cmd)");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ProcessSubstitution { direction, .. }
            if direction == "<"
        ));
    }

    #[test]
    fn locale_string() {
        let parts = decompose("$\"hello\"");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { content, .. } if content == "\"hello\""
        ));
    }

    #[test]
    fn brace_expansion_comma() {
        let parts = decompose("{a,b,c}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::BraceExpansion { content } if content == "{a,b,c}"
        ));
    }

    #[test]
    fn brace_expansion_range() {
        let parts = decompose("{1..10}");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::BraceExpansion { content } if content == "{1..10}"
        ));
    }

    #[test]
    fn brace_expansion_mid_word() {
        let parts = decompose("file{1,2}.txt");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "file"
        ));
        assert!(matches!(
            &parts[1].kind,
            NodeKind::BraceExpansion { content } if content == "{1,2}"
        ));
        assert!(matches!(
            &parts[2].kind,
            NodeKind::WordLiteral { value } if value == ".txt"
        ));
    }

    #[test]
    fn arithmetic_expansion_decomposed() {
        let parts = decompose("$((1+2))");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ArithmeticExpansion {
                expression: Some(_)
            }
        ));
    }

    #[test]
    fn arithmetic_with_variable() {
        let parts = decompose("$((x*2))");
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::ArithmeticExpansion { .. }
        ));
    }

    #[test]
    fn arithmetic_in_mixed_word() {
        let parts = decompose("file_$((n+1)).txt");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0].kind,
            NodeKind::WordLiteral { value } if value == "file_"
        ));
        assert!(matches!(
            &parts[1].kind,
            NodeKind::ArithmeticExpansion {
                expression: Some(_)
            }
        ));
        assert!(matches!(
            &parts[2].kind,
            NodeKind::WordLiteral { value } if value == ".txt"
        ));
    }

    #[test]
    fn arithmetic_expression_tree_shape() {
        // `1+2` should parse as `(+ 1 2)` — verifies we're actually
        // producing a typed sub-AST, not an opaque string blob.
        let parts = decompose("$((1+2))");
        let NodeKind::ArithmeticExpansion {
            expression: Some(expr),
        } = &parts[0].kind
        else {
            unreachable!("expected parsed arithmetic expression");
        };
        let NodeKind::ArithBinaryOp { op, left, right } = &expr.kind else {
            unreachable!("expected binop, got {:?}", expr.kind);
        };
        assert_eq!(op, "+");
        assert!(matches!(&left.kind, NodeKind::ArithNumber { value } if value == "1"));
        assert!(matches!(&right.kind, NodeKind::ArithNumber { value } if value == "2"));
    }

    #[test]
    fn ansi_c_decodes_hex() {
        let parts = decompose("$'\\x41'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
        ));
    }

    #[test]
    fn ansi_c_decodes_newline() {
        let parts = decompose("$'line1\\nline2'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "line1\nline2"
        ));
    }

    #[test]
    fn ansi_c_decodes_octal() {
        // octal 101 = 65 = 'A'
        let parts = decompose("$'\\101'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
        ));
    }

    #[test]
    fn ansi_c_decodes_unicode() {
        let parts = decompose("$'\\u0041'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "A"
        ));
    }

    #[test]
    fn ansi_c_decodes_control_char() {
        // Ctrl-A = 0x01
        let parts = decompose("$'\\cA'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "\u{01}"
        ));
    }

    #[test]
    fn ansi_c_unknown_escape_passthrough() {
        let parts = decompose("$'\\z'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { decoded, .. } if decoded == "\\z"
        ));
    }

    #[test]
    fn ansi_c_preserves_raw_content() {
        // Legacy `content` field must remain byte-identical so the
        // Parable corpus keeps passing.
        let parts = decompose("$'foo\\nbar'");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::AnsiCQuote { content, .. } if content == "foo\\nbar"
        ));
    }

    #[test]
    fn locale_string_strips_quotes() {
        let parts = decompose("$\"hello\"");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { inner, .. } if inner == "hello"
        ));
    }

    #[test]
    fn locale_string_empty() {
        let parts = decompose("$\"\"");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { inner, .. } if inner.is_empty()
        ));
    }

    #[test]
    fn locale_string_preserves_raw_content() {
        // Legacy `content` field keeps the surrounding quotes so the
        // Parable corpus keeps passing.
        let parts = decompose("$\"hello\"");
        assert!(matches!(
            &parts[0].kind,
            NodeKind::LocaleString { content, .. } if content == "\"hello\""
        ));
    }
}
