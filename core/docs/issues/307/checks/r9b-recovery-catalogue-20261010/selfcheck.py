#!/usr/bin/env python3
"""Self-check for the recovery catalogue. Read-only against the repository.

Runs generate.py twice into two scratch directories at the catalogue's own
source commit, compares the bytes with each other and with the delivered
files, and checks row identities. Prints a transcript on stdout; exits 1 on
any failed check.

  python3 -B selfcheck.py --scratch <dir> > selfcheck.txt
"""

import argparse
import hashlib
import json
import os
import random
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FILES = ("reference-recovery-catalogue.jsonl", "missing-pins.txt", "README.md")
EXCLUDED = ("crates/", "core/crates/", "core/vendor/", "core/reference-tests/",
            "core/docs/issues/307/checks/r9b-recovery-catalogue-20261010/")
SEED = 20261010
FAILED = []


def git(*args):
    return subprocess.run(("git",) + args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          check=True).stdout.decode()


def sha256(path):
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def check(label, ok, detail=""):
    print("%s  %s%s" % ("PASS" if ok else "FAIL", label, (" -- " + detail) if detail else ""))
    if not ok:
        FAILED.append(label)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--scratch", required=True, help="scratch directory for the two runs")
    args = parser.parse_args()
    top = git("rev-parse", "--show-toplevel").strip()
    rel = os.path.relpath(HERE, top)
    os.chdir(top)

    with open(os.path.join(HERE, FILES[0]), encoding="utf-8") as handle:
        lines = handle.read().split("\n")
    header = json.loads(lines[0])
    rev = header["catalogue_source_commit"]
    head = git("rev-parse", "HEAD").strip()
    print("catalogue_source_commit %s" % rev)
    print("HEAD at check time      %s%s" % (head, "" if head == rev else
                                            "  (differs: checks below use the catalogue commit)"))
    print("generate.py sha256      %s" % sha256(os.path.join(HERE, "generate.py")))
    check("header script_sha256 equals generate.py",
          header["script_sha256"] == sha256(os.path.join(HERE, "generate.py")))
    print()

    print("== 1. two runs, byte comparison ==")
    runs = []
    for name in ("run-a", "run-b"):
        out = os.path.join(args.scratch, name)
        command = [sys.executable, "-B", os.path.join(rel, "generate.py"), "--rev", rev,
                   "--output-dir", out]
        print("$ python3 -B %s --rev %s --output-dir <scratch>/%s" % (command[2], rev, name))
        proc = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        print("  exit %d; %s" % (proc.returncode, proc.stderr.decode().strip()))
        check("%s exit status 0" % name, proc.returncode == 0)
        runs.append(out)
    for name in FILES:
        a, b = (sha256(os.path.join(run, name)) for run in runs)
        delivered = sha256(os.path.join(HERE, name))
        print("  %s\n    run-a     %s\n    run-b     %s\n    delivered %s" % (name, a, b, delivered))
        check("run-a and run-b identical: " + name, a == b)
        check("delivered file identical to run-a: " + name, a == delivered)
    print()

    print("== 2. line counts (wc -l) ==")
    for name in FILES:
        with open(os.path.join(HERE, name), "rb") as handle:
            print("  %8d %s" % (handle.read().count(b"\n"), name))
    rows = [json.loads(line) for line in lines[1:] if line]
    check("first line is the header", header.get("record_type") == "header")
    check("header row_count equals rows present", header["row_count"] == len(rows),
          "%d rows" % len(rows))
    print()

    print("== 3. evidence_blob equals git rev-parse <catalogue commit>:<path> ==")
    sample = random.Random(SEED).sample(rows, min(200, len(rows)))
    bad = 0
    for row in sample:
        actual = git("rev-parse", "%s:%s" % (rev, row["evidence_path"])).strip()
        bad += actual != row["evidence_blob"]
    check("seeded sample (seed %d) of %d rows, one git rev-parse each" % (SEED, len(sample)),
          bad == 0, "%d mismatches" % bad)
    tree = {}
    for record in subprocess.run(("git", "ls-tree", "-r", "-z", rev), stdout=subprocess.PIPE,
                                 check=True).stdout.split(b"\0"):
        if record:
            meta, path = record.split(b"\t", 1)
            tree[path.decode("utf-8", "surrogateescape")] = meta.split()[2].decode()
    bad = sum(1 for row in rows if tree.get(row["evidence_path"]) != row["evidence_blob"])
    check("all %d rows against one git ls-tree -r" % len(rows), bad == 0, "%d mismatches" % bad)
    print()

    print("== 4. row invariants ==")
    paths = [row["evidence_path"] for row in rows]
    inside = [p for p in paths if p.startswith(EXCLUDED)]
    check("no row path inside an excluded tree or this directory", not inside,
          "%d inside" % len(inside))
    check("no row path is absolute", not any(p.startswith("/") for p in paths))
    check("rows sorted by path and unique", paths == sorted(set(paths)))
    check("original_verdict_touched is false in every row and in the header",
          header["original_verdict_touched"] is False
          and all(row["original_verdict_touched"] is False for row in rows))
    check("class is STRONG or REVIEW and every row has a signal",
          all(row["class"] in ("STRONG", "REVIEW") and row["signals"] for row in rows))
    check("MISSING_OBJECT rows: recover_reference_at null, recoverable_as_executed false",
          all(row["recover_reference_at"] is None and not row["recoverable_as_executed"]
              and row["recovery_basis"] == "unrecoverable_missing_pin"
              for row in rows if row["original_pin_status"] == "MISSING_OBJECT"))
    check("NOT_RECORDED rows: no pin, basis is the not-execution-pin fallback",
          all(row["original_pin"] is None and not row["recoverable_as_executed"]
              and row["recovery_basis"] == "recovery_commit_current_tree_not_execution_pin"
              for row in rows if row["original_pin_status"] == "NOT_RECORDED"))
    check("recoverable_as_executed only with a pin found in the row's own file",
          all(row["original_pin"]["association"] == "self"
              for row in rows if row["recoverable_as_executed"]))
    check("named_reference_paths capped at 8",
          all(len(row["named_reference_paths"]) <= 8 for row in rows))
    pins = sorted({row["original_pin"]["value"] for row in rows
                   if row["original_pin_status"] in ("LOCAL_COMMIT", "LOCAL_TREE")})
    types = subprocess.run(("git", "cat-file", "--batch-check"), stdout=subprocess.PIPE,
                           input=("\n".join(pins) + "\n").encode(), check=True).stdout.decode()
    check("every LOCAL_COMMIT/LOCAL_TREE pin value resolves in this clone (%d values)" % len(pins),
          "missing" not in types)
    gone = sorted({row["original_pin"]["value"] for row in rows
                   if row["original_pin_status"] == "MISSING_OBJECT"})
    types = subprocess.run(("git", "cat-file", "--batch-check"), stdout=subprocess.PIPE,
                           input=("\n".join(gone) + "\n").encode(), check=True).stdout.decode()
    check("every MISSING_OBJECT pin value is absent from this clone (%d values)" % len(gone),
          types.count("missing") == len(gone))
    print()

    print("== 5. introduced_commit, seeded sample of 25 rows ==")
    bad = 0
    for row in random.Random(SEED + 1).sample(rows, min(25, len(rows))):
        adds = git("log", "--topo-order", "--full-history", "-m", "--no-renames",
                   "--diff-filter=A", "--format=%H", rev, "--", row["evidence_path"]).split()
        bad += not adds or adds[-1] != row["introduced_commit"]
    check("introduced_commit equals the last line of a per-path git log --diff-filter=A",
          bad == 0, "%d mismatches" % bad)
    print()

    print("== 6. the reference trees are untouched in the working tree ==")
    status = git("status", "--short", "--", "crates", "core/reference-tests")
    check("git status --short -- crates core/reference-tests is empty", status == "")
    print("recovery commit trees: crates %s, core/reference-tests %s" % (
        git("rev-parse", header["recovery_commit"] + ":crates").strip(),
        git("rev-parse", header["recovery_commit"] + ":core/reference-tests").strip()))
    print()
    print("RESULT: %s" % ("FAIL: " + "; ".join(FAILED) if FAILED else "all checks passed"))
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
