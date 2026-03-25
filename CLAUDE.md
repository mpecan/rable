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
| `src/ast.rs` | `Node` struct, `NodeKind` enum with 50+ variants, `Span` |
| `src/token.rs` | `TokenType` enum (62 variants), `Token` struct |
| `src/error.rs` | `RableError` (Parse / MatchedPair) via thiserror |
| `src/context.rs` | Parsing context and state management |
| `src/lexer/` | Hand-written context-sensitive tokenizer |
| `src/lexer/quotes.rs` | Quote and escape handling |
| `src/lexer/heredoc.rs` | Here-document processing |
| `src/lexer/words.rs` | Word boundary detection |
| `src/lexer/word_builder.rs` | Word assembly with segments |
| `src/lexer/expansions.rs` | Parameter, command, arithmetic expansion parsing |
| `src/lexer/operators.rs` | Operator recognition |
| `src/parser/` | Recursive descent parser (top-level) |
| `src/parser/compound.rs` | Compound commands: if/while/for/case/select/coproc |
| `src/parser/conditional.rs` | Conditional expression parser `[[ ]]` |
| `src/parser/helpers.rs` | Common parsing utilities |
| `src/parser/word_parts.rs` | Word segment processing |
| `src/sexp/` | S-expression output via Display trait |
| `src/sexp/word.rs` | Word segment formatting |
| `src/sexp/ansi_c.rs` | ANSI-C quoting escapes |
| `src/format/` | Canonical bash reformatter (command substitution content) |
| `src/python.rs` | PyO3 bindings (feature-gated) |
| `tests/integration.rs` | Integration test runner for .tests files |
| `tests/parable/` | Parable test corpus (36 files, 1,604 tests) |
| `tests/oracle/` | bash-oracle compatibility tests (11 files) |

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

Oracle tests in `tests/oracle/` provide additional coverage from bash-oracle fuzzing. Run with `just test-oracle`.
