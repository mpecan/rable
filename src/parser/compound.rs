//! Compound command parsers: if, while, until, for, case, select,
//! subshell, brace group, function, coproc, arithmetic command.

use crate::ast::Node;
use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

use super::{
    Parser,
    helpers::{is_fd_number, word_node},
};

impl Parser {
    pub(super) fn parse_if(&mut self) -> Result<Node> {
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

    pub(super) fn parse_elif(&mut self) -> Result<Node> {
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

    pub(super) fn parse_while(&mut self) -> Result<Node> {
        self.parse_loop(TokenType::While, true)
    }

    pub(super) fn parse_until(&mut self) -> Result<Node> {
        self.parse_loop(TokenType::Until, false)
    }

    fn parse_loop(&mut self, keyword: TokenType, is_while: bool) -> Result<Node> {
        self.expect(keyword)?;
        self.skip_newlines()?;
        let condition = self.parse_list()?;
        self.lexer.set_command_start();
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
    pub(super) fn parse_for(&mut self) -> Result<Node> {
        self.expect(TokenType::For)?;

        if self.peek_is(TokenType::LeftParen)? {
            return self.parse_for_arith();
        }

        let var_tok = self.lexer.next_token()?;
        let var = var_tok.value;

        self.lexer.set_command_start();
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
            Some(vec![Node::Word {
                value: "\"$@\"".to_string(),
                parts: Vec::new(),
            }])
        };

        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.set_command_start();
        let (body, redirects) = self.parse_loop_body()?;

        Ok(Node::For {
            var,
            words,
            body: Box::new(body),
            redirects,
        })
    }

    fn parse_for_arith(&mut self) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::LeftParen)?;

        let raw = self.lexer.read_until_double_paren()?;
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

        self.skip_newlines()?;
        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.set_command_start();
        let (body, redirects) = self.parse_loop_body()?;

        Ok(Node::ForArith {
            init,
            cond,
            incr,
            body: Box::new(body),
            redirects,
        })
    }

    /// Shared do/done or {/} loop body parsing.
    fn parse_loop_body(&mut self) -> Result<(Node, Vec<Node>)> {
        if self.peek_is(TokenType::LeftBrace)? {
            let bg = self.parse_brace_group()?;
            let redirects = self.parse_trailing_redirects()?;
            if let Node::BraceGroup { body, .. } = bg {
                Ok((*body, redirects))
            } else {
                Ok((bg, redirects))
            }
        } else {
            self.expect(TokenType::Do)?;
            self.skip_newlines()?;
            let body = self.parse_list()?;
            self.expect(TokenType::Done)?;
            let redirects = self.parse_trailing_redirects()?;
            Ok((body, redirects))
        }
    }

    pub(super) fn parse_case(&mut self) -> Result<Node> {
        self.expect(TokenType::Case)?;
        let word_tok = self.lexer.next_token()?;
        let word = Box::new(Node::Word {
            value: word_tok.value,
            parts: Vec::new(),
        });

        self.lexer.set_command_start();
        self.skip_newlines()?;
        self.expect(TokenType::In)?;
        self.skip_newlines()?;

        let mut patterns = Vec::new();
        self.lexer.set_command_start();
        while !self.peek_is(TokenType::Esac)? && !self.at_end()? {
            patterns.push(self.parse_case_pattern()?);
            self.lexer.set_command_start();
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
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
        }

        let mut pattern_words = Vec::new();
        loop {
            let tok = self.lexer.next_token()?;
            if tok.kind == TokenType::RightParen || tok.kind == TokenType::Eof {
                break;
            }
            if tok.kind == TokenType::Pipe {
                continue;
            }
            pattern_words.push(word_node(&tok.value));
        }

        self.skip_newlines()?;

        let body = if self.peek_is(TokenType::DoubleSemi)?
            || self.peek_is(TokenType::SemiAnd)?
            || self.peek_is(TokenType::SemiSemiAnd)?
            || self.peek_is(TokenType::Esac)?
        {
            None
        } else {
            Some(self.parse_list()?)
        };

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

    pub(super) fn parse_select(&mut self) -> Result<Node> {
        self.expect(TokenType::Select)?;
        let var_tok = self.lexer.next_token()?;
        let var = var_tok.value;

        self.lexer.set_command_start();
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
            Some(vec![word_node("\"$@\"")])
        };

        if self.peek_is(TokenType::Semi)? || self.peek_is(TokenType::Newline)? {
            self.lexer.next_token()?;
        }
        self.skip_newlines()?;
        self.lexer.set_command_start();
        let (body, redirects) = self.parse_loop_body()?;

        Ok(Node::Select {
            var,
            words,
            body: Box::new(body),
            redirects,
        })
    }

    pub(super) fn parse_subshell(&mut self) -> Result<Node> {
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

    pub(super) fn parse_brace_group(&mut self) -> Result<Node> {
        self.expect(TokenType::LeftBrace)?;
        self.skip_newlines()?;
        let body = self.parse_list()?;
        self.expect_brace_close()?;
        let redirects = self.parse_trailing_redirects()?;

        Ok(Node::BraceGroup {
            body: Box::new(body),
            redirects,
        })
    }

    pub(super) fn parse_function(&mut self) -> Result<Node> {
        self.expect(TokenType::Function)?;
        let name_tok = self.lexer.next_token()?;
        let name = name_tok.value;

        self.lexer.set_command_start();
        if self.peek_is(TokenType::LeftParen)? {
            self.lexer.next_token()?;
            if self.peek_is(TokenType::RightParen)? {
                // function f() { ... } — empty parens syntax
                self.lexer.next_token()?;
                self.lexer.set_command_start();
            } else {
                // function f ( cmd ) — subshell body
                self.skip_newlines()?;
                let body = self.parse_list()?;
                self.expect(TokenType::RightParen)?;
                let redirects = self.parse_trailing_redirects()?;
                return Ok(Node::Function {
                    name,
                    body: Box::new(Node::Subshell {
                        body: Box::new(body),
                        redirects,
                    }),
                });
            }
        }

        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(Node::Function {
            name,
            body: Box::new(body),
        })
    }

    pub(super) fn parse_function_def(&mut self, name_tok: &Token) -> Result<Node> {
        self.expect(TokenType::LeftParen)?;
        self.expect(TokenType::RightParen)?;
        self.lexer.set_command_start();
        self.skip_newlines()?;
        let body = self.parse_command()?;

        Ok(Node::Function {
            name: name_tok.value.clone(),
            body: Box::new(body),
        })
    }

    pub(super) fn parse_coproc(&mut self) -> Result<Node> {
        self.expect(TokenType::Coproc)?;

        let tok = self.lexer.peek_token()?;
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

        let first_tok = self.lexer.next_token()?;
        self.lexer.set_command_start();
        let next = self.lexer.peek_token()?;
        let name = if next.kind.starts_command()
            && !matches!(
                next.kind,
                TokenType::Coproc | TokenType::Time | TokenType::Bang
            ) {
            let n = Some(first_tok.value);
            let command = self.parse_command()?;
            return Ok(Node::Coproc {
                name: n,
                command: Box::new(command),
            });
        } else {
            None
        };
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

    pub(super) fn parse_arith_command(&mut self) -> Result<Node> {
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

    pub(super) fn expect_brace_close(&mut self) -> Result<Token> {
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
}
