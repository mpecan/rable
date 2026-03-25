/// Token types produced by the lexer.
///
/// These map to Parable's `TokenType` constants for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// End of input.
    Eof,
    /// A word (command name, argument, etc.).
    Word,
    /// An assignment word (e.g., `FOO=bar`).
    AssignmentWord,
    /// A numeric literal in redirect context.
    Number,
    /// A newline character.
    Newline,
    /// Pipe operator `|`.
    Pipe,
    /// Pipe-both operator `|&`.
    PipeBoth,
    /// AND operator `&&`.
    And,
    /// OR operator `||`.
    Or,
    /// Semicolon `;`.
    Semi,
    /// Double semicolon `;;`.
    DoubleSemi,
    /// Semicolon-ampersand `;&`.
    SemiAnd,
    /// Semicolon-semicolon-ampersand `;;&`.
    SemiSemiAnd,
    /// Ampersand `&`.
    Ampersand,
    /// Left parenthesis `(`.
    LeftParen,
    /// Right parenthesis `)`.
    RightParen,
    /// Left brace `{`.
    LeftBrace,
    /// Right brace `}`.
    RightBrace,
    /// Double left bracket `[[`.
    DoubleLeftBracket,
    /// Double right bracket `]]`.
    DoubleRightBracket,
    /// Less-than `<`.
    Less,
    /// Greater-than `>`.
    Greater,
    /// Double less-than `<<` (here-document).
    DoubleLess,
    /// Double greater-than `>>` (append).
    DoubleGreater,
    /// Less-ampersand `<&`.
    LessAnd,
    /// Greater-ampersand `>&`.
    GreaterAnd,
    /// Less-greater `<>`.
    LessGreater,
    /// Double less-dash `<<-` (here-document with tab stripping).
    DoubleLessDash,
    /// Triple less-than `<<<` (here-string).
    TripleLess,
    /// Greater-pipe `>|` (clobber).
    GreaterPipe,
    /// Ampersand-greater `&>`.
    AndGreater,
    /// Ampersand-double-greater `&>>`.
    AndDoubleGreater,
    /// Bang `!`.
    Bang,

    // Reserved words
    If,
    Then,
    Else,
    Elif,
    Fi,
    Do,
    Done,
    Case,
    Esac,
    While,
    Until,
    For,
    Select,
    In,
    Function,
    Time,
    Coproc,
}

impl TokenType {
    /// Returns the reserved word token type for the given string, if any.
    pub fn reserved_word(s: &str) -> Option<Self> {
        match s {
            "if" => Some(Self::If),
            "then" => Some(Self::Then),
            "else" => Some(Self::Else),
            "elif" => Some(Self::Elif),
            "fi" => Some(Self::Fi),
            "do" => Some(Self::Do),
            "done" => Some(Self::Done),
            "case" => Some(Self::Case),
            "esac" => Some(Self::Esac),
            "while" => Some(Self::While),
            "until" => Some(Self::Until),
            "for" => Some(Self::For),
            "select" => Some(Self::Select),
            "in" => Some(Self::In),
            "function" => Some(Self::Function),
            "time" => Some(Self::Time),
            "coproc" => Some(Self::Coproc),
            "!" => Some(Self::Bang),
            "{" => Some(Self::LeftBrace),
            "}" => Some(Self::RightBrace),
            "[[" => Some(Self::DoubleLeftBracket),
            "]]" => Some(Self::DoubleRightBracket),
            _ => None,
        }
    }

    /// Returns true if this token type starts a compound command.
    pub const fn starts_command(self) -> bool {
        matches!(
            self,
            Self::If
                | Self::Case
                | Self::While
                | Self::Until
                | Self::For
                | Self::Select
                | Self::LeftParen
                | Self::LeftBrace
                | Self::Function
                | Self::Coproc
                | Self::DoubleLeftBracket
                | Self::Bang
                | Self::Time
        )
    }
}

use crate::lexer::word_builder::WordSpan;

/// A token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenType,
    pub value: String,
    pub pos: usize,
    pub line: usize,
    /// Expansion spans within the word value (empty for non-word tokens).
    #[allow(dead_code)]
    pub(crate) spans: Vec<WordSpan>,
}

impl Token {
    pub fn new(kind: TokenType, value: impl Into<String>, pos: usize, line: usize) -> Self {
        Self {
            kind,
            value: value.into(),
            pos,
            line,
            spans: Vec::new(),
        }
    }

    /// Creates a word token with pre-recorded expansion spans.
    #[allow(dead_code)]
    pub(crate) const fn with_spans(
        kind: TokenType,
        value: String,
        pos: usize,
        line: usize,
        spans: Vec<WordSpan>,
    ) -> Self {
        Self {
            kind,
            value,
            pos,
            line,
            spans,
        }
    }

    /// Returns true if this token is immediately adjacent to `other` (no whitespace).
    pub const fn adjacent_to(&self, other: &Self) -> bool {
        self.pos + self.value.len() == other.pos
    }

    pub const fn eof(pos: usize, line: usize) -> Self {
        Self {
            kind: TokenType::Eof,
            value: String::new(),
            pos,
            line,
            spans: Vec::new(),
        }
    }
}
