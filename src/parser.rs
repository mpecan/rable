use crate::ast::Node;
use crate::error::{RableError, Result};
use crate::lexer::Lexer;
use crate::token::{Token, TokenType};

/// Recursive descent parser for bash.
/// Maximum recursion/iteration depth to prevent infinite loops.
const MAX_DEPTH: usize = 1000;

pub struct Parser {
    lexer: Lexer,
    depth: usize,
}

impl Parser {
    pub const fn new(lexer: Lexer) -> Self {
        Self { lexer, depth: 0 }
    }

    /// Increments depth and returns an error if too deep.
    fn enter(&mut self) -> Result<()> {
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

    const fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Parses the entire input, returning a list of top-level nodes.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on syntax errors or unclosed delimiters.
    pub fn parse_all(&mut self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();

        // Skip leading newlines
        self.skip_newlines()?;

        while !self.at_end()? {
            let prev_pos = self.lexer.pos();
            let mut node = self.parse_list()?;
            self.skip_newlines()?;
            // Fill in any pending here-doc contents
            fill_heredoc_contents(&mut node, &mut self.lexer);
            nodes.push(node);
            // Safety: if the parser made no progress, skip a token to avoid loops
            if self.lexer.pos() == prev_pos && !self.at_end()? {
                self.lexer.next_token()?;
            }
        }

        Ok(nodes)
    }

    /// Returns true if the next token is EOF.
    fn at_end(&mut self) -> Result<bool> {
        Ok(self.lexer.peek_token()?.kind == TokenType::Eof)
    }

    /// Skips newline tokens.
    fn skip_newlines(&mut self) -> Result<()> {
        while !self.at_end()? && self.lexer.peek_token()?.kind == TokenType::Newline {
            self.lexer.next_token()?;
        }
        Ok(())
    }

    /// Parses a command list. Precedence (low to high): `;`/`\n` < `&` < `&&`/`||` < `|`.
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

    /// Parses `&` (background) — higher precedence than `;`, lower than `&&`/`||`.
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
            // Check if next is ; or \n (back to parse_list level)
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

    /// Parses `&&` and `||` chains (higher precedence than `&`).
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

    /// Returns true if the current token terminates a list.
    fn is_list_terminator(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        // } and ]] may be tokenized as Word when command_start was false
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

    /// Parses a pipeline (commands separated by `|`).
    fn parse_pipeline(&mut self) -> Result<Node> {
        // Check for `!` negation — can nest with time
        if self.lexer.peek_token()?.kind == TokenType::Bang {
            self.lexer.next_token()?;
            // ! ! cmd → double negation cancels out
            if self.lexer.peek_token()?.kind == TokenType::Bang {
                self.lexer.next_token()?;
                return self.parse_pipeline_inner();
            }
            // ! time ... or just ! pipeline
            let inner = self.parse_pipeline()?;
            return Ok(Node::Negation {
                pipeline: Box::new(inner),
            });
        }

        // Check for `time` — can nest with !
        if self.lexer.peek_token()?.kind == TokenType::Time {
            self.lexer.next_token()?;
            let posix = if self.check_word("-p")? {
                self.lexer.next_token()?;
                true
            } else {
                false
            };
            // time ! cmd → (negation (time (cmd))) — negation wraps time
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

    /// Parses the inner part of a pipeline.
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
                    commands.push(self.parse_command()?);
                }
                TokenType::PipeBoth => {
                    self.lexer.next_token()?;
                    self.skip_newlines()?;
                    // |& adds stderr redirect - for simple commands, add to the command;
                    // for compound commands, add as separate pipeline element
                    if !add_stderr_redirect(commands.last_mut()) {
                        commands.push(make_stderr_redirect());
                    }
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

    /// Checks if the next token is a word with the given value.
    fn check_word(&mut self, expected: &str) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(tok.kind == TokenType::Word && tok.value == expected)
    }

    /// Parses a single command (simple command or compound command).
    fn parse_command(&mut self) -> Result<Node> {
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
                // Check for (( arithmetic command
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
            _ => self.parse_simple_command(),
        }
    }

    /// Parses a simple command (words and redirects).
    fn parse_simple_command(&mut self) -> Result<Node> {
        let mut words = Vec::new();
        let mut redirects = Vec::new();

        loop {
            if self.at_end()? {
                break;
            }
            let tok = self.lexer.peek_token()?;
            match tok.kind {
                // Redirect operators
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
                // Words
                TokenType::Word | TokenType::AssignmentWord | TokenType::Number => {
                    let tok = self.lexer.next_token()?;
                    // Check if this word is a number followed by a redirect
                    if is_fd_number(&tok.value) && self.is_redirect_operator()? {
                        redirects.push(self.parse_redirect_with_fd(&tok)?);
                    } else if is_varfd(&tok.value) && self.is_redirect_operator()? {
                        // Variable fd: {varname}>file — discard the varname
                        redirects.push(self.parse_redirect()?);
                    } else {
                        // Check for function definition: name () { body; }
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

    /// Parses a redirect operator and its target.
    fn parse_redirect(&mut self) -> Result<Node> {
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, -1)
    }

    /// Parses a redirect with an explicit fd number prefix.
    fn parse_redirect_with_fd(&mut self, fd_tok: &Token) -> Result<Node> {
        let fd: i32 = fd_tok
            .value
            .parse()
            .map_err(|_| RableError::parse("invalid fd number", fd_tok.pos, fd_tok.line))?;
        let op_tok = self.lexer.next_token()?;
        self.build_redirect(op_tok, fd)
    }

    /// Shared redirect-building logic for both `parse_redirect` and `parse_redirect_with_fd`.
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

        let target_tok = self.lexer.next_token()?;
        let is_dup = op_tok.kind == TokenType::GreaterAnd || op_tok.kind == TokenType::LessAnd;

        // fd close: >&- or <&-  →  normalize to >&-
        if is_dup && target_tok.value == "-" {
            return Ok(Node::Redirect {
                op: ">&-".to_string(),
                target: Box::new(word_node("0")),
                fd: -1,
            });
        }
        // fd move: >&N- or <&N-  →  strip trailing -
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

    /// Returns true if the next token has the given type.
    fn peek_is(&mut self, kind: TokenType) -> Result<bool> {
        Ok(self.lexer.peek_token()?.kind == kind)
    }

    /// Expects and consumes a token of the given type.
    fn expect(&mut self, kind: TokenType) -> Result<Token> {
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

    // -- Compound command parsers --

    fn parse_if(&mut self) -> Result<Node> {
        self.expect(TokenType::If)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.expect(TokenType::Then)?;
        self.skip_newlines()?;
        let then_body = self.parse_list()?;

        let else_body = if self.peek_is(TokenType::Elif)? {
            Some(Box::new(self.parse_elif()?))
        } else if self.peek_is(TokenType::Else)? {
            self.lexer.next_token()?;
            self.skip_newlines()?;
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };

        self.expect(TokenType::Fi)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::If {
            condition: Box::new(condition),
            then_body: Box::new(then_body),
            else_body,
            redirects,
        })
    }

    fn parse_elif(&mut self) -> Result<Node> {
        self.enter()?;
        self.expect(TokenType::Elif)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.expect(TokenType::Then)?;
        self.skip_newlines()?;
        let then_body = self.parse_list()?;

        let else_body = if self.peek_is(TokenType::Elif)? {
            Some(Box::new(self.parse_elif()?))
        } else if self.peek_is(TokenType::Else)? {
            self.lexer.next_token()?;
            self.skip_newlines()?;
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };

        self.leave();
        Ok(Node::If {
            condition: Box::new(condition),
            then_body: Box::new(then_body),
            else_body,
            redirects: Vec::new(),
        })
    }

    fn parse_while(&mut self) -> Result<Node> {
        self.parse_loop(TokenType::While, true)
    }

    fn parse_until(&mut self) -> Result<Node> {
        self.parse_loop(TokenType::Until, false)
    }

    /// Shared logic for `while` and `until` loops.
    fn parse_loop(&mut self, keyword: TokenType, is_while: bool) -> Result<Node> {
        self.expect(keyword)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.lexer.state.command_start = true;
        self.expect(TokenType::Do)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect(TokenType::Done)?;
        let redirects = self.parse_trailing_redirects()?;

        let condition = Box::new(condition);
        let body = Box::new(body);
        if is_while {
            Ok(Node::While {
                condition,
                body,
                redirects,
            })
        } else {
            Ok(Node::Until {
                condition,
                body,
                redirects,
            })
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_for(&mut self) -> Result<Node> {
        self.expect(TokenType::For)?;

        // Check for C-style for: for (( init; cond; incr ))
        if self.peek_is(TokenType::LeftParen)? {
            return self.parse_for_arith();
        }

        let var_tok = self.lexer.next_token()?;
        let var = var_tok.value;

        // 'in' is a reserved word only recognized at command start
        self.lexer.state.command_start = true;
        self.skip_newlines()?;
        let words = if self.peek_is(TokenType::In)? {
            self.lexer.next_token()?; // consume 'in'
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
                ws.push(word_node(&tok.value));
            }
            Some(ws)
        } else {
            // No 'in' clause: default to "$@"
            Some(vec![Node::Word {
                value: "\"$@\"".to_string(),
                parts: Vec::new(),
            }])
        };

        // Consume ; or newline before do
        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.state.command_start = true;
        // Accept either do/done or {/} for loop body
        let (body, redirects) = if self.peek_is(TokenType::LeftBrace)? {
            let bg = self.parse_brace_group()?;
            let redirects = self.parse_trailing_redirects()?;
            // Extract body from brace group
            if let Node::BraceGroup { body, .. } = bg {
                (*body, redirects)
            } else {
                (bg, redirects)
            }
        } else {
            self.expect(TokenType::Do)?;
            self.skip_newlines()?;
            let body = self.parse_list()?;
            self.expect(TokenType::Done)?;
            let redirects = self.parse_trailing_redirects()?;
            (body, redirects)
        };

        Ok(Node::For {
            var,
            words,
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_for_arith(&mut self) -> Result<Node> {
        // Read raw text between (( and )), split by ;
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::LeftParen)?;

        // Read raw text until ))
        let raw = self.lexer.read_until_double_paren()?;
        // Split by ; into init, cond, incr — empty parts default to "1"
        let parts: Vec<&str> = raw.splitn(3, ';').collect();
        let default_empty = |s: &str| -> String {
            let trimmed = s.trim_start().to_string();
            if trimmed.is_empty() {
                "1".to_string()
            } else {
                trimmed
            }
        };
        let init = default_empty(parts.first().unwrap_or(&""));
        let cond = default_empty(parts.get(1).unwrap_or(&""));
        let incr = default_empty(parts.get(2).unwrap_or(&""));

        // Consume ; or newline before do/{
        self.skip_newlines()?;
        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.state.command_start = true;

        // Accept either do/done or {/} for loop body
        let (body, redirects) = if self.peek_is(TokenType::LeftBrace)? {
            let bg = self.parse_brace_group()?;
            let redirects = self.parse_trailing_redirects()?;
            if let Node::BraceGroup { body, .. } = bg {
                (*body, redirects)
            } else {
                (bg, redirects)
            }
        } else {
            self.expect(TokenType::Do)?;
            self.skip_newlines()?;
            let body = self.parse_list()?;
            self.expect(TokenType::Done)?;
            let redirects = self.parse_trailing_redirects()?;
            (body, redirects)
        };

        Ok(Node::ForArith {
            init,
            cond,
            incr,
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_case(&mut self) -> Result<Node> {
        self.expect(TokenType::Case)?;
        let word_tok = self.lexer.next_token()?;
        let word = Box::new(Node::Word {
            value: word_tok.value,
            parts: Vec::new(),
        });

        self.lexer.state.command_start = true;
        self.skip_newlines()?;
        self.expect(TokenType::In)?;
        self.skip_newlines()?;

        let mut patterns = Vec::new();
        self.lexer.state.command_start = true;
        while !self.peek_is(TokenType::Esac)? && !self.at_end()? {
            patterns.push(self.parse_case_pattern()?);
            self.lexer.state.command_start = true;
            self.skip_newlines()?;
        }

        self.expect(TokenType::Esac)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::Case {
            word,
            patterns,
            redirects,
        })
    }

    fn parse_case_pattern(&mut self) -> Result<crate::ast::CasePattern> {
        // Optional leading (
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
        }

        // Read pattern words until ), separated by |
        let mut pattern_words = Vec::new();
        loop {
            let tok = self.lexer.next_token()?;
            if tok.kind == TokenType::RightParen || tok.kind == TokenType::Eof {
                break;
            }
            // Skip | separators between patterns
            if tok.kind == TokenType::Pipe {
                continue;
            }
            pattern_words.push(word_node(&tok.value));
        }

        self.skip_newlines()?;

        // Read body until ;; or ;& or ;;& or esac
        let body = if self.peek_is(TokenType::DoubleSemi)?
            || self.peek_is(TokenType::SemiAnd)?
            || self.peek_is(TokenType::SemiSemiAnd)?
            || self.peek_is(TokenType::Esac)?
        {
            None
        } else {
            Some(self.parse_list()?)
        };

        // Read terminator
        let terminator = if self.peek_is(TokenType::DoubleSemi)? {
            self.lexer.next_token()?;
            ";;".to_string()
        } else if self.peek_is(TokenType::SemiAnd)? {
            self.lexer.next_token()?;
            ";&".to_string()
        } else if self.peek_is(TokenType::SemiSemiAnd)? {
            self.lexer.next_token()?;
            ";;&".to_string()
        } else {
            ";;".to_string()
        };

        Ok(crate::ast::CasePattern::new(
            pattern_words,
            body,
            terminator,
        ))
    }

    fn parse_select(&mut self) -> Result<Node> {
        self.expect(TokenType::Select)?;
        let var_tok = self.lexer.next_token()?;
        let var = var_tok.value;

        self.lexer.state.command_start = true;
        self.skip_newlines()?;

        let words = if self.peek_is(TokenType::In)? {
            self.lexer.next_token()?;
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
                ws.push(word_node(&tok.value));
            }
            Some(ws)
        } else {
            // No 'in' clause: default to "$@"
            Some(vec![word_node("\"$@\"")])
        };

        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.state.command_start = true;
        // Accept either do/done or {/} for body
        let (body, redirects) = if self.peek_is(TokenType::LeftBrace)? {
            let bg = self.parse_brace_group()?;
            let redirects = self.parse_trailing_redirects()?;
            if let Node::BraceGroup { body, .. } = bg {
                (*body, redirects)
            } else {
                (bg, redirects)
            }
        } else {
            self.expect(TokenType::Do)?;
            self.skip_newlines()?;
            let body = self.parse_list()?;
            self.expect(TokenType::Done)?;
            let redirects = self.parse_trailing_redirects()?;
            (body, redirects)
        };

        Ok(Node::Select {
            var,
            words,
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_subshell(&mut self) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect(TokenType::RightParen)?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::Subshell {
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_brace_group(&mut self) -> Result<Node> {
        self.expect(TokenType::LeftBrace)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        // } may be Word if command_start was false
        self.expect_brace_close()?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::BraceGroup {
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_function(&mut self) -> Result<Node> {
        self.expect(TokenType::Function)?;
        let name_tok = self.lexer.next_token()?;
        let name = name_tok.value;

        // Optional ()
        self.lexer.state.command_start = true;
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
            self.expect(TokenType::RightParen)?;
            self.lexer.state.command_start = true;
        }

        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(Node::Function {
            name,
            body: Box::new(body),
        })
    }

    /// Parses `name () { body; }` function definition form.
    fn parse_function_def(&mut self, name_tok: &Token) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::RightParen)?;
        self.lexer.state.command_start = true;
        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(Node::Function {
            name: name_tok.value.clone(),
            body: Box::new(body),
        })
    }

    fn parse_coproc(&mut self) -> Result<Node> {
        self.expect(TokenType::Coproc)?;

        let tok = self.lexer.peek_token()?;
        // If next token is a compound command keyword, no name
        if tok.kind.starts_command()
            && !matches!(
                tok.kind,
                TokenType::Coproc | TokenType::Time | TokenType::Bang
            )
        {
            let command = self.parse_command()?;
            return Ok(Node::Coproc {
                name: None,
                command: Box::new(command),
            });
        }

        // Read the first word, then check if the NEXT token is a compound command
        let first_tok = self.lexer.next_token()?;
        self.lexer.state.command_start = true;
        let next = self.lexer.peek_token()?;
        let name = if next.kind.starts_command()
            && !matches!(
                next.kind,
                TokenType::Coproc | TokenType::Time | TokenType::Bang
            ) {
            // First word is the name, next is the compound command
            let n = Some(first_tok.value);
            let command = self.parse_command()?;
            return Ok(Node::Coproc {
                name: n,
                command: Box::new(command),
            });
        } else {
            // First word is part of the command, name defaults to COPROC
            None
        };
        // Parse the rest as a simple command starting with first_tok
        let mut words = vec![word_node(&first_tok.value)];
        let mut redirects = Vec::new();
        loop {
            if self.at_end()? {
                break;
            }
            if self.is_redirect_operator()? {
                redirects.push(self.parse_redirect()?);
                continue;
            }
            let tok = self.lexer.peek_token()?;
            if matches!(tok.kind, TokenType::Word | TokenType::Number) {
                let tok = self.lexer.next_token()?;
                if is_fd_number(&tok.value) && self.is_redirect_operator()? {
                    redirects.push(self.parse_redirect_with_fd(&tok)?);
                } else {
                    words.push(word_node(&tok.value));
                }
            } else {
                break;
            }
        }
        Ok(Node::Coproc {
            name,
            command: Box::new(Node::Command { words, redirects }),
        })
    }

    fn parse_cond_command(&mut self) -> Result<Node> {
        self.expect(TokenType::DoubleLeftBracket)?;
        self.lexer.state.cond_expr = true;

        let body = self.parse_cond_or()?;

        self.lexer.state.cond_expr = false;
        // ]] may have been read as Word if command_start was false
        self.expect_cond_close()?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::ConditionalExpr {
            body: Box::new(body),
            redirects,
        })
    }

    /// Parses `||` in conditional expressions (lowest precedence).
    fn parse_cond_or(&mut self) -> Result<Node> {
        let mut left = self.parse_cond_and()?;
        while !self.is_cond_close()? && self.peek_is(TokenType::Or)? {
            self.lexer.next_token()?;
            let right = self.parse_cond_and()?;
            left = Node::CondOr {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parses `&&` in conditional expressions.
    fn parse_cond_and(&mut self) -> Result<Node> {
        let mut left = self.parse_cond_primary()?;
        while !self.is_cond_close()? && self.peek_is(TokenType::And)? {
            self.lexer.next_token()?;
            let right = self.parse_cond_primary()?;
            left = Node::CondAnd {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parses a primary conditional expression.
    fn parse_cond_primary(&mut self) -> Result<Node> {
        let tok = self.lexer.peek_token()?;

        // Handle ! (negation) — Parable drops the negation
        if tok.kind == TokenType::Bang {
            self.lexer.next_token()?;
            return self.parse_cond_primary();
        }

        // Handle ( grouped expression )
        if tok.kind == TokenType::LeftParen {
            self.lexer.next_token()?;
            let inner = self.parse_cond_or()?;
            self.expect(TokenType::RightParen)?;
            return Ok(Node::CondParen {
                inner: Box::new(inner),
            });
        }

        // Read the first word
        let first = self.lexer.next_token()?;

        // Check for unary operators: -f, -d, -z, -n, etc.
        if first.value.starts_with('-')
            && first.value.len() <= 3
            && self.peek_cond_term()?.is_some()
        {
            let operand_tok = self.lexer.next_token()?;
            return Ok(Node::UnaryTest {
                op: first.value,
                operand: Box::new(cond_term(&operand_tok.value)),
            });
        }

        // Check for binary operators: ==, !=, =~, -eq, -ne, -lt, etc.
        if !self.is_cond_close()?
            && !self.peek_is(TokenType::And)?
            && !self.peek_is(TokenType::Or)?
        {
            let op_tok = self.lexer.peek_token()?;
            let is_binary = is_cond_binary_op(&op_tok.value)
                || op_tok.kind == TokenType::Less
                || op_tok.kind == TokenType::Greater;
            if is_binary {
                let op = self.lexer.next_token()?;
                let right = self.lexer.next_token()?;
                return Ok(Node::BinaryTest {
                    op: op.value,
                    left: Box::new(cond_term(&first.value)),
                    right: Box::new(cond_term(&right.value)),
                });
            }
        }

        // Bare word: implicit -n test
        Ok(Node::UnaryTest {
            op: "-n".to_string(),
            operand: Box::new(cond_term(&first.value)),
        })
    }

    /// Expects `}` closing token (may be `Word` or `RightBrace`).
    fn expect_brace_close(&mut self) -> Result<Token> {
        let tok = self.lexer.next_token()?;
        if tok.kind == TokenType::RightBrace || (tok.kind == TokenType::Word && tok.value == "}") {
            Ok(tok)
        } else {
            Err(RableError::parse(
                format!("expected }}, got {:?}", tok.value),
                tok.pos,
                tok.line,
            ))
        }
    }

    /// Expects `]]` closing token (may be `Word` or `DoubleRightBracket`).
    fn expect_cond_close(&mut self) -> Result<Token> {
        let tok = self.lexer.next_token()?;
        if tok.kind == TokenType::DoubleRightBracket
            || (tok.kind == TokenType::Word && tok.value == "]]")
        {
            Ok(tok)
        } else {
            Err(RableError::parse(
                format!("expected ]], got {:?}", tok.value),
                tok.pos,
                tok.line,
            ))
        }
    }

    /// Returns true if the next token is `]]` (either as keyword or word).
    fn is_cond_close(&mut self) -> Result<bool> {
        let tok = self.lexer.peek_token()?;
        Ok(tok.kind == TokenType::DoubleRightBracket
            || (tok.kind == TokenType::Word && tok.value == "]]"))
    }

    /// Peeks to see if the next token could be a conditional term.
    fn peek_cond_term(&mut self) -> Result<Option<()>> {
        if self.is_cond_close()? {
            return Ok(None);
        }
        let tok = self.lexer.peek_token()?;
        if matches!(tok.kind, TokenType::And | TokenType::Or) {
            Ok(None)
        } else {
            Ok(Some(()))
        }
    }

    /// Checks if the next two tokens are both `(`.
    fn is_double_paren(&mut self) -> Result<bool> {
        // The first ( is already peeked. Check if the char right after
        // the current peeked token position is also (
        let tok = self.lexer.peek_token()?;
        if tok.kind != TokenType::LeftParen {
            return Ok(false);
        }
        // Look at the char right after ( in the input
        Ok(self.lexer.char_after_peeked() == Some('('))
    }

    /// Parses `(( expr ))` arithmetic command.
    fn parse_arith_command(&mut self) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::LeftParen)?;
        let content = self.lexer.read_until_double_paren()?;
        let redirects = self.parse_trailing_redirects()?;
        Ok(Node::ArithmeticCommand {
            expression: None,
            redirects,
            raw_content: content,
        })
    }

    /// Parses any trailing redirects after a compound command.
    fn parse_trailing_redirects(&mut self) -> Result<Vec<Node>> {
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

    fn is_redirect_operator(&mut self) -> Result<bool> {
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

/// Creates a `cond-term` node for conditional expressions.
/// Creates a `Word` node with no parts.
fn word_node(value: &str) -> Node {
    Node::Word {
        value: value.to_string(),
        parts: Vec::new(),
    }
}

fn cond_term(value: &str) -> Node {
    Node::CondTerm {
        value: value.to_string(),
    }
}

/// Adds a `(redirect ">&" 1)` to a command node for `|&` pipe-both.
/// Returns true if the redirect was added to the command's redirects,
/// false if the node is not a simple command (compound commands need
/// the redirect as a separate pipeline element).
#[allow(clippy::needless_pass_by_value)]
fn add_stderr_redirect(node: Option<&mut Node>) -> bool {
    if let Some(Node::Command { redirects, .. }) = node {
        redirects.push(make_stderr_redirect());
        true
    } else {
        false
    }
}

/// Creates a `(redirect ">&" 1)` node.
fn make_stderr_redirect() -> Node {
    Node::Redirect {
        op: ">&".to_string(),
        target: Box::new(Node::Word {
            value: "1".to_string(),
            parts: Vec::new(),
        }),
        fd: -1,
    }
}

/// Walks an AST node and fills in empty `HereDoc` content from the lexer queue.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
fn fill_heredoc_contents(node: &mut Node, lexer: &mut Lexer) {
    match node {
        Node::HereDoc { content, .. } if content.is_empty() => {
            if let Some(c) = lexer.take_heredoc_content() {
                *content = c;
            }
        }
        Node::Command { words, redirects } => {
            for w in words {
                fill_heredoc_contents(w, lexer);
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Pipeline { commands } => {
            for c in commands {
                fill_heredoc_contents(c, lexer);
            }
        }
        Node::List { parts } => {
            for p in parts {
                fill_heredoc_contents(p, lexer);
            }
        }
        Node::If {
            condition,
            then_body,
            else_body,
            redirects,
        } => {
            fill_heredoc_contents(condition, lexer);
            fill_heredoc_contents(then_body, lexer);
            if let Some(eb) = else_body {
                fill_heredoc_contents(eb, lexer);
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::While {
            condition,
            body,
            redirects,
        }
        | Node::Until {
            condition,
            body,
            redirects,
        } => {
            fill_heredoc_contents(condition, lexer);
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Subshell { body, redirects } | Node::BraceGroup { body, redirects } => {
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::For {
            body, redirects, ..
        }
        | Node::Select {
            body, redirects, ..
        } => {
            fill_heredoc_contents(body, lexer);
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Case {
            patterns,
            redirects,
            ..
        } => {
            for p in patterns {
                if let Some(body) = &mut p.body {
                    fill_heredoc_contents(body, lexer);
                }
            }
            for r in redirects {
                fill_heredoc_contents(r, lexer);
            }
        }
        Node::Negation { pipeline } | Node::Time { pipeline, .. } => {
            fill_heredoc_contents(pipeline, lexer);
        }
        Node::Function { body, .. } | Node::Coproc { command: body, .. } => {
            fill_heredoc_contents(body, lexer);
        }
        _ => {}
    }
}

/// Parses a here-document delimiter, stripping quotes if present.
/// Returns (delimiter, quoted).
#[allow(clippy::option_if_let_else)]
fn parse_heredoc_delimiter(raw: &str) -> (String, bool) {
    // Strip all quotes from the delimiter. If any quotes were present, it's quoted.
    let mut result = String::new();
    let mut quoted = false;
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = true;
                // Read until closing '
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    result.push(c);
                }
            }
            '"' => {
                quoted = true;
                // Read until closing "
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    result.push(c);
                }
            }
            '\\' => {
                quoted = true;
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(c),
        }
    }
    (result, quoted)
}

/// Returns true if the string is a conditional binary operator.
fn is_cond_binary_op(s: &str) -> bool {
    matches!(
        s,
        "==" | "!="
            | "=~"
            | "<"
            | ">"
            | "-eq"
            | "-ne"
            | "-lt"
            | "-le"
            | "-gt"
            | "-ge"
            | "-nt"
            | "-ot"
            | "-ef"
            | "="
    )
}

/// Returns true if the string is a valid file descriptor number.
fn is_fd_number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit())
}

/// Returns true if the string is a variable fd reference like `{varname}`.
fn is_varfd(s: &str) -> bool {
    s.starts_with('{')
        && s.ends_with('}')
        && s.len() >= 3
        && s[1..s.len() - 1]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    fn parse(source: &str) -> Vec<Node> {
        let lexer = Lexer::new(source, false);
        let mut parser = Parser::new(lexer);
        parser.parse_all().unwrap()
    }

    #[test]
    fn simple_command() {
        let nodes = parse("echo hello");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(command (word "echo") (word "hello"))"#);
    }

    #[test]
    fn pipeline() {
        let nodes = parse("ls | grep foo");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert_eq!(
            output,
            r#"(pipe (command (word "ls")) (command (word "grep") (word "foo")))"#
        );
    }

    #[test]
    fn and_list() {
        let nodes = parse("a && b");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(and (command (word "a")) (command (word "b")))"#);
    }

    #[test]
    fn or_list() {
        let nodes = parse("a || b");
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(or (command (word "a")) (command (word "b")))"#);
    }

    #[test]
    fn redirect_output() {
        let nodes = parse("echo hello > file.txt");
        let output = format!("{}", nodes[0]);
        assert_eq!(
            output,
            r#"(command (word "echo") (word "hello") (redirect ">" "file.txt"))"#
        );
    }

    #[test]
    fn if_then_fi() {
        let nodes = parse("if true; then echo yes; fi");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(if "));
    }

    #[test]
    fn while_loop() {
        let nodes = parse("while true; do echo yes; done");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(while "));
    }

    #[test]
    fn for_loop() {
        let nodes = parse("for x in a b c; do echo $x; done");
        assert_eq!(nodes.len(), 1);
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(for "));
    }

    #[test]
    fn subshell() {
        let nodes = parse("(echo hello)");
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(subshell "));
    }

    #[test]
    fn brace_group() {
        let nodes = parse("{ echo hello; }");
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(brace-group "));
    }

    #[test]
    fn negation() {
        let nodes = parse("! true");
        let output = format!("{}", nodes[0]);
        assert!(output.starts_with("(negation "));
    }

    #[test]
    fn cstyle_for() {
        let nodes = parse("for ((i=0; i<10; i++)); do echo $i; done");
        let output = format!("{}", nodes[0]);
        let expected = r#"(arith-for (init (word "i=0")) (test (word "i<10")) (step (word "i++")) (command (word "echo") (word "$i")))"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn background() {
        let nodes = parse("echo foo &");
        let output = format!("{}", nodes[0]);
        assert_eq!(
            output,
            r#"(background (command (word "echo") (word "foo")))"#
        );
    }

    #[test]
    fn conditional_expr() {
        let nodes = parse("[[ -f file ]]");
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(cond (cond-unary "-f" (cond-term "file")))"#);
    }

    #[test]
    fn cmdsub_while_reformat() {
        let nodes = parse("echo $(while false; do echo x; done)");
        let output = format!("{}", nodes[0]);
        assert_eq!(
            output,
            r#"(command (word "echo") (word "$(while false; do\n    echo x;\ndone)"))"#,
        );
    }

    #[test]
    fn cmdsub_if_else_reformat() {
        let nodes = parse("echo $(if true; then echo yes; else echo no; fi)");
        let output = format!("{}", nodes[0]);
        assert_eq!(
            output,
            r#"(command (word "echo") (word "$(if true; then\n    echo yes;\nelse\n    echo no;\nfi)"))"#,
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extglob_star() {
        let lexer = Lexer::new("*(a|b)", true);
        let mut parser = Parser::new(lexer);
        let nodes = parser.parse_all().unwrap();
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(command (word "*(a|b)"))"#);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extglob_star_in_case() {
        let nodes =
            crate::parse("# @extglob\ncase $x in *(a|b|c)) echo match;; esac", true).unwrap();
        let output = format!("{}", nodes[0]);
        assert!(
            output.contains(r#"(word "*(a|b|c)")"#),
            "expected extglob word, got: {output}"
        );
    }

    #[test]
    fn arith_command() {
        let nodes = parse("((x = 5))");
        let output = format!("{}", nodes[0]);
        assert_eq!(output, r#"(arith (word "x = 5"))"#);
    }
}
