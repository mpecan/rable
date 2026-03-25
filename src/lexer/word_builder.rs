//! Word builder for accumulating word tokens with expansion span tracking.
//!
//! `WordBuilder` bundles the word value string with a list of `WordSpan`s
//! that record where each expansion starts and ends, along with the
//! quoting context at the point of recording. This eliminates the need
//! for downstream code to re-parse word values.
#![allow(dead_code)]

/// The quoting context at the point where a span was recorded.
///
/// Bash has context-sensitive expansion rules. For example, `$'...'`
/// is an ANSI-C quote at top level or inside `${...}`, but is NOT
/// special inside `"..."` (it's just literal `$` + `'...'`).
///
/// Each span captures the context stack so downstream consumers can
/// make correct decisions without re-deriving context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotingContext {
    /// Top-level word context (no quoting).
    None,
    /// Inside double quotes `"..."`.
    DoubleQuote,
    /// Inside parameter expansion `${...}` (resets quoting context).
    ParamExpansion,
    /// Inside command substitution `$(...)` (resets quoting context).
    CommandSub,
    /// Inside backtick command substitution `` `...` ``.
    Backtick,
}

/// Accumulates a word token's value string and expansion spans.
pub struct WordBuilder {
    /// The word value being built character by character.
    pub value: String,
    /// Expansion spans recorded during lexing, ordered by start offset.
    pub spans: Vec<WordSpan>,
    /// Stack of quoting contexts — the current context is the last entry.
    /// Empty means top-level (no quoting).
    context_stack: Vec<QuotingContext>,
}

impl WordBuilder {
    pub const fn new() -> Self {
        Self {
            value: String::new(),
            spans: Vec::new(),
            context_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, c: char) {
        self.value.push(c);
    }

    pub fn push_str(&mut self, s: &str) {
        self.value.push_str(s);
    }

    pub fn ends_with(&self, c: char) -> bool {
        self.value.ends_with(c)
    }

    pub const fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns the current byte offset — use before an expansion to
    /// capture its start position.
    pub const fn span_start(&self) -> usize {
        self.value.len()
    }

    /// Records a completed expansion span from `start` to the current
    /// end of the value string, capturing the current quoting context.
    pub fn record(&mut self, start: usize, kind: WordSpanKind) {
        self.spans.push(WordSpan {
            start,
            end: self.value.len(),
            kind,
            context: self.current_context(),
        });
    }

    /// Returns the current quoting context.
    pub fn current_context(&self) -> QuotingContext {
        self.context_stack
            .last()
            .copied()
            .unwrap_or(QuotingContext::None)
    }

    /// Pushes a quoting context onto the stack.
    pub fn enter_context(&mut self, ctx: QuotingContext) {
        self.context_stack.push(ctx);
    }

    /// Pops the current quoting context from the stack.
    pub fn leave_context(&mut self) {
        self.context_stack.pop();
    }
}

/// A byte-range span within a word token's value string, identifying
/// an expansion or quoting construct and the context it appeared in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    /// Start byte offset in the word value (inclusive).
    pub start: usize,
    /// End byte offset in the word value (exclusive).
    pub end: usize,
    /// The kind of expansion or quoting at this span.
    pub kind: WordSpanKind,
    /// The quoting context this span was recorded in.
    pub context: QuotingContext,
}

/// The kind of expansion or quoting construct a `WordSpan` represents.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WordSpanKind {
    // -- Sexp-relevant (formatter extracts content from these) --
    /// Command substitution: `$(...)`.
    CommandSub,
    /// ANSI-C quoting: `$'...'`.
    AnsiCQuote,
    /// Locale translation string: `$"..."`.
    LocaleString,
    /// Process substitution: `<(...)` or `>(...)`.
    ProcessSub(char),

    // -- Structural (formatter treats as literal text) --
    /// Single-quoted string: `'...'`.
    SingleQuoted,
    /// Double-quoted string: `"..."`.
    DoubleQuoted,
    /// Parameter expansion: `${...}`.
    ParamExpansion,
    /// Simple variable: `$VAR`.
    SimpleVar,
    /// Arithmetic substitution: `$((...))`.
    ArithmeticSub,
    /// Backtick command substitution: `` `...` ``.
    Backtick,
    /// Array subscript: `[...]`.
    BracketSubscript,
    /// Extended glob pattern: `@(...)`, `?(...)`, etc.
    Extglob(char),
    /// Deprecated arithmetic: `$[...]`.
    DeprecatedArith,
    /// Backslash escape: `\X` (not `\<newline>` line continuations).
    Escape,
}
