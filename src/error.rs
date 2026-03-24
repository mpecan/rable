/// Errors produced by the Rable bash parser.
#[derive(Debug, thiserror::Error)]
pub enum RableError {
    /// A syntax error encountered during parsing.
    #[error("parse error at line {line}, position {pos}: {message}")]
    Parse {
        message: String,
        pos: usize,
        line: usize,
    },

    /// An unmatched delimiter (parenthesis, brace, bracket, quote) at EOF.
    #[error("unmatched delimiter at line {line}, position {pos}: {message}")]
    MatchedPair {
        message: String,
        pos: usize,
        line: usize,
    },
}

impl RableError {
    pub fn parse(message: impl Into<String>, pos: usize, line: usize) -> Self {
        Self::Parse {
            message: message.into(),
            pos,
            line,
        }
    }

    pub fn matched_pair(message: impl Into<String>, pos: usize, line: usize) -> Self {
        Self::MatchedPair {
            message: message.into(),
            pos,
            line,
        }
    }
}

pub type Result<T> = std::result::Result<T, RableError>;
