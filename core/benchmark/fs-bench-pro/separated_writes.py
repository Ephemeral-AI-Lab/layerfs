#!/usr/bin/env python3
"""One append-only public mounted-write diagnostic/gate for issues #261/#248."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from shell_package import (BASE, CORE, ROOT, case_spec, digest, identities,
                           json_file, lft1, manifest, one_receipt, run_command, sha,
                           write_tree)

HERE = Path(__file__).resolve().parent
SPEC = ROOT / "docs/roadmap/0.1/0.1.7/issue261-mounted-writes-spec.md"
SPEC_512 = ROOT / "docs/roadmap/0.1/0.1.7/issue261-512-diagnostic-spec.md"
SPEC_100_V2 = ROOT / "docs/roadmap/0.1/0.1.7/issue261-100-phase-diagnostic-v2.md"
SPEC_TREATMENT = ROOT / "docs/roadmap/0.1/0.1.7/issue261-100-ledger-treatment.md"
SPEC_266 = ROOT / "docs/roadmap/0.1/0.1.7/issue266-fuse-refusal-spec.md"
SPEC_266_POST = ROOT / "docs/roadmap/0.1/0.1.7/issue266-post-reply-diagnostic-spec.md"
SPEC_266_WAIT = ROOT / "docs/roadmap/0.1/0.1.7/issue266-reply-wait-treatment-spec.md"
WRITER = HERE / "writers/write-separated.c"
ORIGINAL = {"data.bin": b"A" * 8194}
COUNTS = {"diagnostic": 100, "diagnostic100v2": 100,
          "diagnostic100v3": 100, "diagnostic512": 512,
          "fuse512": 512, "gate": 4097}


def fields(master, name, command):
    value = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                           "genesis_root_serial", "branch_id", "old_commit")}
    prefix = {"gate": "issue248", "fuse512": "issue266"}.get(name, "issue261")
    version = {"diagnostic100v2": 2, "diagnostic100v3": 3}.get(name, 1)
    pattern = "fuse" if name == "fuse512" else "separated"
    value.update(scenario_id=f"{prefix}-{pattern}-{COUNTS[name]}-v{version}",
                 command_hex=command.encode().hex(), expected_failure="0",
                 separated_count=str(COUNTS[name]),
                 telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    return value


def expected(count):
    content = bytearray(ORIGINAL["data.bin"])
    content[0:2 * count:2] = b"X" * count
    return {"data.bin": bytes(content)}


def archive_binary(source, folder, name):
    value = digest(source)
    target = folder / value / name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(0o555)
    return {"path": str(target), "sha256": value}


def prepare(output):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    identity = identities()
    if identity["source_dirty"]:
        raise ValueError(f"prepare requires committed source: {identity['dirty_paths']}")
    fixture = output / "fixture"
    write_tree(fixture, ORIGINAL)
    (output / "old.tsv").write_text(manifest(ORIGINAL))
    for name, count in COUNTS.items():
        (output / f"{name}.tsv").write_text(manifest(expected(count)))
    context = output / "image-context"
    (context / "bin").mkdir(parents=True)
    commands = [
        ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml",
         "--locked", "-p", "layerfs-sdk", "--example", "benchmark_init",
         "--example", "benchmark_shell"],
        ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml",
         "--locked", "-p", "layerfs-server", "--example", "verify_shell"],
        ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path",
         "core/Cargo.toml", "--locked", "--offline", "--target",
         "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"],
        ["zig", "cc", "-target", "aarch64-linux-musl", "-O3", "-static", str(WRITER),
         "-o", str(context / "bin/write-separated")],
    ]
    for index, command in enumerate(commands):
        result, wall = run_command(command, output / f"build-{index}")
        json_file(output / f"build-{index}.json", {"command": command,
                  "wall_ns": wall, "exit_code": result.returncode})
        if result.returncode:
            raise RuntimeError(f"build-{index} failed; retained output")
    archive = output / "binary-archive"
    binaries = {name: archive_binary(source, archive, name) for name, source in {
        "benchmark_init": CORE / "target/release/examples/benchmark_init",
        "benchmark_shell": CORE / "target/release/examples/benchmark_shell",
        "verify_shell": CORE / "target/release/examples/verify_shell",
    }.items()}
    daemon = CORE / "target/aarch64-unknown-linux-musl/release/layerfs-daemon"
    shutil.copyfile(daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o755)
    (context / "bin/write-separated").chmod(0o755)
    dockerfile = (
        f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\n"
        "COPY bin /fixtures/bin\n"
        "ENTRYPOINT [\"/layerfs-daemon\"]\n")
    images = {}
    image_wall = {}
    dockerfiles = {}
    for name in COUNTS:
        interval = {"diagnostic": 25, "diagnostic100v2": 25,
                    "diagnostic100v3": 25, "diagnostic512": 128,
                    "fuse512": 128}.get(name)
        text = dockerfile.replace("ENTRYPOINT",
            f"ENV LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL={interval}\n"
            "ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\nENTRYPOINT") if interval else dockerfile
        (context / "Dockerfile").write_text(text)
        dockerfiles[name] = sha(text.encode())
        image, wall = run_command(["docker", "build", "-q", str(context)],
                                  output / f"image-build-{name}")
        if image.returncode:
            raise RuntimeError(f"{name} image build failed; retained output")
        images[name] = image.stdout.decode().strip()
        image_wall[name] = wall
    key = os.urandom(32).hex()
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key,
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    master = output / "master"
    master.mkdir()
    init_command = [binaries["benchmark_init"]["path"], str(fixture),
                    str(master / "store.sqlite"), str(master / "history.sqlite"),
                    "issue261-separated"]
    init, init_wall = run_command(init_command, master / "init", timeout=60, env=env)
    if init.returncode:
        raise RuntimeError("master init failed; retained output")
    created = json.loads(init.stdout.splitlines()[-1])
    if created["status"] != "COMPLETE":
        raise RuntimeError("master init incomplete")
    seed = {"scenario_id": "issue261-seed", "project_id": created["project_id"],
            "genesis_layer": created["genesis_layer"], "genesis_root": created["root"],
            "genesis_root_serial": created["root_serial"],
            "branch_body": hashlib.sha256(b"issue261-separated").digest()[:16].hex(),
            "command_hex": b"printf A | dd of=data.bin bs=1 seek=0 conv=notrunc 2>/dev/null".hex(),
            "expected_failure": "0",
            "telemetry_run": str(int.from_bytes(os.urandom(16), "big") or 1)}
    case_spec(master / "seed.case", seed)
    seeded, seed_wall = run_command([binaries["benchmark_shell"]["path"], "seed",
        str(master / "seed.case"), str(master / "store.sqlite"),
        str(master / "history.sqlite"), images["gate"]], master / "seed", timeout=25, env=env)
    if seeded.returncode:
        raise RuntimeError("master seed failed; retained output")
    seed_receipt = one_receipt(seeded.stdout)
    if seed_receipt["status"] != "COMPLETE" or not seed_receipt["head_commit"]:
        raise RuntimeError("master seed incomplete")
    seed.update(branch_id=seed_receipt["branch_id"],
                old_commit=seed_receipt["head_commit"], expected_failure="1")
    case_spec(master / "verify.case", seed)
    verified, verify_wall = run_command([binaries["verify_shell"]["path"],
        str(master / "verify.case"), str(master / "store.sqlite"),
        str(master / "history.sqlite"), str(output / "old.tsv"),
        str(output / "old.tsv")], master / "verify", timeout=9, env=env)
    if verified.returncode:
        raise RuntimeError("master verifier failed; retained output")
    prepared_master = {key: seed[key] for key in ("project_id", "genesis_layer",
        "genesis_root", "genesis_root_serial", "branch_id", "old_commit")}
    prepared_master.update(path=str(master), store_sha256=digest(master / "store.sqlite"),
                           history_sha256=digest(master / "history.sqlite"))
    (master / "store.sqlite").chmod(0o444)
    (master / "history.sqlite").chmod(0o444)
    prepared = {"schema": "issue261-separated-prepared-v1", "source": identity,
        "spec_sha256": digest(SPEC), "writer_source_sha256": digest(WRITER),
        "extension_spec_sha256": digest(SPEC_512),
        "phase_spec_sha256": digest(SPEC_100_V2),
        "treatment_spec_sha256": digest(SPEC_TREATMENT),
        "issue266_spec_sha256": digest(SPEC_266),
        "issue266_post_spec_sha256": digest(SPEC_266_POST),
        "issue266_wait_spec_sha256": digest(SPEC_266_WAIT),
        "writer_binary_sha256": digest(context / "bin/write-separated"),
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "dockerfile_sha256": dockerfiles,
        "fixture_sha256": sha(ORIGINAL["data.bin"]),
        "manifest_sha256": {name: digest(output / f"{name}.tsv")
                            for name in ("old", *COUNTS)},
        "binaries": binaries, "images": images, "cursor_key": key,
        "master": prepared_master, "clone_method": "shutil.copyfile independent writable byte copy",
        "cache_contract": "uncontrolled ordinary host/container cache; no prewarm; latency INELIGIBLE",
        "build_mode": "locked Cargo release, worktree-local target; Zig -O3 static writer",
        "preparation_wall_ns": {"init": init_wall, "seed": seed_wall,
                                 "verify": verify_wall, "image": image_wall}}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"),
                      "images": images, "master": str(master)}))


def prepare_reuse(output, previous_file, selection):
    """Add a declared diagnostic without repeating Init or seed."""
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    identity = identities()
    previous = json.loads(previous_file.read_text())
    if identity["source_dirty"] or previous["schema"] != "issue261-separated-prepared-v1":
        raise ValueError("clean source and a sealed issue261 master required")
    if previous["source"]["build_profile"] != "release":
        raise ValueError("previous binaries are not release builds")
    if selection not in ("diagnostic512", "diagnostic100v2", "diagnostic100v3", "fuse512", "gate"):
        raise ValueError("unsupported reuse selection")
    product_same = identity["product_seal"] == previous["source"]["product_seal"]
    changed_product = subprocess.check_output(["git", "diff", "--name-only",
        previous["source"]["source_commit"], identity["source_commit"], "--", "core/crates"],
        cwd=ROOT, text=True).splitlines()
    allowed_telemetry = {"core/crates/layerfs-fuse/src/adapter.rs",
                         "core/crates/layerfs-fuse/src/mount.rs",
                         "core/crates/layerfs-fuse/src/write_sample.rs"}
    allowed_ownership = {"core/crates/layerfs-workspace/src/backing/ownership.rs"}
    allowed_issue266 = {"core/crates/layerfs-workspace/src/runtime/coherence.rs",
                        "core/crates/layerfs-fuse/src/adapter.rs",
                        "core/crates/layerfs-fuse/src/write_sample.rs",
                        "core/crates/layerfs-daemon/src/run.rs"}
    allowed = (allowed_ownership if selection == "diagnostic100v3" else
               allowed_issue266 if selection in ("fuse512", "gate") else allowed_telemetry)
    if not product_same and (selection != "diagnostic100v2"
                             and selection != "diagnostic100v3"
                             and selection != "fuse512"
                             and selection != "gate"
                             or not changed_product or set(changed_product) - allowed):
        raise ValueError(f"unreviewed product changes since master preparation: {changed_product}")
    relevant = ["core/crates/layerfs-api/sdk/examples/benchmark_init.rs",
                "core/crates/layerfs-api/sdk/examples/benchmark_shell.rs",
                "core/crates/layerfs-server/examples/verify_shell.rs",
                "core/Cargo.lock", ".cargo/config.toml"]
    if subprocess.run(["git", "diff", "--quiet", previous["source"]["source_commit"],
                       identity["source_commit"], "--", *relevant], cwd=ROOT).returncode:
        raise ValueError("release binary compilation inputs changed")
    if digest(WRITER) != previous["writer_source_sha256"] or sha(ORIGINAL["data.bin"]) != previous["fixture_sha256"]:
        raise ValueError("writer or fixture changed")
    master = previous["master"]
    for name in ("store", "history"):
        if digest(Path(master["path"]) / f"{name}.sqlite") != master[f"{name}_sha256"]:
            raise ValueError(f"{name} master seal mismatch")
    master_proof = Path(master["path"]) / "verify.stdout"
    if json.loads(master_proof.read_text().splitlines()[-1])["status"] != "PASS":
        raise ValueError("prior master proof is not PASS")
    seed_proof = one_receipt((Path(master["path"]) / "seed.stdout").read_bytes())
    if seed_proof["status"] != "COMPLETE" or not seed_proof["unmount_ok"] or not seed_proof["sandbox_delete_ok"]:
        raise ValueError("prior seed cleanup is not complete")
    for binary in previous["binaries"].values():
        if digest(binary["path"]) != binary["sha256"]:
            raise ValueError("archived release binary seal mismatch")
    for name in ("old", "diagnostic", "gate"):
        source = previous_file.parent / f"{name}.tsv"
        if digest(source) != previous["manifest_sha256"][name]:
            raise ValueError(f"{name} manifest seal mismatch")
        shutil.copyfile(source, output / source.name)
    (output / f"{selection}.tsv").write_text(manifest(expected(COUNTS[selection])))
    context = output / "image-context"
    (context / "bin").mkdir(parents=True)
    old_context = previous_file.parent / "image-context"
    for source, target, expected_sha in (
        (old_context / "bin/write-separated", context / "bin/write-separated", previous["writer_binary_sha256"]),
    ):
        if digest(source) != expected_sha:
            raise ValueError(f"image binary seal mismatch: {source.name}")
        shutil.copyfile(source, target)
        target.chmod(0o755)
    if product_same:
        daemon = old_context / "layerfs-daemon"
        if digest(daemon) != previous["daemon_sha256"]:
            raise ValueError("daemon seal mismatch")
    else:
        command = ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path",
                   "core/Cargo.toml", "--locked", "--offline", "--target",
                   "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"]
        built, build_wall = run_command(command, output / "build-daemon")
        json_file(output / "build-daemon.json", {"command": command,
                  "wall_ns": build_wall, "exit_code": built.returncode})
        if built.returncode:
            raise RuntimeError("release daemon build failed; retained output")
        daemon = CORE / "target/aarch64-unknown-linux-musl/release/layerfs-daemon"
    shutil.copyfile(daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o755)
    interval = (128 if selection in ("diagnostic512", "fuse512") else
                25 if selection != "gate" else None)
    dockerfile = (
        f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\nCOPY bin /fixtures/bin\n"
        + (f"ENV LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL={interval}\n"
           "ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\n" if interval else "")
        + 'ENTRYPOINT ["/layerfs-daemon"]\n'
    )
    (context / "Dockerfile").write_text(dockerfile)
    image, wall = run_command(["docker", "build", "-q", str(context)],
                              output / f"image-build-{selection}")
    if image.returncode:
        raise RuntimeError(f"{selection} image build failed; retained output")
    prepared = {**previous, "source": identity,
        "spec_sha256": digest(SPEC), "extension_spec_sha256": digest(SPEC_512),
        "phase_spec_sha256": digest(SPEC_100_V2),
        "treatment_spec_sha256": digest(SPEC_TREATMENT),
        "issue266_spec_sha256": digest(SPEC_266),
        "issue266_post_spec_sha256": digest(SPEC_266_POST),
        "issue266_wait_spec_sha256": digest(SPEC_266_WAIT),
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "images": {**previous["images"], selection: image.stdout.decode().strip()},
        "dockerfile_sha256": {**previous["dockerfile_sha256"], selection: sha(dockerfile.encode())},
        "manifest_sha256": {**previous["manifest_sha256"],
                            selection: digest(output / f"{selection}.tsv")},
        "build_mode": "sealed SDK/verifier/writer and prepared master reused; daemon release rebuilt if product changed",
        "dependency_reuse": {"prepared": str(previous_file),
                             "master_proof_sha256": digest(master_proof),
                             "compatible_product_changes": changed_product,
                             "binary_sha256": {name: value["sha256"] for name, value in previous["binaries"].items()}},
        "preparation_wall_ns": {f"image_{selection}": wall}}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"),
                      "reused_master": master["path"],
                      "image_id": prepared["images"][selection]}))


def progress(driver, count):
    output = bytes.fromhex(driver["exec_stdout_hex"]).decode("ascii")
    rows = [line.split("\t") for line in output.splitlines()]
    expected_counts = [count // 4, count // 2, count * 3 // 4, count]
    if len(rows) != 4 or any(len(row) != 3 or row[0] != "PROGRESS"
                              for row in rows):
        raise ValueError(f"unexpected progress: {output!r}")
    parsed = [{"writes": int(row[1]), "writer_elapsed_ns": int(row[2])} for row in rows]
    if [row["writes"] for row in parsed] != expected_counts:
        raise ValueError("progress counts mismatch")
    if any(a["writer_elapsed_ns"] >= b["writer_elapsed_ns"]
           for a, b in zip(parsed, parsed[1:])):
        raise ValueError("non-increasing progress clock")
    return parsed


def backing_samples(stderr):
    rows = []
    pattern = re.compile(r"LFS_WRITE_SAMPLE v=(\d+) write_class=Some\((\d+)\) "
        r"elapsed_ns=(\d+) backing=Ok\(BackingStatus \{ (.*?) \}\) "
        r"metadata=Ok\(MetadataStatus \{ (.*?) \}\)"
        r"(?: acquisition_ns=(\d+) publication_ns=(\d+))?")
    for line in stderr.decode(errors="replace").splitlines():
        found = pattern.search(line)
        if found:
            def fields(value):
                return {key: int(raw) if raw.isdigit() else raw
                        for key, raw in (part.split(": ", 1)
                            for part in value.split(", "))}
            rows.append({"version": int(found[1]), "write_class": int(found[2]),
                         "elapsed_ns": int(found[3]), "backing": fields(found[4]),
                         "metadata": fields(found[5]),
                         "acquisition_ns": int(found[6]) if found[6] else None,
                         "publication_ns": int(found[7]) if found[7] else None})
    return rows


def run(prepared_file, output, selection):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    prepared = json.loads(prepared_file.read_text())
    current = identities()
    if current["source_dirty"] or any(current[key] != prepared["source"][key]
        for key in ("source_commit", "source_tree", "product_seal", "harness_seal")):
        raise ValueError("source changed since preparation")
    if digest(SPEC) != prepared["spec_sha256"] or digest(WRITER) != prepared["writer_source_sha256"]:
        raise ValueError("workload/spec changed")
    if selection == "diagnostic512" and digest(SPEC_512) != prepared.get("extension_spec_sha256"):
        raise ValueError("512 diagnostic specification changed")
    if selection == "diagnostic100v2" and digest(SPEC_100_V2) != prepared.get("phase_spec_sha256"):
        raise ValueError("100 phase diagnostic specification changed")
    if selection == "diagnostic100v3" and digest(SPEC_TREATMENT) != prepared.get("treatment_spec_sha256"):
        raise ValueError("100 ledger treatment specification changed")
    if selection in ("fuse512", "gate") and digest(SPEC_266) != prepared.get("issue266_spec_sha256"):
        raise ValueError("issue266 diagnostic specification changed")
    if selection == "fuse512" and digest(SPEC_266_POST) != prepared.get("issue266_post_spec_sha256"):
        raise ValueError("issue266 post-reply specification changed")
    if selection in ("fuse512", "gate") and digest(SPEC_266_WAIT) != prepared.get("issue266_wait_spec_sha256"):
        raise ValueError("issue266 reply wait specification changed")
    for binary in prepared["binaries"].values():
        if digest(binary["path"]) != binary["sha256"]:
            raise ValueError("binary seal mismatch")
    for image_id in prepared["images"].values():
        image = subprocess.check_output(["docker", "image", "inspect", image_id,
                                         "--format", "{{.Id}}"], text=True).strip()
        if image != image_id:
            raise ValueError("image identity changed")
    master = prepared["master"]
    clone = {}
    for name in ("store", "history"):
        source = Path(master["path"]) / f"{name}.sqlite"
        target = output / f"{name}.sqlite"
        if digest(source) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} master seal mismatch")
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if digest(target) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} clone seal mismatch")
        clone[name] = str(target)
    count = COUNTS[selection]
    store_bytes_before = Path(clone["store"]).stat().st_size
    history_bytes_before = Path(clone["history"]).stat().st_size
    shell = f"/fixtures/bin/write-separated data.bin {count}"
    before = fields(master, selection, shell)
    case_spec(output / "case.before", before)
    command = [prepared["binaries"]["benchmark_shell"]["path"], "run",
               str(output / "case.before"), clone["store"], clone["history"],
               prepared["images"][selection]]
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    if selection.startswith("diagnostic") or selection == "fuse512":
        env["LAYERFS_COMPLEXITY_DIAGNOSTIC"] = "1"
    limit = 25 if selection == "gate" else 15
    start = time.monotonic_ns()
    try:
        process = subprocess.run(command, cwd=ROOT, capture_output=True,
                                 timeout=limit, env=env)
        stdout, stderr, code, timed_out = process.stdout, process.stderr, process.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    complete_ns = time.monotonic_ns() - start
    (output / "driver.stdout").write_bytes(stdout)
    (output / "driver.stderr").write_bytes(stderr)
    try:
        driver = one_receipt(stdout)
    except (ValueError, json.JSONDecodeError):
        driver = None
    try:
        checkpoints = progress(driver, count) if driver else None
        progress_error = None
    except (ValueError, KeyError, UnicodeDecodeError) as error:
        checkpoints, progress_error = None, str(error)
    counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",")
                  if "=" in item) if driver else {}
    samples = backing_samples(stderr)
    sample_counts = [row["write_class"] for row in samples]
    expected_samples = ([count // 4, count // 2, count * 3 // 4, count]
                        if selection.startswith("diagnostic") or selection == "fuse512" else [])
    phase_samples_complete = selection not in ("diagnostic100v2", "diagnostic100v3", "fuse512") or all(
        row["version"] == 2 and row["acquisition_ns"] is not None
        and row["publication_ns"] is not None for row in samples)
    verification = {"status": "NOT_RUN"}
    if driver and driver.get("head_commit"):
        after = {**before, "expected_head_commit": driver["head_commit"]}
        case_spec(output / "case.verify", after)
        root = prepared_file.parent
        verify_command = [prepared["binaries"]["verify_shell"]["path"],
            str(output / "case.verify"), clone["store"], clone["history"],
            str(root / "old.tsv"), str(root / f"{selection}.tsv")]
        try:
            checked, wall = run_command(verify_command, output / "verifier", timeout=9, env=env)
            verification = {"status": "PASS" if checked.returncode == 0 else "FAIL",
                            "wall_ns": wall, "exit_code": checked.returncode,
                            "command": verify_command}
        except subprocess.TimeoutExpired:
            verification = {"status": "FAIL", "timeout": True,
                            "command": verify_command}
    daemon_log = stderr.decode(errors="replace")
    daemon_close = "sandbox closed\n" in daemon_log
    daemon_retained = any(term in daemon_log for term in
                          ("sandbox shutdown retained:", "workspace shutdown retained:"))
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                   and (selection not in ("fuse512", "gate") or (daemon_close and not daemon_retained
                        and driver.get("daemon_log_attempted")
                        and not driver.get("daemon_log_truncated")
                        and driver.get("daemon_log_error") == "None")))
    functional = bool(code == 0 and not timed_out and driver and
        driver.get("status") == "COMPLETE" and driver.get("commit_called") and
        checkpoints and sample_counts == expected_samples and phase_samples_complete and
        verification["status"] == "PASS" and cleanup)
    receipt = {"schema": "issue261-separated-attempt-v1", "selection": selection,
        "family_id": "workspace_mounted_separated_writes",
        "scenario_id": before["scenario_id"], "operation_surface": "public WorkspaceApi mount/exec/commit",
        "source": prepared["source"], "spec_sha256": prepared["spec_sha256"],
        "extension_spec_sha256": prepared.get("extension_spec_sha256"),
        "phase_spec_sha256": prepared.get("phase_spec_sha256"),
        "treatment_spec_sha256": prepared.get("treatment_spec_sha256"),
        "issue266_spec_sha256": prepared.get("issue266_spec_sha256"),
        "issue266_post_spec_sha256": prepared.get("issue266_post_spec_sha256"),
        "issue266_wait_spec_sha256": prepared.get("issue266_wait_spec_sha256"),
        "dependency_reuse": prepared.get("dependency_reuse"),
        "writer_source_sha256": prepared["writer_source_sha256"],
        "writer_binary_sha256": prepared["writer_binary_sha256"],
        "daemon_sha256": prepared["daemon_sha256"],
        "dockerfile_sha256": prepared["dockerfile_sha256"][selection],
        "image_id": prepared["images"][selection], "images": prepared["images"],
        "driver_binary_sha256": prepared["binaries"]["benchmark_shell"]["sha256"],
        "verifier_binary_sha256": prepared["binaries"]["verify_shell"]["sha256"],
        "master": master, "clone": clone, "clone_method": prepared["clone_method"],
        "fixture_sha256": prepared["fixture_sha256"],
        "expected_manifest_sha256": prepared["manifest_sha256"][selection],
        "command": command, "shell_command": shell, "write_syscalls_expected": count,
        "writer_progress": checkpoints, "writer_progress_error": progress_error,
        "backing_samples": samples, "backing_sample_counts_expected": expected_samples,
        "phase_samples_complete": phase_samples_complete,
        "complexity_lines": [line for line in stderr.decode(errors="replace").splitlines()
                             if "LFS_PIECE_LOWER" in line or "LFS_C1_SAVE_COUNT" in line],
        "lft1": lft1(stderr),
        "complete_command_wall_ns": complete_ns, "complete_command_limit_s": limit,
        "driver_exit_code": code, "driver_timeout": timed_out, "driver": driver,
        "projection_counts": counts, "fuse_write_callbacks_observed":
            int(counts["write"]) if "write" in counts else None,
        "fuse_write_count_provenance": "Status write class; this command makes only data mutations",
        "store_bytes_before": store_bytes_before,
        "store_bytes_after": Path(clone["store"]).stat().st_size,
        "history_bytes_before": history_bytes_before,
        "history_bytes_after": Path(clone["history"]).stat().st_size,
        "verifier": verification, "cleanup_status": "PASS" if cleanup else "FAIL",
        "daemon_close_observed": daemon_close,
        "daemon_retained_observed": daemon_retained,
        "write_refusals": [line for line in daemon_log.splitlines()
                           if "LFS_WRITE_REFUSAL" in line or "LFS_PROJECTION_REFUSAL" in line],
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
    p = sub.add_parser("prepare"); p.add_argument("--output", required=True, type=Path)
    p.add_argument("--reuse-prepared", type=Path)
    p.add_argument("--reuse-selection", choices=("diagnostic512", "diagnostic100v2", "diagnostic100v3", "fuse512", "gate"),
                   default="diagnostic512")
    p = sub.add_parser("run"); p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--selection", required=True, choices=COUNTS)
    args = parser.parse_args()
    if args.action == "self-check":
        assert len(ORIGINAL["data.bin"]) == 8194
        for count in COUNTS.values():
            content = expected(count)["data.bin"]
            assert len(content) == 8194 and content.count(b"X") == count
        fixture_line = (b"LFS_WRITE_SAMPLE v=1 write_class=Some(25) elapsed_ns=123 "
                        b"backing=Ok(BackingStatus { ledger_reads: 4, payloads: 25 }) "
                        b"metadata=Ok(MetadataStatus { allocated_pages: 3 })\n")
        assert backing_samples(fixture_line)[0]["backing"]["ledger_reads"] == 4
        phase_line = fixture_line.replace(b"v=1", b"v=2").strip() + b" acquisition_ns=7 publication_ns=11\n"
        assert backing_samples(phase_line)[0]["publication_ns"] == 11
        print(json.dumps({"spec_sha256": digest(SPEC), "writer_sha256": digest(WRITER),
                          "counts": COUNTS}))
    elif args.action == "prepare":
        lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.reuse_prepared:
                prepare_reuse(args.output, args.reuse_prepared, args.reuse_selection)
            else:
                prepare(args.output)
    else:
        lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            run(args.prepared, args.output, args.selection)


if __name__ == "__main__":
    main()
