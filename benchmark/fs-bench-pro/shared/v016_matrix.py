#!/usr/bin/env python3
"""Run the retained v0.1.6 invocation matrix and collect every receipt.

One selected invocation runs one case, one seed, one arm and one mode: a
performance receipt and a separate verification receipt. The shared measurement
lock is honoured by waiting for it, never by stealing or removing it. Nothing is
retried after a completed invocation: a miss is a miss and stays on disk.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import isolation
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[3]
BENCH = REPO / "benchmark" / "fs-bench-pro"
RESULTS = REPO / "benchmark-results" / "v016"
LOCK = isolation.worktree_lock_path()

CASES = json.loads(
    (REPO / "docs/roadmap/0.1/0.1.6/cases.json").read_text()
)["cases"]

EXTENDED = [
    ("mixed_load_bearing", "v016-mixed-exhaustive-100mb-5000-k100-v1", "verify"),
    ("mixed_load_bearing", "v016-mixed-exhaustive-500mb-30000-k100-v1", "verify"),
    ("multi_workspace_development", "v016-workspace-four-100mb-5000-k100-v1", "both"),
]


def family_of(case):
    return case["family"]


def registered(family, case, image):
    """Whether the host registry admits this case, so an unregistered row is
    reported as NOT_RUN instead of aborting the matrix."""
    binary = REPO / "target/release/fs-benchmark-pro"
    result = subprocess.run(
        [str(binary), "infra-list", family],
        capture_output=True,
        text=True,
        env={**os.environ, "LAYERFS_V013_IMAGE": image},
    )
    if result.returncode != 0:
        return False
    return any(
        row.get("scenario_id") == case
        for row in (json.loads(line) for line in result.stdout.splitlines() if line.startswith("{"))
    )


def run(argv, timeout=None, env=None):
    started = time.monotonic()
    result = subprocess.run(
        argv, cwd=REPO, capture_output=True, text=True, timeout=timeout, env=env
    )
    return result, time.monotonic() - started


def wait_for_lock(limit_seconds=3600):
    """The measurement lock is non-blocking by design; wait rather than steal."""
    import fcntl

    deadline = time.monotonic() + limit_seconds
    handle = LOCK.open("a")
    while time.monotonic() < deadline:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(handle, fcntl.LOCK_UN)
            return True
        except BlockingIOError:
            time.sleep(5.0)
    handle.close()
    return False


def perf(family, case, seed, image, extended=False, fresh=False):
    output = RESULTS / "performance" / case / str(seed) / "candidate" / "run1"
    if output.exists() and not fresh:
        return "RETAINED", output
    if output.exists():
        # A superseded performance receipt is archived, never deleted: the
        # earlier allowance produced a real, retained measurement.
        archive = output.with_name("run-" + time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
        output.rename(archive)
        print(f"perf {case}: archived superseded receipt to {archive.name}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        sys.executable,
        str(BENCH / "shared" / "runner.py"),
        "--family",
        family,
        "--case",
        case,
        "--seed",
        str(seed),
        "--image",
        image,
        "--perf-fast",
        "--timeout",
        "900",
        "--product-timeout",
        "840",
        "--setup-timeout",
        "900",
        "--output",
        str(output),
    ]
    if case in [row[1] for row in EXTENDED]:
        argv.append("--extended")
    result, wall = run(argv)
    print(f"perf {case} seed{seed}: rc={result.returncode} wall={wall:.1f}s", flush=True)
    if result.returncode != 0:
        print(result.stdout[-4000:], flush=True)
        print(result.stderr[-4000:], flush=True)
    return "RUN", output


def verify_only_case(family, case, seed, image, fresh=False):
    """A verify-only case: performance is explicitly N/A, never zero or PASS."""
    output = RESULTS / "verification" / case / str(seed) / "candidate" / "run1"
    if output.exists() and not fresh:
        return "RETAINED", output
    if output.exists():
        archive = output.with_name("run-" + time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
        output.rename(archive)
        print(f"verify {case}: archived superseded receipt to {archive.name}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        sys.executable,
        str(BENCH / "verify-selected.py"),
        "--family",
        family,
        "--case",
        case,
        "--seed",
        str(seed),
        "--source",
        source_identity(image),
        "--input",
        input_identity(family, case, seed, image),
        "--image",
        image,
        "--extended",
        "--output",
        str(output),
    ]
    result, wall = run(argv)
    print(f"verify {case} seed{seed}: rc={result.returncode} wall={wall:.1f}s", flush=True)
    receipt = output / "verification.json"
    if receipt.is_file():
        status = json.loads(receipt.read_text()).get("status")
    else:
        status = "NO-RECEIPT"
        print(result.stdout[-3000:], flush=True)
        print(result.stderr[-3000:], flush=True)
    print(f"verify {case} seed{seed}: status={status}", flush=True)
    return status, output


def _host_identity(image, attempts=5):
    """Image labels, with a bounded retry for a transient Docker CLI failure."""
    last = None
    for attempt in range(attempts):
        result = subprocess.run(
            ["docker", "image", "inspect", image], capture_output=True, text=True
        )
        if result.returncode == 0:
            return json.loads(result.stdout)[0]["Config"]["Labels"]
        last = result.stderr.strip()[-300:]
        time.sleep(2.0 * (attempt + 1))
    raise SystemExit(f"docker image inspect failed for {image}: {last}")


def source_identity(image):
    return _host_identity(image)["dev.layerfs.source-seal"]


def input_identity(family, case, seed, image):
    """The runner's selected-input identity for one case, verbatim."""
    binary = REPO / "target/release/fs-benchmark-pro"
    listed = subprocess.run(
        [str(binary), "infra-list", family, case],
        check=True,
        capture_output=True,
        text=True,
        env={**os.environ, "LAYERFS_V013_IMAGE": image},
    ).stdout
    rows = [
        row
        for row in (json.loads(line) for line in listed.splitlines() if line.startswith("{"))
        if row.get("scenario_id") == case
    ]
    if not rows:
        raise SystemExit(f"select exactly one registered case: {case}")
    return hashlib.sha256(
        json.dumps(
            {
                "family": family,
                "case": case,
                "seed": seed,
                "source": source_identity(image),
                "recipe": rows[0],
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()


def perf_identities(output):
    path = output / "perf.jsonl"
    if not path.is_file():
        return None
    for line in path.open():
        row = json.loads(line)
        if row.get("kind") == "sample":
            return row["identities"]
    return None


def verify(family, case, seed, image, perf_output, fresh=False):
    output = RESULTS / "verification" / case / str(seed) / "candidate" / "run1"
    if output.exists() and not fresh:
        return "RETAINED", output
    if output.exists():
        # A superseded verification receipt is archived, never deleted: the
        # earlier allowance produced a real, retained miss.
        archive = output.with_name("run-" + time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
        output.rename(archive)
        print(f"verify {case}: archived superseded receipt to {archive.name}", flush=True)
    identities = perf_identities(perf_output)
    if identities is None:
        return "NO_PERF", output
    output.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        sys.executable,
        str(BENCH / "verify-selected.py"),
        "--family",
        family,
        "--case",
        case,
        "--seed",
        str(seed),
        "--source",
        identities["source_identity"],
        "--input",
        identities["input_identity"],
        "--image",
        image,
        "--performance",
        str(perf_output / "perf.jsonl"),
        "--output",
        str(output),
    ]
    if case in [row[1] for row in EXTENDED]:
        argv.append("--extended")
    print(f"verify argv: {' '.join(argv[-3:])}", flush=True)
    result, wall = run(argv, env={**os.environ, **({"LAYERFS_V016_ATTR_DIAGNOSTIC": "1"} if os.environ.get("V016_ATTR_DIAGNOSTIC") else {})})
    receipt = output / "verification.json"
    if receipt.is_file():
        status = json.loads(receipt.read_text()).get("status")
    else:
        status = "NO-RECEIPT"
        print(result.stdout[-3000:], flush=True)
        print(result.stderr[-3000:], flush=True)
    print(f"verify {case} seed{seed}: status={status}", flush=True)
    return status, output


def prepare(family, case, image, extended=False):
    argv = [
        sys.executable,
        str(BENCH / "shared" / "prepare_v016_fixture.py"),
        "--family",
        family,
        "--case",
        case,
        "--image",
        image,
    ]
    if case in [row[1] for row in EXTENDED]:
        argv.append("--extended")
    result, wall = run(argv)
    print(f"prepare {case}: rc={result.returncode} wall={wall:.1f}s", flush=True)
    if result.returncode != 0:
        print(result.stdout[-3000:], flush=True)
        print(result.stderr[-3000:], flush=True)
        return False
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", required=True)
    parser.add_argument("--seed", type=int, action="append", default=None)
    parser.add_argument("--case", action="append", default=None)
    parser.add_argument("--mode", choices=("both", "perf", "verify"), default="both")
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--fresh-perf", action="store_true",
                        help="re-collect a performance receipt that is already on disk")
    parser.add_argument("--fresh-verify", action="store_true",
                        help="re-collect a verification receipt that is already on disk")
    args = parser.parse_args()
    seeds = args.seed or [1]
    selected = args.case
    if args.extended:
        rows = [(family, case) for family, case, _ in EXTENDED]
    else:
        rows = [(family_of(case), case["id"]) for case in CASES]
    if selected:
        rows = [row for row in rows if row[1] in selected]
    summary = []
    for family, case in rows:
        for seed in seeds:
            if not registered(family, case, args.image):
                print(f"skip {case}: not registered by the host harness", flush=True)
                summary.append((case, seed, "NOT_RUN"))
                continue
            if args.prepare:
                prepare(family, case, args.image, args.extended)
            extended = case in [row[1] for row in EXTENDED]
            verify_only = extended and case != "v016-workspace-four-100mb-5000-k100-v1"
            if not wait_for_lock():
                print(f"{case}: measurement lock never became available", flush=True)
                return 1
            if args.mode in ("both", "perf") and not verify_only:
                _, perf_output = perf(
                    family, case, seed, args.image, extended, args.fresh_perf
                )
            else:
                perf_output = RESULTS / "performance" / case / str(seed) / "candidate" / "run1"
            if not wait_for_lock():
                print(f"{case}: measurement lock never became available", flush=True)
                return 1
            if args.mode in ("both", "verify"):
                if verify_only:
                    status, _ = verify_only_case(
                        family, case, seed, args.image, args.fresh_verify
                    )
                else:
                    status, _ = verify(
                        family, case, seed, args.image, perf_output, args.fresh_verify
                    )
            else:
                status = "PERF_ONLY"
            summary.append((case, seed, status))
            (RESULTS / "matrix-progress.json").write_text(
                json.dumps(
                    [{"case": c, "seed": s, "verification": v} for c, s, v in summary],
                    indent=1,
                )
            )
    for case, seed, status in summary:
        print(f"{case}\tseed{seed}\t{status}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
