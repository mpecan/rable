"""Differential runs of Rable against bash-oracle.

bash-oracle is the reference (see CLAUDE.md). This script measures how far
Rable's output is from it and checks that a refactor changes nothing.

Subcommands:
  corpus              every input in tests/parable and tests/oracle
  fuzz                seeded mutations of the corpus (reproducible)
  probe FILE          inputs listed one per line (\\n escapes allowed)
  snapshot OUT        Rable's own output for corpus + fuzz inputs
  compare A B         inputs whose output differs between two snapshots

Every difference has one of three kinds:
  oracle_err  bash rejects the input, Rable accepts it
  rable_err   bash accepts the input, Rable rejects it
  differ      both parse it, to different ASTs

Usage:
  python tests/oracle_diff.py corpus
  python tests/oracle_diff.py fuzz -n 300000 --seed 20261107 --out diffs.jsonl
  python tests/oracle_diff.py snapshot before.jsonl   # then change code, rebuild
  python tests/oracle_diff.py snapshot after.jsonl
  python tests/oracle_diff.py compare before.jsonl after.jsonl
"""

import argparse
import json
import os
import random
import sys
from collections import Counter
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, os.path.dirname(__file__))
import fuzz  # noqa: E402

TESTS = os.path.dirname(os.path.abspath(__file__))


def load_corpus():
    """Yield (name, source, extglob) for every .tests case in both corpora."""
    for sub in ("parable", "oracle"):
        directory = os.path.join(TESTS, sub)
        for fname in sorted(os.listdir(directory)):
            if not fname.endswith(".tests"):
                continue
            lines = open(os.path.join(directory, fname)).read().split("\n")
            extglob = False
            i = 0
            while i < len(lines):
                if lines[i].startswith("# @extglob"):
                    extglob = True
                if lines[i].startswith("=== "):
                    name = lines[i][4:]
                    i += 1
                    source = []
                    while i < len(lines) and lines[i] != "---":
                        source.append(lines[i])
                        i += 1
                    text = "\n".join(source)
                    yield f"{sub}/{fname}::{name}", text, extglob or "# @extglob" in text
                i += 1


def fuzz_inputs(n, seed):
    """Yield n reproducible mutations of the corpus inputs (at most 500 chars)."""
    random.seed(seed)
    seeds = fuzz.load_test_inputs()
    for k in range(n):
        source = random.choice(seeds)
        for _ in range(random.randint(1, 3)):
            source = fuzz.mutate(source)
        if len(source) <= 500:
            yield f"fuzz#{k}", source, False


def compare_one(item):
    """Return a divergence record for one input, or None if Rable matches bash."""
    name, source, extglob = item
    oracle = fuzz.run_oracle(source, extglob=extglob)
    if oracle is None:
        return None
    rable = fuzz.run_rable(source, extglob=extglob)
    if fuzz.normalize(oracle) == fuzz.normalize(rable):
        return None
    if oracle == "<error>":
        kind = "oracle_err"
    elif rable == "<error>":
        kind = "rable_err"
    else:
        kind = "differ"
    return {"name": name, "kind": kind, "input": source, "oracle": oracle, "rable": rable}


def run_diff(items, out_path):
    """Compare every item in parallel; print a summary, optionally write JSONL."""
    if not fuzz.HAS_ORACLE:
        sys.exit(f"bash-oracle not found at {fuzz.BASH_ORACLE} (run `just build-oracle`)")
    items = list(items)
    counts = Counter()
    out = open(out_path, "w") if out_path else None
    with ProcessPoolExecutor(os.cpu_count()) as pool:
        for record in pool.map(compare_one, items, chunksize=32):
            if record:
                counts[record["kind"]] += 1
                if out:
                    out.write(json.dumps(record) + "\n")
    print(f"{len(items)} inputs; differences: "
          + ", ".join(f"{k} {counts[k]}" for k in ("oracle_err", "rable_err", "differ")))
    return counts


def cmd_probe(path):
    """Print bash and Rable side by side for each line of a probe file."""
    diffs = 0
    for line in open(path):
        source = line.rstrip("\n").replace("\\n", "\n").replace("\\t", "\t")
        if not source:
            continue
        oracle = fuzz.run_oracle(source)
        rable = fuzz.run_rable(source)
        same = fuzz.normalize(oracle) == fuzz.normalize(rable)
        diffs += not same
        if same:
            print(f"ok    {source!r:40.40} {oracle[:70]}")
        else:
            print(f"DIFF  {source!r:40.40} bash={oracle[:60]}  rable={rable[:60]}")
    print(f"diffs: {diffs}")
    return 1 if diffs else 0


def snapshot_items(n, seed):
    yield from load_corpus()
    yield from fuzz_inputs(n, seed)


def cmd_snapshot(out_path, n, seed):
    """Write Rable's output for every corpus and fuzz input, one JSON line each."""
    with open(out_path, "w") as out:
        count = 0
        for _, source, extglob in snapshot_items(n, seed):
            out.write(json.dumps(fuzz.run_rable(source, extglob=extglob)) + "\n")
            count += 1
    print(f"{count} outputs written to {out_path}")


def cmd_compare(before_path, after_path, n, seed):
    """List inputs whose Rable output changed, and whether bash agrees with the new one."""
    sources = [(source, extglob) for _, source, extglob in snapshot_items(n, seed)]
    before = open(before_path).read().splitlines()
    after = open(after_path).read().splitlines()
    if not len(before) == len(after) == len(sources):
        sys.exit("snapshots were taken with different inputs (-n / --seed)")
    verdicts = Counter()
    seen = set()
    for (source, extglob), old, new in zip(sources, before, after):
        if old == new or source in seen:
            continue
        seen.add(source)
        oracle = fuzz.normalize(fuzz.run_oracle(source, extglob=extglob))
        old, new = json.loads(old), json.loads(new)
        verdict = ("now matches bash" if oracle == fuzz.normalize(new)
                   else "regressed" if oracle == fuzz.normalize(old)
                   else "differs both before and after")
        verdicts[verdict] += 1
        print(f"{verdict}: {source!r:.80}\n  before {old[:110]}\n  after  {new[:110]}")
    print(f"{len(seen)} changed inputs: {dict(verdicts)}")
    return 1 if verdicts["regressed"] else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("corpus")
    p.add_argument("--out", help="write differences as JSONL")
    for name in ("fuzz", "snapshot", "compare"):
        p = sub.add_parser(name)
        p.add_argument("-n", type=int, default=300000, help="number of mutations")
        p.add_argument("--seed", type=int, default=20261107)
        if name == "fuzz":
            p.add_argument("--out", help="write differences as JSONL")
        elif name == "snapshot":
            p.add_argument("out")
        else:
            p.add_argument("before")
            p.add_argument("after")
    sub.add_parser("probe").add_argument("file")
    args = parser.parse_args()

    if args.cmd == "corpus":
        run_diff(load_corpus(), args.out)
    elif args.cmd == "fuzz":
        run_diff(fuzz_inputs(args.n, args.seed), args.out)
    elif args.cmd == "probe":
        return cmd_probe(args.file)
    elif args.cmd == "snapshot":
        cmd_snapshot(args.out, args.n, args.seed)
    else:
        return cmd_compare(args.before, args.after, args.n, args.seed)
    return 0


if __name__ == "__main__":
    sys.exit(main())
