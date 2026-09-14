"""Read-only source analysis; does not execute a product or benchmark workload."""

import csv
import hashlib
import io
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[6]
OUT = Path(__file__).resolve().parent
RELEASE = "v0.1.5"
CSV_PATH = "release-notes/0.1.5/benchmark-performance.csv"
SELECTED = (
    "workspace-clean-commit-1-compact-v2",
    "workspace-clean-commit-100-mixed-v4",
    "payload-random-read-1-compact-v2",
    "workspace-distributed-sdk-edit-1-compact-v2",
    "workspace-fixed-move-1-compact-v2",
    "tiny-create-1-compact-v2",
    "truncate-tail-4k-on-1mib-ops-1",
    "namespace-10000",
    "namespace-100000",
)


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def write_json(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2) + "\n")


def main():
    raw = git("show", f"{RELEASE}:{CSV_PATH}")
    rows = list(csv.DictReader(io.StringIO(raw.decode())))
    by_id = {row["case"]: row for row in rows}
    assert len(by_id) == len(rows) == 198, "release census changed"
    assert set(SELECTED) <= by_id.keys(), "missing release selection"
    exclusion_path = ROOT / "docs/roadmap/0.1/0.1.6/benchmark-exclusions-issue122.json"
    exclusions = exclusion_path.read_bytes()
    # Exact selected names must not appear as JSON string values in exclusions.
    assert all(json.dumps(case).encode() not in exclusions for case in SELECTED)
    write_json("baseline-selection.json", {
        "kind": "historical evidence extraction, not a new benchmark run",
        "release_commit": git("rev-parse", f"{RELEASE}^{{commit}}").decode().strip(),
        "source_path": CSV_PATH,
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "excluded_manifest_sha256": hashlib.sha256(exclusions).hexdigest(),
        "selection_count": len(SELECTED),
        "rows": [by_id[case] for case in SELECTED],
        "limitations": [
            "Keep candidate_receipt_ns separate from the rounded candidate_ns column.",
            "Public-call sums and edit_commit timers are not interchangeable.",
            "v0.1.3 references have undeclared cache profiles and are not paired evidence.",
            "A release tag is not the producing binary identity; retain row provenance.",
            "No v0.1.6 performance or correctness claim is made.",
        ],
    })
    mib = 1024 ** 2
    gib = 1024 ** 3
    index_memory = 16384 * 80 + 64 * 16 * 1024 + 8192
    fixed_memory = (2 * index_memory + 8192 * 80
                    + (32 + 4 + 256 + 4) * 128 + 256 * 96
                    + 256 * 1024 + 2 * 64 * 1024)
    reserved_disk = 4 * gib + gib + 2 * gib + 64 * 1024 + 256 * mib
    write_json("current-capacity-arithmetic.json", {
        "kind": "source-derived arithmetic, not RSS or physical-allocation measurements",
        "workspace_fixed_reserved_memory_bytes": fixed_memory,
        "workspace_reserved_disk_bytes": reserved_disk,
        "workspace_reserved_fd_slots": 8,
        "eager_index_and_payload_backing_fds": 6,
        "host_reserved_disk_bytes": 128 * gib,
        "maximum_default_workspace_disk_reservations": (128 * gib) // reserved_disk,
        "builder_reserved_memory_bytes": 96 * mib,
        "builder_reserved_fd_slots": 128,
        "singleton_range_page_and_catalog_bytes_per_file": 4096 + 256,
        "singleton_correspondence_pages_and_catalog_bytes_per_file": 3 * (4096 + 256),
        "scope": "Current default component paths; not evidence of default public dispatch.",
    })
    paths = [
        "crates/layerfs-workspace/src/overlay_budget.rs",
        "crates/layerfs-workspace/src/candidate_capacity.rs",
        "crates/layerfs-workspace/src/overlay_index.rs",
        "crates/layerfs-workspace/src/overlay_ranges.rs",
        "crates/layerfs-workspace/src/overlay_payload.rs",
        "crates/layerfs-workspace/src/host_runtime.rs",
        "crates/layerfs-workspace/src/snapshot_candidate.rs",
        "crates/layerfs-workspace/src/correspondence.rs",
        "crates/layerfs-workspace/src/commit_attempt.rs",
        "docs/roadmap/0.1/0.1.6/overlay-snapshot-rule.md",
        "docs/roadmap/0.1/0.1.6/overlay-snapshot-spec.md",
    ]
    write_json("source-manifest.json", {
        "kind": "analysis read identities; not an atomic candidate seal",
        "head": git("rev-parse", "HEAD").decode().strip(),
        "working_tree_status": git("status", "--short").decode().splitlines(),
        "sha256": {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
                   for path in paths},
        "product_benchmark_executed": False,
        "product_test_executed": False,
        "product_source_modified_by_review": False,
    })
    print(f"Extracted {len(SELECTED)} historical rows; no workload executed.")
    print(f"Current reservations: {fixed_memory / mib:.3f} MiB memory, "
          f"{reserved_disk / gib:.6f} GiB disk per Workspace.")


if __name__ == "__main__":
    main()
