#!/usr/bin/env python3
import argparse
import json
import statistics
from pathlib import Path


CASES = (
    ("Cold create 32 MiB", "cold_complete_ns", "create_ns"),
    ("EDIT16", "edit16_ns", "sixteen_edits_sum_ns"),
    ("Prepend temp-copy-rename", "prepend_complete_ns", "prepend_ns"),
    ("Read 32 MiB", "read_complete_ns", "read_ns"),
    ("Registered total", "registered_total_ns", None),
)


def load_layerfs(path: Path):
    records = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("{"):
            records.append(json.loads(line))
    summaries = [r for r in records if r.get("schema") == "fs-bench-pro-v4-summary"]
    if len(summaries) != 1:
        raise ValueError(f"{path}: expected one LayerFS summary")
    summary = summaries[0]
    if summary.get("execution_profile") != "fresh-sh-c":
        raise ValueError(f"{path}: LayerFS did not use the matched shell profile")
    if summary.get("acknowledgement_profile") != "memory-off-live-process":
        raise ValueError(f"{path}: LayerFS acknowledgement profile")
    stores = {
        r["case"]: r for r in records if r.get("schema") == "fs-bench-pro-v4-store"
    }
    required = {"cold-create-32m", "edit16", "prepend-temp-copy-rename", "read-32m"}
    if stores.keys() < required:
        raise ValueError(f"{path}: missing LayerFS Store census")
    return summary, stores


def load_computer(path: Path):
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != "fs-benchmark-pro-computer-v3" or value.get("status") != "PASS":
        raise ValueError(f"{path}: Computer result did not pass")
    for operation in value["operations"]:
        acknowledgement = operation.get("acknowledgement", {})
        if (
            acknowledgement.get("crash_durable") is not False
            or acknowledgement.get("journal_mode") != "memory"
            or acknowledgement.get("synchronous") != 0
            or operation.get("persistence_ns") != 0
        ):
            raise ValueError(f"{path}: unmatched Computer acknowledgement")
    return value


def quantile(values, fraction):
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] * (1 - weight) + ordered[upper] * weight


def milliseconds(value):
    return value / 1_000_000


def mib(value):
    return value / (1024 * 1024)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--minimum-pairs", type=int, default=1)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    pairs = sorted(path for path in args.root.glob("pair-*") if path.is_dir())
    if len(pairs) < args.minimum_pairs:
        raise SystemExit(f"expected at least {args.minimum_pairs} pairs, found {len(pairs)}")

    environment = args.root / "environment"
    started = (environment / "started-utc.txt").read_text(encoding="utf-8").strip()
    layerfs_head = (environment / "layerfs-head.txt").read_text(encoding="utf-8").strip()
    source_seal = (environment / "layerfs-source-seal.sha256").read_text(encoding="utf-8").strip()
    computer_commit = json.loads(
        (pairs[0] / "computer" / "summary.json").read_text(encoding="utf-8")
    )["provenance"]["commit"]
    gates = [
        (pair / "layerfs" / "hard-gate-status.txt").read_text(encoding="utf-8").strip()
        for pair in pairs
        if (pair / "layerfs" / "hard-gate-status.txt").is_file()
    ]

    rows = []
    storage = []
    for pair in pairs:
        layerfs, stores = load_layerfs(pair / "layerfs" / "raw.jsonl")
        computer = load_computer(pair / "computer" / "summary.json")
        values = {}
        for label, layerfs_key, computer_key in CASES:
            layerfs_ns = layerfs[layerfs_key]
            computer_ns = (
                sum(computer["aggregates"].values())
                if computer_key is None
                else computer["aggregates"][computer_key]
            )
            values[label] = (layerfs_ns, computer_ns)
        rows.append(values)

        base = stores["read-32m"]
        edit = stores["edit16"]
        prepend = stores["prepend-temp-copy-rename"]
        computer_storage = computer["storage"]
        storage.append(
            {
                "LayerFS seeded Store": base["allocated_bytes"],
                "Computer seeded Store": computer_storage["read"]["before"][
                    "allocated_bytes"
                ],
                "LayerFS EDIT16 allocation growth": edit["allocated_bytes"]
                - base["allocated_bytes"],
                "Computer EDIT16 allocation growth": computer_storage["edit16"]["after"][
                    "allocated_bytes"
                ]
                - computer_storage["edit16"]["before"]["allocated_bytes"],
                "LayerFS EDIT16 semantic growth": edit["canonical_bytes"]
                - base["canonical_bytes"],
                "Computer EDIT16 semantic growth": computer_storage["edit16"]["after"][
                    "semantic_payload_bytes"
                ]
                - computer_storage["edit16"]["before"]["semantic_payload_bytes"],
                "LayerFS prepend allocation growth": prepend["allocated_bytes"]
                - base["allocated_bytes"],
                "Computer prepend allocation growth": computer_storage["prepend"]["after"][
                    "allocated_bytes"
                ]
                - computer_storage["prepend"]["before"]["allocated_bytes"],
                "LayerFS prepend semantic growth": prepend["canonical_bytes"]
                - base["canonical_bytes"],
                "Computer prepend semantic growth": computer_storage["prepend"]["after"][
                    "semantic_payload_bytes"
                ]
                - computer_storage["prepend"]["before"]["semantic_payload_bytes"],
            }
        )

    lines = [
        "# fs-bench-pro matched paired report",
        "",
        f"Started UTC: `{started}`",
        "",
        f"LayerFS HEAD: `{layerfs_head}`",
        "",
        f"LayerFS source seal: `{source_seal}`",
        "",
        f"Computer product commit: `{computer_commit}`",
        "",
        f"Pairs: {len(rows)}",
        "",
        f"LayerFS standalone hard gates: {gates.count('PASS')}/{len(gates)} passed.",
        "",
        "Acknowledgement: transaction committed and readable from the live local process; "
        "SQLite MEMORY/OFF; no crash or power-loss durability.",
        "",
        "Execution: one fresh `/bin/sh -c` process per command; real FUSE; isolated fresh Store "
        "for every registered row.",
        "Container creation, readiness, and fixture preparation are outside the measured lifecycle; "
        "pair order is deterministically randomized and valid slow samples are retained.",
        "",
        "## Speed",
        "",
        "| Operation | LayerFS median [Q1, Q3] | Computer median [Q1, Q3] | Median paired LayerFS speedup |",
        "| --- | ---: | ---: | ---: |",
    ]
    for label, _, _ in CASES:
        layerfs_values = [row[label][0] for row in rows]
        computer_values = [row[label][1] for row in rows]
        ratios = [computer / layerfs for layerfs, computer in zip(layerfs_values, computer_values)]
        lines.append(
            f"| {label} | {milliseconds(statistics.median(layerfs_values)):.3f} ms "
            f"[{milliseconds(quantile(layerfs_values, .25)):.3f}, "
            f"{milliseconds(quantile(layerfs_values, .75)):.3f}] | "
            f"{milliseconds(statistics.median(computer_values)):.3f} ms "
            f"[{milliseconds(quantile(computer_values, .25)):.3f}, "
            f"{milliseconds(quantile(computer_values, .75)):.3f}] | "
            f"{statistics.median(ratios):.2f}x |"
        )

    lines.extend(
        [
            "",
            "## Storage",
            "",
            "All rows below are medians of physical allocation or semantic content growth from "
            "the same isolated seeded starting state.",
            "",
            "| Metric | LayerFS | Computer | LayerFS reduction |",
            "| --- | ---: | ---: | ---: |",
        ]
    )
    storage_pairs = (
        ("Seeded 32 MiB Store", "LayerFS seeded Store", "Computer seeded Store"),
        (
            "EDIT16 physical allocation growth",
            "LayerFS EDIT16 allocation growth",
            "Computer EDIT16 allocation growth",
        ),
        (
            "EDIT16 semantic content growth",
            "LayerFS EDIT16 semantic growth",
            "Computer EDIT16 semantic growth",
        ),
        (
            "Prepend physical allocation growth",
            "LayerFS prepend allocation growth",
            "Computer prepend allocation growth",
        ),
        (
            "Prepend semantic content growth",
            "LayerFS prepend semantic growth",
            "Computer prepend semantic growth",
        ),
    )
    for label, layerfs_key, computer_key in storage_pairs:
        layerfs_value = statistics.median(item[layerfs_key] for item in storage)
        computer_value = statistics.median(item[computer_key] for item in storage)
        reduction = "n/a" if computer_value == 0 else f"{(1 - layerfs_value / computer_value) * 100:.2f}%"
        lines.append(
            f"| {label} | {mib(layerfs_value):.4f} MiB | {mib(computer_value):.4f} MiB | {reduction} |"
        )

    output = "\n".join(lines) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
