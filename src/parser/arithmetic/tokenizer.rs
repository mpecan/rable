//! Lexer for arithmetic expressions. Turns the inner text of `$((…))` /
//! `((…))` into a flat `Vec<Tok>` for the parser.

use crate::error::Result;

use super::err;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tok {
    Number(String),
    Ident(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Question,
    Colon,
    Comma,
    Bang,
    Tilde,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Power,
    Shl,
    Shr,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    Ne,
    Amp,
    Caret,
    Pipe,
    AmpAmp,
    PipePipe,
    Inc,
    Dec,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ShlAssign,
    ShrAssign,
    AndAssign,
    XorAssign,
    OrAssign,
}

pub(super) fn tokenize(source: &str) -> Result<Vec<Tok>> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let (tok, next) = if b.is_ascii_digit() {
            tokenize_number(source, i)?
        } else if b.is_ascii_alphabetic() || b == b'_' {
            tokenize_ident(source, i)?
        } else if b == b'$' {
            tokenize_dollar(source, i)?
        } else {
            tokenize_operator(source, i)?
        };
        tokens.push(tok);
        i = next;
    }
    Ok(tokens)
}

fn tokenize_number(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    // Hex: 0x... / 0X...
    if bytes[start] == b'0'
        && start + 1 < bytes.len()
        && (bytes[start + 1] == b'x' || bytes[start + 1] == b'X')
    {
        let mut end = start + 2;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        return slice_number(source, start, end);
    }
    // Decimal (possibly followed by base-N marker `#`)
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'#' {
        end += 1;
        while end < bytes.len() && is_base_digit(bytes[end]) {
            end += 1;
        }
    }
    slice_number(source, start, end)
}

const fn is_base_digit(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'@'
}

fn slice_number(source: &str, start: usize, end: usize) -> Result<(Tok, usize)> {
    let value = source
        .get(start..end)
        .ok_or_else(|| err("invalid number literal"))?;
    Ok((Tok::Number(value.to_string()), end))
}

fn tokenize_ident(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let name = source
        .get(start..end)
        .ok_or_else(|| err("invalid identifier"))?;
    Ok((Tok::Ident(name.to_string()), end))
}

/// `$name` inside an arithmetic expression is equivalent to `name`.
/// Other `$`-expansions (`$(...)`, `${...}`, `$((...))`) are not supported
/// by this lightweight parser — the caller will fall back to `None`.
fn tokenize_dollar(source: &str, start: usize) -> Result<(Tok, usize)> {
    let bytes = source.as_bytes();
    let after = start + 1;
    if after >= bytes.len() {
        return Err(err("trailing '$' in arithmetic expression"));
    }
    if bytes[after].is_ascii_alphabetic() || bytes[after] == b'_' {
        return tokenize_ident(source, after);
    }
    Err(err("unsupported $-expansion in arithmetic expression"))
}

fn tokenize_operator(source: &str, start: usize) -> Result<(Tok, usize)> {
    let rest = source
        .get(start..)
        .ok_or_else(|| err("unexpected end of input"))?;
    let bytes = rest.as_bytes();
    if bytes.len() >= 3
        && let Some(t) = match_three(&bytes[..3])
    {
        return Ok((t, start + 3));
    }
    if bytes.len() >= 2
        && let Some(t) = match_two(&bytes[..2])
    {
        return Ok((t, start + 2));
    }
    if let Some(t) = match_one(bytes[0]) {
        return Ok((t, start + 1));
    }
    Err(err(format!(
        "unexpected character '{}' in arithmetic expression",
        bytes[0] as char
    )))
}

fn match_three(pair: &[u8]) -> Option<Tok> {
    Some(match pair {
        b"<<=" => Tok::ShlAssign,
        b">>=" => Tok::ShrAssign,
        _ => return None,
    })
}

fn match_two(pair: &[u8]) -> Option<Tok> {
    Some(match pair {
        b"**" => Tok::Power,
        b"<<" => Tok::Shl,
        b">>" => Tok::Shr,
        b"<=" => Tok::Le,
        b">=" => Tok::Ge,
        b"==" => Tok::EqEq,
        b"!=" => Tok::Ne,
        b"&&" => Tok::AmpAmp,
        b"||" => Tok::PipePipe,
        b"++" => Tok::Inc,
        b"--" => Tok::Dec,
        b"+=" => Tok::AddAssign,
        b"-=" => Tok::SubAssign,
        b"*=" => Tok::MulAssign,
        b"/=" => Tok::DivAssign,
        b"%=" => Tok::ModAssign,
        b"&=" => Tok::AndAssign,
        b"^=" => Tok::XorAssign,
        b"|=" => Tok::OrAssign,
        _ => return None,
    })
}

const fn match_one(c: u8) -> Option<Tok> {
    Some(match c {
        b'+' => Tok::Plus,
        b'-' => Tok::Minus,
        b'*' => Tok::Star,
        b'/' => Tok::Slash,
        b'%' => Tok::Percent,
        b'(' => Tok::LParen,
        b')' => Tok::RParen,
        b'[' => Tok::LBracket,
        b']' => Tok::RBracket,
        b'?' => Tok::Question,
        b':' => Tok::Colon,
        b',' => Tok::Comma,
        b'!' => Tok::Bang,
        b'~' => Tok::Tilde,
        b'<' => Tok::Lt,
        b'>' => Tok::Gt,
        b'&' => Tok::Amp,
        b'^' => Tok::Caret,
        b'|' => Tok::Pipe,
        b'=' => Tok::Assign,
        _ => return None,
    })
}
