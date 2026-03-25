# Contributing to Rable

Thank you for your interest in contributing to Rable! This project is a Rust reimplementation of [Parable](https://github.com/ldayton/Parable), and we take compatibility with Parable's output very seriously.

## Getting Started

```bash
# Clone the repo
git clone https://github.com/mpecan/rable.git
cd rable

# Run all checks (format, lint, test)
just check

# Full setup including Python environment
just setup
```

### Requirements

- **Rust 1.93+** — pinned in `rust-toolchain.toml`, installed automatically by rustup
- **Python 3.12+** — needed for Python bindings and fuzzing tools
- **[just](https://github.com/casey/just)** — task runner (`cargo install just` or `brew install just`)

## Development Workflow

1. **Make your changes**
2. **Run `just check`** — this formats, lints, and tests everything
3. **Run `just test-parable`** — verify 1604/1604 compatibility
4. **Commit** using [Conventional Commits](https://www.conventionalcommits.org/): `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

## Code Quality Requirements

All PRs must:

- Pass `cargo fmt --check` (no formatting issues)
- Pass `cargo clippy --all-targets -- -D warnings` (no lint warnings)
- Pass all 1,604 Parable compatibility tests
- Pass all unit tests
- Not introduce `unwrap()`, `expect()`, `panic!()`, or `todo!()` calls

### Code Limits

| Limit | Value | Enforced by |
|---|---|---|
| Line width | 100 chars | `.rustfmt.toml` |
| Function length | 60 lines max | `clippy.toml` |
| Cognitive complexity | 15 max | `clippy.toml` |
| Function arguments | 5 max | `clippy.toml` |

These are enforced by clippy and rustfmt — `just check` will catch violations.

## Adding Support for New Bash Syntax

1. **Add test cases** to the appropriate `tests/parable/*.tests` file using the format:
   ```
   === descriptive test name
   bash source code here
   ---
   (expected s-expression output)
   ---
   ```

2. **Implement** the lexer/parser/formatter changes needed

3. **Verify** that the S-expression output matches Parable's — use Parable's `bash-oracle` if unsure

4. **Run the full suite**: `just test-parable` must show 1604/1604 (or more, if you added tests)

### Running a specific test file

To iterate quickly on a particular area:

```bash
just test-file 12_command_substitution   # run a single test file
```

The `RABLE_TEST` environment variable filters which test file to run.

## Project Structure

```
src/
  lib.rs              Public API: parse() entry point, re-exports
  ast.rs              AST node types (NodeKind enum with 50+ variants)
  token.rs            Token types and lexer output
  error.rs            Error types (ParseError, MatchedPairError)
  context.rs          Parsing context and state management
  lexer/              Context-sensitive tokenizer
    mod.rs              Main lexer loop
    quotes.rs           Quote and escape handling
    heredoc.rs          Here-document processing
    words.rs            Word boundary detection
    word_builder.rs     Word assembly with segments
    expansions.rs       Parameter/command/arithmetic expansion parsing
    operators.rs        Operator recognition
    tests.rs            Lexer unit tests
  parser/             Recursive descent parser
    mod.rs              Top-level parsing
    compound.rs         if/while/for/case/select/coproc
    conditional.rs      [[ ]] expression parsing
    helpers.rs          Common parsing utilities
    word_parts.rs       Word segment processing
    tests.rs            Parser unit tests
  sexp/               S-expression output
    mod.rs              Main formatter
    word.rs             Word segment formatting
    ansi_c.rs           ANSI-C quoting escapes
  format/             Canonical bash reformatter
    mod.rs              Used for command substitution content
  python.rs           PyO3 bindings (feature-gated under "python")
tests/
  integration.rs      Test runner for .tests files
  parable/            Parable test corpus (36 files, 1,604 tests)
  oracle/             bash-oracle compatibility tests (11 files)
  run_tests.py        Python test harness for Parable compatibility
  benchmark.py        Performance comparison vs Parable
  fuzz.py             Differential fuzzer (mutate/generate/minimize modes)
  generate_oracle_tests.py  Generate oracle tests from bash-oracle
examples/
  basic.rs            Basic usage example
```

## Python Bindings

The Python bindings are feature-gated under `python` and built with [maturin](https://www.maturin.rs/):

```bash
just venv           # create virtual environment (one-time)
just develop        # build and install in development mode
just test-python    # run Parable's own test runner against Rable
just benchmark      # compare performance
```

## Fuzzing

Rable includes a differential fuzzer that compares output against Parable to catch edge-case divergences:

```bash
just setup                    # one-time Python environment setup
just fuzz-mutate 50000        # mutate existing test inputs (default: 10,000)
just fuzz-generate 10000      # generate random bash fragments (default: 5,000)
just fuzz-minimize 'input'    # minimize a failing input
```

If you find a divergence, you can generate oracle tests from it:

```bash
just fuzz-generate-tests      # regenerate oracle test files (requires bash-oracle)
just test-oracle              # run oracle test suite
```

## CI

CI runs on every push to `main` and on pull requests. It includes:

- **Lint** — format check + clippy
- **Test** — full Rust test suite + oracle compatibility report
- **Python** — build PyO3 bindings, run Python tests, Parable compatibility
- **Benchmark** — performance comparison vs Parable (PRs only)

Run `just ci` locally to replicate what CI does.

## Questions?

Open an issue on GitHub. We're happy to help!

## Attribution

Rable's test suite comes from [Parable](https://github.com/ldayton/Parable) by [@ldayton](https://github.com/ldayton), licensed under MIT. We are grateful for their work which made this project possible.
