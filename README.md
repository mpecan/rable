# Rable

**A complete GNU Bash 5.3-compatible parser, written in Rust.**

Rable is a from-scratch reimplementation of [Parable](https://github.com/ldayton/Parable) — the excellent Python-based bash parser by [@ldayton](https://github.com/ldayton). It produces identical S-expression output and provides a drop-in replacement Python API via [PyO3](https://pyo3.rs).

## Acknowledgments

**This project would not exist without [Parable](https://github.com/ldayton/Parable).**

Parable is a remarkable piece of work — a complete, well-tested bash parser that produces clean S-expression AST output validated against bash's own internal parser. Its comprehensive test suite (1,604 tests across 36 files) defines the gold standard for bash parsing correctness, and Rable's compatibility is measured entirely against it.

We are deeply grateful to [@ldayton](https://github.com/ldayton) for:
- Building a high-quality, MIT-licensed bash parser that others can learn from and build upon
- Creating the `bash-oracle` approach that validates parser output against bash itself
- Maintaining the extensive `.tests` corpus that made Rable's development possible
- Designing the clean S-expression output format that Rable faithfully reproduces

Rable exists because Parable showed the way. Thank you.

## Compatibility

| Metric | Value |
|---|---|
| Parable test compatibility | **1,604 / 1,604 (100%)** |
| Test files at 100% | **36 / 36** |
| S-expression output | Identical to Parable |

## Performance

Rable is approximately **9.5x faster** than Parable across all test inputs:

| Input Type | Parable | Rable | Speedup |
|---|---|---|---|
| Simple command | 41us | 5us | 8.1x |
| Pipeline (5 stages) | 144us | 14us | 10.6x |
| Nested compound | 265us | 27us | 10.0x |
| Complex real-world script | 640us | 67us | 9.5x |
| **Overall** | **2.1ms** | **221us** | **9.5x** |

Run `just benchmark` to reproduce these results on your machine.

## Installation

### As a Rust library

```toml
[dependencies]
rable = "0.1"
```

### As a Python package

```bash
pip install rable
```

Or build from source:

```bash
just setup       # creates venv, builds bindings, installs Parable
just develop     # rebuild after code changes
```

## Usage

### Rust

```rust
use rable::parse;

fn main() {
    let nodes = parse("echo hello | grep h", false).unwrap();
    for node in &nodes {
        println!("{node}");
    }
    // Output: (pipe (command (word "echo") (word "hello")) (command (word "grep") (word "h")))
}
```

### Python

```python
from rable import parse, ParseError, MatchedPairError

# Parse bash source into AST nodes
nodes = parse('if [ -f file ]; then cat file; fi')
for node in nodes:
    print(node.to_sexp())
# Output: (if (command (word "[") (word "-f") (word "file") (word "]")) (command (word "cat") (word "file")))

# Errors are raised as exceptions
try:
    parse('if')
except ParseError as e:
    print(f"Syntax error: {e}")

# Enable extended glob patterns
nodes = parse('echo @(foo|bar)', extglob=True)
```

The Python API is a **drop-in replacement** for Parable:

```python
# Before (Parable)
from parable import parse, ParseError, MatchedPairError

# After (Rable) — same API, ~10x faster
from rable import parse, ParseError, MatchedPairError
```

## Development

### Prerequisites

- Rust 1.93+ (see `rust-toolchain.toml`)
- Python 3.12+ (for Python bindings)
- [just](https://github.com/casey/just) (task runner)

### Quick start

```bash
just              # format, lint, test
just check        # same as above
just test-parable # run full Parable compatibility suite
just setup        # set up Python environment + benchmarks
just benchmark    # compare performance vs Parable
```

### Available commands

| Command | Description |
|---|---|
| `just` | Format, lint, and test (default) |
| `just fmt` | Format all Rust code |
| `just clippy` | Run clippy with strict settings |
| `just test` | Run all Rust tests |
| `just test-parable` | Run Parable compatibility suite |
| `just test-file NAME` | Run a specific test file |
| `just setup` | Full Python environment setup |
| `just develop` | Build and install Python bindings |
| `just test-python` | Run Parable's test runner with Rable |
| `just benchmark` | Performance benchmark vs Parable |
| `just ci` | Run exactly what CI runs |
| `just clean` | Clean build artifacts |

## Architecture

Rable is a hand-written recursive descent parser with a context-sensitive lexer:

| Module | Responsibility |
|---|---|
| `lexer/` | Context-sensitive tokenizer with heredoc, quote, and expansion handling |
| `parser/` | Recursive descent parser for all bash constructs |
| `ast.rs` | 50+ AST node types covering the full bash grammar |
| `sexp/` | S-expression output with word segment processing |
| `format/` | Canonical bash reformatter (used for command substitution content) |
| `python.rs` | PyO3 bindings (feature-gated) |

### Design principles

1. **Compatibility is correctness** — output matches Parable's S-expressions exactly
2. **If it is not tested, it is not shipped** — 1,604 integration tests + unit tests
3. **Simplicity is king** — solve problems with least complexity
4. **Correctness over speed** — match bash-oracle behavior, optimize later

## Contributing

Contributions are welcome! Please ensure:

1. **All tests pass**: `just check` must succeed
2. **Parable compatibility**: `just test-parable` must show 1604/1604
3. **Code quality**: No clippy warnings (`just clippy`)
4. **Formatting**: Code is formatted (`just fmt`)

### Adding new features

If you're adding support for new bash syntax:

1. Add test cases to the appropriate `.tests` file in `tests/parable/`
2. Implement the lexer/parser changes
3. Verify S-expression output matches what Parable would produce
4. Run `just test-parable` to confirm no regressions

### Code limits

| Limit | Value |
|---|---|
| Line width | 100 chars |
| Function length | 60 lines |
| Cognitive complexity | 15 |
| Function arguments | 5 |
| Clippy | `deny(unwrap_used, expect_used, panic, todo)` |

## License

MIT License. See [LICENSE](LICENSE) for details.

## Disclosure

Rable is a **complete reimplementation** of [Parable](https://github.com/ldayton/Parable) in Rust. It was built by studying Parable's test suite and output format, not by translating Parable's Python source code. The test corpus (`tests/parable/*.tests`) originates from the Parable project and is used under its MIT license.
