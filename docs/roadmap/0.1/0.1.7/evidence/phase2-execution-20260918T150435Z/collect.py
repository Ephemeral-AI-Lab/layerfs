#!/usr/bin/env python3
"""Phase 1 collection driver.

Runs the frozen Phase 0 workload set (CONTRACT.md §1) once per arm, writes a
fresh log per command, and appends the exit code and wall time to a round-local
manifest. Nothing here edits the product, the tests or the examples: it only
launches them. Append-only: an existing output path is refused.

Usage:
    python3 collect.py <round> <before|after> [set ...]

Sets: c1 (D1-D6), fs (D7-D9), edits (D10-D24), order (D25/D26), extra (D27),
c2 (D28/D29), diag (X1/X2/X3).  `all` runs every set.  A set name may also be
one of the `order` grid arms: order-250, order-500, order-1000, order-2000.
"""

import os
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[5].resolve()
TOOLCHAIN = "+1.85.1"
# The probe client links layerfs-content/ layerfs-storage statically, so it MUST
# be the artifact built from the tree under test. The original constant pointed
# at a /tmp copy that no step rebuilt; a stale binary measures the wrong tree
# (correction 2026-09-18, ROUND-README.md). B2 below builds exactly this path.
CLIENT = str(HERE / "client/target/release/phase0client")
CORE = REPO / "core/target/release/examples"

EDIT_CASES = ["small", "chunked", "small-to-large", "large-to-small", "batch"]


def run(round_dir, step, label, command, cwd=REPO):
    started = time.monotonic()
    merged = dict(os.environ)
    merged["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, env=merged)
    wall = time.monotonic() - started
    log = round_dir / "logs" / f"{step}-{label}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text(
        f"$ {' '.join(command)}\ncwd {cwd}\nexit_code {result.returncode}\n"
        f"wall_s {wall:.3f}\n--- stdout ---\n{result.stdout}--- stderr ---\n{result.stderr}"
    )
    with (round_dir / "commands.tsv").open("a") as handle:
        handle.write(f"{step}\t{label}\t{result.returncode}\t{wall:.3f}\t{' '.join(command)}\n")
    print(f"{step} {label} exit {result.returncode} wall {wall:.3f}s", flush=True)
    return result


def fresh(round_dir, step, label):
    path = round_dir / "output" / step / label
    if path.exists():
        raise SystemExit(f"refusing to reuse an existing output path: {path}")
    return path


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    round_name, arm = sys.argv[1], sys.argv[2]
    sets = sys.argv[3:] or ["all"]
    if "all" in sets:
        sets = ["build", "c1", "fs", "edits", "order", "c2-control", "extra", "v2", "c2", "diag"]
    round_dir = HERE / "rounds" / round_name / arm
    round_dir.mkdir(parents=True, exist_ok=True)

    if "build" in sets:
        run(round_dir, "B1", "core-examples",
            ["cargo", TOOLCHAIN, "build", "--release", "--offline", "--locked",
             "--manifest-path", "core/Cargo.toml", "--examples"])
        run(round_dir, "B2", "client",
            ["cargo", TOOLCHAIN, "build", "--release", "--offline", "--locked"],
            cwd=HERE / "client")

    if "build" in sets:
        # Artifact identity for this arm: the reader must be able to see which
        # binaries produced the rows. The client is the one that used to go stale.
        import hashlib

        hashed = [
            ("client", pathlib.Path(CLIENT)),
            ("filesystem_timing_c1", CORE / "filesystem_timing_c1"),
            ("measure_filesystem", CORE / "measure_filesystem"),
            ("measure_edits", CORE / "measure_edits"),
            ("edit_timing_c1", CORE / "edit_timing_c1"),
            ("edit_memory_probe", CORE / "edit_memory_probe"),
        ]
        lines = []
        for label, path in hashed:
            digest = hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else "MISSING"
            lines.append(f"{label}\t{digest}\t{path}")
        (round_dir / "artifacts.txt").write_text("\n".join(lines) + "\n")
        print("\n".join(lines), flush=True)

    if "c1" in sets:
        for step, case in zip(["D1", "D2", "D3", "D4", "D5", "D6"],
                              ["empty", "directory-update", "inode-update",
                               "hardlink-move", "subtree-remove", "attributes"]):
            out = fresh(round_dir, step, f"c1-{case}")
            out.mkdir(parents=True)
            run(round_dir, step, f"c1-{case}",
                [str(CORE / "filesystem_timing_c1"), "--case", case, "--output", str(out)])

    if "fs" in sets:
        for step, mode in zip(["D7", "D8", "D9"], ["c1", "c2", "pipeline"]):
            out = fresh(round_dir, step, f"fs-{mode}")
            run(round_dir, step, f"fs-{mode}-directory-update",
                [str(CORE / "measure_filesystem"), "--mode", mode, "--case", "directory-update",
                 "--output", str(out)])

    if "edits" in sets:
        step = 10
        for mode in ["c1", "c2", "pipeline"]:
            for case in EDIT_CASES:
                out = fresh(round_dir, f"D{step}", f"edits-{mode}-{case}")
                run(round_dir, f"D{step}", f"edits-{mode}-{case}",
                    [str(CORE / "measure_edits"), "--mode", mode, "--case", case,
                     "--threshold-bytes", "131072", "--output", str(out)])
                step += 1

    if "order" in sets or any(s.startswith("order-") for s in sets):
        if "order" in sets:
            run(round_dir, "D25", "order-default-4096", [CLIENT, "order", "4000", "2000", "4096"])
            run(round_dir, "D26", "order-forced-64", [CLIENT, "order", "4000", "2000", "64"])
        for size in [250, 500, 1000, 2000]:
            if f"order-{size}" in sets:
                run(round_dir, f"D26x{size}", f"order-forced-64-{size}",
                    [CLIENT, "order", str(size * 2), str(size), "64"])

    if "v2" in sets:
        # V2's vehicle rows (examples only; not part of the frozen set, which they
        # extend additively). M1 is the counting-allocator memory probe P1-14 is
        # gated on; M2/M3 are the two new edit_timing_c1 fixtures (P1-9's and
        # P1-8's positive anchors).
        run(round_dir, "M1", "edit-memory-probe",
            [str(CORE / "edit_memory_probe")])
        run(round_dir, "M2", "edit-timing-c1-delete",
            [str(CORE / "edit_timing_c1"), "--case", "delete"])
        run(round_dir, "M3", "edit-timing-c1-shrink",
            [str(CORE / "edit_timing_c1"), "--case", "shrink"])
        # M4 is P1-8's discriminating row: three retained runs of a chunked base
        # whose result is one whole-file object. Added with P1-8's commit
        # (ROUND-README.md correction 4); the frozen cases and fields are
        # unchanged and the `case:` line is additive.
        run(round_dir, "M4", "edit-timing-c1-split",
            [str(CORE / "edit_timing_c1"), "--case", "split"])

    if "c2-control" in sets:
        # Y1 is V7's live control for the page-cache spill counter: the same
        # single-row INSERT shape and one transaction on a harness-owned
        # connection with a deliberately tiny 8-page cache. It is a *diagnostic*,
        # never a gate sample (P0-2's `spillcontrol`, re-run here because the
        # product cannot read SQLITE_DBSTATUS itself).
        run(round_dir, "Y1", "c2-spill-control", [CLIENT, "c2", "20000", "diagnostic-small-cache"])

    if "extra" in sets:
        run(round_dir, "D27", "edit-timing-c1-nodes-read", [str(CORE / "edit_timing_c1")])

    if "c2" in sets or "c2-repeat" in sets:
        if "c2-repeat" not in sets:
            run(round_dir, "D28", "c2-ceiling-default", [CLIENT, "c2", "8191", "default"])
            run(round_dir, "D29", "c2-small-default", [CLIENT, "c2", "1023", "default"])
        # X3 is D28's labelled determinism re-run: the write-path rows (D28/D29)
        # had no repeat in the Phase 0/1 sets, and CONTRACT.md §2.4 requires one
        # per round for the primary row. Added with V5's round (ROUND-README.md
        # correction 1); every other frozen row is untouched.
        run(round_dir, "X3", "c2-ceiling-repeat", [CLIENT, "c2", "8191", "default"])

    if "diag" in sets or "X2" in sets:
        if "diag" in sets:
            run(round_dir, "X1", "order-forced-64-repeat", [CLIENT, "order", "4000", "2000", "64"])
        out = fresh(round_dir, "X2", "c1-subtree-remove-repeat")
        out.mkdir(parents=True)
        run(round_dir, "X2", "c1-subtree-remove-repeat",
            [str(CORE / "filesystem_timing_c1"), "--case", "subtree-remove", "--output", str(out)])


if __name__ == "__main__":
    main()
