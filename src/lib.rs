//! # Rable — A complete GNU Bash 5.3-compatible parser
//!
//! Rable parses bash source code into an abstract syntax tree (AST) of [`Node`]
//! values. Each node can be formatted as an S-expression via its [`Display`]
//! implementation, producing output identical to [Parable](https://github.com/ldayton/Parable).
//!
//! # Quick Start
//!
//! ```
//! use rable::{parse, NodeKind};
//!
//! let nodes = parse("echo hello | grep h", false).unwrap();
//! assert_eq!(nodes.len(), 1);
//!
//! // S-expression output via Display
//! let sexp = nodes[0].to_string();
//! assert!(sexp.contains("pipe"));
//! ```
//!
//! # Parsing Options
//!
//! The `extglob` parameter enables extended glob patterns (`@()`, `?()`, `*()`,
//! `+()`, `!()`). Set to `false` for standard bash parsing.
//!
//! ```
//! // Standard parsing
//! let nodes = rable::parse("echo hello", false).unwrap();
//!
//! // With extended globs
//! let nodes = rable::parse("echo @(foo|bar)", true).unwrap();
//! ```
//!
//! # Error Handling
//!
//! Parse errors include line and position information:
//!
//! ```
//! match rable::parse("if", false) {
//!     Ok(_) => unreachable!(),
//!     Err(e) => {
//!         assert_eq!(e.line(), 1);
//!         println!("Error: {e}");
//!     }
//! }
//! ```
//!
//! # Working with the AST
//!
//! The AST uses a [`Node`] struct wrapping a [`NodeKind`] enum with a [`Span`].
//! Pattern matching on `node.kind` is the primary way to inspect nodes:
//!
//! ```
//! use rable::{parse, NodeKind};
//!
//! let nodes = parse("echo hello world", false).unwrap();
//! match &nodes[0].kind {
//!     NodeKind::Command { words, redirects, .. } => {
//!         assert_eq!(words.len(), 3); // echo, hello, world
//!         assert!(redirects.is_empty());
//!     }
//!     _ => panic!("expected Command"),
//! }
//! ```

pub mod ast;
pub mod error;
pub mod token;

// Public for advanced use (S-expression formatting)
pub mod sexp;

// Implementation details — not part of the stable API
pub(crate) mod context;
pub(crate) mod format;
pub(crate) mod lexer;
pub(crate) mod parser;

#[cfg(feature = "python")]
mod python;

// Convenient re-exports
pub use ast::{CasePattern, ListItem, ListOperator, Node, NodeKind, PipeSep, Span};
pub use error::{RableError, Result};
pub use token::{Token, TokenType};

/// Parses a bash source string into a list of top-level AST nodes.
///
/// Each top-level command separated by newlines becomes a separate node.
/// Commands separated by `;` on the same line are grouped into a single
/// [`NodeKind::List`].
///
/// Set `extglob` to `true` to enable extended glob patterns (`@()`, `?()`,
/// `*()`, `+()`, `!()`).
///
/// # Examples
///
/// ```
/// let nodes = rable::parse("echo hello", false).unwrap();
/// assert_eq!(nodes[0].to_string(), "(command (word \"echo\") (word \"hello\"))");
/// ```
///
/// ```
/// // Multiple top-level commands
/// let nodes = rable::parse("echo a\necho b", false).unwrap();
/// assert_eq!(nodes.len(), 2);
/// ```
///
/// # Errors
///
/// Returns [`RableError::Parse`] for syntax errors and
/// [`RableError::MatchedPair`] for unclosed delimiters.
pub fn parse(source: &str, extglob: bool) -> Result<Vec<Node>> {
    let lexer = lexer::Lexer::new(source, extglob);
    let mut parser = parser::Parser::new(lexer);
    parser.parse_all()
}
