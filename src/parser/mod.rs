//! Recursive-descent parser for Bash.
//!
//! The parser is organised by topic, one file per area of responsibility:
//!
//! | file                   | what it parses                                 |
//! |------------------------|------------------------------------------------|
//! | `lists.rs`             | `;`, `\n`, `&`, `&&`, `\|\|`, pipelines        |
//! | `simple_command.rs`    | command dispatch + simple-command assembly     |
//! | `redirects.rs`         | `<`, `>`, `<<`, heredoc queuing, fd prefixes   |
//! | `conditionals.rs`      | `if`/`elif`/`else`/`fi` and `[[ … ]]`          |
//! | `loops.rs`             | `while`, `until`, `for` (word list + C-style)  |
//! | `case_select.rs`       | `case … esac`, `select … do … done`            |
//! | `functions.rs`         | subshell, brace group, function, `coproc`, `(( ))` |
//! | `arithmetic.rs`        | the `$(( … ))` expression parser               |
//! | `word_parts.rs`        | quoted / expanded word-segment decomposition   |
//! | `helpers.rs`           | tiny utility functions (free functions)        |
//!
//! All parsing methods hang off the [`Parser`] struct defined here; the
//! topic files contribute additional `impl Parser` blocks.

mod arithmetic;
mod case_select;
mod conditionals;
mod functions;
pub mod helpers;
mod lists;
mod loops;
mod redirects;
mod simple_command;
mod word_parts;

use crate::ast::{Node, NodeKind, Span};
use crate::error::{RableError, Result};
use crate::lexer::{Lexer, LexerMode};
use crate::token::{Token, TokenType};

use helpers::{fill_heredoc_contents, word_node_from_token};

/// Maximum recursion/iteration depth to prevent infinite loops.
const MAX_DEPTH: usize = 1000;

/// Recursive descent parser for bash.
pub struct Parser {
    pub(super) lexer: Lexer,
    pub(super) depth: usize,
}

/// Runs the real command-list grammar on a fork of `outer` until the
/// matching `)` is consumed. Returns the inner lexer's final `(pos, line)`;
/// the parsed AST is discarded. `outer_depth` is inherited so `MAX_DEPTH`
/// stays enforced globally across nested `$(...)`.
pub fn parse_cmdsub_body(outer: &Lexer, outer_depth: usize) -> Result<(usize, usize)> {
    let mut parser = Parser {
        lexer: outer.fork(LexerMode::Cmdsub),
        depth: outer_depth,
    };
    parser.parse_cmdsub_body_inner()?;
    Ok((parser.lexer.pos(), parser.lexer.line()))
}

/// Runs the real command-list grammar on a backtick fork of `outer`
/// until an unescaped `` ` `` is reached. Consumes the closing `` ` ``
/// and returns the inner lexer's final `(pos, line)`; the parsed AST is
/// discarded. `outer_depth` is inherited for `MAX_DEPTH`.
pub fn parse_backtick_body(outer: &Lexer, outer_depth: usize) -> Result<(usize, usize)> {
    let mut parser = Parser {
        lexer: outer.fork(LexerMode::Backtick),
        depth: outer_depth,
    };
    parser.parse_backtick_body_inner()?;
    Ok((parser.lexer.pos(), parser.lexer.line()))
}

impl Parser {
    pub const fn new(lexer: Lexer) -> Self {
        Self { lexer, depth: 0 }
    }

    fn parse_cmdsub_body_inner(&mut self) -> Result<()> {
        self.skip_newlines()?;
        if !self.peek_is(TokenType::RightParen)? {
            let _ = self.parse_list()?;
        }
        self.expect(TokenType::RightParen)?;
        Ok(())
    }

    fn parse_backtick_body_inner(&mut self) -> Result<()> {
        self.skip_newlines()?;
        if !self.at_end()? {
            let _ = self.parse_list()?;
        }
        self.lexer.exit_backtick_fork()
    }

    /// Parses the entire input, returning a list of top-level nodes.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on syntax errors or unclosed delimiters.
    pub fn parse_all(&mut self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();

        self.skip_newlines()?;

        while !self.at_end()? {
            let prev_pos = self.lexer.pos();
            let mut node = self.parse_top_level_list()?;
            self.skip_newlines()?;
            fill_heredoc_contents(&mut node, &mut self.lexer);
            nodes.push(node);
            if self.lexer.pos() == prev_pos && !self.at_end()? {
                self.lexer.next_token()?;
            }
        }

        Ok(nodes)
    }

    // -- cursor / span utilities -------------------------------------------

    /// Returns the position of the next token without consuming it.
    pub(super) fn peek_pos(&mut self) -> Result<usize> {
        Ok(self.lexer.peek_token()?.pos)
    }

    /// Creates a span from the given start to the end of the last consumed token.
    pub(super) const fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.lexer.last_token_end())
    }

    /// Creates a spanned node from start to the end of the last consumed token.
    pub(super) const fn spanned(&self, start: usize, kind: NodeKind) -> Node {
        Node::new(kind, self.span_from(start))
    }

    pub(super) fn at_end(&mut self) -> Result<bool> {
        Ok(self.lexer.peek_token()?.kind == TokenType::Eof)
    }

    pub(super) fn skip_newlines(&mut self) -> Result<()> {
        while !self.at_end()? && self.lexer.peek_token()?.kind == TokenType::Newline {
            self.lexer.next_token()?;
        }
        Ok(())
    }

    // -- depth guard -------------------------------------------------------

    /// Increments depth and returns an error if too deep.
    pub(super) fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        self.lexer.set_parser_depth(self.depth);
        if self.depth > MAX_DEPTH {
            return Err(RableError::parse(
                "maximum parsing depth exceeded",
                self.lexer.pos(),
                self.lexer.line(),
            ));
        }
        Ok(())
    }

    pub(super) const fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.lexer.set_parser_depth(self.depth);
    }

    // -- token validation --------------------------------------------------

    pub(super) fn peek_is(&mut self, kind: TokenType) -> Result<bool> {
        Ok(self.lexer.peek_token()?.kind == kind)
    }

    pub(super) fn expect(&mut self, kind: TokenType) -> Result<Token> {
        let tok = self.lexer.next_token()?;
        if tok.kind != kind {
            return Err(RableError::parse(
                format!("expected {:?}, got {:?}", kind, tok.kind),
                tok.pos,
                tok.line,
            ));
        }
        Ok(tok)
    }

    /// Expects a closing delimiter — matches either the specific token type
    /// or a Word with the given value. Used for `}` and `]]` which can be
    /// either dedicated tokens or plain words depending on context.
    pub(super) fn expect_closing(&mut self, kind: TokenType, value: &str) -> Result<Token> {
        let tok = self.lexer.next_token()?;
        if tok.kind == kind || (tok.kind == TokenType::Word && tok.value == value) {
            Ok(tok)
        } else {
            Err(RableError::parse(
                format!("expected {value}, got {:?}", tok.value),
                tok.pos,
                tok.line,
            ))
        }
    }

    pub(super) fn is_double_paren(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        if tok.kind != TokenType::LeftParen {
            return Ok(false);
        }
        Ok(self.lexer.char_after_peeked() == Some('('))
    }

    /// Returns true if the next token would close an enclosing compound
    /// construct (`fi`, `done`, `esac`, `)`, `}`, `;;`, `;&`, `;;&`, `then`,
    /// `else`, `elif`, `do`, `]]`, `}` as a word, or EOF).
    pub(super) fn is_list_terminator(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        if tok.kind == TokenType::Word && (tok.value == "}" || tok.value == "]]") {
            return Ok(true);
        }
        Ok(matches!(
            tok.kind,
            TokenType::Eof
                | TokenType::Fi
                | TokenType::Done
                | TokenType::Esac
                | TokenType::RightParen
                | TokenType::RightBrace
                | TokenType::DoubleSemi
                | TokenType::SemiAnd
                | TokenType::SemiSemiAnd
                | TokenType::Then
                | TokenType::Else
                | TokenType::Elif
                | TokenType::Do
        ))
    }

    // -- shared for / select word-list reader ------------------------------

    /// Reads the words following `in` in a `for`/`select` header. Stops at a
    /// `;`, newline, `do`, `{`, or EOF without consuming the terminator.
    pub(super) fn parse_in_word_list(&mut self) -> Result<Vec<Node>> {
        let mut ws = Vec::new();
        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            if matches!(
                tok.kind,
                TokenType::Semi | TokenType::Newline | TokenType::Do | TokenType::LeftBrace
            ) {
                break;
            }
            let tok = self.lexer.next_token()?;
            ws.push(word_node_from_token(tok));
        }
        Ok(ws)
    }
}

#[cfg(test)]
mod tests;
