#!/usr/bin/env python3
"""Run the single frozen FastCDC CDC-only AB/BA kill screen."""

import csv
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
TARGET_ROOT = REPO / "target/phase4-fastcdc-exact-hot-loop-20260821-v1"
ROOT = TARGET_ROOT / "results-v1/screen-v1"
FIXTURE = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-100.source"
ANALYZER = HERE / "analyze_fastcdc_screen.py"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-FASTCDC-EXACT-HOT-LOOP-v1.tsv"
CONTROL = TARGET_ROOT / "operands-v1/fastcdc_exact_screen-control"
CANDIDATE = TARGET_ROOT / "operands-v1/fastcdc_exact_screen-candidate"
LOCK = TARGET_ROOT / "screen-v1.lock"
ORDERS = ["AB", "BA", "AB", "BA"]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_methodology():
    expected = os.environ.get("FASTCDC_EXACT_HOT_LOOP_METHODOLOGY_SHA256")
    if not expected or sha256(METHODOLOGY) != expected:
        raise RuntimeError("methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    for row in rows:
        path = REPO / row["path"]
        if not path.is_file() or path.stat().st_size != int(row["size_bytes"]) or sha256(path) != row["sha256"]:
            raise RuntimeError(f"methodology mismatch: {row['label']}")


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss:
        raise RuntimeError("/usr/bin/time output lacks CPU or RSS")
    return {
        "external_real_seconds": float(timing.group(1)),
        "user_seconds": float(timing.group(2)),
        "system_seconds": float(timing.group(3)),
        "maximum_resident_set_bytes": int(rss.group(1)),
        "peak_memory_footprint_bytes": int(footprint.group(1)) if footprint else "Unavailable: /usr/bin/time did not report it",
    }


def schedule():
    rows = []
    for position, arm in enumerate("AB", 1):
        rows.append({"kind": "warmup", "pair": 0, "order": "AB", "position": position, "arm": arm})
    for pair, order in enumerate(ORDERS, 1):
        for position, arm in enumerate(order, 1):
            rows.append({"kind": "measured", "pair": pair, "order": order, "position": position, "arm": arm})
    for sequence, row in enumerate(rows, 1):
        row["sequence"] = sequence
        row["label"] = f"{sequence:02d}-{row['kind']}-p{row['pair']}-{row['order']}-pos{row['position']}-{row['arm']}"
    return rows


def write_manifest():
    manifest = ROOT / "SCREEN-MANIFEST-v1.tsv"
    paths = sorted(path for path in ROOT.rglob("*") if path.is_file() and path != manifest)
    with manifest.open("w") as handle:
        handle.write("path\tsha256\tsize_bytes\n")
        for path in paths:
            handle.write(f"{path.relative_to(REPO)}\t{sha256(path)}\t{path.stat().st_size}\n")
    return manifest, len(paths)


def main():
    if not (TARGET_ROOT / "TASK-CLAIM-v1").is_dir() or LOCK.exists() or (ROOT / "SCREEN-RAW-v1.jsonl").exists():
        raise RuntimeError("screen namespace or lock is not fresh")
    verify_methodology()
    custody = json.loads((ROOT / "CUSTODY-v1.json").read_text())
    if sha256(FIXTURE) != custody["fixture_sha256"] or FIXTURE.stat().st_size != 104_857_600:
        raise RuntimeError("fixture custody mismatch")
    if sha256(CONTROL) != custody["A_screen_binary_sha256"] or sha256(CANDIDATE) != custody["B_screen_binary_sha256"]:
        raise RuntimeError("screen operand custody mismatch")
    LOCK.mkdir()
    boundaries = ROOT / "boundaries-v1"
    boundaries.mkdir()
    raw_path = ROOT / "SCREEN-RAW-v1.jsonl"
    raw_path.write_text("")
    schedule_rows = schedule()
    with (ROOT / "SCREEN-SCHEDULE-v1.tsv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(schedule_rows[0]), delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(schedule_rows)

    started_monotonic = time.monotonic_ns()
    started_epoch = time.time_ns()
    try:
        for spec in schedule_rows:
            if time.monotonic_ns() - started_monotonic >= 18_000_000_000:
                raise TimeoutError("screen deadline reached before next row")
            executable = CONTROL if spec["arm"] == "A" else CANDIDATE
            boundary = boundaries / f"{spec['label']}.tsv"
            command = ["/usr/bin/time", "-l", executable, FIXTURE, boundary]
            completed = subprocess.run(command, cwd=REPO, capture_output=True, text=True, timeout=3)
            (ROOT / f"{spec['label']}.stdout").write_text(completed.stdout)
            (ROOT / f"{spec['label']}.stderr").write_text(completed.stderr)
            if completed.returncode:
                raise RuntimeError(f"row failed: {spec['label']}")
            row = json.loads(completed.stdout)
            row.update(spec)
            row.update(parse_time(completed.stderr))
            row["binary_sha256"] = sha256(executable)
            row["boundary_file"] = str(boundary.relative_to(ROOT))
            row["boundary_file_sha256"] = sha256(boundary)
            with raw_path.open("a") as handle:
                handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")

        (ROOT / "SCREEN-ACQUISITION-CUSTODY-v1.json").write_text(json.dumps({
            "screen_raw_sha256": sha256(raw_path),
            "rows": len(schedule_rows),
            "invocations": len(schedule_rows),
        }, indent=2, sort_keys=True) + "\n")
        analyzed = subprocess.run([sys.executable, ANALYZER, ROOT], cwd=REPO, capture_output=True, text=True, timeout=2)
        (ROOT / "SCREEN-ANALYZER.stdout").write_text(analyzed.stdout)
        (ROOT / "SCREEN-ANALYZER.stderr").write_text(analyzed.stderr)
        if analyzed.returncode not in (0, 1):
            raise RuntimeError("screen analyzer orchestration failed")
        before_manifest_ns = time.monotonic_ns() - started_monotonic
        (ROOT / "SCREEN-CLOCK-v1.json").write_text(json.dumps({
            "acquisition_analysis_disposition_ns": before_manifest_ns,
            "ceiling_ns": 19_000_000_000,
            "within_ceiling_before_manifest": before_manifest_ns < 19_000_000_000,
            "global_benchmark_started_epoch_ns": started_epoch,
        }, indent=2, sort_keys=True) + "\n")
        manifest, entries = write_manifest()
        elapsed_ns = time.monotonic_ns() - started_monotonic
        final = {
            "screen_elapsed_ns": elapsed_ns,
            "screen_ceiling_ns": 19_000_000_000,
            "screen_within_ceiling": elapsed_ns < 19_000_000_000,
            "global_benchmark_started_epoch_ns": started_epoch,
            "global_benchmark_finished_epoch_ns": time.time_ns(),
            "manifest_entries": entries,
            "manifest_sha256": sha256(manifest),
        }
        (ROOT / "SCREEN-FINAL-CLOCK-v1.json").write_text(json.dumps(final, indent=2, sort_keys=True) + "\n")
        print(json.dumps(final, sort_keys=True))
        return 0 if final["screen_within_ceiling"] else 124
    finally:
        if LOCK.exists():
            LOCK.rmdir()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
