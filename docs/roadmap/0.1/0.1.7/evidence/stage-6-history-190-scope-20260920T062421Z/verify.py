#!/usr/bin/env python3
"""Run the harness's **verification** phase for one arm/case, identity-matched.

The verification invocation is the harness's own: it reads the state roots and the
Store path out of the measured trace (so it verifies the identity the performance
receipt recorded, not one this script chose), resolves a **declared sample** of 64
paths per state through the product's read path, and compares presence, kind, size
and digest against the corpus oracle. It appends to the same trace and writes
`phases-verify.json`; nothing is overwritten. The row it produces is `INCOMPLETE`
and never `PASS`, because it is a sample - stated as sampled, not as a full check.

The verification hard budget is 60 s. A run that cannot finish inside it is recorded
NOT_RUN with its wall rather than given more time.
"""
import argparse
import json
import os
import platform
import subprocess
import sys
import time
from hashlib import sha256
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
HARNESS = "core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content"
ARMS = {
    "baseline": Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual"),
    "candidate": Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope"),
}
CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
SWITCHES = (
    "ADVISORY", "ORDERED_PREDECESSORS", "DECLARED_ONLY", "SIMILARITY_CANDIDATES",
    "FALLBACK_CANDIDATES", "FULL_PRODUCER", "CHUNK_PREDECESSORS", "DEPTH_LIMIT",
)
LIMIT = 60  # the verification hard budget


def sha(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arm", choices=sorted(ARMS))
    parser.add_argument("case", choices=("history-stride10", "history-stride3"))
    args = parser.parse_args()
    repo = ARMS[args.arm]
    binary = repo / HARNESS
    run = CAMPAIGN / "runs" / f"{args.arm}-{args.case}"
    raw = run / "raw"
    if not (raw / "trace.jsonl").exists():
        print(f"REFUSED: {raw} has no measured trace to verify")
        return 2
    if (run / "verify-receipt.json").exists():
        print(f"REFUSED: {run}/verify-receipt.json exists; evidence is append-only")
        return 2
    env = {key: value for key, value in os.environ.items() if not key.startswith("LAYERFS_")}
    env.update({f"LAYERFS_HISTORY_{key}": "" for key in SWITCHES})
    env["LAYERFS_HISTORY_PHASES"] = "1"
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    command = [str(binary), "--case", args.case, "--corpus", str(CORPUS),
               "--out", str(raw), "--phase", "verify"]
    document = {
        "schema": "h190-pooled-scope-verification-v1",
        "arm": args.arm,
        "case": args.case,
        "command": command,
        "cwd": str(repo),
        "limit_ns": LIMIT * 1_000_000_000,
        "binary": str(binary),
        "binary_sha256": sha(binary),
        "platform": platform.platform(),
        "corpus_manifest_sha256": sha(CORPUS / "checkpoint-manifest.json"),
        "store_sha256": sha(raw / "sample.sqlite"),
        "trace_sha256_before": sha(raw / "trace.jsonl"),
        "purpose": ("identity-matched, sampled read-back of the Store the measured "
                    "invocation wrote; the row it produces is INCOMPLETE by construction"),
        "sampled": True,
    }
    start = time.monotonic_ns()
    timed_out = False
    with (run / "verify-stdout.txt").open("x") as stdout, \
            (run / "verify-stderr.txt").open("x") as stderr:
        try:
            result = subprocess.run(command, cwd=repo, env=env, stdout=stdout,
                                    stderr=stderr, timeout=LIMIT)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out, code = True, None
    document.update(wall_ns=time.monotonic_ns() - start, exit_code=code, timed_out=timed_out,
                    trace_sha256_after=sha(raw / "trace.jsonl"))
    (run / "verify-receipt.json").write_text(json.dumps(document, indent=2) + "\n")
    print(f"verify {args.arm}/{args.case}: exit {code}, wall {document['wall_ns'] / 1e9:.3f} s, "
          f"timed_out {timed_out}")
    return 0 if code == 0 and not timed_out else 1


if __name__ == "__main__":
    raise SystemExit(main())
