#!/usr/bin/env python3
"""Generate the v0.1.2 benchmark report from sealed final-candidate evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
OUTPUT = Path(__file__).with_name("benchmark-results.md")
GITHUB_OUTPUT = Path(__file__).with_name("github-release.md")
EVIDENCE = {
    "conformance": REPO / "benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-c6c14d5a",
    "owner": REPO / "benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-performance-c6c14d5a-r3",
    "same": REPO / "benchmark-results/fs-bench-pro/edit-same-count/final-v012-same-count-c6c14d5a",
    "same_anchor": REPO / "benchmark-results/fs-bench-pro/edit-same-count/final-v012-same-count-c6c14d5a-anchor-custody",
    "count": REPO / "benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-c6c14d5a",
    "count_anchor": REPO / "benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-c6c14d5a-anchor-custody",
    "store": REPO / "benchmark-results/fs-bench-pro/store-footprint/final-v012-store-c6c14d5a",
}
MANIFESTS = {
    "conformance": "deca3578ce3aabbad6ff61c41c5d42297e6d8f02fbd699a4b523194193b2aa4b",
    "owner": "0494d0d9c33ea79e488b3078e18714e86b17995df27e5123c11ecc285861f9e3",
    "same": "07a17444ac938abbe27d3955fd6cb3eeca92f2a87ca10770a61777608e06cc05",
    "same_anchor": "a401fd0092246d380fe626daa55d4e413543bbc2c299241410263416899bad63",
    "count": "491da0d15babd56b38eef00e85f282f318e0f44a847ee5a0a7b289733d979e97",
    "count_anchor": "6c9145ae590d58dced850aa836c273036af07ae39842a214cad1b5eb110d284c",
    "store": "7907b11fa3db15cca13fda6a99a949c3ee0b984cb743270ba182cc0ef586271b",
}
COMMIT = "c6c14d5a5a740665f5efbce439493f681bd7dd95"
TREE = "7c8b843c354fa49f4afa344d66c358a776bfd0d0"
SOURCE = "6b3c039e4237a8ab27eebc5ea4752bc8ad9f58039725ac9b2e3230119b171ec9"
PRODUCT = "438253c10b6b33ae33e6b81113390f0d06d5b98fb2c0fc6c0e0438e0d483431f"
HARNESS = "4c68f918828036082c7110e28bfb2a2e88983d46d404fc1de3899335ad15694c"
WORKLOAD = "c07029d3bf95c187ded2899f3e6840449301a1495c8a51fc694fbbca63fbf6d9"


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()


class Seal:
    def __init__(self, name: str):
        self.root = EVIDENCE[name]
        manifest = self.root / "evidence.sha256"
        assert digest(manifest) == MANIFESTS[name], f"{name} manifest identity"
        self.entries = {}
        for line in manifest.read_text().splitlines():
            expected, relative = line.split(maxsplit=1)
            relative = relative.lstrip("* ").removeprefix("./")
            assert relative not in self.entries
            self.entries[relative] = expected

    def path(self, relative: str) -> Path:
        path = self.root / relative
        assert relative in self.entries, f"unsealed input: {path}"
        assert digest(path) == self.entries[relative], f"changed input: {path}"
        return path

    def text(self, relative: str) -> str:
        return self.path(relative).read_text()

    def json(self, relative: str):
        return json.loads(self.text(relative))

    def jsonl(self, relative: str) -> list[dict]:
        return [json.loads(line) for line in self.text(relative).splitlines() if line]

    def verify_all(self) -> None:
        for relative in self.entries:
            self.path(relative)


def grouped(rows: list[dict], *fields: str) -> dict[tuple, list[dict]]:
    result = defaultdict(list)
    for row in rows:
        result[tuple(row[field] for field in fields)].append(row)
    return result


def stats(rows: list[dict], field: str) -> tuple[float, int, int]:
    values = [row[field] for row in rows]
    return statistics.median(values), min(values), max(values)


def ratio(left: float, right: float) -> float:
    return right / left


def symmetric(left: float, right: float) -> float:
    return max(left / right, right / left)


def ms(value: float) -> str:
    return f"{value / 1_000_000:.3f}"


def seconds(value: float) -> str:
    return f"{value / 1_000_000_000:.3f}"


def mib(value: float) -> str:
    return f"{value / (1024 * 1024):.3f}"


def integer(value: float) -> str:
    return f"{value:,.0f}"


def ms_cell(rows: list[dict], field: str) -> str:
    median, low, high = stats(rows, field)
    return f"{ms(median)} ({ms(low)}–{ms(high)})"


def per_operation_ms_cell(rows: list[dict]) -> str:
    median, low, high = stats(rows, "inner_edit_ns")
    operations = rows[0]["operation_count"]
    assert all(row["operation_count"] == operations for row in rows)
    return f"{ms(median / operations)} ({ms(low / operations)}–{ms(high / operations)})"


def seconds_cell(rows: list[dict], field: str) -> str:
    median, low, high = stats(rows, field)
    return f"{seconds(median)} ({seconds(low)}–{seconds(high)})"


def two_arm_ms(left: list[dict], right: list[dict], field: str) -> str:
    a, a_low, a_high = stats(left, field)
    b, b_low, b_high = stats(right, field)
    return f"{ms(a)} ({ms(a_low)}–{ms(a_high)}) / {ms(b)} ({ms(b_low)}–{ms(b_high)})"


def md_table(headers: list[str], rows: list[list[str]], aligns: list[str] | None = None) -> list[str]:
    aligns = aligns or ["---"] * len(headers)
    return [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(aligns) + " |",
        *("| " + " | ".join(row) + " |" for row in rows),
    ]


def parse_identity(text: str) -> dict[str, str]:
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def main(verify_all: bool) -> tuple[str, str]:
    seals = {name: Seal(name) for name in EVIDENCE}
    if verify_all:
        for seal in seals.values():
            seal.verify_all()

    conformance = seals["conformance"]
    owner = seals["owner"]
    same = seals["same"]
    same_anchor = seals["same_anchor"]
    count = seals["count"]
    count_anchor = seals["count_anchor"]
    store = seals["store"]

    conformance_status = conformance.json("run-status.json")
    conformance_custody = conformance.json("custody.json")
    owner_status = owner.json("run-status.json")
    owner_rows = owner.jsonl("performance/raw.jsonl")
    owner_ledger = owner.jsonl("environment/execution-ledger.jsonl")
    same_status = same.json("run-status.json")
    same_summary = same.json("summary.json")
    same_rows = same.jsonl("performance/raw.jsonl")
    same_verify = same.jsonl("verification/raw.jsonl")
    count_status = count.json("run-status.json")
    count_summary = count.json("summary.json")
    count_rows = count.jsonl("performance/raw.jsonl")
    control_rows = count.jsonl("controls/raw.jsonl")
    scaling_rows = count.jsonl("scaling/raw.jsonl")
    count_verify = count.jsonl("verification/raw.jsonl")
    scaling_verify = count.jsonl("scaling/verification.jsonl")
    scaling_summary = count.json("scaling/summary.json")
    store_status = store.json("run-status.json")
    store_summary = store.json("summary.json")
    store_rows = store.jsonl("performance/raw.jsonl")
    store_verify = store.jsonl("verification/raw.jsonl")
    for anchor, variable, source_manifest in (
        (same_anchor, "LAYERFS_SAME_COUNT_ANCHOR_FIXTURE", MANIFESTS["same"]),
        (count_anchor, "LAYERFS_COUNT_CHANGING_ANCHOR_FIXTURE", MANIFESTS["count"]),
    ):
        custody = anchor.json("custody.json")
        command = anchor.text("environment/command.txt").strip()
        assert custody["status"] == "pass" and not anchor.json("run-status.json")["measurement_rerun"]
        assert custody["anchor_bytes"] == 33_554_432
        assert custody["anchor_sha256"] == "0dea714b8c52c3bbd282eca2e47ca7a1806f35661bc876c21616162312a96c11"
        assert custody["source_evidence_manifest_sha256"] == source_manifest
        assert command == custody["command"] and command.startswith(f"{variable}=/private/tmp/layerfs-edit32-fixture.AIKeS7 ")

    assert conformance_status["status"] == owner_status["status"] == "pass"
    assert same_status == {"schema": "fs-bench-pro-edit-same-count-status-v3", "mode": "admission", "source_identity": "a-a-repeatability", "status": "target-pass", "admission_eligible": True}
    assert count_status["status"] == "tolerated-pass" and count_status["admission_eligible"]
    assert store_status["status"] == "baseline-complete" and not store_status["admission_eligible"]
    for field, expected in (("host_revision", COMMIT), ("source_tree", TREE), ("source_seal", SOURCE), ("product_seal", PRODUCT), ("harness_seal", HARNESS), ("workload_sha256", WORKLOAD)):
        assert conformance_custody[field] == expected
    for seal in (same, count):
        identity = parse_identity(seal.text("environment/prepared-identity.txt"))
        assert (identity["source_commit"], identity["source_tree"], identity["source_seal"], identity["product_seal"], identity["harness_seal"], identity["workload_sha256"]) == (COMMIT, TREE, SOURCE, PRODUCT, HARNESS, WORKLOAD)
    store_identity = store.json("environment/source-seal.json")
    assert (store_identity["source_commit"], store_identity["source_seal"]) == (COMMIT, SOURCE)
    assert store.text("environment/product-seal.txt").strip() == PRODUCT
    assert store.text("environment/harness-seal.txt").strip() == HARNESS
    assert store.text("environment/workload-sha256.txt").strip() == WORKLOAD

    assert len(owner_rows) == len(owner_ledger) == 9
    assert len({(row["case"], row["seed"]) for row in owner_ledger}) == 9
    assert all(row["cleanup_status"] == "pass" and row["swap_bytes"] == 0 and not row["oom"] for row in owner_rows)
    same_groups = grouped(same_rows, "scenario_id", "source_arm")
    assert len(same_rows) == 84 and len({key[0] for key in same_groups}) == 14
    assert all(len(rows) == 3 and {row["seed"] for row in rows} == {1, 2, 3} for rows in same_groups.values())
    assert len(same_verify) == 7 and all(row["status"] == "pass" for row in same_verify)
    count_groups = grouped(count_rows, "scenario_id", "source_arm")
    assert len(count_rows) == 150 and len({key[0] for key in count_groups}) == 25
    assert all(len(rows) == 3 and {row["seed"] for row in rows} == {1, 2, 3} for rows in count_groups.values())
    assert len(control_rows) == 45 and len(scaling_rows) == 18
    assert len(count_verify) == 7 and len(scaling_verify) == 18
    assert all(row["status"] == "pass" for row in count_verify + scaling_verify)
    assert len(store_rows) == 9 and len(store_verify) == 3
    assert all(row["status"] == "pass" and row["cleanup_status"] == "pass" for row in store_rows + store_verify)

    same_arm_a = sum(row["complete_lifecycle_ns"] for row in same_rows if row["source_arm"] == "repeat-a")
    same_arm_b = sum(row["complete_lifecycle_ns"] for row in same_rows if row["source_arm"] == "repeat-b")
    assert same_arm_a == same_summary["arm_complete_lifecycle_ns"]["repeat-a"]
    assert same_arm_b == same_summary["arm_complete_lifecycle_ns"]["repeat-b"]
    assert abs(symmetric(same_arm_a, same_arm_b) - same_summary["comparison_gate_ratio"]) < 1e-12
    recomputed_count_ratios = {}
    for scenario in {key[0] for key in count_groups}:
        baseline = statistics.median(row["complete_lifecycle_ns"] for row in count_groups[(scenario, "baseline")])
        candidate = statistics.median(row["complete_lifecycle_ns"] for row in count_groups[(scenario, "candidate")])
        recomputed_count_ratios[scenario] = ratio(baseline, candidate)
    assert max(recomputed_count_ratios.values()) == count_summary["comparison_gate_ratio"]
    assert count_summary["absolute_status"] == "target-pass"
    assert all(row["status"] == "target-pass" for row in count_summary["absolute_classification"])
    assert scaling_summary["status"] == "target-pass"
    assert all(gate["status"] == "target-pass" and gate["rate_100m_over_rate_10m"] >= 0.9 for gate in scaling_summary["gates"].values())
    assert all(row["cleanup_status"] == "pass" and row["swap_bytes"] == 0 and not row["oom"] and not row["timeout"] for row in count_rows + scaling_rows)
    assert all(row["process_peak_rss_bytes"] <= 128 * 1024 * 1024 and row["container_memory_peak_bytes"] <= 128 * 1024 * 1024 for row in count_rows + scaling_rows)
    assert all(row["root_status"] == row["fresh_reopen_status"] == row["resource_status"] == "pass" and row["committed_root"] == row["reopened_branch_root"] == row["canonical_root"] and row["expected_canonical_file_root"] == row["observed_canonical_file_root"] == row["independent_canonical_file_root"] for row in count_verify + scaling_verify)
    assert all(stats(rows, "operations_per_second")[0] >= 250 for (scenario, arm), rows in count_groups.items() if arm == "candidate" and rows[0]["implementation"] == "direct-posix")
    assert stats(count_groups[("prepend-temp-copy-rename", "candidate")], "complete_lifecycle_ns")[0] <= 223_763_000
    assert all(stats(rows, "inner_edit_ns")[0] <= rows[0]["operation_count"] * 10_000_000 for (scenario, arm), rows in count_groups.items() if arm == "candidate" and rows[0]["fixture_bytes"] == 262_144 and rows[0]["implementation"] == "temp-copy-fsync-rename")
    assert all(row["inner_edit_ns"] < row["operation_count"] * 10_000_000 for row in count_rows if row["source_arm"] == "candidate" and row["fixture_bytes"] == 262_144 and row["implementation"] == "temp-copy-fsync-rename")
    for row in scaling_rows:
        if row["scenario_id"].startswith("delete-"):
            assert row["copied_payload_bytes"] == row["fixture_bytes"] - 2048
        else:
            assert row["copied_payload_bytes"] == row["fixture_bytes"] - 4096
        assert row["read_payload_bytes"] == row["fixture_bytes"]
        assert row["fuse_kernel_write_bytes"] == row["fixture_bytes"] - 2048
    assert store_summary["primary_storage_classification"] == "no-go"
    assert store_summary["primary_total_durable_store_bytes"] > 600_000_000

    lines = [
        "# LayerFS 0.1.2 final-candidate benchmark results",
        "",
        "> **Status:** Final v0.1.2 evidence, measured at code/harness candidate",
        "> `c6c14d5a` and published with the documentation-only release commit.",
        "",
        "**Headline:** Every 256 KiB count-changing temp-copy sample had a batch-average",
        "mutation time below 10 ms/op; full LayerFS lifecycle medians were",
        "approximately 25–343 ms across its 1/10/100-operation cases; 1/10/100 MiB",
        "results demonstrate larger-file scaling behavior.",
        "",
        "The headline is deliberately about mutation latency, not copied-payload MiB/s.",
        "The 256 KiB delete/shrink cases pass their strict latency gate and are not",
        "classified as slow because of a secondary throughput conversion.",
        "",
        "## How to read the tables",
        "",
        "- `N` is the number of raw samples; `N/arm` is the count for each A/B or",
        "  baseline/candidate arm.",
        "- A range is the minimum–maximum of the same raw samples used for the median.",
        "  Every median is computed from the named raw field before display rounding,",
        "  unless the column explicitly identifies a derived value.",
        "- Nanoseconds are divided by `1,000,000` for ms and `1,000,000,000` for s.",
        "  Bytes remain integer bytes; B/s is divided by `1,048,576` for MiB/s.",
        "- A directional ratio is candidate-median / baseline-median. An A/A ratio is",
        "  symmetric (`max(A/B, B/A)`), so it is always at least 1.0.",
        "- Commit/visibility includes the public Commit return and explicit Branch-head",
        "  visibility acknowledgement. Verification is always outside performance timing.",
        "",
        "## Evidence identity",
        "",
    ]
    evidence_rows = [
        ["Universal conformance", EVIDENCE["conformance"].relative_to(REPO).as_posix(), MANIFESTS["conformance"], "pass"],
        ["Owner-side timing supplement", EVIDENCE["owner"].relative_to(REPO).as_posix(), MANIFESTS["owner"], "pass; 9 measurements"],
        ["Same-count", EVIDENCE["same"].relative_to(REPO).as_posix(), MANIFESTS["same"], "target-pass"],
        ["Same-count anchor replay", EVIDENCE["same_anchor"].relative_to(REPO).as_posix(), MANIFESTS["same_anchor"], "custody pass; no measurement rerun"],
        ["Count-changing", EVIDENCE["count"].relative_to(REPO).as_posix(), MANIFESTS["count"], "tolerated-pass"],
        ["Count-changing anchor replay", EVIDENCE["count_anchor"].relative_to(REPO).as_posix(), MANIFESTS["count_anchor"], "custody pass; no measurement rerun"],
        ["Store footprint", EVIDENCE["store"].relative_to(REPO).as_posix(), MANIFESTS["store"], "baseline complete; footprint blocker"],
    ]
    lines += md_table(["Evidence", "Immutable local path", "Manifest SHA-256", "Disposition"], evidence_rows)
    lines += [
        "",
        f"Commit `{COMMIT}`, tree `{TREE}`, source seal `{SOURCE}`, product seal",
        f"`{PRODUCT}`, harness seal `{HARNESS}`, workload SHA-256 `{WORKLOAD}`.",
        "The count-changing frozen baseline has the same commit/tree/product and a",
        "different workload/source seal; the candidate image is labeled clean and the",
        "baseline image is explicitly labeled dirty only because it carries the frozen",
        "workload source.",
        "",
        "The generator checks each manifest's expected SHA-256, rehashes every file used",
        "for these tables, and validates identities, row counts, unique case/arm/seed keys,",
        "statuses, and the headline's strict per-sample condition. `--verify-all` also",
        "rehashes every sealed raw artifact, including the multi-gigabyte Store files.",
        "",
        "## Universal owner-side range edits",
        "",
    ]
    owner_by_case = grouped(owner_rows, "case")
    owner_order = (
        "workspace-range-prepend-head-10b-on-32m",
        "workspace-range-overwrite-middle-4k-on-256k-100",
        "workspace-range-insert-middle-4k-on-256k-100",
    )
    owner_descriptions = {
        owner_order[0]: "Owner prepend, 10 B on 32 MiB",
        owner_order[1]: "100 owner overwrites, 4 KiB on 256 KiB",
        owner_order[2]: "100 owner inserts, 4 KiB on 256 KiB",
    }
    owner_table = []
    for case in owner_order:
        rows = owner_by_case[(case,)]
        owner_table.append([f"`{case}`", owner_descriptions[case], "3", ms_cell(rows, "edit_ns"), ms_cell(rows, "commit_ns"), ms_cell(rows, "edit_commit_ns"), ms_cell(rows, "complete_lifecycle_ns"), integer(stats(rows, "operations_per_second")[0]), "pass"])
    lines += md_table(["Case", "Description", "N", "Edit ms, median (range)", "Commit ms, median (range)", "Edit + Commit ms, median (range)", "Lifecycle ms, median (range)", "Ops/s", "Disposition"], owner_table)
    lines += [
        "",
        "Interpretation: Edit measures the public owner-side range-edit call; Commit is",
        "the public Commit call; Edit + Commit is measured directly. Lifecycle begins",
        "before Workspace creation and ends after clean Workspace end. LayerStack",
        "initialization and Branch fork are excluded. These rows prove the structural",
        "owner path: unchanged payload transfer is zero and conformance is separate.",
        "",
        "## Same-count family (14 IDs)",
        "",
    ]
    same_table = []
    for scenario in sorted({key[0] for key in same_groups}):
        a, b = same_groups[(scenario, "repeat-a")], same_groups[(scenario, "repeat-b")]
        display = a[0]["display_name"]
        lifecycle_a = stats(a, "complete_lifecycle_ns")[0]
        lifecycle_b = stats(b, "complete_lifecycle_ns")[0]
        same_table.append([f"`{scenario}`", display, str(a[0]["operation_count"]), "3", two_arm_ms(a, b, "execution_ns"), two_arm_ms(a, b, "commit_api_ns"), two_arm_ms(a, b, "complete_lifecycle_ns"), f"{integer(stats(a, 'operations_per_second')[0])} / {integer(stats(b, 'operations_per_second')[0])}", f"{mib(max(row['process_peak_rss_bytes'] for row in a))} / {mib(max(row['process_peak_rss_bytes'] for row in b))} MiB", f"{symmetric(lifecycle_a, lifecycle_b):.6f}", "diagnostic only"])
    lines += md_table(["Case", "Description", "Ops", "N/arm", "Execution A median (range) / B median (range) ms", "Commit/visibility A median (range) / B median (range) ms", "Lifecycle A median (range) / B median (range) ms", "Ops/s A/B", "Max RSS A/B", "A/A lifecycle ratio", "Class"], same_table)
    lines += [
        "",
        f"Interpretation: both labels run identical source with one sealed daemon-container identity. The terminal",
        f"gate is the symmetric aggregate arm-wall ratio `{same_summary['comparison_gate_ratio']:.9f}`",
        f"(repeat-a `{seconds(same_arm_a)} s`, repeat-b `{seconds(same_arm_b)} s`, target",
        "`<=1.05`). Per-case A/A ratios are scheduling diagnostics—even values above",
        "`1.10` do not become directional product regressions. Every row still uses a",
        "fresh Store, Branch, Workspace, and workload process; six independent",
        "fragmentation/root/reopen proofs plus one timing/status receipt pass outside",
        "performance timing.",
        "",
        "## Count-changing primary family (25 IDs)",
        "",
    ]
    count_table = []
    for scenario in sorted({key[0] for key in count_groups}):
        candidate = count_groups[(scenario, "candidate")]
        first = candidate[0]
        directional = recomputed_count_ratios[scenario]
        classification = "target" if directional <= 1.05 else "tolerated" if directional <= 1.10 else "no-go"
        throughput = integer(stats(candidate, "operations_per_second")[0]) + " ops/s"
        if first["implementation"] == "temp-copy-fsync-rename":
            rate = stats(candidate, "copied_payload_bytes_per_second")[0] / (1024 * 1024)
            throughput += f"; {rate:.3f} MiB/s copied"
        absolute_gate = "lifecycle ≤223.763 ms" if scenario == "prepend-temp-copy-rename" else "ops/s ≥250" if first["implementation"] == "direct-posix" else "mutation ≤10 ms/op"
        count_table.append([f"`{scenario}`", first["display_name"], first["implementation"], str(first["operation_count"]), "3", per_operation_ms_cell(candidate), ms_cell(candidate, "execution_ns"), ms_cell(candidate, "commit_api_ns"), ms_cell(candidate, "complete_lifecycle_ns"), throughput, absolute_gate, f"{mib(max(row['process_peak_rss_bytes'] for row in candidate))} MiB", f"{directional:.6f}", f"{classification}; absolute target"])
    lines += md_table(["Case", "Description", "Implementation", "Ops", "N/arm", "Mutation/op ms", "Workload ms", "Commit/visibility ms", "Lifecycle ms", "Throughput", "Absolute gate", "Max RSS", "Candidate/baseline", "Class"], count_table)
    lines += [
        "",
        f"Interpretation: the maximum directional ratio is `{count_summary['comparison_gate_ratio']:.9f}`",
        "for `delete-middle-2k-ops-100`: tolerated-pass, below the `1.10` no-go",
        "boundary. Directional target is `<=1.05`; `>1.05` through `1.10` is tolerated",
        "only with phase/counter disposition; `>1.10` is no-go. The under-2 ms",
        "local-step exception can explain a noisy create/end phase but never exempts the",
        "complete lifecycle ratio. The 256 KiB temp-copy mutation gate is strict",
        "`median(inner_edit_ns) <= operation_count * 10,000,000 ns`, with no tolerance band.",
        "Copied MiB/s is secondary. Direct-POSIX append/truncate/sparse rows retain their",
        "operations/s gates. All absolute classifications are target-pass.",
        "",
        "## Count-changing 1/10/100 MiB scaling supplement",
        "",
    ]
    scaling_groups = grouped(scaling_rows, "scenario_id")
    scaling_table = []
    counter_table = []
    for scenario in sorted((key[0] for key in scaling_groups), key=lambda value: (value.startswith("replace-"), scaling_groups[(value,)][0]["fixture_bytes"])):
        rows = scaling_groups[(scenario,)]
        first = rows[0]
        cpu = statistics.median(row["process_user_cpu_ns"] + row["process_system_cpu_ns"] for row in rows)
        size = first["fixture_bytes"] // (1024 * 1024)
        operation = "delete 2 KiB" if scenario.startswith("delete-") else "shrink 4 KiB→2 KiB"
        scale_gate = scaling_summary["gates"]["delete-middle-2k" if scenario.startswith("delete-") else "replace-middle-shrink-4k-to-2k"]
        scaling_table.append([operation, f"{size} MiB", "3", ms_cell(rows, "inner_edit_ns"), ms_cell(rows, "execution_ns"), ms_cell(rows, "commit_api_ns"), ms_cell(rows, "complete_lifecycle_ns"), f"{integer(first['copied_payload_bytes'])} / {integer(first['read_payload_bytes'])} / {integer(first['fuse_kernel_write_bytes'])}", f"{stats(rows, 'copied_payload_bytes_per_second')[0] / (1024 * 1024):.3f}", f"{seconds(cpu)} s", f"{mib(stats(rows, 'process_peak_rss_bytes')[0])} / {mib(stats(rows, 'container_memory_peak_bytes')[0])} / {mib(stats(rows, 'physical_spool_high_water_bytes')[0])} MiB", "0 / false", f"100/10={scale_gate['rate_100m_over_rate_10m']:.6f}; target"])
        counter_table.append([operation, f"{size} MiB", f"{integer(stats(rows, 'commit_cdc_bytes_scanned')[0])} ({integer(stats(rows, 'commit_cdc_bytes_scanned')[1])}–{integer(stats(rows, 'commit_cdc_bytes_scanned')[2])})", f"{integer(stats(rows, 'commit_payload_bytes_read')[0])}", f"{integer(stats(rows, 'candidate_objects')[0])} ({integer(stats(rows, 'candidate_objects')[1])}–{integer(stats(rows, 'candidate_objects')[2])})", f"{integer(stats(rows, 'candidate_bytes')[0])} ({integer(stats(rows, 'candidate_bytes')[1])}–{integer(stats(rows, 'candidate_bytes')[2])})", f"{integer(stats(rows, 'inserted_objects')[0])} / {integer(stats(rows, 'inserted_bytes_total')[0])}", f"{integer(stats(rows, 'reused_objects')[0])} / {integer(stats(rows, 'reused_bytes')[0])}"])
    lines += md_table(["Operation", "Fixture", "N", "Mutation ms", "Workload ms", "Commit/visibility ms", "Lifecycle ms", "Copied / read / written bytes", "Copied MiB/s", "User + system CPU median", "RSS / cgroup peak / spool medians", "Swap / OOM", "Scaling gate"], scaling_table)
    lines += [
        "",
        "Commit-side counters (median and range where a range is shown):",
        "",
    ]
    lines += md_table(["Operation", "Fixture", "CDC bytes", "Old payload read", "Candidate objects", "Candidate bytes", "Inserted objects / bytes", "Reused objects / bytes"], counter_table)
    lines += [
        "",
        f"Interpretation: delete sustains `{scaling_summary['models']['delete-middle-2k']['sustained_copy_bytes_per_second'] / (1024 * 1024):.3f}` MiB/s",
        f"in the diagnostic linear fit and its 100/10 rate ratio is `{scaling_summary['gates']['delete-middle-2k']['rate_100m_over_rate_10m']:.6f}`.",
        f"Shrink sustains `{scaling_summary['models']['replace-middle-shrink-4k-to-2k']['sustained_copy_bytes_per_second'] / (1024 * 1024):.3f}` MiB/s",
        f"and its ratio is `{scaling_summary['gates']['replace-middle-shrink-4k-to-2k']['rate_100m_over_rate_10m']:.6f}`.",
        "Both exceed the `0.90` floor. The 1 MiB rows have no absolute throughput",
        "target. This supplement measures periodic destructive middle-edit suffix",
        "relocation through FUSE/temp-copy; it does not claim near-size-independent",
        "owner-side structural editing, CDC uniqueness, or ObjectId generalization.",
        "`container_memory_peak_bytes` is the daemon cgroup's lifetime high-water while",
        "that container exists, not an independent per-row process peak.",
        "",
        "## Store footprint controls",
        "",
    ]
    store_groups = grouped(store_rows, "control_id")
    store_verifiers = {row["control_id"]: row for row in store_verify}
    store_table = []
    for control in sorted(key[0] for key in store_groups):
        rows = store_groups[(control,)]
        first = rows[0]
        durable, low, high = stats(rows, "total_durable_store_bytes")
        amplification = durable / first["canonical_bytes"]
        verifier = store_verifiers[control]
        disposition = "no-go: >600,000,000 B" if control == "store-footprint-unique-100000" else "explanatory baseline complete"
        store_table.append([f"`{control}`", first["display_name"], "3", seconds_cell(rows, "initialization_ns"), ms_cell(rows, "commit_ns"), ms_cell(rows, "reopen_ns"), seconds_cell(rows, "complete_ns"), integer(first["canonical_bytes"]), f"{integer(durable)} ({integer(low)}–{integer(high)})", f"{amplification:.6f}×", f"{seconds(verifier['verification_ns'])} s", f"{mib(max(row['process_peak_rss_bytes'] for row in rows))} MiB", disposition])
    lines += md_table(["Control", "Description", "N", "Init s", "Commit ms", "Reopen ms", "Lifecycle s", "Canonical bytes", "Durable median (range)", "Amplification", "Verifier phase", "Max perf RSS", "Disposition"], store_table)
    primary = store_summary["primary_total_durable_store_bytes"]
    lines += [
        "",
        f"Interpretation: the primary unique-content Store uses `{primary:,}` bytes,",
        f"`{primary - 600_000_000:,}` above the `600,000,000`-byte goal, so the exact",
        "patch-compatible result remains a recorded blocker rather than a fabricated",
        "pass. Metadata-cardinality and large-object rows are explanatory controls.",
        "Performance lifecycle includes initialization, one edit, Commit, end, reconnect,",
        "and reopen; full tree digest is verifier-only. Store verifier cgroup peaks are",
        "also shared-daemon lifetime high-waters and are not per-sample process RSS.",
        "",
        "## Family walls and resources",
        "",
    ]
    owner_lifecycle = sum(row["complete_lifecycle_ns"] for row in owner_rows)
    same_perf_wall = int(same.text("environment/performance-external-wall-ns.txt"))
    same_verify_wall = int(same.text("environment/verification-external-wall-ns.txt"))
    same_total_wall = int(same.text("environment/total-external-wall-ns.txt"))
    count_perf_wall = int(count.text("environment/performance-external-wall-ns.txt"))
    count_control_wall = int(count.text("environment/control-external-wall-ns.txt"))
    count_verify_wall = int(count.text("environment/verification-external-wall-ns.txt"))
    count_total_wall = int(count.text("environment/total-external-wall-ns.txt"))
    store_perf_wall = int(store.text("environment/performance-external-wall-ns.txt"))
    store_verify_wall = int(store.text("environment/verification-external-wall-ns.txt"))
    all_count_rows = count_rows + scaling_rows
    wall_rows = [
        ["Owner timing supplement", "9", seconds(owner_lifecycle), "not recorded as one wrapper", "—", "separate conformance below", "—", f"{mib(max(row['process_peak_rss_bytes'] for row in owner_rows))} / {mib(max(row['cgroup_peak_bytes'] for row in owner_rows))} / 0 MiB", "pass"],
        ["Same-count", "84", seconds(same_arm_a + same_arm_b), seconds(same_perf_wall), "0", seconds(same_verify_wall), seconds(same_total_wall), f"{mib(max(row['process_peak_rss_bytes'] for row in same_rows))} / {mib(max(row['container_memory_peak_bytes'] for row in same_rows))} / {mib(max(row['spool_allocated_bytes'] for row in same_rows))} MiB", "target-pass"],
        ["Count-changing", "168", seconds(sum(row['complete_lifecycle_ns'] for row in all_count_rows)), seconds(count_perf_wall), seconds(count_control_wall), seconds(count_verify_wall), seconds(count_total_wall), f"{mib(max(row['process_peak_rss_bytes'] for row in all_count_rows))} / {mib(max(row['container_memory_peak_bytes'] for row in all_count_rows))} / {mib(max(row['physical_spool_high_water_bytes'] for row in all_count_rows))} MiB", "tolerated-pass"],
        ["Store", "9", seconds(sum(row['complete_ns'] for row in store_rows)), seconds(store_perf_wall), "—", seconds(store_verify_wall), f"not recorded (component sum {seconds(store_perf_wall + store_verify_wall)})", f"{mib(max(row['process_peak_rss_bytes'] for row in store_rows + store_verify))} / {mib(max(row['container_memory_peak_bytes'] for row in store_rows + store_verify))} / {mib(max(row['temporary_peak_upper_bound_bytes'] for row in store_rows + store_verify))} MiB", "baseline complete; footprint no-go"],
    ]
    lines += md_table(["Family", "Performance N", "Measured lifecycle total s", "Performance external wall s", "Control wall s", "Verification wall s", "Recorded command wall s", "Peak RSS / cgroup / edit spool or Store temporary disk", "Status"], wall_rows)
    lines += [
        "",
        "Interpretation: measured lifecycle totals sum product intervals only. External",
        "walls include daemon startup/shutdown, supervisors, and evidence handling; they",
        "must not be presented as product latency. Count-changing performance wall includes",
        "the 150 primary and 18 scaling rows. Resource columns report maxima, while cgroup",
        "values retain the lifetime-high-water semantics described above.",
        "",
        "## Verification walls",
        "",
    ]
    conformance_groups = [
        "workspace-unit", "file-edit", "reconciliation", "initialization-seed",
        "diagnostic-separation", "scoped-clippy", "live-handle-atomicity",
        "live-root-equality", "live-create-direct-io",
    ]
    conformance_wall = sum(int(conformance.text(f"timing/{name}.external-wall-ns.txt")) for name in conformance_groups)
    same_timing = next(row for row in same_verify if "setup_ns" in row)
    count_setup = sum(row["setup_ns"] for row in count_verify)
    count_phase = sum(row["verification_ns"] for row in count_verify)
    scale_setup = sum(row["setup_ns"] for row in scaling_verify)
    scale_phase = sum(row["verification_ns"] for row in scaling_verify)
    verification_table = [["Universal conformance (9 groups)", "—", "—", "—", seconds(conformance_wall), "group commands; separate from product timing", "pass"]]
    verification_table.append(["Same-count: 6 proofs + 1 timing/status", ms(same_timing["setup_ns"]) + " ms", seconds(same_timing["verification_ns"]), "—", seconds(same_verify_wall), "20 s", "target-pass"])
    verification_table.append(["Count-changing primary (7 receipts)", seconds(count_setup), seconds(count_phase), "—", seconds(int(count.text("environment/primary-verification-external-wall-ns.txt"))), "40 s per verifier", "target-pass"])
    verification_table.append(["Count-changing scaling (18 receipts)", seconds(scale_setup), seconds(scale_phase), "—", seconds(int(count.text("environment/scaling-verification-external-wall-ns.txt"))), "40 s per verifier", "target-pass"])
    for control in ("store-footprint-unique-100000", "store-footprint-metadata-cardinality-100000", "store-footprint-large-object-500m"):
        row = store_verifiers[control]
        external = int(store.text(f"scenarios/{control}/baseline/seed-1-verify/external-wall-ns.txt"))
        disposition = "tolerated-pass" if row["verification_ns"] > 60_000_000_000 else "target-pass"
        verification_table.append([control, seconds(row["initialization_ns"]), seconds(row["verification_ns"]), seconds(row["complete_ns"]), seconds(external), "60 s target / 66 s tolerated phase; 90 s process", disposition])
    lines += md_table(["Verification group", "Setup/init", "Verification phase", "Complete lifecycle", "External wall", "Timeout/classification boundary", "Status"], verification_table)
    lines += [
        "",
        "Interpretation: setup/init, verification work, complete lifecycle, and external",
        "wall are distinct. In particular, Store metadata verification is",
        f"`{seconds(store_verifiers['store-footprint-metadata-cardinality-100000']['verification_ns'])} s`",
        "and therefore tolerated under the 60/66-second phase policy; its longer external",
        "wall is not compared with that phase gate. Count-changing exactness uses fresh",
        "Store/Client reconnect, FUSE reopen, independent byte oracle, observed/expected",
        "digest, and committed/reopened/canonical root equality.",
        "",
        "## Historical and diagnostic evidence (not release-authorizing)",
        "",
    ]
    history = [
        ["`final-v012-count-changing-f6a4d987`", "sealed no-go", "maximum directional ratio 1.106301 exceeded 1.10; no verifier ran", "superseded by exact `c6c14d5a` pass"],
        ["`final-v012-count-changing-a5322303`", "sealed resource failure", "100 MiB cgroup peak exceeded 128 MiB before direct-I/O policy", "retained failure evidence"],
        ["`final-v012-same-count-f6a4d987`", "pass", "older source/harness identity", "superseded by exact `c6c14d5a` run"],
        ["`final-v012-issue14-performance-c6c14d5a-r2`", "measurements pass", "command/nested-custody packaging incomplete", "superseded without rerunning measurements by sealed r3 package"],
        ["all `dev-*` focused runs", "diagnostic only", "dirty-source hypothesis tests", "never admission-eligible"],
    ]
    lines += md_table(["Evidence", "Recorded status", "Why nonterminal", "Disposition"], history)
    lines += [
        "",
        "No failed or diagnostic run is pooled with the final distributions, and no valid",
        "outlier is discarded. Selected performance/verify commands are non-admission",
        "diagnostics; same-count A/A is repeatability evidence, not a product-improvement",
        "claim. Issue #18 remains the deferred physical-pack path for the Store blocker.",
        "",
        "## Reproduce this report",
        "",
        "```bash",
        "python3 release-notes/0.1.2/generate_benchmark_tables.py --check",
        "# Expensive: rehash every sealed file, including all Store databases.",
        "python3 release-notes/0.1.2/generate_benchmark_tables.py --check --verify-all",
        "```",
        "",
        "All displayed milliseconds and seconds are rounded to three decimals; raw JSONL",
        "retains integer nanoseconds. Byte counts and ratios are computed before display",
        "rounding.",
    ]
    github = [
        "# LayerFS 0.1.2 Developer Preview",
        "",
        "> Source-only Developer Preview release.",
        "",
        "LayerFS 0.1.2 adds failure-atomic regular-file range editing through one",
        "shared owner-side/FUSE piece engine while preserving the v0.1.1 storage, CLI,",
        "daemon, projection, and explicit Workspace lifecycle contracts.",
        "",
        "## Benchmark headline",
        "",
        "Every 256 KiB count-changing temp-copy sample had a batch-average mutation",
        "time below 10 ms/op. Full LayerFS lifecycle medians were approximately",
        "25–343 ms across the 1/10/100-operation cases, and the 1/10/100 MiB",
        "supplement demonstrates larger-file suffix-relocation scaling.",
        "",
    ]
    github_owner = []
    for case in owner_order:
        rows = owner_by_case[(case,)]
        github_owner.append([owner_descriptions[case], "3", ms(stats(rows, "edit_ns")[0]), ms(stats(rows, "commit_ns")[0]), ms(stats(rows, "complete_lifecycle_ns")[0])])
    github += md_table(["Owner-side case", "N", "Edit median ms", "Commit median ms", "Lifecycle median ms"], github_owner)
    github += ["", "Family-level disposition:", ""]
    github += md_table(
        ["Evidence", "Samples / receipts", "Headline statistic", "Disposition"],
        [
            ["Universal conformance", "51 native tests + 3 real-FUSE + scoped Clippy", "create-handle direct-I/O coherence and mmap boundary pass", "pass"],
            ["Same-count", "84 / 6 proofs + 1 timing", f"aggregate A/A `{same_summary['comparison_gate_ratio']:.6f}`", "target-pass"],
            ["Count-changing primary", "150 + 45 controls / 7", f"max candidate/baseline `{count_summary['comparison_gate_ratio']:.6f}`", "tolerated-pass (<1.10)"],
            ["Count-changing scaling", "18 / 18", f"100/10 delete `{scaling_summary['gates']['delete-middle-2k']['rate_100m_over_rate_10m']:.6f}`; shrink `{scaling_summary['gates']['replace-middle-shrink-4k-to-2k']['rate_100m_over_rate_10m']:.6f}`", "target-pass (≥0.90)"],
            ["Store unique-100000", "9 family performance / 3 family verifiers", f"`{primary:,}` B vs `600,000,000` B", "exact footprint blocker"],
        ],
    )
    github += [
        "",
        "Copied-payload MiB/s is secondary for 256 KiB temp-copy cases. The scaling",
        "supplement covers periodic destructive middle-edit suffix relocation; it does",
        "not claim CDC uniqueness, ObjectId generalization, or near-size-independent",
        "structural mutation. The exact tables, min–max ranges, timing boundaries,",
        "resources, verifier walls, and manifest hashes are in",
        "[benchmark-results.md](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/release-notes/0.1.2/benchmark-results.md).",
        "",
        "A FUSE file handle returned by `create` uses direct I/O: same-handle and",
        "concurrent-handle I/O is coherent, mmap on that still-open handle returns",
        "`ENODEV`, and mmap works after close/reopen through the retained-cache handle.",
        "",
        "The Store footprint goal is not waived: the primary median is",
        f"`{primary:,}` bytes, `{primary - 600_000_000:,}` bytes above target.",
        "Physical packs remain deferred to open issue #18.",
        "",
        "## Start here",
        "",
        "LayerFS 0.1.2 is source-only. Build from the immutable tag:",
        "",
        "```bash",
        "git clone --branch v0.1.2 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git",
        "cd layerfs",
        "cargo build --release -p layerfs-cli",
        "./target/release/layerfs --version",
        "```",
        "",
        "Prebuilt executables, crates.io packages, and runtime images are not published.",
    ]
    return "\n".join(lines) + "\n", "\n".join(github) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="write benchmark-results.md")
    parser.add_argument("--check", action="store_true", help="fail if benchmark-results.md differs")
    parser.add_argument("--verify-all", action="store_true", help="rehash every sealed artifact")
    args = parser.parse_args()
    assert not (args.write and args.check), "choose --write or --check"
    report, github = main(args.verify_all)
    if args.write:
        OUTPUT.write_text(report)
        GITHUB_OUTPUT.write_text(github)
    elif args.check:
        assert OUTPUT.read_text() == report, f"regenerate {OUTPUT} with --write"
        assert GITHUB_OUTPUT.read_text() == github, f"regenerate {GITHUB_OUTPUT} with --write"
    else:
        print(report, end="")
