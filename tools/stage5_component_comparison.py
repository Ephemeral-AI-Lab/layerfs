#!/usr/bin/env python3
"""Stage 5 component comparison: reference primitives versus candidate primitives.

Both arms receive the same prepared, strictly sorted input and perform the same
three primitive steps (sorted directory merge, sorted inline-inode merge, root
encoding). Excluded on BOTH arms, and stated in every receipt: normalization,
deriving final reference counts, full topology validation, reference ordering
preparation and physical persistence.

The driver refuses to compare timings unless the two arms agree on the base
directory, base table, base root, updated directory, updated table and final root
identities. One sample per case per arm; receipts are append-only.

Usage:
  python3 tools/stage5_component_comparison.py --output <fresh-directory>
"""

import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
CASES = (
    ("small", 200, 20),
    ("wide", 2_000, 200),
    ("large-few-changes", 20_000, 20),
)
TOOLCHAIN = "+1.85.1"


def run(command, cwd):
    started = time.monotonic()
    result = subprocess.run(
        command, cwd=cwd, capture_output=True, text=True, timeout=120
    )
    wall = time.monotonic() - started
    if result.returncode != 0:
        raise SystemExit(
            f"command failed ({result.returncode}): {' '.join(command)}\n{result.stderr}"
        )
    return result.stdout, wall


def parse(stdout):
    fields = {}
    for line in stdout.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "sample":
            fields["elapsed_ns"] = int(parts[3])
            fields["updated_directory"] = parts[5]
            fields["table"] = parts[7]
            fields["root"] = parts[9]
        elif len(parts) >= 2:
            fields[parts[0]] = parts[1]
    return fields


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument(
        "--profile",
        default="release",
        choices=("release", "debug"),
        help="build profile, applied to both arms",
    )
    arguments = parser.parse_args()
    output = pathlib.Path(arguments.output)
    if output.exists():
        raise SystemExit(f"refusing to reuse an existing output directory: {output}")
    output.mkdir(parents=True)

    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    dirty = subprocess.check_output(
        ["git", "status", "--porcelain", "--", "core/crates", "crates"],
        cwd=ROOT,
        text=True,
    ).strip()
    build_flags = ["--release"] if arguments.profile == "release" else []

    receipts = []
    print(f"source_commit {commit}")
    print(f"profile {arguments.profile}")
    print(f"working_tree_dirty {bool(dirty)}")
    for name, files, changes in CASES:
        row = {"case": name, "files": files, "changes": changes}
        for arm, manifest, package, example in (
            (
                "reference",
                "Cargo.toml",
                "layerfs-content",
                "stage5_component_reference",
            ),
            (
                "candidate",
                "core/Cargo.toml",
                "layerfs-content",
                "filesystem_primitives_candidate",
            ),
        ):
            command = [
                "cargo",
                TOOLCHAIN,
                "run",
                "--locked",
                "--manifest-path",
                manifest,
                "-p",
                package,
                "--example",
                example,
            ] + build_flags + [
                "--",
                "--files",
                str(files),
                "--changes",
                str(changes),
                "--samples",
                "1",
            ]
            stdout, wall = run(command, ROOT)
            fields = parse(stdout)
            row[arm] = {"fields": fields, "wall_s": round(wall, 3), "command": command}
            print(f"case {name} {arm} elapsed_ns {fields['elapsed_ns']} wall_s {wall:.3f}")
        identity_keys = (
            "base_directory",
            "base_table",
            "base_root",
            "updated_directory",
            "table",
            "root",
        )
        mismatch = [
            key
            for key in identity_keys
            if row["reference"]["fields"].get(key) != row["candidate"]["fields"].get(key)
        ]
        if mismatch:
            row["identity"] = "MISMATCH"
            row["mismatched"] = mismatch
            print(f"case {name} identity MISMATCH {mismatch}")
        else:
            row["identity"] = "MATCH"
            reference_ns = row["reference"]["fields"]["elapsed_ns"]
            candidate_ns = row["candidate"]["fields"]["elapsed_ns"]
            row["candidate_over_reference"] = round(candidate_ns / reference_ns, 6)
            print(
                f"case {name} identity MATCH candidate_over_reference "
                f"{row['candidate_over_reference']}"
            )
        receipts.append(row)

    sealed = {
        "source_commit": commit,
        "working_tree_dirty": bool(dirty),
        "profile": arguments.profile,
        "toolchain": TOOLCHAIN,
        "scope": "component comparison: sorted directory merge, sorted inline-inode "
        "merge and root encoding",
        "excluded_on_both_arms": [
            "input normalization",
            "deriving final reference counts",
            "full topology validation",
            "reference ordering preparation",
            "physical persistence",
        ],
        "cache_state": "warm in-process fixture; the base tree is built in the arm "
        "process before the timed region",
        "samples": "one per case per arm",
        "cases": receipts,
    }
    (output / "receipt.json").write_text(json.dumps(sealed, indent=2) + "\n")
    digest = hashlib.sha256((output / "receipt.json").read_bytes()).hexdigest()
    print(f"receipt {output / 'receipt.json'} sha256 {digest}")
    if any(row["identity"] != "MATCH" for row in receipts):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
