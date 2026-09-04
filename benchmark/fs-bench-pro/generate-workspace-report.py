#!/usr/bin/env python3
"""Validate sealed Phase 1 evidence without executing product workloads."""
import argparse
from collections import Counter, defaultdict
import gzip
from functools import lru_cache
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
FAST_KINDS = {"fast-canonical-verification", "fast-native-verification", "fast-verification-complete"}
VERIFY_KINDS = FAST_KINDS | {"canonical-verification", "native-verification", "verification-complete", "dedup-verification", "capped-verification", "history-transcript", "history-accounting", "git-semantic-verification", "git-precommit-custody", "git-reopen-custody", "proof-start", "fault-reachability", "transaction-fault-reachability"}


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
    pairs = re.findall(r"\b([a-z_][a-z_0-9]*): ([0-9]+)(?=[, }])", text)
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


# Explicit reductions: work counters sum; capacity/current-state gauges take a
# maximum; cumulative process counters use boundary differences below.
MAX_COUNTERS = {
    "max_transaction_objects", "max_transaction_bytes", "max_write_bytes",
    "max_readahead_bytes", "init_capabilities", "snapshot_cache_rows",
    "snapshot_cache_bytes", "snapshot_cache_rows_at_create", "snapshot_cache_bytes_at_create",
    "edit_piece_count", "edit_piece_height", "edit_piece_logical_charge",
    "edit_spool_allocated_bytes", "edit_spool_peak_bytes", "edit_spool_live_bytes",
    "edit_spool_superseded_bytes", "physical_spool_allocated_bytes",
    "physical_spool_peak_bytes", "physical_spool_observation_count",
}
STRUCT_METRICS = {
    "WorkspaceCommitReceipt": "commit_work", "CandidateReceipt": "candidate",
    "WorkspaceLifecycleReceipt": "lifecycle", "WorkspaceReadReceipt": "fuse_read",
    "FuseWriteReceipt": "fuse_write",
}
CPU_COUNTERS = ("user_cpu_ns", "system_cpu_ns", "disk_read_bytes", "disk_write_bytes", "swaps")
STORE_GAUGES = ("file_bytes", "allocated_bytes", "page_count", "freelist_page_count", "live_page_bytes")


def numeric_values(value):
    return {key: int(item) for key, item in receipt(value).items()
            if not key.endswith(("_root", "_id", "_identity", "_sha256", "_revision")) and (type(item) is int or isinstance(item, str) and re.fullmatch(r"-?[0-9]+", item))}


def reduce_counter(target, key, value, maximum=False):
    target[key] = max(target.get(key, value), value) if maximum else target.get(key, 0) + value


def metrics(records):
    result = {}
    for row in records:
        if row["kind"] in {"sample-complete", "fast-verification-complete"}:
            for key in ("host_orchestration_ns", "pure_call_sum_ns", "orchestration_unattributed_ns", "created_commit_count"):
                if key in row:
                    result[key] = number(row[key], key)
        if row["kind"] in {"phase", "phase-failure"}:
            phase = row.get("phase")
            if not isinstance(phase, str):
                raise ValueError("phase name missing")
            reduce_counter(result, phase + ("_failed_ns" if row["kind"] == "phase-failure" else "_ns"), number(row.get("elapsed_ns"), "phase elapsed_ns"))
            if "workload_receipt" in row:
                for key, value in numeric_values(row["workload_receipt"]).items():
                    if key.endswith(("_ns", "_bytes", "_count")) or key in {"attempted_operations", "completed_operations"}:
                        reduce_counter(result, key, number(value, key), key in MAX_COUNTERS)
        for struct, prefix in STRUCT_METRICS.items():
            for fields, _ in debug_structs([row], struct):
                for key, value in fields.items():
                    reduce_counter(result, prefix + "." + key, value, key in MAX_COUNTERS)
        if row["kind"] == "commit-diagnostics":
            fields = debug_numbers(row.get("details", ""))
            fields.update({key: int(value) for key, value in re.findall(r"\b(physical_spool_(?:allocated|peak)_bytes): Some\(([0-9]+)\)", row.get("details", ""))})
            for key, value in fields.items():
                reduce_counter(result, "commit_diagnostics." + key, value, key in MAX_COUNTERS)
    return result


def observation_data(records, outcome, acquired, clone):
    result = metrics(records)
    steps = {}
    current_step = 0
    host, stores, spool = [], [], []
    verification = []
    for row in records:
        kind = row["kind"]
        if kind == "phase" and (type(row.get("step")) is int or row.get("phase") in {"initialize", "sdk-edit", "commit", "exec"}):
            current_step = row["step"] + 1 if type(row.get("step")) is int else 1
            point = steps.setdefault(current_step, {"step": current_step, "timings": {}, "diagnostics": {}})
            reduce_counter(point["timings"], row["phase"] + "_ns", row["elapsed_ns"])
        elif kind == "published-root":
            point = steps.setdefault(row["step"] + 1, {"step": row["step"] + 1, "timings": {}, "diagnostics": {}})
            point.update(root=row["root"], head=row.get("head"))
        elif kind == "store-observation":
            stores.append(row)
            if row.get("phase") in {"before", "after-commit", "after-initialize", "after-capped-edit"}:
                snapshot_step = 1 if row.get("phase") == "after-initialize" else row["step"]
                point = steps.setdefault(snapshot_step, {"step": snapshot_step, "timings": {}, "diagnostics": {}})
                point["store"] = {key: row[key] for key in STORE_GAUGES}
        elif kind == "host-resources":
            host.append(row)
        elif kind == "resource-failure":
            reduce_counter(result, "host_watchdog.observed_rss_bytes.max", number(row.get("host_rss_bytes"), "watchdog host_rss_bytes"), True)
        elif kind == "host-rss-samples":
            for key in ("baseline_bytes", "sampled_peak_bytes", "final_bytes", "maximum_gap_ns", "sample_count"):
                result["host_sampler." + key] = row[key]
        elif kind in {"workspace-spool-observation", "workspace-physical-spool"}:
            spool.append(row)
            prefix = "spool_boundary" if kind == "workspace-spool-observation" else "spool_event"
            for key in ("logical_bytes", "allocated_bytes", "peak_bytes", "file_count", "observation_count"):
                if type(row.get(key)) is int:
                    reduce_counter(result, prefix + ".max_" + key, row[key], True)
        elif kind in {"canonical-verification", "history-canonical", "history-accounting", "history-transcript", "dedup-verification", "capped-verification"}:
            verification.append({**row, "receipt": receipt(row["receipt"])})
        if kind in {"operation", "commit-diagnostics"} and current_step:
            point = steps.setdefault(current_step, {"step": current_step, "timings": {}, "diagnostics": {}})
            # Details emitted immediately after a reached Commit belong to that
            # zero-based operation ordinal +1; verifier forks are separate rows.
            for key, value in metrics([row]).items():
                reduce_counter(point["diagnostics"], key, value, key.rsplit(".", 1)[-1] in MAX_COUNTERS)
    if host:
        before = next((row for row in host if row.get("phase") == "before"), host[0])
        after = next((row for row in host if row.get("phase") == "after-product"), host[-1])
        for key in CPU_COUNTERS:
            result["host." + key + ".start"] = before[key]
            result["host." + key + ".end"] = after[key]
            if after[key] < before[key]:
                raise ValueError("cumulative host resource counter regressed: " + key)
            result["host." + key + ".delta"] = after[key] - before[key]
        for key in ("resident_bytes", "peak_resident_bytes", "physical_footprint_bytes"):
            result["host." + key + ".max"] = max(row[key] for row in host)
    product_stores = [row for row in stores if row.get("phase") in {"before", "after-commit", "after-initialize", "after-capped-edit", "failure", "initialization-failure"}]
    if product_stores:
        for key in STORE_GAUGES:
            result["store." + key + ".start"] = product_stores[0][key]
            result["store." + key + ".end"] = product_stores[-1][key]
            result["store." + key + ".delta"] = product_stores[-1][key] - product_stores[0][key]
            result["store." + key + ".max"] = max(row[key] for row in product_stores)
        before = product_stores[0]
        previous = before
        for row in product_stores[1:]:
            point = steps.get(1 if row.get("phase") == "after-initialize" else row.get("step"))
            if point is not None:
                point["store_growth_from_input"] = {key: row[key] - before[key] for key in STORE_GAUGES}
                point["store_growth_this_step"] = {key: row[key] - previous[key] for key in STORE_GAUGES}
            previous = row
    for key in ("preparation_ns", "command_wall_ns", "cleanup_ns", "runtime_preparation_ns", "external_process_wall_ns"):
        if type(outcome.get(key)) is int:
            result[key] = outcome[key]
    reused = acquired.get("run_acquisition_reused") is True
    for key in ("cache_acquisition_ns", "cache_build_ns", "cache_validation_ns"):
        if key in acquired:
            result[key] = number(acquired.get("run_acquisition_ns"), "run_acquisition_ns") if reused and key == "cache_acquisition_ns" else 0 if reused else acquired[key]
    for key in ("clone_wall_ns", "clone_bytes"):
        if key in clone:
            result[key] = clone[key]
    fixture = acquired.get("fixture", {})
    for key in ("fixture_bytes", "regular_files"):
        if key in fixture:
            result["input." + key] = fixture[key]
    return {"metrics": result, "steps": [steps[key] for key in sorted(steps)], "verification": verification,
            "resources": {"host": host, "store": stores, "spool": spool},
            "preparation": {"cache_disposition": acquired.get("cache_disposition"), "run_acquisition_reused": reused,
                            "clone_method": clone.get("clone_method"), "key": acquired.get("key")},
            "reduction_scope": "Named public/work counters sum; maxima never sum; CPU/I/O are before-to-after-product differences (failure falls back to final); Store changes are signed product-boundary differences. Spool boundary maxima are distinct from mutation-event high-water."}


def cgroup_observations(path, required_scope_ns):
    required = {"memory.current", "memory.peak", "memory.swap.current", "pids.current", "memory.events.oom", "memory.events.oom_kill", "cpu.stat.usage_usec"}
    required.update("memory.stat." + key for key in ("anon", "file", "file_dirty", "file_writeback", "shmem", "kernel", "slab"))
    first = last = None
    first_fields = last_fields = None
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
                first_fields = fields.copy()
            else:
                gap = max(gap, stamp - last)
            if last_fields is not None and fields["cpu.stat.usage_usec"] < last_fields["cpu.stat.usage_usec"]:
                raise ValueError("cumulative cgroup CPU counter regressed")
            last = stamp
            last_fields = fields.copy()
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
            "cpu_usage_usec_start": first_fields["cpu.stat.usage_usec"],
            "cpu_usage_usec_end": last_fields["cpu.stat.usage_usec"],
            "cpu_usage_usec_delta": last_fields["cpu.stat.usage_usec"] - first_fields["cpu.stat.usage_usec"],
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


def host_rss_termination(records, outcome):
    events = [row for row in records if row["kind"] == "resource-failure"]
    if not events:
        return None
    if len(events) != 1 or set(events[0]) != {"kind", "host_rss_bytes"}:
        raise ValueError("malformed or duplicate host RSS termination")
    rss = number(events[0]["host_rss_bytes"], "watchdog host_rss_bytes")
    if rss <= 2 * GIB or outcome.get("exit_code") != 125 or outcome.get("product_status") != "fail":
        raise ValueError("host RSS termination threshold/exit/outcome mismatch")
    return rss


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
    complete = [row for row in records if row["kind"] in {"sample-complete", "fast-verification-complete"}]
    required_scope = number(complete[0].get("host_orchestration_ns"), "host_orchestration_ns") if len(complete) == 1 else duration
    recovered = [row for row in records if row["kind"] == "recovery"]
    if not successful and recovered:
        cleanup = [row for row in records if row["kind"] == "workspace-spool-observation" and row.get("phase") == "failure-after-discard-cleanup"]
        require(len(recovered) == 1 and recovered[0].get("status", "").startswith("Ok("), "failed attempt recovery did not succeed", issues)
        require(len(cleanup) == 1 and all(cleanup[0].get(key) == 0 for key in ("logical_bytes", "allocated_bytes", "file_count")), "failed attempt recovery cleanup not observed", issues)
        require(any(row["kind"] == "host-resources" and row.get("phase") == "final" for row in records) and any(row["kind"] == "host-rss-samples" for row in records), "failed attempt final observer boundary missing", issues)
        operations = operation_rows(records, issues)
        required_scope = sum(number(row.get("elapsed_ns"), "phase elapsed_ns") for row in records if row["kind"] == "phase")
        required_scope += sum(row["service_ns"] + row["queue_ns"] for row in operations if row.get("outcome") == "failed" or row.get("family") == "workspace.end")
        failed_exec_ns = failed_execution(records, case, issues, outcome["mode"] in {"verify", "fast-verify"})
        if failed_exec_ns is not None:
            required_scope += failed_exec_ns
        require(required_scope > 0, "failed attempt lacks observed reached-call duration", issues)
    windows = [row for row in records if row["kind"] == "runtime-observation-window"]
    if outcome["mode"] in {"verify", "fast-verify"}:
        require(len(windows) == 1, "verification lacks complete runtime observation window", issues)
    if windows:
        require(len(windows) == 1, "duplicate runtime observation window", issues)
        window = windows[0]
        require(window.get("scenario_id") == case["scenario_id"] and window.get("mode") == outcome["mode"] and window.get("start_event") == "selected-run-dispatch" and window.get("end_event") == "selected-run-return-before-process-owner-drain", "runtime window identity/boundaries mismatch", issues)
        require(window.get("status") == ("success" if successful else "error"), "runtime observation status contradicts outcome", issues)
        elapsed = number(window.get("elapsed_ns"), "runtime observation elapsed_ns")
        require(0 < elapsed <= duration and (not complete or elapsed >= complete[0]["host_orchestration_ns"]), "runtime observation window does not encompass reached work", issues)
        required_scope = elapsed
    observations, cgroup_failures = cgroup_observations(directory / "cgroup-samples.tsv.gz", required_scope)
    if windows:
        observations["coverage_scope"] = windows[0].get("scope")
        observations["runtime_observation_window_ns"] = required_scope
    elif not successful and recovered:
        observations["coverage_scope"] = "sampler-ready-before-worker through recovery Client lifetime; reached public-call lower bound; full failed orchestration wall unavailable"
        observations["orchestration_wall_available"] = False
    violations.extend(cgroup_failures)
    watchdog_rss = host_rss_termination(records, outcome)
    if watchdog_rss is not None:
        violations.append("host RSS exceeds frozen 2 GiB; watchdog terminated process")
        observations["host_watchdog_rss_bytes"] = watchdog_rss
        observations["host_watchdog_precision"] = "observed threshold-crossing sample; final sampler summary unavailable after process exit"
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
    require(not any(row["kind"] in {"host-rss-failure", "host-resource-failure", "monitor-observation-failure", "spool-observation-failure", "required-observation-failure", "workspace-physical-spool-error"} for row in records), "mandatory native/workspace observer failed", issues)
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
    if not successful and recovered and any(row.get("receipt", {}).get("family") == "workspace.exec" for row in records if row["kind"] == "operation") and case["operation"] not in CLEAN:
        require(bool(physical) or any(row["kind"] == "commit-diagnostics" for row in records), "failed workload lacks required event-observed physical spool high-water", issues)
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


def validate_timing(records, case, issues, verification=False):
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
                require(workload.get("benchmark_verifier_count") == int(verification) and all(workload.get(key) == 0 for key in ("benchmark_reopen_count", "benchmark_injection_count")), "ordinary workload mode/purity mismatch", issues)
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


def failed_git_command(stderr, partial, case, issues):
    errors = re.findall(r"^fs-benchmark-workload: git (\[[^\n]*\]): (.+)$", stderr, re.M)
    if not errors:
        return False
    require(case.get("operation") == "git-tool" and len(errors) == 1, "unexpected Git subprocess failure source", issues)
    commands = [
        ("git_first_status", ["status", "--porcelain=v1", "-z"]),
        ("git_diff", ["diff", "--no-ext-diff", "--binary", "--"]),
        ("git_add", ["add", "-A", "--"]),
        ("git_cached_check", ["diff", "--cached", "--check"]),
        ("git_commit", ["commit", "--no-gpg-sign", "--no-verify", "-m", "layerfs v0.1.3 tool workflow"]),
        ("git_final_status", ["status", "--porcelain=v1", "-z"]),
    ]
    attempted = number(partial.get("git_process_count"), "partial git_process_count")
    require(1 <= attempted <= len(commands), "Git subprocess attempt count outside frozen sequence", issues)
    if 1 <= attempted <= len(commands):
        require(decode(errors[0][0]) == commands[attempted - 1][1], "failed Git command differs from reached sequence", issues)
        for index, (name, _) in enumerate(commands):
            if index < attempted - 1:
                number(partial.get(name + "_ns"), "completed " + name)
            else:
                require(name + "_ns" not in partial, "Git failure claims later/completed subprocess timing", issues)
    require(partial.get("completed_target_count") == case.get("tier"), "Git failed before completing declared native mutations", issues)
    return True


def failed_execution(records, case, issues, verification=False):
    errors = [row.get("original_error", "") for row in records if row["kind"] == "recovery" and "ExecutionReceipt {" in row.get("original_error", "")]
    if not errors:
        return None
    require(len(errors) == 1, "duplicate failed execution receipt", issues)
    text = errors[0]
    require("fresh-process execution failed:" in text and all(token in text for token in ("transport: Daemon", "docker_engine_calls: 0", "daemon_timing: Some(", "truncated: false", "exited: true")), "failed Exec lacks authentic complete daemon output", issues)
    outer = text.partition("ExecutionReceipt {")[2].partition("daemon_timing:")[0]
    fields = debug_numbers(outer)
    phases = ("spawn_ns", "supervisor_queue_ns", "runtime_ns", "drain_ns", "terminal_publication_ns", "unattributed_ns")
    require(all(key in fields for key in (*phases, "elapsed_ns", "total_wall_ns")), "failed Exec timing observation missing", issues)
    if all(key in fields for key in (*phases, "elapsed_ns", "total_wall_ns")):
        require(fields["elapsed_ns"] == fields["total_wall_ns"] == sum(fields[key] for key in phases), "failed Exec timing equation", issues)
    exit_code = re.search(r"exit_code: Some\(([0-9]+)\)", outer)
    require(exit_code is not None and int(exit_code[1]) != 0, "failed Exec lacks actual nonzero exit", issues)
    output = bytearray()
    for block in re.findall(r"OutputChunk \{[^{}]*stream: Stderr,[^{}]*bytes: \[([0-9, ]*)\]", text):
        output.extend(int(value.strip()) for value in block.split(",") if value.strip())
    stderr = output.decode("utf-8")
    partial = receipt("\n".join(line.removeprefix("partial_") for line in stderr.splitlines() if line.startswith("partial_")))
    require(partial.get("scenario_id") == case["scenario_id"], "failed workload identity missing", issues)
    require(partial.get("benchmark_verifier_count") == int(verification and not case["family_id"].startswith("dedup_")) and all(partial.get(key) == 0 for key in ("benchmark_reopen_count", "benchmark_injection_count")), "failed workload mode/purity counters missing or incorrect", issues)
    if case["family_id"].startswith("dedup_"):
        phase = partial.get("failure_phase")
        require(phase in {"file-open", "file-chmod", "file-write", "file-metadata", "file-sync", "directory-metadata", "directory-open", "directory-sync"} and isinstance(partial.get("failure_path"), str), "dedup partial failure boundary missing", issues)
        for attempted_key, completed_key, failed in (("attempted_operations", "completed_operations", str(phase).startswith("file-")), ("attempted_directory_operation_count", "completed_directory_operation_count", str(phase).startswith("directory-")), ("attempted_sync_count", "sync_count", str(phase).endswith("-sync"))):
            require(number(partial.get(attempted_key), attempted_key) == number(partial.get(completed_key), completed_key) + int(failed), "dedup partial completed/failed operation equation", issues)
        inner = number(partial.get("inner_workload_ns"), "partial inner_workload_ns")
    else:
        attempted = number(partial.get("attempted_syscall_count"), "partial attempted_syscall_count")
        completed = number(partial.get("completed_syscall_count"), "partial completed_syscall_count")
        interrupted = number(partial.get("interrupted_syscall_count"), "partial interrupted_syscall_count")
        iterator_failure = bool(re.search(r"^fs-benchmark-workload: readdir .+: .+$", stderr, re.M))
        git_failure = failed_git_command(stderr, partial, case, issues)
        require(attempted == completed + interrupted + (0 if iterator_failure or git_failure else 1), "partial workload counted-call equation", issues)
        if iterator_failure:
            reads = debug_structs(records, "WorkspaceReadReceipt")
            require(sum(fields.get("callback_readdir", 0) + fields.get("callback_readdirplus", 0) for fields, _ in reads) > 0, "iterator failure lacks actual FUSE directory observations", issues)
        inner = number(partial.get("workload_ns"), "partial workload_ns")
    require(inner <= fields.get("elapsed_ns", 0), "partial workload exceeds failed Exec", issues)
    return fields.get("elapsed_ns")


def validate_performance(case, outcome, records, issues, violations, require_complete=True):
    require(not any(row["kind"] in VERIFY_KINDS for row in records), "verification/fault activity contaminated performance", issues)
    complete = [row for row in records if row["kind"] == "sample-complete"]
    if require_complete:
        require(len(complete) == 1 and complete[0].get("status") == "pass", "missing/duplicate successful performance completion", issues)
    else:
        require(not complete, "failed performance contains successful completion", issues)
    final = complete[0] if len(complete) == 1 else {}
    if final:
        for key in ("benchmark_verifier_count", "benchmark_reopen_count", "benchmark_injection_count"):
            require(type(final.get(key)) is int and final[key] == 0, f"missing/nonzero performance purity counter {key}", issues)
        duration = number(final.get("host_orchestration_ns"), "host_orchestration_ns")
        require(isinstance(final.get("orchestration_scope"), str) and bool(final["orchestration_scope"]), "missing host orchestration scope", issues)
        require(duration <= outcome["external_process_wall_ns"], "product lifecycle exceeds supervised worker", issues)
    ops = operation_rows(records, issues)
    actual = Counter(row.get("family") for row in ops)
    expected = Counter({key: value for key, value in expected_calls(case).items() if value})
    succeeded = Counter(row.get("family") for row in ops if row.get("outcome") in {"success", "up_to_date"})
    failed_exec_ns = failed_execution(records, case, issues) if not require_complete else None
    if require_complete:
        require(actual == expected, f"public operation counts: expected {dict(expected)}, observed {dict(actual)}", issues)
    else:
        require(bool(ops), "failed attempt lacks authentic public operation receipts", issues)
        require(all(key in expected and count <= expected[key] for key, count in actual.items()), "failed attempt used extra/unapproved public operations", issues)
        require(any(row.get("outcome") == "failed" for row in ops) or outcome.get("timeout") or any(row["kind"] == "recovery" for row in records) or host_rss_termination(records, outcome) is not None, "failed performance has no reached failure boundary", issues)
        if actual["workspace.commit"]:
            require(succeeded["workspace.create"] == 1 and succeeded["workspace.exec"] == succeeded["workspace.output"] and succeeded["workspace.exec"] + succeeded["workspace.file_range_edit"] >= actual["workspace.commit"] - (1 if case["operation"] in CLEAN else 0), "failed Commit lacks prerequisite public work", issues)
    for operation in ops:
        wanted = "up_to_date" if operation.get("family") == "workspace.commit" and case["operation"] in CLEAN else "success"
        require(operation.get("outcome") in ({wanted} if require_complete else {wanted, "failed"}), "public operation outcome differs from case contract", issues)
    phases = Counter(row.get("phase") for row in records if row["kind"] == "phase")
    if require_complete:
        wanted = {"initialize": 1} if case["input_mode"] == "directory" else {"create": 1, "end": 1, "visibility": 1, "commit": expected["workspace.commit"], "exec": expected["workspace.exec"], "sdk-edit": expected["workspace.file_range_edit"]}
    else:
        # Recovery End is separately timed by its authentic operation receipt.
        recovery = any(row["kind"] == "recovery" for row in records)
        wanted = {"initialize": succeeded["layerstack.initialize"]} if case["input_mode"] == "directory" else {"create": succeeded["workspace.create"], "end": max(0, succeeded["workspace.end"] - int(recovery)), "visibility": succeeded["query"], "commit": succeeded["workspace.commit"], "exec": max(0, min(succeeded["workspace.exec"], succeeded["workspace.output"]) - int(failed_exec_ns is not None)), "sdk-edit": succeeded["workspace.file_range_edit"]}
    require(phases == Counter({key: value for key, value in wanted.items() if value}), "missing/extra reached product phase boundaries", issues)
    if case["family_id"] == "dedup_branch_history":
        for phase in ("commit", "exec", "sdk-edit"):
            ordinals = [row.get("step") for row in records if row["kind"] == "phase" and row.get("phase") == phase]
            require(ordinals == list(range(len(ordinals))), "history public phase ordinals missing/repeated/reordered", issues)
    sums = sum(number(row.get("elapsed_ns"), "phase elapsed_ns") for row in records if row["kind"] == "phase")
    if final:
        require(sums <= duration, "phase sum exceeds product lifecycle", issues)
        require(number(final.get("pure_call_sum_ns"), "pure_call_sum_ns") == sums and sums + number(final.get("orchestration_unattributed_ns"), "orchestration_unattributed_ns") == duration, "host orchestration/pure-call timing equation", issues)
    if case.get("inherited") and sum(row["elapsed_ns"] for row in records if row["kind"] == "phase" and row.get("phase") in {"sdk-edit", "commit", "end"}) > 2_000_000_000:
        violations.append("capped edit/Commit/End exceeds inherited 2-second gate")
    validate_timing(records, case, issues)
    if succeeded["layerstack.initialize"] or (succeeded["workspace.commit"] and case["operation"] not in CLEAN):
        require(bool(debug_structs(records, "CandidateReceipt")), "missing candidate insert/reuse observations", issues)
    if case["input_mode"] == "directory":
        scans = [row for row in records if row["kind"] == "initialization-scan"]
        require(len(scans) == succeeded["layerstack.initialize"], "missing reached public initialization scan receipt", issues)
        return
    if final:
        require(final.get("created_commit_count") == (0 if case["operation"] in CLEAN else expected["workspace.commit"]), "Created/UpToDate trajectory count", issues)
    require(len(debug_structs(records, "WorkspaceCommitReceipt")) == actual["workspace.commit"], "missing incremental Commit phase receipts", issues)
    require(len(debug_structs(records, "WorkspaceLifecycleReceipt")) == succeeded["workspace.create"] + succeeded["workspace.end"], "missing reached real-FUSE attach/end observations", issues)
    reads = debug_structs(records, "WorkspaceReadReceipt")
    if succeeded["workspace.output"]:
        require(bool(reads), "missing actual FUSE callback observations", issues)
        for fields, _ in reads:
            mandatory = ["callback_" + name for name in FUSE_CALLBACKS] + ["directory_entries_returned", "directory_nonzero_offset_requests", "kernel_read_bytes"]
            require(all(key in fields for key in mandatory), "incomplete FUSE operation/page metrics", issues)
        require(sum(fields.get("callback_" + name, 0) for fields, _ in reads for name in FUSE_CALLBACKS) > 0, "ordinary workload has no actual kernel callbacks", issues)
    if succeeded["workspace.output"] and case["operation"] in {"directory-content-scan", "directory-metadata-scan"}:
        require(sum(fields.get("callback_readdir", 0) + fields.get("callback_readdirplus", 0) for fields, _ in reads) > 0, "full-tree scan has no FUSE directory pages", issues)
    if actual["workspace.commit"]:
        diagnostics = [row for row in records if row["kind"] == "commit-diagnostics"]
        require(len(diagnostics) == actual["workspace.commit"], "missing incremental Commit work diagnostics", issues)
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


def validate_verification(case, records, issues, require_complete=True):
    family, operation = case["family_id"], case["operation"]
    operations = operation_rows(records, issues)
    allowed = {"layerstack.initialize", "branch.fork", "workspace.create", "workspace.exec", "workspace.output", "workspace.file_range_edit", "workspace.commit", "workspace.end", "query"}
    if family == "workspace_reliability":
        allowed.add("workspace.stop")
    require(all(row.get("family") in allowed for row in operations), "unapproved verifier public operation route", issues)
    validate_timing(records, case, issues, verification=True)
    if family != "workspace_reliability":
        require(not any(row["kind"] in {"fault-reachability", "transaction-fault-reachability", "proof-start"} for row in records), "ordinary verification used unapproved fault route", issues)
    if not require_complete:
        require(bool(operations), "failed verification lacks authentic public operation receipts", issues)
        for row in records:
            if row["kind"] in {"native-verification", "git-semantic-verification", "canonical-verification", "dedup-verification", "capped-verification", "history-transcript", "history-accounting"}:
                value = receipt(row.get("receipt"))
                require(bool(value), "empty reached verification receipt", issues)
                # Failed verifier statuses remain actual failure evidence, never passing proofs.
                require(any(key.endswith("status") or key == "retained_snapshot_count" for key in value), "reached verification receipt lacks result", issues)
        require(not any(row["kind"] in {"verification-complete", "proof-complete"} and row.get("status") == "pass" for row in records), "failed verification contradicts passing completion", issues)
        return
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
        require(sorted(row.get("step") for row in transcripts) == list(range(case["tier"] + 1)), "history transcript coverage", issues)
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
        require(len(rows) == (case["tier"] + 1 if family == "dedup_branch_history" else 1), "independent dedup proof omitted", issues)
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


def validate_canonical_artifacts(directory, issues, case=None, records=(), complete=False):
    packages = []
    for marker in sorted(directory.rglob("canonical-receipt.txt")):
        pairs = [line.partition("=") for line in marker.read_text().splitlines() if line]
        if any(not key or separator != "=" for key, separator, _ in pairs):
            raise ValueError("malformed canonical package receipt")
        value = unique_object([(key, item) for key, _, item in pairs])
        compressed = value.get("artifact_encoding") == "gzip-v1"
        require(value.get("artifact_encoding") in {None, "gzip-v1"}, "unknown canonical artifact encoding", issues)
        if case is not None:
            require(value.get("verification_status") == value.get("canonical_role_status") == "pass" and digest(value.get("canonical_root")) and digest(value.get("oracle_identity")), "retained canonical package status/root/oracle missing", issues)
        if compressed:
            require(value.get("artifact_compressor") == "/usr/bin/gzip -n -6 -c", "canonical artifact compressor identity missing", issues)
        suffix = ".gz" if compressed else ""
        folder = marker.parent
        tables = [("payload-extents.tsv", "path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length", 6), ("file-roots.tsv", "path\tcontent_root", 2)]
        manifests = [folder / (name + suffix) for name in ("independent-manifest.tsv", "persistence-bound-manifest.tsv") if (folder / (name + suffix)).is_file()]
        require(len(manifests) == 1, "canonical artifact lacks exactly one expectation manifest", issues)
        tables += [(path.name.removesuffix(suffix) if suffix else path.name, "workspace-independent-manifest-v1", 7) for path in manifests]
        row_counts = {}
        for filename, header, columns in tables:
            path = folder / (filename + suffix)
            count = 0
            with (gzip.open(path, "rt") if compressed else path.open()) as stream:
                require(stream.readline().rstrip("\n") == header, "canonical artifact header mismatch", issues)
                for line in stream:
                    require(len(line.rstrip("\n").split("\t")) == columns, "canonical artifact row schema mismatch", issues)
                    count += 1
            row_counts[filename] = count
        if case is not None:
            require(row_counts.get("file-roots.tsv") == int(value.get("verified_regular_paths", -1)), "canonical regular-file package count mismatch", issues)
            require(sum(count for name, count in row_counts.items() if name.endswith("manifest.tsv")) == int(value.get("verified_paths", -1)), "canonical manifest path count mismatch", issues)
        packages.append({"path": str(folder.relative_to(directory)), "receipt": value, "table_rows": row_counts})
    if case is None or case["family_id"] == "workspace_reliability":
        return packages
    if case["family_id"] == "dedup_branch_history":
        events = [row for row in records if row["kind"] == "history-canonical"]
        if complete:
            require(sorted(row.get("step") for row in events) == list(range(case["tier"] + 1)), "history canonical event coverage missing", issues)
        expected = {f"verification/history-{row['step']}/canonical-verification": row for row in events}
    else:
        events = [row for row in records if row["kind"] == "canonical-verification"]
        if complete:
            require(len(events) == 1, "complete verifier lacks canonical package event", issues)
        expected = {"verification/canonical-verification": row for row in events}
    actual = {row["path"]: row for row in packages}
    require(set(expected).issubset(actual) and (not complete or set(actual) == set(expected)), "missing/extra canonical snapshot packages", issues)
    for path in set(expected) & set(actual):
        event = expected[path]
        declared = receipt(event["receipt"])
        package = actual[path]["receipt"]
        require(package == declared, "canonical package differs from emitted authenticated receipt", issues)
        if "step" in event:
            accounting = [row for row in records if row["kind"] == "history-accounting" and row.get("step") == event["step"]]
            if accounting:
                account = numeric_values(accounting[0]["receipt"])
                require(account.get("current_canonical_objects") == int(package.get("canonical_unique_objects", -1)) and account.get("current_canonical_bytes") == int(package.get("canonical_unique_bytes", -1)), "current canonical census/accounting mismatch", issues)
            require(event.get("root") == package.get("canonical_root"), "history canonical step/root mismatch", issues)
            if event["step"] > 0:
                published = [row for row in records if row["kind"] == "published-root" and row.get("step") == event["step"] - 1]
                require(len(published) == 1 and published[0].get("root") == event["root"], "history package does not bind published root", issues)
    if case["family_id"] == "dedup_branch_history" and complete:
        validate_canonical_union(directory, records, case, issues)
    return packages


def validate_canonical_union(directory, records, case, issues):
    path = directory / "verification/history-0/canonical-verification/history-canonical-union.tsv.gz"
    accounts = {row["step"]: receipt(row["receipt"]) for row in records if row["kind"] == "history-accounting"}
    roots = {row["step"]: row["root"] for row in records if row["kind"] == "history-canonical"}
    seen, totals, new = {}, Counter(), Counter()
    previous_step = 0
    last_row_step, last_object = None, None
    def check(step):
        value = numeric_values(accounts[step])
        require(roots[step] in seen and seen[roots[step]][0] == "Namespace", "canonical union omits the snapshot namespace root", issues)
        require(accounts[step].get("canonical_union_status") == "pass" and accounts[step].get("canonical_root") == roots[step], "history canonical union status/root missing", issues)
        expected = {"retained_canonical_objects": totals["objects"], "retained_canonical_bytes": totals["bytes"],
                    "retained_regular_payload_canonical_objects": totals["regular_objects"], "retained_regular_payload_canonical_bytes": totals["regular_bytes"],
                    "retained_non_payload_canonical_objects": totals["objects"] - totals["regular_objects"], "retained_non_payload_canonical_bytes": totals["bytes"] - totals["regular_bytes"],
                    "retained_metadata_value_canonical_objects": totals["metadata_objects"], "retained_metadata_value_canonical_bytes": totals["metadata_bytes"],
                    "step_new_canonical_objects": new["objects"], "step_new_canonical_bytes": new["bytes"]}
        for role in {entry[0] for entry in seen.values()}:
            expected[f"retained_canonical_{role}_objects"] = totals[role + "_objects"]
            expected[f"retained_canonical_{role}_bytes"] = totals[role + "_bytes"]
        require(all(value.get(key) == count for key, count in expected.items()), f"history step {step} canonical union arithmetic mismatch", issues)
        new.clear()
    with gzip.open(path, "rt") as stream:
        if stream.readline().rstrip("\n") != "step\troot\tobject_id\trole\tcanonical_bytes\tregular_file\tmetadata_value":
            raise ValueError("canonical union ledger header")
        for line in stream:
            fields = line.rstrip("\n").split("\t")
            if len(fields) != 7:
                raise ValueError("canonical union ledger row")
            step_text, root, object_id, role, length_text, regular, metadata = fields
            step, length = int(step_text), int(length_text)
            if not previous_step <= step <= case["tier"] or roots.get(step) != root or not digest(object_id) or length <= 0 or regular not in {"0", "1"} or metadata not in {"0", "1"} or role not in {"Namespace", "InodeTable", "InodeRecord", "Metadata", "FileState", "FileNode", "Chunk", "Symlink", "DirectoryState", "DirectoryNode"}:
                raise ValueError("canonical union ledger identity/order/length")
            if last_row_step == step and object_id <= last_object:
                raise ValueError("canonical union step object order/duplicate")
            last_row_step, last_object = step, object_id
            while previous_step < step:
                check(previous_step)
                previous_step += 1
            flags = (regular == "1", metadata == "1")
            old = seen.get(object_id)
            if old is not None and (old[:2] != (role, length) or any(was and not now for was, now in zip(old[2:], flags)) or old[2:] == flags):
                raise ValueError("canonical union identity changed or usage did not expand")
            if old is None:
                totals.update(objects=1, bytes=length)
                totals.update({role + "_objects": 1, role + "_bytes": length})
                new.update(objects=1, bytes=length)
            if role == "Chunk" and flags[0] and (old is None or not old[2]):
                totals.update(regular_objects=1, regular_bytes=length)
            if flags[1] and (old is None or not old[3]):
                totals.update(metadata_objects=1, metadata_bytes=length)
            seen[object_id] = (role, length, *flags)
    while previous_step <= case["tier"]:
        check(previous_step)
        previous_step += 1


def validate_git_custody(precommit, reopened, records, successful, issues):
    kinds = Counter(row["kind"] for row in records)
    need_pre = successful or kinds["git-precommit-custody"] or kinds["git-reopen-custody"] or kinds["canonical-verification"]
    need_reopen = successful or kinds["git-reopen-custody"]
    if need_pre:
        require(precommit.is_file() and precommit.stat().st_size > 0, "reached Git pre-Commit custody missing", issues)
    if need_reopen:
        require(precommit.is_file() and reopened.is_file() and precommit.read_bytes() == reopened.read_bytes() and precommit.stat().st_size > 0, "Git pre-Commit/reopen full persistence manifests differ", issues)


def derived_product_status(outcome, violations):
    if outcome.get("coverage_status") != "executed":
        return "not-run" if outcome.get("product_status") == "not-run" else "not-established"
    if outcome.get("product_status") == "pass":
        return "fail" if violations else "pass"
    return "fail" if outcome.get("product_status") == "fail" else "not-established"


@lru_cache(maxsize=None)
def sql_history_status(revision):
    source = subprocess.check_output(["git", "show", f"{revision}:{runner.SQL_CAPTURE_SCHEMA}"], cwd=HERE.parents[1])
    if hashlib.sha256(source).hexdigest() == runner.SQL_CAPTURE_SCHEMA_PAIR[1]:
        return "explicit-opt-in; default capture disabled"
    if b"static SQL_TRACE: std::cell::RefCell<Vec<String>>" in source:
        return "unrequested-unbounded-history; diagnostic-only"
    return "unqualified SQL capture state; no performance claim"


@lru_cache(maxsize=None)
def preparation_compatibility(revision):
    return custody.workspace_preparation_digest(custody.source_identity(revision))


def validate_preparation_selection(directory, acquired, build, case, issues):
    path = directory / "preparation/producer-selection.json"
    if not path.exists():
        require(sql_history_status(build["revision"]) != "explicit-opt-in; default capture disabled", "current source lacks preparation producer selection", issues)
        return
    selection = read(path)
    require(selection.get("runtime_revision") == build["revision"] and selection.get("runtime_image_id") == build["image_id"], "preparation selection runtime identity mismatch", issues)
    selected_build = Path(selection["assets"]) / "evidence/build.json"
    require(custody.sha(selected_build) == selection.get("build_manifest_sha256"), "selected producer build manifest mismatch", issues)
    producer = read(selected_build)
    require(producer.get("status") == "pass" and all(producer.get(key) == selection.get(key) for key in ("revision", "image_id", "binary_sha256")), "selected producer identity differs from sealed build", issues)
    producer["workspace_preparation_compatibility"] = preparation_compatibility(producer["revision"])
    runtime = {**build, "workspace_preparation_compatibility": preparation_compatibility(build["revision"])}
    compatible = producer["workspace_preparation_compatibility"]
    require(selection.get("workspace_preparation_compatibility") == compatible and acquired.get("key_data", {}).get("preparation_compatibility_sha256") == compatible and preparation_compatibility(acquired["producer"]["revision"]) == compatible, "actual acquisition producer/cache compatibility mismatch", issues)
    if producer["revision"] == "3422433020a678a77f88e8a110492ca293c05e30":
        require(case["scenario_id"] != "namespace-subtree-relocate-delete-500", "namespace500 selected the known-failing old producer", issues)
    if compatible != runtime["workspace_preparation_compatibility"]:
        recorded_kind = selection.get("source_compatibility", {}).get("kind")
        require(recorded_kind in {"exact-sql-capture-and-derived-spill-preparation-v1", "exact-sql-capture-and-derived-spill-preparation-v2"}, "unknown preparation source compatibility version", issues)
        expected = runner.preparation_source_compatibility(producer, runtime, legacy_full_helpers=recorded_kind == "exact-sql-capture-and-derived-spill-preparation-v1")
        require(selection.get("source_compatibility") == expected, "missing or altered exact preparation source proof", issues)
    else:
        require(selection.get("source_compatibility") is None, "unexpected preparation compatibility exception", issues)


def validate_fast_receipts(directory, outcome, records, successful, issues):
    require(not any(row["kind"] in {"sample-complete", "verification-complete", "canonical-verification", "native-verification"} for row in records), "fast profile must not emit exhaustive/full completion", issues)
    groups = {kind: [row for row in records if row["kind"] == kind] for kind in FAST_KINDS}
    if not successful:return
    require(all(len(value) == 1 for value in groups.values()), "fast profile lacks unique canonical/native/completion receipts", issues)
    if not all(len(value) == 1 for value in groups.values()):return
    folder = directory / "verifier-exchange/fast-certificate"
    certificate = read(folder / "certificate.json")
    binding = custody.sha(folder / "certificate.json")
    projection = custody.sha(folder / "certificate.tsv")
    require(binding == outcome.get("verification_certificate_identity") and projection == outcome.get("verification_certificate_projection_identity"), "fast certificate JSON/projection seal differs", issues)
    require(certificate.get("schema") == "fast-verification-certificate-v1" and certificate.get("profile") == runner.FAST_PROFILE and certificate.get("assurance") == "fully_verified", "fast certificate is not a full reference proof", issues)
    require(certificate.get("seed") == outcome.get("seed") and certificate.get("input_plan_sha256") == outcome.get("input_identity") and certificate.get("product_seal") == outcome.get("product_identity"), "fast certificate input/seed/product assumptions differ", issues)
    source = Path(certificate["source_attempt"]);custody.verify_manifest(source)
    require(custody.sha(source / "evidence.sha256") == certificate["source_manifest_sha256"], "full certificate source seal changed", issues)
    for name, expected in certificate["artifact_sha256"].items():
        require(custody.sha(folder / name) == expected, "fast certificate copied artifact differs: " + name, issues)
    canonical = receipt(groups["fast-canonical-verification"][0]["receipt"])
    native = receipt(groups["fast-native-verification"][0]["receipt"])
    completion = groups["fast-verification-complete"][0]
    require(completion.get("status") == "fast_iteration_verified" and outcome.get("assurance_status") == "fast_iteration_verified", "fast assurance label is invalid", issues)
    for value in (canonical, native):
        require(value.get("verification_status") == "fast_iteration_verified" and value.get("fully_verified") in {False, "false"} and value.get("certificate_binding") == binding, "fast receipt scope/certificate differs", issues)
    require(canonical.get("full_canonical_census_performed") in {False, "false"}, "fast profile claims full canonical census", issues)
    for key in ("authenticated_namespace_paths", "authenticated_global_inodes", "actual_read_regular_paths", "actual_read_logical_bytes", "skipped_current_store_regular_paths", "skipped_current_store_logical_bytes"):
        number(int(canonical[key]), key)
    for key in ("native_namespace_paths_verified", "native_namespace_types_verified", "changed_paths_declared", "absent_paths_verified", "witness_paths_declared", "selected_metadata_paths_verified", "selected_regular_paths_verified", "selected_regular_bytes_verified", "skipped_untouched_regular_bodies", "skipped_untouched_metadata_paths"):
        number(int(native[key]), key)
    require(int(canonical["authenticated_namespace_paths"]) == int(native["native_namespace_paths_verified"]), "fast current namespace coverage differs", issues)
    require(canonical.get("certificate_root") == certificate["root"] and digest(canonical.get("canonical_root")), "fast canonical root binding missing", issues)
    require(bool(native.get("fast_witness_profile")) and bool(native.get("oracle_scope")), "fast skipped/witness scope missing", issues)
    require(read(directory / "verification/fast-verification/receipts.json") == [row for row in records if row["kind"] in FAST_KINDS], "fast retained receipts differ from raw", issues)


def validate_fast_attempt(outcome, classification, case, build):
    return _validate_attempt(outcome, classification, case, build, fast=True)


def validate_attempt(outcome, classification, case, build):
    return _validate_attempt(outcome, classification, case, build, fast=False)


def _validate_attempt(outcome, classification, case, build, fast=False):
    issues, violations = [], []
    records, observed, resource, details, packages = [], {}, {}, {}, []
    directory = Path(outcome.get("evidence_path", ""))
    successful = outcome.get("product_status") == "pass"
    try:
        custody.verify_manifest(directory)
        if outcome.get("mode") == "performance":
            require(sql_history_status(build["revision"]) == "explicit-opt-in; default capture disabled", "performance timer/resource contamination by unrequested or unqualified SQL history capture", issues)
        sealed = read(directory / "outcome.json")
        require(all(value == outcome.get(key) for key, value in sealed.items()) and not (set(outcome) - set(sealed) - {"previous_evidence_path"}), "ledger differs from sealed outcome", issues)
        require(outcome.get("schema") == "fs-bench-pro-v013-sample-v1" and outcome.get("coverage_status") == "executed", "slot has no executed result", issues)
        for key, value in IDENTITY_FIELDS.items():
            require(outcome.get(key) == build[value], f"source identity mismatch: {key}", issues)
        require(outcome.get("source_arm") in {"baseline", "corrected"} and outcome.get("admission_eligible") is False, "invalid Phase 1 arm/admission scope", issues)
        for key in ("scenario_id", "family_id", "proof_only", "inherited"):
            require(outcome.get(key) == case.get(key, False), f"slot identity mismatch: {key}", issues)
        require(outcome.get("mode") in ({"fast-verify"} if fast else {"performance", "verify"}), "unknown evidence mode", issues)
        if outcome.get("coverage_status") != "executed":
            require(outcome.get("product_status") == "not-run", "unexecuted slot contradicts declared product outcome", issues)
            require(outcome.get("supervisor_cleanup_status") == "pass", "unexecuted attempt cleanup was not recovered", issues)
            issues.append("interrupted before sample execution" if outcome.get("interrupted") else "sample has not executed")
            observed = {key: number(outcome[key], key) for key in ("preparation_ns", "command_wall_ns", "cleanup_ns", "runtime_preparation_ns") if key in outcome}
            details = {"metrics": observed, "steps": [], "verification": [], "scope": "preparation/interruption only; no product latency or success claim"}
            return {"issues": sorted(set(issues)), "violations": [], "metrics": observed, "resource_observations": {}, "observations": details, "canonical_packages": [],
                    "product_status": derived_product_status(outcome, []), "verification_pass": False, "fast_iteration_pass": False}
        records = raw(directory / "raw.jsonl")
        require(not any(row.get("kind") == "product-budget-observation-error" for row in records), "mandatory product-budget observer failed", issues)
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
        validate_preparation_selection(directory, acquired, build, case, issues)
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
                validate_git_custody(precommit, reopened, records, successful, issues)
        require(outcome.get("supervisor_cleanup_status") == "pass", "owned supervisor runtime not recovered", issues)
        require(outcome.get("harness_status") != "fail" and not outcome.get("observer_errors"), "harness/resource observer failed", issues)
        if successful:
            require(outcome.get("exit_code") == 0 and outcome.get("timeout") is False and outcome.get("supervisor_failure") is None, "declared success contradicts worker outcome", issues)
            require(outcome.get("mutable_sample_cleanup_status") == "pass", "successful sample mutable state not cleaned", issues)
        resource = validate_resources(directory, outcome, case, records, successful, issues, violations)
        if outcome["mode"] == "performance":
            validate_performance(case, outcome, records, issues, violations, successful)
        elif fast:
            validate_fast_receipts(directory, outcome, records, successful, issues)
        else:
            validate_verification(case, records, issues, successful)
        if outcome["mode"] == "verify":
            packages = validate_canonical_artifacts(directory, issues, case, records, successful)
        details = observation_data(records, outcome, acquired, clone)
        observed = details["metrics"]
        for key, value in resource.get("maxima", {}).items():
            if key != "cpu.stat.usage_usec":
                observed["cgroup.observed_max." + key] = value
        for key in ("cpu_usage_usec_start", "cpu_usage_usec_end", "cpu_usage_usec_delta"):
            if key in resource:
                observed["cgroup." + key] = resource[key]
        if not successful or violations:
            validate_classification(directory, outcome, classification, issues)
    except (OSError, ValueError, KeyError, TypeError, IndexError, AssertionError, EOFError) as error:
        issues.append(f"invalid/missing evidence: {type(error).__name__}: {error}")
    return {"issues": sorted(set(issues)), "violations": sorted(set(violations)), "metrics": observed, "resource_observations": resource, "observations": details, "canonical_packages": packages,
            "product_status": derived_product_status(outcome, violations), "verification_pass": successful and not fast and outcome.get("mode") == "verify" and not issues and not violations,
            "fast_iteration_pass": successful and fast and not issues and not violations}


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


def qualified_build(assets):
    custody.verify_manifest(assets / "evidence")
    build = read(assets / "evidence/build.json")
    if build.get("schema") != "fs-bench-pro-workspace-build-v1" or build.get("status") != "pass":
        raise ValueError("unqualified build assets")
    if custody.sha(assets / "fs-benchmark-pro") != build["binary_sha256"]:
        raise ValueError("registry executable differs from sealed binary")
    registry = [decode(line) for line in subprocess.check_output([str(assets / "fs-benchmark-pro"), "workspace-registry"], text=True).splitlines()]
    registry_cases(registry)
    return build, registry


def selected_build(builds, case, mode, seed=None):
    seed = case.get("seed") if seed is None else seed
    keys = (f"slot:{case['scenario_id']}:{seed}:{mode}", f"case:{case['scenario_id']}:{mode}", f"family:{case['family_id']}:{mode}", f"family:{case['family_id']}", "default")
    return next(builds[key] for key in keys if key in builds)


NORMATIVE_CONTRACT_FILES = {"docs/roadmap/0.1/0.1.3/" + name + ".md" for name in (
    "README", "testing-rules", "phase-1-handoff", "failure-repair-amendment",
    "execution-contract", "ordinary-execution-contract", "dedup-reliability-execution-contract",
    "capped-inherited-replacements", "payload-create-read", "tiny-file-churn",
    "directory-construction-traversal", "git-tool-workflow", "namespace-mutation",
    "workspace-change-locality", "mixed-load-bearing-workload", "dedup-cross-file",
    "dedup-cdc-locality", "dedup-workspace-reuse", "dedup-branch-history", "workspace-reliability",
)}


RUNTIME_SCOPE_POLICY_REVISION = "b1cf098a024870d79097e07e17c6a17bff4b8eb3"


@lru_cache(maxsize=None)
def runtime_scope_contract_pair(filename):
    if filename not in {"docs/roadmap/0.1/0.1.3/" + name for name in ("README.md", "testing-rules.md", "phase-1-handoff.md")}:
        return None
    return tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{filename}"], cwd=HERE.parents[1])).hexdigest()
                 for revision in (CLEAN_PRE_BUDGET_REVISION, RUNTIME_SCOPE_POLICY_REVISION))


def bridge_dependency_paths(family):
    base = "benchmark/fs-bench-pro/"
    paths = {base + "workspace_common.rs", base + "workload.rs", base + "workspace_registry.rs", base + f"families/{family}.rs"}
    if family.startswith("dedup_"):
        paths.update({base + "dedup_workloads.rs", base + "families/sdk_edit_common.rs"})
    elif family == "edit_length_changing_capped":
        paths.update({base + "families/edit_length_changing.rs", base + "families/sdk_edit_common.rs"})
    elif family == "workspace_reliability":
        paths.add(base + "reliability_workloads.rs")
    else:
        paths.add(base + "ordinary_workloads.rs")
    return paths


def sampler_source_parts(source):
    """Narrow source bridge: preserve signature and every non-sampler byte.

    This known function is top-level, with indented body lines. Reject an
    unfamiliar layout instead of broadening the exclusion into other items.
    """
    marker = b"fn sample_resources() -> Result<()> {\n"
    matches = [match.start() for match in re.finditer(rb"(?m)^" + re.escape(marker), source)]
    if len(matches) != 1:
        raise ValueError("sampler bridge requires exactly the known function signature")
    start = matches[0] + len(marker)
    end = source.find(b"\n}", start)
    if end < 0 or source[end + 2:end + 3] not in {b"", b"\n"}:
        raise ValueError("sampler bridge closing boundary missing")
    body = source[start:end]
    if any(line and not line[:1].isspace() for line in body.splitlines()):
        raise ValueError("sampler bridge encountered an unindented body item")
    return source[:start] + b"    /* source-bound sampler body excluded */" + source[end:], body


def fast_definition_parts(filename, source):
    if filename.endswith("workspace_common.rs"):return source[:source.index(b"pub(crate) fn decode_manifest(")]
    if filename.endswith("ordinary_workloads.rs"):return source.split(b"// BEGIN NATIVE FAST VERIFICATION V1", 1)[0].rstrip()
    if filename.endswith("/workload.rs"):return source[source.index(b"pub(crate) struct Sha256"):]
    raise ValueError("fast profile partial source path is not approved")


@lru_cache(maxsize=None)
def fast_profile_source_proof(revision):
    baseline = runner.FAST_VERIFIER_SOURCE
    if product_tree(baseline) != product_tree(revision):raise ValueError("fast profile changes product source/build inputs")
    pairs = runner.fast_verifier_source_proof(revision)
    changed = set(subprocess.check_output(["git", "diff", "--name-only", baseline, revision, "--", "benchmark/fs-bench-pro"], cwd=HERE.parents[1], text=True).splitlines())
    if changed - set(pairs) - {"benchmark/fs-bench-pro/workspace-runner.py", "benchmark/fs-bench-pro/generate-workspace-report.py"}:raise ValueError("fast profile includes unreviewed benchmark source")
    path = "docs/roadmap/0.1/0.1.3/verification-profiles.md"
    expected = subprocess.check_output(["git", "show", "3eaddf47:" + path], cwd=HERE.parents[1])
    actual = subprocess.check_output(["git", "show", revision + ":" + path], cwd=HERE.parents[1])
    if actual != expected:raise ValueError("fast profile changes authorized assurance contract")
    return {"baseline_revision": baseline, "source_pairs": pairs, "profile_contract_sha256": hashlib.sha256(actual).hexdigest(),
        "scope": "Exact separate fast-verification additions and canonical scratch-allocation reuse. Existing full checks and fixed input/oracles remain; old evidence retains actual source, environment and assurance. No cost equivalence or full-gate credit for fast results."}


def validate_bridge_path(filename, expected, revisions):
    sources = {revision: subprocess.check_output(["git", "show", f"{revision}:{filename}"], cwd=HERE.parents[1]) for revision in revisions}
    if isinstance(expected, str):
        if not digest(expected) or any(hashlib.sha256(source).hexdigest() != expected for source in sources.values()):
            raise ValueError("verification bridge source path hash mismatch")
        return
    if isinstance(expected, dict) and expected.get("comparison") == "fast-verifier-definition-v1":
        if set(expected) != {"comparison", "sha256", "source_sha256"} or set(expected["source_sha256"]) != set(revisions):raise ValueError("invalid fast definition bridge")
        for revision, source in sources.items():
            if hashlib.sha256(source).hexdigest() != expected["source_sha256"][revision] or hashlib.sha256(fast_definition_parts(filename, source)).hexdigest() != expected["sha256"]:raise ValueError("fast definition source binding differs")
            if revision not in {CLEAN_PRE_BUDGET_REVISION, runner.FAST_VERIFIER_SOURCE}:fast_profile_source_proof(revision)
        return
    if filename != "benchmark/fs-bench-pro/workspace_registry.rs" or not isinstance(expected, dict) or set(expected) != {"comparison", "sha256", "function_sha256"} or expected.get("comparison") != "exclude-sample_resources-body-v1" or not digest(expected.get("sha256")) or not isinstance(expected.get("function_sha256"), dict) or set(expected["function_sha256"]) != set(revisions):
        raise ValueError("unapproved partial source bridge")
    for revision, source in sources.items():
        normalized, body = sampler_source_parts(source)
        if hashlib.sha256(normalized).hexdigest() != expected["sha256"] or not digest(expected["function_sha256"][revision]) or hashlib.sha256(body).hexdigest() != expected["function_sha256"][revision]:
            raise ValueError("sampler-only source bridge hash mismatch")


UNLINK_BRIDGE_KIND = "proxy-unlink-no-call-v1"
UNLINK_SOURCE_PATH = "crates/layerfs-fuse/src/proxy_client.rs"
NO_UNLINK_OPERATIONS = {"payload-create", "payload-random-read", "tiny-create", "tiny-stat", "tiny-bulk-create", "directory-construct", "directory-metadata-scan", "directory-content-scan"}


def exclude_known_body(source, signature, closing, marker, indentation):
    matches = [match.start() for match in re.finditer(rb"(?m)^" + re.escape(signature), source)]
    if len(matches) != 1:
        raise ValueError("product bridge requires the exact known source signature")
    start = matches[0] + len(signature)
    end = source.find(closing, start)
    if end < 0:
        raise ValueError("product bridge function/module boundary missing")
    body = source[start:end]
    if any(line and not line.startswith(indentation) for line in body.splitlines()):
        raise ValueError("product bridge encountered an unfamiliar source layout")
    return source[:start] + marker + source[end:], body


def unlink_source_parts(source):
    normalized, body = exclude_known_body(source,
        b"    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()> {\n",
        b"\n    }\n", b"        /* exact unlink body excluded */", b"        ")
    normalized, tests = exclude_known_body(normalized, b"#[cfg(test)]\nmod tests {\n",
        b"\n}\n", b"    /* cfg(test)-only module excluded */", b"    ")
    return normalized, body, tests


def product_tree(revision):
    entries = {}
    data = subprocess.check_output(["git", "ls-tree", "-rz", revision], cwd=HERE.parents[1])
    for record in data.split(b"\0"):
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        name = path.decode()
        if name.startswith(custody.PRODUCT) or name in {"Cargo.toml", "Cargo.lock"} or name.startswith(".cargo/"):
            entries[name] = metadata.decode()
    if UNLINK_SOURCE_PATH not in entries:
        raise ValueError("product bridge proxy source missing")
    return entries


def unlink_source_proof(old_revision, new_revision):
    revisions = (old_revision, new_revision)
    trees = {revision: product_tree(revision) for revision in revisions}
    proxy_modes = [trees[revision].pop(UNLINK_SOURCE_PATH).split()[:2] for revision in revisions]
    if proxy_modes[0] != proxy_modes[1] or trees[old_revision] != trees[new_revision]:
        raise ValueError("unlink-only bridge changed another product/build input")
    normalized, methods, tests = {}, {}, {}
    for revision in revisions:
        source = subprocess.check_output(["git", "show", f"{revision}:{UNLINK_SOURCE_PATH}"], cwd=HERE.parents[1])
        remaining, method, test_module = unlink_source_parts(source)
        normalized[revision] = hashlib.sha256(remaining).hexdigest()
        methods[revision] = hashlib.sha256(method).hexdigest()
        tests[revision] = hashlib.sha256(test_module).hexdigest()
    if normalized[old_revision] != normalized[new_revision] or methods[old_revision] == methods[new_revision]:
        raise ValueError("product bridge is not exactly the reviewed unlink method change")
    return {"changed_path": UNLINK_SOURCE_PATH, "normalized_proxy_sha256": normalized[old_revision],
            "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[old_revision], sort_keys=True).encode()).hexdigest(),
            "unlink_body_sha256": methods, "cfg_test_module_sha256": tests}


EMPTY_GENERATION_BRIDGE_KIND = "unlink-and-empty-generation-predicate-v1"
INSTALL_EDIT_PATH = "crates/layerfs-workspace/src/file_io.rs"
INSTALL_EDIT_TEST_PATH = "crates/layerfs-workspace/tests/file_edit.rs"
CREATION_OPERATIONS = {"payload-create", "tiny-create", "tiny-bulk-create"}
READONLY_OPERATIONS = {"payload-random-read", "tiny-stat", "directory-metadata-scan", "directory-content-scan"}
EMPTY_GENERATION_ZERO_WORK = ("workload_ftruncate_call_count", "workload_unlink_call_count", "workload_rmdir_call_count", "workload_rename_call_count", "editor_save_count", "inplace_edit_count", "git_process_count")
EMPTY_GENERATION_PREFIX = b"""        // A successful explicit empty state retires the prior logical edit
        // generation. Keep spool history for in-flight reads; only subsequent
        // mutations receive a fresh edit budget, after existing admission checks.
        let emptied = old.len() != 0 && next.len() == 0;
"""


def empty_generation_source_proof(old_revision, new_revision):
    revisions = (old_revision, new_revision)
    trees = {revision: product_tree(revision) for revision in revisions}
    modes = {}
    for path in (UNLINK_SOURCE_PATH, INSTALL_EDIT_PATH, INSTALL_EDIT_TEST_PATH):
        modes[path] = [trees[revision].pop(path).split()[:2] for revision in revisions]
    if any(pair[0] != pair[1] for pair in modes.values()) or trees[old_revision] != trees[new_revision]:
        raise ValueError("empty-generation bridge changed another product/layout/limit/build input")
    files = {revision: {path: subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1])
             for path in (UNLINK_SOURCE_PATH, INSTALL_EDIT_PATH, INSTALL_EDIT_TEST_PATH)} for revision in revisions}
    old_io, new_io = files[old_revision][INSTALL_EDIT_PATH], files[new_revision][INSTALL_EDIT_PATH]
    old_assignment = b"        *current_edits = edits;"
    new_assignment = b"        *current_edits = if emptied { 0 } else { edits };"
    # Unlike the uncalled unlink body, the installer may run. Therefore permit
    # only this exact two-part transformation, never arbitrary method changes.
    if new_io.count(EMPTY_GENERATION_PREFIX) != 1 or new_io.count(new_assignment) != 1 or old_io.count(old_assignment) != 1:
        raise ValueError("unknown empty-generation retirement implementation")
    restored = new_io.replace(EMPTY_GENERATION_PREFIX, b"", 1).replace(new_assignment, old_assignment, 1)
    if restored != old_io:
        raise ValueError("installer false-predicate path or other file bytes changed")
    proxy = {revision: unlink_source_parts(files[revision][UNLINK_SOURCE_PATH]) for revision in revisions}
    if proxy[old_revision][0] != proxy[new_revision][0]:
        raise ValueError("proxy changed outside the exact unlink/test bodies")
    workload_paths = set().union(*(bridge_dependency_paths(family) for family in ("payload_create_read", "tiny_file_churn", "directory_construction_traversal")))
    workload = {}
    for path in sorted(workload_paths):
        values = {revision: subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1]) for revision in revisions}
        if values[old_revision] == values[new_revision]:
            workload[path] = hashlib.sha256(values[old_revision]).hexdigest()
        elif path == "benchmark/fs-bench-pro/workspace_registry.rs":
            parts = {revision: sampler_source_parts(value) for revision, value in values.items()}
            if parts[old_revision][0] != parts[new_revision][0]:
                raise ValueError("workload dispatch changed outside observer sampler")
            workload[path] = {"comparison": "exclude-sample_resources-body-v1", "sha256": hashlib.sha256(parts[old_revision][0]).hexdigest(),
                              "function_sha256": {revision: hashlib.sha256(part[1]).hexdigest() for revision, part in parts.items()}}
        else:
            raise ValueError("creation/read-only fixture or operation predicate source changed")
    return {"changed_runtime_paths": [UNLINK_SOURCE_PATH, INSTALL_EDIT_PATH],
            "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[old_revision], sort_keys=True).encode()).hexdigest(),
            "normalized_proxy_sha256": hashlib.sha256(proxy[old_revision][0]).hexdigest(),
            "unlink_body_sha256": {revision: hashlib.sha256(parts[1]).hexdigest() for revision, parts in proxy.items()},
            "cfg_test_module_sha256": {revision: hashlib.sha256(parts[2]).hexdigest() for revision, parts in proxy.items()},
            "install_edit_transform": "exact-prefix-predicate-and-conditional-edit-counter-v1",
            "install_edit_file_sha256": {revision: hashlib.sha256(files[revision][INSTALL_EDIT_PATH]).hexdigest() for revision in revisions},
            "restored_installer_file_sha256": hashlib.sha256(restored).hexdigest(),
            "integration_test_only": {"path": INSTALL_EDIT_TEST_PATH, "sha256": {revision: hashlib.sha256(files[revision][INSTALL_EDIT_TEST_PATH]).hexdigest() for revision in revisions}},
            "workload_predicate_source": workload,
            "performance_claim_scope": "original-producing-source-only",
            "new_guard_instruction_cost": "not-measured-by-retained-timings"}


def validate_empty_generation_records(records, case, issues):
    validate_no_unlink_records(records, issues)
    operation = case.get("operation")
    require(operation in NO_UNLINK_OPERATIONS, "operation has no approved no-empty-transition predicate", issues)
    operations = [row.get("receipt", {}) for row in records if row["kind"] == "operation"]
    require(not any(row.get("family") == "workspace.file_range_edit" for row in operations), "retained attempt used owner-side range edits", issues)
    workloads = [receipt(row["workload_receipt"]) for row in records if row["kind"] == "phase" and row.get("phase") == "exec"]
    require(len(workloads) == 1, "predicate bridge needs one complete ordinary workload receipt", issues)
    for workload in workloads:
        require(workload.get("scenario_id") == case["scenario_id"] and workload.get("workload_status") == "pass", "predicate bridge workload identity/status mismatch", issues)
        require(all(workload.get(key) == 0 for key in EMPTY_GENERATION_ZERO_WORK), "retained attempt shrank/replaced/deleted files or lacks required operation counters", issues)
        if operation in READONLY_OPERATIONS or operation == "directory-construct":
            require(all(workload.get(key) == 0 for key in ("completed_write_bytes", "completed_file_write_count", "workload_pwrite_call_count")), "no-installer operation performed file writes", issues)
        elif operation in CREATION_OPERATIONS:
            # Exact source proof pins create_new=true and positive monotone
            # pwrite lengths. Empty created files remain old.len()==0.
            expected_files = 1 if operation == "payload-create" else case["tier"] if operation == "tiny-create" else 200 * case["tier"]
            require(workload.get("completed_file_write_count") == expected_files, "exclusive-creation file count differs from frozen case", issues)
    return {"state_compatibility": "empty-retirement-predicate-false" if operation in CREATION_OPERATIONS else "installer-not-called",
            "timing_claim": "original producing source only", "current_guard_instruction_cost": "unmeasured" if operation in CREATION_OPERATIONS else "changed installer not called"}


SPILL_INDEX_BRIDGE_KIND = "derived-spill-index-source-baseline-v1"
SPILL_INDEX_PARENT = "e7840da1da81404ff228be734a91783cebb946ca"
SPILL_INDEX_PATH = "crates/layerfs-layerstack-store/src/objects.rs"
SPILL_INDEX_SOURCE_HASHES = ("1e88000d97560d5d9d8afdaaf379144cfd859133897650f357c8299a19b3aa32",
                             "4b07eb03a2e6ddfe926a2c5fa621db462c659ff1ee164e41ec3b90cb871df9c8")
SPILL_INDEX_EXTRA_CASES = ({f"{operation}-{tier}" for operation in ("tiny-unlink", "tiny-bulk-delete", "git-tool") for tier in (1,10,100,500)}
                          | {f"namespace-subtree-relocate-delete-{tier}" for tier in (1,10,100)}
                          | {"workspace-sustained-600s-proof"})


def spill_index_source_proof(old_revision, new_revision):
    # This is an exact reviewed source pair, not permission for arbitrary index
    # rewrites. Earlier repairs compose only through their existing strict proof.
    revisions = (SPILL_INDEX_PARENT, new_revision)
    trees = {revision: product_tree(revision) for revision in revisions}
    modes = [trees[revision].pop(SPILL_INDEX_PATH).split()[:2] for revision in revisions]
    if modes[0] != modes[1] or trees[revisions[0]] != trees[revisions[1]]:
        raise ValueError("spill-index bridge changed another product/build input")
    hashes = tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{SPILL_INDEX_PATH}"], cwd=HERE.parents[1])).hexdigest() for revision in revisions)
    if hashes != SPILL_INDEX_SOURCE_HASHES:
        raise ValueError("unreviewed spill-index source pair")
    prior = None if old_revision == SPILL_INDEX_PARENT else empty_generation_source_proof(old_revision, SPILL_INDEX_PARENT)
    families = ("payload_create_read", "tiny_file_churn", "directory_construction_traversal", "git_tool_workflow", "namespace_mutation", "workspace_reliability")
    unchanged = {}
    for path in sorted(set().union(*(bridge_dependency_paths(family) for family in families))):
        values = [subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1]) for revision in revisions]
        if values[0] != values[1]:
            raise ValueError("spill-index bridge changed a retained fixture/workload/oracle definition")
        unchanged[path] = hashlib.sha256(values[0]).hexdigest()
    return {"changed_path": SPILL_INDEX_PATH, "reviewed_source_sha256": dict(zip(revisions, hashes)),
            "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[revisions[0]], sort_keys=True).encode()).hexdigest(),
            "unchanged_workload_paths": unchanged, "prior_predicate_proof": prior,
            "state_compatibility": "derived-offset-index-preserves-canonical-byte-and-collision-results",
            "qualification_test": "objects::tests::spill_index_overflow_preserves_lookup_dedup_and_cleanup_without_scans",
            "performance_claim_scope": "original-producing-source-only",
            "new_index_resource_and_instruction_cost": "not-measured-by-retained-observations"}


def validate_spill_index_records(records, case, bridge, issues):
    prior = bridge["source_proof"]["prior_predicate_proof"]
    scope = validate_empty_generation_records(records, case, issues) if prior is not None else None
    require(case["operation"] in NO_UNLINK_OPERATIONS or case["scenario_id"] in SPILL_INDEX_EXTRA_CASES,
            "case outside the explicit reviewed spill-index baseline scope", issues)
    return {"state_compatibility": "derived-index-logical-results-compatible", "prior_predicate_scope": scope,
            "timing_claim": "original producing source only", "current_index_resource_and_instruction_cost": "unmeasured"}


CONTENT_FRONTIER_BRIDGE_KIND = "bounded-content-frontier-source-baseline-v1"
CONTENT_FRONTIER_PARENT = "a40b17e05486e5b747b689e7710475d739556a69"
CONTENT_FRONTIER_PATH = "crates/layerfs-workspace/src/changes.rs"
CONTENT_FRONTIER_SOURCE_HASHES = ("65d3914cda565ee333832b6da83c1246a0d59dc24ac06e5f859672d4a8378563",
                                  "ea0e3a21653baaaecf35a5d9bfa59da2f33485a003b0a026936fba3f203c08a7")
CONTENT_FRONTIER_EXTRA_CASES = ({f"{operation}-{tier}" for operation in ("workspace-clean-commit", "workspace-fixed-move", "workspace-distributed-sdk-edit") for tier in (1,10,100,500)}
                              | {"workspace-dense-rewrite-1", "workspace-dense-rewrite-10", "namespace-subtree-relocate-delete-500"})


REBASE_LIFETIME_PARENT = "d1325d7f44ef205f5fa748130f3b9868973e9edc"
REBASE_LIFETIME_PATH = "crates/layerfs-workspace/src/lifecycle.rs"
REBASE_LIFETIME_SOURCE_HASHES = ("aa14df244edb8aed1c4cb5cff94ed1fa530cf2610785d89e559c2a67735292f8",
                                "91e65bf1ab13f452a15dab7e6934e1256a39f508a39efbc0b2a00c718ad0c2ac")
REBASE_LIFETIME_EXTRA_CASES = {"workspace-dense-rewrite-100", "workspace-dense-rewrite-500"}


def content_frontier_source_proof(old_revision, new_revision):
    # Continue the existing exact-pair chain for the reviewed rebase lifetime
    # change; this does not create a general compatibility or cost waiver.
    trees = {revision: product_tree(revision) for revision in (REBASE_LIFETIME_PARENT, new_revision)}
    if trees[REBASE_LIFETIME_PARENT][REBASE_LIFETIME_PATH] != trees[new_revision][REBASE_LIFETIME_PATH]:
        revisions = (REBASE_LIFETIME_PARENT, new_revision)
        modes = [trees[revision].pop(REBASE_LIFETIME_PATH).split()[:2] for revision in revisions]
        if modes[0] != modes[1] or trees[revisions[0]] != trees[revisions[1]]:
            raise ValueError("rebase lifetime bridge changed another product/build input")
        hashes = tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{REBASE_LIFETIME_PATH}"], cwd=HERE.parents[1])).hexdigest() for revision in revisions)
        if hashes != REBASE_LIFETIME_SOURCE_HASHES:
            raise ValueError("unreviewed rebase lifetime source pair")
        prior = None if old_revision == REBASE_LIFETIME_PARENT else content_frontier_source_proof(old_revision, REBASE_LIFETIME_PARENT)
        families = ("payload_create_read", "tiny_file_churn", "directory_construction_traversal", "git_tool_workflow", "namespace_mutation", "workspace_change_locality", "workspace_reliability")
        unchanged = {}
        for path in sorted(set().union(*(bridge_dependency_paths(family) for family in families))):
            values = [subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1]) for revision in revisions]
            if values[0] != values[1]:
                raise ValueError("rebase lifetime bridge changed a retained fixture/workload/oracle definition")
            unchanged[path] = hashlib.sha256(values[0]).hexdigest()
        return {"rebase_lifetime_source": {"path": REBASE_LIFETIME_PATH, "reviewed_source_sha256": dict(zip(revisions, hashes))},
                "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[revisions[0]], sort_keys=True).encode()).hexdigest(),
                "unchanged_workload_paths": unchanged, "prior_content_frontier_proof": prior,
                "state_compatibility": "same-live-nodes-canonical-identity-aliases-pins-and-spool-state",
                "qualification_test": "lifecycle::tests::rebase_streams_nodes_preserving_identity_aliases_and_pinned_spools",
                "performance_claim_scope": "original-producing-source-only",
                "new_rebase_allocation_and_instruction_cost": "not-measured-by-retained-observations"}
    revisions = (CONTENT_FRONTIER_PARENT, new_revision)
    trees = {revision: product_tree(revision) for revision in revisions}
    modes = [trees[revision].pop(CONTENT_FRONTIER_PATH).split()[:2] for revision in revisions]
    if modes[0] != modes[1] or trees[revisions[0]] != trees[revisions[1]]:
        raise ValueError("content-frontier bridge changed another product/build input")
    hashes = tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{CONTENT_FRONTIER_PATH}"], cwd=HERE.parents[1])).hexdigest() for revision in revisions)
    if hashes != CONTENT_FRONTIER_SOURCE_HASHES:
        raise ValueError("unreviewed content-frontier source pair")
    prior = None if old_revision == CONTENT_FRONTIER_PARENT else spill_index_source_proof(old_revision, CONTENT_FRONTIER_PARENT)
    families = ("payload_create_read", "tiny_file_churn", "directory_construction_traversal", "git_tool_workflow", "namespace_mutation", "workspace_change_locality", "workspace_reliability")
    unchanged = {}
    for path in sorted(set().union(*(bridge_dependency_paths(family) for family in families))):
        values = [subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1]) for revision in revisions]
        if values[0] != values[1]:
            raise ValueError("content-frontier bridge changed a retained fixture/workload/oracle definition")
        unchanged[path] = hashlib.sha256(values[0]).hexdigest()
    return {"changed_path": CONTENT_FRONTIER_PATH, "reviewed_source_sha256": dict(zip(revisions, hashes)),
            "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[revisions[0]], sort_keys=True).encode()).hexdigest(),
            "unchanged_workload_paths": unchanged, "prior_spill_index_proof": prior,
            "state_compatibility": "structural-frontier-clean-or-fitting-content-plan-results-preserved",
            "qualification_test": "changes::tests::dense_existing_file_delta_uses_bounded_frontier_and_preserves_aliases",
            "performance_claim_scope": "original-producing-source-only",
            "new_planner_instruction_cost": "not-measured-by-retained-observations"}


def validate_content_frontier_records(records, case, bridge, issues):
    if "rebase_lifetime_source" in bridge["source_proof"]:
        prior = bridge["source_proof"]["prior_content_frontier_proof"]
        scope = validate_content_frontier_records(records, case, {"source_proof": prior}, issues) if prior is not None else None
        if prior is None:
            require(case["scenario_id"] in REBASE_LIFETIME_EXTRA_CASES, "unreviewed direct rebase baseline case", issues)
            if case["scenario_id"] == "workspace-dense-rewrite-500":
                starts = [row for row in records if row["kind"] == "sample-start"]
                require(len(starts) == 1 and starts[0].get("seed") == 1 and not any(row["kind"] == "resource-failure" for row in records),
                        "only the completed pre-repair dense500 seed1 observation is reusable", issues)
        return {"state_compatibility": "same-live-nodes-canonical-identity-aliases-pins-and-spool-state", "prior_planner_scope": scope,
                "timing_claim": "original producing source only", "current_rebase_allocation_and_instruction_cost": "unmeasured"}
    prior = bridge["source_proof"]["prior_spill_index_proof"]
    scope = validate_spill_index_records(records, case, {"source_proof": prior}, issues) if prior is not None else None
    require(case["operation"] in NO_UNLINK_OPERATIONS or case["scenario_id"] in SPILL_INDEX_EXTRA_CASES | CONTENT_FRONTIER_EXTRA_CASES,
            "case outside the explicit fitting/structural content-frontier baseline scope", issues)
    return {"state_compatibility": "clean-structural-or-fitting-content-results-compatible", "prior_spill_index_scope": scope,
            "timing_claim": "original producing source only", "current_planner_instruction_cost": "unmeasured"}


RETAINED_PROOF_KIND = "completed-independent-proof-state-only-v1"
RETAINED_PROOFS = {
    "fbf32e84662d00993c033515e113437965395494": ("payload-create-1m", "payload-create-1m-s1-verify-09ef8212a24f"),
    "e7840da1da81404ff228be734a91783cebb946ca": ("workspace-sustained-600s-proof", "workspace-sustained-600s-proof-s1-verify-01219f621176"),
}
SQL_CAPTURE_PARENT = "d6fdf964464ecb6f4a1188c69ee4bbd2e06c3f9c"


def retained_proof_source_proof(old_revision, new_revision):
    if old_revision not in RETAINED_PROOFS:raise ValueError("unreviewed retained independent proof")
    revisions = (SQL_CAPTURE_PARENT, new_revision)
    trees = {revision: product_tree(revision) for revision in revisions}
    modes = [trees[revision].pop(runner.SQL_CAPTURE_SCHEMA).split()[:2] for revision in revisions]
    if modes[0] != modes[1] or trees[revisions[0]] != trees[revisions[1]]:raise ValueError("proof retention changed another product input after rebase qualification")
    hashes = tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{runner.SQL_CAPTURE_SCHEMA}"], cwd=HERE.parents[1])).hexdigest() for revision in revisions)
    if hashes != runner.SQL_CAPTURE_SCHEMA_PAIR:raise ValueError("unreviewed SQL capture state-equivalence pair")
    return {"retained_evidence_basename": RETAINED_PROOFS[old_revision][1],
            "prior_state_identity": content_frontier_source_proof(old_revision, SQL_CAPTURE_PARENT),
            "sql_capture_source_pair": dict(zip(revisions, hashes)),
            "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(trees[revisions[0]], sort_keys=True).encode()).hexdigest(),
            "scope": "Exactly the already-completed independent verification, including its original resource/deadline observations. SQL history changes no canonical/presentation state. No old performance timing is admitted and no current resource cost is inferred."}


def configured_product_bridges(config, primary, cases):
    approved = []
    fields = {"kind", "old_revision", "new_revision", "old_product_seal", "new_product_seal", "case_ids", "source_proof", "required_zero_counters", "reviewed_impact"}
    for bridge in config.get("product_compatibility", []):
        if not isinstance(bridge, dict) or set(bridge) != fields or bridge["kind"] not in {UNLINK_BRIDGE_KIND, EMPTY_GENERATION_BRIDGE_KIND, SPILL_INDEX_BRIDGE_KIND, CONTENT_FRONTIER_BRIDGE_KIND, RETAINED_PROOF_KIND} or bridge["new_revision"] != primary["revision"] or bridge["old_revision"] == bridge["new_revision"] or bridge["new_product_seal"] != primary["product_seal"] or not digest(bridge["old_product_seal"]) or bridge["old_product_seal"] == bridge["new_product_seal"]:
            raise ValueError("invalid exact unlink product bridge identity")
        retained_proof = bridge["kind"] == RETAINED_PROOF_KIND
        if retained_proof:
            if bridge["old_revision"] not in RETAINED_PROOFS or bridge["case_ids"] != [RETAINED_PROOFS[bridge["old_revision"]][0]] or bridge["required_zero_counters"] != [] or len(bridge["reviewed_impact"].strip()) < 80:
                raise ValueError("only the two completed independent proofs may be retained")
            if bridge["source_proof"] != retained_proof_source_proof(bridge["old_revision"], bridge["new_revision"]):raise ValueError("retained proof source-state identity differs")
            if any(other["old_revision"] == bridge["old_revision"] for other in approved):raise ValueError("duplicate retained proof source")
            approved.append(bridge)
            continue
        spill = bridge["kind"] == SPILL_INDEX_BRIDGE_KIND
        frontier = bridge["kind"] == CONTENT_FRONTIER_BRIDGE_KIND
        direct_spill = (spill or frontier) and bridge["old_revision"] == SPILL_INDEX_PARENT
        direct_frontier = frontier and bridge["old_revision"] == CONTENT_FRONTIER_PARENT
        direct_rebase = frontier and bridge["old_revision"] == REBASE_LIFETIME_PARENT
        counters = [] if direct_spill or direct_frontier or direct_rebase else ["callback_unlink", "callback_rmdir"]
        allowed = lambda case: case in cases and (cases[case]["operation"] in NO_UNLINK_OPERATIONS or direct_spill and case in SPILL_INDEX_EXTRA_CASES or direct_frontier and case in CONTENT_FRONTIER_EXTRA_CASES or direct_rebase and case in REBASE_LIFETIME_EXTRA_CASES)
        if bridge["required_zero_counters"] != counters or not isinstance(bridge["case_ids"], list) or not bridge["case_ids"] or len(set(bridge["case_ids"])) != len(bridge["case_ids"]) or any(not allowed(case) for case in bridge["case_ids"]) or not isinstance(bridge["reviewed_impact"], str) or len(bridge["reviewed_impact"].strip()) < 80:
            raise ValueError("product bridge lacks explicit reviewed case/observation scope")
        prove = content_frontier_source_proof if frontier else spill_index_source_proof if spill else empty_generation_source_proof if bridge["kind"] == EMPTY_GENERATION_BRIDGE_KIND else unlink_source_proof
        if bridge["source_proof"] != prove(bridge["old_revision"], bridge["new_revision"]):
            raise ValueError("product bridge source proof differs from committed bytes")
        if any(other["old_revision"] == bridge["old_revision"] for other in approved):
            raise ValueError("duplicate product bridge source")
        approved.append(bridge)
    return approved


def matching_product_bridge(bridges, build, primary, case_id):
    if build["product_seal"] == primary["product_seal"]:
        return None
    matches = [item for item in bridges if item["old_revision"] == build["revision"] and item["old_product_seal"] == build["product_seal"] and item["new_revision"] == primary["revision"] and case_id in item["case_ids"]]
    if len(matches) != 1:
        raise ValueError("selected old product has no exact no-unlink case bridge")
    return matches[0]


def validate_no_unlink_records(records, issues):
    reads = debug_structs(records, "WorkspaceReadReceipt")
    require(bool(reads), "retained product bridge lacks complete FUSE observations", issues)
    require(all(fields.get("callback_unlink") == 0 and fields.get("callback_rmdir") == 0 for fields, _ in reads), "retained attempt reached or failed to observe the changed unlink/rmdir path", issues)


CLEAN_PRE_BUDGET_REVISION = "6c54f8d74a8f07867c6b658da674603c4be6a7c3"
BUDGET_HOST_SOURCE_PAIRS = {
    "benchmark/fs-bench-pro/src/workspace_bench.rs": ("0a0df8c560928ac916aae8ce984b683f65085c344cd832fc86f9e3ffee51fcb3", "df074a4160010b328db3ecb92d99f82a87f4acacd0bf347e5490d435e9f85771"),
    "benchmark/fs-bench-pro/src/sdk_file_edit.rs": ("8cb37ded94f711c6d0a2d463ab70e76ce5d12557a7579a4bdecc78d6fc1e8e36", "729f4499b652f8bb39f0f90e99dcb9cc4a013650c6db1ac986a5b4283b5bdd6c"),
}
BUDGET_HARNESS_PATHS = {"benchmark/fs-bench-pro/" + path for path in (
    "src/workspace_bench.rs", "src/sdk_file_edit.rs", "workspace-runner.py", "generate-workspace-report.py")}


def runtime_budget_source_proof(old_revision, new_revision):
    """Bind the reviewed timer-only harness change; never assert equal cost."""
    if old_revision != CLEAN_PRE_BUDGET_REVISION or old_revision == new_revision:
        raise ValueError("runtime budget bridge must start at exact clean6c source")
    if sql_history_status(old_revision) != "explicit-opt-in; default capture disabled" or sql_history_status(new_revision) != "explicit-opt-in; default capture disabled":
        raise ValueError("runtime budget bridge cannot admit SQL-contaminated timing")
    old_tree, new_tree = product_tree(old_revision), product_tree(new_revision)
    if old_tree != new_tree:
        raise ValueError("runtime budget bridge changed product source/build inputs")
    if new_revision != runner.FAST_VERIFIER_SOURCE:
        host = subprocess.check_output(["git", "show", new_revision + ":benchmark/fs-bench-pro/src/workspace_bench.rs"], cwd=HERE.parents[1])
        if hashlib.sha256(host).hexdigest() == runner.FAST_VERIFIER_HASHES["src/workspace_bench.rs"]:
            return {"prior_runtime_budget_proof": runtime_budget_source_proof(old_revision, runner.FAST_VERIFIER_SOURCE),
                "fast_verification_source_proof": fast_profile_source_proof(new_revision),
                "scope": "Only completed clean6c <=15s performance; original source/environment cost. Separate exact verifier-only implementation; no full-to-fast assurance conversion."}
    changed = set(subprocess.check_output(["git", "diff", "--name-only", old_revision, new_revision, "--", "benchmark/fs-bench-pro"], cwd=HERE.parents[1], text=True).splitlines())
    if not {"benchmark/fs-bench-pro/src/workspace_bench.rs", "benchmark/fs-bench-pro/src/sdk_file_edit.rs", "benchmark/fs-bench-pro/workspace-runner.py"}.issubset(changed) or changed - BUDGET_HARNESS_PATHS:
        raise ValueError("runtime budget bridge changed unreviewed harness dependencies")
    hashes = {path: {revision: hashlib.sha256(subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=HERE.parents[1])).hexdigest() for revision in (old_revision, new_revision)} for path in sorted(changed)}
    for path, reviewed_pair in BUDGET_HOST_SOURCE_PAIRS.items():
        if tuple(hashes[path][revision] for revision in (old_revision, new_revision)) != reviewed_pair:
            raise ValueError("unreviewed host timer implementation; completed clean6c cannot be silently bridged")
    return {"unchanged_product_tree_sha256": hashlib.sha256(json.dumps(old_tree, sort_keys=True).encode()).hexdigest(),
            "changed_harness_source_sha256": hashes,
            "scope": "Only already completed successful clean6c performance with pure_call_sum_ns<=15000000000. Original-producing-source timing/resources only; no new timer observer instruction-cost equivalence. Active correctness verified separately with unchanged workload/oracle definitions."}


def configured_runtime_budget_bridge(config, primary):
    bridge = config.get("runtime_budget_compatibility")
    if bridge is None:
        return None
    if not isinstance(bridge, dict) or set(bridge) != {"old_revision", "new_revision", "source_proof", "reviewed_impact"} or bridge["new_revision"] != primary["revision"] or not isinstance(bridge["reviewed_impact"], str) or len(bridge["reviewed_impact"].strip()) < 80:
        raise ValueError("malformed source-bound runtime-budget evidence bridge")
    if bridge["source_proof"] != runtime_budget_source_proof(bridge["old_revision"], bridge["new_revision"]):
        raise ValueError("runtime-budget reviewed source hashes differ from actual committed bytes")
    return bridge


def full_verifier_source_proof(old_revision, new_revision):
    """One exact exhaustive-traversal repair; historical timing identity stays intact."""
    if old_revision != runner.HISTORICAL_FULL_VERIFIER_REVISION or new_revision == old_revision:
        raise ValueError("full verifier bridge must start at exact7948 VM8 source")
    old_tree, new_tree = product_tree(old_revision), product_tree(new_revision)
    if old_tree != new_tree:raise ValueError("full verifier bridge changed product/build inputs")
    old_pairs = runner.fast_verifier_source_proof(old_revision)
    new_pairs = runner.fast_verifier_source_proof(new_revision)
    path = "benchmark/fs-bench-pro/src/workspace_verify.rs"
    if old_pairs[path]["new_sha256"] != runner.HISTORICAL_FULL_VERIFIER_SHA256 or new_pairs[path]["new_sha256"] != runner.FAST_VERIFIER_HASHES["src/workspace_verify.rs"]:
        raise ValueError("full verifier source pair differs")
    changed = set(subprocess.check_output(["git", "diff", "--name-only", old_revision, new_revision, "--", "benchmark/fs-bench-pro"], cwd=HERE.parents[1], text=True).splitlines())
    if path not in changed or changed - {path, "benchmark/fs-bench-pro/workspace-runner.py", "benchmark/fs-bench-pro/generate-workspace-report.py"}:
        raise ValueError("full verifier repair changed a fixture/workload/host dependency")
    contracts = {}
    for name in sorted(NORMATIVE_CONTRACT_FILES):
        values = [subprocess.check_output(["git", "show", revision + ":" + name], cwd=HERE.parents[1]) for revision in (old_revision, new_revision)]
        if values[0] != values[1]:raise ValueError("full verifier bridge changed frozen contract: " + name)
        contracts[name] = hashlib.sha256(values[0]).hexdigest()
    return {"old_revision": old_revision, "new_revision": new_revision,
        "old_verifier_sha256": runner.HISTORICAL_FULL_VERIFIER_SHA256,
        "new_verifier_sha256": runner.FAST_VERIFIER_HASHES["src/workspace_verify.rs"],
        "unchanged_product_tree_sha256": hashlib.sha256(json.dumps(old_tree, sort_keys=True).encode()).hexdigest(),
        "unchanged_normative_contract_sha256": contracts,
        "fast_profile_source_proof": fast_profile_source_proof(new_revision),
        "scope": "Only exhaustive verifier namespace lookup changes: authenticated inode index replaces repeated root resolution. Every full byte/metadata/alias/typed-census check remains required. Historical7948 product timings stay at their original source/environment; no successor timing or fast-to-full assurance claim."}


def configured_full_verifier_bridge(config, primary):
    bridge = config.get("full_verifier_compatibility")
    if bridge is None:return None
    if not isinstance(bridge, dict) or set(bridge) != {"old_revision", "new_revision", "source_proof", "reviewed_impact"} or bridge["new_revision"] != primary["revision"] or not isinstance(bridge["reviewed_impact"], str) or len(bridge["reviewed_impact"].strip()) < 80:
        raise ValueError("invalid exact full-verifier compatibility record")
    if bridge["source_proof"] != full_verifier_source_proof(bridge["old_revision"], bridge["new_revision"]):raise ValueError("full verifier source proof differs from committed bytes")
    return bridge


def family_builds(campaign, assets, primary, registry):
    families = {row["family_id"] for row in registry}
    cases = {row["scenario_id"]: row for row in registry}
    selected, provenance, bridges = {"default": primary}, {}, []
    path = campaign / "evidence-builds.json"
    if not path.exists():
        return selected, provenance, bridges
    config = read(path)
    if set(config) - {"schema", "selections", "verification_compatibility", "product_compatibility", "runtime_budget_compatibility", "full_verifier_compatibility"} or config.get("schema") != "fs-bench-pro-scoped-builds-v1" or not isinstance(config.get("selections"), dict):
        raise ValueError("invalid explicit scoped build mapping")
    product_bridges = configured_product_bridges(config, primary, cases)
    runtime_budget_bridge = configured_runtime_budget_bridge(config, primary)
    full_verifier_bridge = configured_full_verifier_bridge(config, primary)
    loaded = {assets.resolve(): (primary, registry)}
    for selector, choice in config["selections"].items():
        parts = selector.split(":")
        valid = len(parts) in {2, 3} and parts[0] == "family" and parts[1] in families and (len(parts) == 2 or parts[2] in {"performance", "verify"})
        valid |= len(parts) == 3 and parts[0] == "case" and parts[1] in cases and parts[2] in {"performance", "verify"}
        if len(parts) == 4 and parts[0] == "slot" and parts[1] in cases and parts[2].isdecimal() and parts[3] in {"performance", "verify", "fast-verify"}:
            selected_case, seed, mode = cases[parts[1]], int(parts[2]), parts[3]
            allowed = [1] if selected_case.get("proof_only") or selected_case.get("inherited") and mode == "verify" else range(1, 6) if selected_case.get("inherited") else range(1, 4)
            valid = str(seed) == parts[2] and seed in allowed and not (selected_case.get("proof_only") and mode != "verify")
            if mode == "fast-verify":
                valid = valid and not selected_case.get("inherited") and selected_case.get("input_mode") == "store" and not selected_case["family_id"].startswith("dedup_")
        if not valid or not isinstance(choice, dict) or set(choice) != {"assets", "reason", "build_manifest_sha256"}:
            raise ValueError("unknown selector or malformed scoped build provenance")
        if not isinstance(choice["reason"], str) or len(choice["reason"].strip()) < 16 or not digest(choice["build_manifest_sha256"]):
            raise ValueError("scoped build lacks meaningful impact reason/seal")
        location = (campaign / choice["assets"]).resolve()
        if custody.sha(location / "evidence/evidence.sha256") != choice["build_manifest_sha256"]:
            raise ValueError("scoped build manifest binding mismatch")
        if location not in loaded:
            loaded[location] = qualified_build(location)
        build, candidate_registry = loaded[location]
        if build["revision"] == CLEAN_PRE_BUDGET_REVISION and primary["revision"] != CLEAN_PRE_BUDGET_REVISION:
            if runtime_budget_bridge is None or selector.split(":")[-1] != "performance" or parts[0] != "slot":
                raise ValueError("old clean6c runtime-budget reuse requires exact completed performance-slot selector and source proof")
        if build["revision"] == runner.HISTORICAL_FULL_VERIFIER_REVISION and primary["revision"] != build["revision"]:
            if full_verifier_bridge is None or parts[0] != "slot" or parts[-1] not in {"performance", "fast-verify"}:
                raise ValueError("historical7948 evidence requires exact slot and full-verifier source bridge")
        if build["revision"] == runner.FAST_VERIFIER_SOURCE and primary["revision"] != build["revision"]:
            if parts[0] != "slot" or parts[-1] != "verify":raise ValueError("old full verifier reuse requires exact completed full-proof slot")
            fast_profile_source_proof(primary["revision"])
        if build.get("product_baseline") != primary.get("product_baseline"):
            raise ValueError("scoped mapping changed the pinned release baseline")
        if build["product_seal"] != primary["product_seal"]:
            if build.get("build_configuration", {}).get("profile") != "release" or primary.get("build_configuration", {}).get("profile") != "release":
                raise ValueError("cfg(test)-excluded product bridge requires release assets")
            selected_cases = [case for case in registry if case["family_id"] == parts[1]] if parts[0] == "family" else [cases[parts[1]]]
            for selected_case in selected_cases:
                bridge = matching_product_bridge(product_bridges, build, primary, selected_case["scenario_id"])
                if bridge["kind"] == RETAINED_PROOF_KIND and selector != f"slot:{selected_case['scenario_id']}:1:verify":
                    raise ValueError("completed proof compatibility cannot select performance or another verification slot")
        # Qualification/results/finding narratives can evolve independently.
        # Every explicitly normative file must exist and remain byte-identical.
        for filename in NORMATIVE_CONTRACT_FILES:
            old_hash, new_hash = build["phase1_contract_files"].get(filename), primary["phase1_contract_files"].get(filename)
            if old_hash is None or new_hash is None or old_hash != new_hash and (old_hash, new_hash) != runtime_scope_contract_pair(filename):
                raise ValueError(f"scoped mapping changed existing frozen contract beyond exact user scope amendment: {filename}")
        family = parts[1] if parts[0] == "family" else cases[parts[1]]["family_id"]
        if [row for row in registry if row["family_id"] == family] != [row for row in candidate_registry if row["family_id"] == family]:
            raise ValueError("scoped build changed frozen registry descriptors")
        selected[selector] = build
        provenance[selector] = {**choice, "assets": str(location), "source": build,
            "full_verifier_compatibility": full_verifier_bridge if build["revision"] == runner.HISTORICAL_FULL_VERIFIER_REVISION and primary["revision"] != build["revision"] else None,
            "runtime_budget_compatibility": runtime_budget_bridge if build["revision"] == CLEAN_PRE_BUDGET_REVISION and primary["revision"] != CLEAN_PRE_BUDGET_REVISION else None,
            "product_compatibility": [item for item in product_bridges if item["old_revision"] == build["revision"]] if build["product_seal"] != primary["product_seal"] else []}
    if any("rebase_lifetime_source" in bridge["source_proof"] for bridge in product_bridges):
        dense = cases["workspace-dense-rewrite-500"]
        if any(selected_build(selected, dense, "verify", seed)["product_seal"] != primary["product_seal"] for seed in (1, 2, 3)):
            raise ValueError("all dense500 verification seeds must exercise the repaired current rebase")
    sources = {build["revision"]: build for build in selected.values()}
    for bridge in config.get("verification_compatibility", []):
        fields = {"family_id", "performance_revision", "verification_revision", "reviewed_impact", "unchanged_paths"}
        if not isinstance(bridge, dict) or set(bridge) != fields or bridge["family_id"] not in families or len(bridge["reviewed_impact"].strip()) < 80:
            raise ValueError("unqualified verification source bridge")
        revisions = [bridge["performance_revision"], bridge["verification_revision"]]
        if revisions[0] == revisions[1] or any(revision not in sources for revision in revisions):
            raise ValueError("verification bridge must name two selected sealed sources")
        required = bridge_dependency_paths(bridge["family_id"])
        if set(bridge["unchanged_paths"]) != required:
            raise ValueError("verification bridge omitted fixed input/workload/oracle definitions")
        for filename, expected in bridge["unchanged_paths"].items():
            validate_bridge_path(filename, expected, revisions)
        if any(all(old.get(key) == bridge[key] for key in ("family_id", "performance_revision", "verification_revision")) for old in bridges):
            raise ValueError("duplicate verification compatibility bridge")
        bridges.append(bridge)
    return selected, provenance, bridges


def source_identity(outcome):
    return {key: outcome.get(key) for key in ("source_arm", *IDENTITY_FIELDS, "environment_identity")}


def source_group(outcome):
    identity = source_identity(outcome)
    return hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest(), identity


def sharing_values(value):
    """Keep exact numerator/denominator next to every derived sharing fraction."""
    output = {}
    for label, numerator, denominator in (
        ("regular_payload_sharing", "distinct_payload_bytes", "regular_file_logical_bytes"),
        ("addition_payload_sharing", "addition_new_payload_bytes", "addition_logical_bytes"),
        ("retained_history_payload_sharing", "distinct_retained_payload_bytes", "retained_logical_snapshot_bytes"),
    ):
        if numerator in value and denominator in value:
            n, d = int(value[numerator]), int(value[denominator])
            output[label] = {"unique_bytes": n, "logical_bytes": d, "saved_fraction": 1 - n / d if d else None,
                             "scope": "logical regular-file payload sharing; excludes canonical wrappers, metadata and physical Store slack; not an emitted-CAS-hit fraction"}
    return output


def verification_summary(observations, packages):
    steps = {}
    for event in observations.get("verification", []):
        step = event.get("step", 1)
        point = steps.setdefault(step, {"step": step})
        value = dict(event["receipt"])
        if event["kind"] in {"canonical-verification", "history-canonical"} and "canonical_unique_bytes" not in value:
            roles = [int(size) for key, size in value.items() if re.fullmatch(r"canonical_[A-Z][A-Za-z]*_bytes", key)]
            if roles:
                value["canonical_unique_bytes"] = str(sum(roles))
        point[event["kind"]] = value
        if event.get("root"):
            point["root"] = event["root"]
    for package in packages:
        match = re.search(r"/history-([0-9]+)/", "/" + package["path"])
        step = int(match[1]) if match else 1
        steps.setdefault(step, {"step": step})["canonical-package"] = package
    for point in steps.values():
        point["sharing"] = {}
        for key, value in list(point.items()):
            if isinstance(value, dict) and key != "canonical-package":
                point["sharing"].update(sharing_values(value))
    final = steps[max(steps)] if steps else {}
    final_metrics = {}
    for kind in ("canonical-verification", "history-canonical", "history-accounting", "history-transcript", "dedup-verification", "capped-verification"):
        for key, value in numeric_values(final.get(kind, {})).items():
            if not key.startswith("variant_"):
                final_metrics["verified." + kind + "." + key] = value
    return {"steps": [steps[key] for key in sorted(steps)], "final_metrics": final_metrics,
            "final_sharing": final.get("sharing", {}),
            "scope": "Independent matching verification execution; per-step state and retained-union gauges are endpoints, never summed across snapshots. Step-new canonical counts mean first reachable in the union, not emitted CAS insertions."}


def distribution_rows(values, sources):
    rows = []
    for (source_id, case), metrics_for_case in sorted(values.items()):
        for metric, samples in sorted(metrics_for_case.items()):
            rows.append({"source_group": source_id, "source_identity": sources[source_id], "case": case, "metric": metric,
                         "n": len(samples), "median": statistics.median(samples), "min": min(samples), "max": max(samples)})
    return rows


def read_runtime_suppressions(campaign, registry):
    path = campaign / "phase1-runtime-suppressions.json"
    if not path.is_file():
        raise ValueError("persistent Phase1 runtime suppression ledger missing")
    ledger = read(path)
    if ledger.get("schema") != "phase1-runtime-suppressions-v1" or ledger.get("limit_ns") != 15_000_000_000 or not isinstance(ledger.get("cases"), dict):
        raise ValueError("invalid runtime suppression ledger schema/limit/cases")
    initial = runner.INITIAL_SUPPRESSED_CASES
    if not initial.issubset(ledger["cases"]):
        raise ValueError("persistent ledger omitted initial user suppression")
    policy_path = "docs/roadmap/0.1/0.1.3/phase-1-runtime-suppressions.md"
    policy_hash = hashlib.sha256(subprocess.check_output(["git", "show", f"{RUNTIME_SCOPE_POLICY_REVISION}:{policy_path}"], cwd=HERE.parents[1])).hexdigest()
    for case_id, entry in ledger["cases"].items():
        if not isinstance(entry, dict) or entry.get("scenario_id") != case_id or entry.get("status") != "suppressed_phase1_time_budget" or entry.get("origin") not in {"user-initial", "measured-product-budget"} or not isinstance(entry.get("reason"), str) or not entry["reason"].strip() or not digest(entry.get("policy_sha256")):
            raise ValueError("malformed persistent runtime suppression record")
        number(entry.get("at_unix_ns"), "suppression timestamp")
        if entry["policy_sha256"] != policy_hash:
            raise ValueError("suppression record differs from exact user scope amendment")
        if entry["origin"] == "user-initial" and case_id not in initial:
            raise ValueError("unrequested initial runtime suppression")
        if entry["origin"] == "measured-product-budget":
            if entry.get("mode") != "performance" or entry.get("limit_ns") != ledger["limit_ns"] or number(entry.get("observed_product_ns"), "suppression cumulative product time") <= ledger["limit_ns"]:
                raise ValueError("runtime suppression lacks reached cumulative performance limit")
            directory = Path(entry["evidence_path"])
            original = read(directory / "outcome.json")
            if original.get("scenario_id") != case_id or original.get("seed") != entry.get("seed") or original.get("source_revision") != entry.get("source_revision") or original.get("mode") != "performance":
                raise ValueError("runtime suppression trigger source/slot differs from original outcome")
            if not runner.budget_suppression_can_continue(original):
                raise ValueError("suppressed trigger has unresolved cleanup/resource/observer failure")
    phase1_scope(registry, ledger["cases"])
    return ledger


def phase1_scope(registry, suppressions):
    """Apply durable exact-ID scope decisions without rewriting raw outcomes."""
    ids = {case["scenario_id"]: case for case in registry}
    if set(suppressions) - set(ids):
        raise ValueError("runtime suppression names an unknown frozen scenario")
    if any(ids[key].get("proof_only") for key in suppressions):
        raise ValueError("standalone proof cannot be runtime suppressed")
    new, proofs, inherited = registry_cases(registry)
    all_required = [(case, seed, mode) for case in new for mode in ("performance", "verify") for seed in (1, 2, 3)]
    all_required += [(case, 1, "verify") for case in proofs]
    all_required += [(case, rep, "performance") for case in inherited for rep in range(1, 6)] + [(case, 1, "verify") for case in inherited]
    active = [slot for slot in all_required if slot[0]["scenario_id"] not in suppressions]
    suppressed = [{"case": case["scenario_id"], "family_id": case["family_id"], "seed": seed, "mode": mode,
                   "inherited": case.get("inherited", False), "coverage_status": "suppressed_phase1_time_budget",
                   "product_status": "not-claimed", "suppression": suppressions[case["scenario_id"]]}
                  for case, seed, mode in all_required if case["scenario_id"] in suppressions]
    return active, suppressed


def terminal_status(missing, invalid, issues, failures):
    return "NO_GO" if missing or invalid or issues or failures else "PHASE1_TERMINAL_PASS"


def generate(campaign, assets):
    build, registry = qualified_build(assets)
    new, proofs, inherited = registry_cases(registry)
    selected_builds, build_provenance, compatibility = family_builds(campaign, assets, build, registry)
    product_compatibility = {item["old_revision"]: item for selection in build_provenance.values() for item in selection.get("product_compatibility", [])}
    ledger = read(campaign / "slots.json") if (campaign / "slots.json").exists() else {}
    classifications = read(campaign / "classifications.json") if (campaign / "classifications.json").exists() else {}
    invalidations = [decode(line) for line in (campaign / "invalidations.jsonl").read_text().splitlines() if line] if (campaign / "invalidations.jsonl").exists() else []
    invalidated = defaultdict(list)
    for entry in invalidations:
        if not isinstance(entry.get("previous_evidence"), str) or not isinstance(entry.get("reason"), str) or not entry["reason"].strip():
            raise ValueError("invalidation lacks retained evidence/reason")
        invalidated[str(Path(entry["previous_evidence"]).resolve())].append(entry)
    suppression_ledger = read_runtime_suppressions(campaign, registry)
    suppressions = suppression_ledger["cases"]
    required, suppressed_slots = phase1_scope(registry, suppressions)
    required_keys = {(case["scenario_id"], seed, mode) for case, seed, mode in required}
    current = [row for row in ledger.values() if row.get("mode") != "fast-verify" and row.get("scenario_id") not in suppressions and all(row.get(key) == selected_build(selected_builds, row, row.get("mode"))[value] for key, value in IDENTITY_FIELDS.items())]
    by_slot, global_issues, evidence_paths = {}, [], set()
    for family in {case["family_id"] for case in registry}:
        environments = {row.get("environment_identity") for row in current if row.get("family_id") == family and row.get("mode") == "performance"}
        if len(environments) > 1:
            global_issues.append(f"multiple runtime environment identities cannot be pooled within {family}")
    invocations = []
    selected_invocation_slots = {(row.get("source_revision"), row.get("image_id"), row.get("scenario_id"), row.get("seed"), row.get("mode")) for row in current}
    covered_invocation_slots = set()
    for path in sorted((campaign / "invocations").glob("*.json")):
        value = read(path)
        if not any(value.get("source_revision") == chosen["revision"] and value.get("image_id") == chosen["image_id"] for chosen in selected_builds.values()):
            continue
        planned_rows = value.get("planned_slots")
        planned = {(value.get("source_revision"), value.get("image_id"), *slot) for slot in planned_rows if isinstance(slot, list) and len(slot) == 3 and isinstance(slot[0], str) and type(slot[1]) is int and isinstance(slot[2], str) and slot[2] in {"performance", "verify"}} if isinstance(planned_rows, list) else set()
        if not planned & selected_invocation_slots:
            continue
        try:
            for key in ("source_validation_ns", "registry_query_ns"):
                number(value.get(key), key)
            if value.get("status") == "interrupted-unmeasured-wall":
                require(value.get("invocation_wall_ns") is None and value.get("recovery_reason") == "exclusive campaign lock acquired after prior coordinator ended", "interrupted CLI lacks explicit lock/recovery custody", global_issues)
            else:
                number(value.get("invocation_wall_ns"), "invocation_wall_ns")
                require(value["invocation_wall_ns"] >= value["source_validation_ns"] + value["registry_query_ns"], "CLI invocation hides validation/query work", global_issues)
                require(value.get("status") in {"pass", "failed-outcomes", "completed_with_suppressions", "suppressed_phase1_time_budget", "interrupted"}, "CLI invocation has not completed", global_issues)
                if value.get("status") == "interrupted":
                    require(str(value.get("error", "")).startswith("KeyboardInterrupt:"), "interrupted invocation lacks explicit user-stop reason", global_issues)
            require(isinstance(value.get("planned_slots"), list), "CLI invocation lacks selected slot inventory", global_issues)
        except (ValueError, TypeError) as error:
            global_issues.append(f"invalid invocation receipt {path.name}: {error}")
        invocations.append({**value, "path": str(path), "sha256": custody.sha(path)})
        covered_invocation_slots.update(planned)
    if current and not invocations:
        global_issues.append("no retained CLI invocation wall receipts")
    if selected_invocation_slots - covered_invocation_slots:
        global_issues.append("selected outcomes lack matching CLI invocation slot receipts")
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
        selected_source = selected_build(selected_builds, case, mode, seed)
        product_bridge = matching_product_bridge(list(product_compatibility.values()), selected_source, build, case["scenario_id"])
        value = validate_attempt(outcome, classification, case, selected_source)
        predicate_scope = None
        if product_bridge:
            try:
                retained_records = raw(Path(outcome["evidence_path"]) / "raw.jsonl")
                if product_bridge["kind"] == RETAINED_PROOF_KIND:
                    require(outcome["mode"] == "verify" and seed == 1 and Path(outcome["evidence_path"]).name == product_bridge["source_proof"]["retained_evidence_basename"], "only the exact completed proof is reusable", value["issues"])
                    predicate_scope = {"correctness": "retained independent proof with explicit state identity", "timing_claim": "none; original proof resources only"}
                elif product_bridge["kind"] == CONTENT_FRONTIER_BRIDGE_KIND:
                    predicate_scope = validate_content_frontier_records(retained_records, case, product_bridge, value["issues"])
                elif product_bridge["kind"] == SPILL_INDEX_BRIDGE_KIND:
                    predicate_scope = validate_spill_index_records(retained_records, case, product_bridge, value["issues"])
                elif product_bridge["kind"] == EMPTY_GENERATION_BRIDGE_KIND:
                    predicate_scope = validate_empty_generation_records(retained_records, case, value["issues"])
                else:
                    validate_no_unlink_records(retained_records, value["issues"])
            except (OSError, ValueError, TypeError, KeyError) as error:
                value["issues"].append(f"product bridge observation invalid: {error}")
            if value["issues"]:
                value["verification_pass"] = False
        invalidation = invalidated.get(str(Path(outcome["evidence_path"]).resolve()), [])
        if invalidation:
            value["issues"].append("selected observation was explicitly invalidated")
            value["verification_pass"] = False
        if mode == "performance" and value["product_status"] == "pass":
            require(type(value["metrics"].get("pure_call_sum_ns")) is int and value["metrics"]["pure_call_sum_ns"] <= 15_000_000_000,
                    "active performance exceeds15s or lacks the exact cumulative product-time receipt; durable suppression required", value["issues"])
        checked[key] = value
        row = {"case": key[0], "family_id": case["family_id"], "seed": seed, "mode": mode, "assurance_status": "fully_verified" if value["verification_pass"] else "not_verified", "inherited": case.get("inherited", False),
               "source_identity": {key: outcome.get(key) for key in IDENTITY_FIELDS}, "source_arm": outcome.get("source_arm"), "raw_product_status": outcome.get("product_status"), "coverage_status": outcome.get("coverage_status"), "product_status": value["product_status"], "evidence_status": "REVISE" if value["issues"] else "PASS",
               "issues": value["issues"], "violations": value["violations"], "evidence": outcome["evidence_path"], "metrics": value["metrics"], "resource_observations": value["resource_observations"], "observations": value["observations"], "canonical_packages": value["canonical_packages"],
               "verification_summary": verification_summary(value["observations"], value["canonical_packages"]), "environment_identity": outcome.get("environment_identity"), "input_identity": outcome.get("input_identity"), "invalidation_context": invalidation, "product_source_compatibility": product_bridge, "product_predicate_scope": predicate_scope,
               "measured_current_product_binary": outcome.get("product_identity") == build["product_seal"] and outcome.get("source_revision") == build["revision"]}
        rows.append(row)
        if value["issues"]:
            invalid.append(row)
        if value["product_status"] == "fail":
            failures.append({**row, "classification": classification})
    distributions = defaultdict(lambda: defaultdict(list))
    distribution_sources, step_evidence = {}, []
    for row in rows:
        case = next(case for case in registry if case["scenario_id"] == row["case"])
        proof_key = (row["case"], 1 if case.get("inherited") else row["seed"], "verify")
        proof = checked.get(proof_key, {})
        outcome = by_slot[(row["case"], row["seed"], row["mode"])]
        proof_outcome = by_slot.get(proof_key, {})
        same_source = all(proof_outcome.get(key) == outcome.get(key) for key in IDENTITY_FIELDS)
        bridge = next((item for item in compatibility if item["family_id"] == case["family_id"] and item["performance_revision"] == outcome.get("source_revision") and item["verification_revision"] == proof_outcome.get("source_revision")), None)
        row["verification_source_compatibility"] = "identical sealed source" if same_source else bridge
        eligible = (same_source or bridge is not None) and not global_issues and row["mode"] == "performance" and row["evidence_status"] == "PASS" and row["product_status"] == "pass" and proof.get("verification_pass") and proof_outcome.get("input_identity") == outcome.get("input_identity") and proof_outcome.get("environment_identity") == outcome.get("environment_identity")
        row["performance_claim_eligible"] = bool(eligible)
        if eligible:
            source_id, identity = source_group(outcome)
            distribution_sources[source_id] = identity
            proof_details = verification_summary(proof["observations"], proof["canonical_packages"])
            row["matched_verification"] = {"evidence": proof_outcome["evidence_path"], "source_identity": source_identity(proof_outcome), **proof_details}
            for metric, value in {**row["metrics"], **proof_details["final_metrics"]}.items():
                distributions[(source_id, row["case"])][metric].append(value)
            verified_steps = {point["step"]: point for point in proof_details["steps"]}
            measured_steps = {point["step"]: point for point in row["observations"].get("steps", [])}
            for step in sorted(set(verified_steps) | set(measured_steps)):
                step_evidence.append({"case": row["case"], "family_id": row["family_id"], "seed": row["seed"], "source_group": source_id,
                    "source_identity": identity, "performance_evidence": row["evidence"], "verification_evidence": proof_outcome["evidence_path"],
                    "verification_source_identity": source_identity(proof_outcome), "step": step,
                    "measured": measured_steps.get(step), "verified": verified_steps.get(step)})
    eligible_distributions = distribution_rows(distributions, distribution_sources)
    counts = {"planned_new_cases": 130, "planned_initial_sample_slots": 390, "executed_initial_sample_slots": sum(row["coverage_status"] == "executed" and not row["inherited"] and row["mode"] == "performance" for row in rows),
              "planned_new_verification_slots": 390, "executed_new_verification_slots": sum(row["coverage_status"] == "executed" and row["family_id"] in FAMILY_COUNTS and row["mode"] == "verify" and row["case"] != "dedup-cdc-boundaries-proof" for row in rows),
              "planned_reliability_subcases": 28, "executed_reliability_subcases": sum(row["coverage_status"] == "executed" and row["family_id"] == "workspace_reliability" for row in rows),
              "planned_capped_performance_slots": 25, "executed_capped_performance_slots": sum(row["coverage_status"] == "executed" and row["inherited"] and row["mode"] == "performance" for row in rows),
              "planned_capped_verifiers": 5, "executed_capped_verifiers": sum(row["coverage_status"] == "executed" and row["inherited"] and row["mode"] == "verify" for row in rows),
              "missing_slots": len(missing), "invalid_slots": len(invalid), "unexecuted_slots": sum(row["coverage_status"] != "executed" for row in rows),
              "unknown_product_outcomes": sum(row["product_status"] == "not-established" for row in rows), "product_failed_outcomes": len(failures)}
    counts.update(original_new_cases=len(new), original_new_performance_slots=3 * len(new),
                  suppressed_new_cases=sum(case["scenario_id"] in suppressions for case in new),
                  suppressed_new_performance_slots=3 * sum(case["scenario_id"] in suppressions for case in new),
                  active_new_cases=sum(case["scenario_id"] not in suppressions for case in new),
                  active_new_performance_slots=3 * sum(case["scenario_id"] not in suppressions for case in new),
                  active_new_verification_slots=3 * sum(case["scenario_id"] not in suppressions for case in new),
                  original_capped_cases=len(inherited), original_capped_performance_slots=5 * len(inherited),
                  suppressed_capped_cases=sum(case["scenario_id"] in suppressions for case in inherited),
                  suppressed_capped_performance_slots=5 * sum(case["scenario_id"] in suppressions for case in inherited),
                  active_capped_performance_slots=5 * sum(case["scenario_id"] not in suppressions for case in inherited),
                  active_capped_verification_slots=sum(case["scenario_id"] not in suppressions for case in inherited),
                  suppressed_associated_verification_slots=sum(row["mode"] == "verify" for row in suppressed_slots),
                  active_required_slots=len(required), suppressed_prescribed_slots=len(suppressed_slots))
    family_scope = {family: {"original_cases": len([case for case in registry if case["family_id"] == family]),
                           "suppressed_cases": [case["scenario_id"] for case in registry if case["family_id"] == family and case["scenario_id"] in suppressions],
                           "active_cases": [case["scenario_id"] for case in registry if case["family_id"] == family and case["scenario_id"] not in suppressions]}
                    for family in sorted({case["family_id"] for case in registry})}
    for value in family_scope.values():
        value["execution_scope"] = "wired; all performance and associated verification suppressed_phase1_time_budget" if not value["active_cases"] else "active coverage required; suppressed subsets are not passing coverage"
    retained, retained_outcomes = [], []
    for path in sorted((campaign / "attempts").glob("*/outcome.json")):
        value = read(path)
        value["invalidation_context"] = invalidated.get(str(path.parent.resolve()), [])
        value["sql_history_scope"] = sql_history_status(value["source_revision"]) if value.get("mode") == "performance" else "separate verification; actual original gates retained"
        value["phase1_scope_status"] = "suppressed_phase1_time_budget" if value.get("scenario_id") in suppressions else "active-or-historical"
        if path.parent.name.endswith("fa8300eb5d36"):
            # Preserve the sealed pre-fix coverage field; independently expose the reached work.
            started = any(record.get("kind") in {"sample-start", "proof-start"} for record in raw(path.parent / "raw.jsonl"))
            value["derived_execution_status"] = "executed-partial" if started else "not-established"
            value["derived_phase1_disposition"] = "user-policy-stop" if started and value.get("scenario_id") in suppressions and value.get("supervisor_cleanup_status") == "pass" else "requires-investigation"
        retained_outcomes.append(value)
        if value.get("product_status") != "pass" or value.get("harness_status") == "fail" or value["invalidation_context"] or "diagnostic-only" in value["sql_history_scope"]:
            retained.append({key: value.get(key) for key in ("scenario_id", "seed", "mode", "source_revision", "source_arm", "product_status", "harness_status", "error", "evidence_path", "invalidation_context", "sql_history_scope", "phase1_scope_status", "derived_execution_status", "derived_phase1_disposition")})
    arms = {}
    for value in retained_outcomes:
        key, identity = source_group(value)
        arm = arms.setdefault(key, {"source_group": key, **identity, "raw_performance_outcomes": 0, "raw_pass": 0, "raw_fail": 0, "invalidated_observations": 0, "sql_history_scope": sql_history_status(value["source_revision"]), "evidence": []})
        if value.get("mode") == "performance" and value.get("coverage_status") == "executed":
            arm["raw_performance_outcomes"] += 1
            arm["invalidated_observations"] += bool(value.get("invalidation_context"))
            arm["raw_pass" if value.get("product_status") == "pass" else "raw_fail"] += 1
            arm["evidence"].append(value["evidence_path"])
    fast_results = []
    for outcome in ledger.values():
        if outcome.get("mode") != "fast-verify":continue
        case = next((item for item in registry if item["scenario_id"] == outcome.get("scenario_id")), None)
        selected = selected_build(selected_builds, outcome, "fast-verify") if case else None
        if selected is None or any(outcome.get(key) != selected[value] for key, value in IDENTITY_FIELDS.items()):continue
        value = validate_fast_attempt(outcome, classifications.get(Path(outcome["evidence_path"]).name, {}), case, selected)
        fast_results.append({"case": outcome["scenario_id"], "seed": outcome["seed"], "mode": "fast-verify", "evidence": outcome["evidence_path"],
            "source_identity": source_identity(outcome), "environment_identity": outcome.get("environment_identity"),
            "assurance_status": "fast_iteration_verified" if value["fast_iteration_pass"] else "not_verified", "counts_toward_full_phase1_gate": False,
            "certificate_identity": outcome.get("verification_certificate_identity"), **value})
    summary = {"schema": "fs-bench-pro-phase1-review-v2", "source": build, "scoped_builds": build_provenance, "verification_compatibility": compatibility, "product_compatibility": list(product_compatibility.values()), "runtime_budget_compatibility": next((item["runtime_budget_compatibility"] for item in build_provenance.values() if item.get("runtime_budget_compatibility")), None), "report_generator_sha256": custody.sha(Path(__file__)), "runtime_report_generator_sha256": build["report_generator_sha256"],
               "fast_iteration_results": fast_results, "fast_profile_scope": "Separate development assurance; no exhaustive Phase1 coverage or performance claim. Full verify never falls back to fast.",
               "full_verifier_compatibility": next((item["full_verifier_compatibility"] for item in build_provenance.values() if item.get("full_verifier_compatibility")), None),
               "runtime_suppressions": suppression_ledger, "suppressed_slots": suppressed_slots, "family_scope": family_scope,
               "suppressed_original_outcomes": [value for value in retained_outcomes if value["phase1_scope_status"] == "suppressed_phase1_time_budget"],
               "retained_source_arms": list(arms.values()), "retained_invalidations": invalidations, "eligible_distributions": eligible_distributions, "step_evidence_path": "step-evidence.json", "counts": counts, "phase1_evidence_status": "PASS" if not missing and not invalid and not global_issues else "REVISE", "product_status": "FAIL" if failures else "NOT_ESTABLISHED" if missing or invalid or global_issues else "PASS",
               "phase1_terminal_status": terminal_status(missing, invalid, global_issues, failures), "completion_policy": "phase-1-runtime-suppressions-2026-09-04; all remaining active failure-repair gates unchanged", "scope_amendment_revision": RUNTIME_SCOPE_POLICY_REVISION, "global_issues": global_issues, "missing": missing, "invalid": invalid, "product_findings": failures, "retained_failure_history": retained, "invocations": invocations, "rows": rows}
    results = campaign / "results"
    results.mkdir(exist_ok=True)
    custody.write_json(results / "review.json", summary)
    custody.write_json(results / "step-evidence.json", {"schema": "fs-bench-pro-step-evidence-v1", "rows": step_evidence,
        "scope": "Each row joins a measured operation ordinal+1 with an independently verified snapshot ordinal. Raw roots stay in their own executions; no cross-execution root equality is assumed. Store deltas and retained-union gauges are not summed."})
    inputs = {"build_manifest_sha256": custody.sha(assets / "evidence/evidence.sha256"), "ledger_sha256": custody.sha(campaign / "slots.json") if (campaign / "slots.json").exists() else None,
              "family_build_mapping_sha256": custody.sha(campaign / "evidence-builds.json") if (campaign / "evidence-builds.json").exists() else None,
              "runtime_suppressions_sha256": custody.sha(campaign / "phase1-runtime-suppressions.json") if (campaign / "phase1-runtime-suppressions.json").exists() else None,
              "invalidations_sha256": custody.sha(campaign / "invalidations.jsonl") if (campaign / "invalidations.jsonl").exists() else None,
              "classifications_sha256": custody.sha(campaign / "classifications.json") if (campaign / "classifications.json").exists() else None,
              "generator_sha256": summary["report_generator_sha256"], "policy_helper_sha256": custody.sha(HERE / "workspace-runner.py"), "custody_helper_sha256": custody.sha(HERE / "sdk-edit-custody.py"), "attempt_manifests": {path: custody.sha(Path(path) / "evidence.sha256") if (Path(path) / "evidence.sha256").is_file() else None for path in sorted(evidence_paths)}}
    custody.write_json(results / "report-inputs.json", inputs)
    lines = ["# LayerFS v0.1.3 Phase 1 initial baseline", "", f"Evidence: **{summary['phase1_evidence_status']}**. Product: **{summary['product_status']}**. Phase 1 terminal gate: **{summary['phase1_terminal_status']}**.", "", f"Sealed source: `{build['revision']}`. Report generator: `{summary['report_generator_sha256']}`.", "", "| Coverage | Count |", "| --- | ---: |"]
    lines += [f"| {key} | {value} |" for key, value in counts.items()]
    lines += ["", "## Fast iteration profile", "", "Fast results remain separate from fully verified evidence and never fill required Phase1 slots.", ""]
    lines += [f"- `{item['case']}` seed {item['seed']}: {item['assurance_status']}; full gate contribution: none; evidence `{item['evidence']}`." for item in fast_results]
    lines += ["", "## Phase1 runtime scope suppressions", "", "The original inventory remains visible. Suppressed exact IDs and their associated verification are outside active Phase1 coverage; suppression is neither PASS nor FAIL nor unimplemented work. Their raw historical outcomes are preserved. All active correctness/resource/cleanup gates still apply. Git remains wired with all four execution subsets suppressed.", "", "| Case | Phase1 scope | Reason |", "| --- | --- | --- |"]
    lines += [f"| `{case}` | suppressed_phase1_time_budget | {decision.get('reason', decision.get('trigger', 'persistent15s product-time scope decision'))} |" for case, decision in sorted(suppressions.items())]
    lines += ["", "## Retained original and corrected source arms", "", "Raw outcomes keep their actual producing identities and pass/fail statuses. Every unrequested SQL-history performance recording is diagnostic-only: source labelling does not repair its contaminated timers or memory observations.", "", "| Arm | Source / identity group | Image | Raw performance outcomes | Raw pass | Raw fail | Invalidated observations | SQL history scope |", "| --- | --- | --- | ---: | ---: | ---: | ---: | --- |"]
    lines += [f"| {arm['source_arm']} | `{arm['source_revision']}` / `{arm['source_group'][:16]}` | `{arm['image_id']}` | {arm['raw_performance_outcomes']} | {arm['raw_pass']} | {arm['raw_fail']} | {arm['invalidated_observations']} | {arm['sql_history_scope']} |" for arm in arms.values()]
    lines += ["", "## Eligible source-bound distributions", "", "Only complete, authentic, source/input/environment-matched independently verified samples are eligible. Every source group is separate. Old unrequested SQL-history timings cannot enter these distributions. The two exact already-completed independent proofs retain their original source identity. Separately, successful clean6c samples within15seconds may retain their original timing/resource claims through the explicit product-identical, budget-only harness source bridge; this does not admit SQL-contaminated timings or claim new-observer cost equivalence. CPU/I/O use observed boundary differences; transaction and memory/spool high-water values take maxima; Store growth uses signed endpoint differences. Verification-derived sharing/storage values are labelled verified and retain their independent producing evidence.", "", "| Arm / source group | Case | Metric | n | Median | Min | Max |", "| --- | --- | --- | ---: | ---: | ---: | ---: |"]
    for item in eligible_distributions:
        identity = item["source_identity"]
        lines.append(f"| {identity['source_arm']} / `{item['source_group'][:16]}` | {item['case']} | {item['metric']} | {item['n']} | {item['median']} | {item['min']} | {item['max']} |")
    lines += ["", "## Per-step curves and sharing denominators", "", "[step-evidence.json](step-evidence.json) retains every eligible sample's per-step public timings, published root, Commit/FUSE/candidate observations, Store endpoints/deltas, matching canonical role census, per-variant CDC evidence and retained-history union accounting. Genesis is step0; measured operation ordinal0 joins verified snapshot1. Current-state and retained-union gauges stay distinct. Regular payload sharing excludes metadata, canonical wrappers and Store slack; addition-only and retained-history denominators are explicit.", "", "| Case | Seed | Arm / source group | Step | Commit ns | Store growth this step | New payload bytes | Retained payload bytes | Retained logical bytes | Retained canonical bytes |", "| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for item in step_evidence:
        measured, verified = item.get("measured") or {}, item.get("verified") or {}
        account = verified.get("history-accounting", {})
        if item["family_id"] == "dedup_branch_history":
            lines.append(f"| {item['case']} | {item['seed']} | {item['source_identity']['source_arm']} / `{item['source_group'][:16]}` | {item['step']} | {measured.get('timings', {}).get('commit_ns', '—')} | {measured.get('store_growth_this_step', {}).get('file_bytes', '—')} | {account.get('step_new_payload_bytes', '—')} | {account.get('distinct_retained_payload_bytes', '—')} | {account.get('retained_logical_snapshot_bytes', '—')} | {account.get('retained_canonical_bytes', '—')} |")
    lines += ["", "## CLI invocation wall", "", "A family invocation can cover many samples. Its full CLI wall is not copied into each sample or added to sample wall. Interrupted invocation wall remains unknown.", "", "| Source | Arm | Selected slots | Full CLI ns | Source validation ns | Registry ns |", "| --- | --- | ---: | ---: | ---: | ---: |"]
    for item in invocations:
        lines.append(f"| `{item.get('source_revision')}` | {item.get('source_arm', 'not recorded')} | {len(item.get('planned_slots', []))} | {item.get('invocation_wall_ns')} | {item.get('source_validation_ns')} | {item.get('registry_query_ns')} |")
    lines += ["", "## Failures and remaining evidence work", ""]
    lines += [f"- **Retained invalidated observation**: `{entry['previous_evidence']}` — {entry['reason']}. Its original product status is unchanged; it cannot support performance claims." for entry in invalidations]
    lines += [f"- `{row['case']}` repetition/seed {row['seed']} {row['mode']}: **FAIL**. {row['classification'].get('finding', 'Requires classification')}. Evidence: `{row['evidence']}`." for row in failures]
    lines += [f"- `{row['case']}` repetition/seed {row['seed']} {row['mode']}: **REVISE** — {'; '.join(row['issues'])}." for row in invalid]
    lines += [f"- **REVISE** — {issue}." for issue in global_issues]
    if missing:
        lines.append(f"- {len(missing)} required slots remain missing; review.json contains exact IDs.")
    lines += ["", "## Scope", "", "This is initial benchmark evidence, not release admission. Still-active product failures block Phase1 completion and require repair. Runtime-suppressed coverage is explicitly outside this amended Phase1 scope and is never labelled passing. Historical raw passes and failures remain unchanged in retained_failure_history with diagnostic/invalidation labels; corrected clean-capture outcomes keep separate source identities. Report regeneration does not rerun product work. No cold-cache, optimization or crash/power-loss guarantee is claimed. Issue #21 remains open.", ""]
    (results / "initial-results.md").write_text("\n".join(lines))
    custody.seal(results)
    return summary


def fast_profile_self_check():
    runner.fast_profile_self_check()
    issues = []
    validate_fast_receipts(Path("unused"), {}, [{"kind": "sample-complete", "status": "pass"}], True, issues)
    assert "fast profile must not emit exhaustive/full completion" in issues
    assert "fast profile lacks unique canonical/native/completion receipts" in issues
    print("fast_profile_no_placeholder_or_full_gate_credit_self_check=pass")


def canonical_receipt_self_check():
    with tempfile.TemporaryDirectory(prefix="phase1-canonical-receipt-") as temporary:
        directory = Path(temporary)
        folder = directory / "verification/canonical-verification"
        folder.mkdir(parents=True)
        value = {"verification_status": "pass", "canonical_role_status": "pass", "canonical_root": "0" * 64,
                 "oracle_identity": "1" * 64, "verified_regular_paths": "0", "verified_paths": "1"}
        (folder / "canonical-receipt.txt").write_text("".join(f"{key}={item}\n" for key, item in value.items()))
        (folder / "payload-extents.tsv").write_text("path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length\n")
        (folder / "file-roots.tsv").write_text("path\tcontent_root\n")
        (folder / "independent-manifest.tsv").write_text("workspace-independent-manifest-v1\n.\tdirectory\t0\t755\t0\t0\t-\n")
        events = [{"kind": "canonical-verification", "receipt": json.dumps(value)}]
        issues = []
        packages = validate_canonical_artifacts(directory, issues, {"family_id": "payload_create_read"}, events, True)
        assert not issues, issues
        assert packages[0]["receipt"]["canonical_root"] == "0" * 64
        events[0]["receipt"] = json.dumps({**value, "canonical_root": "2" * 64})
        issues = []
        validate_canonical_artifacts(directory, issues, {"family_id": "payload_create_read"}, events, True)
        assert "canonical package differs from emitted authenticated receipt" in issues
    print("canonical_package_string_identity_self_check=pass")


def aggregation_self_check():
    """Tiny synthetic receipt/ledger models; no registry, Store, Docker or hashing campaign files."""
    def host(phase, cpu, rss):
        return {"kind": "host-resources", "phase": phase, "user_cpu_ns": cpu, "system_cpu_ns": cpu,
                "disk_read_bytes": cpu, "disk_write_bytes": cpu, "swaps": 0,
                "resident_bytes": rss, "peak_resident_bytes": rss, "physical_footprint_bytes": rss}
    def store(step, length, phase):
        return {"kind": "store-observation", "phase": phase, "step": step,
                **{key: length for key in STORE_GAUGES}}
    records = [host("before", 10, 20), store(0, 100, "before"),
               {"kind": "phase", "phase": "commit", "step": 0, "elapsed_ns": 5},
               {"kind": "operation", "details": "CandidateReceipt { candidate_objects: 7, max_transaction_objects: 4, max_transaction_bytes: 20 }"},
               store(1, 120, "after-commit"),
               {"kind": "phase", "phase": "commit", "step": 1, "elapsed_ns": 9},
               {"kind": "operation", "details": "CandidateReceipt { candidate_objects: 9, max_transaction_objects: 2, max_transaction_bytes: 30 }"},
               store(2, 116, "after-commit"), host("after-product", 17, 25), host("final", 100, 30)]
    observed = observation_data(records, {}, {"run_acquisition_reused": True, "run_acquisition_ns": 2, "cache_acquisition_ns": 100, "cache_build_ns": 80, "cache_validation_ns": 10}, {"clone_wall_ns": 3})
    values = observed["metrics"]
    assert values["commit_ns"] == 14 and values["candidate.candidate_objects"] == 16
    assert values["candidate.max_transaction_objects"] == 4 and values["candidate.max_transaction_bytes"] == 30
    assert values["host.user_cpu_ns.delta"] == 7 and values["host.peak_resident_bytes.max"] == 30
    assert values["store.file_bytes.delta"] == 16 and observed["steps"][-1]["store_growth_this_step"]["file_bytes"] == -4
    assert values["cache_acquisition_ns"] == 2 and values["cache_build_ns"] == values["cache_validation_ns"] == 0
    assert source_group({"source_revision": "same", "image_id": "first"})[0] != source_group({"source_revision": "same", "image_id": "second"})[0]
    assert "benchmark/fs-bench-pro/dedup_workloads.rs" in bridge_dependency_paths("dedup_branch_history")
    with tempfile.TemporaryDirectory(prefix="phase1-report-aggregation-") as temporary:
        directory = Path(temporary)
        folder = directory / "verification/history-0/canonical-verification"
        folder.mkdir(parents=True)
        ledger = folder / "history-canonical-union.tsv.gz"
        root, object_id = "a" * 64, "b" * 64
        events = []
        for step in range(3):
            account = {"canonical_root": root, "canonical_union_status": "pass",
                "retained_canonical_objects": "2", "retained_canonical_bytes": "30",
                "retained_regular_payload_canonical_objects": str(int(step > 0)), "retained_regular_payload_canonical_bytes": str(10 if step else 0),
                "retained_non_payload_canonical_objects": str(1 + int(step == 0)), "retained_non_payload_canonical_bytes": str(20 if step else 30),
                "retained_metadata_value_canonical_objects": "1", "retained_metadata_value_canonical_bytes": "10",
                "retained_canonical_Chunk_objects": "1", "retained_canonical_Chunk_bytes": "10",
                "retained_canonical_Namespace_objects": "1", "retained_canonical_Namespace_bytes": "20",
                "step_new_canonical_objects": str(2 if step == 0 else 0), "step_new_canonical_bytes": str(30 if step == 0 else 0),
                "retained_logical_snapshot_bytes": str((step + 1) * 100), "distinct_retained_payload_bytes": "10"}
            events.extend([{"kind": "history-canonical", "step": step, "root": root, "receipt": {}},
                           {"kind": "history-accounting", "step": step, "receipt": account}])
        header = "step\troot\tobject_id\trole\tcanonical_bytes\tregular_file\tmetadata_value\n"
        rows = [f"0\t{root}\t{root}\tNamespace\t20\t0\t0\n", f"0\t{root}\t{object_id}\tChunk\t10\t0\t1\n", f"1\t{root}\t{object_id}\tChunk\t10\t1\t1\n"]
        def write(rows):
            with gzip.open(ledger, "wt") as stream:
                stream.write(header + "".join(rows))
        write(rows)
        issues = []
        validate_canonical_union(directory, events, {"tier": 2}, issues)
        assert not issues, issues
        # A skipped ledger step keeps its union endpoints and adds zero objects.
        summary = verification_summary({"verification": events}, [])
        assert summary["final_metrics"]["verified.history-accounting.retained_logical_snapshot_bytes"] == 300
        assert summary["final_sharing"]["retained_history_payload_sharing"]["logical_bytes"] == 300
        for malformed in (rows + [rows[-1]], [*rows[:2], rows[2].replace("\t10\t", "\t11\t")], [*rows[:2], rows[2].replace("Chunk", "FileNode")]):
            write(malformed)
            try:
                validate_canonical_union(directory, events, {"tier": 2}, [])
                raise AssertionError("malformed canonical union accepted")
            except ValueError:
                pass
        # An ordinary successful proof requires its actual package, not merely a status event.
        case = {"family_id": "payload_create_read", "scenario_id": "synthetic", "tier": 1}
        issues = []
        validate_canonical_artifacts(directory / "absent", issues, case, [{"kind": "canonical-verification", "receipt": {}}], True)
        assert "missing/extra canonical snapshot packages" in issues
    print("report_aggregation_and_history_union_self_check=pass")


def failure_self_check():
    case = {"scenario_id": "payload-create-1m", "family_id": "payload_create_read", "operation": "payload-create", "tier": 1, "input_mode": "store"}
    builds = {"default": "new", "family:payload_create_read:performance": "old", "case:payload-create-1m:performance": "override"}
    assert selected_build(builds, case, "performance") == "override"
    assert selected_build(builds, case, "verify") == "new"
    del builds["case:payload-create-1m:performance"]
    assert selected_build(builds, case, "performance") == "old"
    issues = []
    validate_performance(case, {"external_process_wall_ns": 1}, [{"kind": "native-verification"}], issues, [], False)
    assert "verification/fault activity contaminated performance" in issues
    assert "failed attempt lacks authentic public operation receipts" in issues
    with tempfile.TemporaryDirectory(prefix="phase1-failed-report-check-") as temporary:
        root = Path(temporary)
        issues = []
        validate_git_custody(root / "precommit.tsv", root / "reopened.tsv", [{"kind": "sample-start"}], False, issues)
        assert not issues, "early Git failure incorrectly requires future custody"
        validate_git_custody(root / "precommit.tsv", root / "reopened.tsv", [{"kind": "git-precommit-custody"}], False, issues)
        assert issues, "reached Git custody accepted without its file"
        (root / "canonical-receipt.txt").write_text("artifact_encoding=gzip-v1\nartifact_compressor=/usr/bin/gzip -n -6 -c\n")
        tables = {"payload-extents.tsv.gz": "path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length\n", "file-roots.tsv.gz": "path\tcontent_root\n", "independent-manifest.tsv.gz": "workspace-independent-manifest-v1\n.\tdirectory\t0\t755\t0\t0\t-\n"}
        for name, text in tables.items():
            with gzip.open(root / name, "wt") as stream:
                stream.write(text)
        issues = []
        validate_canonical_artifacts(root, issues)
        assert not issues
        (root / "file-roots.tsv.gz").write_bytes(b"malformed gzip")
        try:
            validate_canonical_artifacts(root, [])
            raise AssertionError("malformed canonical gzip accepted")
        except OSError:
            pass
    print("failed_phase_and_git_custody_self_check=pass")


def suppression_scope_self_check():
    """Product-free inventory model: skips never masquerade as passes."""
    registry = [{"kind": "case", "scenario_id": f"{family}-{index}", "family_id": family}
                for family, count in FAMILY_COUNTS.items() for index in range(count)]
    registry += [{"kind": "case", "scenario_id": f"proof-{index}", "family_id": "workspace_reliability", "proof_only": True} for index in range(28)]
    registry += [{"kind": "case", "scenario_id": "cdc-proof", "family_id": "dedup_cdc_locality", "proof_only": True}]
    registry += [{"kind": "case", "scenario_id": f"edit-{index}-capped-v1", "family_id": "edit_length_changing_capped", "inherited": True, "proof_only": False} for index in range(5)]
    decisions = {row["scenario_id"]: {"status": "suppressed_phase1_time_budget"} for row in registry[:14]}
    active, omitted = phase1_scope(registry, decisions)
    assert len(active) == 755 and len(omitted) == 84
    assert sum(mode == "performance" and not case.get("inherited") for case, _, mode in active) == 348
    assert sum(case.get("proof_only", False) for case, _, _ in active) == 29
    assert all(row["coverage_status"] == "suppressed_phase1_time_budget" and row["product_status"] == "not-claimed" for row in omitted)
    decisions["edit-0-capped-v1"] = {"status": "suppressed_phase1_time_budget"}
    next_active, next_omitted = phase1_scope(registry, decisions)
    assert len(next_active) == len(active) - 6 and len(next_omitted) == len(omitted) + 6
    assert terminal_status([], [], [], []) == "PHASE1_TERMINAL_PASS"
    assert terminal_status([{}], [], [], []) == "NO_GO"
    for bad in ("missing-scenario", "cdc-proof"):
        try:
            phase1_scope(registry, {**decisions, bad: {}})
        except ValueError:
            pass
        else:
            raise AssertionError("invalid/standalone-proof suppression accepted")
    print("runtime_suppression_scope_model=pass original_new=130 suppressed_new=14 active_new=116 active_new_performance=348 standalone_proofs=29 capped_dynamic_removal=6")


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
    parser.add_argument("--suppression-scope-self-check", action="store_true")
    parser.add_argument("--failure-self-check", action="store_true")
    parser.add_argument("--aggregation-self-check", action="store_true")
    parser.add_argument("--canonical-receipt-self-check", action="store_true")
    args = parser.parse_args()
    if args.suppression_scope_self_check:
        suppression_scope_self_check()
        return 0
    if args.canonical_receipt_self_check:
        canonical_receipt_self_check()
        return 0
    if args.aggregation_self_check:
        aggregation_self_check()
        return 0
    if args.failure_self_check:
        failure_self_check()
        return 0
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
