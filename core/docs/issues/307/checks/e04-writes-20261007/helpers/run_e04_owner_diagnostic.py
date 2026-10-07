#!/usr/bin/env python3
"""One original owned E04 diagnostic; root owner executes, never a benchmark gate.

This ignored one-off supervisor reuses the closed prepared inputs and matched
debug binaries. It never builds, retries a product call, regenerates a fixture,
or removes a failed Store/database/container. Product work stops at 80 seconds;
the complete invocation has a 90-second cap including owned process disposal.
The caller must also apply an explicit 90-second external wall stop.
"""
import argparse
import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
TARGET = ROOT / "core/target/cluster2-307"
CHECKS = ROOT / "core/docs/issues/307/checks/e04-writes-20261007"
HOST = TARGET / "debug/examples/e2_writes_host"
LINUX = ROOT / "core/target/cluster2-linux/debug/examples/e2_writes"
IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"
PREPARED = TARGET / "e04-prepared-20261007"
INPUTS = {
    "base.bin": (16777216, "a813c22ec9b6429809beafdeed2903207e31f2c68c4ab5795c11a857d26e6f17"),
    "replacements.bin": (4096000, "c9440baeb12b6f5d6fe142092fe3f862611f8c4ab36788a078922ea764489290"),
}
GAPS = ["private-base-facts-provider-id-trace", "per-original-job-statement-families",
        "whole-operation-copies", "statement-fingerprint-bind-correlation", "indexed-visited-rows",
        "exact-eligible-debt", "queue-only-peak", "isolated-resource-runnable-wait",
        "host-linux-kernel-phase-residency", "physical-io", "cache-enforcement-calibration"]
EXAMPLES = ROOT / "core/crates/layerfs-daemon/examples"
DRIVER = sorted({EXAMPLES / "e2_writes.rs", EXAMPLES / "e2_writes_host.rs",
                 *list((EXAMPLES / "e2_writes").glob("*.rs")),
                 EXAMPLES / "e2_startup/json.rs", EXAMPLES / "e2_startup/records.rs",
                 EXAMPLES / "e2_startup/streams.rs",
                 ROOT / "core/benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"})
ORACLE = EXAMPLES / "e2_writes/oracle.rs"
HARNESS = [ROOT / "core/benchmark/fs-bench-pro/shared/evidence_jobs.py",
           ROOT / "core/benchmark/fs-bench-pro/shared/evidence_workload.py",
           Path(__file__).resolve()]


def require(value, message):
    if not value:
        raise RuntimeError(message)


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb", buffering=0) as source:
        for block in iter(lambda: source.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def descriptor(path, with_bytes=False):
    path = Path(path).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), "artifact absent/outside owning checkout: " + str(path))
    result = {"path": str(path.relative_to(ROOT)), "sha256": sha(path)}
    if with_bytes:
        result["bytes"] = path.stat().st_size
    return result


def read_json(path, maximum=1 << 20):
    with Path(path).open("rb", buffering=0) as source:
        raw = source.read(maximum + 1)
    require(len(raw) <= maximum, "metadata input exceeds declared byte window: " + str(path))
    return json.loads(raw)


def write_json(path, value, maximum=1 << 20):
    raw = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()
    require(len(raw) <= maximum, "metadata output exceeds declared byte window: " + str(path))
    with Path(path).open("xb", buffering=0) as output:
        count = output.write(raw)
        require(count == len(raw), "short original metadata write; acknowledged prefix retained")
    return descriptor(path)


def container_path(path):
    path = Path(path).resolve()
    require(path.is_relative_to(ROOT), "container input outside declared bind")
    return str(Path("/work") / path.relative_to(ROOT))


class OriginalProcess:
    def __init__(self, owner, label, argv, limit):
        self.owner, self.label, self.argv = owner, label, list(argv)
        self.started_ns = time.monotonic_ns()
        self.limit = limit
        self.timed_out = False
        self.recorded = False
        self.path = owner.receipts / (label + ".json")
        self.process = None
        owner.processes.append(self)
        # Defer our Python signal exception until the spawned child is registered.
        # No signal masks or handling are changed in the actual child process.
        owner.in_launch = True
        try:
            with self.path.with_suffix(".stdout").open("xb", buffering=0) as stdout:
                with self.path.with_suffix(".stderr").open("xb", buffering=0) as stderr:
                    self.process = subprocess.Popen(
                        self.argv, cwd=ROOT, stdout=stdout, stderr=stderr,
                        env=dict(os.environ, LAYERFS_CONSTRUCTION_WORKERS="1", GIT_OPTIONAL_LOCKS="0"),
                        start_new_session=True,
                    )
        finally:
            owner.in_launch = False
        if owner.deferred_stop is not None:
            raise TimeoutError(owner.deferred_stop)

    def record(self):
        if self.recorded:
            return self.path
        require(self.process.poll() is not None, "cannot record a still-running original process")
        self.recorded = True
        write_json(self.path, {
            "command": self.argv, "exit_code": self.process.returncode,
            "timed_out": self.timed_out, "wall_timeout_s": self.limit,
            "wall_ns": time.monotonic_ns() - self.started_ns,
            "kind": "one-original-uncontrolled-diagnostic" if self.label in ("host", "consumer") else "read-only-or-owned-lifecycle-observation",
            "mode": "diagnostic", "sample_count": 0, "admission_eligible": False,
            "qualification_status": "NOT_EVALUATED", "pid": self.process.pid,
            "stdout": descriptor(self.path.with_suffix(".stdout")),
            "stderr": descriptor(self.path.with_suffix(".stderr")),
        })
        return self.path

    def kill_process_group(self):
        if self.process.poll() is None:
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass  # The exact child may have exited between poll and signal.
            # Always reap the killed direct child, even when metadata time expired.
            self.process.wait(timeout=0.25)
        self.record()


class Owner:
    def __init__(self, run, profile, started, build_inputs, supplemental):
        self.run, self.profile, self.started = run, profile, started
        self.build_inputs = build_inputs
        self.supplemental = supplemental
        self.started_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
        self.deadline = started + 88  # Leave two seconds for terminal metadata/outer stop.
        self.product_deadline = started + 80
        self.receipts = run / "receipts"
        self.prep = run / "prepared"
        self.receipts.mkdir()
        self.prep.mkdir()
        self.host = self.consumer = None
        self.consumer_argv = None
        self.container_name = "layerfs-e04-" + profile + "-" + uuid.uuid4().hex[:16]
        self.cid_path = run / "consumer.cid"
        self.cid = None
        self.inspected = None
        self.inspect_receipt = None
        self.secondary = []
        self.product_exited = False
        self.removed = False
        self.removal_disposition = "NOT_ATTEMPTED"
        self.external_stop = False
        self.processes = []
        self.in_launch = False
        self.deferred_stop = None

    def remaining(self, product=False):
        duration = (self.product_deadline if product else self.deadline) - time.monotonic()
        require(duration > 0, "original complete diagnostic deadline reached")
        return duration

    def small(self, label, argv, seconds=5):
        limit = min(seconds, self.remaining())
        original = OriginalProcess(self, label, argv, limit)
        try:
            original.process.wait(timeout=limit)
        except BaseException as error:
            original.timed_out = isinstance(error, (TimeoutError, subprocess.TimeoutExpired))
            try:
                original.kill_process_group()
            except BaseException as secondary:
                self.secondary.append("external observation stop " + label + ": " + repr(secondary))
            raise
        original.record()
        require(original.process.returncode == 0, "original external observation refused: " + label)
        return original.path

    def text(self, label, argv, seconds=5):
        receipt = self.small(label, argv, seconds)
        path = receipt.with_suffix(".stdout")
        require(path.stat().st_size <= 1 << 20, "observation output byte window: " + label)
        return path.read_text().strip(), receipt

    def check_build_source(self, label, frozen):
        mismatches = []
        for item in frozen["files"]:
            self.remaining()
            path = ROOT / item["path"]
            actual = sha(path) if path.is_file() else None
            if actual != item["sha256"]:
                mismatches.append({"path": item["path"], "expected": item["sha256"], "actual": actual})
        result = {"schema": "e04-build-source-reconciliation-v1", "source_receipt": descriptor(self.build_inputs),
                  "additional_build_witness": descriptor(self.supplemental),
                  "files_checked": len(frozen["files"]), "mismatches": mismatches,
                  "status": "MATCH" if not mismatches else "MISMATCH", "source_sealed": False}
        write_json(self.prep / (label + ".json"), result)
        require(not mismatches, "source differs from specified exact pre-build receipt; no product invocation/retry")

    def inspect_owned(self, label, identifier, seconds=3):
        receipt = self.small(label, ["docker", "inspect", identifier], seconds)
        values = read_json(receipt.with_suffix(".stdout"))
        require(isinstance(values, list) and len(values) == 1, "single actual owned inspect required")
        value = values[0]
        require(value.get("Image") == IMAGE and value.get("Name") == "/" + self.container_name,
                "inspect did not identify this invocation's exact container")
        require(self.consumer_argv is not None and value.get("Config", {}).get("Cmd") == self.consumer_argv
                and value.get("Config", {}).get("Entrypoint") in (None, [])
                and value.get("Path") == self.consumer_argv[0] and value.get("Args") == self.consumer_argv[1:],
                "inspect original direct collector command differs")
        require(re.fullmatch(r"[0-9a-f]{64}", value.get("Id", "")), "actual inspect lacks exact CID")
        self.cid, self.inspected, self.inspect_receipt = value["Id"], value, receipt
        return value

    def dispose_failed_processes(self):
        # Infrastructure stopping is not a product Close/abort/replay. Never remove.
        for original in list(self.processes):
            if original.process is None:
                continue
            if original.label == "host":
                self.host = original
            elif original.label == "consumer":
                self.consumer = original
            elif not original.recorded:
                try:
                    original.kill_process_group()
                except BaseException as error:
                    self.secondary.append("interrupted observation external stop: " + repr(error))
        self.deferred_stop = None
        if self.host is not None:
            try:
                if self.host.process.poll() is None:
                    self.external_stop = True
                    self.host.timed_out = time.monotonic() >= self.product_deadline
                self.host.kill_process_group()
            except BaseException as error:
                self.secondary.append("host external stop: " + repr(error))
        if self.consumer is None:
            return
        try:
            if self.cid_path.is_file():
                raw = self.cid_path.read_text().strip()
                require(re.fullmatch(r"[0-9a-f]{64}", raw), "owned CID file malformed")
                self.cid = raw
            if self.inspected is None:
                self.inspect_owned("failed-inspect", self.cid or self.container_name, 1.5)
            if self.inspected.get("State", {}).get("Running"):
                self.external_stop = True
                self.small("failed-owned-container-kill", ["docker", "kill", "--signal", "KILL", self.cid], 1.5)
                self.inspect_owned("failed-after-kill-inspect", self.cid, 1.5)
                require(self.inspected.get("State", {}).get("Running") is False,
                        "owned container still running after single explicit stop")
        except BaseException as error:
            self.secondary.append("container external stop/outcome UNKNOWN: " + repr(error))
        try:
            if self.consumer.process.poll() is None and time.monotonic() >= self.product_deadline:
                self.consumer.timed_out = True
            self.consumer.kill_process_group()
        except BaseException as error:
            self.secondary.append("consumer client external stop: " + repr(error))


def clone_input(owner, name):
    expected_bytes, expected_sha = INPUTS[name]
    master = PREPARED / name
    before = master.stat()
    require(master.is_file() and not master.is_symlink() and before.st_size == expected_bytes
            and before.st_mode & 0o222 == 0, "closed prepared input mode/kind/length changed")
    target = owner.prep / name
    digest = hashlib.sha256()
    count = 0
    with master.open("rb", buffering=0) as source, target.open("xb", buffering=0) as output:
        for block in iter(lambda: source.read(65536), b""):
            owner.remaining(product=True)
            digest.update(block)
            require(output.write(block) == len(block), "short original setup clone write; prefix retained")
            count += len(block)
    after = master.stat()
    require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
            and count == expected_bytes and digest.hexdigest() == expected_sha,
            "closed prepared master changed; no regeneration or replay")
    target.chmod(0o444)
    copied = target.stat()
    require(copied.st_nlink == 1 and (copied.st_dev, copied.st_ino) != (before.st_dev, before.st_ino)
            and sha(target) == expected_sha, "setup clone is not an independent exact byte copy")
    return {"master": descriptor(master, True), "copy": descriptor(target, True),
            "setup": "clone", "clone_method": "independent-streamed-byte-copy-create-new",
            "cache_state": "uncontrolled", "cold_claim": False}


def execute(owner):
    require(sys.platform == "darwin", "actual supported macOS host required")
    native = os.uname()
    require(native.machine in ("arm64", "aarch64"), "registered host ARM64 required")
    for path in (HOST, LINUX):
        require(path.is_file() and not path.is_symlink(), "exact built driver absent/aliased: " + str(path))
    frozen = read_json(owner.build_inputs)
    require(frozen.get("schema") == "e04-final-build-inputs-v1" and frozen.get("source_sealed") is False
            and frozen.get("image") == IMAGE, "final build-input receipt differs")
    supplement = read_json(owner.supplemental)
    require(supplement.get("schema") == "e04-additional-build-inputs-v1"
            and supplement.get("status") == "MATCH" and isinstance(supplement.get("files"), list)
            and supplement.get("prior_input_inventory") == descriptor(owner.build_inputs)
            and isinstance(supplement.get("build_receipts"), list) and len(supplement["build_receipts"]) == 2,
            "explicit additional build-source witness absent or unrelated")
    build_receipts = []
    for item in supplement["build_receipts"]:
        path = (ROOT / item["path"]).resolve()
        require(path.is_relative_to(CHECKS) and descriptor(path) == item, "additional build receipt identity changed")
        build_receipts.append(path)
    for path in build_receipts:
        value = read_json(path)
        require(value.get("exit_code") == 0 and value.get("timed_out") is False
                and "--locked" in value.get("command", []) and "build" in value["command"],
                "matched final successful locked example build absent")
    supplement_paths = {item["path"] for item in supplement["files"]}
    require("core/benchmark/fs-bench-pro-storage-content/src/workload/digest.rs" in supplement_paths,
            "shared benchmark digest lacks source-accurate build witness")
    for item in supplement["files"]:
        require(descriptor(ROOT / item["path"]) == item, "supplemental build input changed")
    frozen = dict(frozen, files=frozen["files"] + [item for item in supplement["files"]
                  if item["path"] not in {value["path"] for value in frozen["files"]}])
    owner.check_build_source("source-before", frozen)
    head, _ = owner.text("git-head", ["git", "rev-parse", "HEAD"])
    tree, _ = owner.text("git-tree", ["git", "rev-parse", "HEAD^{tree}"])
    require(head == frozen["head"] and re.fullmatch(r"[0-9a-f]{40}", tree), "build base commit/tree changed")
    binaries = {"host": descriptor(HOST), "consumer": descriptor(LINUX)}
    require(supplement.get("binary_identity_unchanged") == [descriptor(HOST, True), descriptor(LINUX, True)],
            "binaries differ from source-complete build witness; no product invocation")
    docker_info_receipt = owner.small("docker-info", ["docker", "info", "--format", "{{json .}}"])
    docker_info = read_json(docker_info_receipt.with_suffix(".stdout"))
    image_receipt = owner.small("docker-image-inspect", ["docker", "image", "inspect", IMAGE])
    images = read_json(image_receipt.with_suffix(".stdout"))
    require(isinstance(images, list) and len(images) == 1 and images[0].get("Id") == IMAGE
            and images[0].get("Os") == "linux" and images[0].get("Architecture") in ("arm64", "aarch64")
            and images[0].get("Config", {}).get("Entrypoint") in (None, []), "actual image execution facts differ")
    require(docker_info.get("OSType") == "linux" and docker_info.get("Architecture") in ("aarch64", "arm64")
            and isinstance(docker_info.get("KernelVersion"), str) and docker_info["KernelVersion"],
            "actual Docker engine topology unavailable; refuse before product launch")
    rustc, rustc_receipt = owner.text("host-rustc", ["rustup", "run", "1.85.1", "rustc", "--version", "--verbose"])
    require("release: 1.85.1" in rustc and "host: aarch64-apple-darwin" in rustc, "host toolchain observation mismatch")
    toolchain = write_json(owner.prep / "toolchain.json", {
        "schema": "e04-matched-debug-build-identity-v1", "build_profile": "debug-functional-only",
        "before_build": descriptor(owner.build_inputs),
        "additional_build_witness": descriptor(owner.supplemental),
        "build_receipts": [descriptor(path) for path in build_receipts], "host_rustc": descriptor(rustc_receipt),
        "linux_toolchain": "matched-original-locked-build-command-in-pinned-image; no new compiler invocation",
        "binaries": binaries, "repository_ARM64_inputs": descriptor(ROOT / ".cargo/config.toml"),
        "source_accurate_before_after_required": True, "performance_seal": False})
    dependencies = write_json(owner.prep / "dependencies.json", {
        "schema": "e04-locked-dependency-inputs-v1", "lockfile": descriptor(ROOT / "core/Cargo.lock"),
        "active_workspace": descriptor(ROOT / "core/Cargo.toml"),
        "build_inputs": descriptor(owner.build_inputs),
        "scope": "original matched locked build inputs; not a dynamic dependency-cost attribution"})
    inputs = {name: clone_input(owner, name) for name in INPUTS}
    write_json(owner.prep / "closed-input-reuse.json", {
        "schema": "e04-closed-input-reuse-v1", "prepared_receipt": descriptor(CHECKS / "14-closed-input-verification.json"),
        "inputs": inputs, "cache_state": "uncontrolled", "source_copied_bytes_is_not_importer_io": True})
    current_paths = sorted({ROOT / value["path"] for value in frozen["files"]} | set(DRIVER) | set(HARNESS))
    current_sources = [descriptor(path) for path in current_paths]
    require(len(current_sources) <= 4096, "current source inventory window")
    retained_sources = []
    for item in current_sources:
        owner.remaining(product=True)
        retained = owner.prep / "source-copies" / item["path"]
        retained.parent.mkdir(parents=True, exist_ok=True)
        with (ROOT / item["path"]).open("rb", buffering=0) as original, retained.open("xb", buffering=0) as output:
            for block in iter(lambda: original.read(65536), b""):
                require(output.write(block) == len(block), "short original source-retention write")
        require(sha(retained) == item["sha256"], "source changed while retaining independent pre-start copy")
        retained.chmod(0o444)
        retained_sources.append({"original": item, "retained_copy": descriptor(retained)})
    retained_index = write_json(owner.prep / "retained-source-copies.json", {
        "schema": "e04-retained-current-source-copies-v1", "sources": retained_sources})
    product = write_json(owner.prep / "current-source.json", {
        "schema": "cluster-two-e2-source-inventory-v1", "source_commit": head, "source_tree": tree,
        "source_sealed": False, "binding": "base-commit-plus-current-inventory", "sources": current_sources,
        "retained_source_inventory": retained_index,
        "scope": "specified exact active build input inventory plus E04 driver/oracle and actual harness sources"})
    driver = write_json(owner.prep / "driver-sources.json", {
        "schema": "e04-external-driver-source-inventory-v1", "sources": [descriptor(path) for path in DRIVER],
        "harness": [descriptor(path) for path in HARNESS], "binaries": binaries,
        "build_receipts": [descriptor(path) for path in build_receipts], "unavailable": GAPS})
    trace = write_json(owner.prep / "trace.json", {
        "schema": "e04-frozen-original-write-trace-v1", "case": "E04-write-16m", "seed": 1,
        "writes": 1000, "write_bytes": 4096, "input_bytes": 4096000, "base_bytes": 16777216,
        "offset_formula": "4096 * ((104729 * index) % 4096)", "indices": "0..1000",
        "mtime_seconds": -7, "mtime_nanoseconds": 42, "input": inputs["replacements.bin"]["copy"],
        "generator_source": descriptor(ROOT / "core/benchmark/fs-bench-pro/shared/evidence_workload.py"),
        "ordinary_construction_workers": 1, "namespace_init_workers": 4, "single_original_attempt": True})
    cache = write_json(owner.prep / "cache.json", {
        "schema": "e04-uncontrolled-diagnostic-cache-v1", "cache_state": "uncontrolled", "admission_eligible": False,
        "setup": "clone", "setup_warmth_credit": "NO_COLD_OR_NUMERICAL_CLAIM", "client_cache_capacity": 0,
        "host_store_kernel_page_cache": "UNCONTROLLED", "linux_overlay_kernel_page_cache": "UNCONTROLLED",
        "observer_calibration": "UNAVAILABLE", "physical_io": "UNAVAILABLE", "e1_sample_count": 0})
    topology = write_json(owner.prep / "topology.json", {
        "schema": "cluster-two-e2-topology-witness-v1", "status": "OBSERVED",
        "scope": "external-observed-execution-domain", "execution_kind": "docker",
        "source": "actual docker info and exact docker image inspect before original consumer; post-run container inspect binds invocation",
        "os": "linux", "architecture": "aarch64", "kernel": docker_info["KernelVersion"],
        "docker_info": descriptor(docker_info_receipt), "docker_info_stdout": descriptor(docker_info_receipt.with_suffix(".stdout")),
        "image_inspect": descriptor(image_receipt), "image_inspect_stdout": descriptor(image_receipt.with_suffix(".stdout")),
        "host_uname": list(native), "exclusive_resource_ownership": "UNAVAILABLE", "clock_calibration": "UNAVAILABLE"})
    image = write_json(owner.prep / "image.json", {
        "schema": "cluster-two-e2-image-witness-v1", "status": "OBSERVED", "image_id": IMAGE,
        "source": "actual exact docker image inspect before original consumer",
        "receipt": descriptor(image_receipt), "stdout": descriptor(image_receipt.with_suffix(".stdout"))})
    store, source, assignment = owner.run / "store", owner.run / "acquisition-source", owner.prep / "assignment.txt"
    host_output, output, db = owner.run / "host-output", owner.run / "consumer-output", owner.run / "overlay.sqlite"
    identity_path = owner.prep / "identity-input.json"
    host_argv = [str(HOST), str(store), str(source), str(assignment), str(owner.prep / "base.bin"),
                 owner.profile, str(host_output), "host.docker.internal", "80"]
    write_json(owner.prep / "host-command.json", {"command": host_argv, "construction_environment": {"LAYERFS_CONSTRUCTION_WORKERS": "1"},
               "namespace_init_workers": 4, "host_deadline_seconds": 80, "outer_product_cutoff_seconds": 80,
               "complete_external_seconds": 90, "mode": "diagnostic", "sample_count": 0})
    owner.remaining(product=True)
    owner.host = OriginalProcess(owner, "host", host_argv, 80)
    endpoint_path = host_output / "endpoint.txt"
    while not endpoint_path.is_file() or endpoint_path.stat().st_size == 0:
        require(owner.host.process.poll() is None, "original host exited before readiness; do not launch consumer")
        owner.remaining(product=True)
        time.sleep(0.01)
    endpoint = endpoint_path.read_text().strip()
    require(re.fullmatch(r"host\.docker\.internal:[0-9]+", endpoint), "original advertised endpoint malformed")
    require(not source.exists(), "host native acquisition source was not removed")
    fields = assignment.read_text().splitlines()
    require(len(fields) == 20 and fields[0] == "layerfs-r4-functional-v1"
            and fields[18] == fields[19] == owner.profile, "actual host assignment profile/shape mismatch")
    for index in (14, 15, 16):
        require(re.fullmatch(r"[0-9a-f]{64}", fields[index]), "actual assignment canonical context missing")
    acquisition = assignment.with_suffix(".fixture")
    facts = acquisition.read_text().splitlines()
    require(len(facts) == 9 and facts[0] == "layerfs-e04-acquisition-v1" and facts[8] == "source_removed=true"
            and facts[1] == INPUTS["base.bin"][1] and facts[7] == str(INPUTS["base.bin"][0]),
            "actual host acquisition sidecar differs from closed fixture")
    fixture = write_json(owner.prep / "fixture-bundle.json", {
        "schema": "cluster-two-e04-fixture-bundle-v1", "case": "E04-write-16m", "profile": owner.profile, "seed": 1,
        "source_context": {"source_commit": head, "source_tree": tree, "source_sealed": False,
                           "effective_root": fields[14], "scope": fields[15], "filesystem_profile": fields[16], "root_serial": int(fields[17])},
        "inputs": {"assignment": descriptor(assignment, True), "acquisition": descriptor(acquisition, True),
                   "base_input": inputs["base.bin"]["copy"], "replacements": inputs["replacements.bin"]["copy"]}})
    consumer_host = [str(LINUX), endpoint, str(assignment), str(owner.prep / "base.bin"),
                     str(owner.prep / "replacements.bin"), str(db), str(output), str(identity_path)]
    argv = [container_path(value) if index != 1 else value for index, value in enumerate(consumer_host)]
    owner.consumer_argv = argv
    external = ["docker", "run", "--name", owner.container_name, "--cidfile", str(owner.cid_path),
                "-e", "LAYERFS_CONSTRUCTION_WORKERS=1", "-v", str(ROOT) + ":/work", "-w", "/work", IMAGE, *argv]
    command = write_json(owner.prep / "consumer-command.json", {
        "schema": "cluster-two-e2-command-v1", "execution_kind": "docker", "argv": argv,
        "image_id": IMAGE, "external_argv": external, "construction_environment": {"LAYERFS_CONSTRUCTION_WORKERS": "1"}})
    identity = {"schema": "cluster-two-e2-identity-v1", "receipt_id": hashlib.sha256(uuid.uuid4().bytes).hexdigest(),
                "source_commit": head, "source_tree": tree, "source_sealed": False, "source_arm": "diagnostic",
                "profile": "local-overlay-disposable", "cache_state": "uncontrolled", "execution_kind": "docker",
                "os": "linux", "architecture": "aarch64", "kernel": docker_info["KernelVersion"],
                "topology": {"status": "OBSERVED", "artifact": topology}, "image": image, "command_artifact": command,
                "artifacts": {"product": product, "toolchain": toolchain, "cargo_config": descriptor(ROOT / ".cargo/config.toml"),
                              "cargo_lock": descriptor(ROOT / "core/Cargo.lock"), "dependencies": dependencies,
                              "binary": binaries["consumer"], "driver": driver, "fixture": fixture,
                              "trace": trace, "cache": cache, "observer": descriptor(ORACLE)}}
    identity_descriptor = write_json(identity_path, identity, 16384)
    write_json(owner.prep / "pre-consumer-seal.json", {"schema": "e04-pre-consumer-seal-v1",
               "at_monotonic_ns": time.monotonic_ns(), "identity": identity_descriptor, "fixture": fixture,
               "source": product, "binaries": binaries, "host_pid": owner.host.process.pid,
               "host_ready_endpoint": descriptor(endpoint_path), "output_not_created": not output.exists(),
               "database_not_created": not db.exists(), "sample_count": 0, "qualification_status": "NOT_EVALUATED"})
    require(not output.exists() and not db.exists(), "original consumer output/database already exists")
    require(owner.host.process.poll() is None, "original host exited during independent sealing")
    owner.remaining(product=True)
    owner.consumer = OriginalProcess(owner, "consumer", external, owner.remaining(product=True))
    while True:
        host_code, consumer_code = owner.host.process.poll(), owner.consumer.process.poll()
        if host_code is not None:
            owner.host.record()
            require(host_code == 0, "original host failed; later product calls forbidden")
        if consumer_code is not None:
            owner.consumer.record()
            require(consumer_code == 0, "original Linux collector failed; later product calls forbidden")
        if host_code is not None and consumer_code is not None:
            break
        if time.monotonic() >= owner.product_deadline:
            if host_code is None:
                owner.host.timed_out = True
            if consumer_code is None:
                owner.consumer.timed_out = True
            raise RuntimeError("80-second original product cutoff; terminal outcome retained")
        time.sleep(0.01)
    owner.product_exited = True
    require(owner.cid_path.is_file(), "original --cidfile absent")
    owner.cid = owner.cid_path.read_text().strip()
    require(re.fullmatch(r"[0-9a-f]{64}", owner.cid), "original CID malformed")
    inspected = owner.inspect_owned("consumer-inspect", owner.cid)
    require(inspected.get("State", {}).get("Running") is False and inspected["State"].get("ExitCode") == 0,
            "actual inspected collector not successfully exited")
    mounts = [m for m in inspected.get("Mounts", []) if str(m.get("Destination", "")).startswith("/work")]
    require(len(mounts) == 1 and mounts[0].get("Type") == "bind" and mounts[0].get("Destination") == "/work"
            and mounts[0].get("Source") == str(ROOT) and mounts[0].get("RW") is True,
            "actual inspected bind mapping differs; retain container")
    mapping = write_json(owner.run / "docker-path-mapping.json", {
        "schema": "cluster-two-e2-docker-path-mapping-v1", "status": "OBSERVED", "scope": "diagnostic-input-path-binding",
        "source": "single actual successful executed docker run, exact --cidfile and post-exit docker inspect; no dev/inode/block equivalence",
        "container_id": owner.cid, "image_id": IMAGE, "host_root": str(ROOT), "container_root": "/work",
        "collector_argv": argv, "host_argv": consumer_host,
        "artifacts": {"command_receipt": descriptor(owner.consumer.path), "inspect_receipt": descriptor(owner.inspect_receipt),
                      "inspect_stdout": descriptor(owner.inspect_receipt.with_suffix(".stdout")), "cid_file": descriptor(owner.cid_path)}})
    owner.check_build_source("source-after", frozen)
    changed = [item["path"] for item in current_sources if sha(ROOT / item["path"]) != item["sha256"]]
    require(not changed and {"host": descriptor(HOST), "consumer": descriptor(LINUX)} == binaries,
            "source/harness/binary changed during original attempt; retain all artifacts")
    write_json(owner.prep / "source-and-binary-after.json", {"status": "MATCH", "sources_checked": len(current_sources),
               "source_before": product, "binaries": binaries, "source_sealed": False,
               "before_after_only": "not exclusive physical ownership or cache calibration"})
    owner.removal_disposition = "ATTEMPTED_UNKNOWN"
    owner.small("successful-owned-container-removal", ["docker", "rm", owner.cid], 3)
    owner.removed = True
    owner.removal_disposition = "KNOWN_REMOVED"
    return {"consumer_output": str(output), "mapping": mapping, "identity": identity_descriptor,
            "store": str(store), "database": str(db), "raw_host_and_consumer_exit": "SUCCESS",
            "write_window_consistency": "NOT_EVALUATED-validator-not-invoked", "observation_consistency": "INCOMPLETE"}


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--profile", choices=("disposable", "durable"), default="disposable")
    parser.add_argument("--setup", choices=("clone",), required=True)
    parser.add_argument("--build-input-receipt", type=Path, required=True)
    parser.add_argument("--additional-build-witness", type=Path, required=True)
    args = parser.parse_args()
    run = args.run_dir.absolute()
    require(run.parent.is_dir(), "create the owned parent before invocation")
    run = run.parent.resolve() / run.name
    require(run.is_relative_to(TARGET) and not run.exists() and not run.is_symlink(), "fresh ignored owned output root required")
    lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
    require(lock_path.parent.is_dir(), "existing worktree measurement lock directory absent")
    with lock_path.open("a+b", buffering=0) as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        run.mkdir()
        supplemental = args.additional_build_witness.resolve()
        require(supplemental.is_relative_to(ROOT), "additional build witness outside checkout")
        build_inputs = args.build_input_receipt.resolve()
        require(build_inputs.is_relative_to(ROOT), "build input receipt outside checkout")
        owner = Owner(run, args.profile, started, build_inputs, supplemental)
        result, primary = None, None
        def stop_signal(number, frame):
            message = "original external watchdog/signal " + str(number)
            if owner.in_launch:
                owner.deferred_stop = message
                return
            raise TimeoutError(message)
        signal.signal(signal.SIGALRM, stop_signal)
        signal.signal(signal.SIGTERM, stop_signal)
        signal.setitimer(signal.ITIMER_REAL, max(0.001, owner.product_deadline - time.monotonic()))
        try:
            result = execute(owner)
        except BaseException as error:
            signal.setitimer(signal.ITIMER_REAL, 0)
            primary = repr(error)
            owner.dispose_failed_processes()
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
        elapsed = time.monotonic() - started
        outcome = {"schema": "e04-one-original-owner-diagnostic-v1", "mode": "diagnostic", "profile": args.profile,
                   "started_utc": owner.started_utc, "command": sys.argv,
                   "owner_prefix_wall_ns": math.ceil(elapsed * 1e9), "complete_command_limit_ns": 90_000_000_000,
                   "owner_prefix_scope": "helper-start-through-before-terminal-record-encoding-write-hash-print",
                   "complete_command_status": "UNAVAILABLE-requires-enclosing-90-second-process-receipt",
                   "product_cutoff_seconds": 80, "status": "RAW_EXECUTION_SUCCESS" if primary is None else "FAILED",
                   "original_error": primary, "secondary_infrastructure_errors": owner.secondary,
                   "container_id": owner.cid, "container_known_removed": owner.removed,
                   "container_removal_disposition": owner.removal_disposition,
                   "container_last_observed_inspect_state": owner.inspected.get("State") if owner.inspected else None,
                   "host_exit": owner.host.process.poll() if owner.host else "NOT_RUN",
                   "consumer_client_exit": owner.consumer.process.poll() if owner.consumer else "NOT_RUN",
                   "store_and_database_retained": True, "inputs_and_all_receipts_retained": True,
                   "result": result, "e1_sample_count": 0, "e1_sample_status": "NOT_RUN", "E05_status": "NOT_RUN",
                   "admission_eligible": False, "qualification_status": "NOT_EVALUATED", "cache_state": "uncontrolled",
                   "observation_consistency": "INCOMPLETE", "unavailable": GAPS,
                   "external_forced_stop": owner.external_stop,
                   "original_product_outcome": ("NOT_ATTEMPTED" if owner.host is None else
                        "RAW_SUCCESS-requires-validator" if primary is None else
                        "UNAVAILABLE-external-failure-does-not-replace-original-host-and-consumer-custody"),
                   "no_failed_operation_replay": True}
        if elapsed > 90:
            outcome["status"] = "FAILED"
            outcome["owner_prefix_status"] = "OVER_COMPLETE_LIMIT"
        write_json(run / "owner-outcome.json", outcome)
        print(json.dumps({"owner_outcome": str(run / "owner-outcome.json"), "result": result,
                          "status": outcome["status"], "original_error": primary}, sort_keys=True))
        return 0 if primary is None and elapsed <= 90 else 1


if __name__ == "__main__":
    sys.exit(main())
