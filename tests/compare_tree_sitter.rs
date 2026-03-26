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

/// Check if a tree-sitter node or any descendant is an ERROR or MISSING node.
fn has_error(node: &tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error(&child) {
            return true;
        }
    }
    false
}

/// Results for a single parser on a single test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseResult {
    /// Parsed successfully (for rable: matches expected output)
    Pass,
    /// Parsed but output doesn't match expected (rable only)
    WrongOutput,
    /// Parse error / ERROR nodes present
    ParseError,
    /// Test expects an error and parser produced one
    ExpectedError,
}

fn run_rable(case: &TestCase) -> ParseResult {
    match rable::parse(&case.input, case.extglob) {
        Ok(nodes) => {
            let actual = nodes
                .iter()
                .map(|n| format!("{n}"))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            if case.expected == "<error>" {
                return ParseResult::WrongOutput; // expected error but got success
            }

            if actual.trim() == case.expected.trim()
                || normalize_whitespace(&actual) == normalize_whitespace(&case.expected)
            {
                ParseResult::Pass
            } else {
                ParseResult::WrongOutput
            }
        }
        Err(_) => {
            if case.expected == "<error>" {
                ParseResult::ExpectedError
            } else {
                ParseResult::ParseError
            }
        }
    }
}

fn run_tree_sitter(case: &TestCase, parser: &mut tree_sitter::Parser) -> ParseResult {
    match parser.parse(&case.input, None) {
        Some(tree) => {
            let root = tree.root_node();
            if case.expected == "<error>" {
                // For error-expected cases: tree-sitter "passes" if it also has errors
                if has_error(&root) {
                    return ParseResult::ExpectedError;
                }
                return ParseResult::WrongOutput; // parsed successfully but should have errored
            }
            if has_error(&root) {
                ParseResult::ParseError
            } else {
                ParseResult::Pass
            }
        }
        None => {
            if case.expected == "<error>" {
                ParseResult::ExpectedError
            } else {
                ParseResult::ParseError
            }
        }
    }
}

struct ComparisonStats {
    file_name: String,
    total: usize,
    rable_pass: usize,
    ts_pass: usize,
    both_pass: usize,
    rable_only: usize,
    ts_only: usize,
    both_fail: usize,
    error_cases: usize,
    details: Vec<String>,
}

fn compare_file(path: &Path, parser: &mut tree_sitter::Parser, verbose: bool) -> ComparisonStats {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let content = fs::read_to_string(path).unwrap_or_default();
    let cases = parse_test_file(&content);

    let mut stats = ComparisonStats {
        file_name,
        total: 0,
        rable_pass: 0,
        ts_pass: 0,
        both_pass: 0,
        rable_only: 0,
        ts_only: 0,
        both_fail: 0,
        error_cases: 0,
        details: Vec::new(),
    };

    for case in &cases {
        if case.expected == "<error>" {
            stats.error_cases += 1;
            continue; // skip error cases — not meaningful for accuracy comparison
        }

        stats.total += 1;
        let rable_result = run_rable(case);
        let ts_result = run_tree_sitter(case, parser);

        let rable_ok = rable_result == ParseResult::Pass;
        let ts_ok = ts_result == ParseResult::Pass;

        if rable_ok {
            stats.rable_pass += 1;
        }
        if ts_ok {
            stats.ts_pass += 1;
        }

        match (rable_ok, ts_ok) {
            (true, true) => stats.both_pass += 1,
            (true, false) => {
                stats.rable_only += 1;
                if verbose {
                    stats.details.push(format!(
                        "  RABLE-ONLY :: {} | input: {:?}",
                        case.name, case.input,
                    ));
                }
            }
            (false, true) => {
                stats.ts_only += 1;
                if verbose {
                    stats.details.push(format!(
                        "  TS-ONLY    :: {} | input: {:?}",
                        case.name, case.input,
                    ));
                }
            }
            (false, false) => stats.both_fail += 1,
        }
    }

    stats
}

fn make_parser() -> Result<tree_sitter::Parser, tree_sitter::LanguageError> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_bash::LANGUAGE.into())?;
    Ok(parser)
}

fn collect_test_files(dir: &Path) -> Vec<std::path::PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = read_dir
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "tests"))
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

#[allow(clippy::cast_precision_loss)]
fn pct(num: usize, den: usize) -> f64 {
    if den > 0 {
        (num as f64) / (den as f64) * 100.0
    } else {
        0.0
    }
}

struct GrandTotals {
    total: usize,
    rable: usize,
    ts: usize,
    both: usize,
    rable_only: usize,
    ts_only: usize,
    both_fail: usize,
    errors: usize,
}

impl GrandTotals {
    const fn new() -> Self {
        Self {
            total: 0,
            rable: 0,
            ts: 0,
            both: 0,
            rable_only: 0,
            ts_only: 0,
            both_fail: 0,
            errors: 0,
        }
    }

    const fn add(&mut self, stats: &ComparisonStats) {
        self.total += stats.total;
        self.rable += stats.rable_pass;
        self.ts += stats.ts_pass;
        self.both += stats.both_pass;
        self.rable_only += stats.rable_only;
        self.ts_only += stats.ts_only;
        self.both_fail += stats.both_fail;
        self.errors += stats.error_cases;
    }

    fn print_summary(&self) {
        eprintln!("{}", "=".repeat(95));
        eprintln!(
            "{:<45} {:>5} {:>7} {:>7} {:>7} {:>7} {:>7}",
            "TOTAL", self.total, self.rable, self.ts, self.both, self.rable_only, self.ts_only,
        );
        eprintln!();
        eprintln!(
            "Rable accuracy:        {}/{} ({:.1}%)",
            self.rable,
            self.total,
            pct(self.rable, self.total)
        );
        eprintln!(
            "Tree-sitter accuracy:  {}/{} ({:.1}%)",
            self.ts,
            self.total,
            pct(self.ts, self.total)
        );
        eprintln!("Both pass:             {}", self.both);
        eprintln!("Rable-only pass:       {}", self.rable_only);
        eprintln!("Tree-sitter-only pass: {}", self.ts_only);
        eprintln!("Both fail:             {}", self.both_fail);
        eprintln!("Skipped (error cases): {}", self.errors);
        eprintln!();
        eprintln!("NOTE: Rable 'pass' = exact S-expression match with expected output.");
        eprintln!("      Tree-sitter 'pass' = parsed without ERROR/MISSING nodes.");
        eprintln!(
            "      Tree-sitter's bar is lower — it only checks parsability, not correctness."
        );
    }
}

#[test]
fn compare_parsers() {
    let verbose = std::env::var("VERBOSE").is_ok();
    let Ok(mut parser) = make_parser() else {
        return;
    };

    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut all_files = collect_test_files(&base.join("tests/parable"));
    all_files.extend(collect_test_files(&base.join("tests/oracle")));

    let mut totals = GrandTotals::new();

    eprintln!();
    eprintln!(
        "{:<45} {:>5} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "File", "Total", "Rable", "TS", "Both", "R-only", "TS-only",
    );
    eprintln!("{}", "-".repeat(95));

    for path in &all_files {
        let stats = compare_file(path, &mut parser, verbose);
        if stats.total == 0 {
            continue;
        }
        eprintln!(
            "{:<45} {:>5} {:>7} {:>7} {:>7} {:>7} {:>7}",
            stats.file_name,
            stats.total,
            stats.rable_pass,
            stats.ts_pass,
            stats.both_pass,
            stats.rable_only,
            stats.ts_only,
        );
        for detail in &stats.details {
            eprintln!("{detail}");
        }
        totals.add(&stats);
    }

    totals.print_summary();
}
