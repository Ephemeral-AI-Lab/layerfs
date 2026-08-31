#!/usr/bin/env python3
import json
import sys
from pathlib import Path

REQUIRED = {
    "workspace_create_ns",
    "execution_ns",
    "commit_api_ns",
    "layerstack_visible_ns",
    "workspace_end_ns",
    "complete_lifecycle_ns",
}


def load(path: Path):
    records = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.startswith("{"):
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise SystemExit(f"{path}:{number}: {error}") from error
    return records


def validate(records):
    samples = [record for record in records if record.get("schema") == "fs-bench-pro-v4"]
    summaries = [
        record for record in records if record.get("schema") == "fs-bench-pro-v4-summary"
    ]
    if not samples or len(summaries) != 1:
        raise SystemExit("expected lifecycle samples and exactly one v4 summary")
    for sample in samples:
        if sample.get("case") == "edit16":
            continue
        missing = REQUIRED - sample.keys()
        if missing:
            raise SystemExit(f"{sample.get('case')}: missing {sorted(missing)}")
        visible = (
            sample["workspace_create_ns"]
            + sample["execution_ns"]
            + sample["commit_api_ns"]
        )
        complete = visible + sample["workspace_end_ns"]
        if visible != sample["layerstack_visible_ns"]:
            raise SystemExit(f"{sample.get('case')}: visibility equation")
        if complete != sample["complete_lifecycle_ns"]:
            raise SystemExit(f"{sample.get('case')}: lifecycle equation")
    return samples, summaries[0]


def report(path: Path):
    samples, summary = validate(load(path))
    print("### One-Store fs-bench-pro campaign")
    print()
    print(f"- Raw evidence: `{path}`")
    print(f"- Lifecycle samples: {len(samples)}")
    print(f"- Workspace Create median: {summary['workspace_create_ns']} ns")
    print(f"- Small-edit Commit median: {summary['small_commit_ns']} ns")
    print(f"- Small-edit complete median: {summary['small_complete_ns']} ns")
    print(f"- Cold-create-32m Commit median: {summary['cold_commit_ns']} ns")
    print(f"- Cold-create-32m complete median: {summary['cold_complete_ns']} ns")
    print(f"- EDIT16 median: {summary['edit16_ns']} ns")
    print(f"- Prepend median: {summary['prepend_complete_ns']} ns")
    print(f"- Read 32 MiB median: {summary['read_complete_ns']} ns")
    print(f"- Registered four-row total: {summary['registered_total_ns']} ns")
    print(
        "- Inner 32 MiB write throughput: "
        f"{summary['inner_write_bytes_per_second']:.3f} bytes/s"
    )


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        validate(
            [
                {
                    "schema": "fs-bench-pro-v4",
                    "case": "small-edit",
                    "workspace_create_ns": 1,
                    "execution_ns": 2,
                    "commit_api_ns": 3,
                    "layerstack_visible_ns": 6,
                    "workspace_end_ns": 4,
                    "complete_lifecycle_ns": 10,
                },
                {"schema": "fs-bench-pro-v4-summary"},
            ]
        )
        print("PASS fs-bench-pro evidence equations")
    elif len(sys.argv) == 2:
        report(Path(sys.argv[1]))
    else:
        raise SystemExit("usage: compare.py --self-check | RAW_JSONL")
