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
