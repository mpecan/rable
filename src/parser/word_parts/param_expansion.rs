//! `$var`, `${var}`, `${#var}`, `${!var}`, and their operators.

use crate::ast::{Node, NodeKind};

use super::literal_fallback;

/// Parses `$var` — strip leading `$`, remainder is the param name.
pub(super) fn parse_simple_var(text: &str) -> Node {
    let param = text.get(1..).unwrap_or("");
    Node::empty(NodeKind::ParamExpansion {
        param: param.to_string(),
        op: None,
        arg: None,
    })
}

/// Parses `${…}` — strip `${` and `}`, dispatch on inner prefix (`#`, `!`,
/// or plain identifier) and build the appropriate expansion node.
pub(super) fn parse_braced_param(text: &str) -> Node {
    let inner = match text.get(2..text.len().saturating_sub(1)) {
        Some(s) if !s.is_empty() => s,
        _ => return literal_fallback(text),
    };
    let first = inner.as_bytes().first().copied();
    match first {
        // ${#var} — length prefix, unless inner is just "#" (special param)
        Some(b'#') if inner.len() > 1 => {
            let (param, _) = split_param_and_rest(&inner[1..]);
            Node::empty(NodeKind::ParamLength {
                param: param.to_string(),
            })
        }
        // ${!var} — indirect prefix, unless inner is just "!" (special param)
        Some(b'!') if inner.len() > 1 => {
            let (param, op, arg) = extract_param_op_arg(&inner[1..]);
            Node::empty(NodeKind::ParamIndirect { param, op, arg })
        }
        // ${var}[...] or ${var:op:arg}
        _ => {
            let (param, op, arg) = extract_param_op_arg(inner);
            Node::empty(NodeKind::ParamExpansion { param, op, arg })
        }
    }
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
