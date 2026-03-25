use std::fs;
use std::path::Path;

/// A single test case parsed from a `.tests` file.
#[derive(Debug)]
struct TestCase {
    name: String,
    input: String,
    expected: String,
    extglob: bool,
}

/// Parses a `.tests` file into individual test cases.
///
/// Format:
/// ```text
/// === test name
/// input content
/// ---
/// expected output (S-expression)
/// ---
/// ```
///
/// Directives:
/// - `# @extglob` enables extended globbing for subsequent tests
fn parse_test_file(content: &str) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let mut extglob = false;
    let mut lines = content.lines();

    while let Some(line) = lines.next() {
        if line.starts_with("# @extglob") {
            extglob = true;
            continue;
        }
        if let Some(name) = line.strip_prefix("=== ") {
            let name = name.trim().to_string();
            let mut input_lines = Vec::new();
            for line in lines.by_ref() {
                if line == "---" {
                    break;
                }
                input_lines.push(line);
            }
            let mut expected_lines = Vec::new();
            for line in lines.by_ref() {
                if line == "---" {
                    break;
                }
                expected_lines.push(line);
            }
            // Check for @extglob directive in input lines
            let mut test_extglob = extglob;
            let filtered_input: Vec<_> = input_lines
                .iter()
                .filter(|l| {
                    if l.starts_with("# @extglob") {
                        test_extglob = true;
                        false
                    } else {
                        true
                    }
                })
                .copied()
                .collect();
            cases.push(TestCase {
                name,
                input: filtered_input.join("\n"),
                expected: expected_lines.join("\n"),
                extglob: test_extglob,
            });
        }
    }
    cases
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Runs a single test case and returns `(passed, actual_output)`.
fn run_test(case: &TestCase) -> (bool, String) {
    match rable::parse(&case.input, case.extglob) {
        Ok(nodes) => {
            let actual = nodes
                .iter()
                .map(|n| format!("{n}"))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            if case.expected == "<error>" {
                return (false, format!("expected error, got: {actual}"));
            }

            let passed = actual.trim() == case.expected.trim()
                || normalize_whitespace(&actual) == normalize_whitespace(&case.expected);
            (passed, actual)
        }
        Err(e) => {
            if case.expected == "<error>" {
                (true, format!("error (expected): {e}"))
            } else {
                (false, format!("error: {e}"))
            }
        }
    }
}

/// Runs all tests from a single `.tests` file. Panics on any failure.
fn run_file_asserting(file_name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/parable")
        .join(file_name);
    if !path.exists() {
        eprintln!("Skipping: {file_name} not found");
        return;
    }

    let content = fs::read_to_string(&path).unwrap_or_default();
    let cases = parse_test_file(&content);
    let mut pass = 0;
    let mut failures = Vec::new();

    for case in &cases {
        let (passed, actual) = run_test(case);
        if passed {
            pass += 1;
        } else {
            failures.push(format!(
                "  FAIL :: {}\n    input:    {:?}\n    expected: {:?}\n    actual:   {:?}",
                case.name, case.input, case.expected, actual,
            ));
        }
    }

    let total = pass + failures.len();
    if !failures.is_empty() {
        for f in &failures {
            eprintln!("{f}");
        }
        assert!(
            failures.is_empty(),
            "{file_name}: {pass}/{total} passed, {} failed",
            failures.len()
        );
    }
}

/// Generates one `#[test]` per Parable `.tests` file so each file appears
/// as a separate test in `cargo test` output.
macro_rules! parable_tests {
    ($($name:ident => $file:literal),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                run_file_asserting($file);
            }
        )*
    };
}

parable_tests! {
    parable_01_words                => "01_words.tests",
    parable_02_commands             => "02_commands.tests",
    parable_03_pipelines            => "03_pipelines.tests",
    parable_04_lists                => "04_lists.tests",
    parable_05_redirects            => "05_redirects.tests",
    parable_06_compound             => "06_compound.tests",
    parable_07_if                   => "07_if.tests",
    parable_08_loops                => "08_loops.tests",
    parable_09_case                 => "09_case.tests",
    parable_10_functions            => "10_functions.tests",
    parable_11_parameter_expansion  => "11_parameter_expansion.tests",
    parable_12_command_substitution => "12_command_substitution.tests",
    parable_13_arithmetic           => "13_arithmetic.tests",
    parable_14_here_documents       => "14_here_documents.tests",
    parable_15_process_substitution => "15_process_substitution.tests",
    parable_16_negation_time        => "16_negation_time.tests",
    parable_17_conditional_expr     => "17_conditional_expr.tests",
    parable_18_arrays               => "18_arrays.tests",
    parable_19_coproc               => "19_coproc.tests",
    parable_20_select               => "20_select.tests",
    parable_21_cstyle_for           => "21_cstyle_for.tests",
    parable_22_pipe_stderr          => "22_pipe_stderr.tests",
    parable_23_case_fallthrough     => "23_case_fallthrough.tests",
    parable_24_ansi_c_quoting       => "24_ansi_c_quoting.tests",
    parable_25_locale_translation   => "25_locale_translation.tests",
    parable_26_variable_fd          => "26_variable_fd.tests",
    parable_27_deprecated_arithmetic => "27_deprecated_arithmetic.tests",
    parable_28_obscure_edge_cases   => "28_obscure_edge_cases.tests",
    parable_29_arithmetic_internals => "29_arithmetic_internals.tests",
    parable_30_extglob_case         => "30_extglob_case.tests",
    parable_31_parser_bugs          => "31_parser_bugs.tests",
    parable_32_oils_gaps            => "32_oils_gaps.tests",
    parable_33_brace_edge_cases     => "33_brace_edge_cases.tests",
    parable_34_backslash_newline_bugs => "34_backslash_newline_bugs.tests",
    parable_34_line_continuation    => "34_line_continuation.tests",
    parable_35_parser_bugs          => "35_parser_bugs.tests",
}

/// Known oracle test failures — tests that don't match bash-oracle output yet.
/// When a fix makes one of these pass, the test suite will fail with
/// "NEWLY PASSING" so you know to remove it from this list.
///
/// Set `RABLE_VERBOSE=1` to see actual vs expected for known failures.
const KNOWN_ORACLE_FAILURES: &[&str] = &[
    // --- Trailing backslash at EOF: bash doubles it, we don't ---
    // `echo $'\>\\'\\` → we output `\\` but bash expects `\\\\`
    // Root cause: lexer/words.rs read_word_special and quotes.rs — when
    // advance_char() returns None after `\`, we push only `\` not `\\`
    "ansi_c_escapes 3",
    "redirect_formatting 3",
    "heredoc_formatting 1",
    // --- ANSI-C escape: \xN single hex digit produces 1 char, bash produces 2 ---
    // `$'\x1'` → we output `\x01` once, bash outputs it twice
    // Root cause: sexp/mod.rs process_ansi_c_content — bash may repeat
    // the byte for single-digit hex escapes
    "ansi_c_escapes 13",
    // --- ANSI-C escape: \0N octal — bash treats \0 as NUL truncation ---
    // `$'\01Sch1'` → we output `\x01Sch1`, bash truncates at NUL to `Sch1`
    // Root cause: sexp/mod.rs process_ansi_c_content — \01 parsed as
    // octal 1 (non-NUL), but bash may treat \0 as NUL + literal `1Sch1`
    "other 10",
    // --- Heredoc: delimiter with trailing space not matched ---
    // `$( cat <<EOF\n...\nEOF )` — the `EOF ` has trailing space before `)`
    // Root cause: lexer/heredoc.rs — delimiter matching doesn't trim
    // trailing whitespace on the closing line
    "ansi_c_escapes 18",
    // --- Heredoc: `coproc <<cat` not recognized as heredoc ---
    // Root cause: parser/compound.rs parse_coproc — doesn't check for
    // redirect operators when the next token after `coproc` is `<<`
    "heredoc_formatting 8",
    // --- Varfd: `{6d}` consumed as varfd, dropping the word ---
    // `cat {6d}<<n<text` → `{6d}` should stay as word, not be consumed
    // Root cause: parser/helpers.rs is_varfd — too permissive, accepts
    // `{6d}` because it contains letter `d`, but bash requires valid
    // variable name (letters/underscore first, not digits)
    "heredoc_formatting 9",
    // --- Coproc: `coproc<>` adjacent redirect not tokenized ---
    // Root cause: lexer doesn't split `coproc` from `<>` when adjacent;
    // parser/compound.rs parse_coproc doesn't handle redirects before
    // the command word
    "redirect_formatting 7",
    // --- Cmdsub: background `&` placed after heredoc content ---
    // `echo $(ech<<o bg &)` → `&` should come before heredoc content
    // Root cause: format/mod.rs — background operator ordering relative
    // to heredoc content in reformatted command substitutions
    "cmdsub_formatting 9",
    // --- Deprecated arith: `$[...]` stops at first `]`, not balanced ---
    // `$[$[x+1];x+1]` → should include nested `[...]` and `;`
    // Root cause: lexer/expansions.rs read_until_char — finds first `]`
    // without depth tracking for nested brackets
    "word_boundaries 8",
    // --- Assignment detection: `esac` after assignment becomes keyword ---
    // `arr[0 until ]=$ esac foo` → `esac` should be a regular word
    // Root cause: lexer/words.rs — command_start stays true after
    // AssignmentWord, making `esac` a reserved word. Bash doesn't treat
    // words with spaces in bracket subscripts as assignments
    "word_boundaries 2",
];

#[derive(Default)]
struct OracleResults {
    total_pass: usize,
    total_fail: usize,
    regressions: Vec<String>,
    newly_passing: Vec<String>,
}

fn run_oracle_file(path: &Path, results: &mut OracleResults) {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let content = fs::read_to_string(path).unwrap_or_default();
    let cases = parse_test_file(&content);
    let mut pass = 0;
    let mut fail = 0;

    for case in &cases {
        let (passed, actual) = run_test(case);
        let is_known = KNOWN_ORACLE_FAILURES.contains(&case.name.as_str());
        if passed {
            pass += 1;
            if is_known {
                results.newly_passing.push(format!(
                    "  NEWLY PASSING :: {} — remove from KNOWN_ORACLE_FAILURES",
                    case.name,
                ));
            }
        } else {
            fail += 1;
            if is_known {
                if std::env::var("RABLE_VERBOSE").is_ok() {
                    eprintln!(
                        "    KNOWN FAIL :: {}\n      input:    {:?}\n      expected: {:?}\n      actual:   {:?}",
                        case.name, case.input, case.expected, actual,
                    );
                }
            } else {
                results.regressions.push(format!(
                    "  REGRESSION :: {}\n    input:    {:?}\n    expected: {:?}\n    actual:   {:?}",
                    case.name, case.input, case.expected, actual,
                ));
            }
        }
    }

    let total = pass + fail;
    let status = if fail == 0 { "OK" } else { "FAIL" };
    eprintln!("  {file_name}: {pass}/{total} passed [{status}]");
    results.total_pass += pass;
    results.total_fail += fail;
}

/// Generates a test function for each oracle test file.
/// Each test asserts no regressions and no newly passing tests
/// (which would need `KNOWN_ORACLE_FAILURES` updated).
macro_rules! oracle_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/oracle")
                .join($file);
            if !path.exists() {
                eprintln!("Skipping: {} not found", $file);
                return;
            }
            let mut results = OracleResults::default();
            run_oracle_file(&path, &mut results);
            let total = results.total_pass + results.total_fail;
            eprintln!(
                "  {}: {}/{total} passed ({} remaining)",
                $file, results.total_pass, results.total_fail,
            );
            for msg in &results.newly_passing {
                eprintln!("{msg}");
            }
            for msg in &results.regressions {
                eprintln!("{msg}");
            }
            assert!(
                results.regressions.is_empty(),
                "{}: {} regression(s)",
                $file,
                results.regressions.len()
            );
            assert!(
                results.newly_passing.is_empty(),
                "{}: {} newly passing — update KNOWN_ORACLE_FAILURES",
                $file,
                results.newly_passing.len()
            );
        }
    };
}

oracle_test!(oracle_ansi_c_escapes, "ansi_c_escapes.tests");
oracle_test!(oracle_ansi_c_processing, "ansi_c_processing.tests");
oracle_test!(oracle_array_normalization, "array_normalization.tests");
oracle_test!(oracle_cmdsub_formatting, "cmdsub_formatting.tests");
oracle_test!(oracle_heredoc_formatting, "heredoc_formatting.tests");
oracle_test!(oracle_locale_strings, "locale_strings.tests");
oracle_test!(oracle_other, "other.tests");
oracle_test!(oracle_procsub_formatting, "procsub_formatting.tests");
oracle_test!(oracle_redirect_formatting, "redirect_formatting.tests");
oracle_test!(oracle_top_level_separation, "top_level_separation.tests");
oracle_test!(oracle_word_boundaries, "word_boundaries.tests");
