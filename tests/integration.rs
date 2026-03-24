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

/// Runs tests from a single file.
/// If `verbose` is true, prints every test's pass/fail with details.
fn run_single_file(path: &Path, verbose: bool) -> (usize, usize, Vec<String>) {
    let content = fs::read_to_string(path).unwrap_or_default();
    let cases = parse_test_file(&content);
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut pass = 0;
    let mut fail = 0;
    let mut failures = Vec::new();

    for case in &cases {
        let (passed, actual) = run_test(case);
        if passed {
            pass += 1;
            if verbose {
                eprintln!("  PASS :: {}", case.name);
            }
        } else {
            fail += 1;
            let msg = format!(
                "  FAIL :: {}\n    input:    {:?}\n    expected: {:?}\n    actual:   {:?}",
                case.name, case.input, case.expected, actual,
            );
            if verbose {
                eprintln!("{msg}");
            }
            failures.push(msg);
        }
    }

    let total = pass + fail;
    let status = if fail == 0 { "OK" } else { "FAIL" };
    eprintln!("  {file_name}: {pass}/{total} passed [{status}]");
    (pass, fail, failures)
}

/// Discovers and runs all `.tests` files in the given directory.
fn run_test_files(dir: &Path) -> (usize, usize, Vec<String>) {
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut failures = Vec::new();

    let filter = std::env::var("RABLE_TEST").ok();

    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|_| {
            eprintln!("Warning: test directory not found: {}", dir.display());
            std::process::exit(0);
        })
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "tests"))
        .collect();

    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        // If RABLE_TEST is set, only run matching files
        if let Some(ref f) = filter
            && !file_name.contains(f.as_str())
        {
            continue;
        }

        let verbose = filter.is_some();
        let (p, f, mut ff) = run_single_file(&path, verbose);
        total_pass += p;
        total_fail += f;
        failures.append(&mut ff);
    }

    (total_pass, total_fail, failures)
}

#[test]
fn parable_test_suite() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parable");
    if !test_dir.exists() {
        eprintln!("Skipping: tests/parable/ directory not found");
        return;
    }

    eprintln!("\n=== Rable Test Suite (Parable compatibility) ===\n");
    let (pass, fail, failures) = run_test_files(&test_dir);
    eprintln!("\n=== Results: {pass} passed, {fail} failed ===\n");

    if !failures.is_empty() && std::env::var("RABLE_TEST").is_err() {
        let max_show = 50;
        let shown = failures.len().min(max_show);
        eprintln!("First {shown} failures:");
        for f in failures.iter().take(max_show) {
            eprintln!("{f}");
        }
        if failures.len() > max_show {
            eprintln!("  ... and {} more", failures.len() - max_show);
        }
    }

    let total = pass + fail;
    eprintln!("Pass rate: {pass}/{total}");
}

/// Oracle-derived tests: correctness differences found by fuzzing against bash-oracle.
/// These tests track progress toward full bash compatibility beyond Parable's test suite.
/// This test reports results but does NOT fail the build.
#[test]
fn oracle_test_suite() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    if !test_dir.exists() {
        eprintln!("Skipping: tests/oracle/ directory not found");
        return;
    }

    eprintln!("\n=== Oracle Test Suite (bash-oracle compatibility) ===\n");
    let (pass, fail, _failures) = run_test_files(&test_dir);
    let total = pass + fail;
    eprintln!("\n=== Oracle Results: {pass}/{total} passed ({fail} remaining) ===\n");
    // Intentionally does not assert — these are aspirational targets
}
