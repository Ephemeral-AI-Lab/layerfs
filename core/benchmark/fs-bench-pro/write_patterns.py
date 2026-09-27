#!/usr/bin/env python3
"""Three append-only public mounted-write diagnostics over one 10 MiB master."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from shell_package import (BASE, CORE, ROOT, case_spec, digest, identities,
                           json_file, lft1, one_receipt, run_command, sha)
from separated_writes import archive_binary, backing_samples, progress

HERE = Path(__file__).resolve().parent
SPEC = ROOT / "docs/roadmap/0.1/0.1.7/issue261-three-pattern-100-spec.md"
TREATMENT = ROOT / "docs/roadmap/0.1/0.1.7/issue265-mounted-write-treatment.md"
WRITER = HERE / "writers/write-separated.c"
PATTERNS = ("append", "dispersed", "repeated")
SIZE = 10 << 20
COUNT = 100
OLD = {"data.bin": b"A" * SIZE}


def expected(pattern):
    content = bytearray(OLD["data.bin"])
    for i in range(COUNT):
        byte = ord("B") + i % 24
        if pattern == "append":
            content.append(byte)
        elif pattern == "dispersed":
            content[(104729 + i * 2654435761) % SIZE] = byte
        elif pattern == "repeated":
            content[5 << 20] = byte
        else:
            raise ValueError(pattern)
    return {"data.bin": bytes(content)}


EXTENT_SUMS = ("leaf_visits", "branch_visits", "leaf_visit_records",
               "branch_visit_children", "leaf_writes", "branch_writes",
               "leaf_write_records", "branch_write_children",
               "child_edges_added", "custody_edges_added")


def diagnostic(line, marker):
    if not line.startswith(marker + " "):
        return None
    return {key: int(value) if value.isdigit() else value for key, value in
            (item.split("=", 1) for item in line[len(marker) + 1:].split())}


def extent_diagnostics(stderr):
    totals = {key: 0 for key in EXTENT_SUMS}
    snapshots = []
    splices = 0
    root_height = 0
    edge = None
    leaf_min = None
    leaf_max = 0
    lines = stderr.decode(errors="replace").splitlines()
    for line in lines:
        if (row := diagnostic(line, "LFS_EXTENT_SPLICE")) is not None:
            if row["v"] != 1:
                raise ValueError("unknown extent splice counter version")
            splices += 1
            for key in EXTENT_SUMS:
                totals[key] += row[key]
            root_height = row["root_height"]
            if row["leaf_writes"]:
                leaf_min = row["leaf_min"] if leaf_min is None else min(leaf_min, row["leaf_min"])
                leaf_max = max(leaf_max, row["leaf_max"])
        elif (row := diagnostic(line, "LFS_EXTENT_EDGE")) is not None:
            if row["v"] != 1:
                raise ValueError("unknown extent edge counter version")
            edge = row
        elif (match := re.search(r"LFS_WRITE_SAMPLE v=2 write_class=Some\((\d+)\)", line)):
            snapshots.append({"writes": int(match[1]), "splices": splices,
                              "root_height": root_height, "totals": totals.copy(),
                              "leaf_min": leaf_min, "leaf_max": leaf_max,
                              "edge": edge.copy() if edge else None})
    return {"splices": splices, "snapshots": snapshots, "totals": totals,
            "root_height": root_height, "leaf_min": leaf_min, "leaf_max": leaf_max,
            "c1_edit": [diagnostic(line, "LFS_C1_EDIT_LOAD") for line in lines
                        if line.startswith("LFS_C1_EDIT_LOAD ")],
            "file_input": [diagnostic(line, "LFS_FILE_INPUT") for line in lines
                           if line.startswith("LFS_FILE_INPUT ")]}


def report(receipt_files, output):
    """Interpret retained raw diagnostics without changing an attempt receipt."""
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for receipt_file in receipt_files:
        receipt = json.loads(receipt_file.read_text())
        stderr = receipt_file.parent / "driver.stderr"
        expected = next((line.split()[0] for line in
            (receipt_file.parent / "SHA256SUMS").read_text().splitlines()
            if line.endswith("  driver.stderr")), None)
        if (receipt["schema"] != "issue265-patterns-attempt-v1" or
                expected is None or digest(stderr) != expected):
            raise ValueError(f"raw receipt or stderr seal mismatch: {receipt_file}")
        counts = extent_diagnostics(stderr.read_bytes())
        complete = (counts["splices"] == COUNT and
                    [row["writes"] for row in counts["snapshots"]] == [25, 50, 75, 100]
                    and [row["splices"] for row in counts["snapshots"]] == [25, 50, 75, 100]
                    and all(row["edge"] is not None for row in counts["snapshots"])
                    and len(counts["c1_edit"]) == 1 and len(counts["file_input"]) == 1)
        rows.append({"selection": receipt["selection"], "receipt": str(receipt_file.resolve()),
                     "receipt_sha256": digest(receipt_file), "stderr_sha256": expected,
                     "original_row_status": receipt["row_status"],
                     "functional_status": receipt["functional_status"],
                     "verifier_status": receipt["verifier"]["status"],
                     "source": receipt["source"], "diagnostic_status":
                     "COMPLETE" if complete else "INCOMPLETE", "counts": counts})
    if [row["selection"] for row in rows] != list(PATTERNS):
        raise ValueError("report requires one append/dispersed/repeated cohort in order")
    json_file(output / "report.json", {"schema": "issue265-patterns-postprocess-v1",
        "report_generator_sha256": digest(Path(__file__)), "source_commit":
        subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "admission_eligible": False, "rows": rows,
        "note": "Original INCOMPLETE receipts remain unchanged; this is count-only postprocessing."})
    (output / "SHA256SUMS").write_text(f"{digest(output / 'report.json')}  report.json\n")
    print(json.dumps({"report": str(output / "report.json"),
                      "diagnostic_status": [row["diagnostic_status"] for row in rows]}))


def fields(master, pattern, command):
    case = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                        "genesis_root_serial", "branch_id", "old_commit")}
    case.update(scenario_id=f"issue265-{pattern}-100-10m-v1",
                command_hex=command.encode().hex(), expected_failure="0",
                write_pattern=pattern, pattern_count=str(COUNT),
                telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    return case


def prepare_reuse(output, prior_file):
    """Rebuild changed product binaries around the same closed, verified master."""
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = identities()
    prior = json.loads(prior_file.read_text())
    if source["source_dirty"] or prior["schema"] != "issue261-patterns-prepared-v1":
        raise ValueError("committed source and sealed pattern master required")
    changed = subprocess.check_output(["git", "diff", "--name-only",
        prior["source"]["source_commit"], source["source_commit"], "--",
        "core/crates"], cwd=ROOT, text=True).splitlines()
    allowed = {
        "core/crates/layerfs-content/src/file/edit/apply.rs",
        "core/crates/layerfs-content/src/file/edit/tree.rs",
        "core/crates/layerfs-content/tests/edit_localized.rs",
        "core/crates/layerfs-server/src/service/save/file_stream.rs",
        *[f"core/crates/layerfs-workspace/src/backing/{path}" for path in (
            "binary_plus_tree/extent/mod.rs", "binary_plus_tree/extent/splice.rs",
            "binary_plus_tree/extent/telemetry.rs", "metadata.rs", "metadata_reclaim.rs",
            "payload.rs", "reader.rs", "segments.rs")],
        *[f"core/crates/layerfs-workspace/tests/{name}.rs" for name in (
            "backing_ownership", "maintenance", "payload", "symlink")],
    }
    if (prior["source"]["build_profile"] != "release" or
            not changed or not set(changed) <= allowed
            or prior["spec_sha256"] != digest(SPEC)
            or prior["writer_source_sha256"] != digest(WRITER)
            or prior["fixture_sha256"] != sha(OLD["data.bin"])
            or prior["cargo_config_sha256"] != digest(ROOT / ".cargo/config.toml")):
        raise ValueError(f"unreviewed source or workload change: {changed}")
    if subprocess.run(["git", "diff", "--quiet", prior["source"]["source_commit"],
                       source["source_commit"], "--", "core/Cargo.lock",
                       ".cargo/config.toml"], cwd=ROOT).returncode:
        raise ValueError("locked compilation inputs changed")
    master = prior["master"]
    for name in ("store", "history"):
        if digest(Path(master["path"]) / f"{name}.sqlite") != master[f"{name}_sha256"]:
            raise ValueError(f"closed {name} master changed")
    if digest(Path(master["path"]) / "verify.stdout") != master["proof_sha256"]:
        raise ValueError("old master proof changed")
    local_master = output / "master"
    local_master.mkdir()
    for name in ("store.sqlite", "history.sqlite", "verify.case", "verify.stdout"):
        shutil.copyfile(Path(master["path"]) / name, local_master / name)
    if (digest(local_master / "store.sqlite") != master["store_sha256"] or
            digest(local_master / "history.sqlite") != master["history_sha256"] or
            digest(local_master / "verify.stdout") != master["proof_sha256"]):
        raise ValueError("local master copy differs from sealed source")
    for name in ("store.sqlite", "history.sqlite"):
        (local_master / name).chmod(0o444)
    for name in ("old", *PATTERNS):
        source_manifest = prior_file.parent / f"{name}.tsv"
        if digest(source_manifest) != prior["manifest_sha256"][name]:
            raise ValueError(f"{name} manifest changed")
        shutil.copyfile(source_manifest, output / f"{name}.tsv")
    prior_writer = prior_file.parent / "image-context/bin/write-separated"
    if digest(prior_writer) != prior["writer_binary_sha256"]:
        raise ValueError("sealed writer changed")
    context = output / "image-context"
    (context / "bin").mkdir(parents=True)
    shutil.copyfile(prior_writer, context / "bin/write-separated")
    (context / "bin/write-separated").chmod(0o555)
    builds = {
        "sdk": ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
                "core/Cargo.toml", "--locked", "-p", "layerfs-sdk",
                "--example", "benchmark_shell"],
        "verifier": ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
                     "core/Cargo.toml", "--locked", "-p", "layerfs-server",
                     "--example", "verify_shell"],
        "daemon": ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path",
                   "core/Cargo.toml", "--locked", "--offline", "--target",
                   "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"],
    }
    build_wall = {}
    for name, command in builds.items():
        result, wall = run_command(command, output / f"build-{name}")
        build_wall[name] = wall
        json_file(output / f"build-{name}.json", {"command": command,
                                                  "wall_ns": wall,
                                                  "exit_code": result.returncode})
        if result.returncode:
            raise RuntimeError(f"{name} release build failed; retained logs")
    archive = output / "binary-archive"
    binaries = {name: archive_binary(path, archive, name) for name, path in {
        "benchmark_shell": CORE / "target/release/examples/benchmark_shell",
        "verify_shell": CORE / "target/release/examples/verify_shell",
    }.items()}
    daemon = CORE / "target/aarch64-unknown-linux-musl/release/layerfs-daemon"
    shutil.copyfile(daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o555)
    dockerfile = (f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\n"
                  "COPY bin /fixtures/bin\n"
                  "ENV LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL=25\n"
                  "ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\n"
                  'ENTRYPOINT ["/layerfs-daemon"]\n')
    (context / "Dockerfile").write_text(dockerfile)
    image, image_wall = run_command(["docker", "build", "-q", str(context)],
                                    output / "image-build")
    if image.returncode:
        raise RuntimeError("changed-source image build failed; retained logs")
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prior["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    reproof_command = [binaries["verify_shell"]["path"],
        str(local_master / "verify.case"),
        str(local_master / "store.sqlite"),
        str(local_master / "history.sqlite"),
        str(output / "old.tsv"), str(output / "old.tsv")]
    reproof, reproof_wall = run_command(reproof_command, output / "master-reproof",
                                         timeout=9, env=env)
    if (reproof.returncode or
            json.loads(reproof.stdout.splitlines()[-1])["status"] != "PASS"):
        raise RuntimeError("rebuilt verifier rejected old master; retained logs")
    prepared = {**prior, "schema": "issue265-patterns-prepared-v1",
        "source": source, "binaries": binaries,
        "master": {**master, "path": str(local_master)},
        "treatment_spec_sha256": digest(TREATMENT),
        "writer_binary_sha256": digest(context / "bin/write-separated"),
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "dockerfile_sha256": sha(dockerfile.encode()),
        "image_id": image.stdout.decode().strip(),
        "master_reproof_sha256": digest(output / "master-reproof.stdout"),
        "build_mode": "locked release host SDK/verifier and aarch64 daemon rebuilt; sealed static writer reused",
        "dependency_reuse": {"prior_prepared": str(prior_file),
            "prior_source_commit": prior["source"]["source_commit"],
            "master": "closed Store/history copied into this worktree, exact byte identity and rebuilt-verifier proof",
            "reviewed_core_changes": changed,
            "writer": "source and binary SHA-256 matched"},
        "preparation_wall_ns": {"build": build_wall, "image": image_wall,
                                "master_reproof": reproof_wall}}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"),
                      "image_id": prepared["image_id"]}))


def run(prepared_file, output, pattern):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    prepared = json.loads(prepared_file.read_text())
    source = identities()
    if (prepared["schema"] != "issue265-patterns-prepared-v1" or source["source_dirty"]
            or any(source[key] != prepared["source"][key]
                   for key in ("source_commit", "source_tree", "product_seal", "harness_seal"))
            or digest(SPEC) != prepared["spec_sha256"]
            or digest(TREATMENT) != prepared["treatment_spec_sha256"]
            or digest(WRITER) != prepared["writer_source_sha256"]):
        raise ValueError("prepared source/workload identity changed")
    for item in prepared["binaries"].values():
        if digest(item["path"]) != item["sha256"]:
            raise ValueError("release binary seal mismatch")
    if digest(prepared_file.parent / "image-context/bin/write-separated") != prepared["writer_binary_sha256"]:
        raise ValueError("writer binary seal mismatch")
    image = subprocess.check_output(["docker", "image", "inspect", prepared["image_id"],
                                     "--format", "{{.Id}}"], text=True).strip()
    if image != prepared["image_id"]:
        raise ValueError("image identity changed")
    master = prepared["master"]
    if digest(Path(master["path"]) / "verify.stdout") != master["proof_sha256"]:
        raise ValueError("old-head master proof changed")
    if ("master_reproof_sha256" in prepared and
            digest(prepared_file.parent / "master-reproof.stdout") !=
            prepared["master_reproof_sha256"]):
        raise ValueError("changed-source master reproof changed")
    for name in ("old", pattern):
        if digest(prepared_file.parent / f"{name}.tsv") != prepared["manifest_sha256"][name]:
            raise ValueError(f"{name} manifest changed")
    clone = {}
    for name in ("store", "history"):
        source_file = Path(master["path"]) / f"{name}.sqlite"
        target = output / f"{name}.sqlite"
        if digest(source_file) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} master seal mismatch")
        shutil.copyfile(source_file, target)
        target.chmod(0o644)
        if digest(target) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} clone seal mismatch")
        clone[name] = str(target)
    store_bytes_before = Path(clone["store"]).stat().st_size
    history_bytes_before = Path(clone["history"]).stat().st_size
    shell = f"/fixtures/bin/write-separated {pattern} data.bin {COUNT}"
    before = fields(master, pattern, shell)
    case_spec(output / "case.before", before)
    command = [prepared["binaries"]["benchmark_shell"]["path"], "run",
               str(output / "case.before"), clone["store"], clone["history"],
               prepared["image_id"]]
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": "1", "LAYERFS_COMPLEXITY_DIAGNOSTIC": "1"}
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=15, env=env)
        stdout, stderr, code, timeout = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timeout = error.stdout or b"", error.stderr or b"", None, True
    complete_ns = time.monotonic_ns() - started
    (output / "driver.stdout").write_bytes(stdout)
    (output / "driver.stderr").write_bytes(stderr)
    try:
        driver = one_receipt(stdout)
    except (ValueError, json.JSONDecodeError):
        driver = None
    try:
        checkpoints = progress(driver, COUNT) if driver else None
        progress_error = None
    except (ValueError, KeyError, UnicodeDecodeError) as error:
        checkpoints, progress_error = None, str(error)
    samples = backing_samples(stderr)
    try:
        extent = extent_diagnostics(stderr)
        extent_error = None
    except (ValueError, KeyError) as error:
        extent, extent_error = None, str(error)
    sample_counts = [row["write_class"] for row in samples]
    counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",")
                  if "=" in item) if driver else {}
    verification = {"status": "NOT_RUN"}
    if driver and driver.get("head_commit"):
        after = {**before, "expected_head_commit": driver["head_commit"]}
        case_spec(output / "case.verify", after)
        verify_command = [prepared["binaries"]["verify_shell"]["path"],
            str(output / "case.verify"), clone["store"], clone["history"],
            str(prepared_file.parent / "old.tsv"), str(prepared_file.parent / f"{pattern}.tsv")]
        try:
            checked, wall = run_command(verify_command, output / "verifier", timeout=9, env=env)
            verification = {"status": "PASS" if checked.returncode == 0 else "FAIL",
                            "wall_ns": wall, "exit_code": checked.returncode,
                            "command": verify_command}
        except subprocess.TimeoutExpired:
            verification = {"status": "FAIL", "timeout": True,
                            "command": verify_command}
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                   and b"sandbox shutdown retained" not in stderr)
    phases = (len(samples) == 4 and sample_counts == [25, 50, 75, 100]
              and all(row["version"] == 2 and row["acquisition_ns"] is not None
                      and row["publication_ns"] is not None for row in samples))
    counted = bool(extent and extent["splices"] == COUNT and
                   [row["writes"] for row in extent["snapshots"]] == [25, 50, 75, 100]
                   and [row["splices"] for row in extent["snapshots"]] == [25, 50, 75, 100]
                   and all(row["edge"] is not None for row in extent["snapshots"])
                   and len(extent["c1_edit"]) == 1 and len(extent["file_input"]) == 1)
    functional = bool(code == 0 and not timeout and driver and
                      driver.get("status") == "COMPLETE" and driver.get("commit_called")
                      and checkpoints and counts.get("write") == "100" and phases
                      and verification["status"] == "PASS" and cleanup)
    receipt = {"schema": "issue265-patterns-attempt-v1", "family_id": "workspace_mounted_write_patterns",
        "selection": pattern, "scenario_id": before["scenario_id"],
        "operation_surface": "public WorkspaceApi mount/exec/commit",
        "source": prepared["source"], "spec_sha256": prepared["spec_sha256"],
        "treatment_spec_sha256": prepared.get("treatment_spec_sha256"),
        "writer_source_sha256": prepared["writer_source_sha256"],
        "writer_binary_sha256": prepared["writer_binary_sha256"],
        "daemon_sha256": prepared["daemon_sha256"],
        "dockerfile_sha256": prepared["dockerfile_sha256"],
        "image_id": prepared["image_id"], "binaries": prepared["binaries"],
        "cargo_config_sha256": prepared["cargo_config_sha256"],
        "dependency_reuse": prepared["dependency_reuse"], "build_mode": prepared["build_mode"],
        "master": master, "clone": clone, "clone_method": prepared["clone_method"],
        "fixture_sha256": prepared["fixture_sha256"],
        "old_manifest_sha256": prepared["manifest_sha256"]["old"],
        "expected_manifest_sha256": prepared["manifest_sha256"][pattern],
        "command": command, "shell_command": shell, "write_syscalls_expected": COUNT,
        "writer_progress": checkpoints, "writer_progress_error": progress_error,
        "backing_samples": samples, "backing_sample_counts_expected": [25, 50, 75, 100],
        "phase_samples_complete": phases,
        "extent_diagnostics": extent, "extent_diagnostics_error": extent_error,
        "extent_diagnostics_complete": counted,
        "payload_write_bytes_source_prediction": COUNT * 4096,
        "retained_payload_allocated_bytes": [
            row["backing"]["allocated_bytes"] - row["metadata"]["allocated_bytes"]
            for row in samples],
        "complexity_lines": [line for line in stderr.decode(errors="replace").splitlines()
                             if line.startswith(("LFS_PIECE_LOWER ", "LFS_C1_SAVE_COUNT ",
                                                 "LFS_C1_EDIT_LOAD ", "LFS_FILE_INPUT ",
                                                 "LFS_EXTENT_SPLICE ", "LFS_EXTENT_EDGE "))],
        "lft1": lft1(stderr), "complete_command_wall_ns": complete_ns,
        "complete_command_limit_s": 15, "driver_exit_code": code,
        "driver_timeout": timeout, "driver": driver, "projection_counts": counts,
        "fuse_write_callbacks_observed": int(counts["write"]) if "write" in counts else None,
        "fuse_write_count_provenance": "Status write class; only data writes in shell command",
        "store_bytes_before": store_bytes_before,
        "store_bytes_after": Path(clone["store"]).stat().st_size,
        "history_bytes_before": history_bytes_before,
        "history_bytes_after": Path(clone["history"]).stat().st_size,
        "verifier": verification, "cleanup_status": "PASS" if cleanup else "FAIL",
        "functional_status": "PASS" if functional else "FAIL",
        "cache_contract": prepared["cache_contract"], "cache_status": "INELIGIBLE",
        "performance_status": "INELIGIBLE", "admission_eligible": False,
        "row_status": "FAIL" if not functional else
                      ("INELIGIBLE" if counted else "INCOMPLETE"), "sample_count": 1}
    json_file(output / "receipt.json", receipt)
    (output / "SHA256SUMS").write_text("\n".join(
        f"{digest(path)}  {path.name}" for path in sorted(output.iterdir()) if path.is_file()
        and path.name != "SHA256SUMS") + "\n")
    print(json.dumps({"receipt": str(output / "receipt.json"),
                      "row_status": receipt["row_status"]}))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("self-check")
    p = sub.add_parser("prepare-reuse")
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--prior-prepared", required=True, type=Path)
    p = sub.add_parser("run")
    p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--selection", required=True, choices=PATTERNS)
    p = sub.add_parser("report")
    p.add_argument("--receipts", required=True, type=Path, nargs=3)
    p.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.action == "self-check":
        positions = [(104729 + i * 2654435761) % SIZE for i in range(COUNT)]
        assert len(set(positions)) == COUNT and all(0 < p < SIZE - 1 for p in positions)
        assert all(abs(a - b) > 1 for i, a in enumerate(positions) for b in positions[i + 1:])
        assert [len(expected(p)["data.bin"]) for p in PATTERNS] == [SIZE + COUNT, SIZE, SIZE]
        check = (b"LFS_EXTENT_SPLICE v=1 root_height=1 leaf_visits=1 branch_visits=1 "
                 b"leaf_visit_records=2 branch_visit_children=2 leaf_writes=1 branch_writes=1 "
                 b"leaf_write_records=3 branch_write_children=2 leaf_min=3 leaf_max=3 "
                 b"child_edges_added=2 custody_edges_added=1\n"
                 b"LFS_EXTENT_EDGE v=1 child_edges_removed=1 custody_edges_removed=1\n"
                 b"LFS_WRITE_SAMPLE v=2 write_class=Some(25)\n")
        parsed = extent_diagnostics(check)
        assert parsed["splices"] == 1 and parsed["snapshots"][0]["totals"]["leaf_writes"] == 1
        assert parsed["snapshots"][0]["edge"]["custody_edges_removed"] == 1
        print(json.dumps({"spec_sha256": digest(SPEC), "writer_sha256": digest(WRITER),
                          "patterns": PATTERNS, "size": SIZE, "writes": COUNT}))
    elif args.action == "report":
        report(args.receipts, args.output)
    else:
        lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.action == "prepare-reuse":
                prepare_reuse(args.output, args.prior_prepared)
            else:
                run(args.prepared, args.output, args.selection)


if __name__ == "__main__":
    main()
