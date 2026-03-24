pub mod ast;
pub mod context;
pub mod error;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod sexp;
pub mod token;

#[cfg(feature = "python")]
mod python;

use error::Result;

/// Parses a bash source string into a list of AST nodes.
///
/// This is the main entry point for the parser. The returned nodes
/// can be formatted as S-expressions using their `Display` implementation,
/// producing output compatible with Parable.
///
/// # Errors
///
/// Returns `RableError::Parse` for syntax errors and
/// `RableError::MatchedPair` for unclosed delimiters.
pub fn parse(source: &str, extglob: bool) -> Result<Vec<ast::Node>> {
    let lexer = lexer::Lexer::new(source, extglob);
    let mut parser = parser::Parser::new(lexer);
    parser.parse_all()
}
