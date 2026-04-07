//! Post-hoc brace expansion detection.
//!
//! After a word is fully built by the lexer, scans the word value
//! to identify brace expansion patterns (`{a,b,c}`, `{1..10}`) and
//! records `WordSpanKind::BraceExpansion` spans. Existing spans
//! (quotes, escapes, parameter expansions) are used to skip
//! protected regions.

use super::word_builder::{QuotingContext, WordBuilder, WordSpan, WordSpanKind};

/// Scans the completed word value for brace expansion patterns and
/// records `BraceExpansion` spans for each one found.
pub(super) fn detect_brace_expansions(wb: &mut WordBuilder) {
    let value = wb.value.as_bytes();
    let spans = &wb.spans;
    let mut new_spans: Vec<WordSpan> = Vec::new();
    let mut i = 0;

    while i < value.len() {
        if value[i] != b'{' {
            i += 1;
            continue;
        }

        // Skip if preceded by $ (parameter expansion)
        if i > 0 && value[i - 1] == b'$' {
            i += 1;
            continue;
        }

        // Skip if inside an existing span
        if span_end_at(i, spans).is_some() {
            i += 1;
            continue;
        }

        // Try to find matching } with a comma or .. separator
        if let Some(close) = find_brace_close(value, i, spans) {
            new_spans.push(WordSpan {
                start: i,
                end: close + 1,
                kind: WordSpanKind::BraceExpansion,
                context: QuotingContext::None,
            });
            i = close + 1;
        } else {
            i += 1;
        }
    }

    wb.spans.extend(new_spans);
}

/// Returns the byte index of the matching `}` if the content between
/// `{` and `}` contains a `,` or `..` at depth 1. Returns `None` if
/// no valid brace expansion is found.
fn find_brace_close(value: &[u8], open: usize, spans: &[WordSpan]) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut has_comma = false;
    let mut has_dotdot = false;
    let mut j = open + 1;

    while j < value.len() {
        // Skip positions inside existing spans
        if let Some(end) = span_end_at(j, spans) {
            j = end;
            continue;
        }

        match value[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if has_comma || has_dotdot {
                        return Some(j);
                    }
                    return None;
                }
            }
            b',' if depth == 1 => has_comma = true,
            b'.' if depth == 1 && j + 1 < value.len() && value[j + 1] == b'.' => {
                has_dotdot = true;
            }
            _ => {}
        }
        j += 1;
    }

    None
}

/// If `pos` falls inside an existing span, returns the span's end offset.
fn span_end_at(pos: usize, spans: &[WordSpan]) -> Option<usize> {
    spans
        .iter()
        .find(|s| pos >= s.start && pos < s.end)
        .map(|s| s.end)
}

#[cfg(test)]
mod tests {
    use crate::lexer::word_builder::WordSpanKind;

    /// Helper: lex a single word and check for `BraceExpansion` spans.
    #[allow(clippy::unwrap_used)]
    fn brace_spans(source: &str) -> Vec<(usize, usize)> {
        let mut lexer = crate::lexer::Lexer::new(source, false);
        let tok = lexer.next_token().unwrap();
        tok.spans
            .iter()
            .filter(|s| s.kind == WordSpanKind::BraceExpansion)
            .map(|s| (s.start, s.end))
            .collect()
    }

    #[test]
    fn comma_form() {
        let spans = brace_spans("{a,b,c}");
        assert_eq!(spans, vec![(0, 7)]);
    }

    #[test]
    fn range_form() {
        let spans = brace_spans("{1..10}");
        assert_eq!(spans, vec![(0, 7)]);
    }

    #[test]
    fn mid_word() {
        let spans = brace_spans("file{1,2}.txt");
        assert_eq!(spans, vec![(4, 9)]);
    }

    #[test]
    fn nested_braces() {
        let spans = brace_spans("{a,{b,c}}");
        assert_eq!(spans, vec![(0, 9)]);
    }

    #[test]
    fn empty_braces_not_expansion() {
        let spans = brace_spans("{}");
        assert!(spans.is_empty());
    }

    #[test]
    fn single_element_not_expansion() {
        let spans = brace_spans("{a}");
        assert!(spans.is_empty());
    }

    #[test]
    fn trailing_comma() {
        let spans = brace_spans("{a,}");
        assert_eq!(spans, vec![(0, 4)]);
    }

    #[test]
    fn leading_comma() {
        let spans = brace_spans("{,a}");
        assert_eq!(spans, vec![(0, 4)]);
    }

    #[test]
    fn param_expansion_not_brace() {
        // ${foo} should NOT produce a BraceExpansion span
        let spans = brace_spans("${foo}");
        assert!(spans.is_empty());
    }

    #[test]
    fn adjacent_brace_expansions() {
        let spans = brace_spans("{a,b}{c,d}");
        assert_eq!(spans, vec![(0, 5), (5, 10)]);
    }

    #[test]
    fn alpha_range() {
        let spans = brace_spans("{a..z}");
        assert_eq!(spans, vec![(0, 6)]);
    }

    #[test]
    fn range_with_step() {
        let spans = brace_spans("{1..10..2}");
        assert_eq!(spans, vec![(0, 10)]);
    }
}
