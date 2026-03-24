use crate::error::{RableError, Result};
use crate::token::{Token, TokenType};

/// Parser state flags that affect lexer behavior.
///
/// Bash tokenization is context-sensitive — the parser must inform the lexer
/// about the current parsing context.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ParserStateFlags {
    /// Inside a case pattern.
    pub case_pattern: bool,
    /// Inside a command substitution.
    pub command_subst: bool,
    /// Inside a conditional expression `[[ ]]`.
    pub cond_expr: bool,
    /// Inside an arithmetic expression.
    pub arith: bool,
    /// Assignment words are allowed in this position.
    pub assign_ok: bool,
    /// We are at the start of a command (reserved words are recognized).
    pub command_start: bool,
    /// Extglob is enabled.
    pub extglob: bool,
}

/// Pending here-document to be read after the current line.
#[derive(Debug, Clone)]
pub struct PendingHereDoc {
    pub delimiter: String,
    pub strip_tabs: bool,
    pub quoted: bool,
}

/// Hand-written context-sensitive lexer for bash.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    peeked: Option<Token>,
    pub state: ParserStateFlags,
    /// Pending here-documents to be read after the next newline.
    pub pending_heredocs: Vec<PendingHereDoc>,
    /// Completed here-document contents (filled after newline).
    pub heredoc_contents: Vec<String>,
}

impl Lexer {
    pub fn new(source: &str, extglob: bool) -> Self {
        Self {
            input: source.chars().collect(),
            pos: 0,
            line: 1,
            peeked: None,
            state: ParserStateFlags {
                command_start: true,
                extglob,
                ..ParserStateFlags::default()
            },
            pending_heredocs: Vec::new(),
            heredoc_contents: Vec::new(),
        }
    }

    /// Queues a here-document to be read after the next newline.
    pub fn queue_heredoc(&mut self, delimiter: String, strip_tabs: bool, quoted: bool) {
        self.pending_heredocs.push(PendingHereDoc {
            delimiter,
            strip_tabs,
            quoted,
        });
    }

    /// Reads all pending here-document bodies after a newline.
    fn read_pending_heredocs(&mut self) {
        let pending: Vec<_> = self.pending_heredocs.drain(..).collect();
        for hd in pending {
            let content = self.read_heredoc_body(&hd.delimiter, hd.strip_tabs);
            self.heredoc_contents.push(content);
        }
    }

    /// Reads a here-document body until the delimiter line.
    fn read_heredoc_body(&mut self, delimiter: &str, strip_tabs: bool) -> String {
        let mut content = String::new();
        loop {
            if self.at_end() {
                break;
            }
            // Read a line
            let mut line = String::new();
            let mut prev_backslash = false;
            while let Some(c) = self.peek_char() {
                self.advance_char();
                if c == '\n' {
                    break;
                }
                // Line continuation: \<newline> joins lines (but not \\<newline>)
                if c == '\\' && !prev_backslash && self.peek_char() == Some('\n') {
                    self.advance_char();
                    prev_backslash = false;
                    continue;
                }
                prev_backslash = c == '\\' && !prev_backslash;
                line.push(c);
            }
            // Check if this line matches the delimiter
            let check_line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };
            if check_line == delimiter {
                break;
            }
            if strip_tabs {
                content.push_str(line.trim_start_matches('\t'));
            } else {
                content.push_str(&line);
            }
            content.push('\n');
        }
        content
    }

    /// Takes the next completed here-doc content, if any.
    pub fn take_heredoc_content(&mut self) -> Option<String> {
        if self.heredoc_contents.is_empty() {
            None
        } else {
            Some(self.heredoc_contents.remove(0))
        }
    }

    /// Returns the current position.
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the current line number.
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Returns the total input length.
    pub const fn input_len(&self) -> usize {
        self.input.len()
    }

    /// Returns the character right after the current position (after peeked token).
    /// Used to detect `((` — the first `(` is peeked as `LeftParen`,
    /// and we check if the next raw character is also `(`.
    pub fn char_after_peeked(&self) -> Option<char> {
        // The peeked token consumed `(` at self.pos-1 (or wherever)
        // We need to check the char at the current pos
        self.input.get(self.pos).copied()
    }

    /// Returns true if at end of input.
    pub const fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Peeks at the current character without advancing.
    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Advances one character and returns it.
    fn advance_char(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
            }
        }
        ch
    }

    /// Skips blanks (spaces and tabs) and line continuations (`\<newline>`).
    fn skip_blanks(&mut self) {
        loop {
            match self.peek_char() {
                Some(' ' | '\t') => {
                    self.advance_char();
                }
                Some('\\') => {
                    // Line continuation: \<newline> → skip both
                    if self.input.get(self.pos + 1) == Some(&'\n') {
                        self.advance_char(); // skip \
                        self.advance_char(); // skip \n
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    /// Reads raw text until `))` for C-style for loops.
    ///
    /// # Errors
    ///
    /// Returns `RableError` if `))` is not found.
    pub fn read_until_double_paren(&mut self) -> Result<String> {
        // Clear any peeked token since we're reading raw
        self.peeked = None;
        let mut result = String::new();
        let mut depth = 0i32;
        loop {
            match self.peek_char() {
                Some(')') if depth == 0 => {
                    self.advance_char();
                    if self.peek_char() == Some(')') {
                        self.advance_char();
                        return Ok(result);
                    }
                    result.push(')');
                }
                Some('(') => {
                    self.advance_char();
                    depth += 1;
                    result.push('(');
                }
                Some(')') => {
                    self.advance_char();
                    depth -= 1;
                    result.push(')');
                }
                Some(c) => {
                    self.advance_char();
                    result.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated ((",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Skips a comment (from `#` to end of line).
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            self.advance_char();
        }
    }

    /// Returns the next token, consuming it.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on unterminated quotes or unexpected input.
    pub fn next_token(&mut self) -> Result<Token> {
        if let Some(tok) = self.peeked.take() {
            return Ok(tok);
        }
        self.read_token()
    }

    /// Peeks at the next token without consuming it.
    ///
    /// # Errors
    ///
    /// Returns `RableError` on unterminated quotes or unexpected input.
    pub fn peek_token(&mut self) -> Result<&Token> {
        if self.peeked.is_none() {
            let tok = self.read_token()?;
            self.peeked = Some(tok);
        }
        // SAFETY: we just set it above
        self.peeked
            .as_ref()
            .ok_or_else(|| RableError::parse("unexpected end of input", self.pos, self.line))
    }

    /// Core tokenization: reads the next token from input.
    fn read_token(&mut self) -> Result<Token> {
        self.skip_blanks();

        // Skip comments — # starts a comment anywhere after whitespace
        if self.peek_char() == Some('#') {
            self.skip_comment();
            self.skip_blanks();
        }

        if self.at_end() {
            return Ok(Token::eof(self.pos, self.line));
        }

        let start = self.pos;
        let line = self.line;
        let ch = self
            .peek_char()
            .ok_or_else(|| RableError::parse("unexpected end of input", self.pos, self.line))?;

        match ch {
            '\n' => {
                self.advance_char();
                self.state.command_start = true;
                // Read any pending here-documents after the newline
                if !self.pending_heredocs.is_empty() {
                    self.read_pending_heredocs();
                }
                Ok(Token::new(TokenType::Newline, "\n", start, line))
            }
            '|' => self.read_pipe_operator(start, line),
            '&' => self.read_ampersand_operator(start, line),
            ';' => self.read_semicolon_operator(start, line),
            '(' => {
                self.advance_char();
                Ok(Token::new(TokenType::LeftParen, "(", start, line))
            }
            ')' => {
                self.advance_char();
                Ok(Token::new(TokenType::RightParen, ")", start, line))
            }
            '<' => {
                // Check for <( process substitution
                if self.input.get(self.pos + 1) == Some(&'(') {
                    self.read_process_sub_word(start, line)
                } else {
                    self.read_less_operator(start, line)
                }
            }
            '>' => {
                // Check for >( process substitution
                if self.input.get(self.pos + 1) == Some(&'(') {
                    self.read_process_sub_word(start, line)
                } else {
                    self.read_greater_operator(start, line)
                }
            }
            _ => self.read_word_token(start, line),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn read_pipe_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '|'
        match self.peek_char() {
            Some('|') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::Or, "||", start, line))
            }
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::PipeBoth, "|&", start, line))
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Pipe, "|", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn read_ampersand_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '&'
        match self.peek_char() {
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::And, "&&", start, line))
            }
            Some('>') => {
                self.advance_char();
                if self.peek_char() == Some('>') {
                    self.advance_char();
                    Ok(Token::new(TokenType::AndDoubleGreater, "&>>", start, line))
                } else {
                    Ok(Token::new(TokenType::AndGreater, "&>", start, line))
                }
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Ampersand, "&", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn read_semicolon_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume ';'
        match self.peek_char() {
            Some(';') => {
                self.advance_char();
                self.state.command_start = true;
                if self.peek_char() == Some('&') {
                    self.advance_char();
                    Ok(Token::new(TokenType::SemiSemiAnd, ";;&", start, line))
                } else {
                    Ok(Token::new(TokenType::DoubleSemi, ";;", start, line))
                }
            }
            Some('&') => {
                self.advance_char();
                self.state.command_start = true;
                Ok(Token::new(TokenType::SemiAnd, ";&", start, line))
            }
            _ => {
                self.state.command_start = true;
                Ok(Token::new(TokenType::Semi, ";", start, line))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn read_less_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '<'
        match self.peek_char() {
            Some('<') => {
                self.advance_char();
                match self.peek_char() {
                    Some('-') => {
                        self.advance_char();
                        Ok(Token::new(TokenType::DoubleLessDash, "<<-", start, line))
                    }
                    Some('<') => {
                        self.advance_char();
                        Ok(Token::new(TokenType::TripleLess, "<<<", start, line))
                    }
                    _ => Ok(Token::new(TokenType::DoubleLess, "<<", start, line)),
                }
            }
            Some('&') => {
                self.advance_char();
                Ok(Token::new(TokenType::LessAnd, "<&", start, line))
            }
            Some('>') => {
                self.advance_char();
                Ok(Token::new(TokenType::LessGreater, "<>", start, line))
            }
            _ => Ok(Token::new(TokenType::Less, "<", start, line)),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn read_greater_operator(&mut self, start: usize, line: usize) -> Result<Token> {
        self.advance_char(); // consume '>'
        match self.peek_char() {
            Some('>') => {
                self.advance_char();
                Ok(Token::new(TokenType::DoubleGreater, ">>", start, line))
            }
            Some('&') => {
                self.advance_char();
                Ok(Token::new(TokenType::GreaterAnd, ">&", start, line))
            }
            Some('|') => {
                self.advance_char();
                Ok(Token::new(TokenType::GreaterPipe, ">|", start, line))
            }
            _ => Ok(Token::new(TokenType::Greater, ">", start, line)),
        }
    }

    /// Reads a process substitution into an existing word value.
    fn read_process_sub_into(&mut self, value: &mut String) -> Result<()> {
        let dir = self.advance_char().unwrap_or('<');
        value.push(dir);
        self.advance_char(); // (
        value.push('(');
        self.read_matched_parens(value, 1)
    }

    /// Reads a process substitution `<(...)` or `>(...)` as a word token.
    fn read_process_sub_word(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut value = String::new();
        // Read < or >
        let dir = self.advance_char().unwrap_or('<');
        value.push(dir);
        // Read (
        self.advance_char();
        value.push('(');
        // Read until matching )
        self.read_matched_parens(&mut value, 1)?;
        self.state.command_start = false;
        Ok(Token::new(TokenType::Word, value, start, line))
    }

    /// Reads a word token, handling quoting and expansions.
    #[allow(clippy::too_many_lines)]
    fn read_word_token(&mut self, start: usize, line: usize) -> Result<Token> {
        let mut value = String::new();

        while let Some(c) = self.peek_char() {
            match c {
                // Metacharacters end a word
                ' ' | '\t' | '\n' | '|' | '&' | ';' | ')' => break,
                // < and > are metacharacters, but <( and >( are process substitution
                '<' | '>' => {
                    if !value.is_empty() && self.input.get(self.pos + 1) == Some(&'(') {
                        // Process substitution mid-word: cat<(cmd)
                        self.read_process_sub_into(&mut value)?;
                    } else {
                        break;
                    }
                }
                // ( is a metacharacter UNLESS preceded by = (array) or extglob prefix
                '(' => {
                    if value.ends_with('=') {
                        // Array assignment: arr=(...)
                        self.advance_char();
                        value.push('(');
                        self.read_matched_parens(&mut value, 1)?;
                    } else if value.ends_with('@')
                        || value.ends_with('?')
                        || value.ends_with('+')
                        || value.ends_with('!')
                        || (value.ends_with('*') && self.state.extglob)
                    {
                        // Extglob: @(...), ?(...), etc.
                        self.advance_char();
                        value.push('(');
                        self.read_matched_parens(&mut value, 1)?;
                    } else {
                        break;
                    }
                }
                '\'' | '"' | '\\' | '$' | '`' => {
                    self.read_word_special(&mut value, c)?;
                }
                // Extglob: @(...), ?(...), +(...), !(...)
                '@' | '?' | '+' | '!' if self.input.get(self.pos + 1) == Some(&'(') => {
                    self.read_extglob(&mut value, c)?;
                }
                // * extglob only when extglob mode is enabled
                '*' if self.input.get(self.pos + 1) == Some(&'(') && self.state.extglob => {
                    self.read_extglob(&mut value, c)?;
                }
                // Regular character
                _ => {
                    self.advance_char();
                    value.push(c);
                }
            }
        }

        if value.is_empty() {
            return Err(RableError::parse("unexpected character", start, line));
        }

        // Check for reserved words at command start
        let kind = if self.state.command_start {
            TokenType::reserved_word(&value).unwrap_or(TokenType::Word)
        } else {
            TokenType::Word
        };

        // After a word, we're no longer at command start (unless it's a keyword
        // that expects another command)
        self.state.command_start = kind.starts_command()
            || matches!(
                kind,
                TokenType::Then
                    | TokenType::Else
                    | TokenType::Elif
                    | TokenType::Do
                    | TokenType::Semi
            );

        Ok(Token::new(kind, value, start, line))
    }

    /// Reads contents of a single-quoted string (after the opening `'`).
    fn read_single_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.advance_char() {
                Some('\'') => {
                    value.push('\'');
                    return Ok(());
                }
                Some(c) => value.push(c),
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated single quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads contents of an ANSI-C quoted string `$'...'` (after the opening `'`).
    /// Unlike regular single quotes, `\` is an escape character here.
    fn read_ansi_c_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.advance_char();
                    value.push('\'');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    value.push('\\');
                    if let Some(next) = self.advance_char() {
                        value.push(next);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated ANSI-C quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads contents of a double-quoted string (after the opening `"`).
    fn read_double_quoted(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance_char();
                    value.push('"');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    // Line continuation: \<newline> is removed in double quotes
                    if self.peek_char() == Some('\n') {
                        self.advance_char();
                    } else {
                        value.push('\\');
                        if let Some(next) = self.advance_char() {
                            value.push(next);
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated double quote",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a quoted string, escape, dollar expansion, or backtick within a word.
    fn read_word_special(&mut self, value: &mut String, c: char) -> Result<()> {
        match c {
            '\'' => {
                self.advance_char();
                value.push('\'');
                self.read_single_quoted(value)?;
            }
            '"' => {
                self.advance_char();
                value.push('"');
                self.read_double_quoted(value)?;
            }
            '\\' => {
                self.advance_char();
                if self.peek_char() == Some('\n') {
                    self.advance_char(); // line continuation
                } else {
                    value.push('\\');
                    if let Some(next) = self.advance_char() {
                        value.push(next);
                    }
                }
            }
            '$' => {
                self.read_dollar(value)?;
            }
            '`' => {
                self.advance_char();
                value.push('`');
                self.read_backtick(value)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads an extglob pattern `@(...)`, `?(...)`, etc.
    fn read_extglob(&mut self, value: &mut String, prefix: char) -> Result<()> {
        self.advance_char();
        value.push(prefix);
        self.advance_char();
        value.push('(');
        self.read_matched_parens(value, 1)
    }

    /// Reads a dollar expansion into the value string.
    fn read_dollar(&mut self, value: &mut String) -> Result<()> {
        self.advance_char(); // consume '$'
        value.push('$');

        match self.peek_char() {
            Some('(') => {
                self.advance_char();
                value.push('(');
                if self.peek_char() == Some('(') {
                    // $(( ... )) arithmetic expansion
                    self.advance_char();
                    value.push('(');
                    self.read_matched_parens(value, 2)?;
                } else {
                    // $( ... ) command substitution
                    self.read_matched_parens(value, 1)?;
                }
            }
            Some('{') => {
                self.advance_char();
                value.push('{');
                // Parameter expansion: don't count unquoted { as depth increase
                self.read_param_expansion_braces(value)?;
            }
            Some('\'') => {
                // $'...' ANSI-C quoting — handles \ escapes unlike regular ''
                self.advance_char();
                value.push('\'');
                self.read_ansi_c_quoted(value)?;
            }
            Some('"') => {
                // $"..." locale string
                self.advance_char();
                value.push('"');
                self.read_double_quoted(value)?;
            }
            Some('[') => {
                // $[...] deprecated arithmetic
                self.advance_char();
                value.push('[');
                self.read_until_char(value, ']')?;
            }
            Some(c) if is_dollar_start(c) => self.read_variable_name(value, c),
            _ => {} // Bare $ at end of word — just leave it
        }
        Ok(())
    }

    /// Reads a variable name after `$` for simple expansions.
    fn read_variable_name(&mut self, value: &mut String, first: char) {
        if first.is_ascii_alphabetic() || first == '_' {
            while let Some(nc) = self.peek_char() {
                if nc.is_ascii_alphanumeric() || nc == '_' {
                    self.advance_char();
                    value.push(nc);
                } else {
                    break;
                }
            }
        } else {
            self.advance_char();
            value.push(first);
        }
    }

    /// Reads until matching closing parentheses.
    fn read_matched_parens(&mut self, value: &mut String, close_count: usize) -> Result<()> {
        let mut depth = close_count;
        loop {
            match self.peek_char() {
                Some(')') => {
                    self.advance_char();
                    value.push(')');
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some('(') => {
                    self.advance_char();
                    value.push('(');
                    depth += 1;
                }
                Some('\'') => {
                    self.advance_char();
                    value.push('\'');
                    self.read_single_quoted(value)?;
                }
                Some('"') => {
                    self.advance_char();
                    value.push('"');
                    self.read_double_quoted(value)?;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char(); // line continuation
                    } else {
                        value.push('\\');
                        if let Some(c) = self.advance_char() {
                            value.push(c);
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated parenthesis",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a parameter expansion `${...}` allowing unbalanced inner `{`.
    fn read_param_expansion_braces(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('}') => {
                    self.advance_char();
                    value.push('}');
                    return Ok(());
                }
                Some('\'') => {
                    self.advance_char();
                    value.push('\'');
                    self.read_single_quoted(value)?;
                }
                Some('"') => {
                    self.advance_char();
                    value.push('"');
                    self.read_double_quoted(value)?;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\n') {
                        self.advance_char(); // line continuation
                    } else {
                        value.push('\\');
                        if let Some(c) = self.advance_char() {
                            value.push(c);
                        }
                    }
                }
                Some('$') => {
                    self.read_dollar(value)?;
                }
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    self.read_backtick(value)?;
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated parameter expansion",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads a backtick command substitution.
    fn read_backtick(&mut self, value: &mut String) -> Result<()> {
        loop {
            match self.peek_char() {
                Some('`') => {
                    self.advance_char();
                    value.push('`');
                    return Ok(());
                }
                Some('\\') => {
                    self.advance_char();
                    value.push('\\');
                    if let Some(c) = self.advance_char() {
                        value.push(c);
                    }
                }
                Some(c) => {
                    self.advance_char();
                    value.push(c);
                }
                None => {
                    return Err(RableError::matched_pair(
                        "unterminated backtick",
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }

    /// Reads until the given closing character.
    fn read_until_char(&mut self, value: &mut String, close: char) -> Result<()> {
        loop {
            match self.advance_char() {
                Some(c) if c == close => {
                    value.push(c);
                    return Ok(());
                }
                Some(c) => value.push(c),
                None => {
                    return Err(RableError::matched_pair(
                        format!("unterminated '{close}'"),
                        self.pos,
                        self.line,
                    ));
                }
            }
        }
    }
}

/// Returns true if the character can follow `$` to start a variable expansion.
const fn is_dollar_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '*' | '#' | '?' | '-' | '$' | '!')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    fn collect_tokens(source: &str) -> Vec<(TokenType, String)> {
        let mut lexer = Lexer::new(source, false);
        let mut tokens = Vec::new();
        loop {
            let tok = lexer.next_token().unwrap();
            if tok.kind == TokenType::Eof {
                break;
            }
            tokens.push((tok.kind, tok.value));
        }
        tokens
    }

    #[test]
    fn simple_command() {
        let tokens = collect_tokens("echo hello world");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], (TokenType::Word, "echo".to_string()));
        assert_eq!(tokens[1], (TokenType::Word, "hello".to_string()));
        assert_eq!(tokens[2], (TokenType::Word, "world".to_string()));
    }

    #[test]
    fn pipeline() {
        let tokens = collect_tokens("ls | grep foo");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].1, "ls");
        assert_eq!(tokens[1], (TokenType::Pipe, "|".to_string()));
        assert_eq!(tokens[2].1, "grep");
        assert_eq!(tokens[3].1, "foo");
    }

    #[test]
    fn redirections() {
        let tokens = collect_tokens("echo hello > file.txt");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[2], (TokenType::Greater, ">".to_string()));
    }

    #[test]
    fn reserved_words() {
        // if(0) true(1) ;(2) then(3) echo(4) yes(5) ;(6) fi(7)
        let tokens = collect_tokens("if true; then echo yes; fi");
        assert_eq!(tokens[0].0, TokenType::If);
        assert_eq!(tokens[2].0, TokenType::Semi);
        assert_eq!(tokens[3].0, TokenType::Then);
        assert_eq!(tokens[6].0, TokenType::Semi);
        assert_eq!(tokens[7].0, TokenType::Fi);
    }

    #[test]
    fn single_quoted() {
        let tokens = collect_tokens("echo 'hello world'");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].1, "'hello world'");
    }

    #[test]
    fn double_quoted() {
        let tokens = collect_tokens("echo \"hello $name\"");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].1, "\"hello $name\"");
    }

    #[test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn dollar_expansion() {
        let tokens = collect_tokens("echo ${foo:-bar}");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].1, "${foo:-bar}");
    }

    #[test]
    fn command_substitution() {
        let tokens = collect_tokens("echo $(date)");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].1, "$(date)");
    }

    #[test]
    fn and_or() {
        let tokens = collect_tokens("a && b || c");
        assert_eq!(tokens[1], (TokenType::And, "&&".to_string()));
        assert_eq!(tokens[3], (TokenType::Or, "||".to_string()));
    }
}
