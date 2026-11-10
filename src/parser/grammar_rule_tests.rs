//! bash grammar rules beyond plain syntax errors: where arrays, extended
//! patterns and function bodies are allowed, `!`/`time`, `[[ ]]` nesting,
//! subscripts, `for ((…))` and `${ …; }`. Each expectation was checked
//! against bash-oracle.

use super::syntax_test_support::*;
use crate::ast::{NodeKind, RedirectKind};

// -- array assignments ------------------------------------------------------

#[test]
fn array_assignment_needs_assignment_position() {
    assert_syntax_error("echo a=(1 2)", 7);
    assert_syntax_error("a r=(foo)", 4);
    assert_syntax_error("a1 b=2 arr=(x y)", 11);
    assert_syntax_error("command declare a=(x)", 18);
    assert_syntax_error("a=(x) 2>f b=(y)", 12);
    assert_syntax_error("declare >f a=(x)", 13);
    assert_words("x=1 a=(1 2) cmd", &["x=1", "a=(1 2)", "cmd"]);
    assert_words("declare -a a=(1 2)", &["declare", "-a", "a=(1 2)"]);
    assert_words("x=1 local a=(x) b=(y)", &["x=1", "local", "a=(x)", "b=(y)"]);
    assert_words(">f a=(x)", &["a=(x)"]);
    assert_words("2>&1 >&- a=(x)", &["a=(x)"]);
    assert_words("{fd}>f declare a=(x)", &["declare", "a=(x)"]);
}

#[test]
fn array_assignment_needs_a_valid_name() {
    assert_syntax_error("1a=(x)", 4);
    assert_syntax_error("a$b=(x)", 5);
    assert_words("a[0]=(1 2)", &["a[0]=(1 2)"]);
    assert_words("a+=(1 2)", &["a+=(1 2)"]);
}

#[test]
fn array_elements_reject_unquoted_operators() {
    assert_syntax_error("a=(x;y)", 4);
    assert_syntax_error("a=(x|y)", 4);
    assert_syntax_error("a=(x&)", 4);
    assert_syntax_error("a=(x>y)", 4);
    assert_syntax_error("a=((x))", 3);
    assert_syntax_error("a=([k]=(v))", 7);
    assert_words(r#"a=("a;b" 'c|d' a\;b)"#, &[r#"a=("a;b" 'c|d' a\;b)"#]);
    assert_words("a=(<(x) >(y))", &["a=(<(x) >(y))"]);
    assert_words("a=($(x;y))", &["a=($(x; y))"]);
}

#[test]
fn array_subscript_is_bracket_matched() {
    assert_words("arr=([a;b]=1 [c d]=2)", &["arr=([a;b]=1 [c d]=2)"]);
    assert_words("arr=([a)b]=1)", &["arr=([a)b]=1)"]);
    assert_syntax_error("arr=([a]=1;)", 10);
    assert_syntax_error("arr=(x[;])", 7);
}

// -- extended patterns ------------------------------------------------------

#[test]
fn extglob_patterns_need_extglob() {
    assert_syntax_error("echo @(a|b)", 6);
    assert_syntax_error("echo x+(a)", 7);
    assert_syntax_error("case x in @(a)) :;; esac", 11);
    assert!(crate::parse("echo @(a|b) x+(a) ?(b)", true).is_ok());
    // `@` and `+` without a following `(` stay ordinary characters, and
    // are valid function names once `(` no longer opens a pattern.
    assert_words("echo foo@bar a+b", &["echo", "foo@bar", "a+b"]);
    assert_eq!(
        sexp("+() { :; }"),
        "(function \"+\" (brace-group (command (word \":\"))))"
    );
}

#[test]
fn cond_pattern_operand_always_allows_extglob() {
    assert_eq!(
        sexp("[[ x == @(a|b) ]]"),
        "(cond (cond-binary \"==\" (cond-term \"x\") (cond-term \"@(a|b)\")))"
    );
    assert_eq!(
        sexp("[[ x != !(a) ]]"),
        "(cond (cond-binary \"!=\" (cond-term \"x\") (cond-term \"!(a)\")))"
    );
    // Only the pattern operand: elsewhere `(` keeps its usual meaning.
    assert_syntax_error("[[ @(a) ]]", 4);
    assert_syntax_error("[[ -f @(a) ]]", 7);
    // `!(` here is negation and grouping, not an extglob word.
    assert_eq!(
        sexp("[[ !( -f a && -f b ) ]]"),
        "(cond (cond-expr (cond-and (cond-unary \"-f\" (cond-term \"a\")) \
         (cond-unary \"-f\" (cond-term \"b\")))))"
    );
}

// -- function bodies --------------------------------------------------------

#[test]
fn function_body_must_be_compound() {
    assert_syntax_error("f() echo", 4);
    assert_syntax_error("f()", 3);
    assert_syntax_error("f() ;", 4);
    assert_syntax_error("f() g() { :; }", 4);
    assert_syntax_error("f() coproc { :; }", 4);
    assert_syntax_error("f() ! { :; }", 4);
    assert_syntax_error("function f echo", 11);
    assert_syntax_error("function f", 10);
    assert_syntax_error("function ; { :; }", 9);
    for body in [
        "( : )",
        "((x))",
        "[[ x ]]",
        "if x; then :; fi",
        "for x; do :; done",
    ] {
        assert!(try_parse(&format!("f() {body}")).is_ok(), "f() {body}");
    }
    assert_eq!(
        sexp("function f (( x ))"),
        "(function \"f\" (arith (word \" x \")))"
    );
}

#[test]
fn time_options_then_a_command() {
    // After `-p` / `--` a command follows, so reserved words count again.
    assert_eq!(
        sexp("time -p ! a"),
        r#"(negation (time -p (command (word "a"))))"#
    );
    assert_eq!(
        sexp("time -p { a; }"),
        r#"(time -p (brace-group (command (word "a"))))"#
    );
    // bash reports `time --` in its POSIX form and consumes one `--`.
    assert_eq!(sexp("time -- a"), r#"(time -p (command (word "a")))"#);
    assert_eq!(
        sexp("time -p -- -- a"),
        r#"(time -p (command (word "--") (word "a")))"#
    );
}

#[test]
fn cond_and_or_nest_to_the_right() {
    // bash's cond_and / cond_or recurse on their right operand.
    assert_eq!(
        sexp("[[ a && b && c ]]"),
        "(cond (cond-and (cond-unary \"-n\" (cond-term \"a\")) (cond-and \
         (cond-unary \"-n\" (cond-term \"b\")) (cond-unary \"-n\" (cond-term \"c\")))))"
    );
    assert_eq!(
        sexp("[[ a && b || c ]]"),
        "(cond (cond-or (cond-and (cond-unary \"-n\" (cond-term \"a\")) \
         (cond-unary \"-n\" (cond-term \"b\"))) (cond-unary \"-n\" (cond-term \"c\"))))"
    );
}

#[test]
fn and_redirects_take_no_fd_prefix() {
    // `&>` / `&>>` never take an fd prefix, so the prefix is a word.
    assert_eq!(
        sexp("exec {fd}&>1"),
        r#"(command (word "exec") (word "{fd}") (redirect "&>" "1"))"#
    );
    assert_eq!(
        sexp("a 2&>f"),
        r#"(command (word "a") (word "2") (redirect "&>" "f"))"#
    );
    assert_syntax_error("{ a; } {fd}&>x", 7);
}

#[test]
fn rbracket_cannot_start_a_command() {
    assert_syntax_error("]] foo", 0);
    assert_syntax_error("a; ]] b", 3);
    assert_syntax_error("{ ]] a; }", 2);
    assert_syntax_error("! ]]", 2);
    // Elsewhere `]]` is an ordinary word.
    assert_words("echo ]]", &["echo", "]]"]);
    assert_eq!(
        sexp("case x in ]]) :;; esac"),
        r#"(case (word "x") (pattern ((word "]]")) (command (word ":"))))"#
    );
}

#[test]
fn command_start_subscript_must_close() {
    // bash matches `name[` to its `]` across blanks, so these hit EOF.
    for source in ["foo[0", "foo[0 bar", "a=1 b[0", "x; y[1", "arr=([a"] {
        assert_unclosed(source);
    }
    assert_words("foo[a b]=1", &["foo[a b]=1"]);
    assert_words("echo foo[0", &["echo", "foo[0"]);
    // A redirect target is not an assignment, so no subscript there.
    assert_eq!(
        sexp("<exp[ 5  3"),
        r#"(command (word "5") (word "3") (redirect "<" "exp["))"#
    );
}

#[test]
fn arith_for_needs_exactly_three_expressions() {
    for source in [
        "for ((i=0;i<3)); do :; done",
        "for ((;;;)); do :; done",
        "for ((i=0; i<5 i++)); do :; done",
        "for (( )); do :; done",
    ] {
        assert!(try_parse(source).is_err(), "{source:?}");
    }
    // `;` inside quotes or an expansion does not separate expressions.
    assert_eq!(
        sexp("for ((i=0; \"a;b\"; i++)); do :; done"),
        "(arith-for (init (word \"i=0\")) (test (word \"\\\"a;b\\\"\")) \
         (step (word \"i++\")) (command (word \":\")))"
    );
    assert!(sexp("for ((;;)); do :; done").starts_with("(arith-for (init (word \"1\"))"));
}

#[test]
fn funsub_is_parsed_as_commands() {
    // `${ …; }` / `${| …; }` hold commands up to a `}` reserved word.
    assert_rejected("echo ${ foo^}");
    assert_rejected("echo ${ echo hi }");
    // bash reprints the body like `$(…)`.
    assert_words("echo ${ echo hi;}", &["echo", "${ echo hi; }"]);
    assert_words("echo ${| echo hi; }", &["echo", "${|echo hi; }"]);
    assert_words("echo ${ a;}${ b;}", &["echo", "${ a; }${ b; }"]);
    assert_words("echo ${ }", &["echo", "${ }"]);
    // A plain `${…}` is still a parameter expansion.
    assert_words("echo ${foo}", &["echo", "${foo}"]);
}

#[test]
fn redirects_record_what_they_do() {
    // (source, op, fd, target, kind): a dup/move/close always has its fd.
    let cases = [
        ("a > f", ">", -1, "f", RedirectKind::File),
        ("a >&2", ">&", 1, "2", RedirectKind::Dup),
        ("a <&$x", "<&", 0, "$x", RedirectKind::Dup),
        ("a 3<&0-", "<&", 3, "0", RedirectKind::Move),
        ("a >&-", ">&", 1, "-", RedirectKind::Close),
        ("a <&-", "<&", 0, "-", RedirectKind::Close),
        ("a 4>& -", ">&", 4, "-", RedirectKind::Close),
    ];
    for (source, op, fd, target, kind) in cases {
        let nodes = try_parse(source).unwrap_or_default();
        let Some(NodeKind::Command { redirects, .. }) = nodes.first().map(|n| &n.kind) else {
            unreachable!("{source:?}: expected a command");
        };
        let Some(NodeKind::Redirect {
            op: got_op,
            fd: got_fd,
            target: got_target,
            kind: got_kind,
            ..
        }) = redirects.first().map(|r| &r.kind)
        else {
            unreachable!("{source:?}: expected a redirect");
        };
        let NodeKind::Word { value, .. } = &got_target.kind else {
            unreachable!("{source:?}: expected a word target");
        };
        assert_eq!(
            (got_op.as_str(), *got_fd, value.as_str(), *got_kind),
            (op, fd, target, kind),
            "{source:?}"
        );
    }
}

#[test]
fn nested_substitution_debug_and_eq_stay_linear() {
    // A substitution's parsed body is cached on its span and also appears
    // in the word's parts; Debug and PartialEq must not visit both, or
    // their cost doubles with every level of nesting.
    let depth = 40;
    let source = format!("echo {}x{}", "$(a ".repeat(depth), ")".repeat(depth));
    let nodes = try_parse(&source).unwrap_or_default();
    assert!(format!("{nodes:?}").len() < 100_000);
    assert_eq!(nodes, try_parse(&source).unwrap_or_default());
}

#[test]
fn arith_for_split_skips_substitutions_like_bash() {
    // A `;` inside a quoted `$(…)`, or a `case` terminator inside one, is
    // not an expression separator.
    assert_eq!(
        sexp(r#"for ((i=0; "$(printf ";")"; i++)); do :; done"#),
        r#"(arith-for (init (word "i=0")) (test (word "\"$(printf \";\")\"")) (step (word "i++")) (command (word ":")))"#
    );
    let nested = sexp("for ((i=$(case x in x) echo 0;; esac); i<3; i++)); do :; done");
    assert!(
        nested.contains(r#"(test (word "i<3")) (step (word "i++"))"#),
        "{nested}"
    );
}

#[test]
fn array_elements_are_joined_by_one_space() {
    // bash reprints `name=(…)` with one space between elements; a line
    // continuation between them is blank space, not an empty element.
    assert_words("arr=(\"a\" \\\n \"b\")", &["arr=(\"a\" \"b\")"]);
    assert_words("arr=(a \\\n b)", &["arr=(a b)"]);
    assert_words("arr=(\\\na\\\nb c)", &["arr=(ab c)"]);
}
