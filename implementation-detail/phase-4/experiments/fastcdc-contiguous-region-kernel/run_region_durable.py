#!/usr/bin/env python3
"""Run the conditional one-warmup/four-pair durable FastCDC campaign."""

import csv
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
TARGET_ROOT = REPO / "target/phase4-fastcdc-contiguous-region-kernel-20260821-v2"
RESULTS = TARGET_ROOT / "results-v1"
ROOT = RESULTS / "durable-v1"
SCREEN = RESULTS / "screen-v1"
FIXTURE = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-100.source"
CONTROL = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/operands-v1/phase4_create_edit_benchmark-canonical-v2"
CANDIDATE = TARGET_ROOT / "operands-v1/phase4_create_edit_benchmark-fastcdc-contiguous-region-kernel-v2"
ANALYZER = HERE / "analyze_region_durable.py"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-FASTCDC-REGION-v2.tsv"
LOCK = TARGET_ROOT / "durable-v1.lock"
ORDERS = ["AB", "BA", "AB", "BA"]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_methodology():
    expected = os.environ.get("FASTCDC_REGION_V2_METHODOLOGY_SHA256")
    if not expected or sha256(METHODOLOGY) != expected:
        raise RuntimeError("methodology custody anchor mismatch")
    for row in csv.DictReader(METHODOLOGY.open(), delimiter="\t"):
        path = REPO / row["path"]
        if not path.is_file() or path.stat().st_size != int(row["size_bytes"]) or sha256(path) != row["sha256"]:
            raise RuntimeError(f"methodology mismatch: {row['label']}")


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss:
        raise RuntimeError("/usr/bin/time output lacks CPU or RSS")
    return {"external_real_seconds": float(timing.group(1)), "user_seconds": float(timing.group(2)),
            "system_seconds": float(timing.group(3)), "maximum_resident_set_bytes": int(rss.group(1)),
            "peak_memory_footprint_bytes": int(footprint.group(1)) if footprint else "Unavailable: /usr/bin/time did not report it"}


def schedule():
    result = []
    for position, arm in enumerate("AB", 1):
        result.append({"kind": "warmup", "pair": 0, "order": "AB", "position": position, "arm": arm})
    for pair, order in enumerate(ORDERS, 1):
        for position, arm in enumerate(order, 1):
            result.append({"kind": "measured", "pair": pair, "order": order, "position": position, "arm": arm})
    for sequence, row in enumerate(result, 1):
        row["sequence"] = sequence
        row["label"] = f"{sequence:02d}-{row['kind']}-p{row['pair']}-{row['order']}-pos{row['position']}-{row['arm']}"
    return result


def link_source(root):
    root.mkdir(parents=True)
    os.link(FIXTURE, root / FIXTURE.name)


def database(root, iteration):
    return root / f"db-K64-F64-104857600-full-{iteration}.sqlite"


def copy_image(source, target):
    for suffix in ("", ".authority", ".expectations"):
        shutil.copy2(Path(str(source) + suffix), Path(str(target) + suffix))


def remaining(started_epoch_ns):
    return 120.0 - (time.time_ns() - started_epoch_ns) / 1_000_000_000


def run(command, label, started_epoch_ns, env=None, allow_analysis_failure=False):
    budget = remaining(started_epoch_ns)
    if budget <= 1:
        raise TimeoutError(f"global benchmark budget exhausted before {label}")
    completed = subprocess.run(command, cwd=REPO, env=env, capture_output=True, text=True,
                               timeout=max(0.2, budget - 0.2))
    (ROOT / f"{label}.stdout").write_text(completed.stdout)
    (ROOT / f"{label}.stderr").write_text(completed.stderr)
    if completed.returncode and not (allow_analysis_failure and completed.returncode == 1):
        raise RuntimeError(f"command failed: {label}")
    return completed


def write_manifest():
    manifest = ROOT / "DURABLE-MANIFEST-v1.tsv"
    paths = sorted(path for path in ROOT.rglob("*") if path.is_file() and path != manifest)
    with manifest.open("w") as handle:
        handle.write("path\tsha256\tsize_bytes\n")
        for path in paths:
            handle.write(f"{path.relative_to(REPO)}\t{sha256(path)}\t{path.stat().st_size}\n")
    return manifest, len(paths)


def main():
    screen = json.loads((SCREEN / "SCREEN-ANALYSIS-v1.json").read_text())
    if screen.get("advance_to_durable") is not True:
        raise RuntimeError("screen did not authorize durable acquisition")
    if ROOT.exists() or LOCK.exists():
        raise RuntimeError("durable namespace or lock already exists")
    verify_methodology()
    custody = json.loads((SCREEN / "CUSTODY-v1.json").read_text())
    if sha256(CONTROL) != custody["A_durable_binary_sha256"] or sha256(CANDIDATE) != custody["B_durable_binary_sha256"]:
        raise RuntimeError("durable operand custody mismatch")
    ROOT.mkdir(parents=True)
    LOCK.mkdir()
    started = time.time_ns()
    try:
        masters = {}
        for offset, (arm, executable) in enumerate((("A", CONTROL), ("B", CANDIDATE))):
            master_root = ROOT / f"work-v1/master-{arm}"
            link_source(master_root)
            iteration = 970_000 + offset
            run([executable, "--fast-prepare", master_root, "104857600", "write", str(iteration)],
                f"prepare-master-{arm}", started)
            masters[arm] = database(master_root, iteration)

        rows = schedule()
        with (ROOT / "DURABLE-SCHEDULE-v1.tsv").open("w", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
        raw = ROOT / "DURABLE-RAW-v1.jsonl"
        raw.write_text("")
        for spec in rows:
            executable = CONTROL if spec["arm"] == "A" else CANDIDATE
            row_root = ROOT / f"work-v1/rows/{spec['label']}"
            link_source(row_root)
            iteration = 971_000 + spec["sequence"]
            target = database(row_root, iteration)
            copy_image(masters[spec["arm"]], target)
            env = os.environ.copy()
            env.update({
                "LAYERFS_FAST_LANE": "1",
                "WP4M_EXECUTABLE_SHA256": sha256(executable),
                "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
                "WP4M_BASE_DATABASE_SHA256": sha256(target),
                "WP4M_BASE_AUTHORITY_SHA256": sha256(Path(str(target) + ".authority")),
                "WP4M_BASE_EXPECTATIONS_SHA256": sha256(Path(str(target) + ".expectations")),
            })
            command = ["/usr/bin/time", "-l", executable, "--fast-row", row_root, "104857600",
                       "write", str(iteration), str(spec["kind"] == "warmup").lower(), "capture-only"]
            completed = run(command, f"row-{spec['label']}", started, env)
            row = json.loads(completed.stdout)
            row.update(spec)
            row.update(parse_time(completed.stderr))
            row["binary_sha256"] = sha256(executable)
            row["residue_files"] = [str(path.relative_to(row_root)) for path in row_root.rglob("*")
                                    if path.is_file() and path.name.endswith(("-journal", "-wal", "-shm"))]
            with raw.open("a") as handle:
                handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")

        analyzed = run([sys.executable, ANALYZER, ROOT], "durable-analyzer", started,
                       allow_analysis_failure=True)
        if not analyzed.stdout.strip():
            raise RuntimeError("durable analyzer produced no disposition")
        clock = {"durable_campaign_started_epoch_ns": started,
                 "durable_elapsed_ns_before_manifest": time.time_ns() - started,
                 "ceiling_ns": 120_000_000_000,
                 "within_ceiling_before_manifest": remaining(started) > 0}
        (ROOT / "DURABLE-CLOCK-v1.json").write_text(json.dumps(clock, indent=2, sort_keys=True) + "\n")
        manifest, entries = write_manifest()
        final = {"durable_campaign_elapsed_ns": time.time_ns() - started,
                 "durable_campaign_ceiling_ns": 120_000_000_000,
                 "within_durable_ceiling": remaining(started) > 0,
                 "manifest_entries": entries, "manifest_sha256": sha256(manifest)}
        (ROOT / "DURABLE-FINAL-CLOCK-v1.json").write_text(json.dumps(final, indent=2, sort_keys=True) + "\n")
        print(json.dumps(final, sort_keys=True))
        return 0 if final["within_durable_ceiling"] else 124
    finally:
        if LOCK.exists():
            LOCK.rmdir()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
