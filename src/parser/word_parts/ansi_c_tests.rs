//! ANSI-C quoting (`$'…'`) and locale-string (`$"…"`) decoding tests.
//!
//! Split from `tests.rs` to keep both files under the project's file-length
//! limit; the escape table alone accounts for most of these cases.

use crate::ast::{Node, NodeKind};

use super::decompose_word_with_spans;

/// Lexes a single word and decomposes it using the real span path.
#[allow(clippy::unwrap_used)]
fn decompose(source: &str) -> Vec<Node> {
    let mut lexer = crate::lexer::Lexer::new(source, false);
    let tok = lexer.next_token().unwrap();
    decompose_word_with_spans(&tok.value, &tok.spans)
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

/// Decodes an ANSI-C body through the real lexer path and returns the
/// `decoded` field.
#[allow(clippy::expect_used)]
fn ansi_decoded(source: &str) -> String {
    let parts = decompose(source);
    let NodeKind::AnsiCQuote { decoded, .. } = &parts[0].kind else {
        unreachable!(
            "expected AnsiCQuote for {source:?}, got {:?}",
            parts[0].kind
        );
    };
    decoded.clone()
}

#[test]
fn ansi_c_decodes_every_simple_escape() {
    // Each arm of `decode_simple_escape` per bash(1). Without a case per
    // arm, individual escapes silently degrade to `\X` passthrough.
    for (source, expected) in [
        ("$'\\a'", "\u{07}"),
        ("$'\\b'", "\u{08}"),
        ("$'\\e'", "\u{1B}"),
        ("$'\\E'", "\u{1B}"),
        ("$'\\f'", "\u{0C}"),
        ("$'\\n'", "\n"),
        ("$'\\r'", "\r"),
        ("$'\\t'", "\t"),
        ("$'\\v'", "\u{0B}"),
        ("$'\\\\'", "\\"),
        ("$'\\\"'", "\""),
        ("$'\\?'", "?"),
    ] {
        assert_eq!(ansi_decoded(source), expected, "for input {source:?}");
    }
}

#[test]
fn ansi_c_decodes_escaped_single_quote() {
    // `\'` is its own arm and needs the surrounding quotes to survive
    // lexing, so it is checked apart from the table above.
    assert_eq!(ansi_decoded("$'a\\'b'"), "a'b");
}

#[test]
fn ansi_c_decodes_long_unicode_escape() {
    // `\U` takes up to 8 hex digits, distinct from `\u`'s 4.
    assert_eq!(ansi_decoded("$'\\U0001F600'"), "\u{1F600}");
}

#[test]
fn ansi_c_escape_consumes_exactly_its_digits() {
    // The consumed-character count must be exact: too few and leftover
    // digits leak into the output, too many and following text is eaten.
    assert_eq!(ansi_decoded("$'\\U0041x'"), "Ax");
    assert_eq!(ansi_decoded("$'\\u0041x'"), "Ax");
    // `\x` caps at 2 digits and `\NNN` at 3, so the trailing digit is literal.
    assert_eq!(ansi_decoded("$'\\x414'"), "A4");
    assert_eq!(ansi_decoded("$'\\1011'"), "A1");
}

#[test]
fn ansi_c_decode_handles_trailing_backslash() {
    // A body ending in a lone backslash has no following character to
    // inspect; the bounds check must stop rather than index past the end.
    // Exercised directly because the lexer cannot emit an unterminated
    // escape through `$'…'`.
    assert_eq!(super::ansi_c::ansi_c_decode("a\\"), "a\\");
    assert_eq!(super::ansi_c::ansi_c_decode("\\"), "\\");
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
