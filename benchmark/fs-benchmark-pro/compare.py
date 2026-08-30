#!/usr/bin/env python3
"""Validate and report paired fs-benchmark-pro samples."""

from __future__ import annotations

import hashlib
import json
import re
import statistics
import sys
import tempfile
from pathlib import Path


SCHEMA = "fs-benchmark-pro-sample-v1"
RUN_SCHEMA = "fs-benchmark-pro-run-v1"
CANDIDATES = ("computer-upstream", "layerfs-reference")
COMPUTER_COMMIT = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
COMPUTER_TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
INITIAL_BYTES = 33_554_432
INITIAL_SHA256 = "3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c"
EDITED_SHA256 = "30e8b6c71ab635057c32f0e509e6e0037b5781f94bf1b4c88fb438f41d76ca26"
FINAL_BYTES = 33_554_442
FINAL_SHA256 = "7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3"
OPERATION_IDS = ("create", *(f"edit-{index:02d}" for index in range(1, 17)), "prepend", "read")
GROUPS = (
    ("create", ("create",)),
    ("16 durable edits", OPERATION_IDS[1:17]),
    ("10-byte prepend", ("prepend",)),
    ("full read + sync", ("read",)),
    ("registered workload", OPERATION_IDS),
)
SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")


class InvalidRun(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise InvalidRun(message)


def read_json(path: Path) -> dict:
    require(path.is_file() and not path.is_symlink(), f"missing or unsafe JSON: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise InvalidRun(f"cannot read {path}: {error}") from error
    require(isinstance(value, dict), f"{path}: top level must be an object")
    return value


def nonnegative_timings(value: object, path: str = "sample") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key.endswith("_ns"):
                require(
                    child is None or (type(child) is int and child >= 0),
                    f"{path}.{key}: nonnegative integer or null required",
                )
            elif isinstance(child, (dict, list)):
                nonnegative_timings(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            nonnegative_timings(child, f"{path}[{index}]")


def exact_keys(value: dict, keys: set[str], path: str) -> None:
    missing = sorted(keys - value.keys())
    require(not missing, f"{path}: missing keys: {', '.join(missing)}")


def validate_operation(candidate: str, operation: dict, expected_id: str) -> None:
    exact_keys(operation, {"id", "comparable_ns"}, f"{candidate}/{expected_id}")
    require(operation["id"] == expected_id, f"{candidate}: operation order/identity")
    require(type(operation["comparable_ns"]) is int and operation["comparable_ns"] > 0, f"{candidate}/{expected_id}: positive comparable_ns")
    if candidate == "computer-upstream":
        exact_keys(operation, {"api_ns", "persistence_ns", "to_durable_ns"}, f"{candidate}/{expected_id}")
        require(operation["comparable_ns"] == operation["to_durable_ns"], f"{candidate}/{expected_id}: comparable equation")
        require(
            operation["to_durable_ns"] == operation["api_ns"] + operation["persistence_ns"],
            f"{candidate}/{expected_id}: to_durable phase equation",
        )
    else:
        phases = {
            "workspace_create_ns",
            "shell_ns",
            "workspace_commit_api_ns",
            "push_api_ns",
            "workspace_end_ns",
            "authority_checkpoint_ns",
            "complete_turn_ns",
        }
        exact_keys(operation, phases, f"{candidate}/{expected_id}")
        require(
            operation["authority_checkpoint_ns"]
            == operation["shell_ns"] + operation["workspace_commit_api_ns"] + operation["push_api_ns"],
            f"{candidate}/{expected_id}: authority checkpoint equation",
        )
        require(operation["comparable_ns"] == operation["authority_checkpoint_ns"], f"{candidate}/{expected_id}: comparable equation")
        require(
            operation["complete_turn_ns"]
            == operation["workspace_create_ns"] + operation["authority_checkpoint_ns"] + operation["workspace_end_ns"],
            f"{candidate}/{expected_id}: complete-turn equation",
        )


def validate_sample(sample: dict, candidate: str) -> dict:
    exact_keys(
        sample,
        {"schema", "candidate", "workload", "operations", "aggregates", "storage", "verification", "provenance", "status"},
        candidate,
    )
    require(sample["schema"] == SCHEMA, f"{candidate}: schema")
    require(sample["candidate"] == candidate, f"{candidate}: candidate identity")
    require(str(sample["status"]).upper() == "PASS", f"{candidate}: failed status")
    workload = sample["workload"]
    require(isinstance(workload, dict), f"{candidate}: workload")
    expected_workload = {
        "initial_bytes": INITIAL_BYTES,
        "initial_sha256": INITIAL_SHA256,
        "edit_count": 16,
        "edit_size_bytes": 10,
        "prepend_bytes": 10,
    }
    for key, expected in expected_workload.items():
        require(workload.get(key) == expected, f"{candidate}: workload.{key}")

    operations = sample["operations"]
    require(isinstance(operations, list) and len(operations) == len(OPERATION_IDS), f"{candidate}: operation matrix")
    for operation, expected_id in zip(operations, OPERATION_IDS, strict=True):
        require(isinstance(operation, dict), f"{candidate}/{expected_id}: object required")
        validate_operation(candidate, operation, expected_id)
    by_id = {operation["id"]: operation for operation in operations}

    aggregates = sample["aggregates"]
    require(isinstance(aggregates, dict), f"{candidate}: aggregates")
    expected_aggregates = {
        "create_ns": by_id["create"]["comparable_ns"],
        "sixteen_edits_sum_ns": sum(by_id[f"edit-{index:02d}"]["comparable_ns"] for index in range(1, 17)),
        "prepend_ns": by_id["prepend"]["comparable_ns"],
        "read_ns": by_id["read"]["comparable_ns"],
    }
    for key, expected in expected_aggregates.items():
        require(aggregates.get(key) == expected, f"{candidate}: aggregate equation for {key}")

    verification = sample["verification"]
    require(isinstance(verification, dict), f"{candidate}: verification")
    required_verification = {
        "initial_bytes": INITIAL_BYTES,
        "initial_sha256": INITIAL_SHA256,
        "after_edits_sha256": EDITED_SHA256,
        "final_bytes": FINAL_BYTES,
        "final_sha256": FINAL_SHA256,
        "reopen_passed": True,
    }
    for key, expected in required_verification.items():
        require(verification.get(key) == expected, f"{candidate}: verification.{key}")

    require(isinstance(sample["storage"], dict), f"{candidate}: storage")
    require(isinstance(sample["provenance"], dict), f"{candidate}: provenance")
    nonnegative_timings(sample, candidate)
    return {"raw": sample, "by_id": by_id}


def median(values: list[int]) -> int:
    return int(statistics.median(values))


def group_value(sample: dict, ids: tuple[str, ...], key: str = "comparable_ns") -> int:
    return sum(sample["by_id"][operation_id][key] for operation_id in ids)


def storage_metric(raw: dict, metric: str) -> int | None:
    storage = raw.get("storage", {})
    final = storage.get("authority_checkpoint", storage.get("final", storage.get("after_prepend", {})))
    if not isinstance(final, dict):
        return None
    value = final.get(metric)
    return value if type(value) is int and value >= 0 else None


def format_ns(value: int) -> str:
    return f"{value / 1_000_000:.3f} ms"


def format_bytes(value: int | None) -> str:
    if value is None:
        return "N/A"
    units = ("B", "KiB", "MiB", "GiB")
    scaled = float(value)
    unit = units[0]
    for unit in units:
        if scaled < 1024 or unit == units[-1]:
            break
        scaled /= 1024
    return f"{scaled:.2f} {unit}"


def hash_evidence(run_dir: Path) -> dict[str, str]:
    hashes: dict[str, str] = {}
    inode_hashes: dict[tuple[int, int], str] = {}
    excluded = {"comparison.json", "comparison.md"}
    for path in sorted(run_dir.rglob("*")):
        if path.is_file() and path.name not in excluded:
            require(not path.is_symlink(), f"symlink evidence is not accepted: {path}")
            stat = path.stat()
            identity = (stat.st_dev, stat.st_ino)
            digest = inode_hashes.get(identity)
            if digest is None:
                hasher = hashlib.sha256()
                with path.open("rb") as source:
                    while block := source.read(1024 * 1024):
                        hasher.update(block)
                digest = hasher.hexdigest()
                inode_hashes[identity] = digest
            hashes[str(path.relative_to(run_dir))] = digest
    return hashes


def render_markdown(receipt: dict) -> str:
    lines = [
        f"# `fs-benchmark-pro` — `{receipt['run_id']}`",
        "",
        f"Status: **{receipt['status']}**. Same-host adjacent pairs: **{receipt['pair_count']}**. Lower latency and space are better.",
        "",
        "Exactly two arms are present: pinned upstream Cloudflare Computer and LayerFS Reference. C3, Replica, and multi-agent workloads are excluded.",
        "",
        "## Comparable durable latency",
        "",
        "| Workload | Computer to durable | LayerFS authority checkpoint | LayerFS speedup | LayerFS Workspace Commit |",
        "|---|---:|---:|---:|---:|",
    ]
    for row in receipt["latency"]:
        lines.append(
            f"| {row['workload']} | {format_ns(row['computer_median_ns'])} | "
            f"{format_ns(row['layerfs_median_ns'])} | {row['layerfs_speedup']:.3f}× | "
            f"{format_ns(row['layerfs_commit_median_ns'])} |"
        )
    lines += [
        "",
        "Computer `to durable` is compared only with LayerFS `authority checkpoint` (`shell + Workspace Commit + Push`). Workspace Commit is a diagnostic subset, not a separate candidate. LayerFS Add is excluded from the comparable boundary and, when recorded, remains diagnostic only.",
        "",
        "## Durability",
        "",
        "| Candidate | Final bytes | Final SHA-256 | Reopen |",
        "|---|---:|---|---|",
    ]
    for candidate in CANDIDATES:
        verification = receipt["verification"][candidate]
        lines.append(
            f"| `{candidate}` | {verification['final_bytes']:,} | `{verification['final_sha256']}` | "
            f"{'PASS' if verification['reopen_passed'] else 'FAIL'} |"
        )
    lines += [
        "",
        "## Comparable storage snapshot",
        "",
        "| Metric | Computer upstream | LayerFS Reference | LayerFS reduction |",
        "|---|---:|---:|---:|",
    ]
    for row in receipt["storage"]:
        reduction = "N/A" if row["reduction"] is None else f"{row['reduction'] * 100:.1f}%"
        lines.append(
            f"| {row['label']} | {format_bytes(row['computer'])} | {format_bytes(row['layerfs'])} | {reduction} |"
        )
    lines += [
        "",
        "Computer uses its final durable snapshot; LayerFS uses the final authority-checkpoint snapshot before its separately excluded Add. Physical allocation and semantic payload are different measurements. Space comparisons are valid only at the same retained-history boundary; LayerFS retains immutable history unless the report explicitly proves an equal-retention cleanup policy. `N/A` means the arm did not expose that metric, not zero.",
        "",
        "## Trial order",
        "",
        "| Pair | First | Second | Computer total | LayerFS total |",
        "|---:|---|---|---:|---:|",
    ]
    for pair in receipt["pairs"]:
        lines.append(
            f"| {pair['pair_id']} | `{pair['order'][0]}` | `{pair['order'][1]}` | "
            f"{format_ns(pair['computer_registered_ns'])} | {format_ns(pair['layerfs_registered_ns'])} |"
        )
    lines += [
        "",
        "## Provenance",
        "",
        f"Computer is pinned to commit `{COMPUTER_COMMIT}` and tree `{COMPUTER_TREE}`. Candidate and container-envelope evidence, the randomized schedule, fixture identity, and SHA-256 values for raw evidence are recorded in `comparison.json` and the run directory.",
        "",
    ]
    return "\n".join(lines)


def compare_run(run_dir: Path, write: bool = True) -> dict:
    manifest = read_json(run_dir / "manifest.json")
    require(manifest.get("schema") == RUN_SCHEMA, "run manifest schema")
    run_id = manifest.get("run_id")
    require(isinstance(run_id, str) and SAFE_ID.fullmatch(run_id), "unsafe run ID")
    require(run_dir.name == run_id, "run directory/ID mismatch")
    require(manifest.get("candidates") == list(CANDIDATES), "candidate matrix must be exactly Computer and LayerFS Reference")
    pins = manifest.get("pins", {})
    require(pins.get("computer-upstream", {}).get("commit") == COMPUTER_COMMIT, "Computer commit pin")
    require(pins.get("computer-upstream", {}).get("tree") == COMPUTER_TREE, "Computer tree pin")
    build_mode = pins.get("computer-upstream", {}).get("build_mode")
    if manifest.get("profile") == "formal":
        require(build_mode == "sealed-source-build", "formal run requires sealed Computer source build")
    elif build_mode is not None:
        require(
            build_mode in {"sealed-source-build", "diagnostic-prebuilt-dist"},
            "unsupported Computer build mode",
        )
    fixture = manifest.get("fixture", {})
    require(fixture.get("bytes") == INITIAL_BYTES, "fixture size")
    require(fixture.get("sha256") == INITIAL_SHA256, "fixture digest")

    schedule = manifest.get("schedule")
    require(isinstance(schedule, list) and schedule, "empty schedule")
    profile = manifest.get("profile")
    expected_pairs = {"smoke": 1, "formal": 30, "self-check": 1}.get(profile)
    require(expected_pairs is not None and len(schedule) == expected_pairs, "profile/pair-count mismatch")
    if profile != "self-check":
        terminal = read_json(run_dir / "terminal.json")
        require(
            terminal.get("schema") == "fs-benchmark-pro-terminal-v1" and terminal.get("status") == "complete",
            "run did not reach a complete terminal receipt",
        )
    require(manifest.get("pair_count", len(schedule)) == len(schedule), "manifest pair_count")
    envelope = manifest.get("envelope", {})
    require(
        envelope.get("same_docker_daemon", profile == "self-check") is True
        and envelope.get("adjacent_pairs", profile == "self-check") is True
        and envelope.get("randomized_order", profile == "self-check") is True,
        "same-host randomized adjacent-pair envelope",
    )
    if profile != "self-check":
        require(
            envelope.get("cpus") == 1
            and envelope.get("memory_bytes") == 1_073_741_824
            and envelope.get("memory_swap_bytes") == 1_073_741_824
            and envelope.get("pids_limit") == 512
            and envelope.get("tmpfs") == "/tmp:rw,nosuid,nodev,size=256m"
            and isinstance(envelope.get("architecture"), str)
            and bool(envelope["architecture"])
            and envelope.get("layerfs_control_process_inside_envelope") is True,
            "candidate container envelope",
        )
    pair_samples: list[dict[str, dict]] = []
    pair_rows = []
    for index, entry in enumerate(schedule, 1):
        require(isinstance(entry, dict), f"schedule[{index}]")
        pair_id = entry.get("pair_id")
        require(pair_id == f"{index:03d}", f"schedule[{index}]: pair ID")
        order = entry.get("order")
        require(isinstance(order, list) and sorted(order) == sorted(CANDIDATES), f"schedule[{index}]: adjacent order")
        pair_dir = run_dir / "pairs" / pair_id
        require(pair_dir.is_dir(), f"missing pair directory: {pair_id}")
        unexpected = sorted(path.name for path in pair_dir.iterdir() if path.is_dir() and path.name not in CANDIDATES)
        require(not unexpected, f"{pair_id}: unsupported arms: {', '.join(unexpected)}")
        current: dict[str, dict] = {}
        for candidate in CANDIDATES:
            sample_path = pair_dir / candidate / "summary.json"
            current[candidate] = validate_sample(read_json(sample_path), candidate)
        computer_provenance = current["computer-upstream"]["raw"]["provenance"]
        require(computer_provenance.get("commit") == COMPUTER_COMMIT, f"{pair_id}: Computer summary commit")
        require(computer_provenance.get("tree") == COMPUTER_TREE, f"{pair_id}: Computer summary tree")
        layerfs_provenance = current["layerfs-reference"]["raw"]["provenance"]
        require(
            layerfs_provenance.get("source_commit") == pins.get("layerfs-reference", {}).get("commit"),
            f"{pair_id}: LayerFS summary commit",
        )
        require(
            layerfs_provenance.get("source_dirty") == pins.get("layerfs-reference", {}).get("dirty"),
            f"{pair_id}: LayerFS summary dirty state",
        )
        pair_samples.append(current)
        pair_rows.append(
            {
                "pair_id": pair_id,
                "order": order,
                "computer_registered_ns": group_value(current["computer-upstream"], OPERATION_IDS),
                "layerfs_registered_ns": group_value(current["layerfs-reference"], OPERATION_IDS),
            }
        )

    latency = []
    for label, ids in GROUPS:
        computer_values = [group_value(pair["computer-upstream"], ids) for pair in pair_samples]
        layerfs_values = [group_value(pair["layerfs-reference"], ids) for pair in pair_samples]
        commit_values = [group_value(pair["layerfs-reference"], ids, "workspace_commit_api_ns") for pair in pair_samples]
        computer_median = median(computer_values)
        layerfs_median = median(layerfs_values)
        latency.append(
            {
                "workload": label,
                "computer_median_ns": computer_median,
                "layerfs_median_ns": layerfs_median,
                "layerfs_speedup": computer_median / layerfs_median if layerfs_median else None,
                "layerfs_commit_median_ns": median(commit_values),
                "computer_samples_ns": computer_values,
                "layerfs_samples_ns": layerfs_values,
                "layerfs_commit_samples_ns": commit_values,
            }
        )

    storage_definitions = (
        ("Logical bytes", "logical_bytes"),
        ("SQLite database bytes", "database_bytes"),
        ("SQLite WAL bytes", "wal_bytes"),
        ("SQLite SHM bytes", "shm_bytes"),
        ("Durable allocated bytes", "durable_allocated_bytes"),
        ("Semantic payload bytes", "semantic_payload_bytes"),
        ("Wire bytes", "wire_bytes"),
    )
    storage_rows = []
    for label, key in storage_definitions:
        computer_values = [storage_metric(pair["computer-upstream"]["raw"], key) for pair in pair_samples]
        layerfs_values = [storage_metric(pair["layerfs-reference"]["raw"], key) for pair in pair_samples]
        computer = median([value for value in computer_values if value is not None]) if all(value is not None for value in computer_values) else None
        layerfs = median([value for value in layerfs_values if value is not None]) if all(value is not None for value in layerfs_values) else None
        reduction = None if computer in {None, 0} or layerfs is None else 1 - layerfs / computer
        storage_rows.append({"label": label, "key": key, "computer": computer, "layerfs": layerfs, "reduction": reduction})

    last = pair_samples[-1]
    receipt = {
        "schema": "fs-benchmark-pro-comparison-v1",
        "status": "VALID",
        "run_id": run_id,
        "profile": manifest.get("profile"),
        "pair_count": len(pair_samples),
        "scope": "single-agent durable real-FUSE editing; upstream Computer versus LayerFS Reference",
        "excludes": ["c3", "replica", "multi-agent", "concurrency"],
        "computer_pin": {"commit": COMPUTER_COMMIT, "tree": COMPUTER_TREE},
        "computer_build_mode": build_mode,
        "fixture": fixture,
        "latency": latency,
        "storage": storage_rows,
        "verification": {candidate: last[candidate]["raw"]["verification"] for candidate in CANDIDATES},
        "pairs": pair_rows,
        "evidence_sha256": hash_evidence(run_dir),
    }
    if write:
        outputs = (run_dir / "comparison.json", run_dir / "comparison.md")
        require(not any(path.exists() for path in outputs), "refusing to overwrite comparison output")
        with outputs[0].open("x", encoding="utf-8") as output:
            json.dump(receipt, output, indent=2, sort_keys=True)
            output.write("\n")
        with outputs[1].open("x", encoding="utf-8") as output:
            output.write(render_markdown(receipt))
    return receipt


def synthetic_sample(candidate: str, base: int) -> dict:
    operations = []
    for index, operation_id in enumerate(OPERATION_IDS, 1):
        if candidate == "computer-upstream":
            api = base + index
            persistence = base // 2 + index
            comparable = api + persistence
            operation = {
                "id": operation_id,
                "api_ns": api,
                "persistence_ns": persistence,
                "to_durable_ns": comparable,
                "comparable_ns": comparable,
            }
        else:
            workspace_create = index
            shell = base + index
            commit = base // 2 + index
            push = base // 4 + index
            workspace_end = index + 1
            authority = shell + commit + push
            operation = {
                "id": operation_id,
                "workspace_create_ns": workspace_create,
                "shell_ns": shell,
                "workspace_commit_api_ns": commit,
                "push_api_ns": push,
                "workspace_end_ns": workspace_end,
                "authority_checkpoint_ns": authority,
                "complete_turn_ns": workspace_create + authority + workspace_end,
                "comparable_ns": authority,
            }
        operations.append(operation)
    by_id = {operation["id"]: operation for operation in operations}
    return {
        "schema": SCHEMA,
        "candidate": candidate,
        "workload": {
            "initial_bytes": INITIAL_BYTES,
            "initial_sha256": INITIAL_SHA256,
            "edit_count": 16,
            "edit_size_bytes": 10,
            "prepend_bytes": 10,
        },
        "operations": operations,
        "aggregates": {
            "create_ns": by_id["create"]["comparable_ns"],
            "sixteen_edits_sum_ns": sum(by_id[f"edit-{index:02d}"]["comparable_ns"] for index in range(1, 17)),
            "prepend_ns": by_id["prepend"]["comparable_ns"],
            "read_ns": by_id["read"]["comparable_ns"],
        },
        "storage": {
            "initial": {"logical_bytes": 0, "database_bytes": 4096},
            "final": {
                "logical_bytes": FINAL_BYTES,
                "database_bytes": INITIAL_BYTES + base,
                "wal_bytes": 0,
                "shm_bytes": 0,
                "durable_allocated_bytes": INITIAL_BYTES + base * 2,
                "semantic_payload_bytes": INITIAL_BYTES + base,
                "wire_bytes": None,
            },
        },
        "verification": {
            "initial_bytes": INITIAL_BYTES,
            "initial_sha256": INITIAL_SHA256,
            "after_edits_sha256": EDITED_SHA256,
            "final_bytes": FINAL_BYTES,
            "final_sha256": FINAL_SHA256,
            "reopen_passed": True,
        },
        "provenance": (
            {"status": "synthetic", "commit": COMPUTER_COMMIT, "tree": COMPUTER_TREE}
            if candidate == "computer-upstream"
            else {"status": "synthetic", "source_commit": "a" * 40, "source_dirty": False}
        ),
        "status": "PASS",
    }


def self_check() -> None:
    with tempfile.TemporaryDirectory(prefix="fs-benchmark-pro-check-") as temporary:
        run_dir = Path(temporary) / "synthetic"
        for candidate in CANDIDATES:
            arm = run_dir / "pairs" / "001" / candidate
            arm.mkdir(parents=True)
            (arm / "summary.json").write_text(json.dumps(synthetic_sample(candidate, 1000)), encoding="utf-8")
        manifest = {
            "schema": RUN_SCHEMA,
            "run_id": "synthetic",
            "profile": "self-check",
            "candidates": list(CANDIDATES),
            "fixture": {"bytes": INITIAL_BYTES, "sha256": INITIAL_SHA256},
            "pair_count": 1,
            "envelope": {"same_docker_daemon": True, "adjacent_pairs": True, "randomized_order": True},
            "pins": {
                "computer-upstream": {"commit": COMPUTER_COMMIT, "tree": COMPUTER_TREE},
                "layerfs-reference": {"commit": "a" * 40, "dirty": False},
            },
            "schedule": [{"pair_id": "001", "order": list(CANDIDATES)}],
        }
        (run_dir / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        receipt = compare_run(run_dir)
        assert receipt["status"] == "VALID" and receipt["pair_count"] == 1
        try:
            compare_run(run_dir)
        except InvalidRun as error:
            assert "overwrite" in str(error)
        else:
            raise AssertionError("comparison overwrite was accepted")
        invalid = synthetic_sample("layerfs-reference", 1000)
        invalid["verification"]["reopen_passed"] = False
        try:
            validate_sample(invalid, "layerfs-reference")
        except InvalidRun as error:
            assert "reopen_passed" in str(error)
        else:
            raise AssertionError("failed durability was accepted")
    print("PASS fs-benchmark-pro paired verifier/report self-check")


def main(argv: list[str]) -> int:
    if argv == ["--self-check"]:
        self_check()
        return 0
    if len(argv) != 1 or not SAFE_ID.fullmatch(argv[0]):
        print(f"usage: {Path(sys.argv[0]).name} RUN_ID\n       {Path(sys.argv[0]).name} --self-check", file=sys.stderr)
        return 2
    repo = Path(__file__).resolve().parents[2]
    run_dir = repo / "benchmark-results" / "fs-benchmark-pro" / argv[0]
    try:
        receipt = compare_run(run_dir)
    except InvalidRun as error:
        print(f"fs-benchmark-pro: invalid run: {error}", file=sys.stderr)
        return 1
    print(f"PASS {receipt['pair_count']} paired sample(s): {run_dir / 'comparison.md'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
