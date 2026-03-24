# Rable — Rust Bash Parser

## Constitution

1. **Compatibility is correctness** — output must match Parable's S-expressions exactly
2. **If it is not tested, it is not shipped** — every feature has unit and integration tests
3. **Simplicity is king** — solve problems with least complexity
4. **Correctness over speed** — match bash-oracle behavior, optimize later
5. **User first** — sensible defaults, zero-config, clear errors

## Project Overview

- **Library name**: `rable`
- **Purpose**: Complete GNU Bash 5.3-compatible parser, Rust implementation of Parable
- **Input**: Bash source string
- **Output**: AST nodes with S-expression serialization matching Parable exactly
- **Python bindings**: Via PyO3/maturin (feature-gated under `python`)

## Architecture

| Module | Responsibility |
|---|---|
| `src/lib.rs` | Library re-exports, public `parse()` entry point |
| `src/error.rs` | `ParseError`, `MatchedPairError` via thiserror |
| `src/token.rs` | `TokenType` enum, `Token` struct |
| `src/lexer.rs` | Hand-written context-sensitive tokenizer |
| `src/lexer_word.rs` | Word/expansion parsing (split for complexity) |
| `src/lexer_matched.rs` | Matched pair parsing for nested constructs |
| `src/ast.rs` | `Node` enum with all 50+ variants |
| `src/sexp.rs` | S-expression output via Display trait |
| `src/parser.rs` | Recursive descent parser (top-level) |
| `src/parser_compound.rs` | Compound commands: if/while/for/case/select/coproc |
| `src/parser_arith.rs` | Arithmetic expression parser |
| `src/parser_cond.rs` | Conditional expression parser `[[ ]]` |
| `src/python.rs` | PyO3 bindings (feature-gated) |
| `tests/` | Integration tests using Parable's .tests format |

## Code Limits

| Limit | Value | Enforced by |
|---|---|---|
| Line width | 100 chars | `.rustfmt.toml` |
| Function length | 60 lines | `clippy.toml` |
| Cognitive complexity | 15 | `clippy.toml` |
| Function arguments | 5 | `clippy.toml` |

## Clippy Rules

- **Denied**: `unwrap_used`, `expect_used`, `panic`, `todo`
- **Warned**: `pedantic`, `nursery` groups
- **Allowed**: `module_name_repetitions`, `must_use_candidate`

## Before Every Change

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## Commit Conventions

Conventional Commits: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`, `perf`.
One logical change per commit.

## Testing

Tests use `.tests` files from the Parable project in `tests/parable/`:
```
=== test name
bash source code
---
(expected s-expression)
---
```

Run with `cargo test`. The test runner reads `.tests` files, parses input, compares S-expression output.
