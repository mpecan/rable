# Rable — Rust Bash Parser
# https://github.com/mpecan/rable

set dotenv-load := false

# Default: run all checks
default: fmt clippy test lint-extra

# --- Development ---

# Format all Rust code
fmt:
    cargo fmt --all

# Run clippy with strict settings
clippy:
    cargo clippy --all-targets -- -D warnings

# Run all Rust tests (unit + integration)
test:
    cargo test --all-targets

# Enforce file length limits (soft 500 / hard 700 via cargo-lint-extra).
# Configured in .cargo-lint-extra.toml.
#
# NOTE: `cargo lint-extra -W` has a bug where it exits 1 even when zero
# diagnostics are reported, so we can't rely on it for "warnings as errors".
# Instead: run once for human output, then once with --format json and
# fail if the output is anything other than `[]`. This catches both soft
# (warn) and hard (error) file-length violations.
lint-extra:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo lint-extra
    diagnostics=$(cargo lint-extra --format json)
    if [ "$diagnostics" != "[]" ]; then
        echo "cargo lint-extra reported diagnostics — see output above" >&2
        exit 1
    fi

# Run only the Parable compatibility suite with full output
test-parable:
    cargo test parable_test_suite -- --nocapture

# Run a specific test file (e.g., just test-file 12_command_substitution)
test-file name:
    RABLE_TEST={{name}} cargo test parable_test_suite -- --nocapture

# Full pre-commit check: format, lint, test, file-length enforcement
check: fmt clippy test lint-extra

# --- Python Bindings ---

# Set up a Python virtual environment with maturin
venv:
    python3 -m venv .venv
    .venv/bin/pip install --upgrade pip
    .venv/bin/pip install maturin

# Build and install the Python bindings (requires venv)
develop: _ensure-venv
    PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 .venv/bin/maturin develop

# Run the Parable test suite via the Python bindings
test-python: develop _install-parable
    .venv/bin/python3 -c "\
    import sys; import rable as p; sys.modules['parable'] = p; \
    exec(open('tests/run_tests.py').read())" -- tests/parable/

# --- Benchmarks ---

# Run the performance benchmark (Rable vs Parable)
benchmark: develop _install-parable
    .venv/bin/python3 tests/benchmark.py

# --- Setup ---

# Full setup: venv, Python bindings, Parable for comparison
setup: venv develop _install-parable
    @echo "Setup complete. Run 'just benchmark' to compare performance."

# Install Parable from GitHub for benchmarking
_install-parable: _ensure-venv
    @.venv/bin/pip show parable >/dev/null 2>&1 && \
        .venv/bin/python3 -c "from parable import parse; parse('echo ok')" 2>/dev/null || \
        .venv/bin/pip install git+https://github.com/ldayton/Parable.git

# Ensure venv exists
_ensure-venv:
    @test -d .venv || (echo "Run 'just venv' first" && exit 1)

# --- Fuzzing ---

# Differential fuzzer: mutate existing test inputs (default 10k iterations)
fuzz-mutate n="10000": develop _install-parable
    .venv/bin/python3 tests/fuzz.py mutate -n {{n}}

# Differential fuzzer: generate random bash fragments
fuzz-generate n="5000": develop _install-parable
    .venv/bin/python3 tests/fuzz.py generate -n {{n}}

# Minimize a failing fuzzer input to its smallest form
fuzz-minimize input: develop _install-parable
    .venv/bin/python3 tests/fuzz.py minimize "{{input}}"

# Regenerate oracle test cases from bash-oracle fuzzing (requires bash-oracle)
fuzz-generate-tests: develop
    .venv/bin/python3 tests/generate_oracle_tests.py

# Compare rable vs tree-sitter-bash accuracy (VERBOSE=1 for details)
compare-tree-sitter:
    cargo test compare_parsers -- --nocapture

# Run the oracle test suite (aspirational — does not fail build)
test-oracle:
    cargo test oracle_test_suite -- --nocapture

# Build bash-oracle from source (requires autotools)
build-oracle:
    @if [ -d ~/source/bash-oracle ]; then \
        echo "bash-oracle source exists, rebuilding..."; \
    else \
        echo "Cloning bash-oracle..."; \
        mkdir -p ~/source && git clone https://github.com/ldayton/bash-oracle.git ~/source/bash-oracle; \
    fi
    cd ~/source/bash-oracle && ./configure && make

# --- CI Helpers ---

# Run exactly what CI runs
ci: fmt clippy test lint-extra

# Build the Python wheel
wheel:
    PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 .venv/bin/maturin build --release

# Clean build artifacts
clean:
    cargo clean
    rm -rf .venv target/wheels
