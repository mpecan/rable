//! Depth-guard and error-reporting tests for the recursive-descent parser.
//!
//! Split from `tests.rs` to keep both files under the project's file-length
//! limit. These cover the `MAX_DEPTH` guard — which needs an oversized stack
//! to reach in a debug build — and the position/message a `RableError`
//! carries back to callers.

use super::{Lexer, MAX_DEPTH, Parser};
use crate::ast::Node;

#[allow(clippy::unwrap_used)]
fn parse(source: &str) -> Vec<Node> {
    let lexer = Lexer::new(source, false);
    let mut parser = Parser::new(lexer);
    parser.parse_all().unwrap()
}

#[test]
fn error_reports_the_real_line_position_and_message() {
    // `RableError`'s accessors are the only way a caller can locate a
    // syntax error, so assert concrete values rather than just `is_err`.
    let lexer = Lexer::new("echo a\necho b\nfi", false);
    let mut parser = Parser::new(lexer);
    let Err(err) = parser.parse_all() else {
        unreachable!("expected a parse error");
    };
    assert_eq!(err.line(), 3);
    assert_eq!(err.pos(), 14);
    assert_eq!(err.message(), "unexpected reserved word 'fi'");
}

#[test]
fn flat_command_sequences_do_not_accumulate_depth() {
    // The depth counter guards against runaway nesting, so it must unwind
    // as each command finishes. A long *flat* script nests one level deep
    // but enters the guard once per command; if depth were never released
    // this would trip `MAX_DEPTH` and fail to parse.
    let source = "a; ".repeat(MAX_DEPTH + 200);
    let nodes = parse(&source);
    assert_eq!(nodes.len(), 1);
}

#[test]
fn nesting_beyond_the_depth_limit_is_rejected() {
    // The depth guard exists so pathological nesting fails with an error
    // instead of exhausting the stack. Reaching `MAX_DEPTH` needs more
    // native stack than a debug-build test thread has by default — release
    // frames are several times smaller — so run it with headroom. The
    // assertion is about the guard firing, not about frame size.
    let Ok(handle) = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let depth = MAX_DEPTH;
            let source = format!("{}x{}", "{ ".repeat(depth), "; }".repeat(depth));
            let lexer = Lexer::new(&source, false);
            let mut parser = Parser::new(lexer);
            let Err(err) = parser.parse_all() else {
                unreachable!("expected the depth guard to reject {depth}-deep nesting");
            };
            assert_eq!(err.message(), "maximum parsing depth exceeded");
        })
    else {
        unreachable!("could not spawn the deep-nesting test thread");
    };
    assert!(
        handle.join().is_ok(),
        "deep-nesting test thread panicked; see the panic message above"
    );
}
