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
SPEC_266_1024 = ROOT / "docs/roadmap/0.1/0.1.7/issue266-1024-count-diagnostic-spec.md"
SPEC_271_2048 = ROOT / "docs/roadmap/0.1/0.1.7/issue271-2048-diagnostic-spec.md"
SPEC_271_SPONSOR = ROOT / "docs/roadmap/0.1/0.1.7/issue271-sponsored-page-treatment-spec.md"
SPEC_271_ONE_READ = ROOT / "docs/roadmap/0.1/0.1.7/issue271-sponsor-one-read-spec.md"
SPEC_271_FOUR_HOP = ROOT / "docs/roadmap/0.1/0.1.7/issue271-four-hop-custody-spec.md"
SPEC_271_4097 = ROOT / "docs/roadmap/0.1/0.1.7/issue271-4097-extended-count-diagnostic-spec.md"
SPEC_271_CAUSE = ROOT / "docs/roadmap/0.1/0.1.7/issue271-causal-diagnostic-spec.md"
WRITER = HERE / "writers/write-separated.c"
ORIGINAL = {"data.bin": b"A" * 8194}
COUNTS = {"diagnostic": 100, "diagnostic100v2": 100,
          "diagnostic100v3": 100, "diagnostic512": 512,
          "fuse512": 512, "diagnostic1024": 1024,
          "diagnostic2048": 2048, "diagnostic4097": 4097,
          "diagnostic4097cause": 4097, "gate": 4097}
CAUSE_BACKING = (
    "acquisition_maintenance", "acquisition_metadata_maintenance",
    "acquisition_payload_maintenance", "payload_acquire",
    "publication_maintenance", "publication_metadata_maintenance",
    "publication_payload_maintenance", "publication_core", "checked_notifier")
CAUSE_METADATA = ("ledger_file", "metadata_page_create")
CAUSE_LOGS = ("LFS_FILE_STREAM_CAUSE", "LFS_COMMIT_SOURCE_CAUSE")
CAUSE_STREAM_FIELDS = ("request", "scope", "status", "declared_extents",
                       "descriptor_read_calls", "parsed_extents", "edits",
                       "edit_spool_write_calls", "edit_spool_write_bytes", "edit_spool_write_ns")
CAUSE_SOURCE_FIELDS = ("scope", "source_complete", "declared_extents", "final_length",
                       "replacement_bytes", "descriptor_source_calls", "descriptor_source_bytes",
                       "descriptor_source_ns", "descriptor_cursor_next_calls",
                       "descriptor_cursor_next_ns", "replacement_source_calls",
                       "replacement_source_bytes", "replacement_source_ns",
                       "replacement_cursor_next_calls", "replacement_cursor_next_ns",
                       "local_reader_create_calls", "local_reader_create_ns", "local_read_calls",
                       "local_read_bytes", "local_read_ns", "local_segment_open_calls",
                       "local_segment_open_ns", "local_aligned_read_calls",
                       "local_aligned_read_bytes", "local_aligned_read_ns")


def fields(master, name, command):
    value = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                           "genesis_root_serial", "branch_id", "old_commit")}
    prefix = {"gate": "issue248", "fuse512": "issue266",
              "diagnostic1024": "issue266", "diagnostic2048": "issue271",
              "diagnostic4097": "issue271", "diagnostic4097cause": "issue271"}.get(name, "issue261")
    version = {"diagnostic100v2": 2, "diagnostic100v3": 3}.get(name, 1)
    pattern = "fuse" if name == "fuse512" else "separated"
    scenario = ("issue271-separated-4097-cause-v1" if name == "diagnostic4097cause"
                else f"{prefix}-{pattern}-{COUNTS[name]}-v{version}")
    value.update(scenario_id=scenario,
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
                    "fuse512": 128, "diagnostic1024": 256,
                    "diagnostic2048": 512, "diagnostic4097": 512,
                    "diagnostic4097cause": 512}.get(name)
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
        "issue266_1024_spec_sha256": digest(SPEC_266_1024),
        "issue271_2048_spec_sha256": digest(SPEC_271_2048),
        "issue271_sponsor_spec_sha256": digest(SPEC_271_SPONSOR),
        "issue271_one_read_spec_sha256": digest(SPEC_271_ONE_READ),
        "issue271_four_hop_spec_sha256": digest(SPEC_271_FOUR_HOP),
        "issue271_4097_spec_sha256": digest(SPEC_271_4097),
        "issue271_cause_spec_sha256": digest(SPEC_271_CAUSE),
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
    if selection not in ("diagnostic512", "diagnostic100v2", "diagnostic100v3", "fuse512", "diagnostic1024", "diagnostic2048", "diagnostic4097", "diagnostic4097cause", "gate"):
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
                        "core/crates/layerfs-fuse/src/lib.rs",
                        "core/crates/layerfs-fuse/src/open_flags.rs",
                        "core/crates/layerfs-fuse/src/write_sample.rs",
                        "core/crates/layerfs-fuse/tests/kernel_write.rs",
                        "core/crates/layerfs-daemon/src/run.rs"}
    # This treatment changes only daemon-side extent packing and its external
    # test; the archived SDK/verifier have no Workspace package dependency.
    allowed_packing = {"core/crates/layerfs-workspace/src/backing/binary_plus_tree/extent/pack.rs",
                       "core/crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs",
                       "core/crates/layerfs-workspace/tests/pieces_sequence.rs"}
    allowed_sponsor = {
        *[f"core/crates/layerfs-workspace/src/backing/{name}" for name in (
            "binary_plus_tree/extent/cursor.rs", "binary_plus_tree/extent/telemetry.rs",
            "metadata.rs", "metadata_reclaim.rs", "ownership.rs", "ownership/sponsored.rs")],
        "core/crates/layerfs-workspace/tests/owner_finalization.rs",
    }
    allowed_cause = {
        "core/crates/layerfs-workspace/src/types.rs",
        "core/crates/layerfs-workspace/src/runtime/host.rs",
        "core/crates/layerfs-workspace/src/runtime/coherence.rs",
        "core/crates/layerfs-workspace/src/backing/payload.rs",
        "core/crates/layerfs-workspace/src/backing/reclaim.rs",
        "core/crates/layerfs-workspace/src/backing/ownership.rs",
        "core/crates/layerfs-workspace/src/backing/ownership/sponsored.rs",
        "core/crates/layerfs-workspace/src/backing/metadata.rs",
        "core/crates/layerfs-workspace/src/backing/metadata/telemetry.rs",
        "core/crates/layerfs-workspace/src/backing/reader.rs",
        "core/crates/layerfs-workspace/src/filesystem/write.rs",
        "core/crates/layerfs-workspace/src/commit/source.rs",
        "core/crates/layerfs-workspace/src/commit/upload.rs",
        "core/crates/layerfs-fuse/src/write_sample.rs",
        "core/crates/layerfs-server/src/service/save/content.rs",
        "core/crates/layerfs-server/src/service/save/file_stream.rs",
    }
    allowed = (allowed_ownership if selection == "diagnostic100v3" else
               allowed_issue266 if selection in ("fuse512", "diagnostic4097", "gate") else allowed_telemetry)
    if selection == "diagnostic4097cause":
        allowed = allowed_cause
    if selection in ("diagnostic100v3", "fuse512", "diagnostic4097", "gate"):
        allowed |= allowed_packing | allowed_sponsor
    if selection in ("diagnostic1024", "diagnostic2048"):
        allowed = allowed_packing
    if not product_same and (selection != "diagnostic100v2"
                             and selection != "diagnostic100v3"
                             and selection != "fuse512"
                             and selection != "diagnostic1024"
                             and selection != "diagnostic2048"
                             and selection != "diagnostic4097"
                             and selection != "diagnostic4097cause"
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
    binaries = previous["binaries"]
    host_build_wall = {}
    master_verify_sha = None
    if selection == "diagnostic4097cause":
        host_commands = [
            ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml",
             "--locked", "-p", "layerfs-sdk", "--example", "benchmark_init",
             "--example", "benchmark_shell"],
            ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml",
             "--locked", "-p", "layerfs-server", "--example", "verify_shell"],
        ]
        for index, command in enumerate(host_commands):
            built, wall = run_command(command, output / f"build-host-{index}")
            json_file(output / f"build-host-{index}.json", {"command": command,
                "wall_ns": wall, "budget_ns": 30_000_000_000,
                "status": "PASS" if built.returncode == 0 and wall <= 30_000_000_000
                          else "BUILD_SLOW" if built.returncode == 0 else "FAIL",
                "exit_code": built.returncode})
            if built.returncode:
                raise RuntimeError(f"build-host-{index} failed; retained output")
            host_build_wall[f"build_host_{index}"] = wall
        archive = output / "binary-archive"
        binaries = {name: archive_binary(source, archive, name) for name, source in {
            "benchmark_init": CORE / "target/release/examples/benchmark_init",
            "benchmark_shell": CORE / "target/release/examples/benchmark_shell",
            "verify_shell": CORE / "target/release/examples/verify_shell",
        }.items()}
        env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": previous["cursor_key"],
               "LAYERFS_CONSTRUCTION_WORKERS": "1"}
        master_case = Path(master["path"]) / "verify.case"
        verified, wall = run_retaining_timeout([binaries["verify_shell"]["path"], str(master_case),
            str(Path(master["path"]) / "store.sqlite"),
            str(Path(master["path"]) / "history.sqlite"),
            str(output / "old.tsv"), str(output / "old.tsv")],
            output / "master-verify-new", timeout=9, env=env)
        if verified.returncode or json.loads(verified.stdout.splitlines()[-1])["status"] != "PASS":
            raise RuntimeError("new release verifier rejected closed master")
        host_build_wall["master_verify_new"] = wall
        master_verify_sha = digest(output / "master-verify-new.stdout")
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
    interval = (512 if selection in ("diagnostic2048", "diagnostic4097", "diagnostic4097cause") else
                256 if selection == "diagnostic1024" else
                128 if selection in ("diagnostic512", "fuse512") else
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
        "issue266_1024_spec_sha256": digest(SPEC_266_1024),
        "issue271_2048_spec_sha256": digest(SPEC_271_2048),
        "issue271_sponsor_spec_sha256": digest(SPEC_271_SPONSOR),
        "issue271_one_read_spec_sha256": digest(SPEC_271_ONE_READ),
        "issue271_four_hop_spec_sha256": digest(SPEC_271_FOUR_HOP),
        "issue271_4097_spec_sha256": digest(SPEC_271_4097),
        "issue271_cause_spec_sha256": digest(SPEC_271_CAUSE),
        "binaries": binaries,
        "daemon_sha256": digest(context / "layerfs-daemon"),
        "images": {**previous["images"], selection: image.stdout.decode().strip()},
        "dockerfile_sha256": {**previous["dockerfile_sha256"], selection: sha(dockerfile.encode())},
        "manifest_sha256": {**previous["manifest_sha256"],
                            selection: digest(output / f"{selection}.tsv")},
        "build_mode": ("locked release SDK/verifier/daemon rebuilt; sealed writer and closed master reused"
                       if selection == "diagnostic4097cause" else
                       "sealed SDK/verifier/writer and prepared master reused; daemon release rebuilt if product changed"),
        "dependency_reuse": {"prepared": str(previous_file),
                             "master_proof_sha256": digest(master_proof),
                             "new_master_proof_sha256": master_verify_sha,
                             "compatible_product_changes": changed_product,
                             "previous_binary_sha256": {name: value["sha256"] for name, value in previous["binaries"].items()},
                             "binary_sha256": {name: value["sha256"] for name, value in binaries.items()}},
        "preparation_wall_ns": {f"image_{selection}": wall, **host_build_wall}}
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


def run_retaining_timeout(command, log, **kwargs):
    try:
        return run_command(command, log, **kwargs)
    except subprocess.TimeoutExpired as error:
        Path(str(log) + ".stdout").write_bytes(error.stdout or b"")
        Path(str(log) + ".stderr").write_bytes(error.stderr or b"")
        raise


def causal_diagnostics(stderr, samples):
    """Read one source-owned cause record per phase, preserving missing evidence."""
    lines = stderr.decode(errors="replace").splitlines()
    causes = {}
    errors = []
    for marker in CAUSE_LOGS:
        found = [line for line in lines if line.startswith(marker + " v=1 ")]
        if len(found) != 1:
            errors.append(f"{marker}: expected one record, got {len(found)}")
            continue
        try:
            pairs = [part.split("=", 1) for part in found[0].split()[1:]]
            if any(len(pair) != 2 or not pair[0] for pair in pairs):
                raise ValueError("malformed key/value")
            fields = dict(pairs)
            if len(fields) != len(pairs):
                raise ValueError("duplicate key")
            causes[marker] = fields
        except ValueError:
            errors.append(f"{marker}: malformed or duplicate key/value record")
    save = []
    for line in lines:
        if not line.startswith("LFT1 "):
            continue
        try:
            event = json.loads(line[5:])
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict):
            continue
        timing = event.get("timing")
        if not isinstance(timing, dict):
            continue
        if event.get("role") == 1 and event.get("kind") == "operation" and timing.get("name") == "SaveFile":
            children = timing.get("children")
            if isinstance(children, list):
                save.extend(child for child in children if isinstance(child, dict)
                            and child.get("name") == "service.pre_save_input")
    host_ns = save[0].get("elapsed_ns") if len(save) == 1 else None
    if not isinstance(host_ns, int) or host_ns <= 0:
        errors.append(f"service.pre_save_input: expected one positive host timing, got {len(save)}")
    if len(samples) != 8 or [row["write_class"] for row in samples] != list(range(512, 4097, 512)):
        errors.append("causal 512-WRITE checkpoints incomplete")
    previous = {}
    for row in samples:
        if row["version"] != 3:
            errors.append(f"WRITE {row['write_class']}: expected LFS_WRITE_SAMPLE v=3")
        for scope, names in (("backing", CAUSE_BACKING), ("metadata", CAUSE_METADATA)):
            fields = row[scope]
            if fields.get("operator_diagnostics_enabled") != "true":
                errors.append(f"WRITE {row['write_class']}: {scope} diagnostics disabled")
            for name in names:
                for suffix in ("calls", "ns"):
                    key = f"{name}_{suffix}"
                    value = fields.get(key)
                    if not isinstance(value, int) or value < previous.get((scope, key), 0):
                        errors.append(f"WRITE {row['write_class']}: missing/nonmonotone {scope}.{key}")
                    else:
                        previous[(scope, key)] = value
            if scope == "metadata":
                for key in ("sponsor_attempts", "sponsor_accepted", "sponsor_depth_fallbacks"):
                    value = fields.get(key)
                    if not isinstance(value, int) or value < previous.get((scope, key), 0):
                        errors.append(f"WRITE {row['write_class']}: missing/nonmonotone metadata.{key}")
                    else:
                        previous[(scope, key)] = value
        for name in ("acquisition_maintenance", "payload_acquire",
                     "publication_maintenance", "publication_core", "checked_notifier"):
            if row["backing"].get(f"{name}_calls") != row["write_class"]:
                errors.append(f"WRITE {row['write_class']}: {name} call count mismatch")
    stream = causes.get("LFS_FILE_STREAM_CAUSE", {})
    source = causes.get("LFS_COMMIT_SOURCE_CAUSE", {})
    for marker, fields, required in (("LFS_FILE_STREAM_CAUSE", stream, CAUSE_STREAM_FIELDS),
                                     ("LFS_COMMIT_SOURCE_CAUSE", source, CAUSE_SOURCE_FIELDS)):
        for key in required:
            value = fields.get(key)
            if value is None:
                errors.append(f"{marker}.{key}: missing")
            elif key not in ("scope", "status", "source_complete") and not value.isdigit():
                errors.append(f"{marker}.{key}: expected unsigned integer")
    for key, value in {"scope": "file_stream_read", "status": "ok",
                       "declared_extents": "8194", "parsed_extents": "8194",
                       "descriptor_read_calls": "8194", "edits": "4097",
                       "edit_spool_write_calls": "16388",
                       "edit_spool_write_bytes": "131104"}.items():
        if stream.get(key) != value:
            errors.append(f"LFS_FILE_STREAM_CAUSE.{key}: expected {value}, got {stream.get(key)}")
    for key, value in {"scope": "save_file", "source_complete": "true",
                       "declared_extents": "8194", "final_length": "8194",
                       "replacement_bytes": "4097", "descriptor_source_bytes": "196656",
                       "descriptor_cursor_next_calls": "8194",
                       "replacement_source_bytes": "4097",
                       "local_reader_create_calls": "4097", "local_read_calls": "4097",
                       "local_read_bytes": "4097",
                       "local_segment_open_calls": "4097",
                       "local_aligned_read_calls": "4097",
                       "local_aligned_read_bytes": "16781312"}.items():
        if source.get(key) != value:
            errors.append(f"LFS_COMMIT_SOURCE_CAUSE.{key}: expected {value}, got {source.get(key)}")
    for marker, fields, keys in (("LFS_FILE_STREAM_CAUSE", stream,
                                  ("edit_spool_write_ns",)),
                                 ("LFS_COMMIT_SOURCE_CAUSE", source,
                                  ("descriptor_source_ns", "replacement_source_ns",
                                   "local_segment_open_ns", "local_aligned_read_ns"))):
        for key in keys:
            if fields.get(key, "0").isdigit() and int(fields.get(key, "0")) == 0:
                errors.append(f"{marker}.{key}: empty time for nonempty work")
    return {"status": "PASS" if not errors else "INCOMPLETE", "errors": errors,
            "logs": causes, "host_pre_save_input_ns": host_ns,
            "bridge_body_frames_observed": None}


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
    if selection == "diagnostic1024" and digest(SPEC_266_1024) != prepared.get("issue266_1024_spec_sha256"):
        raise ValueError("issue266 1024-count specification changed")
    if selection == "diagnostic2048" and digest(SPEC_271_2048) != prepared.get("issue271_2048_spec_sha256"):
        raise ValueError("issue271 2048-count specification changed")
    if digest(SPEC_271_SPONSOR) != prepared.get("issue271_sponsor_spec_sha256"):
        raise ValueError("issue271 sponsorship specification changed")
    if digest(SPEC_271_ONE_READ) != prepared.get("issue271_one_read_spec_sha256"):
        raise ValueError("issue271 sponsor one-read specification changed")
    if digest(SPEC_271_FOUR_HOP) != prepared.get("issue271_four_hop_spec_sha256"):
        raise ValueError("issue271 four-hop specification changed")
    if selection == "diagnostic4097" and digest(SPEC_271_4097) != prepared.get("issue271_4097_spec_sha256"):
        raise ValueError("issue271 extended diagnostic specification changed")
    if selection == "diagnostic4097cause" and digest(SPEC_271_CAUSE) != prepared.get("issue271_cause_spec_sha256"):
        raise ValueError("issue271 causal diagnostic specification changed")
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
    limit = 60 if selection in ("diagnostic4097", "diagnostic4097cause") else 25 if selection == "gate" else 15
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
    try:
        samples = backing_samples(stderr)
        sample_parse_error = None
    except ValueError as error:
        samples, sample_parse_error = [], str(error)
    sample_counts = [row["write_class"] for row in samples]
    expected_samples = (list(range(512, count, 512)) if selection in ("diagnostic4097", "diagnostic4097cause") else
                        [count // 4, count // 2, count * 3 // 4, count]
                        if selection.startswith("diagnostic") or selection == "fuse512" else [])
    sample_versions = (3,) if selection == "diagnostic4097cause" else (2, 3)
    phase_samples_complete = selection not in ("diagnostic100v2", "diagnostic100v3", "fuse512", "diagnostic1024", "diagnostic2048", "diagnostic4097", "diagnostic4097cause") or all(
        row["version"] in sample_versions and row["acquisition_ns"] is not None
        and row["publication_ns"] is not None for row in samples)
    cause = causal_diagnostics(stderr, samples) if selection == "diagnostic4097cause" else None
    verification = {"status": "NOT_RUN"}
    if driver and driver.get("head_commit"):
        after = {**before, "expected_head_commit": driver["head_commit"]}
        case_spec(output / "case.verify", after)
        root = prepared_file.parent
        verify_command = [prepared["binaries"]["verify_shell"]["path"],
            str(output / "case.verify"), clone["store"], clone["history"],
            str(root / "old.tsv"), str(root / f"{selection}.tsv")]
        verify_started = time.monotonic_ns()
        try:
            checked, wall = run_retaining_timeout(verify_command, output / "verifier", timeout=9, env=env)
            verification = {"status": "PASS" if checked.returncode == 0 else "FAIL",
                            "wall_ns": wall, "exit_code": checked.returncode,
                            "command": verify_command}
        except subprocess.TimeoutExpired as error:
            verification = {"status": "FAIL", "timeout": True,
                            "wall_ns": time.monotonic_ns() - verify_started,
                            "limit_ns": 9_000_000_000,
                            "stdout_bytes": len(error.stdout or b""),
                            "stderr_bytes": len(error.stderr or b""),
                            "command": verify_command}
    daemon_log = stderr.decode(errors="replace")
    daemon_close = "sandbox closed\n" in daemon_log
    daemon_retained = any(term in daemon_log for term in
                          ("sandbox shutdown retained:", "workspace shutdown retained:"))
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                   and (selection not in ("fuse512", "diagnostic1024", "diagnostic2048", "diagnostic4097", "diagnostic4097cause", "gate") or (daemon_close and not daemon_retained
                        and driver.get("daemon_log_attempted")
                        and not driver.get("daemon_log_truncated")
                        and driver.get("daemon_log_error") == "None")))
    functional = bool(code == 0 and not timed_out and driver and
        driver.get("status") == "COMPLETE" and driver.get("commit_called") and
        checkpoints and sample_counts == expected_samples and phase_samples_complete and
        (selection != "diagnostic4097cause" or
         (driver.get("upstream_calls") == 4 and counts.get("write") == "4097"
          and counts.get("open") == "1" and counts.get("read") == "0")) and
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
        "issue266_1024_spec_sha256": prepared.get("issue266_1024_spec_sha256"),
        "issue271_2048_spec_sha256": prepared.get("issue271_2048_spec_sha256"),
        "issue271_sponsor_spec_sha256": prepared.get("issue271_sponsor_spec_sha256"),
        "issue271_one_read_spec_sha256": prepared.get("issue271_one_read_spec_sha256"),
        "issue271_four_hop_spec_sha256": prepared.get("issue271_four_hop_spec_sha256"),
        "issue271_4097_spec_sha256": prepared.get("issue271_4097_spec_sha256"),
        "issue271_cause_spec_sha256": prepared.get("issue271_cause_spec_sha256"),
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
        "backing_sample_parse_error": sample_parse_error,
        "phase_samples_complete": phase_samples_complete,
        "causal_diagnostics": cause,
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
        "extended_diagnostic_only": selection in ("diagnostic4097", "diagnostic4097cause"),
        "row_status": ("INELIGIBLE" if cause is None or cause["status"] == "PASS"
                       else "INCOMPLETE") if functional else "FAIL", "sample_count": 1}
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
    p.add_argument("--reuse-selection", choices=("diagnostic512", "diagnostic100v2", "diagnostic100v3", "fuse512", "diagnostic1024", "diagnostic2048", "diagnostic4097", "diagnostic4097cause", "gate"),
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
        assert list(range(512, COUNTS["diagnostic4097"], 512)) == [512 * i for i in range(1, 9)]
        assert fields({key: 0 for key in ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial", "branch_id", "old_commit")}, "diagnostic4097cause", "x")["scenario_id"] == "issue271-separated-4097-cause-v1"
        assert causal_diagnostics(b"", [])["status"] == "INCOMPLETE"
        samples = []
        for count in range(512, 4097, 512):
            backing = {f"{name}_{suffix}": count for name in CAUSE_BACKING
                       for suffix in ("calls", "ns")}
            metadata = {f"{name}_{suffix}": count for name in CAUSE_METADATA
                        for suffix in ("calls", "ns")}
            metadata.update(sponsor_attempts=count, sponsor_accepted=count,
                            sponsor_depth_fallbacks=0)
            samples.append({"version": 3, "write_class": count,
                            "backing": {**backing, "operator_diagnostics_enabled": "true"},
                            "metadata": {**metadata, "operator_diagnostics_enabled": "true"}})
        stream = dict.fromkeys(CAUSE_STREAM_FIELDS, "1")
        stream.update(scope="file_stream_read", status="ok", declared_extents="8194",
                      descriptor_read_calls="8194", parsed_extents="8194", edits="4097",
                      edit_spool_write_calls="16388", edit_spool_write_bytes="131104")
        source = dict.fromkeys(CAUSE_SOURCE_FIELDS, "1")
        source.update(scope="save_file", source_complete="true", declared_extents="8194",
                      final_length="8194", replacement_bytes="4097",
                      descriptor_source_bytes="196656", descriptor_cursor_next_calls="8194",
                      replacement_source_bytes="4097", local_reader_create_calls="4097",
                      local_read_calls="4097", local_read_bytes="4097",
                      local_segment_open_calls="4097", local_aligned_read_calls="4097",
                      local_aligned_read_bytes="16781312")
        logs = b"\n".join((f"{marker} v=1 " + " ".join(f"{key}={value}" for key, value in fields.items())).encode()
                          for marker, fields in ((CAUSE_LOGS[0], stream), (CAUSE_LOGS[1], source)))
        logs += b'\nLFT1 {"role":1,"kind":"operation","timing":{"name":"SaveFile","children":[{"name":"service.pre_save_input","elapsed_ns":1}]}}\n'
        assert causal_diagnostics(logs, samples)["status"] == "PASS"
        assert causal_diagnostics(logs.replace(b"edits=4097", b"edits=4097 edits=4097"), samples)["status"] == "INCOMPLETE"
        assert causal_diagnostics(logs.replace(b'"elapsed_ns":1', b'"elapsed_ns":"bad"'), samples)["status"] == "INCOMPLETE"
        assert causal_diagnostics(logs.replace(b"local_aligned_read_ns=1", b""), samples)["status"] == "INCOMPLETE"
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
