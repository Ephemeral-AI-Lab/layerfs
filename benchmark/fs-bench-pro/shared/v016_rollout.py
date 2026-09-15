#!/usr/bin/env python3
"""The #154 rollout driver: one performance and one separate verification
invocation per case, seed and arm, with the complete-command wall measured by
this wrapper and the budget verdict recorded next to it.

One sample per case/arm/mode. A completed invocation is never retried: a miss is
retained as it landed. Superseded receipts are archived beside the live one,
never deleted or overwritten. Performance and verification are separate
invocations that must share one image identity; the verification binds the
performance receipt's own source/input identities.

The declared budgets are the frozen cases.json values: 15 s per regular
invocation, a declared measured exception up to 25 s, and the extensions' own
watchdogs (120 s / 300 s / 60 s).
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[3]
BENCH = REPO / "benchmark" / "fs-bench-pro"
CASES = json.loads((REPO / "docs/roadmap/0.1/0.1.6/cases.json").read_text())["cases"]
LOCK = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"

# The 15-second regular family target, the narrower ≤25 s declared exception band,
# and the owner-granted 60-second complete-command allowance for a regular v0.1.6
# invocation (owner ruling on #154, 2026-09-16). The target is reported separately
# and is never redefined by the allowance.
REGULAR_LIMIT = 15.0
EXCEPTION_LIMIT = 25.0
GRANTED_LIMIT = 60.0
VERIFY_ONLY = {
    "v016-mixed-exhaustive-100mb-5000-k100-v1",
    "v016-mixed-exhaustive-500mb-30000-k100-v1",
}
EXTENDED_LIMITS = {
    "v016-mixed-exhaustive-100mb-5000-k100-v1": 120.0,
    "v016-mixed-exhaustive-500mb-30000-k100-v1": 300.0,
    "v016-workspace-four-100mb-5000-k100-v1": 60.0,
}


def case_row(case_id):
    return next(case for case in CASES if case["id"] == case_id)


def limit_for(case_id):
    return EXTENDED_LIMITS.get(case_id, REGULAR_LIMIT)


def ceiling_for(case_id):
    """The execution allowance of one invocation: the owner-granted 60 s for a
    regular v0.1.6 case, the case's own watchdog for an extended case."""
    if case_id in EXTENDED_LIMITS:
        return EXTENDED_LIMITS[case_id]
    return GRANTED_LIMIT


def gate(wall, case_id, mode="performance"):
    """Target / declared-exception / owner-granted-allowance / failure. The
    granted 60-second allowance covers performance invocations only; the
    verification gate stays at the unchanged 25-second ceiling."""
    limit = limit_for(case_id)
    if wall <= limit:
        return "PASS"
    if case_id in EXTENDED_LIMITS:
        return "FAIL_BUDGET"
    if wall <= EXCEPTION_LIMIT:
        return "EXCEPTION"
    if mode == "performance" and wall <= GRANTED_LIMIT:
        return "GRANTED"
    return "FAIL_BUDGET"


def wait_for_lock(limit_seconds=3600):
    import fcntl

    deadline = time.monotonic() + limit_seconds
    handle = LOCK.open("a")
    while time.monotonic() < deadline:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(handle, fcntl.LOCK_UN)
            handle.close()
            return True
        except BlockingIOError:
            time.sleep(5.0)
    handle.close()
    return False


def host_load():
    """The host's 1/5/15-minute load averages at invocation time. A sample taken
    while the host is oversubscribed is not comparable with a quiet one, so the
    condition is recorded next to the number instead of being inferred later."""
    try:
        one, five, fifteen = os.getloadavg()
        return {"load_1m": round(one, 2), "load_5m": round(five, 2), "load_15m": round(fifteen, 2),
                "cpu_count": os.cpu_count()}
    except OSError:
        return None


def remove_orphan_samples():
    """Remove only this benchmark's own leftover sample containers.

    A wrapper-killed invocation cannot run its own cleanup, and a leftover
    container keeps competing for the declared 2 CPU / 2 GiB budget. The label
    is the benchmark's own owner label, so another owner's run is never touched.
    """
    listed = subprocess.run(
        ["docker", "ps", "-q", "--filter", "label=dev.layerfs.fs-bench.owner=benchmark-infrastructure-v1"],
        capture_output=True, text=True)
    removed = [name for name in listed.stdout.split() if name]
    for container in removed:
        subprocess.run(["docker", "rm", "-f", container], capture_output=True, text=True)
    return removed


def fresh_output(path):
    """Append-only receipts: a live path is archived, never overwritten."""
    if path.exists():
        archive = path.with_name("run-" + time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
        path.rename(archive)
        print(f"archived superseded receipt: {archive}", flush=True)
    path.parent.mkdir(parents=True, exist_ok=True)


def run(argv, timeout, env=None):
    started = time.monotonic()
    try:
        result = subprocess.run(argv, cwd=REPO, capture_output=True, text=True,
                                timeout=timeout, env=env)
        code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired as expired:
        code = -9
        stdout = (expired.stdout or b"").decode("utf-8", "replace") if isinstance(expired.stdout, bytes) else (expired.stdout or "")
        stderr = (expired.stderr or b"").decode("utf-8", "replace") if isinstance(expired.stderr, bytes) else (expired.stderr or "")
    return code, stdout, stderr, time.monotonic() - started


def records(text):
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("{"):
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return rows


def perf_receipt(output):
    path = output / "perf.jsonl"
    if not path.is_file():
        return None
    rows = [json.loads(line) for line in path.open() if line.strip().startswith("{")]
    header = next((row for row in rows if row.get("kind") == "header"), None)
    sample = next((row for row in rows if row.get("kind") == "sample"), None)
    return header, sample, rows



def _host_labels(image):
    result = subprocess.run(["docker", "image", "inspect", image], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"docker image inspect failed for {image}: {result.stderr.strip()[-300:]}")
    return json.loads(result.stdout)[0]["Config"]["Labels"]


def unresolved_identity(family, case_id, seed, image):
    """The runner's own selected-input identity for a case with no performance
    receipt: the image's source seal and the registered recipe row."""
    source = _host_labels(image)["dev.layerfs.source-seal"]
    listed = subprocess.run(
        [str(REPO / "target/release/fs-benchmark-pro"), "infra-list", family, case_id],
        check=True, capture_output=True, text=True,
        env={**os.environ, "LAYERFS_V013_IMAGE": image},
    ).stdout
    rows = [row for row in records(listed) if row.get("scenario_id") == case_id]
    if len(rows) != 1:
        raise SystemExit(f"select exactly one registered case: {case_id}")
    return {
        "source_identity": source,
        "input_identity": hashlib.sha256(json.dumps(
            {"family": family, "case": case_id, "seed": seed, "source": source, "recipe": rows[0]},
            sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
    }

def registered(family, case_id, image):
    result = subprocess.run(
        [str(REPO / "target/release/fs-benchmark-pro"), "infra-list", family, case_id],
        capture_output=True, text=True, env={**os.environ, "LAYERFS_V013_IMAGE": image},
    )
    return any(
        row.get("scenario_id") == case_id for row in records(result.stdout)
    )


def self_check(image):
    result = subprocess.run(
        [str(REPO / "target/release/fs-benchmark-pro"), "workspace-self-check"],
        capture_output=True, text=True, env={**os.environ, "LAYERFS_V013_IMAGE": image},
    )
    for row in records(result.stdout):
        if row.get("kind") == "self-check":
            return row
    return {"status": "FAIL", "stdout": result.stdout[-400:]}


def prepare(family, case_id, image, extended=False):
    argv = [sys.executable, str(BENCH / "shared" / "prepare_v016_fixture.py"),
            "--family", family, "--case", case_id, "--image", image]
    if extended:
        argv.append("--extended")
    code, stdout, stderr, wall = run(argv, 3600)
    print(f"prepare {case_id}: rc={code} wall={wall:.1f}s", flush=True)
    if code != 0:
        print(stdout[-2000:], stderr[-2000:], flush=True)
    return code == 0


def perf_invocation(family, case_id, seed, image, output, extended, wall_limit):
    argv = [sys.executable, str(BENCH / "shared" / "runner.py"),
            "--family", family, "--case", case_id, "--seed", str(seed),
            "--image", image, "--perf-fast", "--setup", "clone",
            "--timeout", str(int(wall_limit) + 5),
            "--product-timeout", str(int(wall_limit)),
            "--setup-timeout", "900",
            "--output", str(output)]
    if extended:
        argv.append("--extended")
    # The wrapper allowance is the declared ceiling plus a bounded exit margin;
    # an invocation killed there is retained as TIMEOUT with its measured wall.
    return run(argv, ceiling_for(case_id) + 20)


def verify_invocation(family, case_id, seed, image, output, perf_identities, extended, wall_limit):
    argv = [sys.executable, str(BENCH / "verify-selected.py"),
            "--family", family, "--case", case_id, "--seed", str(seed),
            "--source", perf_identities["source_identity"],
            "--input", perf_identities["input_identity"],
            "--image", image,
            "--timeout", str(int(wall_limit) + 5),
            "--product-timeout", str(int(wall_limit)),
            "--output", str(output)]
    if perf_identities.get("perf_path"):
        argv += ["--performance", perf_identities["perf_path"]]
    if extended:
        argv.append("--extended")
    return run(argv, ceiling_for(case_id) + 20)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", required=True)
    parser.add_argument("--tag", required=True, help="results subdirectory for this phase")
    parser.add_argument("--family", action="append", default=None)
    parser.add_argument("--case", action="append", default=None)
    parser.add_argument("--seed", type=int, action="append", default=None)
    parser.add_argument("--mode", choices=("both", "perf", "verify"), default="both")
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    seed = (args.seed or [1])[0]
    results = REPO / "benchmark-results" / "v016" / args.tag
    results.mkdir(parents=True, exist_ok=True)
    if args.extended:
        rows = [case["id"] for case in CASES if case["lane"] == "extended"]
    else:
        rows = [case["id"] for case in CASES if case["lane"] == "regular"]
    if args.family:
        rows = [row for row in rows if case_row(row)["family"] in args.family]
    if args.case:
        rows = [row for row in rows if row in args.case]
    if not rows:
        raise SystemExit("no case selected")

    ledger = results / "rollout-matrix.json"
    current = json.loads(ledger.read_text()) if ledger.is_file() else {"image": args.image, "rows": []}
    verdicts = {(row["case"], row["seed"]): row for row in current["rows"]}
    print(f"self-check: {json.dumps(self_check(args.image), sort_keys=True)}", flush=True)
    orphans = remove_orphan_samples()
    if orphans:
        print(f"removed {len(orphans)} leftover sample container(s) before the phase", flush=True)

    for case_id in rows:
        family = case_row(case_id)["family"]
        extended = case_id in EXTENDED_LIMITS
        if not registered(family, case_id, args.image):
            print(f"{case_id}: NOT REGISTERED by the host harness", flush=True)
            verdicts[(case_id, seed)] = {
                "case": case_id, "family": family, "seed": seed,
                "status": "NOT_READY", "reason": "case not registered by the host harness",
            }
            continue
        if args.prepare:
            if not prepare(family, case_id, args.image, extended):
                verdicts[(case_id, seed)] = {
                    "case": case_id, "family": family, "seed": seed,
                    "status": "NOT_READY", "reason": "fixture preparation failed",
                }
                continue
        if args.dry_run:
            print(f"would run {case_id}")
            continue
        row = verdicts.get((case_id, seed)) or {
            "case": case_id, "family": family, "seed": seed,
            "image": args.image, "limit_seconds": limit_for(case_id),
        }
        row.update({"case": case_id, "family": family, "seed": seed,
                    "image": args.image, "limit_seconds": limit_for(case_id)})

        perf_output = results / "performance" / case_id / str(seed) / "candidate" / "run1"
        verify_output = results / "verification" / case_id / str(seed) / "candidate" / "run1"
        identities = None
        if args.mode in ("both", "perf") and case_id not in VERIFY_ONLY:
            # An invocation that never reached the product produced no sample:
            # that is infrastructure-invalid, not a measurement, and the
            # invalid pair is re-run together with the failed attempt retained.
            infrastructure_retries = 0
            for attempt in (1, 2):
                if not wait_for_lock():
                    raise SystemExit("measurement lock never became available")
                load_before = host_load()
                fresh_output(perf_output)
                code, stdout, stderr, wall = perf_invocation(
                    family, case_id, seed, args.image, perf_output, extended, ceiling_for(case_id))
                header, sample, _ = perf_receipt(perf_output) or (None, None, None)
                if sample is not None or attempt == 2:
                    break
                infrastructure_retries += 1
                print(f"perf {case_id}: infrastructure-invalid attempt retained "
                      f"(no sample record); {stdout.strip()[-200:]}{stderr.strip()[-200:]}", flush=True)
            status = (sample or {}).get("status", "NO-RECEIPT")
            print(f"perf {case_id} seed{seed}: status={status} rc={code} wall={wall:.2f}s", flush=True)
            if code != 0 and status == "NO-RECEIPT":
                print(stdout[-1500:], stderr[-1500:], flush=True)
            row["performance"] = {
                "status": status, "returncode": code, "complete_wall_seconds": round(wall, 3),
                "gate": gate(wall, case_id, "performance"),
                "timer_ns": (sample or {}).get("pure_call_sum_ns"),
                "command_wall_ns": (sample or {}).get("command_wall_ns"),
                "preparation_wall_ns": (sample or {}).get("preparation_wall_ns"),
                "cleanup": (sample or {}).get("cleanup"),
                "created_commit_count": next(
                    (rec.get("created_commit_count") for rec in (sample or {}).get("records", [])
                     if rec.get("kind") == "sample-complete"), None),
                "receipt": str(perf_output.relative_to(REPO)),
                "error": (sample or {}).get("error"),
                "infrastructure_invalid_attempts": infrastructure_retries,
                "host_load": load_before,
            }
            if header:
                row["identities"] = {
                    key: header["identities"].get(key) for key in (
                        "source_identity", "input_identity", "image", "setup_identity",
                        "source_arm", "harness_identity", "host_executor")
                }
            if header and sample:
                identities = {
                    "source_identity": header["identities"]["source_identity"],
                    "input_identity": header["identities"]["input_identity"],
                    "perf_path": str(perf_output / "perf.jsonl"),
                }
        if case_id in VERIFY_ONLY and args.mode in ("both", "perf"):
            row["performance"] = {"status": "N/A", "reason": "verify-only declared case; performance is N/A, never zero or PASS"}
        if args.mode in ("both", "verify"):
            if identities is None and case_id in VERIFY_ONLY:
                # A verify-only case has no performance binding: it consumes the
                # declared sealed producer state and is selected by its own
                # identity, never by a fabricated performance row.
                identities = unresolved_identity(family, case_id, seed, args.image)
            if identities is None:
                prior = row.get("identities") or {}
                if prior.get("source_identity") and prior.get("input_identity"):
                    identities = {"source_identity": prior["source_identity"],
                                  "input_identity": prior["input_identity"],
                                  "perf_path": str(perf_output / "perf.jsonl")}
            if identities is None:
                row["verification"] = {"status": "NOT_RUN", "reason": "no matching performance identity"}
            else:
                if not wait_for_lock():
                    raise SystemExit("measurement lock never became available")
                load_before = host_load()
                fresh_output(verify_output)
                code, stdout, stderr, wall = verify_invocation(
                    family, case_id, seed, args.image, verify_output, identities, extended, ceiling_for(case_id))
                receipt_path = verify_output / "verification.json"
                receipt = json.loads(receipt_path.read_text()) if receipt_path.is_file() else {}
                status = receipt.get("status", "NO-RECEIPT")
                print(f"verify {case_id} seed{seed}: status={status} rc={code} wall={wall:.2f}s", flush=True)
                if status == "NO-RECEIPT":
                    print(stdout[-1500:], stderr[-1500:], flush=True)
                row["verification"] = {
                    "status": status, "returncode": code, "complete_wall_seconds": round(wall, 3),
                    "gate": gate(wall, case_id, "verification"),
                    "policy": receipt.get("verification_policy"),
                    "cleanup": receipt.get("cleanup"),
                    "omissions": receipt.get("omissions"),
                    "reused_proof_identities": receipt.get("reused_proof_identities"),
                    "receipt": str(verify_output.relative_to(REPO)),
                    "error": receipt.get("error"),
                    "host_load": load_before,
                }
        verdicts[(case_id, seed)] = row
        current["rows"] = list(verdicts.values())
        ledger.write_text(json.dumps(current, indent=1, sort_keys=True) + "\n")

    print("\ncase\tstatus(perf/verify)\tgate(perf/verify)\twall(perf/verify)")
    for case_id in rows:
        row = verdicts.get((case_id, seed))
        if not row:
            continue
        perf = row.get("performance", {})
        verify = row.get("verification", {})
        print(f"{case_id}\t{perf.get('status')}/{verify.get('status')}\t"
              f"{perf.get('gate')}/{verify.get('gate')}\t"
              f"{perf.get('complete_wall_seconds')}/{verify.get('complete_wall_seconds')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
