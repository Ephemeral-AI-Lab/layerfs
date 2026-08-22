#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import os
import secrets
import subprocess
import time
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V8_RUNNER = HERE.parent / "v8/run_g4_v8.py"
EXPECTED_V8_RUNNER = "22e924e37ddba807917818acefeffe1c7feeec290b1ab64847c2d9e3dfa14de4"
CANDIDATE_SHA256 = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
SOURCE_HASHES = {
    REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs": "01886da1d413ce73bbeba38f1b5cbc45a939e9d50e69fa7273c1af33f65554cb",
    REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs": "320ecb529c11de4464ce9a76ce97cc11f60d719d418f33a40d945e5f6dde196a",
    REPO / "crates/layerfs-core/src/canonical_v2.rs": "8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc",
    REPO / "Cargo.lock": "70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8",
}
ESTIMATED = (8, 16, 17, 18, 19, 20, 22, 24, 25, 26, 27, 29, 30)

if hashlib.sha256(V8_RUNNER.read_bytes()).hexdigest() != EXPECTED_V8_RUNNER:
    raise SystemExit("frozen v8 runner custody mismatch")
spec = importlib.util.spec_from_file_location("phase4_g4_frozen_v8_runner", V8_RUNNER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
runner = module.runner
runner.HERE = HERE
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v12.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v12"
runner.RESULTS = runner.TARGET / "results-v12"
runner.WORK = runner.TARGET / "work-v12"
runner.HASHES["candidate"] = CANDIDATE_SHA256
runner.BUCKET_LIMITS = {
    "lock_and_measured_preflight": 1_000_000_000,
    "private_base_and_shared_preparation": 75_000_000_000,
    "row_dispatch_and_measured_operations": 38_000_000_000,
    "exact_row_verification": 1_000_000_000,
    "primary_and_independent_analysis": 1_000_000_000,
    "cleanup_storage_and_mode_audit": 1_000_000_000,
    "payload_manifest_terminal_and_verification": 3_000_000_000,
}
if sum(runner.BUCKET_LIMITS.values()) != runner.CEILING_NS:
    raise SystemExit("v12 bucket partition must sum to the 120-second ceiling")

ACTUAL_LOCK = module.module.ACTUAL_LOCK
lock_state = {"fd": None, "device": None, "inode": None, "token": None, "content": None, "legacy_fd": None, "release_started": False}
child_records = []
pending_candidates = {}
prepared_skip = set()
bucket_state = {}
prepared_source_hashes = {}
original_run_capture = runner.run_capture
original_verify_methodology = runner.verify_methodology


class TrackedBuckets(runner.Buckets):
    def __init__(self, started_ns):
        super().__init__(started_ns)
        bucket_state["instance"] = self


runner.Buckets = TrackedBuckets


class OwnedHeldLock:
    def __fspath__(self):
        return os.fspath(ACTUAL_LOCK)

    def __str__(self):
        return str(ACTUAL_LOCK)

    @property
    def parent(self):
        return ACTUAL_LOCK.parent

    def exists(self):
        return ACTUAL_LOCK.exists()

    def read_text(self, *args, **kwargs):
        return ACTUAL_LOCK.read_text(*args, **kwargs)

    def unlink(self):
        return None


runner.GLOBAL_LOCK = OwnedHeldLock()


def checked_pwrite(fd, content):
    written = 0
    while written < len(content):
        amount = os.pwrite(fd, content[written:], written)
        if amount <= 0:
            raise OSError("short lock-attestation write")
        written += amount
    os.ftruncate(fd, len(content))


class CustodyOs:
    def __init__(self, delegate):
        self.delegate = delegate

    def __getattr__(self, name):
        return getattr(self.delegate, name)

    def open(self, path, flags, mode=0o777, *, dir_fd=None):
        actual = Path(os.fspath(path))
        acquiring = actual == ACTUAL_LOCK and flags & os.O_CREAT and flags & os.O_EXCL
        if not acquiring:
            return self.delegate.open(path, flags, mode, dir_fd=dir_fd)
        retained = self.delegate.open(path, (flags & ~os.O_WRONLY) | os.O_RDWR, mode, dir_fd=dir_fd)
        lock_state["fd"] = retained
        descriptor = self.delegate.fstat(retained)
        token = secrets.token_hex(32)
        content = (json.dumps({"schema": "phase4-g4-v12-lock-attestation", "state": "held", "pid": os.getpid(), "token": token}, sort_keys=True) + "\n").encode()
        lock_state.update({"device": descriptor.st_dev, "inode": descriptor.st_ino, "token": token, "content": content})
        checked_pwrite(retained, content)
        self.delegate.fsync(retained)
        runner.fsync_dir(ACTUAL_LOCK.parent)
        legacy = self.delegate.dup(retained)
        lock_state["legacy_fd"] = legacy
        return legacy

    def write(self, fd, content):
        if fd == lock_state["legacy_fd"]:
            return len(content)
        return self.delegate.write(fd, content)

    def close(self, fd):
        if fd == lock_state["legacy_fd"]:
            lock_state["legacy_fd"] = None
        return self.delegate.close(fd)


runner.os = CustodyOs(os)


def verify_methodology_and_sources():
    retained = lock_state["fd"]
    if retained is None:
        raise RuntimeError("benchmark lock custody was not established at O_EXCL")
    descriptor = os.fstat(retained)
    named = os.stat(ACTUAL_LOCK, follow_symlinks=False)
    content = os.pread(retained, len(lock_state["content"]), 0)
    if (descriptor.st_dev, descriptor.st_ino) != (named.st_dev, named.st_ino) or content != lock_state["content"] or descriptor.st_size != len(content):
        raise RuntimeError("benchmark lock identity or token changed before methodology")
    rows = original_verify_methodology()
    for path, expected in SOURCE_HASHES.items():
        runner.verify_file(path, expected)
    runner.verify_file(runner.CANDIDATE, CANDIDATE_SHA256)
    return rows


runner.verify_methodology = verify_methodology_and_sources


def seal_owned_lock(disposition):
    if lock_state["release_started"]:
        return None
    lock_state["release_started"] = True
    retained = lock_state["fd"]
    if retained is None:
        return None
    current = None
    try:
        descriptor = os.fstat(retained)
        lock_state["device"] = descriptor.st_dev
        lock_state["inode"] = descriptor.st_ino
        if lock_state["token"] is None:
            lock_state["token"] = secrets.token_hex(32)
        attestation = {
            "schema": "phase4-g4-v12-lock-attestation",
            "state": disposition,
            "pid": os.getpid(),
            "token": lock_state["token"],
            "device": lock_state["device"],
            "inode": lock_state["inode"],
        }
        content = (json.dumps(attestation, sort_keys=True) + "\n").encode()
        checked_pwrite(retained, content)
        os.fsync(retained)
        current = os.open(ACTUAL_LOCK, os.O_RDONLY | os.O_NOFOLLOW)
        descriptor = os.fstat(current)
        named_content = os.pread(current, len(content), 0)
        owned = (
            descriptor.st_dev == lock_state["device"]
            and descriptor.st_ino == lock_state["inode"]
            and descriptor.st_size == len(content)
            and named_content == content
        )
        if not owned:
            return {"released": False, "attested": False, "reason": "lock-identity-or-token-mismatch"}
        destination = runner.RESULTS / "BENCHMARK-LOCK-RELEASE-ATTESTATION-v12.json" if disposition == "release" and runner.RESULTS.is_dir() else REPO / "target" / f"BENCHMARK-LOCK-FAILURE-ATTESTATION-v12-{lock_state['token'][:16]}.json"
        if destination.exists():
            return {"released": False, "attested": False, "reason": "lock-attestation-destination-exists"}
        os.rename(ACTUAL_LOCK, destination)
        runner.fsync_dir(ACTUAL_LOCK.parent)
        if destination.parent != ACTUAL_LOCK.parent:
            runner.fsync_dir(destination.parent)
        os.fchmod(retained, 0o400)
        os.fsync(retained)
        renamed = os.stat(destination, follow_symlinks=False)
        verified = (
            not ACTUAL_LOCK.exists()
            and (renamed.st_dev, renamed.st_ino) == (lock_state["device"], lock_state["inode"])
            and os.pread(retained, len(content), 0) == content
            and renamed.st_size == len(content)
        )
        return {
            "released": disposition == "release" and verified,
            "attested": verified,
            "disposition": disposition,
            "device": lock_state["device"],
            "inode": lock_state["inode"],
            "token_sha256": hashlib.sha256(lock_state["token"].encode()).hexdigest(),
            "attestation_path": str(destination),
            "attestation_sha256": runner.sha256(destination) if verified else None,
            "lock_absent_after_release": not ACTUAL_LOCK.exists(),
        }
    finally:
        if current is not None:
            os.close(current)
        os.close(retained)
        lock_state["fd"] = None


def protected_ns(sequence, payload):
    if 16 <= sequence <= 27:
        return payload["operation_total_ns"]
    if sequence == 8:
        return payload["range_measurements"][0]["wall_ns"]
    if sequence == 30:
        return payload["fresh_reopen_head_wall_ns"]
    return payload["durable_capture_total_wall_ns"]


def set_protected_ns(sequence, payload, value):
    if 16 <= sequence <= 27:
        payload["operation_total_ns"] = value
    elif sequence == 8:
        payload["range_measurements"] = [dict(payload["range_measurements"][0], wall_ns=value)]
    elif sequence == 30:
        payload["fresh_reopen_head_wall_ns"] = value
    else:
        payload["durable_capture_total_wall_ns"] = value


def aggregate_external(samples):
    total = lambda key: sum(sample[key] for sample in samples)
    switches = lambda key: total(key) if all(isinstance(sample[key], int) for sample in samples) else "Unavailable"
    return {
        "external_real_seconds": total("external_real_seconds"),
        "external_user_seconds": total("external_user_seconds"),
        "external_system_seconds": total("external_system_seconds"),
        "maximum_resident_set_bytes": max(sample["maximum_resident_set_bytes"] for sample in samples),
        "voluntary_context_switches": switches("voluntary_context_switches"),
        "involuntary_context_switches": switches("involuntary_context_switches"),
    }


def run_measured(command, label, directory, env, sequence, role, sample):
    started = time.monotonic_ns()
    completed, external = original_run_capture(command, label, directory, env, True, False)
    ended = time.monotonic_ns()
    stdout = Path(directory) / f"{label}.stdout"
    stderr = Path(directory) / f"{label}.stderr"
    record = {
        "order": len(child_records) + 1,
        "sequence": sequence,
        "role": role,
        "sample": sample,
        "label": label,
        "command": [str(item) for item in command],
        "binary_sha256": runner.sha256(command[0]),
        "started_monotonic_ns": started,
        "ended_monotonic_ns": ended,
        "stdout_sha256": runner.sha256(stdout),
        "stderr_sha256": runner.sha256(stderr),
        "external": external,
    }
    child_records.append(record)
    runner.append_jsonl(
        runner.RESULTS / "CHRONOLOGY-v1.jsonl",
        {"event": "measured-child-complete", **record},
    )
    return completed, external, record


def fast_database(command, iteration):
    internal = {"read-range-1m": "read-range-1m", "write": "full", "edit-same": "same-middle", "reopen": "reopen"}
    return Path(command[2]) / f"db-K64-F64-{command[3]}-{internal[command[4]]}-{iteration}.sqlite"


def prepare_fast(command, iteration, label):
    buckets = bucket_state.get("instance")
    if buckets is None:
        raise RuntimeError("tracked bucket authority unavailable")
    prior = buckets.current
    buckets.switch("private_base_and_shared_preparation")
    try:
        source = fast_database(command, int(command[5]))
        destination = fast_database(command, iteration)
        custody = []
        for suffix in ("", ".authority", ".expectations"):
            source_file = Path(f"{source}{suffix}")
            destination_file = Path(f"{destination}{suffix}")
            if destination_file.exists():
                raise RuntimeError(f"clone preparation destination exists: {destination_file}")
            source_hash = prepared_source_hashes.setdefault(str(source_file), runner.sha256(source_file))
            subprocess.run(["cp", "-c", source_file, destination_file], check=True, capture_output=True, text=True)
            with destination_file.open("rb") as handle:
                os.fsync(handle.fileno())
            destination_hash = runner.sha256(destination_file)
            if destination_hash != source_hash or source_file.stat().st_ino == destination_file.stat().st_ino:
                raise RuntimeError(f"clone preparation custody mismatch: {destination_file}")
            custody.append({"source": str(source_file), "destination": str(destination_file), "sha256": destination_hash, "size_bytes": destination_file.stat().st_size})
        runner.fsync_dir(destination.parent)
        runner.write_text(
            runner.RESULTS / f"preparation-v1/{label}.stdout",
            json.dumps({"schema": "phase4-g4-v12-private-clone-preparation", "copy_method": "apfs-cp-c", "files": custody}, sort_keys=True) + "\n",
        )
        runner.write_text(runner.RESULTS / f"preparation-v1/{label}.stderr", "")
    finally:
        buckets.switch(prior)


def fast_env(base, command, iteration, digest):
    database = fast_database(command, iteration)
    value = dict(base)
    value.update(
        {
            "WP4M_EXECUTABLE_SHA256": digest,
            "WP4M_BASE_DATABASE_SHA256": runner.sha256(database),
            "WP4M_BASE_AUTHORITY_SHA256": runner.sha256(Path(f"{database}.authority")),
            "WP4M_BASE_EXPECTATIONS_SHA256": runner.sha256(Path(f"{database}.expectations")),
        }
    )
    return value


def aggregate_role(sequence, role, aggregate_label, command, samples):
    payloads = [runner.child_json(sample[0]) for sample in samples]
    values = [protected_ns(sequence, payload) for payload in payloads]
    payload = dict(payloads[0])
    set_protected_ns(sequence, payload, (sum(values) + 1) // 2)
    order_name = "ABBA" if ESTIMATED.index(sequence) % 2 == 0 else "BAAB"
    payload["adjacent_estimator_v12"] = {
        "schema": "phase4-g4-adjacent-estimator-v12",
        "replications_per_role": 2,
        "estimator": "equal-weight-arithmetic-mean",
        "relative_limit_basis_points": 10_500,
        "balanced_order": order_name,
        "samples_ns": values,
        "sum_ns": sum(values),
        "mean_ns_ceil": (sum(values) + 1) // 2,
        "sample_payload_paths": [f"arm-raw-v1/{sample[2]['label']}.stdout" for sample in samples],
        "sample_payload_sha256": [sample[2]["stdout_sha256"] for sample in samples],
        "sample_commands": [sample[2]["command"] for sample in samples],
        "sample_order": [sample[2]["order"] for sample in samples],
        "sample_external": [sample[1] for sample in samples],
    }
    encoded = json.dumps(payload, separators=(",", ":"), sort_keys=True) + "\n"
    runner.write_text(runner.RESULTS / f"arm-raw-v1/{aggregate_label}.stdout", encoded)
    runner.write_text(
        runner.RESULTS / f"arm-raw-v1/{aggregate_label}.stderr",
        json.dumps({"schema": "phase4-g4-v12-aggregate-stderr", "role": role, "sample_order": payload["adjacent_estimator_v12"]["sample_order"]}, sort_keys=True) + "\n",
    )
    completed = subprocess.CompletedProcess([str(item) for item in command], 0, stdout=encoded, stderr="")
    return completed, aggregate_external([sample[1] for sample in samples])


def balanced_quartet(command, label, directory, env, sequence):
    is_g3 = "--g3-row" in command
    control_role = "g3-control" if is_g3 else "protected-control"
    candidate_role = "s1-candidate" if is_g3 else "protected-candidate"
    candidate_label = label.replace(control_role, candidate_role)
    control1 = [str(item) for item in command]
    control2 = control1.copy()
    candidate1 = control1.copy()
    candidate2 = control1.copy()
    candidate1[0] = str(runner.RESULTS / "operands-v1/phase4_create_edit_benchmark-g4")
    candidate2[0] = candidate1[0]
    control_env1 = dict(env)
    control_env2 = dict(env)
    candidate_env1 = dict(env)
    candidate_env2 = dict(env)
    candidate_env1["WP4M_EXECUTABLE_SHA256"] = CANDIDATE_SHA256
    candidate_env2["WP4M_EXECUTABLE_SHA256"] = CANDIDATE_SHA256
    if is_g3:
        control2[2] = f"{control1[2]}-sample-2"
        candidate1[2] = control1[2].replace(control_role, candidate_role)
        candidate2[2] = f"{candidate1[2]}-sample-2"
    else:
        control_iteration = int(control1[5])
        candidate_iteration = control_iteration + 1
        control2[5] = str(control_iteration + 100_000)
        candidate1[5] = str(candidate_iteration)
        candidate2[5] = str(candidate_iteration + 100_000)
        prepare_fast(control1, int(control2[5]), f"prepare-{sequence:02d}-{control_role}-sample-2")
        candidate_prepare_label = f"prepare-{sequence:02d}-{candidate_role}"
        prepare_fast(control1, candidate_iteration, candidate_prepare_label)
        prepared_skip.add(candidate_prepare_label)
        prepare_fast(control1, int(candidate2[5]), f"prepare-{sequence:02d}-{candidate_role}-sample-2")
        control_env2 = fast_env(control_env2, control2, int(control2[5]), runner.HASHES["protected_control"])
        candidate_env1 = fast_env(candidate_env1, candidate1, candidate_iteration, CANDIDATE_SHA256)
        candidate_env2 = fast_env(candidate_env2, candidate2, int(candidate2[5]), CANDIDATE_SHA256)
    commands = {
        "C1": (control1, f"{label}-sample-1", control_env1, control_role, 1),
        "C2": (control2, f"{label}-sample-2", control_env2, control_role, 2),
        "P1": (candidate1, f"{candidate_label}-sample-1", candidate_env1, candidate_role, 1),
        "P2": (candidate2, f"{candidate_label}-sample-2", candidate_env2, candidate_role, 2),
    }
    order = ("C1", "P1", "P2", "C2") if ESTIMATED.index(sequence) % 2 == 0 else ("P1", "C1", "C2", "P2")
    results = {}
    for key in order:
        item = commands[key]
        results[key] = run_measured(item[0], item[1], directory, item[2], sequence, item[3], item[4])
    control = aggregate_role(sequence, control_role, label, control1, [results["C1"], results["C2"]])
    candidate = aggregate_role(sequence, candidate_role, candidate_label, candidate1, [results["P1"], results["P2"]])
    pending_candidates[sequence] = candidate
    return control


def v12_run_capture(command, label, directory, env=None, timed=True, allow_nonzero=False):
    if not timed and label in prepared_skip:
        stdout = runner.RESULTS / f"preparation-v1/{label}.stdout"
        stderr = runner.RESULTS / f"preparation-v1/{label}.stderr"
        return subprocess.CompletedProcess(command, 0, stdout=stdout.read_text(), stderr=stderr.read_text()), None
    sequence = int(label[:2]) if len(label) >= 2 and label[:2].isdigit() else None
    if timed and sequence in pending_candidates and ("s1-candidate" in label or "protected-candidate" in label):
        return pending_candidates.pop(sequence)
    if timed and sequence in ESTIMATED and ("g3-control" in label or "protected-control" in label):
        return balanced_quartet(command, label, directory, env or os.environ.copy(), sequence)
    if timed and sequence is not None and ("--g3-row" in command or "--fast-row" in command or "--g4-row" in command):
        roles = (
            "r1-attribution-control", "protected-candidate", "protected-control",
            "s1-candidate", "g3-control", "m0-candidate", "m0-control",
            "r1-candidate", "r0-control",
        )
        role = next((item for item in roles if label.endswith(f"-{item}")), None)
        if role is None:
            raise RuntimeError(f"unrecognized measured child role: {label}")
        completed, external, _ = run_measured(command, label, directory, env or os.environ.copy(), sequence, role, 1)
        return completed, external
    return original_run_capture(command, label, directory, env, timed, allow_nonzero)


runner.run_capture = v12_run_capture


def completed_bucket_overruns():
    buckets = bucket_state.get("instance")
    if buckets is None:
        raise RuntimeError("tracked bucket authority unavailable")
    return {
        name: {"actual_ns": value, "limit_ns": runner.BUCKET_LIMITS[name]}
        for name, value in buckets.values.items()
        if name != buckets.current and value > runner.BUCKET_LIMITS[name]
    }


def v12_write_json(path, value):
    path = Path(path)
    if path.name == "COMMANDS-v1.json":
        if len(child_records) != 76 or pending_candidates:
            raise RuntimeError(f"expected 76 measured child commands and no pending candidates, got {len(child_records)} and {sorted(pending_candidates)}")
        value = child_records
    if path.name == "CLEANUP-v1.json":
        value["declared_deleted_root"] = runner.WORK.name
    if path.name == "MEASURED-TERMINAL-v1.json":
        value["measured_payload_observations"] = len(child_records)
        value["balanced_estimator_routes"] = len(ESTIMATED)
        value["completed_bucket_overruns"] = completed_bucket_overruns()
        if value["completed_bucket_overruns"]:
            value["issues"] = sorted(set(value.get("issues", [])) | {f"bucket-overrun-{name}" for name in value["completed_bucket_overruns"]})
            value["status"] = "REVISE"
            value["disposition"] = "G4 REVISE"
            value["g5_eligible_after_static_and_final_audits"] = False
    if path.name == "MEASURED-TERMINAL-VERIFICATION-v1.json":
        value["completed_bucket_overruns"] = completed_bucket_overruns()
        terminal = json.loads((runner.RESULTS / "MEASURED-TERMINAL-v1.json").read_text())
        value["status"] = terminal["status"]
        value["lock_absent"] = False
        value["lock_held_through_terminal_verification_fsync"] = True
        source_custody = {}
        for source, expected in SOURCE_HASHES.items():
            runner.verify_file(source, expected)
            source_custody[str(source.relative_to(REPO))] = expected
        operand_custody = {
            "live_candidate": (runner.CANDIDATE, CANDIDATE_SHA256),
            "measured_candidate": (runner.RESULTS / "operands-v1/phase4_create_edit_benchmark-g4", CANDIDATE_SHA256),
            "measured_g3_control": (runner.RESULTS / "operands-v1/phase4_create_edit_benchmark-g3-control", runner.HASHES["g3_control"]),
            "measured_protected_control": (runner.RESULTS / "operands-v1/phase4_create_edit_benchmark-protected-control", runner.HASHES["protected_control"]),
        }
        for operand, (target, expected) in operand_custody.items():
            runner.verify_file(target, expected)
            operand_custody[operand] = {"path": str(target), "sha256": expected}
        value["terminal_source_custody"] = source_custody
        value["terminal_operand_custody"] = operand_custody
        module.module.original_write_json(path, value)
        release = seal_owned_lock("release")
        if not release or not release.get("released"):
            raise RuntimeError(f"owner-bound lock release unresolved: {release}")
        module.module.original_write_json(
            runner.RESULTS / "LOCK-RELEASE-v12.json",
            {
                "schema": "phase4-g4-v12-lock-release-v1",
                "status": value["status"],
                "terminal_verification_sha256": runner.sha256(path),
                "lock_held_through_terminal_verification_fsync": True,
                **release,
                "release_monotonic_ns": time.monotonic_ns(),
            },
        )
        return
    if path.name == "COMPLETE-WALL-v1.json":
        all_overruns = {
            name: {"actual_ns": actual, "limit_ns": runner.BUCKET_LIMITS[name]}
            for name, actual in value["buckets_ns"].items()
            if actual > runner.BUCKET_LIMITS[name]
        }
        value["bucket_overruns"] = all_overruns
        value["status"] = json.loads((runner.RESULTS / "MEASURED-TERMINAL-v1.json").read_text())["status"] if not all_overruns else "REVISE"
    module.module.original_write_json(path, value)


runner.write_json = v12_write_json

if __name__ == "__main__":
    try:
        status = runner.main()
        wall = json.loads((runner.RESULTS / "COMPLETE-WALL-v1.json").read_text())
        overruns = [name for name, value in wall["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
        cleanup = json.loads((runner.RESULTS / "CLEANUP-v1.json").read_text())
        release = json.loads((runner.RESULTS / "LOCK-RELEASE-v12.json").read_text())
        if overruns or cleanup.get("declared_deleted_root") != "work-v12" or not release.get("lock_absent_after_release"):
            raise SystemExit(f"v12 wrapper closure mismatch: {overruns}")
        raise SystemExit(status)
    finally:
        if lock_state["fd"] is not None and not lock_state["release_started"]:
            seal_owned_lock("failure")
