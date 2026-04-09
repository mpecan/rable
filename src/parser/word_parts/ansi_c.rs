//! ANSI-C string (`$'…'`) escape decoding and locale-string (`$"…"`) trimming.

/// Strips the outer pair of double quotes from a locale-string body.
///
/// Locale strings are parsed with the surrounding quotes included (for
/// backwards-compatible S-expression output); `LocaleString.inner` is
/// the same text with that outer pair removed so consumers see just the
/// translatable message.
pub(super) fn strip_locale_quotes(content: &str) -> String {
    content
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .map_or_else(|| content.to_string(), ToString::to_string)
}

/// Decodes ANSI-C quoted content per the bash manual.
///
/// Handles control-char escapes (`\n`, `\t`, etc.), hex/octal/Unicode
/// byte escapes, `\cX` control characters, and backslash-escaped quote
/// / backslash / question mark. Unknown escapes pass through as a
/// backslash followed by the character (matching bash behavior).
pub(super) fn ansi_c_decode(raw: &str) -> String {
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
        'x' => take_radix_escape(chars, start + 1, 16, 2).map(|(ch, n)| (ch, n + 1)),
        'u' => take_radix_escape(chars, start + 1, 16, 4).map(|(ch, n)| (ch, n + 1)),
        'U' => take_radix_escape(chars, start + 1, 16, 8).map(|(ch, n)| (ch, n + 1)),
        'c' => take_control_escape(chars, start + 1).map(|(ch, n)| (ch, n + 1)),
        '0'..='7' => take_radix_escape(chars, start, 8, 3),
        _ => None,
    }
}

/// Reads up to `max_digits` digits in `radix` starting at `start`.
/// Used for both hex (`\xHH` / `\uHHHH` / `\UHHHHHHHH`) and octal (`\NNN`)
/// escapes.
fn take_radix_escape(
    chars: &[char],
    start: usize,
    radix: u32,
    max_digits: usize,
) -> Option<(char, usize)> {
    let mut value: u32 = 0;
    let mut consumed = 0;
    while consumed < max_digits {
        let Some(c) = chars.get(start + consumed) else {
            break;
        };
        let Some(digit) = c.to_digit(radix) else {
            break;
        };
        value = value * radix + digit;
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
