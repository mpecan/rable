mod compound;
mod conditional;
pub mod helpers;

use crate::ast::Node;
use crate::error::{RableError, Result};
use crate::lexer::Lexer;
use crate::token::{Token, TokenType};

use helpers::{
    add_stderr_redirect, fill_heredoc_contents, is_fd_number, is_varfd, make_stderr_redirect,
    parse_heredoc_delimiter, word_node,
};

/// Maximum recursion/iteration depth to prevent infinite loops.
const MAX_DEPTH: usize = 1000;

/// Recursive descent parser for bash.
pub struct Parser {
    pub(super) lexer: Lexer,
    pub(super) depth: usize,
}

impl Parser {
    pub const fn new(lexer: Lexer) -> Self {
        Self { lexer, depth: 0 }
    }

    /// Increments depth and returns an error if too deep.
    pub(super) fn enter(&mut self) -> Result<()> {
        self.depth += 1;
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

    /// Like `parse_list` but stops at newlines — used only at the top level
    /// so that newline-separated commands become separate nodes.
    fn parse_top_level_list(&mut self) -> Result<Node> {
        self.enter()?;
        let mut left = self.parse_top_level_background()?;

        loop {
            if self.at_end()? {
                break;
            }
            let prev_pos = self.lexer.pos();
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::Semi => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    if self.at_end()? || self.is_list_terminator()? {
                        break;
                    }
                    // Check if next token is a newline (already skipped)
                    let right = self.parse_background()?;
                    left = Node::List {
                        parts: vec![left, Node::Operator { op: ";".into() }, right],
                    };
                }
                _ => break,
            }
            if self.lexer.pos() == prev_pos {
                break;
            }
        }

        self.leave();
        Ok(left)
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

    /// Parses a command list. Precedence: `;`/`\n` < `&` < `&&`/`||` < `|`.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on syntax errors or unclosed delimiters.
    pub fn parse_list(&mut self) -> Result<Node> {
        self.enter()?;
        let mut left = self.parse_background()?;

        loop {
            if self.at_end()? {
                break;
            }
            let prev_pos = self.lexer.pos();
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::Semi | TokenType::Newline => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    if self.at_end()? || self.is_list_terminator()? {
                        break;
                    }
                    let right = self.parse_top_level_background()?;
                    left = Node::List {
                        parts: vec![left, Node::Operator { op: ";".into() }, right],
                    };
                }
                _ => break,
            }
            if self.lexer.pos() == prev_pos {
                break;
            }
        }

        self.leave();
        Ok(left)
    }

    /// Like `parse_background` but stops at newlines — `&\n` creates a trailing
    /// background operator and returns, letting `parse_all` handle the next line.
    fn parse_top_level_background(&mut self) -> Result<Node> {
        let mut left = self.parse_and_or()?;

        loop {
            if self.at_end()? {
                break;
            }
            if self.lexer.peek_token()?.kind != TokenType::Ampersand {
                break;
            }
            self.lexer.next_token()?;
            // At top level, don't skip newlines after & — newline means next node
            if self.at_end()?
                || self.is_list_terminator()?
                || self.lexer.peek_token()?.kind == TokenType::Newline
            {
                left = Node::List {
                    parts: vec![left, Node::Operator { op: "&".into() }],
                };
                break;
            }
            let peek = self.lexer.peek_token()?;
            if peek.kind == TokenType::Semi {
                left = Node::List {
                    parts: vec![left, Node::Operator { op: "&".into() }],
                };
                break;
            }
            let right = self.parse_and_or()?;
            left = Node::List {
                parts: vec![left, Node::Operator { op: "&".into() }, right],
            };
        }

        Ok(left)
    }

    fn parse_background(&mut self) -> Result<Node> {
        let mut left = self.parse_and_or()?;

        loop {
            if self.at_end()? {
                break;
            }
            if self.lexer.peek_token()?.kind != TokenType::Ampersand {
                break;
            }
            self.lexer.next_token()?;
            self.skip_newlines()?;
            if self.at_end()? || self.is_list_terminator()? {
                left = Node::List {
                    parts: vec![left, Node::Operator { op: "&".into() }],
                };
                break;
            }
            let peek = self.lexer.peek_token()?;
            if matches!(peek.kind, TokenType::Semi | TokenType::Newline) {
                left = Node::List {
                    parts: vec![left, Node::Operator { op: "&".into() }],
                };
                break;
            }
            let right = self.parse_and_or()?;
            left = Node::List {
                parts: vec![left, Node::Operator { op: "&".into() }, right],
            };
        }

        Ok(left)
    }

    fn parse_and_or(&mut self) -> Result<Node> {
        let mut left = self.parse_pipeline()?;

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::And => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    let right = self.parse_pipeline()?;
                    left = Node::List {
                        parts: vec![left, Node::Operator { op: "&&".into() }, right],
                    };
                }
                TokenType::Or => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    let right = self.parse_pipeline()?;
                    left = Node::List {
                        parts: vec![left, Node::Operator { op: "||".into() }, right],
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

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

    fn parse_pipeline(&mut self) -> Result<Node> {
        if self.lexer.peek_token()?.kind == TokenType::Bang {
            self.lexer.next_token()?;
            if self.lexer.peek_token()?.kind == TokenType::Bang {
                self.lexer.next_token()?;
                return self.parse_pipeline_inner();
            }
            let inner = self.parse_pipeline()?;
            return Ok(Node::Negation {
                pipeline: Box::new(inner),
            });
        }

        if self.lexer.peek_token()?.kind == TokenType::Time {
            self.lexer.next_token()?;
            let posix = if self.check_word("-p")? {
                self.lexer.next_token()?;
                true
            } else {
                false
            };
            if self.lexer.peek_token()?.kind == TokenType::Bang {
                self.lexer.next_token()?;
                let p = self.parse_pipeline_inner()?;
                return Ok(Node::Negation {
                    pipeline: Box::new(Node::Time {
                        pipeline: Box::new(p),
                        posix,
                    }),
                });
            }
            let inner = self.parse_pipeline_inner()?;
            return Ok(Node::Time {
                pipeline: Box::new(inner),
                posix,
            });
        }

        self.parse_pipeline_inner()
    }

    fn parse_pipeline_inner(&mut self) -> Result<Node> {
        let mut commands = vec![self.parse_command()?];

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::Pipe => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    // After |, time is NOT a keyword — it's a regular command
                    self.lexer.ctx.command_start = false;
                    commands.push(self.parse_command()?);
                }
                TokenType::PipeBoth => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    if !add_stderr_redirect(commands.last_mut()) {
                        commands.push(make_stderr_redirect());
                    }
                    self.lexer.ctx.command_start = false;
                    commands.push(self.parse_command()?);
                }
                _ => break,
            }
        }

        if commands.len() == 1 {
            Ok(commands.remove(0))
        } else {
            Ok(Node::Pipeline { commands })
        }
    }

    fn check_word(&mut self, expected: &str) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(tok.kind == TokenType::Word && tok.value == expected)
    }

    pub(super) fn parse_command(&mut self) -> Result<Node> {
        self.enter()?;
        let result = self.parse_command_inner();
        self.leave();
        result
    }

    fn parse_command_inner(&mut self) -> Result<Node> {
        let tok = self.lexer.peek_token()?;
        match tok.kind {
            TokenType::If => self.parse_if(),
            TokenType::While => self.parse_while(),
            TokenType::Until => self.parse_until(),
            TokenType::For => self.parse_for(),
            TokenType::Case => self.parse_case(),
            TokenType::Select => self.parse_select(),
            TokenType::LeftParen => {
                if self.lexer.pos() + 1 < self.lexer.input_len() && self.is_double_paren()? {
                    self.parse_arith_command()
                } else {
                    self.parse_subshell()
                }
            }
            TokenType::LeftBrace => self.parse_brace_group(),
            TokenType::Function => self.parse_function(),
            TokenType::Coproc => self.parse_coproc(),
            TokenType::DoubleLeftBracket => self.parse_cond_command(),
            // Closing reserved words that cannot start a command
            TokenType::Fi | TokenType::Done | TokenType::Esac => {
                let tok = self.lexer.peek_token()?;
                Err(RableError::parse(
                    format!("unexpected reserved word '{}'", tok.value),
                    tok.pos,
                    tok.line,
                ))
            }
            _ => self.parse_simple_command(),
        }
    }

    fn parse_simple_command(&mut self) -> Result<Node> {
        let mut words = Vec::new();
        let mut redirects = Vec::new();

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                TokenType::Less
                | TokenType::Greater
                | TokenType::DoubleGreater
                | TokenType::LessAnd
                | TokenType::GreaterAnd
                | TokenType::LessGreater
                | TokenType::GreaterPipe
                | TokenType::AndGreater
                | TokenType::AndDoubleGreater
                | TokenType::DoubleLess
                | TokenType::DoubleLessDash
                | TokenType::TripleLess => {
                    redirects.push(self.parse_redirect()?);
                }
                TokenType::Word | TokenType::AssignmentWord | TokenType::Number => {
                    let tok = self.lexer.next_token()?;
                    // fd numbers before redirects — only when adjacent (no space)
                    // and never before &> or &>>
                    let fd_end = tok.pos + tok.value.len();
                    let next_pos = self.lexer.peek_token().map(|t| t.pos).unwrap_or(0);
                    let adjacent = fd_end == next_pos;
                    if adjacent
                        && is_fd_number(&tok.value)
                        && self.is_redirect_operator()?
                        && !self.is_and_redirect()?
                    {
                        redirects.push(self.parse_redirect_with_fd(&tok)?);
                    } else if is_varfd(&tok.value) && self.is_redirect_operator()? {
                        redirects.push(self.parse_redirect()?);
                    } else {
                        if words.is_empty() && self.peek_is(TokenType::LeftParen)? {
                            return self.parse_function_def(&tok);
                        }
                        words.push(Node::Word {
                            value: tok.value,
                            parts: Vec::new(),
                        });
                    }
                }
                _ => break,
            }
        }

        if words.is_empty() && redirects.is_empty() {
            return Ok(Node::Empty);
        }

        Ok(Node::Command { words, redirects })
    }

    pub(super) fn parse_redirect(&mut self) -> Result<Node> {
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, -1)
    }

    pub(super) fn parse_redirect_with_fd(&mut self, fd_tok: &Token) -> Result<Node> {
        let fd: i32 = fd_tok
            .value
            .parse()
            .map_err(|_| RableError::parse("invalid fd number", fd_tok.pos, fd_tok.line))?;
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, fd)
    }

    fn build_redirect(&mut self, op_tok: Token, fd: i32) -> Result<Node> {
        if op_tok.kind == TokenType::DoubleLess || op_tok.kind == TokenType::DoubleLessDash {
            let delim_tok = self.lexer.next_token()?;
            let strip_tabs = op_tok.kind == TokenType::DoubleLessDash;
            let (delimiter, quoted) = parse_heredoc_delimiter(&delim_tok.value);
            self.lexer
                .queue_heredoc(delimiter.clone(), strip_tabs, quoted);
            return Ok(Node::HereDoc {
                delimiter,
                content: String::new(),
                strip_tabs,
                quoted,
                fd,
                complete: true,
            });
        }

        // >&- and <&- are complete close-fd operators (no target needed)
        if op_tok.value == ">&-" || op_tok.value == "<&-" {
            return Ok(Node::Redirect {
                op: ">&-".to_string(),
                target: Box::new(word_node("0")),
                fd,
            });
        }

        let target_tok = self.lexer.next_token()?;
        let is_dup = op_tok.kind == TokenType::GreaterAnd || op_tok.kind == TokenType::LessAnd;

        if is_dup && target_tok.value == "-" {
            return Ok(Node::Redirect {
                op: ">&-".to_string(),
                target: Box::new(word_node("0")),
                fd: -1,
            });
        }
        if is_dup && target_tok.value.ends_with('-') {
            let fd_str = &target_tok.value[..target_tok.value.len() - 1];
            return Ok(Node::Redirect {
                op: op_tok.value,
                target: Box::new(word_node(fd_str)),
                fd: -1,
            });
        }

        Ok(Node::Redirect {
            op: op_tok.value,
            target: Box::new(word_node(&target_tok.value)),
            fd,
        })
    }

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

    fn is_double_paren(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        if tok.kind != TokenType::LeftParen {
            return Ok(false);
        }
        Ok(self.lexer.char_after_peeked() == Some('('))
    }

    pub(super) fn parse_trailing_redirects(&mut self) -> Result<Vec<Node>> {
        let mut redirects = Vec::new();
        loop {
            if self.at_end()? {
                break;
            }
            if self.is_redirect_operator()? {
                redirects.push(self.parse_redirect()?);
            } else {
                let tok = self.lexer.peek_token()?;
                if tok.kind == TokenType::Word || tok.kind == TokenType::Number {
                    if is_fd_number(&tok.value) {
                        let tok = self.lexer.next_token()?;
                        if self.is_redirect_operator()? {
                            redirects.push(self.parse_redirect_with_fd(&tok)?);
                            continue;
                        }
                        break;
                    }
                    if is_varfd(&tok.value) {
                        let _varfd = self.lexer.next_token()?;
                        if self.is_redirect_operator()? {
                            redirects.push(self.parse_redirect()?);
                            continue;
                        }
                        break;
                    }
                }
                break;
            }
        }
        Ok(redirects)
    }

    /// Returns true if the next token is `&>` or `&>>` (which never take fd prefixes).
    fn is_and_redirect(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(matches!(
            tok.kind,
            TokenType::AndGreater | TokenType::AndDoubleGreater
        ))
    }

    pub(super) fn is_redirect_operator(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(matches!(
            tok.kind,
            TokenType::Less
                | TokenType::Greater
                | TokenType::DoubleGreater
                | TokenType::LessAnd
                | TokenType::GreaterAnd
                | TokenType::LessGreater
                | TokenType::GreaterPipe
                | TokenType::AndGreater
                | TokenType::AndDoubleGreater
                | TokenType::DoubleLess
                | TokenType::DoubleLessDash
                | TokenType::TripleLess
        ))
    }
}

#[cfg(test)]
mod tests;
