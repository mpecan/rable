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
#[allow(clippy::too_many_lines)]
pub fn process_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
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
                        skip_to_closing_quote(chars, pos);
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
                        skip_to_closing_quote(chars, pos);
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
                        skip_to_closing_quote(chars, pos);
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
                        skip_to_closing_quote(chars, pos);
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
