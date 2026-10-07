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
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
TARGET = ROOT / "core/target/cluster2-307"
CHECKS = ROOT / "core/docs/issues/307/checks/e04-writes-20261007"
NATIVE_CHECKS = ROOT / "core/docs/issues/307/checks/e04-native-backing-20261007"
DISPOSAL_CHECKS = ROOT / "core/docs/issues/307/checks/e04-disposal-20261007"
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
BACKING_PROBE = TARGET / "e04_native_backing_observation.py"
NATIVE_ROOT = "/state"
NATIVE_DATABASE = "/state/overlay.sqlite"
RETENTION = "RETAIN_ORIGINAL_VOLUME_ON_SUCCESS_OR_FAILURE"
HARNESS = [ROOT / "core/benchmark/fs-bench-pro/shared/evidence_jobs.py",
           ROOT / "core/benchmark/fs-bench-pro/shared/evidence_backing.py",
           ROOT / "core/benchmark/fs-bench-pro/shared/evidence_workload.py",
           Path(__file__).resolve(), BACKING_PROBE]


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
        finished_ns = time.monotonic_ns()
        write_json(self.path, {
            "command": self.argv, "exit_code": self.process.returncode,
            "timed_out": self.timed_out, "wall_timeout_s": self.limit,
            "wall_ns": finished_ns - self.started_ns,
            "started_monotonic_ns": self.started_ns, "finished_monotonic_ns": finished_ns,
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
        self.receipt_id = hashlib.sha256(uuid.uuid4().bytes).hexdigest()
        self.volume_name = self.container_name + "-state"
        self.volume_labels = {"layerfs.e04.receipt": self.receipt_id,
                              "layerfs.e04.owner": self.container_name}
        self.volume_created = False
        self.volume_creation_disposition = "NOT_ATTEMPTED"
        self.backing = None
        self.pre_start = None
        self.post_exit = None
        self.probe_source = None
        self.control = None
        self.pre_host_seal = None
        self.binary_seals = None
        self.input_seals = None
        self.consumer_manifest = None
        self.inspect_evidence_refusals = []
        self.helper_containers = []
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

    def small(self, label, argv, seconds=5, expected_exit=0):
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
        require(original.process.returncode == expected_exit, "original external observation refused: " + label)
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

    def inspect_owned(self, label, identifier, seconds=3, allow_evidence_refusal=False):
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
        if self.cid is not None:
            require(value["Id"] == self.cid, "actual inspect substituted original consumer CID")
        self.require_owned_labels(value, "consumer")
        require(value.get("HostConfig", {}).get("ContainerIDFile") == str(self.cid_path),
                "actual inspect differs from original consumer CID-file ownership")
        self.cid, self.inspected, self.inspect_receipt = value["Id"], value, receipt
        try:
            self.require_container_context(value, readonly=False, role="consumer")
        except BaseException as error:
            self.inspect_evidence_refusals.append({"role": "consumer", "inspect_receipt": descriptor(receipt),
                                                  "original_error": repr(error)})
            if not allow_evidence_refusal:
                raise
        return value

    def require_owned_labels(self, value, role):
        labels = value.get("Config", {}).get("Labels", {})
        require(all(labels.get(key) == expected for key, expected in self.volume_labels.items())
                and labels.get("layerfs.e04.role") == role,
                "actual inspected container ownership labels differ")

    def require_container_context(self, value, readonly, role):
        self.require_owned_labels(value, role)
        require(value.get("Config", {}).get("WorkingDir") == "/work"
                and "LAYERFS_CONSTRUCTION_WORKERS=1" in value.get("Config", {}).get("Env", [])
                and value.get("HostConfig", {}).get("AutoRemove") is False,
                "actual inspected working directory/environment/retention differs")
        mounts = value.get("Mounts", [])
        require(len(mounts) == 2, "actual container must have exactly the two declared mounts")
        by_path = {item.get("Destination"): item for item in mounts}
        require(set(by_path) == {"/work", NATIVE_ROOT}, "actual inspected mount destinations differ")
        config = value.get("HostConfig", {})
        require(config.get("VolumesFrom") in (None, []) and config.get("Mounts") in (None, [])
                and config.get("Tmpfs") in (None, {}) and value.get("Config", {}).get("Volumes") in (None, {}),
                "alternate or inherited mount mechanism is not the original declared bind")
        binds = config.get("Binds")
        require(isinstance(binds, list) and len(binds) == 2, "exact two original inspected Binds required")
        bind_by_path = {}
        for item in binds:
            require(isinstance(item, str), "inspected Bind is not text")
            fields = item.split(":")
            require(len(fields) in (2, 3) and fields[1] in ("/work", NATIVE_ROOT)
                    and fields[1] not in bind_by_path, "ambiguous or overlapping inspected Bind")
            bind_by_path[fields[1]] = (fields[0], fields[2] if len(fields) == 3 else "rw")
        require(set(bind_by_path) == {"/work", NATIVE_ROOT}, "inspected Binds omit an original destination")
        bind_source, bind_mode = bind_by_path["/work"]
        require(PurePosixPath(bind_source).is_absolute() and ".." not in PurePosixPath(bind_source).parts
                and bind_mode == "rw" and bind_by_path[NATIVE_ROOT] ==
                    (self.volume_name, "ro" if readonly else "rw"), "inspected original Bind source/access differs")
        work, state = by_path["/work"], by_path[NATIVE_ROOT]
        require(work.get("Type") == "bind" and work.get("Source") == bind_source and work.get("RW") is True,
                "actual repository mount differs from its own inspect's opaque Docker Bind source")
        require(self.backing is not None and state.get("Type") == "volume"
                and state.get("Name") == self.volume_name and state.get("Driver") == self.backing["driver"]
                and state.get("Source") == self.backing["mountpoint"] and state.get("RW") is (not readonly),
                "actual inspected original native volume differs")

    def inspect_helper(self, helper, label, identifier, seconds=3, allow_evidence_refusal=False):
        receipt = self.small(label, ["docker", "inspect", identifier], seconds)
        values = read_json(receipt.with_suffix(".stdout"))
        require(isinstance(values, list) and len(values) == 1, "single owned helper inspect required")
        value = values[0]
        require(value.get("Image") == IMAGE and value.get("Name") == "/" + helper["name"]
                and re.fullmatch(r"[0-9a-f]{64}", value.get("Id", "")), "actual helper identity differs")
        if helper["cid"] is not None:
            require(value["Id"] == helper["cid"], "actual inspect substituted original helper CID")
        require(value.get("Config", {}).get("Cmd") == helper["argv"]
                and value.get("Config", {}).get("Entrypoint") in (None, [])
                and value.get("Path") == helper["argv"][0] and value.get("Args") == helper["argv"][1:],
                "actual direct helper process differs")
        self.require_owned_labels(value, helper["phase"])
        require(value.get("HostConfig", {}).get("ContainerIDFile") == str(helper["cid_path"]),
                "actual inspect differs from original helper CID-file ownership")
        helper["cid"], helper["inspected"], helper["inspect_receipt"] = value["Id"], value, receipt
        try:
            self.require_container_context(value, readonly=True, role=helper["phase"])
        except BaseException as error:
            self.inspect_evidence_refusals.append({"role": helper["phase"], "inspect_receipt": descriptor(receipt),
                                                  "original_error": repr(error)})
            if not allow_evidence_refusal:
                raise
        return value

    def dispose_failed_helpers(self):
        for helper in self.helper_containers:
            if helper["inspected"] is not None and helper["inspected"].get("State", {}).get("Running") is False:
                continue  # Original exact inspect already establishes no live process to stop.
            try:
                if helper["cid_path"].is_file():
                    raw = helper["cid_path"].read_text().strip()
                    require(re.fullmatch(r"[0-9a-f]{64}", raw), "owned helper CID file malformed")
                    helper["cid"] = raw
                value = self.inspect_helper(helper, helper["phase"] + "-failed-inspect",
                                            helper["cid"] or helper["name"], 1.5, allow_evidence_refusal=True)
                if value.get("State", {}).get("Running"):
                    self.external_stop = True
                    self.small(helper["phase"] + "-failed-owned-kill",
                               ["docker", "kill", "--signal", "KILL", helper["cid"]], 1.5)
                    value = self.inspect_helper(helper, helper["phase"] + "-after-kill-inspect",
                                                helper["cid"], 1.5, allow_evidence_refusal=True)
                    require(value.get("State", {}).get("Running") is False,
                            "owned helper still running after one explicit stop")
            except BaseException as error:
                self.secondary.append(helper["phase"] + " container stop/outcome UNKNOWN: " + repr(error))

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
        if self.consumer is not None:
            self.dispose_failed_consumer()
        self.dispose_failed_helpers()

    def dispose_failed_consumer(self):
        try:
            if self.cid_path.is_file():
                raw = self.cid_path.read_text().strip()
                require(re.fullmatch(r"[0-9a-f]{64}", raw), "owned CID file malformed")
                self.cid = raw
            if self.inspected is None:
                self.inspect_owned("failed-inspect", self.cid or self.container_name, 1.5, allow_evidence_refusal=True)
            if self.inspected.get("State", {}).get("Running"):
                self.external_stop = True
                self.small("failed-owned-container-kill", ["docker", "kill", "--signal", "KILL", self.cid], 1.5)
                self.inspect_owned("failed-after-kill-inspect", self.cid, 1.5, allow_evidence_refusal=True)
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


def native_container_command(owner, name, cid_path, role, argv, readonly):
    external = ["docker", "run", "--name", name, "--cidfile", str(cid_path)]
    for key, value in owner.volume_labels.items():
        external.extend(["--label", key + "=" + value])
    external.extend(["--label", "layerfs.e04.role=" + role,
                     "-e", "LAYERFS_CONSTRUCTION_WORKERS=1", "-v", str(ROOT) + ":/work",
                     "-v", owner.volume_name + ":/state" + (":ro" if readonly else ""),
                     "-w", "/work", IMAGE, *argv])
    return external


def observe_native_backing(owner, phase):
    helper = {"phase": phase, "name": owner.container_name + "-" + phase,
              "cid_path": owner.run / (phase + ".cid"), "cid": None, "inspected": None}
    helper["argv"] = ["python3", "-B", container_path(BACKING_PROBE), phase,
                      owner.receipt_id, owner.container_name, owner.volume_name]
    external = native_container_command(owner, helper["name"], helper["cid_path"],
                                        phase, helper["argv"], readonly=True)
    # Register exact infrastructure custody before Popen, including deferred signals.
    owner.helper_containers.append(helper)
    receipt = owner.small(phase + "-observation", external, 5)
    require(helper["cid_path"].is_file(), "original helper --cidfile absent")
    helper["cid"] = helper["cid_path"].read_text().strip()
    require(re.fullmatch(r"[0-9a-f]{64}", helper["cid"]), "original helper CID malformed")
    inspected = owner.inspect_helper(helper, phase + "-inspect", helper["cid"])
    require(inspected.get("State", {}).get("Running") is False
            and inspected["State"].get("ExitCode") == 0, "original helper did not exit successfully")
    observed = read_json(receipt.with_suffix(".stdout"), 65536)
    require(observed.get("schema") == "e04-native-backing-observation-v2"
            and observed.get("phase") == phase and observed.get("receipt_id") == owner.receipt_id
            and observed.get("owner_label") == owner.container_name
            and observed.get("volume_name") == owner.volume_name
            and observed.get("root_path") == NATIVE_ROOT and observed.get("database_path") == NATIVE_DATABASE,
            "native observation is not the exact selected original backing")
    require(owner.probe_source is not None and observed.get("probe_source") == {
                "path": container_path(BACKING_PROBE), "bytes": owner.probe_source["bytes"],
                "sha256": owner.probe_source["sha256"]},
            "actual original executing probe source differs from the pre-host source seal")
    require(observed.get("database", {}).get("exists") is (phase == "post-exit"),
            "native observation database existence differs from original phase")
    witness = {"container_id": helper["cid"], "container_name": helper["name"],
               "command_receipt": descriptor(receipt), "inspect_receipt": descriptor(helper["inspect_receipt"]),
               "inspect_stdout": descriptor(helper["inspect_receipt"].with_suffix(".stdout")),
               "cid_file": descriptor(helper["cid_path"]),
               "observation_stdout": descriptor(receipt.with_suffix(".stdout")),
               "probe_source": descriptor(BACKING_PROBE), "probe_argv": helper["argv"]}
    write_json(owner.prep / (phase + "-backing-witness.json"), witness)
    return witness, observed


def prepare_native_backing(owner):
    create_argv = ["docker", "volume", "create"]
    for key, value in owner.volume_labels.items():
        create_argv.extend(["--label", key + "=" + value])
    create_argv.append(owner.volume_name)
    write_json(owner.prep / "native-backing-selection.json", {
        "schema": "e04-native-backing-selection-v1", "receipt_id": owner.receipt_id,
        "owner_label": owner.container_name, "name": owner.volume_name, "labels": owner.volume_labels,
        "kind": "docker-volume", "database_path": NATIVE_DATABASE, "container_root": NATIVE_ROOT,
        "original_create_command": create_argv, "retention": RETENTION,
        "probe_source": descriptor(BACKING_PROBE), "initial_database_required": "ABSENT",
        "filesystem": "UNAVAILABLE-until-original-pre-start-observation", "fallback": "NONE"})
    absence = owner.small("native-volume-absence", ["docker", "volume", "inspect", owner.volume_name],
                          expected_exit=1)
    require(read_json(absence.with_suffix(".stdout")) == []
            and absence.with_suffix(".stderr").read_text().strip() ==
                "Error response from daemon: get " + owner.volume_name + ": no such volume",
            "exact new native volume absence not observed; no creation or replay")
    owner.volume_creation_disposition = "ATTEMPTED_UNKNOWN"
    owner.volume_created = None
    created, create_receipt = owner.text("native-volume-create", create_argv)
    require(created == owner.volume_name, "original volume creation returned a different identity")
    owner.volume_created = True
    inspected_receipt = owner.small("native-volume-inspect", ["docker", "volume", "inspect", owner.volume_name])
    values = read_json(inspected_receipt.with_suffix(".stdout"))
    require(isinstance(values, list) and len(values) == 1, "one original native volume inspect required")
    actual = values[0]
    require(actual.get("Name") == owner.volume_name and actual.get("Labels") == owner.volume_labels
            and actual.get("Driver") == "local" and actual.get("Scope") == "local"
            and actual.get("Options") in (None, {}) and isinstance(actual.get("Mountpoint"), str)
            and actual["Mountpoint"].startswith("/"), "actual created native volume ownership differs")
    owner.volume_creation_disposition = "KNOWN_CREATED_RETAINED"
    owner.backing = {"kind": "docker-volume", "name": owner.volume_name,
                     "driver": actual["Driver"], "mountpoint": actual["Mountpoint"],
                     "database_path": NATIVE_DATABASE, "container_root": NATIVE_ROOT,
                     "receipt_id": owner.receipt_id, "owner_label": owner.container_name,
                     "labels": owner.volume_labels, "retention": RETENTION,
                     "volume_absence_receipt": descriptor(absence),
                     "volume_absence_stdout": descriptor(absence.with_suffix(".stdout")),
                     "volume_absence_stderr": descriptor(absence.with_suffix(".stderr")),
                     "volume_create_receipt": descriptor(create_receipt),
                     "volume_inspect_receipt": descriptor(inspected_receipt),
                     "volume_inspect_stdout": descriptor(inspected_receipt.with_suffix(".stdout"))}
    write_json(owner.prep / "native-backing-created.json", owner.backing)
    owner.pre_start, observation = observe_native_backing(owner, "pre-start")
    return observation


def prepare_disposal_control(owner, binaries, product, host_argv, source_inventory):
    directory = owner.run / "control"
    directory.mkdir(mode=0o700)
    path = owner.prep / "disposal.control"
    fields = ["layerfs-e04-disposal-control-v1", owner.receipt_id, str(directory),
              container_path(directory), "explicit-host-fence-v1"]
    require(all("\n" not in value and "\r" not in value for value in fields), "control config field has newline")
    raw = ("\n".join(fields) + "\n").encode("ascii")
    require(len(raw) <= 8192, "pre-host control configuration exceeds registered byte admission")
    descriptor_fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o444)
    primary = None
    try:
        require(os.write(descriptor_fd, raw) == len(raw), "short original control configuration write; prefix retained")
    except BaseException as error:
        primary = error
    try:
        os.close(descriptor_fd)
    except BaseException as error:
        if primary is None:
            primary = error
        else:
            owner.secondary.append("original control configuration close: " + repr(error))
    if primary is not None:
        raise primary
    require(path.stat().st_mode & 0o222 == 0 and not any(directory.iterdir()),
            "pre-host control configuration must be closed read-only with an empty fresh control directory")
    owner.control = descriptor(path, True)
    owner.probe_source = descriptor(BACKING_PROBE, True)
    require({"path": owner.probe_source["path"], "sha256": owner.probe_source["sha256"]} in source_inventory,
            "probe changed after independent pre-host source-copy retention")
    sealed_binaries = {"host": descriptor(HOST, True), "consumer": descriptor(LINUX, True)}
    require(all(sealed_binaries[key]["sha256"] == binaries[key]["sha256"] for key in binaries),
            "binary changed before original pre-host control seal")
    owner.binary_seals = sealed_binaries
    owner.pre_host_seal = write_json(owner.prep / "pre-host-seal.json", {
        "schema": "e04-pre-host-seal-v3", "at_monotonic_ns": time.monotonic_ns(),
        "receipt_id": owner.receipt_id, "control": owner.control,
        "control_directory": str(directory), "guest_control_directory": container_path(directory),
        "control_directory_empty": True, "probe_source": owner.probe_source,
        "host_argv": host_argv, "image_id": IMAGE,
        "binaries": sealed_binaries, "source": product, "sample_count": 0,
        "qualification_status": "NOT_EVALUATED"})
    return path


def seal_consumer_inputs(owner, assignment, acquisition, identity_path, inputs):
    originals = {"binary": owner.binary_seals["consumer"],
                 "assignment": descriptor(assignment, True), "acquisition": descriptor(acquisition, True),
                 "base": inputs["base.bin"]["copy"], "replacements": inputs["replacements.bin"]["copy"],
                 "identity": descriptor(identity_path, True), "control": owner.control}
    owner.input_seals = {"scope": "original-pre-start-logical-input-bytes"}
    for key, item in originals.items():
        owner.input_seals[key] = {"path": container_path(ROOT / item["path"]),
                                  "bytes": item["bytes"], "sha256": item["sha256"]}
    return write_json(owner.prep / "pre-consumer-input-seals.json", {
        "schema": "e04-independent-input-seals-v3", "at_monotonic_ns": time.monotonic_ns(),
        "receipt_id": owner.receipt_id, "host_inputs": originals, "expected_original_inputs": owner.input_seals})


def check_original_consumer_inputs(owner, output, identity_descriptor):
    manifest = read_json(output / "manifest.json", 65536)
    require(manifest.get("schema") == "cluster-two-job-receipts-v3"
            and manifest.get("driver_version") == "e04-original-write-receipts-v3"
            and manifest.get("pre_start_inputs") == owner.input_seals,
            "original collector input bytes differ from independently sealed pre-consumer inputs")
    invocation = manifest.get("invocation", {})
    require(invocation.get("argv") == owner.consumer_argv
            and invocation.get("bound_before_startup") is True
            and invocation.get("identity_source_sha256") == identity_descriptor["sha256"]
            and invocation.get("identity_source_bytes") == owner.input_seals["identity"]["bytes"]
            and manifest.get("actual_binary_sha256") == owner.binary_seals["consumer"]["sha256"]
            and manifest.get("identity", {}).get("receipt_id") == owner.receipt_id
            and manifest.get("identity", {}).get("control") == owner.control
            and owner.consumer_argv[6] == container_path(output),
            "original input/output/identity invocation differs from the exact submitted logical bind")
    owner.consumer_manifest = descriptor(output / "manifest.json")


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
        require(any(path.is_relative_to(directory) for directory in (CHECKS, NATIVE_CHECKS, DISPOSAL_CHECKS))
                and descriptor(path) == item, "additional build receipt identity changed/outside owning E04 directories")
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
    host_output, output = owner.run / "host-output", owner.run / "consumer-output"
    identity_path = owner.prep / "identity-input.json"
    control_path = owner.prep / "disposal.control"
    host_argv = [str(HOST), str(store), str(source), str(assignment), str(owner.prep / "base.bin"),
                 owner.profile, str(host_output), "host.docker.internal", "80", str(control_path)]
    require(prepare_disposal_control(owner, binaries, product, host_argv, current_sources) == control_path,
            "explicit pre-host control configuration path differs")
    pre_observation = prepare_native_backing(owner)
    write_json(owner.prep / "host-command.json", {"schema": "e04-host-command-v3", "command": host_argv,
               "control": owner.control, "construction_environment": {"LAYERFS_CONSTRUCTION_WORKERS": "1"},
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
                     str(owner.prep / "replacements.bin"), None, str(output), str(identity_path), str(control_path)]
    argv = [NATIVE_DATABASE if index == 5 else value if index == 1 else container_path(value)
            for index, value in enumerate(consumer_host)]
    owner.consumer_argv = argv
    external = native_container_command(owner, owner.container_name, owner.cid_path,
                                        "consumer", argv, readonly=False)
    command = write_json(owner.prep / "consumer-command.json", {
        "schema": "cluster-two-e2-command-v1", "execution_kind": "docker", "argv": argv,
        "image_id": IMAGE, "external_argv": external, "construction_environment": {"LAYERFS_CONSTRUCTION_WORKERS": "1"}})
    identity = {"schema": "cluster-two-e2-identity-v1", "receipt_id": owner.receipt_id,
                "control": owner.control,
                "source_commit": head, "source_tree": tree, "source_sealed": False, "source_arm": "diagnostic",
                "profile": "local-overlay-disposable", "cache_state": "uncontrolled", "execution_kind": "docker",
                "os": "linux", "architecture": "aarch64", "kernel": docker_info["KernelVersion"],
                "topology": {"status": "OBSERVED", "artifact": topology}, "image": image, "command_artifact": command,
                "artifacts": {"product": product, "toolchain": toolchain, "cargo_config": descriptor(ROOT / ".cargo/config.toml"),
                              "cargo_lock": descriptor(ROOT / "core/Cargo.lock"), "dependencies": dependencies,
                              "binary": binaries["consumer"], "driver": driver, "fixture": fixture,
                              "trace": trace, "cache": cache, "observer": descriptor(ORACLE)}}
    identity_descriptor = write_json(identity_path, identity, 16384)
    input_seals = seal_consumer_inputs(owner, assignment, acquisition, identity_path, inputs)
    write_json(owner.prep / "pre-consumer-seal.json", {"schema": "e04-pre-consumer-seal-v3",
               "at_monotonic_ns": time.monotonic_ns(), "identity": identity_descriptor, "fixture": fixture,
               "source": product, "binaries": binaries, "host_pid": owner.host.process.pid,
               "host_ready_endpoint": descriptor(endpoint_path), "output_not_created": not output.exists(),
               "database_not_created": pre_observation["database"]["exists"] is False,
               "native_backing": owner.backing, "pre_start": owner.pre_start,
               "control": owner.control, "pre_host_seal": owner.pre_host_seal, "input_seals": input_seals,
               "sample_count": 0, "qualification_status": "NOT_EVALUATED"})
    require(not output.exists() and pre_observation["database"]["exists"] is False,
            "original consumer output/database already exists")
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
    # Product work remains cut off at 80 seconds. Independent terminal observation
    # uses only the already-declared remainder of the 88/90-second complete budget.
    signal.setitimer(signal.ITIMER_REAL, owner.remaining())
    require(owner.cid_path.is_file(), "original --cidfile absent")
    owner.cid = owner.cid_path.read_text().strip()
    require(re.fullmatch(r"[0-9a-f]{64}", owner.cid), "original CID malformed")
    inspected = owner.inspect_owned("consumer-inspect", owner.cid)
    require(inspected.get("State", {}).get("Running") is False and inspected["State"].get("ExitCode") == 0,
            "actual inspected collector not successfully exited")
    check_original_consumer_inputs(owner, output, identity_descriptor)
    owner.post_exit, post_observation = observe_native_backing(owner, "post-exit")
    require(post_observation["root_identity"] == pre_observation["root_identity"]
            and all(post_observation["filesystem"][key] == pre_observation["filesystem"][key]
                    for key in ("type", "magic")), "original guest volume identity/filesystem changed")
    original_stat = read_json(output / "outcomes.json")["database_artifact"]
    require(original_stat["path"] == NATIVE_DATABASE
            and all(original_stat[key] == post_observation["database"][key]
                    for key in ("logical_bytes", "allocated_bytes", "device", "inode", "links")),
            "independent same-guest artifact observation differs from original collector stat")
    mapping = write_json(owner.run / "docker-path-mapping.json", {
        "schema": "cluster-two-e2-docker-path-mapping-v3", "status": "OBSERVED", "scope": "diagnostic-input-path-binding",
        "source": "exact submitted repository bind, opaque same-inspect Docker bind source and original pre-start input bytes; same-volume guest observations; no host/guest physical equivalence",
        "container_id": owner.cid, "image_id": IMAGE, "host_root": str(ROOT), "container_root": "/work",
        "collector_argv": argv, "host_argv": consumer_host,
        "backing": owner.backing, "pre_start": owner.pre_start, "post_exit": owner.post_exit, "logical_export": None,
        "application_disposal": {"host_log": descriptor(host_output / "host.jsonl"),
                                 "host_command_receipt": descriptor(owner.host.path),
                                 "pre_host_seal": owner.pre_host_seal},
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
            "store": str(store), "database": NATIVE_DATABASE, "database_domain": "original-retained-guest-volume",
            "native_volume": owner.volume_name, "native_volume_retention": RETENTION,
            "logical_export": None, "raw_host_and_consumer_exit": "SUCCESS",
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
        outcome = {"schema": "e04-one-original-owner-diagnostic-v3", "mode": "diagnostic", "profile": args.profile,
                   "started_utc": owner.started_utc, "command": sys.argv,
                   "owner_prefix_wall_ns": math.ceil(elapsed * 1e9), "complete_command_limit_ns": 90_000_000_000,
                   "owner_prefix_scope": "helper-start-through-before-terminal-record-encoding-write-hash-print",
                   "complete_command_status": "UNAVAILABLE-requires-enclosing-90-second-process-receipt",
                   "product_cutoff_seconds": 80, "status": "RAW_EXECUTION_SUCCESS" if primary is None else "FAILED",
                   "original_error": primary, "secondary_infrastructure_errors": owner.secondary,
                   "container_id": owner.cid, "container_known_removed": owner.removed,
                   "container_removal_disposition": owner.removal_disposition,
                   "container_last_observed_inspect_state": owner.inspected.get("State") if owner.inspected else None,
                   "inspect_evidence_refusals": owner.inspect_evidence_refusals,
                   "control_configuration": owner.control,
                   "pre_host_seal": owner.pre_host_seal,
                   "native_volume": owner.volume_name, "native_volume_created": owner.volume_created,
                   "native_volume_creation_disposition": owner.volume_creation_disposition,
                   "native_volume_retention": RETENTION, "native_backing": owner.backing,
                   "pre_start_backing_witness": owner.pre_start, "post_exit_backing_witness": owner.post_exit,
                   "helper_containers": [{"phase": item["phase"], "name": item["name"], "cid": item["cid"],
                       "last_observed_inspect_state": item["inspected"].get("State") if item["inspected"] else None,
                       "retention": "RETAIN_CONTAINER_AND_ORIGINAL_VOLUME"} for item in owner.helper_containers],
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
