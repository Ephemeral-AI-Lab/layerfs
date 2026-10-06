"""Read-only E01 diagnostic receipt consistency; never performance admission.

Scope: the committed cluster-two-e2-job-receipts-v1 specification. E04/E05 and
all E1 qualification dimensions remain unimplemented/unrun by this module.
"""
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[4]
WINDOW = 65_536
ARTIFACT_WINDOW = 512 << 20  # External metadata/binary validation bound, not a product file cap.
SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
STATEMENT = ("attempts", "executions", "rows_returned", "rows_changed", "direct_rows_changed",
             "returned_blob_bytes", "returned_value_bytes", "vm_steps", "fullscan_steps", "sorts",
             "autoindex_rows", "reprepares", "bound_bytes", "sql_bytes", "statement_memory_samples",
             "statement_memory_sample_bytes", "elapsed_ns")
ALLOCATION = ("attempts", "requested_bytes", "admitted_jobs", "refusals", "observations", "freelist_queries")
PAYLOAD = ("write_input_bytes", "write_cells", "partial_write_cells", "cell_copy_bytes",
           "cell_zeroed_bytes", "read_window_zeroed_bytes", "read_local_copy_bytes")
OWNER = ("admitted", "credited_bytes", "peak_credited_bytes", "outstanding", "maintenance_jobs",
         "maintenance_rows", "maintenance_data_bytes", "maintenance_ns")
ARTIFACTS = ("product", "toolchain", "cargo_config", "cargo_lock", "dependencies", "binary",
             "driver", "fixture", "trace", "cache", "observer")
GAPS = ("route-dependent-resources-pages-freelist-probes", "whole-operation-copies",
        "statement-fingerprint-bind-correlation", "indexed-visited-rows", "exact-eligible-debt",
        "queue-only-peak", "isolated-resource-runnable-wait", "host-linux-kernel-phase-residency",
        "physical-io", "cache-enforcement-calibration")
SUBSTITUTIONS = ("phase_resident_bytes", "eligible_debt_peak_bytes", "queue_peak_jobs",
                 "whole_operation_copy_bytes", "rss_phase_peak_bytes", "physical_read_bytes",
                 "physical_write_bytes", "runnable_wait_max_ns", "resource_wait_max_ns")


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def integer(value):
    return type(value) is int and 0 <= value <= (1 << 64) - 1


def hash_bytes(value):
    return hashlib.sha256(value).hexdigest()


def read_json(path, maximum=WINDOW):
    with Path(path).open("rb") as source:
        value = source.read(maximum + 1)
    require(len(value) <= maximum, "JSON byte window exceeded: " + str(path))
    # Duplicate JSON keys can otherwise hide an earlier identity/counter claim.
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key: " + key)
            result[key] = value
        return result
    return json.loads(value, object_pairs_hook=unique), value


def counter_map(value, names, label):
    require(isinstance(value, dict) and set(value) == set(names), label + ": exact counter inventory missing")
    require(all(integer(value[key]) for key in names), label + ": unavailable/invalid counter, never zero-fill")
    return value


def exact(value, names, label):
    require(isinstance(value, dict) and set(value) == set(names), label + ": unregistered/missing nested fields")
    return value


def database(value):
    exact(value, ("scope", "families", "total"), "SQL database observation")
    require(isinstance(value, dict) and value.get("scope") == "connection-statement-families", "SQL counter scope mismatch")
    families = value.get("families")
    require(isinstance(families, list) and len(families) == 14, "SQL statement-family cardinality mismatch")
    for row in families:
        counter_map(row, STATEMENT, "SQL family")
        require(row["rows_changed"] >= row["direct_rows_changed"], "trigger-inclusive changes below direct changes")
        require(row["returned_value_bytes"] >= row["returned_blob_bytes"], "delivered column bytes below BLOB bytes")
    total = counter_map(value.get("total"), STATEMENT, "SQL total")
    require(all(total[key] == sum(row[key] for row in families) for key in STATEMENT), "SQL total differs from original families")
    return total


def artifact(descriptor, root):
    require(isinstance(descriptor, dict) and isinstance(descriptor.get("path"), str)
            and isinstance(descriptor.get("sha256"), str) and SHA.fullmatch(descriptor["sha256"]), "artifact descriptor missing")
    root = Path(root).resolve()
    path = (root / descriptor["path"]).resolve()
    require(path.is_relative_to(root) and path.is_file(), "artifact outside owning root or absent")
    require(path.stat().st_size <= ARTIFACT_WINDOW, "external artifact hashing window exceeded")
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as source:
        for block in iter(lambda: source.read(8192), b""):
            count += len(block)
            require(count <= ARTIFACT_WINDOW, "external artifact grew beyond hashing window")
            digest.update(block)
    require(digest.hexdigest() == descriptor["sha256"], "artifact hash mismatch: " + descriptor["path"])
    return path


def identity(value, root):
    require(isinstance(value, dict) and value.get("schema") == "cluster-two-e2-identity-v1", "diagnostic identity schema missing")
    require(isinstance(value.get("receipt_id"), str) and SHA.fullmatch(value["receipt_id"])
            and value["receipt_id"] != "0" * 64, "unique diagnostic receipt identity missing")
    require(all(isinstance(value.get(key), str) and COMMIT.fullmatch(value[key]) for key in ("source_commit", "source_tree")), "source commit/tree identity missing")
    require(value.get("source_arm") == "diagnostic" and value.get("profile") == "local-overlay-disposable"
            and value.get("cache_state") == "uncontrolled" and value.get("source_sealed") is False,
            "diagnostic cannot claim a sealed treatment/cache/global persistence arm")
    execution = value.get("execution_kind")
    require(execution in ("standalone-host", "docker"), "native host/Docker execution applicability missing")
    topology = value.get("topology")
    require(isinstance(topology, dict), "topology observed/unavailable witness missing")
    if topology.get("status") == "OBSERVED":
        witness, _ = read_json(artifact(topology.get("artifact"), root))
        require(witness.get("schema") == "cluster-two-e2-topology-witness-v1" and witness.get("status") == "OBSERVED"
                and witness.get("scope") == "external-observed-execution-domain" and witness.get("execution_kind") == execution
                and isinstance(witness.get("source"), str) and witness["source"]
                and all(isinstance(value.get(key), str) and value[key] and witness.get(key) == value[key]
                        for key in ("os", "architecture", "kernel")), "supplied topology lacks matched observed witness")
    else:
        require(topology.get("status") == "UNAVAILABLE" and isinstance(topology.get("reason"), str)
                and topology["reason"] and all(value.get(key) is None for key in ("os", "architecture", "kernel")),
                "unavailable topology was fabricated as a physical fact")
        raise ValueError("execution topology observation unavailable")
    descriptors = value.get("artifacts")
    require(isinstance(descriptors, dict) and set(descriptors) == set(ARTIFACTS), "identity artifact inventory incomplete")
    paths = {key: artifact(descriptors[key], root) for key in ARTIFACTS}
    product, _ = read_json(paths["product"], 1 << 20)
    require(product.get("schema") == "cluster-two-e2-source-inventory-v1"
            and all(product.get(key) == value[key] for key in ("source_commit", "source_tree", "source_sealed")), "product inventory belongs to another source identity")
    sources = product.get("sources")
    require(isinstance(sources, list) and 0 < len(sources) <= 4096, "current diagnostic source inventory missing")
    declared = set()
    for source in sources:
        path = artifact(source, root)
        require(path not in declared, "duplicate diagnostic source artifact")
        declared.add(path)
    image = value.get("image")
    require(isinstance(image, dict), "image applicability missing")
    if image.get("status") == "NOT_IN_SCOPE":
        require(execution == "standalone-host" and isinstance(image.get("reason"), str) and image["reason"], "Docker execution cannot omit exact image")
    else:
        image_path = artifact(image, root)
        if execution == "docker":
            image_witness, _ = read_json(image_path)
            require(image_witness.get("schema") == "cluster-two-e2-image-witness-v1"
                    and image_witness.get("status") == "OBSERVED"
                    and isinstance(image_witness.get("source"), str) and image_witness["source"]
                    and isinstance(image_witness.get("image_id"), str)
                    and re.fullmatch(r"sha256:[0-9a-f]{64}", image_witness["image_id"]),
                    "Docker image requires exact observed image-ID witness")
    command, _ = read_json(artifact(value.get("command_artifact"), root))
    require(command.get("schema") == "cluster-two-e2-command-v1" and command.get("execution_kind") == execution
            and isinstance(command.get("argv"), list) and len(command["argv"]) == 4
            and all(isinstance(item, str) for item in command["argv"]), "exact external command artifact missing")
    require((Path(root) / command["argv"][0]).resolve() == paths["binary"], "command does not invoke exact sealed diagnostic binary")
    if execution == "docker":
        require(command.get("image_id") == image_witness["image_id"]
                and isinstance(command.get("external_argv"), list) and command["external_argv"]
                and all(isinstance(item, str) for item in command["external_argv"])
                and image_witness["image_id"] in command["external_argv"],
                "Docker command/image identity missing or mixed")
    return descriptors["binary"]["sha256"], command


def invocation(manifest, command, directory, root):
    value = exact(manifest.get("invocation"), ("schema", "path_scope", "bound_before_startup", "argv",
        "identity_source_sha256", "identity_source_bytes", "identity_copy_sha256", "docker_path_mapping"), "original invocation")
    require(value["schema"] == "cluster-two-e2-invocation-v1" and value["path_scope"] == "process-local-canonical-inputs"
            and value["bound_before_startup"] is True, "inputs not bound before original startup")
    mapping = exact(value["docker_path_mapping"], ("status", "reason"), "Docker path mapping applicability")
    require(mapping == {"status": "UNAVAILABLE", "reason": "no-verified-host-container-path-mapping"}, "unimplemented Docker mapping claimed verified")
    if manifest["identity"]["execution_kind"] == "docker":
        raise ValueError("Docker host/container input path mapping UNAVAILABLE; no guessed aliases")
    argv = value["argv"]
    require(isinstance(argv, list) and len(argv) == 4 and all(isinstance(arg, str) and Path(arg).is_absolute() for arg in argv), "canonical original invocation argv missing")
    declared = command["argv"]
    require(all(Path(arg).is_absolute() for arg in declared), "command witness requires explicit absolute input paths")
    for actual, expected in zip(argv, declared):
        require(Path(actual) == Path(actual).resolve() and Path(actual) == Path(expected).resolve(), "command witness differs from actual binary/DB/output/identity invocation")
    require(Path(argv[2]) == Path(directory).resolve(), "command targets another output artifact set")
    binary = artifact(manifest["identity"]["artifacts"]["binary"], root)
    require(Path(argv[0]) == binary, "invocation binary differs from original artifact")
    supplied, raw = read_json(argv[3], 16_384)
    require(supplied == manifest["identity"] and integer(value["identity_source_bytes"])
            and value["identity_source_bytes"] == len(raw) and value["identity_source_sha256"] == hash_bytes(raw)
            and value["identity_copy_sha256"] == manifest["identity_sha256"]
            and hash_bytes(raw.strip()) == manifest["identity_sha256"], "command identity input differs from actual copied input artifact")
    return argv


def substitutions(value):
    if isinstance(value, dict):
        for key, child in value.items():
            if key in ("evidence_admission", "retained_evidence_status", "performance_status", "numeric_gate_status"):
                raise ValueError("numeric/performance/evidence qualification claim outside diagnostic scope")
            if key in SUBSTITUTIONS:
                require(child is None, "unavailable counter/phase substituted: " + key)
            substitutions(child)
    elif isinstance(value, list):
        for child in value:
            substitutions(child)


def envelope(record, manifest, expected_kind, expected_index, ids):
    require(isinstance(record, dict), "record object missing")
    common = {"schema", "case", "kind", "record_index", "record_id", "attempt_id", "owner_id", "mode",
              "sample_count", "admission_eligible", "qualification_status", "global_persistence", "source_binding",
              "build_target_os", "build_target_architecture", "identity", "identity_sha256", "actual_binary_sha256", "invocation"}
    fields = {"startup": {"scope", "attempt_count", "clock", "opened_ns", "closed_ns", "owner_elapsed_ns",
                           "configuration", "creation_reported", "original_outcome", "creation"},
              "owner-diagnostics": {"endpoint", "scope", "is_sql_command", "at_ns", "status", "work"},
              "outcomes": {"startup_status", "stop_status", "route", "route_status", "source_owners", "reply_publications",
                           "complete_command_ns", "complete_command_status", "collector_span_scope", "collector_span_ns",
                           "original_error", "database_artifact"},
              "manifest": {"driver_version", "e1_sample_status", "e1_sample_count", "observation_consistency", "streams",
                           "identity_input", "outcomes", "unavailable", "forbidden_substitutions", "database_retained"}}
    allowed = common | fields[expected_kind]
    if expected_kind == "owner-diagnostics" and record.get("status") == "UNAVAILABLE":
        allowed.add("error")
    require(set(record) == allowed, "unregistered/missing original record fields or counter-scope claim")
    for key in ("schema", "case", "mode", "sample_count", "admission_eligible", "qualification_status",
                "global_persistence", "identity", "identity_sha256", "actual_binary_sha256", "attempt_id", "owner_id", "invocation"):
        require(record.get(key) == manifest.get(key), "mixed/missing record identity or scope: " + key)
    require(record.get("source_binding") == "base-commit-plus-current-inventory"
            and record.get("build_target_os") == manifest.get("build_target_os")
            and record.get("build_target_architecture") == manifest.get("build_target_architecture"),
            "dirty source relabeled as committed arm or build target mixed")
    require(record.get("kind") == expected_kind and type(record.get("record_index")) is int
            and record["record_index"] == expected_index, "record membership/order mismatch")
    record_id = hash_bytes(f"{manifest['identity_sha256']}:{expected_kind}:{expected_index}".encode())
    require(record.get("record_id") == record_id and record_id not in ids, "duplicate/missing original receipt identity")
    ids.add(record_id)
    substitutions(record)


def stream(directory, descriptor, name, count, status, jsonl):
    require(isinstance(descriptor, dict) and descriptor.get("path") == name
            and descriptor.get("status") == status and descriptor.get("records") == count
            and type(descriptor.get("records")) is int, "stream membership/cardinality/disposition mismatch: " + name)
    path = Path(directory) / name
    require(path.is_file() and not path.is_symlink(), "original stream missing/aliased: " + name)
    digest = hashlib.sha256()
    length = 0
    lines = 0
    records = []
    with path.open("rb") as source:
        for line in iter(lambda: source.readline(WINDOW + 1), b""):
            require(len(line) <= WINDOW and line.endswith(b"\n"), "truncated/oversized original record")
            digest.update(line)
            lines += 1
            length += len(line)
            require(length <= WINDOW * max(count, 1), "stream exceeds frozen original cardinality window")
            if jsonl:
                require(len(records) < count, "duplicate/extra original record")
                # Reuse duplicate-key refusal without an unbounded full stream.
                def unique(pairs):
                    output = {}
                    for key, value in pairs:
                        require(key not in output, "duplicate JSON record key")
                        output[key] = value
                    return output
                records.append(json.loads(line, object_pairs_hook=unique))
    require(integer(descriptor.get("bytes")) and length == descriptor["bytes"]
            and descriptor.get("sha256") == digest.hexdigest(), "stream bytes/hash differ from original manifest")
    if jsonl:
        require(len(records) == count, "missing original stream record")
    else:
        require(lines == count, "text stream actual record cardinality differs")
    return records


def startup(record):
    require(record.get("scope") == "original-startup-through-readiness-or-error"
            and record.get("attempt_count") == 1 and type(record.get("attempt_count")) is int,
            "exact original startup scope/attempt missing")
    require(record.get("clock") == "process-local-Instant-elapsed"
            and all(integer(record.get(key)) for key in ("opened_ns", "closed_ns", "owner_elapsed_ns"))
            and record["opened_ns"] <= record["closed_ns"]
            and record["owner_elapsed_ns"] <= record["closed_ns"] - record["opened_ns"], "startup causal clock window invalid")
    creation = record.get("creation")
    require(isinstance(creation, dict) and type(record.get("creation_reported")) is bool, "original creation availability missing")
    if not record["creation_reported"]:
        exact(creation, ("status", "work", "reason"), "unavailable original creation")
        require(creation.get("status") == "UNAVAILABLE" and creation.get("work") is None, "unreported startup work was fabricated as zero")
        return None
    require(creation.get("status") == "OBSERVED", "reported creation work unavailable")
    exact(creation, ("status", "work"), "original creation observation")
    work = creation.get("work")
    exact(work, ("calls", "sql", "allocation", "elapsed_ns", "payload", "payload_status", "allocation_state"), "original creation work")
    counter_map(work.get("calls"), ("file_create_calls", "sqlite_open_calls", "connection_configuration_calls", "cache_configuration_calls"), "creation calls")
    database(work.get("sql"))
    counter_map(work.get("allocation"), ALLOCATION, "creation allocation")
    require(integer(work.get("elapsed_ns")) and work["elapsed_ns"] <= record["owner_elapsed_ns"], "creation span outside readiness")
    require(work.get("payload") is None and work.get("payload_status") == "UNAVAILABLE-not-reported-by-CreationWork", "startup payload scope/availability invented")
    state = work.get("allocation_state")
    require(isinstance(state, dict), "allocation readback disposition missing")
    if state.get("status") != "OBSERVED":
        exact(state, ("status", "values", "error") if state.get("status") == "FAILED" else ("status", "values", "reason"), "unavailable allocation observation")
        require(state.get("status") in ("FAILED", "UNAVAILABLE") and state.get("values") is None, "unavailable allocation readback became zero")
        return None
    exact(state, ("status", "high_water_scope", "requested_volume_meaning", "values", "work"), "allocation observation")
    require(state.get("high_water_scope") == "daemon-startup-lifetime-observed-allocation"
            and state.get("requested_volume_meaning") == "range-call-volume-not-new-disk", "allocation scope/meaning substituted")
    values = counter_map(state.get("values"), ("logical_bytes", "allocated_bytes", "high_water_allocated_bytes", "reserved_tail_bytes", "cleanup_headroom_bytes"), "allocation readback")
    counter_map(state.get("work"), ALLOCATION, "allocation snapshot work")
    require(values["high_water_allocated_bytes"] >= values["allocated_bytes"], "allocation lifetime high-water below observed allocation")
    require(values["reserved_tail_bytes"] <= values["allocated_bytes"] and values["allocated_bytes"] >= 268_435_456
            and values["cleanup_headroom_bytes"] == 134_217_728, "full reservation/cleanup capacity not actually observed")
    return work


def ready_profile(record):
    configuration = record.get("configuration") or {}
    exact(configuration, ("profile", "owner"), "actual configuration")
    require(configuration.get("profile") == {"pager_kib": 2048, "max_pages": None}
            and configuration.get("owner") == {"bytes": 8_388_608, "lifecycle_reserve": 65_536,
                "namespaces": 16, "jobs_per_namespace": 16, "lifecycle_jobs_per_namespace": 2}, "E01 changed default configuration")
    value = (record.get("original_outcome") or {}).get("profile")
    require(isinstance(value, dict) and value.get("sqlite_version") in ("3.51.0", "3.53.2"), "actual local SQLite profile missing")
    expected = {"journal_mode": "memory", "locking_mode": "exclusive", "synchronous": 0,
                "mmap_size": 0, "cache_size": -2048, "page_size": 4096, "max_pages": 4_294_967_294,
                "explicit_page_quota": None, "foreign_keys": 1, "busy_timeout": 0, "temp_store": 1, "auto_vacuum": 0}
    exact(value, (*expected, "sqlite_version", "schema_version", "compile_options"), "actual local profile")
    require(all(value.get(key) == wanted and type(value.get(key)) is type(wanted) for key, wanted in expected.items()), "local profile readback differs from actual default contract")
    require(type(value.get("schema_version")) is int and value["schema_version"] > 0
            and isinstance(value.get("compile_options"), list) and value["compile_options"]
            and all(isinstance(option, str) for option in value["compile_options"]), "actual schema/build readback missing")


def owner_probe(record, endpoint):
    require(record.get("scope") == "separate-post-ready-owner-diagnostics" and record.get("is_sql_command") is False
            and record.get("endpoint") == endpoint and integer(record.get("at_ns")), "post-ready probe scope/endpoint invalid")
    require(record.get("status") == "OBSERVED" and isinstance(record.get("work"), dict), "required owner diagnostics unavailable")
    work = record["work"]
    exact(work, ("scope", "credit_scope", "sql_foreground", "sql_maintenance", "payload_foreground", "payload_maintenance",
                "allocation_foreground", "allocation_maintenance", "counters", "completed", "queue_wait_ns", "service_ns"), "owner diagnostics work")
    require(work.get("scope") == "same-owner-cumulative"
            and work.get("credit_scope") == "queued-executing-caller-held-results", "owner cumulative/result credit mislabeled as queue/phase")
    for key in ("sql_foreground", "sql_maintenance"):
        database(work.get(key))
    for key in ("payload_foreground", "payload_maintenance"):
        counter_map(work.get(key), PAYLOAD, key)
    for key in ("allocation_foreground", "allocation_maintenance"):
        counter_map(work.get(key), ALLOCATION, key)
    counters = counter_map(work.get("counters"), OWNER, "owner counters")
    for key in ("completed", "queue_wait_ns", "service_ns"):
        require(isinstance(work.get(key), list) and len(work[key]) == 6 and all(integer(item) for item in work[key]), "class original counter inventory missing")
    require(counters["admitted"] == 0 and counters["outstanding"] == 0 and counters["credited_bytes"] == 0
            and counters["peak_credited_bytes"] == 0 and all(value == 0 for value in work["completed"]), "startup-only secretly submitted/retained original jobs")
    require(all(value == 0 for value in work["sql_foreground"]["total"].values())
            and all(value == 0 for value in work["payload_foreground"].values())
            and all(value == 0 for value in work["allocation_foreground"].values())
            and all(value == 0 for value in work["queue_wait_ns"] + work["service_ns"]), "unreceipted foreground diagnostic job or startup misattribution")
    return work


def monotonic(before, after):
    if isinstance(before, dict):
        require(isinstance(after, dict) and set(before) == set(after), "counter endpoint inventory differs")
        for key in before:
            monotonic(before[key], after[key])
    elif isinstance(before, list):
        require(isinstance(after, list) and len(before) == len(after), "counter endpoint cardinality differs")
        for a, b in zip(before, after):
            monotonic(a, b)
    elif type(before) is int:
        require(integer(after) and after >= before, "same-owner cumulative counter decreased")
    else:
        require(before == after, "counter scope changed between endpoints")


def validate(directory, root=ROOT):
    """Read exact retained E01 files. Never invoke a driver or modify a verdict."""
    result = {"schema": "cluster-two-e2-observation-consistency-v1", "case": "E01-startup",
              "observation_consistency": "INCOMPLETE", "qualification_status": "NOT_EVALUATED",
              "admission_eligible": False, "e1_sample_status": "NOT_RUN", "e1_sample_count": 0,
              "unavailable": list(GAPS), "errors": []}
    failed = False
    try:
        manifest, _ = read_json(Path(directory) / "manifest.json")
        require(manifest.get("schema") == "cluster-two-job-receipts-v1" and manifest.get("case") == "E01-startup"
                and manifest.get("kind") == "manifest" and manifest.get("mode") == "diagnostic"
                and manifest.get("sample_count") == 0 and type(manifest.get("sample_count")) is int
                and manifest.get("admission_eligible") is False and manifest.get("qualification_status") == "NOT_EVALUATED"
                and manifest.get("global_persistence") == "NOT_IN_SCOPE" and manifest.get("e1_sample_status") == "NOT_RUN"
                and manifest.get("e1_sample_count") == 0 and type(manifest.get("e1_sample_count")) is int
                and manifest.get("observation_consistency") == "NOT_EVALUATED", "diagnostic manifest claimed qualification/sample/other route")
        binary_hash, command = identity(manifest.get("identity"), root)
        require(manifest.get("actual_binary_sha256") == binary_hash, "actual executing binary differs from identity")
        require(isinstance(manifest.get("identity_sha256"), str) and SHA.fullmatch(manifest["identity_sha256"]), "identity digest missing")
        # Hash of supplied identity's exact bytes is retained separately; an
        # artifact points to those original bytes to avoid reserialization guesses.
        identity_descriptor = manifest.get("identity_input")
        require(isinstance(identity_descriptor, dict) and identity_descriptor.get("path") == "identity.json", "exact supplied identity bytes artifact missing")
        identity_path = Path(directory) / "identity.json"
        require(identity_path.is_file() and not identity_path.is_symlink(), "supplied identity copy absent or aliased")
        supplied, identity_raw = read_json(identity_path, 16_384)
        require(supplied == manifest["identity"] and hash_bytes(identity_raw.strip()) == manifest["identity_sha256"]
                and identity_descriptor.get("sha256") == hash_bytes(identity_raw)
                and identity_descriptor.get("bytes") == len(identity_raw) and type(identity_descriptor.get("bytes")) is int,
                "mixed/edited supplied identity bytes")
        require(manifest.get("build_target_os") == supplied["os"] and manifest.get("build_target_architecture") == supplied["architecture"], "execution witness differs from actual compiled target")
        actual_argv = invocation(manifest, command, directory, root)
        require(manifest.get("attempt_id") == hash_bytes(f"{manifest['identity_sha256']}:attempt:0".encode())
                and manifest.get("owner_id") == hash_bytes(f"{manifest['identity_sha256']}:owner:0".encode()), "original startup/owner identity missing")
        require(manifest.get("unavailable") == list(GAPS), "unavailable coverage was omitted or relabeled")
        require(manifest.get("database_retained") is True, "database custody/retention not explicit")
        substitutions(manifest)
        ids = set()
        envelope(manifest, manifest, "manifest", 4, ids)
        outcome_desc = manifest.get("outcomes")
        require(isinstance(outcome_desc, dict) and outcome_desc.get("path") == "outcomes.json", "original outcomes inventory absent")
        outcomes, raw = read_json(Path(directory) / "outcomes.json")
        require(outcome_desc.get("sha256") == hash_bytes(raw) and outcome_desc.get("bytes") == len(raw)
                and type(outcome_desc.get("bytes")) is int, "outcome bytes/hash mismatch")
        envelope(outcomes, manifest, "outcomes", 3, ids)
        streams = manifest.get("streams")
        require(isinstance(streams, list) and len(streams) == 5, "stream inventory incomplete")
        require([item.get("path") for item in streams] == ["startup.jsonl", "jobs.jsonl", "probes.jsonl", "stdout.txt", "stderr.txt"], "missing/duplicate/reordered stream")
        ready = outcomes.get("startup_status") == "READY"
        recorded_startup = stream(directory, streams[0], "startup.jsonl", 1, "RECORDED", True)[0]
        stream(directory, streams[1], "jobs.jsonl", 0, "UNRUN-route_unavailable", True)
        probes = stream(directory, streams[2], "probes.jsonl", 2 if ready else 0,
                        "RECORDED" if ready else "UNRUN-startup_failed", True)
        stopped = ready and outcomes.get("stop_status") == "STOPPED"
        stream(directory, streams[3], "stdout.txt", 1 if stopped else 0, "COLLECTOR_LOG", False)
        stream(directory, streams[4], "stderr.txt", 0 if stopped else 1, "COLLECTOR_LOG", False)
        envelope(recorded_startup, manifest, "startup", 0, ids)
        outcome = recorded_startup.get("original_outcome") or {}
        exact(outcome, ("status", "error", "profile"), "original startup outcome")
        require(outcome.get("status") == outcomes.get("startup_status"), "startup outcome was replaced by later result")
        require(outcomes.get("route") is None and outcomes.get("route_status") == "NOT_IN_SCOPE"
                and outcomes.get("source_owners") is None and outcomes.get("reply_publications") is None,
                "startup-only claimed Workspace/source/publication qualification")
        require(outcomes.get("complete_command_ns") is None and outcomes.get("complete_command_status") == "UNAVAILABLE-external-command-wall-required"
                and outcomes.get("collector_span_scope") == "entry-through-stop-and-log-before-final-files"
                and integer(outcomes.get("collector_span_ns")), "collector prefix mislabeled as complete command")
        artifact_state = outcomes.get("database_artifact")
        require(isinstance(artifact_state, dict) and isinstance(artifact_state.get("path"), str)
                and Path(artifact_state["path"]).is_absolute() and Path(artifact_state["path"]).resolve() == Path(actual_argv[1]),
                "command DB input differs from collected database artifact")
        failed = not ready or not stopped
        work = startup(recorded_startup)
        if not ready or not stopped:
            failed = True
            require(isinstance(outcomes.get("original_error"), str) and outcomes["original_error"], "failed/unattempted outcome original error missing")
            if not ready:
                require(outcomes.get("startup_status") == "FAILED" and outcomes.get("stop_status") == "UNRUN-no-ready-owner"
                        and isinstance(outcome.get("error"), str) and outcome["error"], "failed startup was qualified or Stop invented")
                require(outcomes["original_error"] == outcome["error"], "failed startup original error was replaced")
            else:
                require(outcomes.get("stop_status") == "FAILED", "original Stop disposition missing")
            result["errors"].append("original startup/Stop failed; retained observations do not qualify success")
        else:
            require(work is not None and outcome.get("error") is None and outcomes.get("original_error") is None, "successful startup required observations unavailable")
            ready_profile(recorded_startup)
            require(work["calls"] == {"file_create_calls": 1, "sqlite_open_calls": 1,
                    "connection_configuration_calls": 2, "cache_configuration_calls": 1}
                    and work["sql"]["total"]["attempts"] > 0 and work["sql"]["total"]["executions"] > 0,
                    "ready startup omitted actual file/open/configuration/SQL work")
            require(work["allocation"]["attempts"] == 1 and work["allocation"]["admitted_jobs"] == 1
                    and work["allocation"]["refusals"] == 0 and work["allocation"]["requested_bytes"] >= 268_435_456
                    and work["allocation"]["observations"] > 0
                    and work["allocation_state"]["work"] == work["allocation"],
                    "ready startup omitted or mixed original reservation/readback work")
            for index, (probe, endpoint) in enumerate(zip(probes, ("post-ready", "pre-stop")), 1):
                envelope(probe, manifest, "owner-diagnostics", index, ids)
                owner_probe(probe, endpoint)
                require(probe["at_ns"] >= recorded_startup["closed_ns"] and probe["at_ns"] <= outcomes["collector_span_ns"], "diagnostic probe charged to startup or outside collector")
            require(probes[1]["at_ns"] >= probes[0]["at_ns"], "probe endpoint order reversed")
            monotonic(probes[0]["work"], probes[1]["work"])
            exact(artifact_state, ("scope", "status", "path", "logical_bytes", "allocated_bytes", "device", "inode", "links"), "retained artifact observation")
            require(artifact_state.get("scope") == "separate-post-stop-artifact-stat" and artifact_state.get("status") == "OBSERVED"
                    and isinstance(artifact_state.get("path"), str) and Path(artifact_state["path"]).is_absolute()
                    and all(integer(artifact_state.get(key)) for key in ("logical_bytes", "allocated_bytes", "device", "inode", "links"))
                    and artifact_state["links"] == 1, "actual retained database artifact identity/allocation unavailable")
            actual = Path(artifact_state["path"]).lstat()
            require(Path(artifact_state["path"]).is_file() and not Path(artifact_state["path"]).is_symlink()
                    and (actual.st_size, actual.st_blocks * 512, actual.st_dev, actual.st_ino, actual.st_nlink)
                    == tuple(artifact_state[key] for key in ("logical_bytes", "allocated_bytes", "device", "inode", "links")),
                    "retained database artifact identity/allocation changed")
            result["observation_consistency"] = "PASS"
    except (ValueError, OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        result["errors"].append(str(error))
    if failed:
        result["observation_consistency"] = "FAIL"
    return result
