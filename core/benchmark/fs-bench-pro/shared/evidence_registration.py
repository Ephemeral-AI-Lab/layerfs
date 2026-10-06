"""Fail-closed admission of the existing harness's cluster-two registrations."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

from shared.evidence_gates import LIMIT_DEFINITIONS, artifact, integer, resolve_limit
from shared.evidence_workload import write_trace

HASH64 = re.compile(r"[0-9a-f]{64}\Z")
HASH40 = re.compile(r"[0-9a-f]{40}\Z")
SEALS = ("product_sha256", "compilation_sha256", "dependency_sha256", "harness_sha256",
         "workload_sha256", "cache_sha256", "observer_sha256")
LIMITS = {name: definition[0] for name, definition in LIMIT_DEFINITIONS.items()}


def digest_json(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def _command(value):
    return isinstance(value, list) and bool(value) and all(isinstance(part, str) and part for part in value)


def _sealed_executable(command, binary, root):
    """Resolve owning-root aliases without accepting a different executable."""
    candidate = Path(command[0])
    if not candidate.is_absolute():
        candidate = Path(root) / candidate
    try:
        candidate = candidate.resolve(strict=True)
    except (OSError, RuntimeError):
        return False
    return candidate.is_relative_to(Path(root).resolve()) and candidate == binary


def _committed(root, commit, sealed):
    """The owning specification is already frozen in Git before admission."""
    if not isinstance(commit, str) or HASH40.fullmatch(commit) is None:
        raise ValueError("missing specification commit")
    artifact(sealed, root)
    result = subprocess.run(["git", "show", commit + ":" + sealed["path"]],
                            cwd=root, capture_output=True, timeout=5, check=False)
    if result.returncode or hashlib.sha256(result.stdout).hexdigest() != sealed["sha256"]:
        raise ValueError("specification is not frozen at its recorded commit")


def _identity(value, root):
    if not isinstance(value, dict):
        raise ValueError("missing sealed identity")
    for key in ("source_commit", "source_tree"):
        if not isinstance(value.get(key), str) or HASH40.fullmatch(value[key]) is None:
            raise ValueError("missing exact " + key)
    for key in SEALS:
        if not isinstance(value.get(key), str) or HASH64.fullmatch(value[key]) is None:
            raise ValueError("missing " + key)
    if value.get("build_profile") != "release" or value.get("locked") is not True:
        raise ValueError("locked release compilation required")
    if value.get("source_dirty") is not False:
        raise ValueError("dirty source cannot qualify sealed identity")
    if not _command(value.get("build_command")) or "--locked" not in value["build_command"] or "--release" not in value["build_command"]:
        raise ValueError("exact locked release build command missing")
    required = ("cargo_config", "cargo_lock", "toolchain", "product_manifest", "compilation_manifest", "dependency_manifest", "harness_manifest", "workload_manifest", "cache_manifest", "observer_manifest")
    for key in required:
        artifact(value.get(key), root)
    for name in ("product", "compilation", "dependency", "harness", "workload", "cache", "observer"):
        if value[name + "_manifest"]["sha256"] != value[name + "_sha256"]:
            raise ValueError("seal does not match its manifest: " + name)
    product = json.loads(artifact(value["product_manifest"], root).read_text())
    entries = product.get("artifacts")
    if product.get("schema") != "cluster-two-product-seal-v1" or not isinstance(entries, list) or not entries:
        raise ValueError("exact product artifact inventory missing")
    for entry in entries:
        artifact(entry, root)
        path = entry["path"]
        if not path.startswith("core/crates/") or "-legacy/" in path or "/layerfs-server/" in path:
            raise ValueError("reference/excluded implementation cannot supply product seal")
    artifacts = value.get("source_artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        raise ValueError("source artifact inventory missing")
    for item in artifacts:
        artifact(item, root)
    if value.get("host_os") != "macos" or value.get("linux_arch") != "aarch64":
        raise ValueError("authentic macOS-host/Linux-ARM64 topology missing")
    if not isinstance(value.get("image_id"), str) or re.fullmatch(r"sha256:[0-9a-f]{64}", value["image_id"]) is None:
        raise ValueError("exact Docker image ID missing")
    if not isinstance(value.get("kernel"), str) or not value["kernel"]:
        raise ValueError("Linux kernel identity missing")


def inspect_source(identity, root):
    """Check the sealed candidate against the checkout, without building it."""
    errors = []
    if not isinstance(identity, dict):
        return ["missing candidate identity"]
    for args, key in ((["rev-parse", "HEAD"], "source_commit"), (["rev-parse", "HEAD^{tree}"], "source_tree")):
        result = subprocess.run(["git", *args], cwd=root, capture_output=True, text=True, timeout=5, check=False)
        if result.returncode or result.stdout.strip() != identity.get(key):
            errors.append("checkout does not match " + key)
    result = subprocess.run(["git", "status", "--porcelain", "--untracked-files=all"],
                            cwd=root, capture_output=True, text=True, timeout=5, check=False)
    independent = {}
    for entry in identity.get("independent_changes", []):
        try:
            artifact(entry, root)
            if entry.get("scope") != "separately-owned-note" or not entry["path"].startswith("core/docs/") or not entry["path"].endswith(".md"):
                raise ValueError("only separately owned Markdown notes can be excluded")
            independent[entry["path"]] = entry
        except (ValueError, OSError) as error:
            errors.append(str(error))
    unsealed = [line for line in result.stdout.splitlines() if not (line.startswith("?? ") and line[3:] in independent)]
    if result.returncode or unsealed:
        errors.append("checkout has unsealed changes; classify/pin their independence before sampling")
    return errors


def _route(binding, case, root):
    driver = binding.get("driver") or {}
    if not _command(driver.get("command")):
        raise ValueError("compiled public-route driver command missing")
    binary = artifact(driver.get("binary"), root)
    artifact(driver.get("source"), root)
    if not _sealed_executable(driver["command"], binary, root):
        raise ValueError("driver command does not use sealed binary")
    proof = json.loads(artifact(driver.get("route_proof"), root).read_text())
    if not isinstance(proof, dict):
        raise ValueError("public route proof schema missing")
    wanted = {"route": case["route"]["id"], "entrypoint": case["route"]["entrypoint"],
              "binary_sha256": driver["binary"]["sha256"], "source_commit": (binding.get("identity") or {}).get("source_commit")}
    if proof.get("schema") != "cluster-two-route-proof-v1" or proof.get("status") != "PASS" or any(proof.get(key) != val for key, val in wanted.items()):
        raise ValueError("public route has no identity-matched proof")
    if any(not integer(proof.get(name)) or proof[name] != 0 for name in ("legacy_fallbacks", "forbidden_routes")):
        raise ValueError("forbidden/excluded product route")


def _construction(binding, case):
    identity = binding.get("identity") or {}
    if (identity.get("environment") or {}).get("LAYERFS_CONSTRUCTION_WORKERS") != "1":
        raise ValueError("construction environment must export LAYERFS_CONSTRUCTION_WORKERS=1")
    profile = identity.get("construction_profile")
    if not isinstance(profile, dict) or any(not integer(profile.get(key), positive=True) for key in ("constructor_worker_limit", "save_producer_limit", "environment_workers")) or profile != case.get("construction_profile"):
        raise ValueError("actual per-operation constructor profile mismatched; native Init retains four")


def _source_coverage(binding, case, catalog, root):
    identity = binding.get("identity") or {}
    declared = identity.get("source_artifacts")
    if not isinstance(declared, list):
        raise ValueError("registered source coverage missing")
    inventory = {item["path"]: item["sha256"] for item in declared if isinstance(item, dict) and isinstance(item.get("path"), str)}
    if len(inventory) != len(declared):
        raise ValueError("source inventory has missing or duplicate paths")
    product = json.loads(artifact(identity.get("product_manifest"), root).read_text())
    product_inventory = {item["path"]: item["sha256"] for item in product.get("artifacts", [])}
    pins = list(case["route"]["sources"])
    for name in case["observers"]:
        if not catalog[name]["sources"]:
            raise ValueError("registered observer has no implemented pinned source: " + name)
        pins.extend(catalog[name]["sources"])
    for pinned in pins:
        artifact(pinned, root)
        if inventory.get(pinned["path"]) != pinned["sha256"]:
            raise ValueError("declared inventory omits/mismatches registry pin: " + pinned["path"])
        if pinned["path"].startswith("core/crates/") and product_inventory.get(pinned["path"]) != pinned["sha256"]:
            raise ValueError("product seal omits/mismatches selected source: " + pinned["path"])


def _oracle(binding, case, root):
    oracle = binding.get("oracle") or {}
    artifact(oracle.get("source"), root)
    binary = artifact(oracle.get("binary"), root)
    if not _command(oracle.get("command")) or oracle.get("uses_candidate_result") is not False:
        raise ValueError("separate independent verifier missing")
    if not _sealed_executable(oracle["command"], binary, root):
        raise ValueError("oracle command does not use sealed binary")
    for key in case["oracle"]["required_coverage"]:
        if oracle.get("coverage", {}).get(key) is not True:
            raise ValueError("independent oracle coverage missing: " + key)


def _cache(binding, case, root):
    cache = binding.get("cache") or {}
    artifact(cache.get("contract"), root)
    if cache.get("mode") != case["cache"]["mode"] or cache.get("equal_arms") is not True:
        raise ValueError("phase-specific equal cache treatment missing")
    domains = cache.get("domains", {})
    for name in case["cache"]["domains"]:
        domain = domains.get(name, {})
        artifact(domain.get("enforcement_source"), root)
        if not isinstance(domain.get("method"), str) or not domain["method"] or domain.get("attest_each_phase") is not True:
            raise ValueError("cache enforcement/attestation missing: " + name)
    if cache.get("setup_credit") is not False or cache.get("own_write_credit") is not False:
        raise ValueError("setup/own-write cache credit forbidden")


def _observers(binding, case, catalog, root):
    selected = binding.get("observers") or {}
    for name in case["observers"]:
        item = selected.get(name, {})
        if item.get("schema") != catalog[name]["schema"]:
            raise ValueError("observer schema absent/mismatched: " + name)
        if item.get("source") not in catalog[name]["sources"]:
            raise ValueError("observer source is not its registered pin: " + name)
        artifact(item.get("source"), root)
        proof = json.loads(artifact(item.get("capability_proof"), root).read_text())
        if not isinstance(proof, dict) or proof.get("schema") != "cluster-two-observer-proof-v1" or proof.get("observer") != name or proof.get("status") != "PASS" or proof.get("source_sha256") != item["source"]["sha256"] or proof.get("unavailable_metrics"):
            raise ValueError("observer capability proof missing/mismatched: " + name)
        if item.get("scope") != "whole-original-operation" or item.get("unavailable_metrics"):
            raise ValueError("observer coverage incomplete: " + name)
        if name in ("host-residency", "linux-residency") and item.get("peak_kind") not in ("sampled", "continuous"):
            raise ValueError("lifetime resource peak forbidden: " + name)
    sql = binding.get("sql_plan_profile") or {}
    if sql.get("correlated") is not True:
        raise ValueError("EXPLAIN and correlated actual SQL profiles required")
    for name in ("plans", "profiles"):
        artifact(sql.get(name), root)


def _workload(binding, case, root):
    workload = json.loads(artifact(binding.get("workload_manifest"), root).read_text())
    fixture = json.loads(artifact(binding.get("fixture_manifest"), root).read_text())
    if workload.get("schema") != "cluster-two-workload-v1" or workload.get("case") != case["id"]:
        raise ValueError("workload schema/case mismatch")
    parameters = workload.get("parameters", {})
    for key, expected in case["workload"].items():
        if expected is not None and parameters.get(key) != expected:
            raise ValueError("workload changed frozen parameter: " + key)
        if expected is None and parameters.get(key) is None:
            raise ValueError("workload unresolved parameter: " + key)
    if fixture.get("schema") != "cluster-two-fixture-v1" or fixture.get("case") != case["id"] or fixture.get("uses_candidate_result") is not False:
        raise ValueError("independent fixture manifest missing")
    artifact(fixture.get("generator_source"), root)
    if fixture.get("seed") != 1 or not isinstance(fixture.get("expected_state_sha256"), str) or HASH64.fullmatch(fixture["expected_state_sha256"]) is None:
        raise ValueError("fixture seed/independent state seal missing")
    for count in ("regular_files", "directories", "symlinks", "aliases", "payload_bytes", "max_depth", "max_fanout", "device_capacity_bytes", "initial_allocated_bytes", "mutation_bytes", "body_disconnect_byte", "truncate_cutoff", "regrow_length"):
        if count in parameters and not integer(parameters[count]):
            raise ValueError("non-numeric workload parameter: " + count)
    if case["id"].startswith("Q0") and "entries" in parameters:
        if sum(parameters.get(key, 0) for key in ("regular_files", "directories", "symlinks")) != parameters["entries"] or parameters["aliases"] > parameters["regular_files"]:
            raise ValueError("full-root entry/kind/alias cardinality mismatch")
    dense = parameters.get("dense_base_bytes", parameters.get("dense_payload_bytes"))
    if dense is not None and (fixture.get("source_shape") != "dense" or fixture.get("actual_payload_bytes") != dense):
        raise ValueError("dense workload requires actual declared payload; sparse substitute forbidden")
    if case["id"] in ("E04-write-16m", "E05-write-1g") and workload.get("write_trace") != write_trace(parameters["dense_base_bytes"]):
        raise ValueError("dense write offsets/input hashes differ from frozen seed-1 trace")
    if case["id"] == "E06-read-mixed":
        for key, count, width in (("read_offsets", 64, 65536), ("overwrite_offsets", 16, 4096)):
            offsets = parameters.get(key)
            if not isinstance(offsets, list) or len(offsets) != count or any(not integer(offset) or offset + width > parameters["dense_base_bytes"] for offset in offsets):
                raise ValueError("fixed E06 offsets absent/out of range")
        if parameters["truncate_cutoff"] > parameters["regrow_length"] or parameters["regrow_length"] > parameters["dense_base_bytes"]:
            raise ValueError("E06 truncate/regrow frontier invalid")
    return parameters


def _service(binding, case, parameters, root):
    schedule = json.loads(artifact(binding.get("schedule_manifest"), root).read_text())
    if schedule.get("schema") != "cluster-two-schedule-v1" or schedule.get("case") != case["id"]:
        raise ValueError("finite schedule schema/case mismatch")
    arrivals = schedule.get("arrivals")
    if not isinstance(arrivals, list):
        raise ValueError("finite arrival list missing")
    if not parameters.get("schedule_required"):
        return []
    horizon = parameters.get("offered_horizon_ns", schedule.get("horizon_ns"))
    if not integer(horizon, positive=True) or schedule.get("horizon_ns") != horizon:
        raise ValueError("numerical offered horizon missing/mismatched")
    classes = parameters.get("classes", ["Demand", "Control"] if case["id"] == "P01-slow-peer" else ["Demand"])
    service = binding.get("service_classes")
    if not isinstance(service, dict) or set(service) != set(classes):
        raise ValueError("every ready service class requires its own numerical contract")
    totals = {name: [0, 0] for name in classes}
    previous = -1
    for arrival in arrivals:
        if not isinstance(arrival, dict) or arrival.get("class") not in totals:
            raise ValueError("arrival class absent/unknown")
        timestamp = arrival.get("at_ns")
        if not integer(timestamp) or timestamp < previous or timestamp >= horizon or not integer(arrival.get("bytes")):
            raise ValueError("arrival timestamp/byte trace invalid")
        for key, cap in (("workspace", parameters.get("workspaces", 1)), ("connection", parameters.get("connections", 1))):
            if not integer(arrival.get(key)) or arrival[key] >= cap:
                raise ValueError("arrival owner outside declared topology")
        previous = timestamp
        totals[arrival["class"]][0] += 1
        totals[arrival["class"]][1] += arrival["bytes"]
    if "logical_rpc" in parameters and len(arrivals) != parameters["logical_rpc"]:
        raise ValueError("finite offered-call cardinality mismatch")
    fields = ("offered_operations", "offered_bytes", "minimum_completed", "maximum_refused", "maximum_queue_jobs", "maximum_queue_bytes", "maximum_runnable_wait_ns", "maximum_runnable_turns", "drain_deadline_ns")
    gates = []
    for name in classes:
        row = service[name]
        if not isinstance(row, dict) or any(not integer(row.get(key)) for key in fields):
            raise ValueError("missing numerical service/refusal/queue/drain field: " + name)
        if [row["offered_operations"], row["offered_bytes"]] != totals[name] or row["minimum_completed"] > row["offered_operations"] or row["maximum_refused"] > row["offered_operations"]:
            raise ValueError("service gate disagrees with frozen arrival trace: " + name)
        if row["minimum_completed"] == 0 or row["maximum_runnable_wait_ns"] == 0 or row["maximum_runnable_turns"] == 0 or row["drain_deadline_ns"] == 0:
            raise ValueError("ready class cannot have zero progress/wait/drain contract: " + name)
        for metric, key, operator in (("offered_operations", "offered_operations", "eq"), ("offered_bytes", "offered_bytes", "eq"), ("completed", "minimum_completed", "ge"), ("refused", "maximum_refused", "le"), ("queue_jobs", "maximum_queue_jobs", "le"), ("queue_bytes", "maximum_queue_bytes", "le"), ("runnable_wait_ns", "maximum_runnable_wait_ns", "le"), ("runnable_turns", "maximum_runnable_turns", "le"), ("drain_ns", "drain_deadline_ns", "le")):
            gates.append({"id": "service-" + name + "-" + metric,
                          "metric": "service." + name + "." + metric,
                          "operator": operator, "value": row[key]})
    return gates


def validate(catalog, manifest, root):
    """Produce admission for all 27 rows; omitted siblings stay NOT_RUN.

Registration PASS is permission to attempt its predeclared arm, never a sample
or milestone PASS. Required gaps are individually retained as INCOMPLETE.
"""
    globals_ = []
    if manifest.get("schema") != "cluster-two-registration-v1" or manifest.get("registry_sha256") != digest_json(catalog):
        globals_.append("registry/schema identity missing or mismatched")
    if manifest.get("sample_count") != 0:
        globals_.append("registration must precede all candidate sampling")
    specification = manifest.get("specification", {})
    try:
        _committed(root, specification.get("commit"), specification.get("artifact"))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        globals_.append(str(error))
    unknown = set(manifest.get("rows", {})) - {case["id"] for case in catalog["cases"]}
    if unknown:
        globals_.append("unknown case IDs: " + ", ".join(sorted(unknown)))
    rows = []
    for case in catalog["cases"]:
        binding = manifest.get("rows", {}).get(case["id"], {})
        if binding.get("selected") is not True:
            rows.append({"case": case["id"], "registration_status": "NOT_RUN", "sample_status": "NOT_RUN",
                         "reason": "not selected", "sample_count": 0})
            continue
        errors = list(globals_)
        identity = binding.get("identity") or {}
        checks = (lambda: _identity(identity, root), lambda: _route(binding, case, root),
                  lambda: _construction(binding, case),
                  lambda: _source_coverage(binding, case, catalog["observers"], root),
                  lambda: _oracle(binding, case, root), lambda: _cache(binding, case, root),
                  lambda: _observers(binding, case, catalog["observers"], root))
        for check in checks:
            try:
                check()
            except (ValueError, OSError, KeyError, TypeError) as error:
                errors.append(str(error))
        if binding.get("profile") not in case["profiles"]:
            errors.append("supported global/local profile missing")
        if binding.get("source_arm") not in ("baseline", "candidate"):
            errors.append("registered source arm missing")
        if not isinstance(binding.get("sample_id"), str) or HASH64.fullmatch(binding["sample_id"]) is None or binding["sample_id"] == "0" * 64:
            errors.append("prospective unique single-sample identity missing")
        attempt_contract = binding.get("attempt_contract") or {}
        if attempt_contract.get("scope") != "declared-public-operation-attempts" or not integer(attempt_contract.get("count"), positive=True):
            errors.append("exact original public-operation attempt cardinality missing")
        if case.get("original_attempt_count") is not None and attempt_contract.get("count") != case["original_attempt_count"]:
            errors.append("original attempt count differs from source-qualified workload cardinality")
        if binding.get("resident_spread_contract") != case.get("resident_spread_contract"):
            errors.append("resident spread operands/meaning differ from frozen cohort")
        budgets = binding.get("budgets", {})
        expected = case["budgets"]
        for name in ("complete_command_ns", "verification_ns", "functional_ns"):
            if not integer(budgets.get(name), positive=True) or expected.get(name) is None or budgets[name] != expected[name]:
                errors.append("owning budget unresolved/mismatched: " + name)
        if binding.get("boundary_sha256") != digest_json(case["boundaries"]):
            errors.append("timer/acknowledgement/cleanup boundaries not sealed")
        if identity.get("workload_manifest") != binding.get("workload_manifest") or identity.get("cache_manifest") != (binding.get("cache") or {}).get("contract"):
            errors.append("row workload/cache do not match identity/control seals")
        for key in ("fixture_manifest", "workload_manifest", "schedule_manifest"):
            try:
                artifact(binding.get(key), root)
            except (ValueError, OSError) as error:
                errors.append(key + ": " + str(error))
        service_gates = []
        try:
            parameters = _workload(binding, case, root)
            service_gates = _service(binding, case, parameters, root)
            if case["id"] == "Q01-mixed-load" and attempt_contract.get("count") != sum(row["offered_operations"] for row in binding["service_classes"].values()):
                raise ValueError("mixed-load original attempts do not cover the frozen offered trace")
        except (ValueError, OSError, TypeError, KeyError) as error:
            errors.append(str(error))
        context = {**identity, "case": case["id"], "profile": binding.get("profile"), "route": case["route"]["id"]}
        try:
            _committed(root, specification.get("commit"), binding.get("gate_specification"))
            frozen = json.loads(artifact(binding.get("gate_specification"), root).read_text())
            if frozen.get("schema") != "cluster-two-gate-specification-v1" or frozen.get("case") != case["id"] or any(frozen.get(key) != binding.get(key) for key in ("limits", "service_classes", "source_arm", "sample_id", "attempt_contract", "resident_spread_contract")):
                raise ValueError("numerical gates differ from committed prospective specification")
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            errors.append(str(error))
        limits = {}
        for name in LIMITS:
            if name in case.get("not_applicable_limits", []):
                continue
            try:
                value = binding.get("limits", {}).get(name)
                if not isinstance(value, dict) or value.get("unit") != LIMITS[name]:
                    raise ValueError("missing/wrong unit")
                limits[name] = resolve_limit(value, root, {**context, "limit": name})
                if name.endswith("_ns") and name != "clock_error_ns" and limits[name] == 0:
                    raise ValueError("zero duration cannot qualify required service/observation")
                if name in ("host_resident_bytes", "linux_resident_bytes", "allocated_peak_bytes", "allocated_final_bytes", "queue_jobs", "queue_bytes") and limits[name] == 0:
                    raise ValueError("zero aggregate owner capacity cannot qualify admission")
            except (ValueError, OSError, TypeError) as error:
                errors.append("limit " + name + ": " + str(error))
        if limits.get("operation_ns", 0) > budgets.get("complete_command_ns", 0):
            errors.append("operation deadline exceeds complete-command budget")
        if limits.get("observer_interval_ns", 0) > limits.get("observer_gap_ns", 0):
            errors.append("observer interval exceeds allowed gap")
        if case["group"] == "engine" and limits.get("allocated_peak_bytes", 0) < 268_435_456:
            errors.append("physical envelope omits full daemon reservation")
        if case["group"] == "engine" and limits.get("headroom_bytes", 0) < 134_217_728:
            errors.append("protected cleanup headroom below owning allocation contract")
        rows.append({"case": case["id"], "registration_status": "INCOMPLETE" if errors else "PASS",
                     "sample_status": "NOT_RUN", "sample_count": 0, "errors": errors,
                     "resolved_limits": limits, "gates": case["gates"] + service_gates})
    selected = [row for row in rows if row["registration_status"] != "NOT_RUN"]
    status = "PASS" if selected and not globals_ and all(row["registration_status"] == "PASS" for row in selected) else "INCOMPLETE"
    return {"schema": "cluster-two-admission-v1", "status": status,
            "measurement_status": "NOT_RUN", "sample_count": 0,
            "global_errors": globals_, "rows": rows}
