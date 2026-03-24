/// AST node representing all bash constructs.
///
/// This enum mirrors Parable's AST node classes exactly, ensuring
/// S-expression output compatibility.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::use_self)]
pub enum Node {
    /// A word token, possibly containing expansion parts.
    Word { value: String, parts: Vec<Node> },

    /// A simple command: words and redirects.
    Command {
        words: Vec<Node>,
        redirects: Vec<Node>,
    },

    /// A pipeline of commands separated by `|`.
    Pipeline { commands: Vec<Node> },

    /// A list of commands with operators (`;`, `&&`, `||`, `&`, `\n`).
    List { parts: Vec<Node> },

    /// An operator between commands in a list.
    Operator { op: String },

    // -- Compound commands --
    /// `if condition; then body; [elif ...; then ...;] [else ...;] fi`
    If {
        condition: Box<Node>,
        then_body: Box<Node>,
        else_body: Option<Box<Node>>,
        redirects: Vec<Node>,
    },

    /// `while condition; do body; done`
    While {
        condition: Box<Node>,
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// `until condition; do body; done`
    Until {
        condition: Box<Node>,
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// `for var [in words]; do body; done`
    For {
        var: String,
        words: Option<Vec<Node>>,
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// C-style for loop: `for (( init; cond; incr )); do body; done`
    ForArith {
        init: String,
        cond: String,
        incr: String,
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// `select var [in words]; do body; done`
    Select {
        var: String,
        words: Option<Vec<Node>>,
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// `case word in pattern) body;; ... esac`
    Case {
        word: Box<Node>,
        patterns: Vec<CasePattern>,
        redirects: Vec<Node>,
    },

    /// A function definition: `name() { body; }` or `function name { body; }`
    Function { name: String, body: Box<Node> },

    /// A subshell: `( commands )`
    Subshell {
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// A brace group: `{ commands; }`
    BraceGroup {
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// A coprocess: `coproc [name] command`
    Coproc {
        name: Option<String>,
        command: Box<Node>,
    },

    // -- Redirections --
    /// I/O redirection: `[fd]op target`
    Redirect {
        op: String,
        target: Box<Node>,
        fd: i32,
    },

    /// Here-document: `<<[-]DELIM\ncontent\nDELIM`
    HereDoc {
        delimiter: String,
        content: String,
        strip_tabs: bool,
        quoted: bool,
        fd: i32,
        complete: bool,
    },

    // -- Expansions --
    /// Parameter expansion: `$var` or `${var[op arg]}`
    ParamExpansion {
        param: String,
        op: Option<String>,
        arg: Option<String>,
    },

    /// Parameter length: `${#var}`
    ParamLength { param: String },

    /// Indirect expansion: `${!var[op arg]}`
    ParamIndirect {
        param: String,
        op: Option<String>,
        arg: Option<String>,
    },

    /// Command substitution: `$(cmd)` or `` `cmd` ``
    CommandSubstitution { command: Box<Node>, brace: bool },

    /// Process substitution: `<(cmd)` or `>(cmd)`
    ProcessSubstitution {
        direction: String,
        command: Box<Node>,
    },

    /// ANSI-C quoting: `$'...'`
    AnsiCQuote { content: String },

    /// Locale string: `$"..."`
    LocaleString { content: String },

    /// Arithmetic expansion: `$(( expr ))`
    ArithmeticExpansion { expression: Option<Box<Node>> },

    /// Arithmetic command: `(( expr ))`
    ArithmeticCommand {
        expression: Option<Box<Node>>,
        redirects: Vec<Node>,
        raw_content: String,
    },

    // -- Arithmetic expression nodes --
    /// A numeric literal in arithmetic context.
    ArithNumber { value: String },

    /// A variable reference in arithmetic context.
    ArithVar { name: String },

    /// A binary operation in arithmetic context.
    ArithBinaryOp {
        op: String,
        left: Box<Node>,
        right: Box<Node>,
    },

    /// A unary operation in arithmetic context.
    ArithUnaryOp { op: String, operand: Box<Node> },

    /// Pre-increment `++var`.
    ArithPreIncr { operand: Box<Node> },

    /// Post-increment `var++`.
    ArithPostIncr { operand: Box<Node> },

    /// Pre-decrement `--var`.
    ArithPreDecr { operand: Box<Node> },

    /// Post-decrement `var--`.
    ArithPostDecr { operand: Box<Node> },

    /// Assignment in arithmetic context.
    ArithAssign {
        op: String,
        target: Box<Node>,
        value: Box<Node>,
    },

    /// Ternary `cond ? true : false`.
    ArithTernary {
        condition: Box<Node>,
        if_true: Option<Box<Node>>,
        if_false: Option<Box<Node>>,
    },

    /// Comma operator in arithmetic context.
    ArithComma { left: Box<Node>, right: Box<Node> },

    /// Array subscript in arithmetic context.
    ArithSubscript { array: String, index: Box<Node> },

    /// Empty arithmetic expression.
    ArithEmpty,

    /// An escaped character in arithmetic context.
    ArithEscape { ch: String },

    /// Deprecated `$[expr]` arithmetic.
    ArithDeprecated { expression: String },

    /// Concatenation in arithmetic context (e.g., `0x$var`).
    ArithConcat { parts: Vec<Node> },

    // -- Conditional expression nodes (`[[ ]]`) --
    /// `[[ expr ]]`
    ConditionalExpr {
        body: Box<Node>,
        redirects: Vec<Node>,
    },

    /// Unary test: `-f file`, `-z string`, etc.
    UnaryTest { op: String, operand: Box<Node> },

    /// Binary test: `a == b`, `a -nt b`, etc.
    BinaryTest {
        op: String,
        left: Box<Node>,
        right: Box<Node>,
    },

    /// `[[ a && b ]]`
    CondAnd { left: Box<Node>, right: Box<Node> },

    /// `[[ a || b ]]`
    CondOr { left: Box<Node>, right: Box<Node> },

    /// `[[ ! expr ]]`
    CondNot { operand: Box<Node> },

    /// `[[ ( expr ) ]]`
    CondParen { inner: Box<Node> },

    /// A term (word) in a conditional expression.
    CondTerm { value: String },

    // -- Other --
    /// Pipeline negation with `!`.
    Negation { pipeline: Box<Node> },

    /// `time [-p] pipeline`
    Time { pipeline: Box<Node>, posix: bool },

    /// Marker for `|&` (pipe both stdout and stderr).
    PipeBoth,

    /// Array literal: `(a b c)`.
    Array { elements: Vec<Node> },

    /// An empty node.
    Empty,

    /// A comment: `# text`.
    Comment { text: String },
}

/// A single case pattern clause within a `case` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct CasePattern {
    pub patterns: Vec<Node>,
    pub body: Option<Node>,
    pub terminator: String,
}

impl CasePattern {
    pub const fn new(patterns: Vec<Node>, body: Option<Node>, terminator: String) -> Self {
        Self {
            patterns,
            body,
            terminator,
        }
    }
}
