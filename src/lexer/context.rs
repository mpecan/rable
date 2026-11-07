//! Context flags the lexer carries between tokens.

/// Mutable context flags the parser uses to inform the lexer.
/// Private — the parser interacts via methods on `Lexer`.
// Independent flags, each set by different tokens: not states of one enum.
#[allow(clippy::struct_excessive_bools)]
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
    /// The current simple command's first word is an assignment builtin
    /// (`declare`, `local`, …), so its `name=(…)` arguments are array
    /// assignments too.
    pub(crate) assignment_builtin: bool,
    /// The next word is a redirect's target (or heredoc delimiter), not
    /// part of the command: `>f a=(x)` keeps `a=(x)` an assignment.
    pub(crate) redirect_target: bool,
    /// The current simple command has consumed a word or assignment, so a
    /// redirect now ends assignment position: `a=(x) >f b=(y)` is an error.
    pub(crate) command_has_word: bool,
}

impl Default for LexerContext {
    fn default() -> Self {
        Self {
            command_start: true,
            reserved_words_ok: true,
            cond_expr: false,
            assignment_builtin: false,
            redirect_target: false,
            command_has_word: false,
        }
    }
}
