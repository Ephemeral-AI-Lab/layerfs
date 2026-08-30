#!/usr/bin/env python3
"""Validate and report one Computer/LayerFS fs-bench pair."""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path


SCENARIOS = (
    "create 1000 files",
    "stat 1000 files",
    "rm 1000 files",
    "mkdir tree (10x10x10)",
    "find tree",
    "write 64 MiB",
    "copy 64 MiB",
    "read 64 MiB",
    "pure read 64 MiB",
    "pure copy 64 MiB",
    "overwrite 64 MiB",
    "git init + commit 100 files",
)
CANDIDATES = ("computer-upstream", "layerfs-reference")
RUNNER_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
COMPUTER_COMMIT = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
COMPUTER_TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
SAFE_ID = re.compile(r"^[A-Za-z0-9._-]+$")


class InvalidPair(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise InvalidPair(message)


def read_manifest(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        require(len(fields) == 2 and fields[0], f"{path}:{number}: malformed TSV")
        require(fields[0] not in values, f"{path}:{number}: duplicate {fields[0]}")
        values[fields[0]] = fields[1]
    return values


def read_arm(pair_dir: Path, candidate: str) -> tuple[dict, dict[str, str]]:
    arm = pair_dir / candidate
    require(arm.is_dir(), f"missing candidate directory: {candidate}")
    manifest = read_manifest(arm / "manifest.tsv")
    require(manifest.get("schema") == "layerfs-fs-bench-v2", f"{candidate}: schema")
    require(manifest.get("pair_id") == pair_dir.name, f"{candidate}: pair_id")
    require(manifest.get("candidate") == candidate, f"{candidate}: identity")
    require(manifest.get("exit_status") == "0", f"{candidate}: nonzero exit")
    require(manifest.get("canonical_runner_sha256") == RUNNER_SHA256, f"{candidate}: runner hash")
    require(manifest.get("provenance_status") in {"verified", "unverified"}, f"{candidate}: provenance")
    if candidate == "computer-upstream":
        require(manifest.get("intended_candidate_commit") == COMPUTER_COMMIT, "Computer commit pin")
        require(manifest.get("intended_candidate_tree") == COMPUTER_TREE, "Computer tree pin")

    raw = json.loads((arm / "result.json").read_text(encoding="utf-8"))
    config = raw.get("config")
    require(isinstance(config, dict), f"{candidate}: missing config")
    require(set(config) == {"reps", "warmup", "randomizeTargets", "mount", "base"}, f"{candidate}: config keys")
    require(config["base"] == "", f"{candidate}: BASE must be empty")
    require(config["mount"] == manifest.get("mount"), f"{candidate}: mount mismatch")
    require(config["reps"] == int(manifest["reps"]), f"{candidate}: reps mismatch")
    require(config["warmup"] == int(manifest["warmup"]), f"{candidate}: warmup mismatch")
    require(config["randomizeTargets"] == int(manifest["randomize_targets"]), f"{candidate}: randomize mismatch")
    require(config["reps"] > 0 and config["warmup"] >= 0, f"{candidate}: invalid sample config")
    require(config["randomizeTargets"] in {0, 1}, f"{candidate}: invalid randomize config")

    rows = raw.get("results")
    require(isinstance(rows, list) and len(rows) == len(SCENARIOS), f"{candidate}: expected 12 rows")
    by_scenario = {}
    for row in rows:
        require(isinstance(row, dict), f"{candidate}: non-object row")
        require(set(row) == {"scenario", "target", "meanNs", "medianNs", "p95Ns", "minNs", "maxNs", "samples"}, f"{candidate}: row keys")
        scenario = row["scenario"]
        require(scenario in SCENARIOS and scenario not in by_scenario, f"{candidate}: scenario matrix")
        require(row["target"] == "computerd", f"{candidate}/{scenario}: target")
        for key in ("meanNs", "medianNs", "p95Ns", "minNs", "maxNs", "samples"):
            require(type(row[key]) is int, f"{candidate}/{scenario}: {key} type")
        require(row["samples"] == config["reps"], f"{candidate}/{scenario}: sample count")
        require(0 < row["minNs"] <= row["medianNs"] <= row["maxNs"], f"{candidate}/{scenario}: median bounds")
        require(row["minNs"] <= row["meanNs"] <= row["maxNs"], f"{candidate}/{scenario}: mean bounds")
        require(row["medianNs"] <= row["p95Ns"] <= row["maxNs"], f"{candidate}/{scenario}: p95 bounds")
        if config["reps"] < 20:
            require(row["p95Ns"] == row["maxNs"], f"{candidate}/{scenario}: nearest-rank p95")
        by_scenario[scenario] = row
    require(tuple(s for s in SCENARIOS if s in by_scenario) == SCENARIOS, f"{candidate}: incomplete matrix")

    stdout = (arm / "stdout.txt").read_text(encoding="utf-8", errors="replace")
    stderr = (arm / "stderr.txt").read_text(encoding="utf-8", errors="replace")
    require("FAIL" not in stdout and "FAIL" not in stderr, f"{candidate}: FAIL marker")
    plain_stdout = re.sub(r"\x1b\[[0-9;]*m", "", stdout)
    require(sum(bool(re.match(r"\s*OK\s", line)) for line in plain_stdout.splitlines()) == 12, f"{candidate}: expected 12 OK rows")
    require("git clone (shallow, ~1MB)" not in plain_stdout, f"{candidate}: network scenario ran")
    return {"config": config, "rows": by_scenario}, manifest


def evidence_hashes(pair_dir: Path) -> dict[str, str]:
    hashes = {}
    for candidate in CANDIDATES:
        arm = pair_dir / candidate
        for path in sorted(arm.iterdir()):
            require(path.is_file() and not path.is_symlink(), f"unexpected evidence entry: {path}")
            hashes[str(path.relative_to(pair_dir))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def render_markdown(receipt: dict) -> str:
    lines = [
        f"# `fs-bench` paired result — `{receipt['pair_id']}`",
        "",
        f"Status: **{receipt['status']}**. Lower median is better.",
        "",
        "This is a resident-FUSE microbenchmark. It excludes setup, Pull, Fork, mount startup, Workspace Commit, Push, Add, reopen, and persistent-space accounting.",
        "",
        "## Provenance",
        "",
        "| Candidate | Intended commit | Intended tree | Provenance | Basis |",
        "|---|---|---|---|---|",
    ]
    for candidate in CANDIDATES:
        provenance = receipt["provenance"][candidate]
        lines.append(
            f"| `{candidate}` | `{provenance['intended_commit']}` | "
            f"`{provenance['intended_tree'] or 'N/A'}` | **{provenance['status']}** | {provenance['basis']} |"
        )
    lines += [
        "",
        "The workload provenance is verified by the frozen upstream script SHA-256. Candidate provenance is never upgraded from caller metadata; only captured container/image evidence can mark it verified.",
        "",
        "## Scenario medians",
        "",
        "| Scenario | Computer upstream | LayerFS Reference | LayerFS speedup | Winner |",
        "|---|---:|---:|---:|---|",
    ]
    for row in receipt["rows"]:
        lines.append(
            f"| {row['scenario']} | {row['computer_median_ns'] / 1_000_000:.3f} ms | "
            f"{row['layerfs_median_ns'] / 1_000_000:.3f} ms | {row['layerfs_speedup']:.3f}× | "
            f"`{row['winner']}` |"
        )
    totals = receipt["aggregates"]
    lines += [
        "",
        "## Summary",
        "",
        f"LayerFS won **{totals['layerfs_wins']}/12** scenarios and Computer won **{totals['computer_wins']}/12**; ties: **{totals['ties']}**.",
        "",
        f"The convenience sum of scenario medians is {totals['computer_sum_ns'] / 1_000_000:.3f} ms for Computer and {totals['layerfs_sum_ns'] / 1_000_000:.3f} ms for LayerFS ({totals['sum_speedup']:.3f}×). It is not an end-to-end agent-turn duration.",
        "",
        "Raw-evidence SHA-256 values are recorded in `comparison.json`.",
        "",
    ]
    return "\n".join(lines)


def compare_pair(results_root: Path, pair_id: str) -> dict:
    require(bool(SAFE_ID.fullmatch(pair_id)), "unsafe pair id")
    pair_dir = (results_root / "fs-bench" / pair_id).resolve()
    expected_root = (results_root / "fs-bench").resolve()
    require(pair_dir.parent == expected_root and pair_dir.is_dir(), "pair must be under benchmark-results/fs-bench")
    unknown = sorted(path.name for path in pair_dir.iterdir() if path.is_dir() and path.name not in CANDIDATES)
    require(not unknown, f"unsupported candidate directories: {', '.join(unknown)}")
    outputs = (pair_dir / "comparison.json", pair_dir / "comparison.md")
    require(not any(path.exists() for path in outputs), "refusing to overwrite comparison output")

    arms = {}
    manifests = {}
    for candidate in CANDIDATES:
        arms[candidate], manifests[candidate] = read_arm(pair_dir, candidate)
    for key in ("reps", "warmup", "randomizeTargets"):
        require(arms[CANDIDATES[0]]["config"][key] == arms[CANDIDATES[1]]["config"][key], f"paired config mismatch: {key}")

    rows = []
    layerfs_wins = computer_wins = ties = 0
    computer_sum = layerfs_sum = 0
    for scenario in SCENARIOS:
        computer = arms["computer-upstream"]["rows"][scenario]["medianNs"]
        layerfs = arms["layerfs-reference"]["rows"][scenario]["medianNs"]
        winner = "layerfs-reference" if layerfs < computer else "computer-upstream" if computer < layerfs else "tie"
        layerfs_wins += winner == "layerfs-reference"
        computer_wins += winner == "computer-upstream"
        ties += winner == "tie"
        computer_sum += computer
        layerfs_sum += layerfs
        rows.append(
            {
                "scenario": scenario,
                "computer_median_ns": computer,
                "layerfs_median_ns": layerfs,
                "layerfs_speedup": computer / layerfs,
                "winner": winner,
            }
        )

    receipt = {
        "schema": "layerfs-fs-bench-comparison-v1",
        "status": "VALID",
        "pair_id": pair_id,
        "scope": "resident real-FUSE filesystem operations only",
        "excludes": ["setup", "pull", "fork", "mount", "workspace-commit", "push", "add", "reopen", "persistent-space"],
        "config": {key: arms["computer-upstream"]["config"][key] for key in ("reps", "warmup", "randomizeTargets")},
        "workload_provenance": {
            "status": "verified",
            "runner_sha256": RUNNER_SHA256,
            "upstream_commit": COMPUTER_COMMIT,
            "upstream_tree": COMPUTER_TREE,
            "upstream_path": "script/fs-bench.sh",
        },
        "provenance": {
            candidate: {
                "status": manifests[candidate]["provenance_status"],
                "basis": manifests[candidate]["provenance_basis"],
                "intended_commit": manifests[candidate]["intended_candidate_commit"],
                "intended_tree": manifests[candidate]["intended_candidate_tree"],
            }
            for candidate in CANDIDATES
        },
        "evidence_sha256": evidence_hashes(pair_dir),
        "rows": rows,
        "aggregates": {
            "computer_sum_ns": computer_sum,
            "layerfs_sum_ns": layerfs_sum,
            "sum_speedup": computer_sum / layerfs_sum,
            "layerfs_wins": layerfs_wins,
            "computer_wins": computer_wins,
            "ties": ties,
        },
    }
    with outputs[0].open("x", encoding="utf-8") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
    with outputs[1].open("x", encoding="utf-8") as output:
        output.write(render_markdown(receipt))
    return receipt


def synthetic_arm(path: Path, pair_id: str, candidate: str, median: int) -> None:
    path.mkdir(parents=True)
    manifest = {
        "schema": "layerfs-fs-bench-v2",
        "pair_id": pair_id,
        "candidate": candidate,
        "intended_candidate_commit": COMPUTER_COMMIT if candidate == "computer-upstream" else "a" * 40,
        "intended_candidate_tree": COMPUTER_TREE if candidate == "computer-upstream" else "",
        "canonical_runner_sha256": RUNNER_SHA256,
        "mount": "/workspace",
        "reps": "3",
        "warmup": "1",
        "randomize_targets": "1",
        "exit_status": "0",
        "provenance_status": "unverified",
        "provenance_basis": "synthetic self-check",
    }
    (path / "manifest.tsv").write_text("".join(f"{key}\t{value}\n" for key, value in manifest.items()), encoding="utf-8")
    rows = [
        {
            "scenario": scenario,
            "target": "computerd",
            "meanNs": median,
            "medianNs": median,
            "p95Ns": median + 1,
            "minNs": median - 1,
            "maxNs": median + 1,
            "samples": 3,
        }
        for scenario in SCENARIOS
    ]
    (path / "result.json").write_text(
        json.dumps({"config": {"reps": 3, "warmup": 1, "randomizeTargets": 1, "mount": "/workspace", "base": ""}, "results": rows}),
        encoding="utf-8",
    )
    (path / "stdout.txt").write_text("".join(f"  OK    {scenario}\n" for scenario in SCENARIOS), encoding="utf-8")
    (path / "stderr.txt").write_text("", encoding="utf-8")


def self_check() -> None:
    with tempfile.TemporaryDirectory(prefix="layerfs-fs-bench-compare-") as temporary:
        root = Path(temporary) / "benchmark-results"
        pair_id = "synthetic"
        pair = root / "fs-bench" / pair_id
        synthetic_arm(pair / "computer-upstream", pair_id, "computer-upstream", 200)
        synthetic_arm(pair / "layerfs-reference", pair_id, "layerfs-reference", 100)
        receipt = compare_pair(root, pair_id)
        assert receipt["aggregates"]["layerfs_wins"] == 12
        assert receipt["aggregates"]["sum_speedup"] == 2
        assert len(receipt["evidence_sha256"]) == 8
        try:
            compare_pair(root, pair_id)
        except InvalidPair as error:
            assert "overwrite" in str(error)
        else:
            raise AssertionError("overwrite was accepted")

        bad_pair = root / "fs-bench" / "bad-matrix"
        synthetic_arm(bad_pair / "computer-upstream", "bad-matrix", "computer-upstream", 200)
        synthetic_arm(bad_pair / "layerfs-reference", "bad-matrix", "layerfs-reference", 100)
        raw_path = bad_pair / "layerfs-reference" / "result.json"
        raw = json.loads(raw_path.read_text(encoding="utf-8"))
        raw["results"].pop()
        raw_path.write_text(json.dumps(raw), encoding="utf-8")
        try:
            compare_pair(root, "bad-matrix")
        except InvalidPair as error:
            assert "12 rows" in str(error)
        else:
            raise AssertionError("incomplete matrix was accepted")

        fail_pair = root / "fs-bench" / "fail-marker"
        synthetic_arm(fail_pair / "computer-upstream", "fail-marker", "computer-upstream", 200)
        synthetic_arm(fail_pair / "layerfs-reference", "fail-marker", "layerfs-reference", 100)
        (fail_pair / "layerfs-reference" / "stderr.txt").write_text("FAIL\n", encoding="utf-8")
        try:
            compare_pair(root, "fail-marker")
        except InvalidPair as error:
            assert "FAIL marker" in str(error)
        else:
            raise AssertionError("FAIL marker was accepted")
    print("PASS fs-bench paired verifier self-check")


def main() -> None:
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        return
    if len(sys.argv) != 2:
        raise SystemExit("usage: compare.py PAIR_ID | --self-check")
    repo = Path(__file__).resolve().parents[2]
    try:
        receipt = compare_pair(repo / "benchmark-results", sys.argv[1])
    except (InvalidPair, FileNotFoundError, json.JSONDecodeError, KeyError, ValueError) as error:
        raise SystemExit(f"fs-bench comparison rejected: {error}") from error
    print(json.dumps({"status": receipt["status"], "pair_id": receipt["pair_id"]}, sort_keys=True))


if __name__ == "__main__":
    main()
