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
const KNOWN_ORACLE_FAILURES: &[&str] = &[
    // Cosmetic: bash adds a space before ) in $(cmd <<heredoc &\n...\n )
    // but we produce $(cmd <<heredoc &\n...\n). The space is semantically
    // irrelevant — $(cmd ) and $(cmd) are identical in bash. The space
    // is an artifact of bash's internal parser boundary between heredoc
    // content and the $(...) close delimiter.
    "cmdsub_formatting 9",
    // bash_valid_divergences — collected from differential fuzzing against
    // bash-oracle (tests/fuzz.py mutate --valid-only). Each cluster is
    // tracked as a separate GitHub issue.
    // #35 — ]] tokenization outside [[ ]]: all 3 cases fixed.
    // #36 — unbalanced [...] absorbing || / &&: all 3 cases fixed as a
    //   side effect of #35's guarded bracket-subscript helper.
    // #37 — reserved words as plain words: cases 1, 3, 4 fixed by #44;
    //   case 5 fixed as a side effect of #35; case 2 fixed by #42
    //   (`((` → nested subshell fallback).
    // #38 — backticks opaque on invalid content
    "backtick_opaque 1",
    "backtick_opaque 2",
    "backtick_opaque 3",
    "backtick_opaque 4",
    "backtick_opaque 5",
    "backtick_opaque 6",
    // #39 — heredoc inside $(...)
    "heredoc_in_cmdsub 1",
    "heredoc_in_cmdsub 2",
    // #40 — command-sub canonical reformat drift
    "cmdsub_reformat 1",
    "cmdsub_reformat 2",
    "cmdsub_reformat 3",
    "cmdsub_reformat 4",
    "cmdsub_reformat 5",
    "cmdsub_reformat 6",
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
            if !is_known {
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
oracle_test!(
    oracle_bash_valid_divergences,
    "bash_valid_divergences.tests"
);
