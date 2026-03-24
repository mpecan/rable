# Contributing to Rable

Thank you for your interest in contributing to Rable! This project is a Rust reimplementation of [Parable](https://github.com/ldayton/Parable), and we take compatibility with Parable's output very seriously.

## Getting Started

```bash
# Clone the repo
git clone https://github.com/mpecan/rable.git
cd rable

# Run all checks
just check

# Full setup including Python environment
just setup
```

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

## Code Style

| Limit | Value |
|---|---|
| Line width | 100 chars |
| Function length | 60 lines max |
| Cognitive complexity | 15 max |
| Function arguments | 5 max |

These are enforced by `clippy.toml` and `.rustfmt.toml`.

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

## Project Structure

```
src/
  lib.rs          Public API
  ast.rs          AST node types
  token.rs        Token types
  error.rs        Error types
  lexer/          Context-sensitive tokenizer
  parser/         Recursive descent parser
  sexp/           S-expression output
  format/         Canonical bash reformatter
  python.rs       PyO3 bindings (feature-gated)
tests/
  integration.rs  Parable compatibility test runner
  parable/        Test corpus from the Parable project
  benchmark.py    Performance benchmark
```

## Python Bindings

The Python bindings are feature-gated under `python` and built with [maturin](https://www.maturin.rs/):

```bash
just develop        # build and install in development mode
just test-python    # run Parable's own test runner against Rable
just benchmark      # compare performance
```

## Questions?

Open an issue on GitHub. We're happy to help!

## Attribution

Rable's test suite comes from [Parable](https://github.com/ldayton/Parable) by [@ldayton](https://github.com/ldayton), licensed under MIT. We are grateful for their work which made this project possible.
