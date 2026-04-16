//! Canonical bash formatter for command substitution content.
//!
//! Re-parses bash source and produces the canonical indented format
//! that Parable outputs inside `$(...)`. The implementation is split
//! across sibling files by topic:
//!
//! | file          | responsibility                                       |
//! |---------------|------------------------------------------------------|
//! | `mod.rs`      | `reformat_bash` entry + recursion depth guard        |
//! | `nodes.rs`    | `format_node` dispatch and compound constructs       |
//! | `redirects.rs`| redirect / pipeline / heredoc-pipe interactions      |
//! | `lists.rs`    | `;`, `&&`, `\|\|`, `&` operator placement            |
//! | `words.rs`    | span-based word-value reconstruction and indent util |

mod lists;
mod nodes;
mod redirects;
mod words;

use std::cell::Cell;

use nodes::format_node;

thread_local! {
    static REFORMAT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard for the reformat depth counter.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        REFORMAT_DEPTH.with(|d| {
            let v = d.get();
            // Allow up to depth 2 for nested command substitutions
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
        REFORMAT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Attempts to reformat bash source into canonical form.
/// Returns `None` if parsing fails (in which case raw text is used).
///
/// The re-parse runs in `LexerMode::Cmdsub` because `reformat_bash` is
/// only ever called on content extracted from a `$(…)` / `<(…)` / `>(…)`
/// span, and that mode enables the same sloppy heredoc-terminator
/// recognition the original fork used (see #39 and
/// `Lexer::try_match_sloppy_delimiter`).
pub fn reformat_bash(source: &str) -> Option<String> {
    if source.is_empty() || source.len() > 1000 {
        return None;
    }
    let _guard = DepthGuard::enter()?;

    // Always try to reformat if the content has any operators or special syntax.
    // The DepthGuard prevents recursion, and the 1000-char limit handles performance.
    let dominated_by_words = source
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.' || c == '/');
    if dominated_by_words {
        return None;
    }

    let mut lexer = crate::lexer::Lexer::new(source, false);
    lexer.set_mode(crate::lexer::LexerMode::Cmdsub);
    let mut parser = crate::parser::Parser::new(lexer);
    let nodes = parser.parse_all().ok()?;
    if nodes.is_empty() {
        return Some(String::new());
    }
    let mut out = String::new();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_node(node, &mut out, 0);
    }
    // Trim trailing spaces/tabs but NOT newlines (heredocs need those)
    let trimmed = out.trim_end_matches([' ', '\t']);
    Some(trimmed.to_string())
}
