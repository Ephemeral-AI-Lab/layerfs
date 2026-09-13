#!/usr/bin/env python3
"""Build the machine-readable v0.1.6 matrix from the retained receipts.

Reads the exact case plan (`docs/roadmap/0.1/0.1.6/cases.json`), the performance
receipts under `performance/<case>/<seed>/<arm>/<run>/perf.jsonl` and the
separate verification receipts under `verification/<case>/<seed>/<arm>/<run>/`,
and writes one JSON document with a row per requested case/mode/seed. A row is
never invented: a missing receipt is reported as such.
"""

import argparse
import json
from pathlib import Path
import sys

REPO = Path(__file__).resolve().parents[3]
RESULTS = REPO / "benchmark-results" / "v016"
CASES = json.loads((REPO / "docs/roadmap/0.1/0.1.6/cases.json").read_text())["cases"]

EXTENDED = {
    "v016-mixed-exhaustive-100mb-5000-k100-v1": "verify-only",
    "v016-mixed-exhaustive-500mb-30000-k100-v1": "verify-only",
    "v016-workspace-four-100mb-5000-k100-v1": "perf-and-verify",
}


def load_jsonl(path):
    rows = []
    if not path.is_file():
        return rows
    for line in path.open():
        line = line.strip()
        if line:
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                rows.append({"kind": "unparsed", "raw": line[:200]})
    return rows


def perf_row(case, seed):
    root = RESULTS / "performance" / case / str(seed) / "candidate" / "run1"
    rows = load_jsonl(root / "perf.jsonl")
    sample = next((row for row in rows if row.get("kind") == "sample"), None)
    if sample is None:
        return {
            "status": "NOT_RUN",
            "evidence": str(root),
            "reason": "no performance receipt was retained for this case and seed",
        }
    identities = sample.get("identities", {})
    records = sample.get("records", [])
    commits = [row for row in records if row.get("kind") == "v016-commit"]
    # Every route publishes its Commits with its own evidence row: the M1 mixed
    # route emits `v016-commit`, the host-orchestrated boundary and history
    # routes emit `published-root`, and every route emits a `phase` row carrying
    # the full `WorkspaceCommitStatus`. The count below is always an observed
    # count, never the requested one.
    published = [row for row in records if row.get("kind") == "published-root"]
    statuses = [
        row
        for row in records
        if row.get("kind") == "phase" and row.get("phase") == "commit" and row.get("outcome")
    ]
    created_statuses = [row for row in statuses if "result: Created" in row["outcome"]]
    presentation_failed = [
        row
        for row in statuses
        if "presentation_failed: true" in row["outcome"]
    ] + [
        row
        for row in commits
        if str(row.get("presentation_failed")).lower() == "true"
    ]
    observed_commits = len(commits) + len(published)
    commit_ns = sorted(
        int(row["commit_ns"]) for row in commits if isinstance(row.get("commit_ns"), (int, float))
    )
    if not commit_ns:
        commit_ns = sorted(
            int(row["elapsed_ns"])
            for row in statuses
            if isinstance(row.get("elapsed_ns"), (int, float))
        )
    median = commit_ns[len(commit_ns) // 2] if commit_ns else None
    counters = {}
    for row in records:
        if row.get("kind") == "v016-counters":
            counters = row.get("counters", {})
            overlap = row.get("observed_overlap_ns")
            overlap_rounds = row.get("overlap_rounds")
            break
    else:
        overlap = overlap_rounds = None
    return {
        "status": sample.get("status"),
        "phase": sample.get("phase"),
        "complete_command_status": sample.get("complete_command_status"),
        "family_target_status": sample.get("family_target_status"),
        "command_wall_ns": sample.get("command_wall_ns"),
        "preparation_wall_ns": sample.get("preparation_wall_ns"),
        "declared_complete_deadline_seconds": sample.get("declared_complete_deadline_seconds"),
        "pure_call_sum_ns": next(
            (row.get("pure_call_sum_ns") for row in records if row.get("kind") == "v016-timers"),
            None,
        ),
        "created_commits_observed": observed_commits,
        "created_commit_status_rows": len(created_statuses),
        "created_commits_source": "observed evidence rows",
        "presentation_failed": len(presentation_failed) > 0,
        "commit_ns_samples": len(commit_ns),
        "commit_ns_median": median,
        "commit_ns_min": commit_ns[0] if commit_ns else None,
        "commit_ns_max": commit_ns[-1] if commit_ns else None,
        "counters": counters,
        "observed_overlap_ns": overlap,
        "overlap_rounds": overlap_rounds,
        "source_identity": identities.get("source_identity"),
        "input_identity": identities.get("input_identity"),
        "product_identity": identities.get("product_identity"),
        "harness_identity": identities.get("harness_identity"),
        "image_identity": identities.get("image"),
        "error": str(sample.get("error"))[:600] if sample.get("error") else None,
        "evidence": str(root),
    }


def verify_row(case, seed):
    root = RESULTS / "verification" / case / str(seed) / "candidate" / "run1"
    receipt = root / "verification.json"
    if not receipt.is_file():
        return {
            "status": "NOT_RUN",
            "evidence": str(root),
            "reason": "no verification receipt was retained for this case and seed",
        }
    data = json.loads(receipt.read_text())
    mismatches = [
        row
        for row in data.get("checks", [])
        if row.get("kind") == "v016-oracle-mismatch"
    ]
    return {
        "status": data.get("status"),
        "wall_seconds": data.get("wall_seconds"),
        "command_wall_ns": data.get("command_wall_ns"),
        "preparation_wall_ns": data.get("preparation_wall_ns"),
        "cleanup_status": (data.get("cleanup") or {}).get("status"),
        "source_identity": data.get("source_identity"),
        "input_identity": data.get("input_identity"),
        "harness_identity": data.get("harness_identity"),
        "image_identity": data.get("image_identity"),
        "declared_mismatches": sum(int(row.get("mismatch_count", 0)) for row in mismatches),
        "first_mismatch": (mismatches[0].get("mismatches") if mismatches else None),
        "error": str(data.get("error"))[:600] if data.get("error") else None,
        "evidence": str(receipt),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, action="append", default=None)
    parser.add_argument("--out", default=str(RESULTS / "matrix.json"))
    args = parser.parse_args()
    seeds = args.seed or [1]
    rows = []
    for case in CASES:
        for seed in seeds:
            row = {
                "case": case["id"],
                "family": case["family"],
                "lane": case["lane"],
                "fixture": case["fixture"],
                "seed": seed,
                "requested_modes": case["modes"],
                "new_commits_total": case.get("new_commits_total"),
                "retained_graph_roots": case.get("retained_graph_roots"),
                "longest_ancestry_commits": case.get("longest_ancestry_commits"),
                "branch_count": case.get("branch_count"),
                "max_live_workspaces": case.get("max_live_workspaces"),
                "perf": (
                    perf_row(case["id"], seed)
                    if "performance" in case["modes"]
                    else {
                        "status": "N/A",
                        "reason": "verify-only case: performance is explicitly not applicable",
                    }
                ),
                "verify": verify_row(case["id"], seed),
            }
            row["row_status"] = (
                "PASS"
                if row["perf"]["status"] in ("PASS", "N/A")
                and row["verify"]["status"] == "PASS"
                else "FAIL"
            )
            # A row also carries the requested Commit count so a reader can see
            # whether the observed publication matches the request.
            row["requested_commits"] = case.get("new_commits_total")
            observed = row["perf"].get("created_commits_observed")
            row["publication_matches_request"] = (
                None
                if observed is None or case.get("new_commits_total") is None
                else observed == case.get("new_commits_total")
            )
            rows.append(row)
    document = {
        "schema": "layerfs-v016-matrix-v1",
        "issue": 122,
        "regular_case_count": len(CASES),
        "extended_case_count": len(EXTENDED),
        "seeds": seeds,
        "extended_cases": [
            {
                "case": case,
                "mode": mode,
                "perf": (
                    perf_row(case, seed) if mode == "perf-and-verify" else {"status": "N/A"}
                ),
                "verify": verify_row(case, seed),
            }
            for case, mode in EXTENDED.items()
            for seed in seeds
        ],
        "rows": rows,
    }
    Path(args.out).write_text(json.dumps(document, indent=1, sort_keys=True))
    passed = sum(1 for row in rows if row["row_status"] == "PASS")
    print(f"{passed}/{len(rows)} regular rows PASS -> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
