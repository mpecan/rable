//! Canonical bash formatter for command substitution content.
//!
//! Re-parses bash source and produces the canonical indented format
//! that Parable outputs inside `$(...)`. The implementation is split
//! across sibling files by topic, all hanging off the `Formatter`
//! struct defined in `formatter.rs`:
//!
//! | file            | responsibility                                    |
//! |-----------------|---------------------------------------------------|
//! | `mod.rs`        | `reformat_bash` entry + recursion depth guard     |
//! | `formatter.rs`  | `Formatter` struct + low-level write primitives   |
//! | `nodes.rs`      | `format_node` dispatch + command / cond-node      |
//! | `compound.rs`   | if / while / for / case / function / subshell / … |
//! | `redirects.rs`  | redirect / pipeline / heredoc-pipe interactions   |
//! | `lists.rs`      | `;`, `&&`, `\|\|`, `&` operator placement         |
//! | `words.rs`      | span-based word-value reconstruction              |

mod compound;
mod formatter;
mod lists;
mod nodes;
mod redirects;
mod words;

use std::cell::Cell;

use formatter::Formatter;

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
    let mut f = Formatter::new();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            f.write_char('\n');
        }
        f.format_node(node);
    }
    Some(f.finish())
}

#[cfg(test)]
mod tests {
    use super::reformat_bash;

    /// `reformat_bash` bails out early on word-only input, so every case
    /// here carries at least one operator.
    fn reformat(source: &str) -> String {
        let Some(out) = reformat_bash(source) else {
            unreachable!("expected {source:?} to be reformatted");
        };
        out
    }

    #[test]
    fn plain_word_input_is_left_alone() {
        // Reformatting is skipped when the content is nothing but word
        // characters — there is no layout to canonicalize, and re-emitting
        // it risks perturbing text the corpus expects verbatim. Each
        // character class in that predicate is checked on its own.
        for source in ["a b", "a_b", "a-b", "a.b", "a/b", "path/to/file-1.txt v2"] {
            assert!(
                reformat_bash(source).is_none(),
                "expected {source:?} to be left alone"
            );
        }
        // One operator is enough to opt back in.
        assert_eq!(reformat("a;b"), "a; b");
    }

    #[test]
    fn oversized_input_is_left_alone() {
        // Long substitution bodies are passed through rather than reparsed.
        let at_limit = format!("{}b", "a; ".repeat(333));
        assert_eq!(at_limit.len(), 1000);
        assert!(reformat_bash(&at_limit).is_some());

        let over_limit = "a; ".repeat(400);
        assert_eq!(over_limit.len(), 1200);
        assert!(reformat_bash(&over_limit).is_none());

        assert!(reformat_bash("").is_none());
    }

    #[test]
    fn formats_time_pipelines() {
        assert_eq!(reformat("time ls; echo hi"), "time ls; echo hi");
        assert_eq!(reformat("time -p ls; echo hi"), "time -p ls; echo hi");
    }

    #[test]
    fn formats_coproc_with_and_without_a_name() {
        assert_eq!(reformat("coproc { ls; }"), "coproc { ls; }");
        assert_eq!(reformat("coproc NAME { ls; }"), "coproc NAME { ls; }");
    }

    #[test]
    fn omits_the_file_descriptor_when_it_is_the_operator_default() {
        // Input redirections default to fd 0, output redirections to fd 1.
        assert_eq!(reformat("cat 0< f; echo hi"), "cat < f; echo hi");
        assert_eq!(reformat("cat 0<&1; echo hi"), "cat <&1; echo hi");
        assert_eq!(reformat("cat 0<> f; echo hi"), "cat <> f; echo hi");
        assert_eq!(reformat("cat 1> f; echo hi"), "cat > f; echo hi");
    }

    #[test]
    fn keeps_the_file_descriptor_when_it_differs_from_the_default() {
        assert_eq!(reformat("cat 2> f; echo hi"), "cat 2> f; echo hi");
        assert_eq!(reformat("cat 1< f; echo hi"), "cat 1< f; echo hi");
        // Operators with no default fd always keep an explicit one.
        assert_eq!(reformat("cat 1<<<x; echo hi"), "cat 1<<< x; echo hi");
    }

    #[test]
    fn heredoc_body_follows_a_pipeline_stage() {
        // The heredoc body is emitted after the pipe, which requires
        // looking through the pipeline to its last command.
        assert_eq!(
            reformat("cat <<EOF | wc\nbody\nEOF"),
            "cat <<EOF |\nbody\nEOF\n  wc"
        );
    }

    #[test]
    fn trailing_operator_lands_on_the_heredoc_delimiter_line() {
        // `&` belongs on the line that opens the heredoc, not after its
        // body. Locating that heredoc means looking through the pipeline
        // to its last command.
        assert_eq!(
            reformat("a | cat <<EOF &\nbody\nEOF"),
            "a | cat <<EOF &\nbody\nEOF\n"
        );
    }

    #[test]
    fn heredoc_on_the_final_pipeline_stage_gets_no_pipe_marker() {
        // The ` |` marker is only for a heredoc that has a stage after it.
        assert_eq!(
            reformat("wc | cat <<EOF\nbody\nEOF"),
            "wc | cat <<EOF\nbody\nEOF\n"
        );
    }

    #[test]
    fn nested_substitutions_stop_reformatting_at_the_depth_limit() {
        // One level in, `c;d` is normalized to `c; d`. Two levels in the
        // budget is spent and the text is left exactly as written.
        assert_eq!(reformat("a $(b;c)"), "a $(b; c)");
        assert_eq!(reformat("a $(b $(c;d))"), "a $(b $(c;d))");
    }
}
