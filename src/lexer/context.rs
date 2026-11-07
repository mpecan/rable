//! Context flags the lexer carries between tokens.

/// Mutable context flags the parser uses to inform the lexer.
/// Private — the parser interacts via methods on `Lexer`.
#[derive(Debug, Clone)]
pub(crate) struct LexerContext {
    /// At command start position — eligible to begin a new simple command
    /// or to accept an `AssignmentWord`.
    pub(crate) command_start: bool,
    /// Reserved-word recognition is enabled. Distinct from `command_start`:
    /// after a simple command has consumed one or more `AssignmentWord`s,
    /// subsequent words must NOT be classified as reserved words, even
    /// though we are still at command-word position. Re-armed whenever
    /// `command_start` is re-armed (separators, newlines, etc.).
    pub(crate) reserved_words_ok: bool,
    /// Inside a `[[ ]]` conditional expression.
    pub(crate) cond_expr: bool,
}

impl Default for LexerContext {
    fn default() -> Self {
        Self {
            command_start: true,
            reserved_words_ok: true,
            cond_expr: false,
        }
    }
}
