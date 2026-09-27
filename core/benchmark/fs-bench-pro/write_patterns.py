#!/usr/bin/env python3
"""Three append-only public mounted-write diagnostics over one 10 MiB master."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from shell_package import (BASE, CORE, ROOT, case_spec, digest, identities,
                           json_file, lft1, manifest, one_receipt, run_command,
                           sha, write_tree)
from separated_writes import archive_binary, backing_samples, progress

HERE = Path(__file__).resolve().parent
SPEC = ROOT / "docs/roadmap/0.1/0.1.7/issue261-three-pattern-100-spec.md"
TREATMENT = ROOT / "docs/roadmap/0.1/0.1.7/issue261-ownership-io-treatment.md"
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


def compatible_previous(path, current):
    prior = json.loads(path.read_text())
    old = prior["source"]
    changed_core = subprocess.check_output(["git", "diff", "--name-only",
        old["source_commit"], current["source_commit"], "--", "core/crates",
        "core/Cargo.toml", "core/Cargo.lock", ".cargo/config.toml"],
        cwd=ROOT, text=True).splitlines()
    if (old["build_profile"] != "release" or
            set(changed_core) != {"core/crates/layerfs-server/examples/verify_shell.rs"}):
        raise ValueError(f"unreviewed release compilation changes: {changed_core}")
    inputs = ("core/crates/layerfs-api/sdk/examples/benchmark_init.rs",
              "core/crates/layerfs-api/sdk/examples/benchmark_shell.rs",
              "core/Cargo.lock", ".cargo/config.toml")
    if subprocess.run(["git", "diff", "--quiet", old["source_commit"],
                       current["source_commit"], "--", *inputs], cwd=ROOT).returncode:
        raise ValueError("reused SDK compilation inputs changed")
    for name in ("benchmark_init", "benchmark_shell"):
        if digest(prior["binaries"][name]["path"]) != prior["binaries"][name]["sha256"]:
            raise ValueError(f"prior {name} archive changed")
    daemon = path.parent / "image-context/layerfs-daemon"
    if digest(daemon) != prior["daemon_sha256"]:
        raise ValueError("prior daemon seal changed")
    return prior, daemon


def fields(master, pattern, command):
    case = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                        "genesis_root_serial", "branch_id", "old_commit")}
    case.update(scenario_id=f"issue261-{pattern}-100-10m-v1",
                command_hex=command.encode().hex(), expected_failure="0",
                write_pattern=pattern, pattern_count=str(COUNT),
                telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    return case


def prepare(output, prior_file):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = identities()
    if source["source_dirty"]:
        raise ValueError(f"committed source required: {source['dirty_paths']}")
    prior, prior_daemon = compatible_previous(prior_file, source)
    fixture = output / "fixture"
    write_tree(fixture, OLD)
    (output / "old.tsv").write_text(manifest(OLD))
    for pattern in PATTERNS:
        (output / f"{pattern}.tsv").write_text(manifest(expected(pattern)))
    context = output / "image-context"
    (context / "bin").mkdir(parents=True)
    archive = output / "binary-archive"
    binaries = {}
    for name in ("benchmark_init", "benchmark_shell"):
        binaries[name] = archive_binary(prior["binaries"][name]["path"], archive, name)
    build = ["cargo", "+1.85.1", "build", "--release", "--manifest-path",
             "core/Cargo.toml", "--locked", "-p", "layerfs-server",
             "--example", "verify_shell"]
    result, build_wall = run_command(build, output / "build-verifier")
    json_file(output / "build-verifier.json", {"command": build,
                                               "wall_ns": build_wall,
                                               "exit_code": result.returncode})
    if result.returncode:
        raise RuntimeError("release verifier build failed; retained logs")
    binaries["verify_shell"] = archive_binary(
        CORE / "target/release/examples/verify_shell", archive, "verify_shell")
    writer_build = ["zig", "cc", "-target", "aarch64-linux-musl", "-O3", "-static",
                    str(WRITER), "-o", str(context / "bin/write-separated")]
    result, writer_wall = run_command(writer_build, output / "build-writer")
    json_file(output / "build-writer.json", {"command": writer_build,
                                             "wall_ns": writer_wall,
                                             "exit_code": result.returncode})
    if result.returncode:
        raise RuntimeError("static writer build failed; retained logs")
    shutil.copyfile(prior_daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o555)
    (context / "bin/write-separated").chmod(0o555)
    dockerfile = (f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\n"
                  "COPY bin /fixtures/bin\n"
                  "ENV LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL=25\n"
                  "ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\n"
                  'ENTRYPOINT ["/layerfs-daemon"]\n')
    (context / "Dockerfile").write_text(dockerfile)
    image, image_wall = run_command(["docker", "build", "-q", str(context)],
                                    output / "image-build")
    if image.returncode:
        raise RuntimeError("diagnostic image build failed; retained logs")
    image_id = image.stdout.decode().strip()
    key = os.urandom(32).hex()
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key,
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    master = output / "master"
    master.mkdir()
    init = [binaries["benchmark_init"]["path"], str(fixture),
            str(master / "store.sqlite"), str(master / "history.sqlite"),
            "issue261-patterns"]
    created, init_wall = run_command(init, master / "init", timeout=30, env=env)
    if created.returncode:
        raise RuntimeError("master Init failed; retained logs")
    genesis = json.loads(created.stdout.splitlines()[-1])
    if genesis["status"] != "COMPLETE":
        raise RuntimeError("master Init incomplete")
    seed = {"scenario_id": "issue261-pattern-seed", "project_id": genesis["project_id"],
            "genesis_layer": genesis["genesis_layer"], "genesis_root": genesis["root"],
            "genesis_root_serial": genesis["root_serial"],
            "branch_body": hashlib.sha256(b"issue261-patterns").digest()[:16].hex(),
            "command_hex": b"printf A | dd of=data.bin bs=1 seek=0 conv=notrunc 2>/dev/null".hex(),
            "expected_failure": "0",
            "telemetry_run": str(int.from_bytes(os.urandom(16), "big") or 1)}
    case_spec(master / "seed.case", seed)
    command = [binaries["benchmark_shell"]["path"], "seed", str(master / "seed.case"),
               str(master / "store.sqlite"), str(master / "history.sqlite"), image_id]
    seeded, seed_wall = run_command(command, master / "seed", timeout=25, env=env)
    if seeded.returncode:
        raise RuntimeError("master seed failed; retained logs")
    receipt = one_receipt(seeded.stdout)
    if (receipt["status"] != "COMPLETE" or not receipt["head_commit"]
            or not receipt["unmount_ok"] or not receipt["sandbox_delete_ok"]):
        raise RuntimeError("master seed incomplete")
    seed.update(branch_id=receipt["branch_id"], old_commit=receipt["head_commit"],
                expected_failure="1")
    case_spec(master / "verify.case", seed)
    command = [binaries["verify_shell"]["path"], str(master / "verify.case"),
               str(master / "store.sqlite"), str(master / "history.sqlite"),
               str(output / "old.tsv"), str(output / "old.tsv")]
    checked, verify_wall = run_command(command, master / "verify", timeout=9, env=env)
    if checked.returncode or json.loads(checked.stdout.splitlines()[-1])["status"] != "PASS":
        raise RuntimeError("master old-head proof failed; retained logs")
    prepared_master = {name: seed[name] for name in ("project_id", "genesis_layer",
        "genesis_root", "genesis_root_serial", "branch_id", "old_commit")}
    prepared_master.update(path=str(master), store_sha256=digest(master / "store.sqlite"),
                           history_sha256=digest(master / "history.sqlite"),
                           proof_sha256=digest(master / "verify.stdout"))
    (master / "store.sqlite").chmod(0o444)
    (master / "history.sqlite").chmod(0o444)
    prepared = {"schema": "issue261-patterns-prepared-v1", "source": source,
        "spec_sha256": digest(SPEC), "writer_source_sha256": digest(WRITER),
        "writer_binary_sha256": digest(context / "bin/write-separated"),
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "dockerfile_sha256": sha(dockerfile.encode()), "image_id": image_id,
        "fixture_sha256": sha(OLD["data.bin"]),
        "manifest_sha256": {name: digest(output / f"{name}.tsv")
                            for name in ("old", *PATTERNS)},
        "cargo_config_sha256": digest(ROOT / ".cargo/config.toml"),
        "binaries": binaries, "cursor_key": key, "master": prepared_master,
        "clone_method": "shutil.copyfile independent writable byte copy",
        "cache_contract": "uncontrolled ordinary host/container cache; no prewarm; latency INELIGIBLE",
        "build_mode": "sealed release SDK/daemon reused; locked release verifier and Zig -O3 static writer built",
        "dependency_reuse": {"prior_prepared": str(prior_file),
                             "prior_source_commit": prior["source"]["source_commit"],
                             "prior_product_seal": prior["source"]["product_seal"],
                             "reviewed_compilation_change":
                                 "core/crates/layerfs-server/examples/verify_shell.rs"},
        "preparation_wall_ns": {"verifier_build": build_wall, "writer_build": writer_wall,
                                 "image": image_wall, "init": init_wall,
                                 "seed": seed_wall, "master_verify": verify_wall}}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"), "image_id": image_id}))


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
    allowed = {"core/crates/layerfs-workspace/src/backing/ownership.rs",
               "core/crates/layerfs-workspace/src/backing/metadata_reclaim.rs"}
    if (prior["source"]["build_profile"] != "release" or set(changed) != allowed
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
        str(Path(master["path"]) / "verify.case"),
        str(Path(master["path"]) / "store.sqlite"),
        str(Path(master["path"]) / "history.sqlite"),
        str(output / "old.tsv"), str(output / "old.tsv")]
    reproof, reproof_wall = run_command(reproof_command, output / "master-reproof",
                                         timeout=9, env=env)
    if (reproof.returncode or
            json.loads(reproof.stdout.splitlines()[-1])["status"] != "PASS"):
        raise RuntimeError("rebuilt verifier rejected old master; retained logs")
    prepared = {**prior, "source": source, "binaries": binaries,
        "treatment_spec_sha256": digest(TREATMENT),
        "writer_binary_sha256": digest(context / "bin/write-separated"),
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "dockerfile_sha256": sha(dockerfile.encode()),
        "image_id": image.stdout.decode().strip(),
        "master_reproof_sha256": digest(output / "master-reproof.stdout"),
        "build_mode": "locked release host SDK/verifier and aarch64 daemon rebuilt; sealed static writer reused",
        "dependency_reuse": {"prior_prepared": str(prior_file),
            "prior_source_commit": prior["source"]["source_commit"],
            "master": "closed Store/history, exact byte identity and rebuilt-verifier proof",
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
    if (prepared["schema"] != "issue261-patterns-prepared-v1" or source["source_dirty"]
            or any(source[key] != prepared["source"][key]
                   for key in ("source_commit", "source_tree", "product_seal", "harness_seal"))
            or digest(SPEC) != prepared["spec_sha256"]
            or ("treatment_spec_sha256" in prepared and
                digest(TREATMENT) != prepared["treatment_spec_sha256"])
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
    functional = bool(code == 0 and not timeout and driver and
                      driver.get("status") == "COMPLETE" and driver.get("commit_called")
                      and checkpoints and counts.get("write") == "100" and phases
                      and verification["status"] == "PASS" and cleanup)
    receipt = {"schema": "issue261-patterns-attempt-v1", "family_id": "workspace_mounted_write_patterns",
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
        "complexity_lines": [line for line in stderr.decode(errors="replace").splitlines()
                             if "LFS_PIECE_LOWER" in line or "LFS_C1_SAVE_COUNT" in line],
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
        "row_status": "INELIGIBLE" if functional else "FAIL", "sample_count": 1}
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
    p = sub.add_parser("prepare")
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--prior-prepared", required=True, type=Path)
    p = sub.add_parser("prepare-reuse")
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--prior-prepared", required=True, type=Path)
    p = sub.add_parser("run")
    p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--selection", required=True, choices=PATTERNS)
    args = parser.parse_args()
    if args.action == "self-check":
        positions = [(104729 + i * 2654435761) % SIZE for i in range(COUNT)]
        assert len(set(positions)) == COUNT and all(0 < p < SIZE - 1 for p in positions)
        assert all(abs(a - b) > 1 for i, a in enumerate(positions) for b in positions[i + 1:])
        assert [len(expected(p)["data.bin"]) for p in PATTERNS] == [SIZE + COUNT, SIZE, SIZE]
        print(json.dumps({"spec_sha256": digest(SPEC), "writer_sha256": digest(WRITER),
                          "patterns": PATTERNS, "size": SIZE, "writes": COUNT}))
    else:
        lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.action == "prepare":
                prepare(args.output, args.prior_prepared)
            elif args.action == "prepare-reuse":
                prepare_reuse(args.output, args.prior_prepared)
            else:
                run(args.prepared, args.output, args.selection)


if __name__ == "__main__":
    main()
