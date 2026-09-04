#!/usr/bin/env python3
"""Validate sealed Phase 1 evidence without executing product workloads."""
import argparse
from collections import Counter, defaultdict
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import statistics
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent

def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value

custody = module("phase1_report_custody", "sdk-edit-custody.py")
runner = module("phase1_report_runner", "workspace-runner.py")
GIB = 1024 ** 3
FAMILY_COUNTS = dict(zip(("payload_create_read", "tiny_file_churn", "directory_construction_traversal", "git_tool_workflow", "namespace_mutation", "workspace_change_locality", "mixed_load_bearing", "dedup_cross_file", "dedup_cdc_locality", "dedup_workspace_reuse", "dedup_branch_history"), (8, 20, 12, 4, 4, 16, 4, 10, 20, 12, 20)))
CLEAN = {"workspace-clean-commit", "payload-random-read", "tiny-stat", "directory-metadata-scan", "directory-content-scan"}
FUSE_CALLBACKS = "lookup getattr setattr readlink mknod mkdir unlink rmdir symlink rename link open read write flush release fsync opendir readdir readdirplus releasedir fsyncdir statfs access create".split()
IDENTITY_FIELDS = {"product_identity": "product_seal", "harness_identity": "harness_seal", "source_revision": "revision", "image_id": "image_id", "contract_commit": "phase1_contract_commit"}
VERIFY_KINDS = {"canonical-verification", "native-verification", "verification-complete", "dedup-verification", "capped-verification", "history-transcript", "history-accounting", "git-semantic-verification", "git-precommit-custody", "git-reopen-custody", "proof-start", "fault-reachability", "transaction-fault-reachability"}


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON field: {key}")
        value[key] = item
    return value


def decode(text):
    return json.loads(text, object_pairs_hook=unique_object)


def read(path):
    return decode(Path(path).read_text())


def raw(path):
    rows = []
    with Path(path).open() as stream:
        for ordinal, line in enumerate(stream, 1):
            if not line.strip():
                continue
            value = decode(line.removeprefix("RELIABILITY\t"))
            if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
                raise ValueError(f"raw row {ordinal} lacks typed kind")
            rows.append(value)
    return rows


def number(value, name):
    if type(value) is not int or value < 0:
        raise ValueError(f"{name} must be a nonnegative integer")
    return value


def digest(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def require(condition, message, issues):
    if not condition:
        issues.append(message)


def receipt(value):
    if isinstance(value, dict):
        return value
    if not isinstance(value, str):
        raise ValueError("missing typed receipt")
    if value.startswith("{"):
        result = decode(value)
        if not isinstance(result, dict):
            raise ValueError("receipt must be an object")
        return result
    result = {}
    for line in value.splitlines():
        key, separator, item = line.partition("=")
        if not separator or not key or key in result:
            raise ValueError("malformed or duplicate workload receipt field")
        result[key] = int(item) if item.isdecimal() else item
    return result


def validate_input_manifest(text):
    lines = text.splitlines()
    if not lines or lines[0] != "workspace-independent-manifest-v1":
        raise ValueError("unknown prepared input manifest")
    entries = {}
    for line in lines[1:]:
        parts = line.split("\t")
        if len(parts) != 7:
            raise ValueError("input manifest column count")
        path, kind, length, mode, seconds, nanos, identity = parts
        if path in entries or not path or (path != "." and any(part in {"", ".", ".."} for part in path.split("/"))):
            raise ValueError("duplicate/noncanonical input path")
        length, mode, seconds, nanos = int(length), int(mode, 8), int(seconds), int(nanos)
        if length < 0 or length > 524_288_000 or mode < 0 or mode > 0o7777 or nanos < 0 or nanos >= 1_000_000_000:
            raise ValueError("input manifest file/metadata bound")
        if kind == "file":
            if not digest(identity): raise ValueError("input content hash missing")
        elif kind == "directory":
            if length or identity != "-": raise ValueError("directory manifest encoding")
        elif kind in {"symlink", "hardlink"}:
            identity = bytes.fromhex(identity).decode("utf-8")
            if "\0" in identity or (kind == "symlink" and len(identity.encode()) != length) or (kind == "hardlink" and length):
                raise ValueError("link manifest encoding")
        else:
            raise ValueError("unrecognized input type")
        entries[path] = (kind, length, identity)
    if entries.get(".", (None,))[0] != "directory":
        raise ValueError("input manifest lacks root")
    total = files = 0
    for path, (kind, length, identity) in entries.items():
        if path != "." and entries.get(str(Path(path).parent), (None,))[0] != "directory":
            raise ValueError("input path parent is not a directory")
        if kind == "hardlink":
            if entries.get(identity, (None,))[0] != "file": raise ValueError("hardlink target is not a file")
            length = entries[identity][1]
        if kind in {"file", "hardlink"}: files += 1
        total += length
    if total >= GIB:
        raise ValueError("input workload logical total reaches1GiB")
    return total, files


def debug_numbers(text):
    """Read only named integer fields from the source-sealed Rust Debug schema."""
    pairs = re.findall(r"\b([a-z_]+): ([0-9]+)(?=[, }])", text)
    return {key: int(value) for key, value in pairs}


def debug_structs(records, name):
    result = []
    for row in records:
        for body in re.findall(r"\b" + re.escape(name) + r" \{([^{}]*)\}", row.get("details", "")):
            result.append((debug_numbers(body), body))
    return result


def debug_stdout(text):
    """Decode bounded retained OutputPage stdout, never execute Debug text."""
    output = bytearray()
    for block in re.findall(r"OutputChunk \{[^{}]*stream: Stdout,[^{}]*bytes: \[([0-9, ]*)\]", text):
        values = [int(value.strip()) for value in block.split(",") if value.strip()]
        if any(value > 255 for value in values):
            raise ValueError("invalid retained stdout byte")
        output.extend(values)
    return output.decode("utf-8")


def metrics(records):
    result = defaultdict(int)
    for row in records:
        if row["kind"] == "sample-complete":
            for key in ("host_orchestration_ns", "pure_call_sum_ns", "orchestration_unattributed_ns"):
                if key in row:
                    result[key] = number(row[key], key)
        if row["kind"] == "phase":
            phase = row.get("phase")
            if not isinstance(phase, str):
                raise ValueError("phase name missing")
            result[phase + "_ns"] += number(row.get("elapsed_ns"), "phase elapsed_ns")
            if "workload_receipt" in row:
                for key, value in receipt(row["workload_receipt"]).items():
                    if type(value) is int and (key.endswith(("_ns", "_bytes", "_count")) or key in ("attempted_operations", "completed_operations")):
                        number(value, key)
                        if "peak" in key or "high_water" in key or key.startswith("maximum_"):
                            result[key] = max(result[key], value)
                        else:
                            result[key] += value
    return dict(result)


def cgroup_observations(path, required_scope_ns):
    required = {"memory.current", "memory.peak", "memory.swap.current", "pids.current", "memory.events.oom", "memory.events.oom_kill", "cpu.stat.usage_usec"}
    required.update("memory.stat." + key for key in ("anon", "file", "file_dirty", "file_writeback", "shmem", "kernel", "slab"))
    first = last = None
    count = gap = 0
    maxima = defaultdict(int)
    violations = set()
    with gzip.open(path, "rt") as stream:
        for line in stream:
            fields = {}
            for token in line.rstrip("\n").split("\t"):
                if token.startswith("sample_ns="):
                    key, value = token.split("=", 1)
                else:
                    category, separator, value = token.partition(":")
                    if not separator:
                        raise ValueError("malformed cgroup category")
                    if "=" in value:
                        subkey, value = value.split("=", 1)
                        key = category + "." + subkey
                    else:
                        key = category
                if key in fields or not value.isdecimal():
                    raise ValueError("duplicate or nonnumeric cgroup observation")
                fields[key] = int(value)
            if not required.issubset(fields) or "sample_ns" not in fields:
                raise ValueError("missing mandatory cgroup categories")
            stamp = fields["sample_ns"]
            if last is not None and stamp <= last:
                raise ValueError("nonmonotonic cgroup sampler")
            if first is None:
                first = stamp
            else:
                gap = max(gap, stamp - last)
            last = stamp
            count += 1
            for key in required:
                maxima[key] = max(maxima[key], fields[key])
            if max(fields["memory.current"], fields["memory.peak"]) > 2 * GIB:
                violations.add("cgroup memory exceeds frozen 2 GiB")
            if fields["memory.swap.current"]:
                violations.add("cgroup swap is nonzero")
            if fields["memory.events.oom"] or fields["memory.events.oom_kill"]:
                violations.add("cgroup OOM event observed")
            if fields["pids.current"] > 256:
                violations.add("cgroup PID limit exceeded")
    if count < 2:
        raise ValueError("cgroup sampler has fewer than two observations")
    # Sampler readiness precedes worker startup. The released daemon can exit
    # normally after Client drop, before host observer drain/supervisor polling.
    # This checks the declared causal operation envelope, not host process wall
    # or an exact continuous/category peak. Retain the actual final sample gap.
    if last - first + gap < required_scope_ns:
        raise ValueError("cgroup observations do not span the declared orchestration scope")
    return {"sample_count": count, "first_ns": first, "last_ns": last, "maximum_gap_ns": gap,
            "required_scope_ns": required_scope_ns,
            "coverage_scope": "sampler-ready-before-worker; operation/orchestration envelope; excludes post-owner-close host drain and supervisor polling",
            "precision": "causally-bracketed samples; not exact continuous phase/category maxima",
            "maxima": dict(maxima)}, sorted(violations)


def validate_environment(directory, outcome, build, issues, violations):
    environment = read(directory / "environment.json")
    identity = hashlib.sha256(json.dumps(environment, sort_keys=True).encode()).hexdigest()
    require(digest(outcome.get("environment_identity")) and outcome["environment_identity"] == identity, "sealed runtime environment identity mismatch", issues)
    require(environment.get("resource_profile") == "v013-macos-docker-linux-fuse-ack-window-v1", "runtime resource profile identity missing", issues)
    before = read(directory / "container-before.json")
    after = read(directory / "container-after.json")
    require(before.get("Image") == after.get("Image") == build["image_id"], "runtime image differs from sealed source", issues)
    require(before.get("Id") == after.get("Id"), "runtime container changed", issues)
    config = before.get("HostConfig", {})
    for field, wanted in (("NanoCpus", 2_000_000_000), ("Memory", 2 * GIB), ("MemorySwap", 2 * GIB), ("PidsLimit", 256)):
        require(config.get(field) == after.get("HostConfig", {}).get(field) == wanted, f"runtime {field} cap differs from frozen profile", issues)
    require(type(after.get("State", {}).get("OOMKilled")) is bool, "runtime OOM observation missing", issues)
    require(before.get("State", {}).get("Running") is True and type(after.get("State", {}).get("Running")) is bool, "runtime running-state observation missing", issues)
    if after.get("State", {}).get("Running") is False:
        state = after["State"]
        if state.get("ExitCode") != 0 or state.get("OOMKilled") or state.get("Error") or state.get("Dead") or state.get("Restarting"):
            violations.append("owned daemon container exited abnormally")
        elif outcome.get("product_status") == "pass":
            rows = raw(directory / "raw.jsonl")
            final = [row for row in rows if row["kind"] == "workspace-spool-observation" and row.get("phase") == "final-client-drop-cleanup"]
            require(len(final) == 1 and all(final[0].get(key) == 0 for key in ("logical_bytes", "allocated_bytes", "file_count")), "normal daemon exit lacks successful Client-drop cleanup evidence", issues)
    require(any(device.get("PathOnHost") == "/dev/fuse" for device in config.get("Devices", [])), "real FUSE device absent", issues)
    for capabilities in (config.get("CapAdd"), after.get("HostConfig", {}).get("CapAdd")):
        require(isinstance(capabilities, list) and len(capabilities) == 1 and capabilities[0] in {"SYS_ADMIN", "CAP_SYS_ADMIN"}, "FUSE capability differs from the single SYS_ADMIN grant", issues)
    bindings = before.get("NetworkSettings", {}).get("Ports", {}).get("41273/tcp") or []
    require(len(bindings) == 1 and bindings[0].get("HostIp") == "127.0.0.1", "daemon endpoint is not loopback-bound", issues)
    for mount in before.get("Mounts", []):
        destination = mount.get("Destination")
        allowed = destination == "/qualified/git-reference" and mount.get("RW") is False and outcome["family_id"] == "git_tool_workflow"
        allowed |= destination == "/verification" and outcome["mode"] == "verify" and outcome["family_id"] == "git_tool_workflow"
        require(allowed, f"unapproved runtime mount: {destination}", issues)
    if after.get("State", {}).get("OOMKilled") or outcome.get("container_oom_killed"):
        violations.append("runtime container was OOM-killed")
    return before.get("Id")


def validate_resources(directory, outcome, case, records, successful, issues, violations):
    duration = number(outcome.get("external_process_wall_ns"), "external_process_wall_ns")
    require(outcome.get("hard_deadline_seconds") == runner.deadline(case, outcome["mode"]), "case deadline differs from frozen policy", issues)
    for key in ("preparation_ns", "command_wall_ns", "cleanup_ns", "runtime_preparation_ns"):
        number(outcome.get(key), key)
    if outcome["preparation_ns"] > runner.preparation_deadline(case) * 1_000_000_000:
        violations.append("preparation exceeded frozen deadline")
    if outcome["cleanup_ns"] > 60_000_000_000:
        violations.append("cleanup exceeded frozen deadline")
    if outcome.get("timeout") or duration > runner.deadline(case, outcome["mode"]) * 1_000_000_000:
        violations.append("worker exceeded frozen case deadline")
    require(outcome["command_wall_ns"] >= outcome["preparation_ns"] + duration, "command wall hides preparation or worker time", issues)
    require(outcome.get("command_wall_scope") == "one sample preparation/runtime/product/cleanup; CLI validation is in invocation receipt", "sample command wall scope missing", issues)
    complete = [row for row in records if row["kind"] == "sample-complete"]
    required_scope = number(complete[0].get("host_orchestration_ns"), "host_orchestration_ns") if len(complete) == 1 else duration
    observations, cgroup_failures = cgroup_observations(directory / "cgroup-samples.tsv.gz", required_scope)
    violations.extend(cgroup_failures)
    host = [row for row in records if row["kind"] == "host-resources"]
    require(bool(host), "missing native host resource observation", issues)
    for row in host:
        for key in ("user_cpu_ns", "system_cpu_ns", "resident_bytes", "peak_resident_bytes", "physical_footprint_bytes", "disk_read_bytes", "disk_write_bytes", "swaps"):
            number(row.get(key), key)
        if max(row["resident_bytes"], row["peak_resident_bytes"]) > 2 * GIB:
            violations.append("host RSS exceeds frozen 2 GiB")
        if row["swaps"]:
            violations.append("host swap activity observed")
    samples = [row for row in records if row["kind"] == "host-rss-samples"]
    if successful:
        require(len(samples) == 1, "missing or duplicate host RSS sampler receipt", issues)
        require(any(row.get("phase") == "final" for row in host), "missing final host resources", issues)
    for row in samples:
        for key in ("sample_count", "baseline_bytes", "sampled_peak_bytes", "final_bytes", "maximum_gap_ns", "nominal_interval_ns"):
            number(row.get(key), key)
        require(row["sample_count"] > 0 and row["nominal_interval_ns"] == 10_000_000, "invalid native sampling profile", issues)
        if row["sampled_peak_bytes"] > 2 * GIB:
            violations.append("sampled host RSS exceeds frozen 2 GiB")
    require(not any(row["kind"] in {"host-rss-failure", "host-resource-failure", "monitor-observation-failure", "spool-observation-failure"} for row in records), "mandatory native/workspace observer failed", issues)
    stores = [row for row in records if row["kind"] == "store-observation"]
    if successful and not case.get("proof_only"):
        require(len(stores) >= 2, "missing before/after physical Store observations", issues)
    for row in stores:
        values = {key: number(row.get(key), key) for key in ("file_bytes", "allocated_bytes", "page_size_bytes", "page_count", "freelist_page_count", "live_page_bytes")}
        require(values["page_count"] >= values["freelist_page_count"] and values["live_page_bytes"] == (values["page_count"] - values["freelist_page_count"]) * values["page_size_bytes"], "Store page accounting equation", issues)
        if max(values["file_bytes"], values["allocated_bytes"]) > 4 * GIB:
            violations.append("physical Store exceeds frozen 4 GiB")
    spool = [row for row in records if row["kind"] == "workspace-spool-observation"]
    if successful:
        require(bool(spool), "missing physical/logical Workspace spool boundary observations", issues)
        final_spool = [row for row in spool if row.get("phase") == "final-client-drop-cleanup"]
        require(len(final_spool) == 1 and all(final_spool[0].get(key) == 0 for key in ("logical_bytes", "allocated_bytes", "file_count")), "final owned spool/runtime cleanup was not proved", issues)
    for row in spool:
        require(row.get("precision") == "boundary-observation", "spool scope/precision unspecified", issues)
        for key in ("logical_bytes", "allocated_bytes", "file_count", "observer_ns"):
            number(row.get(key), key)
        if max(row["logical_bytes"], row["allocated_bytes"]) > 2 * GIB:
            violations.append("Workspace spool boundary exceeds frozen 2 GiB")
    physical = [row for row in records if row["kind"] == "workspace-physical-spool"]
    if successful and case["family_id"] == "workspace_reliability" and case["operation"] not in {"corrupt-descendant", "missing-descendant"}:
        require(bool(physical), "missing proof physical spool event observations", issues)
    for row in physical:
        require(row.get("precision") == "mutation-event-aggregate-allocation" and row.get("method") == "verification_workspace_state", "proof physical spool provenance missing", issues)
        for key in ("allocated_bytes", "peak_bytes", "observation_errors", "observation_count"):
            number(row.get(key), key)
        require(row["observation_errors"] == 0 and row["allocated_bytes"] <= row["peak_bytes"], "proof physical spool observation failure/equation", issues)
        if row["peak_bytes"] > 2 * GIB:
            violations.append("proof physical spool exceeds frozen 2 GiB")
    return observations


def operation_rows(records, issues):
    rows = [row["receipt"] for row in records if row["kind"] == "operation"]
    ids = []
    for row in rows:
        require(isinstance(row, dict) and row.get("schema_version") == 4, "invalid public operation receipt schema", issues)
        if not isinstance(row, dict):
            continue
        ids.append(number(row.get("id"), "operation id"))
        number(row.get("service_ns"), "operation service_ns")
        number(row.get("queue_ns"), "operation queue_ns")
    require(ids == sorted(set(ids)), "duplicate/reordered public operation IDs", issues)
    return rows


def expected_calls(case):
    if case["input_mode"] == "directory":
        return {"layerstack.initialize": 1}
    steps = case["tier"] if case["family_id"] == "dedup_branch_history" else 1
    edits = case["tier"] if case["operation"] == "workspace-distributed-sdk-edit" else steps if case["family_id"] == "dedup_branch_history" and case["operation"] in {"distributed", "hotset", "recurring"} else 1 if case.get("inherited") else 0
    execs = 0 if edits or case["operation"] == "workspace-clean-commit" else steps
    return {"workspace.create": 1, "workspace.exec": execs, "workspace.output": execs, "workspace.file_range_edit": edits, "workspace.commit": steps, "workspace.end": 1, "query": 1}


def validate_timing(records, case, issues):
    for row in records:
        if row["kind"] != "phase":
            continue
        elapsed = number(row.get("elapsed_ns"), "phase elapsed_ns")
        if row.get("phase") == "exec":
            workload = receipt(row.get("workload_receipt"))
            inner_key = "inner_workload_ns" if case["family_id"].startswith("dedup_") else "workload_ns"
            require(number(workload.get(inner_key), inner_key) <= elapsed, "inner workload exceeds public Exec interval", issues)
            attempt = workload.get("attempted_syscall_count", workload.get("attempted_operations"))
            completed = workload.get("completed_syscall_count", workload.get("completed_operations"))
            attempt = number(attempt, "attempted workload count")
            completed = number(completed, "completed workload count")
            interrupted = number(workload.get("interrupted_syscall_count", 0), "interrupted syscall count")
            require(attempt == completed + interrupted and completed > 0, "workload completion count equation", issues)
            operation, tier = case["operation"], case["tier"]
            expected_work = {}
            if operation == "payload-create":
                expected_work = {"completed_write_bytes": tier * 1_048_576, "completed_file_write_count": 1}
            elif operation == "payload-random-read":
                expected_work = {"completed_read_request_count": tier, "completed_read_bytes": tier * 4096, "completed_write_bytes": 0}
            elif operation in {"tiny-create", "tiny-stat", "tiny-unlink", "git-tool"}:
                expected_work = {"completed_target_count": tier}
                if operation == "git-tool": expected_work["git_process_count"] = 6
                if operation == "tiny-stat": expected_work.update(completed_read_bytes=0, completed_write_bytes=0)
            elif operation in {"tiny-bulk-create", "workspace-dense-rewrite"}:
                expected_work = {"completed_file_write_count": 200 * tier, "completed_write_bytes": tier * 1_048_576}
            elif operation == "tiny-bulk-delete":
                expected_work = {"workload_unlink_call_count": 200 * tier}
            elif operation == "directory-construct":
                expected_work = {"completed_chain_count": tier, "completed_write_bytes": 0}
            elif operation in {"directory-metadata-scan", "directory-content-scan"}:
                expected_work = {"visited_file_count": 200 * tier, "visited_path_count": 201 * tier + 133, "completed_read_bytes": tier * 1_048_576 if operation == "directory-content-scan" else 0, "completed_write_bytes": 0}
            elif operation == "agent-episodes":
                expected_work = {"completed_episode_count": tier}
            elif case["family_id"] == "dedup_workspace_reuse":
                expected_work = {"completed_operations": tier, "successful_write_bytes": tier * 1_048_576}
            elif case["family_id"] == "dedup_branch_history":
                expected_work = {"completed_operations": 1 if operation == "metadata" else 200, "successful_write_bytes": 0 if operation == "metadata" else 1_048_576}
            for key, wanted in expected_work.items():
                require(workload.get(key) == wanted, f"case work cardinality {key}: expected {wanted}, observed {workload.get(key)}", issues)
            if not case["family_id"].startswith("dedup_"):
                require(workload.get("workload_status") == "pass" and workload.get("scenario_id") == case["scenario_id"], "ordinary workload identity/status mismatch", issues)
                require(all(workload.get(key) == 0 for key in ("benchmark_verifier_count", "benchmark_reopen_count", "benchmark_injection_count")), "ordinary workload performance purity mismatch", issues)
            execution = row.get("execution_receipt", "")
            require(all(text in execution for text in ("transport: Daemon", "docker_engine_calls: 0", "exit_code: Some(0)", "daemon_timing: Some(")), "Exec did not authenticate daemon route/success", issues)
            # DaemonTiming repeats field names; outer receipt values precede it.
            timing = debug_numbers(execution.partition("daemon_timing:")[0])
            fields = ("spawn_ns", "supervisor_queue_ns", "runtime_ns", "drain_ns", "terminal_publication_ns", "unattributed_ns")
            require(all(key in timing for key in (*fields, "elapsed_ns", "total_wall_ns")), "incomplete Exec timing receipt", issues)
            if all(key in timing for key in (*fields, "elapsed_ns", "total_wall_ns")):
                require(timing["elapsed_ns"] == timing["total_wall_ns"] == sum(timing[key] for key in fields), "Exec timing equation", issues)
    for value, body in debug_structs(records, "WorkspaceLifecycleReceipt"):
        phases = ("proxy_ns", "docker_setup_ns", "helper_copy_ns", "mount_ready_ns", "unmount_ns", "wait_ns", "cleanup_ns", "unattributed_ns")
        require(all(key in value for key in (*phases, "total_ns", "docker_calls")), "incomplete FUSE lifecycle observation", issues)
        if all(key in value for key in (*phases, "total_ns", "docker_calls")):
            require(value["docker_calls"] == 0 and value["total_ns"] == sum(value[key] for key in phases), "FUSE lifecycle route/timing equation", issues)
    commits = debug_structs(records, "WorkspaceCommitReceipt")
    for value, body in commits:
        phases = ("pause_fence_ns", "quiesce_ns", "capture_ns", "candidate_plan_ns", "dirty_compare_ns", "content_ns", "namespace_ns", "candidate_finish_ns", "local_admission_ns", "object_admission_ns", "publication_ns", "in_place_rebase_ns", "resume_ns", "unattributed_ns")
        require(all(key in value for key in (*phases, "total_ns", "payload_bytes_read")), "incomplete Commit phase/work observations", issues)
        if all(key in value for key in (*phases, "total_ns")):
            require(value["total_ns"] == sum(value[key] for key in phases) and "capture_mode: Some(Live)" in body, "Commit timing equation or real-FUSE capture route", issues)
    for value, _ in debug_structs(records, "CandidateReceipt"):
        fields = ("candidate_objects", "candidate_bytes", "inserted_objects", "inserted_bytes", "reused_objects", "reused_bytes", "max_transaction_objects", "max_transaction_bytes")
        require(all(key in value for key in fields), "incomplete candidate observation", issues)
        if all(key in value for key in fields):
            require(value["candidate_objects"] == value["inserted_objects"] + value["reused_objects"] and value["candidate_bytes"] == value["inserted_bytes"] + value["reused_bytes"], "candidate accounting equation", issues)
            require(value["max_transaction_objects"] < (8192 if case["input_mode"] == "directory" else 128) and value["max_transaction_bytes"] < 4 * 1024 ** 2, "candidate transaction bound", issues)


def validate_performance(case, outcome, records, issues, violations):
    require(not any(row["kind"] in VERIFY_KINDS for row in records), "verification/fault activity contaminated performance", issues)
    complete = [row for row in records if row["kind"] == "sample-complete"]
    require(len(complete) == 1 and complete[0].get("status") == "pass", "missing/duplicate successful performance completion", issues)
    if len(complete) != 1:
        return
    final = complete[0]
    for key in ("benchmark_verifier_count", "benchmark_reopen_count", "benchmark_injection_count"):
        require(type(final.get(key)) is int and final[key] == 0, f"missing/nonzero performance purity counter {key}", issues)
    duration = number(final.get("host_orchestration_ns"), "host_orchestration_ns")
    require(isinstance(final.get("orchestration_scope"), str) and bool(final["orchestration_scope"]), "missing host orchestration scope", issues)
    require(duration <= outcome["external_process_wall_ns"], "product lifecycle exceeds supervised worker", issues)
    ops = operation_rows(records, issues)
    actual = Counter(row.get("family") for row in ops)
    expected = Counter({key: value for key, value in expected_calls(case).items() if value})
    require(actual == expected, f"public operation counts: expected {dict(expected)}, observed {dict(actual)}", issues)
    for operation in ops:
        wanted = "up_to_date" if operation.get("family") == "workspace.commit" and case["operation"] in CLEAN else "success"
        require(operation.get("outcome") == wanted, "public operation outcome differs from case contract", issues)
    phases = Counter(row.get("phase") for row in records if row["kind"] == "phase")
    wanted = {"initialize": 1} if case["input_mode"] == "directory" else {"create": 1, "end": 1, "visibility": 1, "commit": expected["workspace.commit"], "exec": expected["workspace.exec"], "sdk-edit": expected["workspace.file_range_edit"]}
    require(phases == Counter({key: value for key, value in wanted.items() if value}), "missing/extra product phase boundaries", issues)
    sums = sum(row["elapsed_ns"] for row in records if row["kind"] == "phase")
    require(sums <= duration, "phase sum exceeds product lifecycle", issues)
    require(number(final.get("pure_call_sum_ns"), "pure_call_sum_ns") == sums and sums + number(final.get("orchestration_unattributed_ns"), "orchestration_unattributed_ns") == duration, "host orchestration/pure-call timing equation", issues)
    if case.get("inherited") and sum(row["elapsed_ns"] for row in records if row["kind"] == "phase" and row.get("phase") in {"sdk-edit", "commit", "end"}) > 2_000_000_000:
        violations.append("capped edit/Commit/End exceeds inherited 2-second gate")
    validate_timing(records, case, issues)
    if case["input_mode"] == "directory" or case["operation"] not in CLEAN:
        require(bool(debug_structs(records, "CandidateReceipt")), "missing candidate insert/reuse observations", issues)
    if case["input_mode"] == "directory":
        scans = [row for row in records if row["kind"] == "initialization-scan"]
        require(len(scans) == 1, "missing public initialization scan receipt", issues)
        return
    require(final.get("created_commit_count") == (0 if case["operation"] in CLEAN else expected["workspace.commit"]), "Created/UpToDate trajectory count", issues)
    require(len(debug_structs(records, "WorkspaceCommitReceipt")) == expected["workspace.commit"], "missing incremental Commit phase receipts", issues)
    require(len(debug_structs(records, "WorkspaceLifecycleReceipt")) == 2, "missing real-FUSE attach/end observations", issues)
    reads = debug_structs(records, "WorkspaceReadReceipt")
    if expected["workspace.exec"]:
        require(bool(reads), "missing actual FUSE callback observations", issues)
        for fields, _ in reads:
            mandatory = ["callback_" + name for name in FUSE_CALLBACKS] + ["directory_entries_returned", "directory_nonzero_offset_requests", "kernel_read_bytes"]
            require(all(key in fields for key in mandatory), "incomplete FUSE operation/page metrics", issues)
        require(sum(fields.get("callback_" + name, 0) for fields, _ in reads for name in FUSE_CALLBACKS) > 0, "ordinary workload has no actual kernel callbacks", issues)
    if case["operation"] in {"directory-content-scan", "directory-metadata-scan"}:
        require(sum(fields.get("callback_readdir", 0) + fields.get("callback_readdirplus", 0) for fields, _ in reads) > 0, "full-tree scan has no FUSE directory pages", issues)
    if expected["workspace.commit"]:
        diagnostics = [row for row in records if row["kind"] == "commit-diagnostics"]
        require(len(diagnostics) == expected["workspace.commit"], "missing incremental Commit work diagnostics", issues)
        for row in diagnostics:
            fields = debug_numbers(row.get("details", ""))
            require(all(key in fields for key in ("cdc_bytes_scanned", "edit_spool_peak_bytes", "namespace_base_paths_visited", "namespace_final_paths_visited", "namespace_dirty_nodes_visited", "namespace_clean_nodes_visited", "namespace_candidate_probe_nodes")), "missing Commit locality/spool work fields", issues)
            if fields.get("edit_spool_peak_bytes", 0) > 2 * GIB:
                violations.append("logical edit spool exceeds frozen 2 GiB")
            physical = {key: int(value) for key, value in re.findall(r"\b(physical_spool_(?:allocated|peak)_bytes): Some\(([0-9]+)\)", row.get("details", ""))}
            require(set(physical) == {"physical_spool_allocated_bytes", "physical_spool_peak_bytes"} and fields.get("physical_spool_observation_errors") == 0 and "physical_spool_observation_count" in fields, "missing/error physical spool event observations", issues)
            if len(physical) == 2:
                require(physical["physical_spool_allocated_bytes"] <= physical["physical_spool_peak_bytes"], "physical spool current/peak equation", issues)
                if physical["physical_spool_peak_bytes"] > 2 * GIB:
                    violations.append("physical spool event high-water exceeds frozen 2 GiB")
            if case["operation"] in CLEAN:
                require(fields.get("cdc_bytes_scanned") == 0, "clean Commit performed CDC payload work", issues)


def validate_verification(case, records, issues):
    family, operation = case["family_id"], case["operation"]
    operations = operation_rows(records, issues)
    forbidden = {"workspace.shell", "layerstack.add", "dedup.analyze"}
    require(not any(row.get("family") in forbidden for row in operations), "unapproved verifier public operation route", issues)
    if family == "workspace_reliability":
        starts = [row for row in records if row["kind"] == "proof-start"]
        ends = [row for row in records if row["kind"] == "proof-complete"]
        require(len(starts) == len(ends) == 1 and case["scenario_id"] in starts[0].get("detail", "") and case["scenario_id"] in ends[0].get("detail", "") and "pass" in ends[0].get("detail", ""), "reliability proof identity/completion missing", issues)
        require(not any(row["kind"] in {"proof-failed", "unconsumed-workspace-fault", "unconsumed-store-fault", "leaked-runtime-files"} for row in records), "reliability proof/fault/cleanup failure", issues)
        kinds = Counter(row["kind"] for row in records)
        require(kinds["mount-cleanup"] >= 1 and kinds["proof-cgroup-resources"] >= 2, "reliability cleanup/resource proof missing", issues)
        if operation in {"corrupt-descendant", "missing-descendant"}:
            require(kinds["integrity-detection-qualified"] == 1 and kinds["fault-target-payload"] == 1, "integrity fault detection/target receipt missing", issues)
        else:
            require(kinds["live-full-tree"] >= 2 and kinds["canonical-full-tree"] >= 1, "reliability live/reopen/canonical verification missing", issues)
            require(bool(operations), "reliability public operation observations missing", issues)
            for row in records:
                if row["kind"] == "live-full-tree":
                    require("verification_status=pass" in debug_stdout(row.get("detail", "")), "reliability native complete-tree receipt missing/failing", issues)
                elif row["kind"] == "canonical-full-tree":
                    require('"verification_status": "pass"' in row.get("detail", "") and '"canonical_role_status": "pass"' in row.get("detail", ""), "reliability canonical typed/content proof missing/failing", issues)
        if operation in {"candidate-failure-retry", "published-presentation-failure", "short-spool-write", "deferred-nospace"}:
            require(kinds["fault-reachability"] >= 1, "Workspace fault boundary was not reached", issues)
        if operation in {"admission-batch-failure-retry", "final-publication-failure-retry"}:
            require(kinds["transaction-fault-reachability"] >= 1, "Store fault boundary was not reached", issues)
        for row in records:
            if row["kind"] in {"fault-reachability", "transaction-fault-reachability"}:
                require(debug_numbers(row.get("detail", "")).get("hit_count") == 1, "fault did not hit exactly once", issues)
        if operation == "exec-500":
            execs = [row for row in records if row["kind"] == "exec-one"]
            require(len(execs) == 500 and all(debug_stdout(row.get("detail", "")) == f"{index}\n" for index, row in enumerate(execs)), "repeated Exec proof did not retain all500 distinct outputs", issues)
        if operation == "sustained-600s":
            text = "\n".join(debug_stdout(row.get("detail", "")) for row in records if row["kind"] == "sustained")
            elapsed = re.findall(r"^active_elapsed_ns=([0-9]+)$", text, re.M)
            cycles = re.findall(r"^completed_cycles=([0-9]+)$", text, re.M)
            require(len(elapsed) == len(cycles) == 1 and int(elapsed[0]) >= 600_000_000_000 and int(cycles[0]) > 0, "sustained proof lacks600seconds actual work", issues)
        return
    complete = [row for row in records if row["kind"] == "verification-complete"]
    require(len(complete) == 1 and complete[0].get("status") == "pass", "missing independent verification completion", issues)
    canonical = [row for row in records if row["kind"] == "canonical-verification"]
    if family == "dedup_branch_history":
        accounting = [row for row in records if row["kind"] == "history-accounting"]
        transcripts = [row for row in records if row["kind"] == "history-transcript"]
        require(sorted(row.get("step") for row in accounting) == list(range(case["tier"] + 1)), "history accounting omitted retained snapshots", issues)
        require(sorted(row.get("step") for row in transcripts) == list(range(1, case["tier"] + 1)), "history transcript coverage", issues)
        for row in accounting:
            value = receipt(row.get("receipt"))
            require(int(value.get("retained_snapshot_count", -1)) == row["step"] + 1 and int(value.get("retained_logical_snapshot_bytes", -1)) == (row["step"] + 1) * 1_048_576, "history retained logical bound", issues)
    else:
        require(len(canonical) == 1, "missing or duplicate canonical verification", issues)
        for row in canonical:
            value = receipt(row.get("receipt"))
            require(value.get("verification_status") == value.get("canonical_role_status") == "pass" and digest(value.get("oracle_identity")), "canonical byte/metadata/typed-graph proof failed", issues)
    for row in records:
        if row["kind"] in {"native-verification", "git-semantic-verification"}:
            key = "git_semantic_verification_status" if row["kind"].startswith("git") else "verification_status"
            require(receipt(row.get("receipt")).get(key) == "pass", "native full-state verification failed", issues)
    if operation == "git-tool":
        kinds = Counter(row["kind"] for row in records)
        require(kinds["git-semantic-verification"] == 2 and kinds["git-precommit-custody"] == kinds["git-reopen-custody"] == 1, "Git independent semantics/persistence custody incomplete", issues)
    else:
        native = [row for row in records if row["kind"] == "native-verification"]
        expected_steps = list(range(case["tier"] + 1)) if family == "dedup_branch_history" else [1]
        require(sorted(row.get("step") for row in native) == expected_steps, "native reopened snapshot coverage", issues)
    if family.startswith("dedup_"):
        kind = "history-transcript" if family == "dedup_branch_history" else "dedup-verification"
        rows = [row for row in records if row["kind"] == kind]
        require(len(rows) == (case["tier"] if family == "dedup_branch_history" else 1), "independent dedup proof omitted", issues)
        for row in rows:
            value = receipt(row.get("receipt"))
            require(value.get("dedup_transcript_status") == "pass", "independent dedup transcript failure", issues)
            if operation == "boundaries":
                require(value.get("boundary_file_count") == "60" and value.get("boundary_seed_count") == "3" and all(value.get(f"seed_{seed}_boundary_status") == "pass" for seed in (1, 2, 3)), "CDC boundary cohort incomplete", issues)
    if case.get("inherited"):
        rows = [row for row in records if row["kind"] == "capped-verification"]
        require(len(rows) == 1 and receipt(rows[0].get("receipt")).get("capped_sdk_transcript_status") == "pass", "capped independent splice proof missing", issues)


def validate_classification(directory, outcome, classification, issues):
    require(classification.get("classification") == "reproduced-product-finding", "failed outcome needs an explicit reproduced product finding", issues)
    require(classification.get("failure_class") in {"correctness", "timeout", "resource", "cleanup", "unsupported"}, "unclassified failure type", issues)
    for key in ("finding", "impact", "phase2_dependency", "failure_signature"):
        value = classification.get(key)
        require(isinstance(value, str) and len(value.strip()) >= 8 and value.strip().lower() not in {"placeholder", "not applicable", "unknown", "todo todo"}, f"product classification lacks meaningful {key}", issues)
    signature = classification.get("failure_signature", "")
    text = "\n".join((directory / name).read_text() for name in ("stderr.txt", "raw.jsonl", "container-after.json"))
    require(bool(signature) and signature in text, "classification signature absent from original failure", issues)
    paths = classification.get("reproduction_evidence", [])
    if isinstance(paths, str):
        paths = [paths]
    require(isinstance(paths, list) and bool(paths), "missing sealed reproduction evidence", issues)
    for item in paths if isinstance(paths, list) else []:
        path = Path(item).resolve()
        require(path != directory.resolve(), "a failure cannot independently reproduce itself", issues)
        custody.verify_manifest(path)
        other = read(path / "outcome.json")
        require(all(other.get(key) == outcome.get(key) for key in ("product_identity", "image_id", "source_revision")), "reproduction differs from product/environment source", issues)
        require(other.get("coverage_status") == "executed" and other.get("product_status") != "pass", "reproduction has no executed failed outcome", issues)
        evidence = "\n".join((path / name).read_text() for name in ("stderr.txt", "raw.jsonl", "container-after.json"))
        require(bool(signature) and signature in evidence, "linked reproduction does not exhibit classified failure", issues)


def validate_attempt(outcome, classification, case, build):
    issues, violations = [], []
    records, observed, resource = [], {}, {}
    directory = Path(outcome.get("evidence_path", ""))
    successful = outcome.get("product_status") == "pass"
    try:
        custody.verify_manifest(directory)
        sealed = read(directory / "outcome.json")
        require(all(value == outcome.get(key) for key, value in sealed.items()) and not (set(outcome) - set(sealed) - {"previous_evidence_path"}), "ledger differs from sealed outcome", issues)
        require(outcome.get("schema") == "fs-bench-pro-v013-sample-v1" and outcome.get("coverage_status") == "executed", "slot has no executed result", issues)
        for key, value in IDENTITY_FIELDS.items():
            require(outcome.get(key) == build[value], f"source identity mismatch: {key}", issues)
        require(outcome.get("source_arm") == "baseline" and outcome.get("admission_eligible") is False, "invalid Phase 1 arm/admission scope", issues)
        for key in ("scenario_id", "family_id", "proof_only", "inherited"):
            require(outcome.get(key) == case.get(key, False), f"slot identity mismatch: {key}", issues)
        require(outcome.get("mode") in {"performance", "verify"}, "unknown evidence mode", issues)
        records = raw(directory / "raw.jsonl")
        if case["family_id"] != "workspace_reliability":
            starts = [row for row in records if row["kind"] == "sample-start"]
            require(len(starts) == 1 and all(starts[0].get(key) == outcome.get(key) for key in ("scenario_id", "family_id", "seed", "mode")), "raw sample identity differs from scheduled slot", issues)
        command = read(directory / "command.json")["argv"]
        require(len(command) == 8 and command[1] == "workspace-run" and command[4:7] == [outcome["scenario_id"], str(outcome["seed"]), outcome["mode"]], "retained command does not execute selected case/seed/mode", issues)
        container = validate_environment(directory, outcome, build, issues, violations)
        require(command[-1] == container, "public workload container binding differs", issues)
        acquired = read(directory / "preparation/acquisition.json")
        clone = read(directory / "preparation/clone.json")
        require(acquired.get("status") == "pass" and acquired.get("producer", {}).get("status") == "pass", "unqualified prepared input producer", issues)
        require(digest(outcome.get("input_identity")) and outcome["input_identity"] == acquired.get("fixture", {}).get("input_plan_sha256") and outcome.get("cache_key") == acquired.get("key"), "input/cache identity mismatch", issues)
        require(clone.get("status") == "pass" and clone.get("hard_link") is False, "sample not an independent qualified clone", issues)
        cache = read(directory / "preparation/master-cache.json")
        require(cache.get("key") == acquired.get("key") and cache.get("fixture") == acquired.get("fixture") and cache.get("producer") == acquired.get("producer"), "copied master cache identity mismatch", issues)
        master_seal = directory / "preparation/master-evidence.sha256"
        require(custody.sha(master_seal) == acquired.get("cache_manifest_sha256"), "retained master manifest seal mismatch", issues)
        master_files = {}
        for line in master_seal.read_text().splitlines():
            sha, relative = line.split(maxsplit=1)
            require(digest(sha) and relative not in master_files and not Path(relative).is_absolute() and ".." not in Path(relative).parts, "invalid original master manifest entry", issues)
            master_files[relative] = sha
        require(master_files.get("cache.json") == custody.sha(directory / "preparation/master-cache.json") and master_files.get("input-manifest.tsv") == custody.sha(directory / "preparation/master-input-manifest.tsv"), "copied input/cache manifest not authenticated by master seal", issues)
        logical, files = validate_input_manifest((directory / "preparation/master-input-manifest.tsv").read_text())
        if case["operation"] != "git-tool":
            require((logical, files) == (acquired["fixture"].get("fixture_bytes"), acquired["fixture"].get("regular_files")), "prepared manifest bytes/files differ from fixture identity", issues)
        if case["input_mode"] == "store":
            require(clone.get("clone_store_sha256") == clone.get("prepared_store_sha256") == acquired.get("store_sha256") == master_files.get("store/store.sqlite"), "pristine Store clone identity mismatch", issues)
        elif successful and outcome["mode"] == "performance":
            scans = [row for row in records if row["kind"] == "initialization-scan"]
            if len(scans) == 1:
                require(scans[0].get("scanned_files") == acquired["fixture"].get("regular_files") and scans[0].get("scanned_bytes") == acquired["fixture"].get("fixture_bytes"), "actual initialization files/bytes differ from qualified input", issues)
        for category, cap in (("preparation_footprint", 4 * GIB), ("cache_footprint", 24 * GIB)):
            footprint = acquired.get(category)
            require(isinstance(footprint, dict) and bool(footprint), f"missing {category}", issues)
            if isinstance(footprint, dict):
                for field, size in footprint.items():
                    if number(size, field) > cap:
                        violations.append(f"{category} exceeds frozen disk bound")
        if case["operation"] == "git-tool":
            reference = read(directory / "preparation/reference/acquisition.json")
            require(reference.get("status") == "pass" and reference.get("producer", {}).get("status") == "pass" and digest(outcome.get("oracle_identity")) and outcome["oracle_identity"] == reference.get("fixture", {}).get("input_plan_sha256"), "independent Git reference identity/custody missing", issues)
            require(custody.sha(directory / "preparation/reference/master-evidence.sha256") == reference.get("cache_manifest_sha256"), "Git reference master seal mismatch", issues)
            if outcome["mode"] == "verify":
                precommit = directory / "verifier-exchange/precommit.tsv"
                reopened = directory / "verifier-exchange/reopened.tsv"
                require(precommit.read_bytes() == reopened.read_bytes() and precommit.stat().st_size > 0, "Git pre-Commit/reopen full persistence manifests differ", issues)
        require(outcome.get("supervisor_cleanup_status") == "pass", "owned supervisor runtime not recovered", issues)
        require(outcome.get("harness_status") != "fail" and not outcome.get("observer_errors"), "harness/resource observer failed", issues)
        if successful:
            require(outcome.get("exit_code") == 0 and outcome.get("timeout") is False and outcome.get("supervisor_failure") is None, "declared success contradicts worker outcome", issues)
            require(outcome.get("mutable_sample_cleanup_status") == "pass", "successful sample mutable state not cleaned", issues)
        resource = validate_resources(directory, outcome, case, records, successful, issues, violations)
        if successful:
            if outcome["mode"] == "performance":
                validate_performance(case, outcome, records, issues, violations)
            else:
                validate_verification(case, records, issues)
        observed = metrics(records)
        if not successful or violations:
            validate_classification(directory, outcome, classification, issues)
    except (OSError, ValueError, KeyError, TypeError, IndexError, AssertionError, EOFError) as error:
        issues.append(f"invalid/missing evidence: {type(error).__name__}: {error}")
    return {"issues": sorted(set(issues)), "violations": sorted(set(violations)), "metrics": observed, "resource_observations": resource,
            "product_status": "pass" if successful and not violations else "fail", "verification_pass": successful and outcome.get("mode") == "verify" and not issues and not violations}


def registry_cases(rows):
    if any(not isinstance(row, dict) or row.get("kind") != "case" for row in rows):
        raise ValueError("registry schema")
    ids = [row["scenario_id"] for row in rows]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate registry identity")
    new = [row for row in rows if not row.get("proof_only") and not row.get("inherited")]
    proofs = [row for row in rows if row.get("proof_only")]
    inherited = [row for row in rows if row.get("inherited")]
    if Counter(row["family_id"] for row in new) != Counter(FAMILY_COUNTS):
        raise ValueError("130-case family membership differs from frozen contract")
    if len(proofs) != 29 or Counter(row["family_id"] for row in proofs) != {"workspace_reliability": 28, "dedup_cdc_locality": 1}:
        raise ValueError("CDC/reliability proof membership differs from frozen contract")
    if len(inherited) != 5 or any(row["family_id"] != "edit_length_changing_capped" or row["proof_only"] or not row["scenario_id"].endswith("-capped-v1") for row in inherited):
        raise ValueError("five versioned capped SDK replacements are required")
    return new, proofs, inherited


def generate(campaign, assets):
    custody.verify_manifest(assets / "evidence")
    build = read(assets / "evidence/build.json")
    if build.get("schema") != "fs-bench-pro-workspace-build-v1" or build.get("status") != "pass":
        raise ValueError("unqualified build assets")
    if custody.sha(assets / "fs-benchmark-pro") != build["binary_sha256"]:
        raise ValueError("registry executable differs from sealed binary")
    registry = [decode(line) for line in subprocess.check_output([str(assets / "fs-benchmark-pro"), "workspace-registry"], text=True).splitlines()]
    new, proofs, inherited = registry_cases(registry)
    ledger = read(campaign / "slots.json") if (campaign / "slots.json").exists() else {}
    classifications = read(campaign / "classifications.json") if (campaign / "classifications.json").exists() else {}
    required = [(case, seed, mode) for case in new for mode in ("performance", "verify") for seed in (1, 2, 3)]
    required += [(case, 1, "verify") for case in proofs]
    required += [(case, rep, "performance") for case in inherited for rep in range(1, 6)] + [(case, 1, "verify") for case in inherited]
    required_keys = {(case["scenario_id"], seed, mode) for case, seed, mode in required}
    current = [row for row in ledger.values() if all(row.get(key) == build[value] for key, value in IDENTITY_FIELDS.items())]
    by_slot, global_issues, evidence_paths = {}, [], set()
    environments = {row.get("environment_identity") for row in current}
    if len(environments) > 1:
        global_issues.append("multiple runtime environment identities cannot be pooled")
    invocations = []
    for path in sorted((campaign / "invocations").glob("*.json")):
        value = read(path)
        if value.get("source_revision") != build["revision"] or value.get("image_id") != build["image_id"]:
            continue
        try:
            for key in ("source_validation_ns", "registry_query_ns", "invocation_wall_ns"):
                number(value.get(key), key)
            require(value["invocation_wall_ns"] >= value["source_validation_ns"] + value["registry_query_ns"], "CLI invocation hides validation/query work", global_issues)
            require(value.get("status") in {"pass", "failed-outcomes"}, "CLI invocation has not completed", global_issues)
            require(isinstance(value.get("planned_slots"), list), "CLI invocation lacks selected slot inventory", global_issues)
        except (ValueError, TypeError) as error:
            global_issues.append(f"invalid invocation receipt {path.name}: {error}")
        invocations.append({**value, "path": str(path), "sha256": custody.sha(path)})
    if current and not invocations:
        global_issues.append("no retained CLI invocation wall receipts")
    for outcome in current:
        key = (outcome.get("scenario_id"), outcome.get("seed"), outcome.get("mode"))
        if key not in required_keys or key in by_slot or outcome.get("evidence_path") in evidence_paths:
            global_issues.append(f"duplicate, extra, or reused slot/evidence: {key}")
        else:
            by_slot[key] = outcome
            evidence_paths.add(outcome.get("evidence_path"))
    missing, rows, invalid, failures = [], [], [], []
    checked = {}
    for case, seed, mode in required:
        key = (case["scenario_id"], seed, mode)
        outcome = by_slot.get(key)
        if outcome is None:
            missing.append({"case": key[0], "seed": seed, "mode": mode})
            continue
        classification = classifications.get(Path(outcome["evidence_path"]).name, {})
        value = validate_attempt(outcome, classification, case, build)
        checked[key] = value
        row = {"case": key[0], "family_id": case["family_id"], "seed": seed, "mode": mode, "inherited": case.get("inherited", False),
               "raw_product_status": outcome.get("product_status"), "coverage_status": outcome.get("coverage_status"), "product_status": value["product_status"], "evidence_status": "REVISE" if value["issues"] else "PASS",
               "issues": value["issues"], "violations": value["violations"], "evidence": outcome["evidence_path"], "metrics": value["metrics"], "resource_observations": value["resource_observations"]}
        rows.append(row)
        if value["issues"]:
            invalid.append(row)
        if value["product_status"] != "pass":
            failures.append({**row, "classification": classification})
    distributions = defaultdict(lambda: defaultdict(list))
    for row in rows:
        case = next(case for case in registry if case["scenario_id"] == row["case"])
        proof_key = (row["case"], 1 if case.get("inherited") else row["seed"], "verify")
        proof = checked.get(proof_key, {})
        outcome = by_slot[(row["case"], row["seed"], row["mode"])]
        proof_outcome = by_slot.get(proof_key, {})
        eligible = not global_issues and row["mode"] == "performance" and row["evidence_status"] == "PASS" and row["product_status"] == "pass" and proof.get("verification_pass") and proof_outcome.get("input_identity") == outcome.get("input_identity") and proof_outcome.get("environment_identity") == outcome.get("environment_identity")
        row["performance_claim_eligible"] = bool(eligible)
        if eligible:
            for metric, value in row["metrics"].items():
                distributions[row["case"]][metric].append(value)
            for metric in ("command_wall_ns", "preparation_ns"):
                distributions[row["case"]][metric].append(outcome[metric])
    counts = {"planned_new_cases": 130, "planned_initial_sample_slots": 390, "executed_initial_sample_slots": sum(row["coverage_status"] == "executed" and not row["inherited"] and row["mode"] == "performance" for row in rows),
              "planned_new_verification_slots": 390, "executed_new_verification_slots": sum(row["coverage_status"] == "executed" and row["family_id"] in FAMILY_COUNTS and row["mode"] == "verify" and row["case"] != "dedup-cdc-boundaries-proof" for row in rows),
              "planned_reliability_subcases": 28, "executed_reliability_subcases": sum(row["coverage_status"] == "executed" and row["family_id"] == "workspace_reliability" for row in rows),
              "planned_capped_performance_slots": 25, "executed_capped_performance_slots": sum(row["coverage_status"] == "executed" and row["inherited"] and row["mode"] == "performance" for row in rows),
              "planned_capped_verifiers": 5, "executed_capped_verifiers": sum(row["coverage_status"] == "executed" and row["inherited"] and row["mode"] == "verify" for row in rows),
              "missing_slots": len(missing), "invalid_slots": len(invalid), "product_failed_outcomes": len(failures)}
    retained = []
    for path in sorted((campaign / "attempts").glob("*/outcome.json")):
        value = read(path)
        if value.get("product_status") != "pass" or value.get("harness_status") == "fail":
            retained.append({key: value.get(key) for key in ("scenario_id", "seed", "mode", "source_revision", "product_status", "harness_status", "error", "evidence_path")})
    summary = {"schema": "fs-bench-pro-phase1-review-v2", "source": build, "report_generator_sha256": custody.sha(Path(__file__)), "runtime_report_generator_sha256": build["report_generator_sha256"],
               "counts": counts, "phase1_evidence_status": "PASS" if not missing and not invalid and not global_issues else "REVISE", "product_status": "FAIL" if failures else "NOT_ESTABLISHED" if missing or invalid or global_issues else "PASS",
               "global_issues": global_issues, "missing": missing, "invalid": invalid, "product_findings": failures, "retained_failure_history": retained, "invocations": invocations, "rows": rows}
    results = campaign / "results"
    results.mkdir(exist_ok=True)
    custody.write_json(results / "review.json", summary)
    inputs = {"build_manifest_sha256": custody.sha(assets / "evidence/evidence.sha256"), "ledger_sha256": custody.sha(campaign / "slots.json") if (campaign / "slots.json").exists() else None,
              "classifications_sha256": custody.sha(campaign / "classifications.json") if (campaign / "classifications.json").exists() else None,
              "generator_sha256": summary["report_generator_sha256"], "policy_helper_sha256": custody.sha(HERE / "workspace-runner.py"), "custody_helper_sha256": custody.sha(HERE / "sdk-edit-custody.py"), "attempt_manifests": {path: custody.sha(Path(path) / "evidence.sha256") if (Path(path) / "evidence.sha256").is_file() else None for path in sorted(evidence_paths)}}
    custody.write_json(results / "report-inputs.json", inputs)
    lines = ["# LayerFS v0.1.3 Phase 1 initial baseline", "", f"Evidence: **{summary['phase1_evidence_status']}**. Product: **{summary['product_status']}**.", "", f"Sealed source: `{build['revision']}`. Report generator: `{summary['report_generator_sha256']}`.", "", "| Coverage | Count |", "| --- | ---: |"]
    lines += [f"| {key} | {value} |" for key, value in counts.items()]
    lines += ["", "## Eligible initial distributions", "", "Only complete, authentic, source/input-matched independently verified samples are eligible. Pending or failed verification excludes performance claims without deleting raw timing evidence.", "", "| Case | Metric | n | Median | Min | Max |", "| --- | --- | ---: | ---: | ---: | ---: |"]
    for case, values in sorted(distributions.items()):
        for metric, values in sorted(values.items()):
            lines.append(f"| {case} | {metric} | {len(values)} | {statistics.median(values)} | {min(values)} | {max(values)} |")
    lines += ["", "## Failures and remaining evidence work", ""]
    lines += [f"- `{row['case']}` repetition/seed {row['seed']} {row['mode']}: **FAIL**. {row['classification'].get('finding', 'Requires classification')}. Evidence: `{row['evidence']}`." for row in failures]
    lines += [f"- `{row['case']}` repetition/seed {row['seed']} {row['mode']}: **REVISE** — {'; '.join(row['issues'])}." for row in invalid]
    lines += [f"- **REVISE** — {issue}." for issue in global_issues]
    if missing:
        lines.append(f"- {len(missing)} required slots remain missing; review.json contains exact IDs.")
    lines += ["", "## Scope", "", "This is initial benchmark evidence, not release admission. Product failures remain failed Phase 2 dependencies. Prior failures remain in retained_failure_history. Report regeneration does not rerun product work. No cold-cache, optimization or crash/power-loss guarantee is claimed. Issue #21 remains open.", ""]
    (results / "initial-results.md").write_text("\n".join(lines))
    custody.seal(results)
    return summary


def self_check():
    """One product-free malformed-receipt regression; never start Docker/LayerFS."""
    case = {"scenario_id": "payload-create-1m", "family_id": "payload_create_read", "operation": "payload-create", "tier": 1, "input_mode": "store", "proof_only": False, "inherited": False}
    outcome = {"external_process_wall_ns": 100}
    issues, violations = [], []
    validate_performance(case, outcome, [{"kind": "sample-complete", "status": "pass", "host_orchestration_ns": 1, "pure_call_sum_ns": 0, "orchestration_unattributed_ns": 1, "orchestration_scope": "test-only synthetic envelope"}], issues, violations)
    assert issues and any("purity" in issue for issue in issues) and any("operation counts" in issue for issue in issues)
    issues = []
    validate_verification(case, [{"kind": "sample-complete", "status": "pass"}], issues)
    assert any("verification completion" in issue for issue in issues)
    assert any("canonical" in issue for issue in issues)
    try:
        decode('{"kind":"sample-complete","kind":"proof-complete"}')
        raise AssertionError("duplicate JSON accepted")
    except ValueError:
        pass
    with tempfile.TemporaryDirectory(prefix="phase1-report-check-") as temporary:
        path = Path(temporary) / "cgroup.tsv.gz"
        fields = ["memory.current:100", "memory.peak:100", "memory.swap.current:0", "pids.current:1", "memory.events:oom=0", "memory.events:oom_kill=0", "cpu.stat:usage_usec=1"]
        fields += ["memory.stat:" + key + "=1" for key in ("anon", "file", "file_dirty", "file_writeback", "shmem", "kernel", "slab")]
        with gzip.open(path, "wt") as stream:
            for stamp in (0, 10_000_000):
                stream.write("\t".join([f"sample_ns={stamp}", *fields]) + "\n")
        observed, failures = cgroup_observations(path, 10_000_000)
        assert observed["sample_count"] == 2 and not failures
        with gzip.open(path, "at") as stream:
            stream.write("\t".join(["sample_ns=20000000", *[field.replace("memory.events:oom=0", "memory.events:oom=1") for field in fields]]) + "\n")
        assert "cgroup OOM event observed" in cgroup_observations(path, 10_000_000)[1]
        path.write_bytes(b"not gzip")
        try:
            cgroup_observations(path, 1)
            raise AssertionError("malformed resource data accepted")
        except OSError:
            pass
    print("report_validator_self_check=pass")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path, nargs="?")
    parser.add_argument("--assets", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        self_check()
        return 0
    if args.campaign is None or args.assets is None:
        parser.error("campaign and --assets are required")
    summary = generate(args.campaign.resolve(), args.assets.resolve())
    print(json.dumps({"phase1_evidence_status": summary["phase1_evidence_status"], "product_status": summary["product_status"], **summary["counts"]}, sort_keys=True))
    return 0 if summary["phase1_evidence_status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
