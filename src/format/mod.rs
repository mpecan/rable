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
/// bash's form of `${ …; }` / `${| …; }` (`content` is the text after
/// `${ ` / `${|`, before `}`): the reformatted body, a `;` unless the body
/// ends a line or with ` &` (bash's `semicolon()`), then ` }`. So
/// `${ echo hi;}` → `${ echo hi; }` and `${| echo hi; }` → `${|echo hi; }`.
pub fn funsub_text(pipe: bool, content: &str) -> String {
    let open = if pipe { "${|" } else { "${ " };
    let mut body =
        reformat_bash(content).unwrap_or_else(|| crate::sexp::normalize_cmdsub_content(content));
    if body.is_empty() {
        // bash prints an empty body as `${ }`, with or without the `|`.
        return "${ }".to_string();
    }
    // A body ended by a newline keeps it, as bash's newline connector does.
    if content.trim_end_matches([' ', '\t']).ends_with('\n') && !body.ends_with('\n') {
        body.push('\n');
    }
    let mut text = format!("{open}{body}");
    if !(body.ends_with('\n') || body.ends_with(" &")) {
        text.push(';');
    }
    text.push_str(" }");
    text
}

pub fn reformat_bash(source: &str) -> Option<String> {
    if source.is_empty() || source.len() > 1000 {
        return None;
    }
    let _guard = DepthGuard::enter()?;

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

    fn reformat(source: &str) -> String {
        let Some(out) = reformat_bash(source) else {
            unreachable!("expected {source:?} to be reformatted");
        };
        out
    }

    #[test]
    fn bare_negation_and_time_keep_their_space() {
        // bash prints `! ` / `time ` before the (empty) command: `$(! )`.
        assert_eq!(reformat("! ;"), "! ");
        assert_eq!(reformat("a && time ;"), "a && time ");
        assert_eq!(reformat("! ; b"), "! ; b");
    }

    #[test]
    fn time_is_printed_before_negation() {
        assert_eq!(reformat("! time a"), "time ! a");
        assert_eq!(reformat("time ! a"), "time ! a");
        assert_eq!(reformat("! time -p a"), "time -p ! a");
    }

    #[test]
    fn plain_word_input_is_normalized_too() {
        // bash reprints word-only content as well: blanks collapse, and
        // `time --` is its `-p` form.
        assert_eq!(reformat("a  b"), "a b");
        assert_eq!(reformat("path/to/file-1.txt v2"), "path/to/file-1.txt v2");
        assert_eq!(reformat("time  -p a"), "time -p a");
        assert_eq!(reformat("time -- a"), "time -p a");
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
    fn omits_the_file_descriptor_when_it_is_the_operator_default() {
        // Input redirections default to fd 0, output redirections to fd 1.
        assert_eq!(reformat("cat 0< f; echo hi"), "cat < f; echo hi");
        assert_eq!(reformat("cat 0<> f; echo hi"), "cat <> f; echo hi");
        assert_eq!(reformat("cat 1> f; echo hi"), "cat > f; echo hi");
    }

    #[test]
    fn arithmetic_commands_and_pipe_both_after_compounds() {
        // These used to leak S-expression text into the word.
        assert_eq!(reformat("((x  =5)) >f"), "((x  =5)) > f");
        assert_eq!(reformat("((x)) && y"), "((x)) && y");
        // `|&` after a compound is a bare `2>&1` stage; bash prints it on
        // the command.
        assert_eq!(reformat("(cmd) |& cat"), "( cmd ) 2>&1 | cat");
        assert_eq!(reformat("{ a; } |& b"), "{ a; } 2>&1 | b");
        assert_eq!(reformat("((x++)) |& c"), "((x++)) 2>&1 | c");
    }

    #[test]
    fn separators_after_heredocs_follow_bash_print_cmd() {
        // Bodies take a `;`'s place, and bash then drops the next `;`.
        assert_eq!(reformat("a <<E; b\nx\nE"), "a <<E\nx\nE\n b");
        assert_eq!(reformat("a <<E && b; c\nx\nE"), "a <<E && \nx\nE\n b c");
        assert_eq!(
            reformat("a <<E && b; c; d\nx\nE"),
            "a <<E && \nx\nE\n b c; d"
        );
        // `semicolon()`: no `;` right after the bodies.
        assert_eq!(reformat("{ a <<E; } && b\nx\nE"), "{ a <<E\nx\nE\n } && b");
        // A newline separator stays a newline.
        assert_eq!(reformat("{ a\nb; }"), "{ a\nb; }");
        assert_eq!(
            reformat("{ cat <<E\nx\nE\n  b; }"),
            "{ cat <<E\nx\nE\n\nb; }"
        );
    }

    #[test]
    fn function_bodies_break_lines_like_bash() {
        let body = |source: &str| reformat(source).replace("function f () \n{ \n", "");
        assert_eq!(body("f() { a; b; c; }"), "    a;\n    b;\n    c\n}");
        assert_eq!(body("f() { a & b; }"), "    a & b\n}");
        assert_eq!(body("f() { a & }"), "    a &\n}");
        assert_eq!(body("f() { a\nb; }"), "    a\n\n    b\n}");
    }

    #[test]
    fn coproc_with_a_compound_body_is_named() {
        assert_eq!(reformat("coproc { ls; }"), "coproc COPROC { ls; }");
        assert_eq!(reformat("coproc NAME { ls; }"), "coproc NAME { ls; }");
        assert_eq!(reformat("coproc (cat)"), "coproc COPROC ( cat )");
        assert_eq!(reformat("coproc cat <f"), "coproc cat < f");
    }

    #[test]
    fn dup_move_and_close_follow_bash_print_redirection() {
        // A numeric dup or a move always prints its fd; a close is always
        // `N>&-`; a dup to a word omits the default fd.
        assert_eq!(reformat("cat 0<&1; echo hi"), "cat 0<&1; echo hi");
        assert_eq!(reformat("echo error >&2"), "echo error 1>&2");
        assert_eq!(reformat("read <&0"), "read 0<&0");
        assert_eq!(reformat("cmd >&02"), "cmd 1>&2");
        assert_eq!(reformat("exec 3<&0-"), "exec 3<&0-");
        assert_eq!(reformat("exec 3<&a0-"), "exec 3<&a0-");
        assert_eq!(reformat("cmd >&-"), "cmd 1>&-");
        assert_eq!(reformat("cmd <&-"), "cmd 0>&-");
        assert_eq!(reformat("cmd 3<&-"), "cmd 3>&-");
        assert_eq!(reformat("cmd >& -"), "cmd 1>&-");
        assert_eq!(reformat("cmd >&$fd"), "cmd >&$fd");
        assert_eq!(reformat("cmd 2>&$fd"), "cmd 2>&$fd");
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
        // body, and bash keeps a space after the body (`$(… )`). Locating
        // that heredoc means looking through the pipeline to its last
        // command.
        assert_eq!(
            reformat("a | cat <<EOF &\nbody\nEOF"),
            "a | cat <<EOF &\nbody\nEOF\n "
        );
    }

    #[test]
    fn list_operator_after_heredoc_precedes_its_body() {
        // bash's print_deferred_heredocs: operator, bodies, then a space;
        // `&` followed by a command gets two.
        assert_eq!(reformat("a <<E && b\nx\nE"), "a <<E && \nx\nE\n b");
        assert_eq!(reformat("a <<E || b\nx\nE"), "a <<E || \nx\nE\n b");
        assert_eq!(
            reformat("cat <<E & cat <<F &\nb\nE\nc\nF"),
            "cat <<E &\nb\nE\n  cat <<F &\nc\nF\n "
        );
        // The operator goes where the bodies start, even if a body
        // itself contains `<<`.
        assert_eq!(reformat("cat <<E &\na <<b\nE"), "cat <<E &\na <<b\nE\n ");
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
