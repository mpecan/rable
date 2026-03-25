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
    // Trailing backslash doubling
    "ansi_c_escapes 3",
    "redirect_formatting 3",
    "heredoc_formatting 1",
    // ANSI-C \x single hex digit and \0 octal repeat behavior
    "ansi_c_escapes 13",
    "other 10",
    // Heredoc delimiter edge cases
    "ansi_c_escapes 18",
    "heredoc_formatting 8",
    // Varfd {6d} not recognized → word dropped
    "heredoc_formatting 9",
    // Coproc with adjacent redirect
    "redirect_formatting 7",
    // Background & placement after heredoc in cmdsub
    "cmdsub_formatting 9",
    // Deprecated $[...] with ; splits word
    "word_boundaries 8",
    // Assignment detection causes esac to be keyword
    "word_boundaries 2",
];

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

/// Oracle-derived tests: correctness differences found by fuzzing against bash-oracle.
/// Asserts that known failures still fail and known passes don't regress.
#[test]
fn oracle_test_suite() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    if !test_dir.exists() {
        eprintln!("Skipping: tests/oracle/ directory not found");
        return;
    }

    eprintln!("\n=== Oracle Test Suite (bash-oracle compatibility) ===\n");

    let filter = std::env::var("RABLE_TEST").ok();
    let mut results = OracleResults {
        total_pass: 0,
        total_fail: 0,
        regressions: Vec::new(),
        newly_passing: Vec::new(),
    };

    let mut entries: Vec<_> = fs::read_dir(&test_dir)
        .unwrap_or_else(|_| {
            eprintln!("Warning: test directory not found: {}", test_dir.display());
            std::process::exit(0);
        })
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "tests"))
        .collect();

    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) {
            continue;
        }
        run_oracle_file(&path, &mut results);
    }

    let total = results.total_pass + results.total_fail;
    eprintln!(
        "\n=== Oracle Results: {}/{total} passed ({} remaining) ===\n",
        results.total_pass, results.total_fail
    );

    for msg in &results.newly_passing {
        eprintln!("{msg}");
    }
    for msg in &results.regressions {
        eprintln!("{msg}");
    }

    assert!(
        results.regressions.is_empty(),
        "{} oracle test regression(s)",
        results.regressions.len()
    );
    assert!(
        results.newly_passing.is_empty(),
        "{} oracle test(s) newly passing — update KNOWN_ORACLE_FAILURES",
        results.newly_passing.len()
    );
}
