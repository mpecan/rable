"""Differential fuzzer: compares Rable against bash-oracle and/or Parable.

Modes:
  mutate   — Mutate existing .tests inputs to find edge cases
  generate — Generate random bash fragments from a grammar
  minimize — Reduce a failing input to its minimal form

Usage:
  python tests/fuzz.py [mode] [options]
  python tests/fuzz.py mutate -n 10000
  python tests/fuzz.py generate -n 5000 --layer 1-3
  python tests/fuzz.py minimize "failing input here"
"""

import argparse
import os
import random
import shutil
import string
import subprocess
import sys
import time

# --- Configuration ---

BASH_ORACLE = os.environ.get(
    "BASH_ORACLE", os.path.expanduser("~/source/bash-oracle/bash-oracle")
)
HAS_ORACLE = os.path.isfile(BASH_ORACLE) and os.access(BASH_ORACLE, os.X_OK)

# --- Oracle interface ---


def run_oracle(source, extglob=False):
    """Run bash-oracle on input, return S-expression output or None on error."""
    if not HAS_ORACLE:
        return None
    cmd = [BASH_ORACLE, "--dump-ast"]
    if extglob:
        cmd.append("-O")
        cmd.append("extglob")
    try:
        result = subprocess.run(
            cmd, input=source, capture_output=True, text=True, timeout=5,
            errors="replace",
        )
        if result.returncode != 0:
            return "<error>"
        return result.stdout.strip()
    except (subprocess.TimeoutExpired, OSError, UnicodeDecodeError):
        return None


def run_rable(source, extglob=False):
    """Run Rable parser, return S-expression output or '<error>'."""
    from rable import MatchedPairError, ParseError, parse

    try:
        nodes = parse(source, extglob=extglob)
        return " ".join(node.to_sexp() for node in nodes)
    except (ParseError, MatchedPairError):
        return "<error>"


def run_parable(source, extglob=False):
    """Run Parable parser, return S-expression output or '<error>'."""
    try:
        from parable import MatchedPairError, ParseError, parse

        nodes = parse(source, extglob=extglob)
        return " ".join(node.to_sexp() for node in nodes)
    except (ImportError, Exception):
        return None


# --- Input generators ---

# Characters commonly found in bash
BASH_CHARS = list(
    "abcdefghijklmnopqrstuvwxyz"
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    "0123456789"
    " \t\n"
    '|&;()<>$"\'\\`!{}[]#*?~=+-/@%^'
)

BASH_KEYWORDS = [
    "if", "then", "else", "elif", "fi",
    "while", "until", "do", "done",
    "for", "in", "case", "esac",
    "function", "select", "coproc", "time",
    "[[", "]]", "!", "{", "}",
]

BASH_OPERATORS = [
    "|", "||", "&&", ";", ";;", ";&", ";;&",
    "&", "|&", ">", ">>", "<", "<<", "<<<",
    ">&", "<&", ">|", "<>",
]

SIMPLE_COMMANDS = [
    "echo hello",
    "cat file",
    "ls -la",
    "grep pattern file",
    "true",
    "false",
    ":",
    "echo $HOME",
    "echo ${var:-default}",
    'echo "quoted"',
    "echo 'single'",
]

COMPOUND_TEMPLATES = [
    "if {cmd}; then {cmd}; fi",
    "while {cmd}; do {cmd}; done",
    "for i in 1 2 3; do {cmd}; done",
    "case $x in a) {cmd};; esac",
    "( {cmd} )",
    "{{ {cmd}; }}",
    "$( {cmd} )",
    "{cmd} | {cmd}",
    "{cmd} && {cmd}",
    "{cmd} || {cmd}",
    "{cmd} > /dev/null",
    "{cmd} 2>&1",
]


def load_test_inputs():
    """Load all inputs from .tests files as seed corpus."""
    inputs = []
    test_dir = os.path.join(os.path.dirname(__file__), "parable")
    for fname in sorted(os.listdir(test_dir)):
        if not fname.endswith(".tests"):
            continue
        path = os.path.join(test_dir, fname)
        with open(path) as f:
            lines = f.read().split("\n")
        i = 0
        while i < len(lines):
            if lines[i].startswith("=== "):
                i += 1
                input_lines = []
                while i < len(lines) and lines[i] != "---":
                    input_lines.append(lines[i])
                    i += 1
                if input_lines:
                    inputs.append("\n".join(input_lines))
            i += 1
    return inputs


def mutate(source):
    """Apply a random mutation to a bash source string."""
    if not source:
        return source
    mutation = random.choice([
        "insert_char",
        "delete_char",
        "replace_char",
        "swap_chars",
        "insert_keyword",
        "insert_operator",
        "duplicate_segment",
        "wrap_compound",
    ])

    chars = list(source)

    if mutation == "insert_char" and chars:
        pos = random.randint(0, len(chars))
        chars.insert(pos, random.choice(BASH_CHARS))

    elif mutation == "delete_char" and len(chars) > 1:
        pos = random.randint(0, len(chars) - 1)
        del chars[pos]

    elif mutation == "replace_char" and chars:
        pos = random.randint(0, len(chars) - 1)
        chars[pos] = random.choice(BASH_CHARS)

    elif mutation == "swap_chars" and len(chars) > 1:
        i = random.randint(0, len(chars) - 2)
        chars[i], chars[i + 1] = chars[i + 1], chars[i]

    elif mutation == "insert_keyword":
        pos = random.randint(0, len(chars))
        kw = random.choice(BASH_KEYWORDS)
        for c in reversed(" " + kw + " "):
            chars.insert(pos, c)

    elif mutation == "insert_operator":
        pos = random.randint(0, len(chars))
        op = random.choice(BASH_OPERATORS)
        for c in reversed(op):
            chars.insert(pos, c)

    elif mutation == "duplicate_segment" and len(chars) > 2:
        start = random.randint(0, len(chars) - 2)
        length = random.randint(1, min(10, len(chars) - start))
        segment = chars[start : start + length]
        pos = random.randint(0, len(chars))
        for j, c in enumerate(segment):
            chars.insert(pos + j, c)

    elif mutation == "wrap_compound":
        template = random.choice(COMPOUND_TEMPLATES)
        return template.replace("{cmd}", source)

    return "".join(chars)


def generate_bash(layer=1):
    """Generate a random bash fragment at the given complexity layer."""
    if layer <= 1:
        # Simple words and commands
        return random.choice([
            random.choice(SIMPLE_COMMANDS),
            "echo " + "".join(random.choices(string.ascii_lowercase, k=random.randint(1, 8))),
            random.choice(BASH_KEYWORDS),
            "".join(random.choices(string.ascii_lowercase + " $", k=random.randint(3, 20))),
        ])

    if layer == 2:
        # Simple compound
        cmd = generate_bash(1)
        template = random.choice(COMPOUND_TEMPLATES)
        return template.replace("{cmd}", cmd)

    # Nested compound (layer 3+)
    inner = generate_bash(layer - 1)
    template = random.choice(COMPOUND_TEMPLATES)
    return template.replace("{cmd}", inner)


def minimize(source, check_fn):
    """Delta-debug minimize: find smallest input that still triggers check_fn."""
    if not check_fn(source):
        print("Input does not trigger the failure.")
        return source

    # Try removing characters one at a time
    current = source
    i = 0
    while i < len(current):
        candidate = current[:i] + current[i + 1 :]
        if candidate and check_fn(candidate):
            current = candidate
        else:
            i += 1

    # Try removing lines
    lines = current.split("\n")
    if len(lines) > 1:
        i = 0
        while i < len(lines):
            candidate = "\n".join(lines[:i] + lines[i + 1 :])
            if candidate and check_fn(candidate):
                lines = lines[:i] + lines[i + 1 :]
                current = "\n".join(lines)
            else:
                i += 1

    return current


# --- Comparison ---


def normalize(s):
    """Normalize whitespace for comparison."""
    return " ".join(s.split()) if s else s


def compare(source, verbose=False):
    """Compare Rable output against oracle(s). Returns (match, details)."""
    rable_out = run_rable(source)

    oracle_out = run_oracle(source) if HAS_ORACLE else None
    parable_out = run_parable(source)

    reference = oracle_out if oracle_out is not None else parable_out
    if reference is None:
        return True, None  # No reference available

    rable_norm = normalize(rable_out)
    ref_norm = normalize(reference)

    if rable_norm == ref_norm:
        return True, None

    ref_name = "bash-oracle" if oracle_out is not None else "Parable"
    details = {
        "input": source,
        "rable": rable_out,
        ref_name: reference,
    }
    if verbose:
        # Also show the other reference if available
        if oracle_out is not None and parable_out is not None:
            details["Parable"] = parable_out

    return False, details


# --- Main ---


def cmd_mutate(args):
    """Mutation-based fuzzing."""
    seeds = load_test_inputs()
    if not seeds:
        print("No seed inputs found in tests/parable/")
        return 1

    print(f"Loaded {len(seeds)} seed inputs")
    ref = "bash-oracle" if HAS_ORACLE else ("Parable" if run_parable("echo") else "none")
    print(f"Reference: {ref}")
    if ref == "none":
        print("No reference parser available. Install Parable or bash-oracle.")
        return 1

    failures = []
    start = time.time()
    for i in range(args.n):
        seed = random.choice(seeds)
        # Apply 1-3 mutations
        mutated = seed
        for _ in range(random.randint(1, 3)):
            mutated = mutate(mutated)

        # Skip very long inputs
        if len(mutated) > 500:
            continue

        match, details = compare(mutated, verbose=args.verbose)
        if not match:
            failures.append(details)
            if args.verbose:
                print(f"\n[{i+1}/{args.n}] MISMATCH:")
                for k, v in details.items():
                    print(f"  {k}: {v!r}")
            if args.stop_after and len(failures) >= args.stop_after:
                print(f"\nStopping after {args.stop_after} failures")
                break

        if (i + 1) % 1000 == 0:
            elapsed = time.time() - start
            rate = (i + 1) / elapsed
            print(
                f"  [{i+1}/{args.n}] {len(failures)} failures, {rate:.0f} tests/sec"
            )

    elapsed = time.time() - start
    print(f"\n{args.n} mutations, {len(failures)} failures in {elapsed:.1f}s")
    if failures and not args.verbose:
        print(f"\nFirst {min(5, len(failures))} failures:")
        for d in failures[:5]:
            print(f"  Input: {d['input']!r}")
            for k, v in d.items():
                if k != "input":
                    print(f"    {k}: {v!r}")
    return 1 if failures else 0


def cmd_generate(args):
    """Grammar-based generation fuzzing."""
    ref = "bash-oracle" if HAS_ORACLE else ("Parable" if run_parable("echo") else "none")
    print(f"Reference: {ref}")
    if ref == "none":
        print("No reference parser available.")
        return 1

    layer_min, layer_max = 1, 3
    if args.layer:
        parts = args.layer.split("-")
        layer_min = int(parts[0])
        layer_max = int(parts[1]) if len(parts) > 1 else layer_min

    failures = []
    start = time.time()
    for i in range(args.n):
        layer = random.randint(layer_min, layer_max)
        source = generate_bash(layer)

        match, details = compare(source, verbose=args.verbose)
        if not match:
            failures.append(details)
            if args.verbose:
                print(f"\n[{i+1}/{args.n}] MISMATCH:")
                for k, v in details.items():
                    print(f"  {k}: {v!r}")
            if args.stop_after and len(failures) >= args.stop_after:
                break

        if (i + 1) % 1000 == 0:
            elapsed = time.time() - start
            rate = (i + 1) / elapsed
            print(
                f"  [{i+1}/{args.n}] {len(failures)} failures, {rate:.0f} tests/sec"
            )

    elapsed = time.time() - start
    print(f"\n{args.n} generated, {len(failures)} failures in {elapsed:.1f}s")
    if failures and not args.verbose:
        print(f"\nFirst {min(5, len(failures))} failures:")
        for d in failures[:5]:
            print(f"  Input: {d['input']!r}")
            for k, v in d.items():
                if k != "input":
                    print(f"    {k}: {v!r}")
    return 1 if failures else 0


def cmd_minimize(args):
    """Minimize a failing input."""
    source = args.input

    def check(s):
        match, _ = compare(s)
        return not match

    print(f"Original ({len(source)} chars): {source!r}")
    result = minimize(source, check)
    print(f"Minimized ({len(result)} chars): {result!r}")

    _, details = compare(result, verbose=True)
    if details:
        for k, v in details.items():
            print(f"  {k}: {v!r}")
    return 0


def main():
    parser = argparse.ArgumentParser(
        description="Differential fuzzer for Rable bash parser"
    )
    sub = parser.add_subparsers(dest="mode")

    p_mutate = sub.add_parser("mutate", help="Mutation-based fuzzing")
    p_mutate.add_argument("-n", type=int, default=10000, help="Number of iterations")
    p_mutate.add_argument("--stop-after", type=int, default=0, help="Stop after N failures")
    p_mutate.add_argument("-v", "--verbose", action="store_true")

    p_gen = sub.add_parser("generate", help="Grammar-based generation")
    p_gen.add_argument("-n", type=int, default=5000, help="Number of iterations")
    p_gen.add_argument("--layer", type=str, default="1-3", help="Complexity layer range")
    p_gen.add_argument("--stop-after", type=int, default=0, help="Stop after N failures")
    p_gen.add_argument("-v", "--verbose", action="store_true")

    p_min = sub.add_parser("minimize", help="Minimize a failing input")
    p_min.add_argument("input", help="The failing input string")

    args = parser.parse_args()

    if args.mode == "mutate":
        sys.exit(cmd_mutate(args))
    elif args.mode == "generate":
        sys.exit(cmd_generate(args))
    elif args.mode == "minimize":
        sys.exit(cmd_minimize(args))
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
