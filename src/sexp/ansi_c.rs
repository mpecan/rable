//! ANSI-C quoting (`$'…'`) escape sequence processing used when emitting
//! S-expressions and when the reformatter rebuilds bash source.
//!
//! Distinct from `parser::word_parts::ansi_c` — the parser copy decodes
//! escapes into the final `AnsiCQuote.decoded` field, whereas this copy is
//! the legacy character-cursor walker used by `sexp::word`, `sexp::redirects`,
//! and `format::mod` which all operate on `&[char]` input with a shared
//! position cursor.

/// Process ANSI-C escape sequences inside `$'...'`.
/// `chars` is the full character array, `pos` points to the first char after `$'`.
/// Returns the processed content (without surrounding quotes).
/// Advances `pos` past the closing `'`.
pub fn process_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
    let mut out = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '\'' {
            *pos += 1;
            return out;
        }
        if c != '\\' || *pos + 1 >= chars.len() {
            out.push(c);
            *pos += 1;
            continue;
        }
        *pos += 1;
        let esc = chars[*pos];
        *pos += 1;
        if handle_escape(chars, pos, esc, &mut out) {
            // NUL-valued escape truncated the string; skip_to_closing_quote
            // was already called by the specific handler.
            return out;
        }
    }
    out
}

/// Dispatch a single escape sequence. Returns `true` when the sequence
/// NUL-truncated the quoted string so the caller can exit the loop.
fn handle_escape(chars: &[char], pos: &mut usize, esc: char, out: &mut String) -> bool {
    if let Some(ch) = simple_escape(esc) {
        out.push(ch);
        return false;
    }
    match esc {
        'c' => handle_control(chars, pos, out),
        '\'' => push_escaped_quote(out),
        'x' => return handle_hex(chars, pos, out),
        'u' => return handle_unicode(chars, pos, out, 4),
        'U' => return handle_unicode(chars, pos, out, 8),
        '0'..='7' => return handle_octal(chars, pos, esc, out),
        _ => {
            out.push('\\');
            out.push(esc);
        }
    }
    false
}

/// Table of single-character escapes. Returns `None` for escapes that
/// need context (hex/unicode/octal/control) or are unrecognised.
const fn simple_escape(esc: char) -> Option<char> {
    Some(match esc {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'a' => '\x07',
        'b' => '\x08',
        'f' => '\x0C',
        'v' => '\x0B',
        'e' | 'E' => '\x1B',
        '\\' => '\\',
        '"' => '"',
        _ => return None,
    })
}

/// `\xNN` — up to 2 hex digits. Returns `true` on NUL truncation.
fn handle_hex(chars: &[char], pos: &mut usize, out: &mut String) -> bool {
    let before = *pos;
    let hex = read_hex(chars, pos, 2);
    if *pos == before {
        out.push('\\');
        out.push('x');
        return false;
    }
    if hex == 0 {
        skip_to_closing_quote(chars, pos);
        return true;
    }
    if hex > 0x7F {
        // High bytes are invalid standalone UTF-8 — replacement char.
        out.push('\u{FFFD}');
    } else if let Some(ch) = char::from_u32(hex) {
        push_with_ctlesc(out, ch);
    }
    false
}

/// `\uNNNN` (width=4) or `\UNNNNNNNN` (width=8). Returns `true` on NUL
/// truncation.
fn handle_unicode(chars: &[char], pos: &mut usize, out: &mut String, width: usize) -> bool {
    let before = *pos;
    let val = read_hex(chars, pos, width);
    if *pos == before {
        out.push('\\');
        out.push(if width == 4 { 'u' } else { 'U' });
        return false;
    }
    if val == 0 {
        skip_to_closing_quote(chars, pos);
        return true;
    }
    if let Some(ch) = char::from_u32(val) {
        out.push(ch);
    }
    false
}

/// `\0`–`\7` followed by up to 2 additional octal digits. Returns `true`
/// on NUL truncation.
fn handle_octal(chars: &[char], pos: &mut usize, first: char, out: &mut String) -> bool {
    let mut val = u32::from(first as u8 - b'0');
    for _ in 0..2 {
        if *pos < chars.len() && chars[*pos] >= '0' && chars[*pos] <= '7' {
            val = val * 8 + u32::from(chars[*pos] as u8 - b'0');
            *pos += 1;
        }
    }
    if val == 0 {
        skip_to_closing_quote(chars, pos);
        return true;
    }
    if let Some(ch) = char::from_u32(val) {
        push_with_ctlesc(out, ch);
    }
    false
}

/// `\cX` — emits `chr(X & 0x1F)`. `\c@` (value 0) is silently dropped,
/// matching the existing behavior (not a NUL-truncation case).
fn handle_control(chars: &[char], pos: &mut usize, out: &mut String) {
    if *pos >= chars.len() {
        // `\c` at end of input — output literal backslash + c.
        out.push('\\');
        out.push('c');
        return;
    }
    let ctrl = chars[*pos];
    *pos += 1;
    let val = (ctrl as u32) & 0x1F;
    if val > 0
        && let Some(ch) = char::from_u32(val)
    {
        out.push(ch);
    }
}

/// Pushes `ch` to `out`, prefixing `0x01` (CTLESC) for bytes that bash
/// escapes internally (`0x01` and `0x7F`).
fn push_with_ctlesc(out: &mut String, ch: char) {
    if ch == '\x01' || ch == '\x7F' {
        out.push('\x01');
    }
    out.push(ch);
}

/// `\'` inside `$'...'` expands to the 4-character sequence `'\''` — close
/// the current quote, escape a single quote, reopen. The outer loop
/// continues from the next character; no recursive re-entry needed.
fn push_escaped_quote(out: &mut String) {
    out.push('\'');
    out.push('\\');
    out.push('\'');
    out.push('\'');
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

/// Advances `pos` to just past the closing `'` — used when a NUL-valued
/// escape truncates the quoted string per bash semantics.
fn skip_to_closing_quote(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos] != '\'' {
        *pos += 1;
    }
    if *pos < chars.len() {
        *pos += 1;
    }
}
