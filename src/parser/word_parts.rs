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
        WordSegment::AnsiCQuote(content) => Node::empty(NodeKind::AnsiCQuote { content }),
        WordSegment::LocaleString(content) => Node::empty(NodeKind::LocaleString { content }),
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
            NodeKind::AnsiCQuote { content } if content == "foo\\nbar"
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
            NodeKind::LocaleString { content } if content == "\"hello\""
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
}
