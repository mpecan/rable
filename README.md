# Rable

[![CI](https://github.com/mpecan/rable/actions/workflows/ci.yml/badge.svg)](https://github.com/mpecan/rable/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rable.svg)](https://crates.io/crates/rable)
[![docs.rs](https://docs.rs/rable/badge.svg)](https://docs.rs/rable)
[![PyPI](https://img.shields.io/pypi/v/rable.svg)](https://pypi.org/project/rable/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

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
| Minimum Rust version | 1.93 |
| Python version | 3.12+ |

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
use rable::{parse, NodeKind};

fn main() {
    // Parse bash source into AST nodes
    let nodes = parse("echo hello | grep h", false).unwrap();
    for node in &nodes {
        println!("{node}");
    }
    // Output: (pipe (command (word "echo") (word "hello")) (command (word "grep") (word "h")))

    // Inspect the AST via pattern matching
    if let NodeKind::Pipeline { commands, .. } = &nodes[0].kind {
        println!("Pipeline with {} stages", commands.len());
    }

    // Enable extended glob patterns (@(), ?(), *(), +(), !())
    let nodes = parse("echo @(foo|bar)", true).unwrap();
    println!("{}", nodes[0]);
}
```

**Error handling:**

```rust
match rable::parse("if", false) {
    Ok(nodes) => { /* use nodes */ }
    Err(e) => {
        eprintln!("line {}, pos {}: {}", e.line(), e.pos(), e.message());
    }
}
```

See [`examples/basic.rs`](examples/basic.rs) for a more complete example, or run it with:

```bash
cargo run --example basic
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

## API Reference

### `parse(source, extglob) -> Vec<Node>`

The main entry point. Parses a bash source string into a list of top-level AST nodes.

- **`source`** — the bash source code to parse
- **`extglob`** — set to `true` to enable extended glob patterns (`@()`, `?()`, `*()`, `+()`, `!()`)
- **Returns** — `Vec<Node>`, where each top-level command separated by newlines is a separate node
- **Errors** — `RableError::Parse` for syntax errors, `RableError::MatchedPair` for unclosed delimiters

### AST Types

The AST is built from `Node` structs, each containing a `NodeKind` variant and a source `Span`:

```rust
use rable::{Node, NodeKind, Span};
```

**Key `NodeKind` variants:**

| Category | Variants |
|---|---|
| **Basic** | `Word`, `Command`, `Pipeline`, `List` |
| **Compound** | `If`, `While`, `Until`, `For`, `ForArith`, `Select`, `Case`, `Function`, `Subshell`, `BraceGroup`, `Coproc` |
| **Redirections** | `Redirect`, `HereDoc` |
| **Expansions** | `ParamExpansion`, `ParamLength`, `ParamIndirect`, `CommandSubstitution`, `ProcessSubstitution`, `ArithmeticExpansion`, `AnsiCQuote`, `LocaleString` |
| **Arithmetic** | `ArithmeticCommand`, `ArithNumber`, `ArithVar`, `ArithBinaryOp`, `ArithUnaryOp`, `ArithTernary`, `ArithAssign`, and more |
| **Conditionals** | `ConditionalExpr`, `UnaryTest`, `BinaryTest`, `CondAnd`, `CondOr`, `CondNot` |
| **Other** | `Negation`, `Time`, `Array`, `Comment`, `Empty` |

Every node implements `Display`, producing S-expression output identical to Parable.

### Error Types

```rust
use rable::{RableError, Result};
```

Both error variants provide `.line()`, `.pos()`, and `.message()` accessors:

- **`RableError::Parse`** — syntax error (e.g., unexpected token, missing keyword)
- **`RableError::MatchedPair`** — unclosed delimiter (parenthesis, brace, bracket, or quote)

## Architecture

Rable is a hand-written recursive descent parser with a context-sensitive lexer:

```
Source string
  -> Lexer (context-sensitive tokenizer)
    -> Parser (recursive descent)
      -> AST (Node tree)
        -> S-expression output (via Display)
```

| Module | Responsibility |
|---|---|
| `lexer/` | Context-sensitive tokenizer with heredoc, quote, and expansion handling |
| `parser/` | Recursive descent parser for all bash constructs |
| `ast.rs` | 50+ AST node types covering the full bash grammar |
| `token.rs` | Token types and lexer output |
| `error.rs` | Error types with line/position information |
| `context.rs` | Parsing context and state management |
| `sexp/` | S-expression output with word segment processing |
| `format/` | Canonical bash reformatter (used for command substitution content) |
| `python.rs` | PyO3 bindings (feature-gated under `python`) |

### Design principles

1. **Compatibility is correctness** — output matches Parable's S-expressions exactly
2. **If it is not tested, it is not shipped** — 1,604 integration tests + unit tests
3. **Simplicity is king** — solve problems with least complexity
4. **Correctness over speed** — match bash-oracle behavior, optimize later

## Development

### Prerequisites

- Rust 1.93+ (pinned in `rust-toolchain.toml`)
- Python 3.12+ (for Python bindings)
- [just](https://github.com/casey/just) (task runner)

### Quick start

```bash
git clone https://github.com/mpecan/rable.git
cd rable
just              # format, lint, test
```

### Available commands

**Core development:**

| Command | Description |
|---|---|
| `just` | Format, lint, and test (default) |
| `just fmt` | Format all Rust code |
| `just clippy` | Run clippy with strict settings |
| `just test` | Run all Rust tests |
| `just test-parable` | Run Parable compatibility suite |
| `just test-file NAME` | Run a specific test file (e.g., `just test-file 12_command_substitution`) |
| `just check` | Same as `just` — format, lint, test |
| `just ci` | Run exactly what CI runs |

**Python bindings:**

| Command | Description |
|---|---|
| `just setup` | Full setup: venv + bindings + Parable for comparison |
| `just venv` | Create Python virtual environment with maturin |
| `just develop` | Build and install Python bindings in dev mode |
| `just test-python` | Run Parable's test runner against Rable's Python bindings |
| `just benchmark` | Performance benchmark vs Parable |
| `just wheel` | Build a release Python wheel |

**Fuzzing and oracle testing:**

| Command | Description |
|---|---|
| `just fuzz-mutate [N]` | Differential fuzzer: mutate existing test inputs (default 10,000 iterations) |
| `just fuzz-generate [N]` | Differential fuzzer: generate random bash fragments (default 5,000) |
| `just fuzz-minimize INPUT` | Minimize a failing fuzzer input to its smallest form |
| `just fuzz-generate-tests` | Regenerate oracle test cases from bash-oracle fuzzing |
| `just test-oracle` | Run the bash-oracle compatibility test suite |
| `just build-oracle` | Build bash-oracle from source (requires autotools) |

**Cleanup:**

| Command | Description |
|---|---|
| `just clean` | Clean build artifacts and venv |

## Testing

### Test corpus

Tests live in `tests/parable/` using Parable's `.tests` format:

```
=== test name
bash source code
---
(expected s-expression)
---
```

There are 36 test files covering words, commands, pipelines, lists, redirects, compound statements, loops, functions, expansions, arithmetic, here-documents, process substitution, conditionals, arrays, and more.

### Oracle tests

Additional tests in `tests/oracle/` are generated from `bash-oracle` differential fuzzing. These provide extra coverage beyond Parable's test suite and are run separately:

```bash
just test-oracle
```

### Differential fuzzing

The fuzzer (`tests/fuzz.py`) compares Rable's output against Parable on randomly generated or mutated bash inputs, catching edge-case divergences:

```bash
just setup               # one-time: set up Python environment
just fuzz-mutate 50000   # mutate existing test inputs
just fuzz-generate 10000 # generate random bash fragments
```

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide. The short version:

1. **All tests pass**: `just check` must succeed
2. **Parable compatibility**: `just test-parable` must show 1604/1604
3. **Code quality**: No clippy warnings (`just clippy`)
4. **Formatting**: Code is formatted (`just fmt`)
5. **Commit style**: [Conventional Commits](https://www.conventionalcommits.org/) (`feat`, `fix`, `refactor`, `test`, `docs`, `chore`)

## License

MIT License. See [LICENSE](LICENSE) for details.

## Disclosure

Rable is a **complete reimplementation** of [Parable](https://github.com/ldayton/Parable) in Rust. It was built by studying Parable's test suite and output format, not by translating Parable's Python source code. The test corpus (`tests/parable/*.tests`) originates from the Parable project and is used under its MIT license.
