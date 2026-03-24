# Rable — Rust Bash Parser
# https://github.com/mpecan/rable

set dotenv-load := false

# Default: run all checks
default: fmt clippy test

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

# Run only the Parable compatibility suite with full output
test-parable:
    cargo test parable_test_suite -- --nocapture

# Run a specific test file (e.g., just test-file 12_command_substitution)
test-file name:
    RABLE_TEST={{name}} cargo test parable_test_suite -- --nocapture

# Full pre-commit check: format, lint, test
check: fmt clippy test

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

# --- CI Helpers ---

# Run exactly what CI runs
ci: fmt clippy test

# Build the Python wheel
wheel:
    PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 .venv/bin/maturin build --release

# Clean build artifacts
clean:
    cargo clean
    rm -rf .venv target/wheels
