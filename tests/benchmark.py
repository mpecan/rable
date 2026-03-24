"""Performance benchmark: Rable (Rust/PyO3) vs Parable (Python)."""

import time
import statistics

# Test inputs of varying complexity
INPUTS = {
    "simple_command": "echo hello world",
    "pipeline": "cat file | grep pattern | sort | uniq -c | head -20",
    "if_else": "if [ -f /etc/passwd ]; then echo exists; else echo missing; fi",
    "for_loop": "for i in 1 2 3 4 5; do echo $i; done",
    "case_statement": """case "$1" in
  start) echo starting;;
  stop) echo stopping;;
  restart) echo restarting;;
  *) echo "unknown: $1";;
esac""",
    "nested_compound": """if [ -d "$dir" ]; then
  for f in "$dir"/*; do
    if [ -f "$f" ]; then
      cat "$f" | while read line; do
        echo ">> $line"
      done
    fi
  done
fi""",
    "redirects_expansions": 'exec 3>&1 4>&2; cmd1 > /dev/null 2>&1; echo "${var:-default}" >> "$log"',
    "command_substitution": 'result=$(echo "$(date +%Y)" | tr -d "\\n"); echo "$result"',
    "complex_real_world": """#!/bin/bash
set -euo pipefail
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for config in "$SCRIPT_DIR"/configs/*.conf; do
  if [ -f "$config" ]; then
    source "$config"
    if [[ "${ENABLED:-false}" == "true" ]]; then
      echo "Processing: $config"
      "${SCRIPT_DIR}/process.sh" "$config" 2>&1 | tee -a "$LOG_FILE"
    fi
  fi
done""",
    "array_heredoc": """arr=(one two three)
cat <<'EOF'
hello world
EOF
select opt in "${arr[@]}"; do
  case $opt in
    *) echo "$opt"; break;;
  esac
done""",
}

def benchmark_parser(parse_fn, name, inputs, iterations=1000):
    """Benchmark a parser function over all inputs."""
    results = {}
    for label, source in inputs.items():
        times = []
        # Warmup
        for _ in range(10):
            parse_fn(source)
        # Measure
        for _ in range(iterations):
            start = time.perf_counter_ns()
            parse_fn(source)
            elapsed = time.perf_counter_ns() - start
            times.append(elapsed)
        results[label] = {
            "median_ns": statistics.median(times),
            "mean_ns": statistics.mean(times),
            "p95_ns": sorted(times)[int(len(times) * 0.95)],
            "min_ns": min(times),
        }
    return results

def format_ns(ns):
    """Format nanoseconds to human-readable string."""
    if ns >= 1_000_000:
        return f"{ns / 1_000_000:.1f}ms"
    if ns >= 1_000:
        return f"{ns / 1_000:.1f}us"
    return f"{ns:.0f}ns"

def main():
    from parable import parse as parable_parse
    from rable import parse as rable_parse

    iterations = 1000
    print(f"Benchmarking {len(INPUTS)} inputs x {iterations} iterations each\n")

    print("Running Parable (Python)...")
    parable_results = benchmark_parser(parable_parse, "Parable", INPUTS, iterations)

    print("Running Rable (Rust/PyO3)...")
    rable_results = benchmark_parser(rable_parse, "Rable", INPUTS, iterations)

    # Print results
    print(f"\n{'Input':<28} {'Parable':>10} {'Rable':>10} {'Speedup':>10}")
    print("-" * 62)

    total_parable = 0
    total_rable = 0

    for label in INPUTS:
        p = parable_results[label]["median_ns"]
        r = rable_results[label]["median_ns"]
        speedup = p / r if r > 0 else float("inf")
        total_parable += p
        total_rable += r
        print(f"{label:<28} {format_ns(p):>10} {format_ns(r):>10} {speedup:>9.1f}x")

    print("-" * 62)
    overall = total_parable / total_rable if total_rable > 0 else float("inf")
    print(f"{'TOTAL':<28} {format_ns(total_parable):>10} {format_ns(total_rable):>10} {overall:>9.1f}x")

    print(f"\n{'Input':<28} {'Parable p95':>12} {'Rable p95':>12}")
    print("-" * 55)
    for label in INPUTS:
        p95_p = parable_results[label]["p95_ns"]
        p95_r = rable_results[label]["p95_ns"]
        print(f"{label:<28} {format_ns(p95_p):>12} {format_ns(p95_r):>12}")

if __name__ == "__main__":
    main()
