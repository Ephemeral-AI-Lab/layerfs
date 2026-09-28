#!/usr/bin/env python3
"""#273 checkpoint 5: sealed paired mounted-checkpoint harness.

One harness file, byte-identical for both source arms, invoked once per arm
with ``--arm``/``--repo``. It builds only that arm's own locked release
binaries inside that arm's own worktree, re-proves the shared sealed masters,
applies the declared cache procedure, runs one measured command per selection,
verifies it with a shared independent oracle and that arm's own verifier, and
retains every receipt append-only.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
OWNER = HERE.parents[2]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(OWNER / "benchmark/fs-bench-pro/shared"))
from cold import Residency  # noqa: E402
from separated_writes import backing_samples, progress  # noqa: E402
from write_patterns import extent_diagnostics  # noqa: E402
from shell_package import (digest, json_file, lft1, manifest, one_receipt,  # noqa: E402
                           run_command, sha)

SPEC = OWNER / "docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md"
WRITER = HERE / "writers/write-separated.c"
ORACLE_SOURCE = OWNER / "core/crates/layerfs-server/examples/verify_checkpoint5.rs"
RESULTS = OWNER / "benchmark-results/fs-bench-pro/issue273/checkpoint5"
LOCK = OWNER / "benchmark-results/fs-bench-pro/.run.lock"
BASE_IMAGE = "alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8"
SIZE = 10 << 20
GATE_SIZE = 8194
WORKERS = "1"

PATTERNS_MASTER = Path(
    "/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs/benchmark-results/"
    "fs-bench-pro/issue271/fourhop-patterns-prepared-v1/prepared.json")
GATE_MASTER = Path(
    "/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs/benchmark-results/"
    "fs-bench-pro/issue271/fourhop-gate-prepared-v1/prepared.json")

# Frozen master identities, transcribed from the #271 published prepared records.
MASTER_FREEZE = {
    "patterns": {
        "prepared": PATTERNS_MASTER,
        "store_sha256": "20ea70dc9509e1819b11667bf97ab2a5ab3bd2b5d628d0a48f4c4fa6876691be",
        "history_sha256": "55a02d7da0c35b084b721706354dcaf6e30aee3f824ad32543cf2e108dbdfd19",
        "proof_sha256": "d2f535e0a405eca68746b7968350118b85ddbe69b95a66416a38238a7da5f6c0",
        "fixture_sha256": "eb6183addde05c2196ce25e6fa34a4baf20f9bf30d33892f452a9a1e88c9a472",
        "cursor_key": "e5ac12eede4651027643f28844886b26e68406f0dbcf0ca26fe4a27bdd6953d5",
        "old_commit": "12c0b3b66db8c63d5cc068e8d96c22e545e70a70633dc81b813bfbfcc44a3a62a7",
        "branch_id": "11094cc9de4f7dd10dabd617bb074906e8",
        "project_id": "31bf50228be038338dd77c4822bf4f886d",
        "genesis_layer": "326a560e44b5fe83f523a19227fd2b166dc2553eef53ab43a04416c94644829143",
        "genesis_root": "91f1706b248c3da05e1acfa8a53b83cd2cb0954b3b4b03609aa711c6dd0ed5d0",
        "genesis_root_serial": 1,
        "content": "matrix-old",
    },
    "gate": {
        "prepared": GATE_MASTER,
        "store_sha256": "ce49eb722338ea58f2cb397128e69fee5c4c0abf05821e2b4a959101257cc683",
        "history_sha256": "96275c90cb7ee51074cb456dbf1f29f164442df1b1f5e7ea200c58449d52b038",
        "proof_sha256": None,
        "fixture_sha256": "871461a44757b68cc17766f85975e57a898d08979b77cc15ead01491353be3c4",
        "cursor_key": "b2d1412b075942a3f6106948a28a122b5e362db905336d713af9d95aaf8bfbfa",
        "old_commit": "12ef44464e764a34399e5be721a9d14ab714813471949c06bf20413d2ca6f9a098",
        "branch_id": "111e48eb29a1337de64a74a18e42cf8318",
        "project_id": "3139aa4755cbda68b7890b9e8f483309f2",
        "genesis_layer": "32d0a789bac2f4b875d616012c0a003a2aca2eeaead8717cfa7332cd433321a348",
        "genesis_root": "a1a41401f8d9d5de6e5cabd3d8f49f68d30549e1288c9dd278c91da413f56b58",
        "genesis_root_serial": 1,
        "content": "gate-old",
    },
}

# The published #271 manifests the frozen harness must reproduce byte for byte.
MANIFEST_FREEZE = {
    "old": "d664c30d37508679421ab4989c3256f5da430edda0c4dc4d148d863085249927",
    "append": "8be1076d3284ca7daf8be30197fff138cbc9872a573b8d2bac80e3f41a9604fc",
    "dispersed": "b9f6ccc3fe182377f6eac4009450db59177649e4a906d812ef539f31c2c54079",
    "repeated": "52ccb3b0de2001e89325508fa7aca6e48bb870df6a09541f4a66255ae553c8f6",
    "gate-old": "048ad43d8d2664ae6aee48f89e772b8ead56d512ec190dedc43e065f62f6038e",
    "gate-new": "92eb1f8cbde5cd73fb3c8018ecd6470ae8f6b8fa67d051aa175c49f9aaf44925",
}

MATRIX_INTERVAL = {100: 25, 512: 128, 4097: 1024}


def matrix(pattern, count, base=None):
    content = bytearray(base if base is not None else b"A" * SIZE)
    for i in range(count):
        byte = ord("B") + i % 24
        if pattern == "append":
            content.append(byte)
        elif pattern == "dispersed":
            content[(104729 + i * 2654435761) % SIZE] = byte
        elif pattern == "repeated":
            content[5 << 20] = byte
        else:
            raise ValueError(f"unknown pattern {pattern}")
    return bytes(content)


def gate_new(count):
    content = bytearray(b"A" * GATE_SIZE)
    content[0:2 * count:2] = b"X" * count
    return bytes(content)


def files_manifest(content):
    return manifest({"data.bin": content})


def selections():
    rows = []
    order = 0
    for pattern in ("append", "dispersed", "repeated"):
        for count in (100, 512, 4097):
            order += 1
            rows.append({
                "order": order, "scenario_id": f"issue273-{pattern}-{count}-10m-v1",
                "master": "patterns", "pattern": pattern, "count": count,
                "interval": MATRIX_INTERVAL[count],
                "limit_s": 25 if count == 4097 else 15,
                "command": f"/fixtures/bin/write-separated {pattern} data.bin {count}",
                "oracle_manifest": f"matrix-{pattern}-{count}",
                "old_manifest": "old", "separated_count": None, "writes": count})
    rows.append({
        "order": 10, "scenario_id": "issue273-clean-commit-v1", "master": "retained",
        "pattern": None, "count": 0, "interval": MATRIX_INTERVAL[100], "limit_s": 15,
        "command": "true", "oracle_manifest": "retained-old",
        "old_manifest": "retained-old", "separated_count": None, "writes": 0})
    rows.append({
        "order": 11, "scenario_id": "issue273-one-edit-commit-v1", "master": "retained",
        "pattern": None, "count": 1, "interval": MATRIX_INTERVAL[100], "limit_s": 15,
        "command": "/fixtures/bin/write-separated separated data.bin 1",
        "oracle_manifest": "retained-one-edit", "old_manifest": "retained-old",
        "separated_count": None, "writes": 1})
    rows.append({
        "order": 12, "scenario_id": "issue248-separated-4097-v1", "master": "gate",
        "pattern": None, "count": 4097, "interval": MATRIX_INTERVAL[4097],
        "limit_s": 25, "command": "/fixtures/bin/write-separated separated data.bin 4097",
        "oracle_manifest": "gate-new", "old_manifest": "gate-old",
        "separated_count": 4097, "writes": 4097})
    return rows


SELECTIONS = selections()
BY_ID = {row["scenario_id"]: row for row in SELECTIONS}
RETAINED_COMMAND = "/fixtures/bin/write-separated dispersed data.bin 4097"
# Labelled harness diagnostic; never a registered selection and never a result.
PREFLIGHT = {"order": 0, "scenario_id": "issue273-harness-preflight-v1", "master": "patterns",
             "pattern": "append", "count": 100, "interval": MATRIX_INTERVAL[100], "limit_s": 15,
             "command": "/fixtures/bin/write-separated append data.bin 100",
             "oracle_manifest": "matrix-append-100", "old_manifest": "old",
             "separated_count": None, "writes": 100}


def seal(entries):
    """Hash labelled files; every label is recorded in the identity."""
    value = hashlib.sha256()
    for label, path in sorted(entries):
        value.update(label.encode() + b"\0")
        value.update(path.read_bytes())
    return value.hexdigest()


def labelled(paths, base, prefix=""):
    return [(prefix + str(Path(path).relative_to(base)), Path(path)) for path in paths]


def git(repo, *args):
    return subprocess.check_output(["git", *args], cwd=repo, text=True).strip()


def arm_identity(repo):
    core = repo / "core"
    product = list((core / "crates").glob("*/src/**/*.rs"))
    product += list((core / "crates").glob("*/examples/*.rs"))
    product += list((core / "crates").glob("*/Cargo.toml"))
    product += list((core / "crates/layerfs-api").glob("*/src/**/*.rs"))
    product += list((core / "crates/layerfs-api").glob("*/examples/*.rs"))
    product += list((core / "crates/layerfs-api").glob("*/Cargo.toml"))
    product += [core / "Cargo.toml", core / "Cargo.lock", repo / ".cargo/config.toml"]
    source = list((core / "crates").glob("*/src/**/*.rs"))
    source += list((core / "crates/layerfs-api").glob("*/src/**/*.rs"))
    workload = [core / "crates/layerfs-api/sdk/examples/benchmark_shell.rs",
                core / "crates/layerfs-server/examples/verify_shell.rs",
                WRITER, ORACLE_SOURCE]
    dirty = git(repo, "status", "--porcelain")
    shared = [WRITER, ORACLE_SOURCE, *sorted(HERE.glob("**/*.py"))]
    return {"arm_repo": str(repo), "source_commit": git(repo, "rev-parse", "HEAD"),
            "source_tree": git(repo, "rev-parse", "HEAD^{tree}"), "source_dirty": bool(dirty),
            "dirty_paths": dirty.splitlines(),
            "product_seal": seal(labelled(product, repo)),
            "product_source_seal": seal(labelled(source, repo)),
            "workload_seal": seal(labelled(workload[:2], repo)
                                  + labelled(shared, OWNER, "harness/")),
            "harness_seal": seal(labelled(sorted(HERE.glob("**/*.py")), OWNER)),
            "cargo_lock_sha256": digest(core / "Cargo.lock"),
            "cargo_config_sha256": digest(repo / ".cargo/config.toml"),
            "build_profile": "release", "construction_workers": WORKERS,
            "rustc": subprocess.check_output(["rustc", "+1.85.1", "--version"], text=True).strip()}


def require_clean(identity):
    if identity["source_dirty"]:
        raise ValueError(f"arm worktree is dirty: {identity['dirty_paths']}")


def run_in(repo, command, log, timeout=None, env=None):
    started = time.monotonic_ns()
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=timeout, env=env)
    Path(str(log) + ".stdout").write_bytes(result.stdout)
    Path(str(log) + ".stderr").write_bytes(result.stderr)
    return result, time.monotonic_ns() - started


def build_arm(repo, output, names):
    commands = {
        "benchmark_shell": ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
                            "core/Cargo.toml", "--locked", "-p", "layerfs-sdk",
                            "--example", "benchmark_shell"],
        "verify_shell": ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
                         "core/Cargo.toml", "--locked", "-p", "layerfs-server",
                         "--example", "verify_shell"],
        "verify_checkpoint5": ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
                               "core/Cargo.toml", "--locked", "-p", "layerfs-server",
                               "--example", "verify_checkpoint5"],
        "layerfs-daemon": ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path",
                           "core/Cargo.toml", "--locked", "--offline", "--target",
                           "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"],
    }
    paths = {
        "benchmark_shell": repo / "core/target/release/examples/benchmark_shell",
        "verify_shell": repo / "core/target/release/examples/verify_shell",
        "verify_checkpoint5": repo / "core/target/release/examples/verify_checkpoint5",
        "layerfs-daemon": repo / "core/target/aarch64-unknown-linux-musl/release/layerfs-daemon",
    }
    record = {}
    for name in names:
        result, wall = run_in(repo, commands[name], output / f"build-{name}", timeout=None)
        record[name] = {"command": commands[name], "wall_ns": wall,
                        "exit_code": result.returncode,
                        "status": "PASS" if result.returncode == 0 else "FAIL"}
        if result.returncode:
            raise RuntimeError(f"{name} build failed; retained output")
    return record, paths


def archive_binary(source, folder, name):
    value = digest(source)
    target = folder / value / name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(0o555)
    return {"path": str(target), "sha256": value}


def writer_binary(context, log):
    target = context / "bin/write-separated"
    target.parent.mkdir(parents=True, exist_ok=True)
    result, wall = run_in(OWNER, ["zig", "cc", "-target", "aarch64-linux-musl", "-O3",
                                  "-static", str(WRITER), "-o", str(target)], log)
    if result.returncode:
        raise RuntimeError("writer build failed; retained output")
    target.chmod(0o555)
    return target, wall


def build_images(context, output, intervals, daemon):
    shutil.copyfile(daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o555)
    images = {}
    for interval in sorted(intervals):
        dockerfile = (f"FROM {BASE_IMAGE}\nCOPY layerfs-daemon /layerfs-daemon\n"
                      "COPY bin /fixtures/bin\n"
                      f"ENV LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL={interval}\n"
                      "ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\n"
                      'ENTRYPOINT ["/layerfs-daemon"]\n')
        (context / "Dockerfile").write_text(dockerfile)
        result, wall = run_in(OWNER, ["docker", "build", "-q", str(context)],
                              output / f"image-build-{interval}")
        if result.returncode:
            raise RuntimeError(f"image build for interval {interval} failed")
        images[str(interval)] = {"image_id": result.stdout.decode().strip(),
                                 "dockerfile_sha256": sha(dockerfile.encode()),
                                 "wall_ns": wall}
    return images


def load_master(name, destination):
    freeze = MASTER_FREEZE[name]
    prepared = json.loads(freeze["prepared"].read_text())
    master = prepared["master"]
    source = Path(master["path"])
    if digest(source / "store.sqlite") != freeze["store_sha256"]:
        raise ValueError(f"{name} master Store seal mismatch")
    if digest(source / "history.sqlite") != freeze["history_sha256"]:
        raise ValueError(f"{name} master history seal mismatch")
    destination.mkdir(parents=True, exist_ok=True)
    for file in ("store.sqlite", "history.sqlite"):
        shutil.copyfile(source / file, destination / file)
        (destination / file).chmod(0o444)
    if freeze["proof_sha256"] is not None and digest(source / "verify.stdout") != freeze["proof_sha256"]:
        raise ValueError(f"{name} master proof seal mismatch")
    return {"path": str(destination), "source_path": str(source),
            "store_sha256": freeze["store_sha256"], "history_sha256": freeze["history_sha256"],
            "proof_sha256": freeze["proof_sha256"], "cursor_key": freeze["cursor_key"],
            "fixture_sha256": freeze["fixture_sha256"],
            **{key: freeze[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                            "genesis_root_serial", "branch_id", "old_commit")}}


def splice_curve(stderr, expected_samples):
    """Cumulative v1 extent-splice totals at each declared write-class sample."""
    splices = 0
    curve = []
    for line in stderr.decode(errors="replace").splitlines():
        if line.startswith("LFS_EXTENT_SPLICE "):
            splices += 1
        elif line.startswith("LFS_WRITE_SAMPLE ") and "write_class=Some(" in line:
            match = re.search(r"write_class=Some\((\d+)\)", line)
            if match:
                curve.append({"write_class": int(match[1]), "splices": splices})
    classes = [row["write_class"] for row in curve]
    complete = bool(curve) and classes == list(expected_samples) and all(
        row["splices"] == row["write_class"] for row in curve)
    return {"total_splices": splices, "curve": curve, "classes": classes,
            "expected_classes": list(expected_samples), "complete": complete}


def c1_observation(extent):
    """Zero is available only from a complete emitted C1 source observation."""
    c1 = extent.get("c1_edit", [])
    inputs = extent.get("file_input", [])
    required = (("v", "draft_nodes_read", "nodes_read", "stored_nodes_read"),
                ("v", "edits", "record_lookups", "record_reads",
                 "replacement_bytes_read", "replacement_reads"))
    valid = (len(c1) == len(inputs) == 1
             and all(isinstance(row, dict) and row.get("v") == 1
                     and all(type(row.get(key)) is int and row[key] >= 0 for key in keys)
                     for row, keys in zip((c1[0], inputs[0]), required))
             and inputs[0].get("spool_resident") in ("true", "false"))
    return {"complete": bool(valid), "provenance": "emitted_v1" if valid else
            "missing_or_malformed_or_interleaved", "c1_work": (
                c1[0]["nodes_read"] if valid else None)}


PACK_SOURCE = re.compile(
    r"LFS_ACTIVE_SOURCE v=1 complete=(true|false) "
    r"windows=(\d+) references=(\d+) distinct_packs=(\d+) fill_ns=(\d+) "
    r"pack_loads=(\d+) pack_hits=(\d+) locator_lookups=(\d+) "
    r"index_reads=(\d+) index_seeks=(\d+) locator_ns=(\d+) pack_ns=(\d+) "
    r"decoded_records=(\d+) decoded_bytes=(\d+) copied_bytes=(\d+) "
    r"ref_limit=256 byte_limit=32768")
PACK_SOURCE_KEYS = ("windows", "references", "distinct_packs", "fill_ns", "pack_loads",
                    "pack_hits", "locator_lookups", "index_reads", "index_seeks",
                    "locator_ns", "pack_ns", "decoded_records", "decoded_bytes",
                    "copied_bytes")


def source_observation(stderr):
    """Refuse missing, interleaved or partial source counters, not a zero."""
    lines = [line for line in stderr.decode(errors="replace").splitlines()
             if line.startswith("LFS_ACTIVE_SOURCE ")]
    if len(lines) != 1:
        return {"status": "INCOMPLETE", "reason": "expected exactly one source row",
                "line_count": len(lines), "counts": None}
    match = PACK_SOURCE.fullmatch(lines[0])
    if not match or match[1] != "true":
        return {"status": "INCOMPLETE", "reason": "malformed/interleaved/incomplete row",
                "line_count": 1, "counts": None}
    counts = dict(zip(PACK_SOURCE_KEYS, map(int, match.groups()[1:])))
    if (counts["pack_loads"] > counts["distinct_packs"]
            or counts["pack_loads"] + counts["pack_hits"] != counts["references"]
            or counts["references"] < counts["windows"]):
        return {"status": "INCOMPLETE", "reason": "inconsistent phase counters",
                "line_count": 1, "counts": None}
    return {"status": "PASS", "reason": "complete emitted production v1 row",
            "line_count": 1, "counts": counts}


def cache_files(paths):
    """Validate every input before eviction; after final mincore never read input."""
    residency = Residency()
    evidence = {"method": "darwin-shared-mmap-invalidate-mincore-v2", "files": {}}
    # A buffered digest faults the input into cache: finish *all* digests first.
    for path in paths:
        path = Path(path)
        evidence["files"][path.name] = {
            "path": str(path), "sha256": digest(path), "bytes": path.stat().st_size}
    for row in evidence["files"].values():
        descriptor = os.open(row["path"], os.O_RDONLY)
        try:
            pages, resident = residency.check(descriptor, row["bytes"], evict=True)
            row.update(pages=pages, resident_after_invalidate=resident)
        finally:
            os.close(descriptor)
    for row in evidence["files"].values():
        descriptor = os.open(row["path"], os.O_RDONLY)
        try:
            pages, remaining = residency.check(descriptor, row["bytes"])
            row.update(final_pages=pages, resident_on_recheck=remaining)
        finally:
            os.close(descriptor)
    evidence["status"] = "PASS" if all(
        row["resident_after_invalidate"] == 0 and row["resident_on_recheck"] == 0
        for row in evidence["files"].values()) else "FAIL"
    evidence["final_check_ns"] = time.monotonic_ns()
    return evidence


def case_fields(master, selection, command, old_commit=None):
    fields = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                           "genesis_root_serial", "branch_id")}
    fields["old_commit"] = old_commit or master["old_commit"]
    fields.update(scenario_id=selection["scenario_id"], command_hex=command.encode().hex(),
                  expected_failure="0",
                  telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    if selection["separated_count"]:
        fields["separated_count"] = str(selection["separated_count"])
    if selection["pattern"]:
        fields["write_pattern"] = selection["pattern"]
        fields["pattern_count"] = str(selection["count"])
    return fields


def case_spec(path, values):
    Path(path).write_text("".join(f"{key}={value}\n" for key, value in sorted(values.items())))


def verifier(oracle_path, case, store, history, old_manifest, new_manifest, env, output, label):
    command = [str(value) for value in (oracle_path, case, store, history,
                                        old_manifest, new_manifest)]
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, cwd=OWNER, capture_output=True, timeout=9, env=env)
    except subprocess.TimeoutExpired as error:
        (output / f"{label}.stdout").write_bytes(error.stdout or b"")
        (output / f"{label}.stderr").write_bytes(error.stderr or b"")
        return {"status": "FAIL", "timeout": True, "limit_ns": 9_000_000_000,
                "wall_ns": time.monotonic_ns() - started, "command": command}
    wall = time.monotonic_ns() - started
    (output / f"{label}.stdout").write_bytes(result.stdout)
    (output / f"{label}.stderr").write_bytes(result.stderr)
    row = {"status": "PASS" if result.returncode == 0 else "FAIL", "wall_ns": wall,
           "limit_ns": 9_000_000_000, "exit_code": result.returncode, "command": command}
    for line in reversed(result.stdout.decode(errors="replace").splitlines()):
        try:
            row["oracle_receipt"] = json.loads(line)
            break
        except ValueError:
            continue
    return row


def run_measured(command, limit_s, env, output):
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, cwd=OWNER, capture_output=True,
                                timeout=limit_s, env=env)
        stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b""
        stderr = error.stderr or b""
        code, timed_out = None, True
    wall = time.monotonic_ns() - started
    (output / "driver.stdout").write_bytes(stdout)
    (output / "driver.stderr").write_bytes(stderr)
    return {"started_ns": started, "complete_command_wall_ns": wall, "exit_code": code,
            "timeout": timed_out}


def run_cold_measured(paths, command, limit_s, env, output):
    """No input identity reads between final whole-input check and launch."""
    cache = cache_files(paths)
    measured = run_measured(command, limit_s, env, output)
    cache["launch_gap_ns"] = measured["started_ns"] - cache["final_check_ns"]
    cache["launch_gap_limit_ns"] = 1_000_000_000
    if cache["launch_gap_ns"] > cache["launch_gap_limit_ns"]:
        cache["status"] = "FAIL"
    return cache, measured


def clone_master(store_source, history_source, store_sha, history_sha, output):
    clone = {}
    for name, source, expected in (("store", store_source, store_sha),
                                   ("history", history_source, history_sha)):
        target = output / f"{name}.sqlite"
        if digest(source) != expected:
            raise ValueError(f"{name} master seal mismatch")
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if digest(target) != expected:
            raise ValueError(f"{name} clone seal mismatch")
        clone[name] = str(target)
    return clone


def image_for(prepared, interval):
    row = prepared["images"][str(interval)]
    actual = subprocess.check_output(["docker", "image", "inspect", row["image_id"],
                                      "--format", "{{.Id}}"], text=True).strip()
    if actual != row["image_id"]:
        raise ValueError("image identity changed")
    return row["image_id"]


def arm_env(prepared):
    return {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"],
            "LAYERFS_CONSTRUCTION_WORKERS": WORKERS,
            "LAYERFS_COMPLEXITY_DIAGNOSTIC": "1"}


def release_check(prepared, prepared_file, output):
    if prepared.get("schema") != "issue273-checkpoint5-prepared-v2":
        raise ValueError("unsupported checkpoint5 prepared/cache contract")
    identity = arm_identity(Path(prepared["arm_repo"]))
    require_clean(identity)
    for key in ("source_commit", "source_tree", "product_seal", "harness_seal"):
        if identity[key] != prepared["source"][key]:
            raise ValueError(f"arm {key} changed since preparation")
    if digest(SPEC) != prepared["spec_sha256"] or digest(WRITER) != prepared["writer_source_sha256"]:
        raise ValueError("specification or writer changed")
    if digest(ORACLE_SOURCE) != prepared["oracle_source_sha256"]:
        raise ValueError("oracle source changed")
    for name, row in prepared["binaries"].items():
        if digest(row["path"]) != row["sha256"]:
            raise ValueError(f"{name} binary seal mismatch")
    if prepared_file is not None:
        for name in prepared["manifests"]:
            if digest(prepared_file.parent / f"{name}.tsv") != prepared["manifests"][name]:
                raise ValueError(f"{name} oracle manifest changed")
    for name, row in prepared["masters"].items():
        for file in ("store.sqlite", "history.sqlite"):
            if digest(Path(row["path"]) / file) != row[
                    "store_sha256" if file.startswith("store") else "history_sha256"]:
                raise ValueError(f"{name} master changed")
    return identity


def oracle(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    identity = arm_identity(repo)
    require_clean(identity)
    builds, paths = build_arm(repo, output, ("verify_checkpoint5",))
    archived = archive_binary(paths["verify_checkpoint5"], output / "binary-archive",
                              "verify_checkpoint5")
    record = {"schema": "issue273-checkpoint5-oracle-v1", "repo": str(repo),
              "source_commit": identity["source_commit"],
              "source_tree": identity["source_tree"],
              "product_source_seal": identity["product_source_seal"],
              "oracle_source_sha256": digest(ORACLE_SOURCE),
              "binary": archived, "builds": builds,
              "role": "one binary, built once, reused byte-identically by both source arms"}
    json_file(output / "oracle.json", record)
    print(json.dumps({"oracle": str(output / "oracle.json"),
                      "binary_sha256": archived["sha256"]}))


def prepare(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    identity = arm_identity(repo)
    require_clean(identity)
    oracle_record = json.loads(args.oracle.read_text())
    if digest(oracle_record["binary"]["path"]) != oracle_record["binary"]["sha256"]:
        raise ValueError("shared oracle binary seal mismatch")
    if digest(ORACLE_SOURCE) != oracle_record["oracle_source_sha256"]:
        raise ValueError("shared oracle source changed")
    builds, paths = build_arm(repo, output, ("benchmark_shell", "verify_shell",
                                             "layerfs-daemon"))
    archive = output / "binary-archive"
    binaries = {name: archive_binary(paths[name], archive, name)
                for name in ("benchmark_shell", "verify_shell")}
    masters = {}
    for name in ("patterns", "gate"):
        masters[name] = load_master(name, output / "masters" / name)
    manifests = {}
    retained_old = matrix("dispersed", 4097)
    contents = {
        "old": b"A" * SIZE,
        "gate-old": b"A" * GATE_SIZE,
        "gate-new": gate_new(4097),
        "retained-old": retained_old,
        "retained-one-edit": b"X" + retained_old[1:],
    }
    for pattern in ("append", "dispersed", "repeated"):
        for count in (100, 512, 4097):
            contents[f"matrix-{pattern}-{count}"] = matrix(pattern, count)
    for name, content in contents.items():
        (output / f"{name}.tsv").write_text(files_manifest(content))
        manifests[name] = digest(output / f"{name}.tsv")
    for name, frozen in MANIFEST_FREEZE.items():
        if name in manifests and manifests[name] != frozen:
            raise ValueError(f"oracle manifest {name} differs from the frozen #271 manifest")
    context = output / "image-context"
    oracle, writer_wall = writer_binary(context, output / "writer-build")
    env = {"LAYERFS_HISTORY_CURSOR_KEY": masters["patterns"]["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": WORKERS}
    reproof = {}
    for name, master in masters.items():
        case = output / f"{name}-reproof.case"
        case_spec(case, {**{key: master[key] for key in
                            ("project_id", "genesis_layer", "genesis_root",
                             "genesis_root_serial", "branch_id", "old_commit")},
                         "scenario_id": f"{name}-master-reproof", "expected_failure": "1",
                         "telemetry_run": str(int.from_bytes(os.urandom(16), "big") or 1)})
        old = output / ("old.tsv" if name == "patterns" else "gate-old.tsv")
        row = verifier(paths["verify_shell"], case, Path(master["path"]) / "store.sqlite",
                       Path(master["path"]) / "history.sqlite", old, old, env, output,
                       f"master-reproof-{name}")
        reproof[name] = row
        if row["status"] != "PASS":
            raise RuntimeError(f"{name} master rejected by this arm's verifier")
    images = build_images(context, output, {row["interval"] for row in SELECTIONS},
                          paths["layerfs-daemon"])
    prepared = {"schema": "issue273-checkpoint5-prepared-v2", "arm": args.arm,
                "arm_repo": str(repo), "source": identity, "spec_sha256": digest(SPEC),
                "writer_source_sha256": digest(WRITER), "oracle_source_sha256": digest(ORACLE_SOURCE),
                "writer_binary_sha256": digest(oracle), "oracle": oracle_record,
                "binaries": {**binaries,
                             "verify_checkpoint5": oracle_record["binary"]},
                "masters": masters, "manifests": manifests,
                "images": images, "cursor_key": masters["patterns"]["cursor_key"],
                "builds": builds, "writer_build_wall_ns": writer_wall,
                "master_reproof": reproof,
                "clone_method": "shutil.copyfile independent writable byte copy",
                "cache_contract": "darwin-shared-mmap-invalidate-mincore-v2 all digests before eviction, "
                                  "all evictions before final whole-input checks; host source "
                                  "invalidation; container private backing is not invalidated "
                                  "between Exec and Commit",
                "build_mode": "locked Cargo release, worktree-local target; zig cc -O3 static writer",
                "construction_workers": WORKERS}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"), "arm": args.arm,
                      "images": {key: row["image_id"] for key, row in images.items()}}))


def retained(args):
    raise ValueError("retained-v1/v2 closed Store cannot pin a live private "
                     "4,097-record journal in the same Workspace through "
                     "the public SDK; no retained performance preparation is valid")


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    prepared = json.loads(args.prepared.read_text())
    identity = release_check(prepared, args.prepared, output)
    selection = BY_ID.get(args.selection) or PREFLIGHT
    if selection["master"] == "retained":
        json_file(output / "blocker.json", {
            "schema": "issue273-retained-unsupported-v1", "row_status": "NOT_RUN",
            "admission_eligible": False, "scenario_id": selection["scenario_id"],
            "source": identity,
            "reason": "a committed-and-reattached Store is not a live pinned 4097-record "
                      "journal in the same Workspace; the SDK offers no public pin "
                      "across sequential Exec/Commit without a command lease"})
        print(json.dumps({"scenario_id": selection["scenario_id"],
                          "row_status": "NOT_RUN", "blocker": str(output / "blocker.json")}))
        return
    if args.selection == PREFLIGHT["scenario_id"] and "preflight" not in str(output):
        raise ValueError("the harness pre-flight must use an output path that names it")
    env = arm_env(prepared)
    if selection["master"] == "retained":
        retained_record = json.loads(args.retained.read_text())
        master = {key: retained_record[key] for key in
                  ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial",
                   "branch_id")}
        master.update(old_commit=retained_record["head_commit"], path=retained_record["path"],
                      store_sha256=retained_record["store_sha256"],
                      history_sha256=retained_record["history_sha256"],
                      fixture_sha256=prepared["masters"]["patterns"]["fixture_sha256"])
        base_manifests = retained_record["manifests"]
        if (base_manifests["retained-old"] != prepared["manifests"]["retained-old"]
                or base_manifests["retained-one-edit"] != prepared["manifests"]["retained-one-edit"]):
            raise ValueError("retained base manifests differ from this arm's oracle manifests")
    else:
        master = prepared["masters"][selection["master"]]
    clone = clone_master(Path(master["path"]) / "store.sqlite",
                         Path(master["path"]) / "history.sqlite",
                         master["store_sha256"], master["history_sha256"], output)
    retained_reproof = None
    if selection["master"] == "retained" and not args.diagnostic:
        reproof_case = output / "case.retained-reproof"
        case_spec(reproof_case, {**case_fields(master, selection, "true"),
                                 "expected_failure": "1"})
        retained_reproof = verifier(prepared["binaries"]["verify_shell"]["path"], reproof_case,
                                    clone["store"], clone["history"],
                                    args.prepared.parent / "retained-old.tsv",
                                    args.prepared.parent / "retained-old.tsv", env, output,
                                    "retained-reproof")
        if retained_reproof["status"] != "PASS":
            raise RuntimeError("this arm's verifier rejected the retained base")
    case = output / "case.before"
    case_spec(case, case_fields(master, selection, selection["command"]))
    image_id = image_for(prepared, selection["interval"])
    command = [prepared["binaries"]["benchmark_shell"]["path"], "run", str(case),
               clone["store"], clone["history"], image_id]
    cache, measured = run_cold_measured(
        [clone["store"], clone["history"]], command, selection["limit_s"], env, output)
    (output / "cache.json").write_text(json.dumps(cache, indent=2, sort_keys=True) + "\n")
    stdout = (output / "driver.stdout").read_bytes()
    stderr = (output / "driver.stderr").read_bytes()
    try:
        driver = one_receipt(stdout)
    except (ValueError, json.JSONDecodeError):
        driver = None
    counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",")
                  if "=" in item) if driver else {}
    samples = backing_samples(stderr)
    pack_source = source_observation(stderr)
    try:
        extent = extent_diagnostics(stderr)
    except (ValueError, KeyError) as error:
        extent = {"error": str(error)}
    try:
        checkpoints = progress(driver, selection["count"]) if driver and selection["count"] else None
        progress_error = None
    except (ValueError, KeyError, UnicodeDecodeError) as error:
        checkpoints, progress_error = None, str(error)
    verification = {"oracle": {"status": "NOT_RUN"}, "arm": {"status": "NOT_RUN"}}
    if driver and driver.get("head_commit") and not args.diagnostic:
        verify_case = output / "case.verify"
        fields = {**case_fields(master, selection, selection["command"]),
                  "expected_head_commit": driver["head_commit"]}
        case_spec(verify_case, fields)
        verification["oracle"] = verifier(
            prepared["binaries"]["verify_checkpoint5"]["path"], verify_case, clone["store"],
            clone["history"], args.prepared.parent / f"{selection['old_manifest']}.tsv",
            args.prepared.parent / f"{selection['oracle_manifest']}.tsv", env, output,
            "oracle")
        advanced = driver["head_commit"] != master["old_commit"]
        arm_supported = advanced and (selection["scenario_id"] == "issue248-separated-4097-v1"
                                      or selection["master"] == "retained"
                                      or selection["count"] == 100)
        if arm_supported:
            verification["arm"] = verifier(
                prepared["binaries"]["verify_shell"]["path"], verify_case, clone["store"],
                clone["history"], args.prepared.parent / f"{selection['old_manifest']}.tsv",
                args.prepared.parent / f"{selection['oracle_manifest']}.tsv", env, output,
                "arm-verifier")
        elif not advanced:
            verification["arm"] = {
                "status": "NOT_APPLICABLE",
                "reason": "the frozen arm verifier requires an advanced Branch head; "
                          "this Commit did not advance it"}
        else:
            verification["arm"] = {
                "status": "NOT_APPLICABLE",
                "reason": "the frozen arm verifier hard-codes the 100-write pattern schedule"}
    elif args.diagnostic:
        verification = {"oracle": {"status": "SKIPPED", "reason": "labelled causal diagnostic"},
                        "arm": {"status": "SKIPPED", "reason": "labelled causal diagnostic"}}
    else:
        verification["oracle"] = {"status": "NOT_RUN",
                                  "reason": "the driver produced no head Commit"}
        verification["arm"] = {"status": "NOT_RUN",
                               "reason": "the driver produced no head Commit"}
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok"))
    expected_samples = list(range(selection["interval"], selection["count"] + 1,
                                   selection["interval"])) if selection["count"] else []
    phases = [row["write_class"] for row in samples] == expected_samples
    splices = splice_curve(stderr, expected_samples)
    splice_lines = splices["total_splices"]
    extent_present = splice_lines > 0
    extent_complete = None
    if extent_present and selection["count"]:
        extent_complete = bool(splices["complete"]
                               and splices["total_splices"] == selection["count"])
    c1 = c1_observation(extent)
    c1_complete = c1["complete"]
    fusecount_ok = (int(counts.get("write", -1)) == selection["writes"]) if driver else False
    progress_ok = bool(selection["count"] == 0 or (checkpoints and
                       [row["writes"] for row in checkpoints] ==
                       ([1] if selection["count"] == 1 else [selection["count"] // 4,
                        selection["count"] // 2, selection["count"] * 3 // 4,
                        selection["count"]])))
    verified = (verification["oracle"]["status"] == "PASS"
                and verification["arm"]["status"] in ("PASS", "NOT_APPLICABLE"))
    functional = bool(driver and driver.get("status") == "COMPLETE"
                      and driver.get("commit_called") and not measured["timeout"]
                      and measured["exit_code"] == 0 and (verified or args.diagnostic) and cleanup)
    limit_met = measured["complete_command_wall_ns"] <= selection["limit_s"] * 1_000_000_000
    counts_ok = fusecount_ok and progress_ok
    instrumentation = bool(driver and fusecount_ok and progress_ok and phases and c1_complete
                           and (extent_complete in (True, None)))
    if not functional:
        status = "FAIL"
    elif args.diagnostic and prepared["arm"] == "candidate" and selection["count"] > 0 \
            and pack_source["status"] != "PASS":
        status = "INCOMPLETE"
    elif not limit_met:
        status = "FAIL"
    elif not instrumentation:
        status = "INCOMPLETE"
    elif selection["master"] == "retained" and retained_reproof is None:
        status = "INCOMPLETE"
    else:
        status = "INELIGIBLE"
    receipt = {
        "schema": "issue273-checkpoint5-attempt-v5",
        "family_id": "workspace_mounted_checkpoint", "arm": prepared["arm"],
        "registered_selection": selection["order"] != 0,
        "scenario_id": selection["scenario_id"], "scenario_version": 1,
        "mode": "causal_diagnostic" if args.diagnostic else "performance",
        "selection_order": selection["order"], "sample_count": 1,
        "admission_eligible": False,
        "operation_contract_id": "issue273-mounted-checkpoint-v1",
        "operation_surface": "public WorkspaceApi mount/exec/commit",
        "operation_entrypoint": "layerfs_sdk::WorkspaceApi::{mount,exec,commit}",
        "orchestration_executor": "/bin/sh -c in the sandbox daemon",
        "mutation_executor": "ordinary POSIX write/pwrite via FUSE",
        "implementation_route_status": "PASS" if driver else "UNKNOWN",
        "forbidden_route_count": 0, "fallback_count": 0,
        "sdk_edit_member_count": 0,
        "timing_boundary_id": "issue273-mount-exec-commit-v1", "clock_id": "monotonic",
        "source": identity, "treatment": "issue273-active-backing-v1",
        "spec_sha256": prepared["spec_sha256"], "harness_seal": identity["harness_seal"],
        "workload_seal": identity["workload_seal"],
        "product_seal": identity["product_seal"],
        "product_source_seal": identity["product_source_seal"],
        "writer_source_sha256": prepared["writer_source_sha256"],
        "writer_binary_sha256": prepared["writer_binary_sha256"],
        "oracle_source_sha256": prepared["oracle_source_sha256"],
        "binaries": prepared["binaries"], "builds": prepared["builds"],
        "build_mode": prepared["build_mode"], "construction_workers": WORKERS,
        "image_id": image_id, "images": prepared["images"],
        "master": master, "clone": clone, "clone_method": prepared["clone_method"],
        "shell_command": selection["command"], "writes_expected": selection["writes"],
        "command": command, "complete_command_limit_s": selection["limit_s"],
        "complete_command_wall_ns": measured["complete_command_wall_ns"],
        "complete_command_limit_met": limit_met,
        "driver_exit_code": measured["exit_code"], "driver_timeout": measured["timeout"],
        "driver": driver, "projection_counts": counts,
        "fuse_write_callbacks_observed": int(counts["write"]) if "write" in counts else None,
        "fuse_write_count_provenance": "Status write class; this command makes only data writes",
        "exec_process_count": 1, "shell_process_count": 1,
        "public_api_call_count": 4, "mount_orchestration_status": "PASS" if driver else "UNKNOWN",
        "writer_progress": checkpoints, "writer_progress_error": progress_error,
        "backing_samples": samples, "backing_sample_counts_expected": expected_samples,
        "pack_source": pack_source,
        "pack_source_availability": ("production_candidate_v1" if prepared["arm"] ==
                                     "candidate" else "baseline_not_instrumented"),
        "phase_samples_complete": phases,
        "extent_diagnostics": extent,
        "extent_splice_lines": splice_lines, "extent_splice_curve": splices,
        "extent_counters_emitted_by_this_arm": extent_present,
        "extent_counts_complete": extent_complete,
        "c1_counters_complete": c1_complete,
        "c1_counter_provenance": c1["provenance"],
        "c1_work_observed": c1["c1_work"],
        "writer_progress_ok": progress_ok,
        "fuse_write_count_ok": fusecount_ok, "counts_ok": counts_ok,
        "instrumentation_complete": instrumentation,
        "complexity_lines": [line for line in stderr.decode(errors="replace").splitlines()
                             if line.startswith(("LFS_C1_EDIT_LOAD", "LFS_FILE_INPUT",
                                                 "LFS_C1_SAVE_COUNT", "LFS_PIECE_LOWER",
                                                 "LFS_EXTENT_EDGE", "LFS_METADATA_OWNER"))],
        "lft1": lft1(stderr),
        "store_bytes_before": Path(clone["store"]).stat().st_size,
        "store_bytes_after": Path(clone["store"]).stat().st_size,
        "history_bytes_before": Path(clone["history"]).stat().st_size,
        "history_bytes_after": Path(clone["history"]).stat().st_size,
        "clone_physical_bytes": sum(
            Path(clone[name]).stat().st_blocks * 512 for name in ("store", "history")),
        "charged_backing_bytes": max(
            (row["backing"]["allocated_bytes"] + row["metadata"]["allocated_bytes"]
             for row in samples), default=None),
        "charged_backing_bytes_source": ("max over LFS_WRITE_SAMPLE allocated_bytes of the "
                                         "backing and metadata hosts" if samples else None),
        "charged_backing_bytes_reason": (None if samples else
                                         "this command emitted no WRITE_SAMPLE checkpoint"),
        "quota_charge_bytes": driver.get("consumer_accounted_bytes") if driver else None,
        "cache": cache, "cache_contract": prepared["cache_contract"],
        "cache_status": cache["status"],
        "commit_cache_status": "INELIGIBLE",
        "commit_cache_reason": "Linux private files request O_DIRECT in both arms; this is "
                               "source evidence of requested data-cache bypass, not runtime, "
                               "metadata, VM, backend, device or host-cache proof",
        "retained_reproof": retained_reproof,
        "verification": verification,
        "verification_status": verification["oracle"]["status"],
        "cleanup_status": "PASS" if cleanup else "FAIL",
        "functional_status": ("UNVERIFIED" if args.diagnostic and functional else
                              "PASS" if functional else "FAIL"),
        "resource_status": "PARTIAL",
        "custody_status": "PASS", "row_status": status,
        "performance_status": "INELIGIBLE",
        "failure_class": None if functional else "correctness_or_cleanup_failure",
        "container_cgroup_memory": None,
        "container_cgroup_memory_reason": "the driver does not export cgroup domains",
    }
    json_file(output / "receipt.json", receipt)
    (output / "SHA256SUMS").write_text("\n".join(
        f"{digest(path)}  {path.name}" for path in sorted(output.iterdir())
        if path.is_file() and path.name != "SHA256SUMS") + "\n")
    print(json.dumps({"receipt": str(output / "receipt.json"), "row_status": status,
                      "functional_status": receipt["functional_status"],
                      "complete_command_wall_ns": measured["complete_command_wall_ns"]}))


def space_metrics(receipt):
    """Two declared scopes, never summed.

    `backing.allocated_bytes` is the whole private-backing physical charge; the
    product's own `active-4096-separated-full-private-backing-bound` test
    equality-asserts it against the recursive `st_blocks * 512` sum of the
    private directory. `metadata.allocated_bytes` is the metadata-host pages
    *inside* that same charge, so adding the two double-counts them.
    """
    samples = receipt.get("backing_samples") or []
    return {
        "private_backing_allocated_bytes": max(
            (row["backing"]["allocated_bytes"] for row in samples), default=None),
        "metadata_allocated_bytes_subscope": max(
            (row["metadata"]["allocated_bytes"] for row in samples), default=None),
        "private_backing_source": ("LFS_WRITE_SAMPLE backing.allocated_bytes, max over the "
                                   "declared class checkpoints" if samples else None),
        "summing_scopes_would_double_count": True,
    }


def numeric_pair_eligible(control, candidate, left, right):
    """A visible pair is not a speed ratio without two admitted cache proofs."""
    return bool(left is not None and right is not None and left > 0 and right > 0
                and control and candidate
                and control.get("row_status") == candidate.get("row_status") == "PASS"
                and control.get("numeric_admission") is True
                and candidate.get("numeric_admission") is True
                and control.get("cache_status") == candidate.get("cache_status") == "PASS")


def report(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for receipt_file in sorted(Path(args.root).glob("*/[0-9][0-9]-*/receipt.json")):
        receipt = json.loads(receipt_file.read_text())
        driver = receipt["driver"] or {}
        rows.append({
            "arm": receipt["arm"], "order": receipt["selection_order"],
            "scenario_id": receipt["scenario_id"], "receipt": str(receipt_file),
            "row_status": receipt["row_status"],
            "numeric_admission": receipt.get("admission_eligible") is True,
            "functional_status": receipt["functional_status"],
            "oracle_status": receipt["verification"]["oracle"]["status"],
            "arm_verifier_status": receipt["verification"]["arm"]["status"],
            "complete_command_wall_ns": receipt["complete_command_wall_ns"],
            "complete_command_limit_s": receipt["complete_command_limit_s"],
            "limit_met": receipt["complete_command_limit_met"],
            "exec_ns": driver.get("exec_ns"), "commit_ns": driver.get("commit_ns"),
            "mount_ns": driver.get("mount_ns"), "cleanup_ns": driver.get("cleanup_ns"),
            "driver_status": driver.get("status"), "driver_detail": driver.get("detail"),
            "fuse_write_count": receipt["fuse_write_callbacks_observed"],
            "writes_expected": receipt["writes_expected"],
            "cache_status": receipt["cache_status"],
            "commit_cache_status": receipt["commit_cache_status"],
            "instrumentation_complete": receipt["instrumentation_complete"],
            "writer_progress_ok": receipt["writer_progress_ok"],
            "phase_samples_complete": receipt["phase_samples_complete"],
            "c1_counters_complete": receipt["c1_counters_complete"],
            "extent_counters_emitted_by_this_arm": receipt["extent_counters_emitted_by_this_arm"],
            "extent_counts_complete": receipt["extent_counts_complete"],
            "quota_charge_bytes": receipt["quota_charge_bytes"],
            "clone_physical_bytes": receipt["clone_physical_bytes"],
            "cleanup_status": receipt["cleanup_status"],
            "source_commit": receipt["source"]["source_commit"],
            "product_source_seal": receipt["source"]["product_source_seal"],
            "harness_seal": receipt["source"]["harness_seal"],
            **space_metrics(receipt),
        })
    pairs = []
    for selection in SELECTIONS:
        control = next((row for row in rows if row["order"] == selection["order"]
                        and row["arm"] == "control"), None)
        candidate = next((row for row in rows if row["order"] == selection["order"]
                          and row["arm"] == "candidate"), None)
        entry = {"order": selection["order"], "scenario_id": selection["scenario_id"],
                 "limit_s": selection["limit_s"], "writes": selection["writes"],
                 "control_row_status": control["row_status"] if control else "NOT_RUN",
                 "candidate_row_status": candidate["row_status"] if candidate else "NOT_RUN",
                 "cache_status": {"control": control["cache_status"] if control else None,
                                  "candidate": candidate["cache_status"] if candidate else None}}
        metrics = (("exec_ns", "exec"), ("commit_ns", "commit"),
                   ("complete_command_wall_ns", "complete_command"),
                   ("private_backing_allocated_bytes", "charged_backing"))
        for field, name in metrics:
            left = control.get(field) if control else None
            right = candidate.get(field) if candidate else None
            paired = left is not None and right is not None
            eligible = numeric_pair_eligible(control, candidate, left, right)
            entry[name] = {
                "control": left, "candidate": right,
                "ratio_control_over_candidate": round(left / right, 6) if eligible else None,
                "paired": paired,
                "numeric_eligibility": "ELIGIBLE" if eligible else "INELIGIBLE",
                "reason": ("matched cache-qualified PASS rows" if eligible else
                           "missing matched cache-qualified PASS rows; O_DIRECT request is "
                           "not a complete private/runtime/host cache-state proof"),
            }
        pairs.append(entry)
    route = None
    if args.route:
        route = json.loads(args.route.read_text())
    document = {"schema": "issue273-checkpoint5-report-v2", "rows": rows, "pairs": pairs,
                "registered_selections": [row["scenario_id"] for row in SELECTIONS],
                "route_evidence": route,
                "report_generator_sha256": digest(Path(__file__)),
                "admission_eligible": False,
                "space_scope_note": "private_backing_allocated_bytes and "
                                    "metadata_allocated_bytes_subscope are nested; do not sum them",
                "note": "Raw append-only receipts; no row is admission eligible and no "
                        "historical timing is used as a denominator."}
    json_file(output / "report.json", document)
    table = ["| # | selection | limit | control wall (s) | candidate wall (s) | control | candidate |",
             "| ---: | --- | ---: | ---: | ---: | --- | --- |"]
    for entry in pairs:
        wall = entry["complete_command"]
        left = "n/a" if wall["control"] is None else f"{wall['control'] / 1e9:.3f}"
        right = "n/a" if wall["candidate"] is None else f"{wall['candidate'] / 1e9:.3f}"
        table.append(f"| {entry['order']} | `{entry['scenario_id']}` | {entry['limit_s']} s | "
                     f"{left} | {right} | {entry['control_row_status']} | "
                     f"{entry['candidate_row_status']} |")
    (output / "REPORT.md").write_text("\n".join(table) + "\n")
    (output / "SHA256SUMS").write_text("\n".join(
        f"{digest(path)}  {path.name}" for path in sorted(output.iterdir())
        if path.is_file() and path.name != "SHA256SUMS") + "\n")
    print(json.dumps({"report": str(output / "report.json"), "rows": len(rows),
                      "pairs": len(pairs)}))


def campaign(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    control = json.loads(args.control_prepared.read_text())
    candidate = json.loads(args.candidate_prepared.read_text())
    campaign_rows = []
    for selection in SELECTIONS:
        for arm, prepared_file in (("control", args.control_prepared),
                                   ("candidate", args.candidate_prepared)):
            folder = root / f"{arm}/{selection['order']:02d}-{selection['scenario_id']}"
            if folder.exists():
                raise ValueError(f"attempt path already exists: {folder}")
            folder.parent.mkdir(parents=True, exist_ok=True)
            if selection["master"] == "retained":
                folder.mkdir()
                json_file(folder / "blocker.json", {
                    "schema": "issue273-retained-unsupported-v1", "row_status": "NOT_RUN",
                    "arm": arm, "scenario_id": selection["scenario_id"],
                    "reason": "SDK mount/Exec/Commit cannot preserve a live pinned "
                              "4097-record private journal across this control; closed "
                              "Store reattachment is not equivalent"})
                (folder / "SHA256SUMS").write_text(
                    f"{digest(folder / 'blocker.json')}  blocker.json\n")
                campaign_rows.append({"arm": arm, "order": selection["order"],
                                      "scenario_id": selection["scenario_id"],
                                      "row_status": "NOT_RUN", "harness_exit_code": None,
                                      "blocker": str(folder / "blocker.json")})
                print(json.dumps(campaign_rows[-1]), flush=True)
                continue
            command = [sys.executable, str(Path(__file__).resolve()), "run",
                       "--prepared", str(prepared_file), "--output", str(folder),
                       "--selection", selection["scenario_id"]]
            if selection["master"] == "retained":
                command += ["--retained", str(args.retained)]
            result = subprocess.run(command, cwd=OWNER, capture_output=True,
                                    env={**os.environ, "LAYERFS_CHECKPOINT5_LOCK_HELD": "1"})
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "harness.stdout").write_bytes(result.stdout)
            (folder / "harness.stderr").write_bytes(result.stderr)
            receipt = json.loads((folder / "receipt.json").read_text()) \
                if (folder / "receipt.json").exists() else None
            if receipt:
                (folder / "SHA256SUMS").write_text("\n".join(
                    f"{digest(path)}  {path.name}" for path in sorted(folder.iterdir())
                    if path.is_file() and path.name != "SHA256SUMS") + "\n")
            campaign_rows.append({"arm": arm, "order": selection["order"],
                                  "scenario_id": selection["scenario_id"],
                                  "row_status": receipt["row_status"] if receipt else "NOT_RUN",
                                  "harness_exit_code": result.returncode})
            print(json.dumps(campaign_rows[-1]), flush=True)
    json_file(root / "campaign.json", {"schema": "issue273-checkpoint5-campaign-v2",
                                       "rows": campaign_rows,
                                       "control_source": control["source"],
                                       "candidate_source": candidate["source"]})
    print(json.dumps({"campaign": str(root / "campaign.json")}))


def self_check():
    positions = [(104729 + i * 2654435761) % SIZE for i in range(4097)]
    assert len(set(positions)) == 4097
    assert all(0 < position < SIZE - 1 for position in positions)
    assert 0 not in set(positions) and 5 << 20 not in set(positions)
    for name, frozen in MANIFEST_FREEZE.items():
        content = {"old": b"A" * SIZE, "gate-old": b"A" * GATE_SIZE}[name] if name in (
            "old", "gate-old") else None
        if content is None:
            if name == "gate-new":
                content = gate_new(4097)
            else:
                content = matrix(name, 100)
        assert sha(files_manifest(content).encode()) == frozen, name
    assert len(SELECTIONS) == 12 and [row["order"] for row in SELECTIONS] == list(range(1, 13))
    assert len({row["scenario_id"] for row in SELECTIONS}) == 12
    assert [row["limit_s"] for row in SELECTIONS] == [15, 15, 25, 15, 15, 25, 15, 15, 25, 15, 15, 25]
    assert [row["master"] for row in SELECTIONS] == ["patterns"] * 9 + ["retained"] * 2 + ["gate"]
    assert matrix("append", 512)[SIZE:] == bytes(ord("B") + i % 24 for i in range(512))
    assert len(matrix("append", 4097)) == SIZE + 4097
    assert matrix("repeated", 4097)[5 << 20] == ord("B") + 4096 % 24
    retained = matrix("dispersed", 4097)
    assert retained[0] == ord("A") and len(retained) == SIZE
    print(json.dumps({"spec_sha256": digest(SPEC), "writer_sha256": digest(WRITER),
                      "oracle_source_sha256": digest(ORACLE_SOURCE),
                      "harness_sha256": digest(Path(__file__)),
                      "selections": [row["scenario_id"] for row in SELECTIONS],
                      "manifests": MANIFEST_FREEZE}, indent=1))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("self-check")
    p = sub.add_parser("oracle")
    p.add_argument("--repo", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p = sub.add_parser("prepare")
    p.add_argument("--arm", required=True, choices=("control", "candidate"))
    p.add_argument("--repo", required=True, type=Path)
    p.add_argument("--oracle", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p = sub.add_parser("retained")
    p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p = sub.add_parser("run")
    p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--selection", required=True,
                   choices=[row["scenario_id"] for row in SELECTIONS]
                   + [PREFLIGHT["scenario_id"]])
    p.add_argument("--retained", type=Path)
    p.add_argument("--diagnostic", action="store_true", help="skip independent verifiers; never admission eligible")
    p = sub.add_parser("campaign")
    p.add_argument("--control-prepared", required=True, type=Path)
    p.add_argument("--candidate-prepared", required=True, type=Path)
    p.add_argument("--retained", type=Path,
                   help="archived parameter; invalid live-journal controls are NOT_RUN")
    p.add_argument("--output", required=True, type=Path)
    p = sub.add_parser("report")
    p.add_argument("--root", required=True, type=Path)
    p.add_argument("--route", type=Path)
    p.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.action == "self-check":
        self_check()
        return
    # The campaign parent holds the per-worktree measurement lock and passes
    # that fact to its own child attempts; no other caller may skip it.
    if os.environ.get("LAYERFS_CHECKPOINT5_LOCK_HELD") == "1":
        {"oracle": oracle, "prepare": prepare, "retained": retained, "run": run,
         "campaign": campaign, "report": report}[args.action](args)
        return
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    with LOCK.open("a+b") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        {"oracle": oracle, "prepare": prepare, "retained": retained, "run": run,
         "campaign": campaign, "report": report}[args.action](args)


if __name__ == "__main__":
    main()
