"""Read-only original-job diagnostic consistency; never performance admission.

Scope: original v1 receipts and the E04 native-backing v2 successor. E01 startup
and E04's frozen 1000 aligned writes have distinct predicates. E05 and all E1
qualification dimensions remain unrun; validation never executes a collector.
"""
import hashlib
import json
from pathlib import Path
import re
from .evidence_workload import payload_chunks, write_trace
from . import evidence_backing, evidence_disposal

ROOT = Path(__file__).resolve().parents[4]
WINDOW = 65_536
ARTIFACT_WINDOW = 512 << 20  # External metadata/binary validation bound, not a product file cap.
SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
E04_BYTES = 16_777_216
E04_WRITES = 1000
E04_PAYLOAD_MODEL = "borrowed-whole-row-v1"
E04_LEGACY_PAYLOAD_MODEL = "legacy-copied-cell-v1"
E04_VERSIONS = {"cluster-two-job-receipts-v1": "e04-original-write-receipts-v1",
                "cluster-two-job-receipts-v2": "e04-original-write-receipts-v2",
                "cluster-two-job-receipts-v3": "e04-original-write-receipts-v3"}
CLIENT = ("cache_hits", "cache_misses", "upstream_batches", "authenticated_bytes",
          "evictions", "charged_cache_bytes", "cached_objects")
E04_GAPS = ("private-base-facts-provider-id-trace", "per-original-job-statement-families",
            "whole-operation-copies", "statement-fingerprint-bind-correlation", "indexed-visited-rows",
            "exact-eligible-debt", "queue-only-peak", "isolated-resource-runnable-wait",
            "host-linux-kernel-phase-residency", "physical-io", "cache-enforcement-calibration")
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


def identity(value, root, command_arguments=4, invoked_binary=None):
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
            and isinstance(command.get("argv"), list) and len(command["argv"]) == command_arguments
            and all(isinstance(item, str) for item in command["argv"]), "exact external command artifact missing")
    executable = command["argv"][0] if invoked_binary is None else invoked_binary
    require((Path(root) / executable).resolve() == paths["binary"], "command does not invoke exact sealed diagnostic binary")
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


def streamed_records(directory, descriptor, name, count, status="RECORDED"):
    """Validate one fixed-size JSONL record at a time, including the final hash.

    Callers must consume the iterator completely before recording consistency.
    The retained E04 job stream is never collected into a whole-stream list.
    """
    exact(descriptor, ("path", "status", "records", "bytes", "sha256"), name + " descriptor")
    require(descriptor["path"] == name and descriptor["status"] == status
            and type(descriptor["records"]) is int and descriptor["records"] == count
            and integer(count), "stream membership/cardinality/disposition mismatch: " + name)
    path = Path(directory) / name
    require(path.is_file() and not path.is_symlink(), "original stream missing/aliased: " + name)
    digest = hashlib.sha256()
    length = records = 0
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON record key")
            result[key] = value
        return result
    with path.open("rb") as source:
        for line in iter(lambda: source.readline(WINDOW + 1), b""):
            require(records < count and len(line) <= WINDOW and line.endswith(b"\n"),
                    "extra/truncated/oversized original record")
            digest.update(line)
            length += len(line)
            records += 1
            yield json.loads(line, object_pairs_hook=unique)
    require(records == count and integer(descriptor["bytes"]) and length == descriptor["bytes"]
            and descriptor["sha256"] == digest.hexdigest(), "stream bytes/hash/cardinality differ: " + name)


def original_job_work(value):
    """Completion JobWork owns a StatementWork total, never fourteen families."""
    exact(value, ("sql", "sql_scope", "statement_family_status", "payload", "allocation", "parked_turns", "queue_wait_ns", "service_ns"),
          "original Completion work")
    require(value["sql_scope"] == "original-job-statement-total"
            and value["statement_family_status"] == "UNAVAILABLE", "original job family/scope invented")
    sql = counter_map(value["sql"], STATEMENT, "original job statement total")
    require(sql["rows_changed"] >= sql["direct_rows_changed"]
            and sql["returned_value_bytes"] >= sql["returned_blob_bytes"], "original job SQL meaning mismatch")
    counter_map(value["payload"], PAYLOAD, "original job payload")
    counter_map(value["allocation"], ALLOCATION, "original job allocation")
    require(all(integer(value[key]) for key in ("parked_turns", "queue_wait_ns", "service_ns")),
            "original queue/parking/service observation unavailable")
    return value


def observed_owner_work(work):
    """Actual cumulative owner inventory, with current held-result gauges."""
    exact(work, ("scope", "credit_scope", "sql_foreground", "sql_maintenance", "payload_foreground",
        "payload_maintenance", "allocation_foreground", "allocation_maintenance", "counters", "completed",
        "queue_wait_ns", "service_ns"), "owner diagnostics work")
    require(work["scope"] == "same-owner-cumulative"
            and work["credit_scope"] == "queued-executing-caller-held-results", "owner counter scope substituted")
    for key in ("sql_foreground", "sql_maintenance"):
        database(work[key])
    for key in ("payload_foreground", "payload_maintenance"):
        counter_map(work[key], PAYLOAD, key)
    for key in ("allocation_foreground", "allocation_maintenance"):
        counter_map(work[key], ALLOCATION, key)
    counters = counter_map(work["counters"], OWNER, "owner counters")
    require(counters["peak_credited_bytes"] >= counters["credited_bytes"], "held credit exceeds observed owner peak")
    for key in ("completed", "queue_wait_ns", "service_ns"):
        require(isinstance(work[key], list) and len(work[key]) == 6
                and all(integer(value) for value in work[key]), "owner class counter inventory missing")
    return work


def owner_cumulative(before, after):
    """Do not interpret decreasing current credit/cache gauges as lost work."""
    observed_owner_work(before)
    observed_owner_work(after)
    for key in ("sql_foreground", "sql_maintenance", "payload_foreground", "payload_maintenance",
                "allocation_foreground", "allocation_maintenance", "completed", "queue_wait_ns", "service_ns"):
        monotonic(before[key], after[key])
    for key in OWNER:
        if key not in ("credited_bytes", "outstanding"):
            require(after["counters"][key] >= before["counters"][key], "owner cumulative counter decreased")


def empty_job_sums():
    return {"sql": dict.fromkeys(STATEMENT, 0), "payload": dict.fromkeys(PAYLOAD, 0),
            "allocation": dict.fromkeys(ALLOCATION, 0), "completed": [0] * 6,
            "queue_wait_ns": [0] * 6, "service_ns": [0] * 6, "admitted": 0}


def add_original_work(totals, work, service_class):
    original_job_work(work)
    require(type(service_class) is int and 0 <= service_class < 6, "original service class mismatch")
    for domain in ("sql", "payload", "allocation"):
        for key, value in work[domain].items():
            totals[domain][key] += value
            require(integer(totals[domain][key]), "original work sum overflow")
    totals["admitted"] += 1
    totals["completed"][service_class] += 1
    for key in ("queue_wait_ns", "service_ns"):
        totals[key][service_class] += work[key]


def foreground_equals(before, after, totals):
    owner_cumulative(before, after)
    for domain, key in (("sql", "sql_foreground"), ("payload", "payload_foreground"),
                        ("allocation", "allocation_foreground")):
        a = before[key]["total"] if domain == "sql" else before[key]
        b = after[key]["total"] if domain == "sql" else after[key]
        require(all(b[name] - a[name] == count for name, count in totals[domain].items()),
                "unreceipted/misattributed foreground " + domain)
    for key in ("completed", "queue_wait_ns", "service_ns"):
        require([b - a for a, b in zip(before[key], after[key])] == totals[key],
                "foreground class count/span differs from original Completions")
    require(after["counters"]["admitted"] - before["counters"]["admitted"] == totals["admitted"],
            "foreground admission differs from original command receipts")
    require(before["counters"]["credited_bytes"] == after["counters"]["credited_bytes"] == 0
            and before["counters"]["outstanding"] == after["counters"]["outstanding"] == 0,
            "foreground endpoint still owns original results")


def writes_invocation(manifest, directory, root, docker_mapping=None):
    """Bind the original eight arguments; external Docker evidence never rewrites them."""
    value = exact(manifest.get("invocation"), ("schema", "path_scope", "bound_before_startup", "argv",
        "identity_source_sha256", "identity_source_bytes", "identity_copy_sha256", "docker_path_mapping"), "original E04 invocation")
    v3 = manifest["schema"] == "cluster-two-job-receipts-v3"
    require(value["schema"] == ("cluster-two-e2-writes-invocation-v3" if v3 else "cluster-two-e2-writes-invocation-v1")
            and value["path_scope"] == "process-local-canonical-inputs" and value["bound_before_startup"] is True,
            "E04 inputs not bound before original startup")
    require(value["docker_path_mapping"] == {"status": "UNAVAILABLE", "reason": "no-verified-host-container-path-mapping"},
            "collector invented host/container mapping")
    argv = value["argv"]
    require(isinstance(argv, list) and len(argv) == (9 if v3 else 8) and all(isinstance(arg, str) and arg for arg in argv),
            "original E04 invocation cardinality missing")
    require(all(Path(argv[index]).is_absolute() for index in (0, 2, 3, 4, 5, 6, 7)), "original E04 input path is not absolute")
    command, _ = read_json(artifact(manifest["identity"].get("command_artifact"), root))
    require(command.get("argv") == argv, "E04 command differs from captured original invocation")
    native = None
    if manifest["identity"].get("execution_kind") == "docker":
        if manifest["schema"] in ("cluster-two-job-receipts-v2", "cluster-two-job-receipts-v3"):
            host, native = native_backing().mapping(docker_mapping, argv, command, root, manifest["identity"],
                version=3 if v3 else 2, original_inputs=manifest.get("pre_start_inputs"))
        else:
            host = observed_docker_mapping(docker_mapping, argv, command, root)
    else:
        require(docker_mapping is None, "standalone-host claimed unrelated Docker evidence")
        host = argv
    for index in ((0, 2, 3, 4, 6, 7) if native is not None else (0, 2, 3, 4, 5, 6, 7)):
        require(Path(host[index]) == Path(host[index]).resolve(), "mapped E04 host path is not canonical")
    if v3:
        require(native is not None and Path(host[8]) == Path(host[8]).resolve(), "E04 v3 control/native path observation missing")
    require(Path(host[6]) == Path(directory).resolve(), "E04 command targets another output artifact set")
    supplied, raw = read_json(host[7], 16_384)
    require(supplied == manifest["identity"] and value["identity_source_sha256"] == hash_bytes(raw)
            and value["identity_source_bytes"] == len(raw) and type(value["identity_source_bytes"]) is int
            and value["identity_copy_sha256"] == manifest["identity_sha256"]
            and hash_bytes(raw.strip()) == manifest["identity_sha256"], "E04 original identity input byte binding differs")
    return host, command, native


def native_backing():
    return evidence_backing.Validator(require, exact, artifact, read_json, integer)


def direct_docker_command(external, argv, image, cid_path, inspected, host_root):
    """Validate this v1 vehicle's direct, foreground docker run and original CID."""
    require(isinstance(external, list) and all(isinstance(item, str) for item in external)
            and external[:2] == ["docker", "run"], "Docker outer command is not the declared direct run")
    options, environment, volumes = {}, {}, []
    aliases = {"--cidfile": "cidfile", "--name": "name", "-v": "volume", "--volume": "volume",
               "-w": "workdir", "--workdir": "workdir", "-e": "env", "--env": "env"}
    index = 2
    while index < len(external) and external[index].startswith("-"):
        flag = external[index]
        if flag == "--rm":
            require("rm" not in options, "Docker repeated outer option")
            options["rm"] = True
            index += 1
            continue
        key, separator, value = flag.partition("=") if flag.startswith("--") else (flag, "", "")
        require(key in aliases, "Docker outer option is not registered for the direct vehicle")
        if not separator:
            index += 1
            require(index < len(external), "Docker outer option lacks its original value")
            value = external[index]
        require(bool(value), "Docker outer option value is empty")
        name = aliases[key]
        if name == "env":
            variable, equals, setting = value.partition("=")
            require(bool(equals) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", variable)
                    and variable not in environment, "Docker runtime environment is inherited or ambiguous")
            environment[variable] = setting
        elif name == "volume":
            volumes.append(value)
        else:
            require(name not in options, "Docker repeated outer option")
            options[name] = value
        index += 1
    require(external[index:] == [image, *argv], "Docker outer image/collector command differs from captured invocation")
    cid_argument = options.get("cidfile")
    require(isinstance(cid_argument, str) and Path(cid_argument).is_absolute()
            and Path(cid_argument) == Path(cid_argument).resolve() == cid_path,
            "Docker retained CID witness is not the original --cidfile")
    require(volumes in ([str(host_root) + ":/work"], [str(host_root) + ":/work:rw"]),
            "Docker original command does not declare the exact writable bind mount")
    config = inspected.get("Config")
    require(isinstance(config, dict) and "Entrypoint" in config and config["Entrypoint"] in (None, [])
            and config.get("Cmd") == argv and inspected.get("Path") == argv[0]
            and inspected.get("Args") == argv[1:],
            "Docker inspected effective command is not the original direct collector")
    if "workdir" in options:
        require(config.get("WorkingDir") == options["workdir"], "Docker inspected working directory differs")
    actual_environment = config.get("Env")
    require(isinstance(actual_environment, list)
            and all(isinstance(value, str) and "=" in value for value in actual_environment),
            "Docker inspected runtime environment unavailable")
    actual = {}
    for value in actual_environment:
        variable, _, setting = value.partition("=")
        require(variable not in actual, "Docker inspected runtime environment is ambiguous")
        actual[variable] = setting
    require(all(actual.get(variable) == setting for variable, setting in environment.items()),
            "Docker declared runtime environment differs from actual container")


def observed_docker_mapping(descriptor, argv, command, root):
    """Read a post-run mount attestation, not a guessed /work prefix replacement."""
    require(descriptor is not None, "Docker host/container input path mapping UNAVAILABLE")
    value, _ = read_json(artifact(descriptor, root))
    exact(value, ("schema", "status", "scope", "source", "container_id", "image_id", "host_root", "container_root",
                  "collector_argv", "host_argv", "artifacts"), "external Docker path mapping")
    require(value["schema"] == "cluster-two-e2-docker-path-mapping-v1" and value["status"] == "OBSERVED"
            and value["scope"] == "diagnostic-input-path-binding" and isinstance(value["source"], str) and value["source"],
            "Docker input mapping lacks observed source")
    require(isinstance(value["container_id"], str) and SHA.fullmatch(value["container_id"])
            and value["image_id"] == command.get("image_id") and value["collector_argv"] == argv,
            "Docker mapping belongs to another container/image/invocation")
    artifacts = exact(value["artifacts"], ("command_receipt", "inspect_receipt", "inspect_stdout", "cid_file"),
                      "Docker mapping artifact inventory")
    paths = {key: artifact(item, root) for key, item in artifacts.items()}
    outer, _ = read_json(paths["command_receipt"])
    inspected, _ = read_json(paths["inspect_receipt"])
    require(outer.get("command") == command.get("external_argv") and outer.get("exit_code") == 0
            and type(outer.get("exit_code")) is int and outer.get("timed_out") is False,
            "Docker mapping lacks successful exact executed outer command")
    require(inspected.get("command") == ["docker", "inspect", value["container_id"]]
            and inspected.get("exit_code") == 0 and type(inspected.get("exit_code")) is int
            and inspected.get("timed_out") is False and paths["inspect_stdout"] == paths["inspect_receipt"].with_suffix(".stdout"),
            "Docker mapping raw inspect is unrelated or failed")
    with paths["cid_file"].open("rb") as source:
        cid = source.read(129)
    require(len(cid) <= 128 and cid.strip().decode("ascii") == value["container_id"], "Docker CID file differs")
    inspect, _ = read_json(paths["inspect_stdout"])
    require(isinstance(inspect, list) and len(inspect) == 1 and isinstance(inspect[0], dict) and inspect[0].get("Id") == value["container_id"]
            and inspect[0].get("Image") == value["image_id"], "Docker actual inspect identity differs")
    host_root = Path(value["host_root"])
    container_root = Path(value["container_root"])
    require(host_root.is_absolute() and host_root == host_root.resolve() == Path(root).resolve()
            and container_root == Path("/work"), "Docker mapping root differs from owning source root")
    direct_docker_command(command["external_argv"], argv, value["image_id"], paths["cid_file"], inspect[0], host_root)
    mounts = inspect[0].get("Mounts")
    require(isinstance(mounts, list), "Docker inspected mounts unavailable")
    matches = [mount for mount in mounts if isinstance(mount, dict)
               and isinstance(mount.get("Destination"), str)
               and Path(mount["Destination"]).is_relative_to(container_root)]
    require(len(matches) == 1 and matches[0].get("Type") == "bind" and matches[0].get("Destination") == str(container_root)
            and matches[0].get("RW") is True and Path(matches[0].get("Source", "")) == host_root,
            "Docker inspected bind mount missing, different, read-only or overlaid")
    host = value["host_argv"]
    require(isinstance(host, list) and len(host) == 8 and all(isinstance(item, str) for item in host)
            and host[1] == argv[1], "Docker host argument inventory differs")
    for index in (0, 2, 3, 4, 5, 6, 7):
        local = Path(argv[index])
        require(local.is_relative_to(container_root) and Path(host[index]).is_absolute()
                and host_root / local.relative_to(container_root) == Path(host[index])
                and Path(host[index]) == Path(host[index]).resolve(), "Docker captured input does not match attested bind path")
    return host


def e04_publication(value, namespace):
    exact(value, ("namespace", "generation", "revision"), "original publication")
    require(value["namespace"] == namespace and all(integer(value[key]) and value[key] > 0
            for key in value), "publication belongs to another route or has invalid identity")
    return value


def e04_source(value, namespace, root):
    exact(value, ("namespace", "owner", "root"), "original base source")
    require(integer(value["namespace"]) and value["namespace"] == namespace and integer(value["owner"]) and value["owner"] > 0
            and value["root"] == root and SHA.fullmatch(root), "base source belongs to another route/root/owner")
    return value


def write_protocol(records, namespace, root, serial):
    """One fixed write's original source/Needs/publication/reply/release chain.

    This bounded state follows the ordinary by-serial Write route. Its single
    immutable inode need is source-derived, not a new operation-size allowance.
    No successful transition can substitute another source or publication.
    """
    current = None
    operation = -1
    stage = "released"
    source = publication = None
    needs = []
    last_publication = None
    publications = []
    previous_command_close = 0
    for record in records:
        index = record["operation_index"]
        require(type(index) is int and 0 <= index < E04_WRITES, "write operation identity missing")
        require(all(integer(record.get(key)) for key in ("opened_ns", "closed_ns"))
                and previous_command_close <= record["opened_ns"] <= record["closed_ns"], "write command original clock window missing")
        previous_command_close = record["closed_ns"]
        if index != operation:
            require(index == operation + 1 and stage == "released", "missing/reordered source/reply/release coverage")
            operation = index
            stage = "acquire"
            source = publication = None
            current = {"needs": None, "publication": None, "source": None,
                       "first_job_opened_ns": record["opened_ns"], "last_job_closed_ns": None}
        require(integer(record["route_ns"]) and record["route_ns"] == namespace, "write command belongs to another route")
        outcome = exact(record["original_result"], ("status", "response_kind", "error", "custody", "details"), "original write result")
        require(record["disposition"] == "completed" and outcome["status"] == "OK"
                and outcome["error"] is None and outcome["custody"] == "original-completion-retained-during-record",
                "original write command failed/unattempted; no success inferred")
        command = record["command_kind"]
        if command == "AcquireBaseSource":
            require(stage == "acquire" and record["class"] == 5 and record["source"] is None
                    and record["publication"] is None and outcome["response_kind"] == "BaseSource", "source acquisition coverage mismatch")
            source = e04_source(outcome["details"], namespace, root)
            current["source"] = source
            stage = "namespace"
        elif command == "Namespace":
            require(stage == "namespace" and record["class"] == 1 and record["source"] == source
                    and record["publication"] is None and outcome["response_kind"] == "Namespace", "Needs/publication source mismatch")
            detail = outcome["details"]
            require(isinstance(detail, dict), "original namespace result missing")
            if detail.get("outcome") == "Needs":
                exact(detail, ("outcome", "needs"), "original Needs")
                require(current["needs"] is None and detail["needs"] == [{"type": "inode", "serial": serial}]
                        and type(detail["needs"][0]["serial"]) is int,
                        "missing/duplicate/foreign immutable inode Needs")
                current["needs"] = record["external_job_id"]
                needs.append((index, record["external_job_id"], source))
            else:
                exact(detail, ("outcome", "publication", "inode"), "original applied publication")
                require(detail["outcome"] == "Applied", "frozen write did not publish its original mutation")
                publication = e04_publication(detail["publication"], namespace)
                if last_publication is not None:
                    require(publication["generation"] == last_publication["generation"]
                            and publication["revision"] > last_publication["revision"], "duplicate/reordered publication identity")
                last_publication = publication
                current["publication"] = publication
                inode = exact(detail["inode"], ("serial", "kind", "mode", "nlink", "size", "born", "entries",
                    "inherited_cutoff", "mtime_nanoseconds", "mtime_seconds"), "published inode")
                require(inode["serial"] == serial and inode["kind"] == 1 and inode["size"] == E04_BYTES,
                        "publication changed another inode/kind/logical size")
                require(all(integer(inode[key]) for key in inode if key != "mtime_seconds")
                        and type(inode["mtime_seconds"]) is int and -(1 << 63) <= inode["mtime_seconds"] < (1 << 63)
                        and inode["mtime_nanoseconds"] < 1_000_000_000, "published inode facts unavailable")
                stage = "reply"
        elif command == "ReplyAttempted":
            e04_publication(record["publication"], namespace)
            require(stage == "reply" and record["class"] == 3 and record["source"] is None
                    and record["publication"] == publication and outcome["response_kind"] == "Done"
                    and outcome["details"] is None, "missing/foreign publication ReplyAttempted")
            stage = "release"
        elif command == "ReleaseBaseSource":
            require(stage == "release" and record["class"] == 3 and record["source"] == source
                    and record["publication"] is None and outcome["response_kind"] == "Done"
                    and outcome["details"] is None, "missing/foreign original source release")
            stage = "released"
            current["last_job_closed_ns"] = record["closed_ns"]
            publications.append(current)
        else:
            raise ValueError("unregistered command substituted inside frozen write window")
    require(operation == E04_WRITES - 1 and stage == "released" and len(publications) == E04_WRITES,
            "missing original write/source/reply/release prefix")
    return publications, needs


def frozen_write_inputs(base_path, replacements_path):
    """Recompute expectations from closed independent files, never candidate bytes."""
    paths = (Path(base_path), Path(replacements_path))
    for path, size in zip(paths, (E04_BYTES, E04_WRITES * 4096)):
        require(path.is_file() and not path.is_symlink() and path.stat().st_size == size,
                "independent E04 input kind/length/alias differs")
    signatures = [(path.stat().st_dev, path.stat().st_ino, path.stat().st_size, path.stat().st_mtime_ns)
                  for path in paths]
    replacement_hash = hashlib.sha256()
    with paths[1].open("rb") as source:
        for index in range(E04_WRITES):
            value = source.read(4096)
            expected = b"".join(payload_chunks(1, index + 1, 4096))
            require(value == expected, "replacement input differs from frozen generator")
            replacement_hash.update(value)
        require(source.read(1) == b"", "replacement input has extra bytes")
    base_hash = hashlib.sha256()
    final_hash = hashlib.sha256()
    inverse = pow(104729, -1, E04_BYTES // 4096)
    with paths[0].open("rb") as base, paths[1].open("rb") as replacements:
        generated = iter(payload_chunks(1, 0, E04_BYTES))
        for cell in range(E04_BYTES // 4096):
            value = base.read(4096)
            expected = b"".join(next(generated) for _ in range(4096 // 32))
            require(value == expected, "dense initial input differs from frozen generator")
            base_hash.update(value)
            index = (cell * inverse) % (E04_BYTES // 4096)
            if index < E04_WRITES:
                replacements.seek(index * 4096)
                value = replacements.read(4096)
                require(len(value) == 4096, "replacement input ended during independent oracle")
            final_hash.update(value)
        require(base.read(1) == b"", "initial input has extra bytes")
    require(signatures == [(path.stat().st_dev, path.stat().st_ino, path.stat().st_size, path.stat().st_mtime_ns)
                           for path in paths], "independent input changed during verification")
    return {"base_sha256": base_hash.hexdigest(), "replacements_sha256": replacement_hash.hexdigest(),
            "expected_sha256": final_hash.hexdigest()}


def write_trace_consistency(records, publications):
    records = iter(records)
    require(len(publications) == E04_WRITES, "trace lacks complete original publication inventory")
    count = 0
    previous_close = 0
    for record, expected in zip(records, write_trace(E04_BYTES)):
        require(type(record.get("index")) is int and record["index"] == expected["index"]
                and record.get("record_index") == expected["index"]
                and all(record.get(key) == expected[key] and type(record.get(key)) is type(expected[key])
                        for key in ("offset", "length", "replacement_sha256")), "frozen write trace/index/input hash differs")
        require(record.get("outcome") == "Applied"
                and record.get("publication") == publications[count]["publication"], "trace substituted original publication/outcome")
        require(all(integer(record.get(key)) for key in ("opened_ns", "closed_ns"))
                and previous_close <= record["opened_ns"] <= publications[count]["first_job_opened_ns"]
                <= publications[count]["last_job_closed_ns"] <= record["closed_ns"], "write trace causal order differs")
        previous_close = record["closed_ns"]
        count += 1
    # Exhausting the stream also checks its original bytes/hash/cardinality.
    require(next(records, None) is None and count == E04_WRITES, "missing/extra frozen write trace")


E04_COMMON = {"schema", "case", "kind", "record_index", "record_id", "attempt_id", "owner_id", "mode",
    "sample_count", "admission_eligible", "qualification_status", "global_persistence", "source_binding",
    "build_target_os", "build_target_architecture", "identity", "identity_sha256", "actual_binary_sha256", "invocation"}
E04_FIELDS = {
    "manifest": {"driver_version", "e1_sample_status", "e1_sample_count", "E05_status", "observation_consistency",
        "write_window_consistency", "streams", "fixture_inputs", "oracle_source", "outcomes", "unavailable", "forbidden_substitutions",
        "unrun_writes", "database_retained"},
    "outcomes": {"status", "write_records", "complete_command_ns", "complete_command_status", "collector_span_ns",
        "original_error", "database_retained", "E05_status", "startup_status", "stop_status", "fixture", "oracle",
        "write_attempts", "database_artifact"},
    "startup": {"scope", "attempt_count", "clock", "opened_ns", "closed_ns", "owner_elapsed_ns", "configuration",
        "creation_reported", "original_outcome", "creation"},
    "owner-job": {"external_job_id", "phase", "operation_index", "command_kind", "class", "route_ns", "source",
        "publication", "opened_ns", "closed_ns", "span_scope", "disposition", "original_result", "work", "copy_scope"},
    "write-operation": {"public_attempt_id", "operation_index", "index", "serial", "source", "offset", "length",
        "replacement_sha256", "opened_ns", "closed_ns", "outcome", "publication", "boundary", "mtime_seconds", "mtime_nanoseconds"},
    "owner-diagnostics": {"endpoint", "at_ns", "status", "work"},
    "bootstrap-messages": {"binding_bytes", "policy_bytes", "binding_sha256", "policy_sha256", "scope"},
    "base-fact-interval": {"operation_index", "needs_job_index", "needs_external_job_id", "source", "needs_count", "status",
        "client_before", "client_after", "private_fact_provider_trace", "scope"},
    "independent-oracle": {"status", "scope", "bytes", "final_sha256", "expected_sha256", "file_mode", "nlink",
        "mtime_seconds", "mtime_nanoseconds", "old_file_content_root_unchanged", "binding_unchanged",
        "namespace_membership_checked", "root_serial", "namespace_entries", "binding_scope"},
    "native-fence": {"scope", "partial_messages", "live_messages", "credited_bytes", "id_copied_bytes",
        "send_framing_debug", "send_native_debug", "receive_native_debug", "receive_debug"},
    "final-binding": {"scope", "at_ns", "correlation", "complete", "bytes", "sha256", "body_hex", "context"},
    "application-disposal": {"control"},
}


def e04_envelope(record, manifest, kind, index):
    fields = E04_COMMON | E04_FIELDS[kind]
    if kind == "startup" and manifest.get("schema") in ("cluster-two-job-receipts-v2", "cluster-two-job-receipts-v3"):
        fields = fields | {"filesystem"}
    if manifest.get("schema") == "cluster-two-job-receipts-v3":
        fields |= {"manifest": {"pre_start_inputs"}, "outcomes": {"application_disposal"},
                   "native-fence": {"close_status", "control_acknowledged", "opened_ns", "closed_ns"}}.get(kind, set())
    else:
        require(kind not in ("final-binding", "application-disposal"), "v3 disposal observation mixed into earlier receipt")
    exact(record, fields, "original E04 " + kind)
    for key in E04_COMMON - {"kind", "record_index", "record_id", "global_persistence"}:
        require(record[key] == manifest[key], "mixed E04 record identity/source/arm/input: " + key)
    require(type(record["sample_count"]) is int and record["sample_count"] == 0
            and record["admission_eligible"] is False and record["qualification_status"] == "NOT_EVALUATED",
            "original E04 sample/admission scope substituted")
    require(record["kind"] == kind and type(record["record_index"]) is int and record["record_index"] == index
            and record["record_id"] == hash_bytes(f"{manifest['identity_sha256']}:{kind}:{index}".encode()),
            "duplicate/missing/mixed original E04 receipt identity")
    early = kind == "startup" or (kind == "owner-diagnostics" and record.get("endpoint") == "before-attach")
    require(record["global_persistence"] == (None if early else manifest["global_persistence"]),
            "local startup/global persistence availability was substituted")
    substitutions(record)


def original_e04_job(record, manifest, index):
    e04_envelope(record, manifest, "owner-job", index)
    require(record["external_job_id"] == hash_bytes(f"{manifest['identity_sha256']}:job:{index}".encode()),
            "external original command identity missing/duplicated")
    require(record["phase"] in ("setup", "writes", "verification", "cleanup")
            and type(record["class"]) is int and 0 <= record["class"] < 6
            and all(integer(record[key]) for key in ("opened_ns", "closed_ns"))
            and record["opened_ns"] <= record["closed_ns"], "original command phase/class/boundary missing")
    expected_span = "external-attach-inclusive-boundary-exact-Open-JobWork" if record["command_kind"] == "Open" and record["phase"] == "setup" else "external-command-admission-through-original-result-including-observer-wait"
    require(record["span_scope"] == expected_span
            and record["copy_scope"] == "UNAVAILABLE-private-input-and-exclusive-copy-accounting", "command span/copy scope substituted")
    value = exact(record["original_result"], ("status", "response_kind", "error", "custody", "details"), "original E04 result")
    require(record["disposition"] in ("completed", "unattempted", "pending_failure"), "original attempt disposition missing")
    if record["disposition"] == "completed":
        original_job_work(record["work"])
        require(value["custody"] == "original-completion-retained-during-record", "original Completion custody replaced")
        require(record["work"]["service_ns"] <= record["closed_ns"] - record["opened_ns"], "exclusive service span outside original command")
    else:
        require(record["work"] is None and value["custody"] == "original-unattempted-command-or-pending-retained",
                "unattempted/missing original Completion was fabricated")
    failed = value["status"] == "FAILED" or record["disposition"] != "completed"
    require(value["status"] in ("OK", "FAILED") and (isinstance(value["error"], str) and value["error"] if failed
            else value["error"] is None), "original failure text/result disposition missing")
    if not failed and value["response_kind"] == "Namespace":
        require(isinstance(value["details"], dict), "original Namespace outcome missing")
        failed = value["details"].get("outcome") == "Refused"
    if not failed:
        expected = {"Open": (3, "Opened"), "AcquireBaseSource": (5, "BaseSource"), "Namespace": (1, "Namespace"),
            "ReplyAttempted": (3, "Done"), "ReleaseBaseSource": (3, "Done"), "SourceInode": (0, "Inode"),
            "SourceDentry": (0, "Dentry"), "SourceNames": (0, "Names"), "SourceCell": (0, "Cell"),
            "SourceRead": (0, "Read"), "Close": (3, "Done"), "CleanupState": (3, "CleanupState")}
        require(record["command_kind"] in expected and (record["class"], value["response_kind"]) == expected[record["command_kind"]],
                "unregistered original command/class/response combination")
    return failed


def e04_fixture(manifest, host, root, directory):
    value = exact(manifest["fixture_inputs"], ("assignment", "acquisition", "base", "replacements", "seed", "base_identity", "writes"), "independent fixture inputs")
    require(all(type(value[key]) is int for key in ("seed", "base_identity", "writes"))
            and (value["seed"], value["base_identity"], value["writes"]) == (1, 0, E04_WRITES), "frozen dense fixture shape/identity differs")
    bundle, _ = read_json(artifact(manifest["identity"]["artifacts"]["fixture"], root))
    exact(bundle, ("schema", "case", "profile", "seed", "source_context", "inputs"), "prepared independent fixture bundle")
    require(bundle["schema"] == "cluster-two-e04-fixture-bundle-v1" and bundle["case"] == "E04-write-16m"
            and bundle["profile"] == manifest["global_persistence"] and type(bundle["seed"]) is int and bundle["seed"] == 1,
            "independent fixture bundle belongs to another case/profile/generator")
    context = exact(bundle["source_context"], ("source_commit", "source_tree", "source_sealed", "effective_root", "scope",
        "filesystem_profile", "root_serial"), "prepared source context")
    require(all(context[key] == manifest["identity"][key] for key in ("source_commit", "source_tree"))
            and context["source_sealed"] is False, "prepared fixture source context differs from diagnostic base/inventory")
    inputs = exact(bundle["inputs"], ("assignment", "acquisition", "base_input", "replacements"), "prepared input inventory")
    assignment = Path(host[2])
    acquisition = assignment.with_suffix(".fixture")
    for label, path, repeated, original in (("assignment", assignment, "assignment", manifest["invocation"]["argv"][2]),
        ("acquisition", acquisition, "acquisition", str(Path(manifest["invocation"]["argv"][2]).with_suffix(".fixture"))),
        ("base_input", Path(host[3]), "base", manifest["invocation"]["argv"][3]),
        ("replacements", Path(host[4]), "replacements", manifest["invocation"]["argv"][4])):
        descriptor = exact(inputs[label], ("path", "sha256", "bytes"), "prepared input " + label)
        recorded = exact(value[repeated], ("path", "sha256", "bytes"), "observed input " + label)
        require(artifact(descriptor, root) == path and integer(descriptor["bytes"])
                and descriptor["bytes"] == path.stat().st_size and recorded["sha256"] == descriptor["sha256"]
                and recorded["bytes"] == descriptor["bytes"] and type(recorded["bytes"]) is int and recorded["path"] == original,
                "observed input differs from independently sealed bundle: " + label)
    with assignment.open("rb") as source:
        raw = source.read(8193)
    require(len(raw) <= 8192 and hash_bytes(raw) == value["assignment"]["sha256"], "independent assignment hash/byte window differs")
    fields = raw.decode().splitlines()
    require(len(fields) == 20 and fields[0] == "layerfs-r4-functional-v1"
            and fields[18] == fields[19] == manifest["global_persistence"], "actual assigned global profile differs")
    require(SHA.fullmatch(fields[14]) and SHA.fullmatch(fields[15]) and SHA.fullmatch(fields[16]), "original immutable assignment root/context missing")
    require(context["effective_root"] == fields[14] and context["scope"] == fields[15]
            and context["filesystem_profile"] == fields[16] and integer(context["root_serial"])
            and fields[17] == str(context["root_serial"]), "prepared authenticated source assignment context changed")
    with acquisition.open("rb") as source:
        raw = source.read(4097)
    require(len(raw) <= 4096 and hash_bytes(raw) == value["acquisition"]["sha256"], "independent actual acquisition facts changed")
    lines = raw.decode().splitlines()
    require(len(lines) == 9 and lines[0] == "layerfs-e04-acquisition-v1" and lines[8] == "source_removed=true"
            and lines[1] == value["base"]["sha256"], "dense acquisition/source removal facts missing")
    require(all(re.fullmatch(r"[0-9]+", lines[index]) for index in (2, 4, 5, 6, 7))
            and re.fullmatch(r"-?[0-9]+", lines[3]), "actual acquisition metadata/capacity unavailable")
    numbers = [int(field) for field in lines[2:8]]
    require(numbers[1] >= -(1 << 63) and numbers[1] < (1 << 63) and numbers[2] < 1_000_000_000
            and numbers[3] == 1 and numbers[4] >= E04_BYTES and numbers[5] == E04_BYTES,
            "native dense source/metadata/explicit copied bytes differ")
    expected = frozen_write_inputs(host[3], host[4])
    require(expected["base_sha256"] == value["base"]["sha256"]
            and expected["replacements_sha256"] == value["replacements"]["sha256"], "closed independent fixture hash differs")
    product, _ = read_json(artifact(manifest["identity"]["artifacts"]["product"], root), 1 << 20)
    oracle = artifact(manifest["identity"]["artifacts"]["observer"], root)
    require(oracle.name == "oracle.rs" and oracle.parent.name == "e2_writes"
            and any(artifact(item, root) == oracle for item in product["sources"]), "independent oracle source absent from declared inventory")
    copied = exact(manifest["oracle_source"], ("path", "bytes", "sha256", "source_path"), "retained independent oracle source")
    require(copied["path"] == "oracle-source.rs" and (Path(root) / copied["source_path"]).resolve() == oracle
            and copied["sha256"] == manifest["identity"]["artifacts"]["observer"]["sha256"]
            and integer(copied["bytes"]) and copied["bytes"] == oracle.stat().st_size,
            "retained oracle source differs from independent artifact")
    copy_path = Path(directory) / copied["path"]
    require(copy_path.is_file() and not copy_path.is_symlink()
            and artifact({"path": str(copy_path.resolve()), "sha256": copied["sha256"]}, root) == copy_path.resolve()
            and copy_path.stat().st_size == copied["bytes"], "retained independent oracle source copy changed")
    return fields[14], {"mode": numbers[0], "nlink": numbers[3], "root_serial": context["root_serial"],
        "mtime_seconds": numbers[1], "mtime_nanoseconds": numbers[2], "source_allocated_bytes": numbers[4]}, expected


def e04_response(value):
    """Exact registered response inventory, without interpreting error strings."""
    kind, detail = value["response_kind"], value["details"]
    fields = {"Opened": ("namespace",), "BaseSource": ("namespace", "owner", "root"),
        "Dentry": ("present",), "Names": ("active_rows", "captured_rows"), "Cell": ("present",),
        "CleanupState": ("state",), "MaintenanceIdle": ("idle",)}
    if kind == "Done":
        require(detail is None, "Done substituted a result")
    elif kind in fields:
        exact(detail, fields[kind], "original response " + kind)
        if kind in ("Dentry", "Cell"):
            require(type(detail["present"]) is bool, "original presence was unavailable/nonboolean")
        elif kind == "Names":
            require(all(integer(detail[key]) for key in detail), "original name counts unavailable")
    elif kind == "Inode":
        if detail is not None:
            exact(detail, ("serial", "kind", "mode", "nlink", "size", "born", "entries", "inherited_cutoff",
                           "mtime_nanoseconds", "mtime_seconds"), "original inode response")
    elif kind == "Read":
        require(isinstance(detail, dict) and type(detail.get("present")) is bool, "original read presence missing")
        fields = ("present", "data_bytes", "data_capacity", "inherited_bytes", "inherited_capacity") if detail["present"] else ("present",)
        exact(detail, fields, "original read response")
        if detail["present"]:
            require(all(integer(detail[key]) for key in fields if key != "present")
                    and detail["data_bytes"] <= detail["data_capacity"] <= 65_536
                    and detail["inherited_bytes"] <= detail["inherited_capacity"] <= 8192, "returned read allocation outside source window")
    elif kind == "Namespace":
        require(isinstance(detail, dict), "original namespace response missing")
        variants = {"Needs": ("outcome", "needs"), "Applied": ("outcome", "publication", "inode"),
                    "Unchanged": ("outcome", "inode"), "Refused": ("outcome", "refusal")}
        require(detail.get("outcome") in variants, "unregistered namespace outcome")
        exact(detail, variants[detail["outcome"]], "original Namespace outcome")
    else:
        raise ValueError("unregistered response claimed inside E04")


def e04_write_payload(applied, payload_model=E04_PAYLOAD_MODEL):
    """Expected caller work, selected by the frozen source contract, not counters.

    CELL_BYTES remains 4096. payload/stream.rs splits whole writes on 32 KiB
    run boundaries, counts covered 4 KiB cells, and binds the caller's slice
    directly in write_whole; only write_part copies a merge buffer. Each frozen
    E04 write is aligned and exactly one cell: (4096, 1, 0, 0). Pre-R7 source
    copied that cell; retained validations must explicitly select that contract.
    """
    require(payload_model in (E04_PAYLOAD_MODEL, E04_LEGACY_PAYLOAD_MODEL),
            "unregistered E04 payload source contract")
    copied = 4096 if payload_model == E04_LEGACY_PAYLOAD_MODEL else 0
    return (4096, 1, 0, copied) if applied else (0, 0, 0, 0)


def validate_e04(directory, root=ROOT, docker_mapping=None, *, payload_model=E04_LEGACY_PAYLOAD_MODEL):
    """Validate the frozen diagnostic write window; required E2 gaps stay open."""
    result = {"schema": "cluster-two-e2-observation-consistency-v1", "case": "E04-write-16m",
        "observation_consistency": "INCOMPLETE", "write_window_consistency": "INCOMPLETE",
        "qualification_status": "NOT_EVALUATED", "admission_eligible": False, "e1_sample_status": "NOT_RUN",
        "e1_sample_count": 0, "E05_status": "NOT_RUN", "payload_model": payload_model,
        "unavailable": list(E04_GAPS), "errors": []}
    failed = False
    try:
        e04_write_payload(False, payload_model)
        manifest, _ = read_json(Path(directory) / "manifest.json")
        if manifest.get("schema") == "cluster-two-job-receipts-v3":
            result["application_disposal_consistency"] = "INCOMPLETE"
        require(manifest.get("schema") in E04_VERSIONS and manifest.get("case") == "E04-write-16m"
                and manifest.get("mode") == "diagnostic" and manifest.get("sample_count") == 0 and type(manifest.get("sample_count")) is int
                and manifest.get("admission_eligible") is False and manifest.get("qualification_status") == "NOT_EVALUATED"
                and manifest.get("e1_sample_status") == "NOT_RUN" and manifest.get("e1_sample_count") == 0
                and type(manifest.get("e1_sample_count")) is int and manifest.get("E05_status") == "NOT_RUN"
                and manifest.get("observation_consistency") == "INCOMPLETE"
                and manifest.get("write_window_consistency") == "NOT_EVALUATED", "E04 diagnostic claimed another route/sample/qualification")
        e04_envelope(manifest, manifest, "manifest", 0)
        require(manifest["driver_version"] == E04_VERSIONS[manifest["schema"]] and manifest["unavailable"] == list(E04_GAPS)
                and manifest["database_retained"] is True and manifest["source_binding"] == "base-commit-plus-current-inventory",
                "E04 version/unavailable/source/custody scope differs")
        exact(manifest["forbidden_substitutions"], ("phase_resident_bytes", "eligible_debt_peak_bytes", "queue_peak_jobs", "whole_operation_copy_bytes"),
              "unavailable E04 substitution inventory")
        outcome_desc = exact(manifest["outcomes"], ("path", "bytes", "sha256"), "E04 outcomes descriptor")
        require(outcome_desc["path"] == "outcomes.json", "original E04 outcomes path differs")
        outcomes, raw = read_json(Path(directory) / "outcomes.json")
        require(outcome_desc["sha256"] == hash_bytes(raw) and outcome_desc["bytes"] == len(raw)
                and type(outcome_desc["bytes"]) is int, "original E04 outcome bytes changed")
        e04_envelope(outcomes, manifest, "outcomes", 0)
        failed = outcomes["status"] == "FAILED"
        require(outcomes["status"] in ("FAILED", "RECORDED"), "original E04 disposition missing")
        if failed:
            require(isinstance(outcomes["original_error"], str) and outcomes["original_error"], "original E04 failure missing")
            result["errors"].append(outcomes["original_error"])
        host, command, native = writes_invocation(manifest, directory, root, docker_mapping)
        v3 = manifest["schema"] == "cluster-two-job-receipts-v3"
        binary, _ = identity(manifest["identity"], root, 9 if v3 else 8, host[0])
        require(binary == manifest["actual_binary_sha256"] and manifest["build_target_os"] == manifest["identity"]["os"]
                and manifest["build_target_architecture"] == manifest["identity"]["architecture"], "actual E04 binary/build topology differs")
        supplied, copied = read_json(Path(directory) / "identity.json", 16_384)
        require(supplied == manifest["identity"] and hash_bytes(copied.strip()) == manifest["identity_sha256"]
                and copied.strip() == Path(host[7]).read_bytes().strip(), "E04 supplied identity copy byte binding differs")
        require(manifest["attempt_id"] == hash_bytes(f"{manifest['identity_sha256']}:attempt:0".encode())
                and manifest["owner_id"] == hash_bytes(f"{manifest['identity_sha256']}:owner:0".encode()), "original E04 attempt/owner identity differs")
        require(outcomes["complete_command_ns"] is None
                and outcomes["complete_command_status"] == "UNAVAILABLE-external-command-wall-required"
                and integer(outcomes["collector_span_ns"]) and outcomes["database_retained"] is True
                and outcomes["E05_status"] == "NOT_RUN", "E04 prefix renamed complete command or custody omitted")
        streams = manifest["streams"]
        names = ["startup.jsonl", "jobs.jsonl", "probes.jsonl", "trace.jsonl", "stdout.txt", "stderr.txt"]
        require(isinstance(streams, list) and len(streams) == len(names)
                and [item.get("path") for item in streams] == names, "E04 stream inventory missing/duplicated/reordered")
        for item in streams:
            exact(item, ("path", "sha256", "bytes", "records", "status"), "E04 stream descriptor")
            require(integer(item.get("records")) and item.get("status") == ("RECORDED" if item["records"] else "NOT_RUN-or-empty"),
                    "E04 actual stream count/disposition differs")
        started = stream(directory, streams[0], names[0], 1, "RECORDED", True)[0]
        e04_envelope(started, manifest, "startup", 0)
        work = startup(started)
        if manifest["schema"] in ("cluster-two-job-receipts-v2", "cluster-two-job-receipts-v3"):
            e04_filesystem(started)
        original_startup = exact(started["original_outcome"], ("status", "error", "profile"), "original E04 startup")
        require(original_startup["status"] == outcomes["startup_status"], "original E04 startup result replaced")
        if original_startup["status"] != "READY":
            failed = True
            require(original_startup["status"] == "FAILED" and outcomes["stop_status"] == "NOT_RUN"
                    and outcomes["original_error"] == original_startup["error"], "failed E04 startup error/Stop was replaced")
            raise ValueError("original E04 startup failed; no write result is inferred")
        require(work is not None, "original E04 startup observations unavailable")
        if manifest["schema"] in ("cluster-two-job-receipts-v2", "cluster-two-job-receipts-v3"):
            require(native is not None, "E04 v2 successful window requires original native backing custody")
            require(native["consumer_exit_code"] == 0, "successful E04 window has failed original consumer exit")
            require(started["filesystem"]["linux_filesystem_type"] == native["filesystem_magic"],
                    "original startup filesystem differs from observed native backing")
        ready_profile(started)
        require(work["calls"] == {"file_create_calls": 1, "sqlite_open_calls": 1,
                "connection_configuration_calls": 2, "cache_configuration_calls": 1}
                and work["sql"]["total"]["attempts"] > 0 and work["sql"]["total"]["executions"] > 0
                and work["allocation"]["attempts"] == work["allocation"]["admitted_jobs"] == 1
                and work["allocation"]["refusals"] == 0 and work["allocation"]["requested_bytes"] >= 268_435_456
                and work["allocation_state"]["work"] == work["allocation"], "original startup file/configuration/reservation work missing")
        # A failed prefix stays failed before any full-window accounting promise.
        if failed:
            raise ValueError("original E04 route failed; retained prefix cannot qualify a completed window")
        require(outcomes["stop_status"] == "STOPPED" and outcomes["original_error"] is None
                and outcomes["write_records"] == E04_WRITES and type(outcomes["write_records"]) is int
                and outcomes["write_attempts"] == E04_WRITES and type(outcomes["write_attempts"]) is int
                and manifest["unrun_writes"] == {"from_index": E04_WRITES, "to_exclusive": E04_WRITES}, "E04 failed/unrun writes or Stop hidden")
        require(manifest["global_persistence"] in ("durable", "disposable"), "actual global profile unavailable")
        base_root, metadata, expected = e04_fixture(manifest, host, root, directory)
        fixture = exact(outcomes["fixture"], ("route_namespace", "serial", "base_root", "file_root", "mode", "mtime_seconds",
            "mtime_nanoseconds", "nlink", "logical_len", "acquisition"), "actual authenticated E04 fixture context")
        namespace, serial = fixture["route_namespace"], fixture["serial"]
        require(integer(namespace) and namespace > 0 and integer(serial) and serial > 0
                and fixture["base_root"] == base_root and isinstance(fixture["file_root"], str)
                and SHA.fullmatch(fixture["file_root"]) and fixture["logical_len"] == E04_BYTES
                and all(fixture[key] == metadata[key] and type(fixture[key]) is int
                        for key in ("mode", "mtime_seconds", "mtime_nanoseconds", "nlink")), "actual original route/file binding unavailable")
        acquired = exact(fixture["acquisition"], ("base_sha256", "source_allocated_bytes", "source_copied_bytes", "source_removed"), "actual independent acquisition")
        require(acquired["base_sha256"] == expected["base_sha256"] and acquired["source_removed"] is True
                and acquired["source_allocated_bytes"] == metadata["source_allocated_bytes"]
                and integer(acquired["source_allocated_bytes"]) and acquired["source_copied_bytes"] == E04_BYTES
                and type(acquired["source_copied_bytes"]) is int, "actual acquisition/copy facts differ from prepared witness")
        endpoints, intervals = {}, {}
        oracle = fence = bootstrap = None
        final_binding = disposal = None
        for index, probe in enumerate(streamed_records(directory, streams[2], names[2], streams[2]["records"])):
            kind = probe.get("kind")
            allowed = ("owner-diagnostics", "base-fact-interval", "independent-oracle", "native-fence", "bootstrap-messages")
            require(kind in allowed + (("final-binding", "application-disposal") if v3 else ()),
                    "unregistered E04 probe")
            e04_envelope(probe, manifest, kind, index)
            if kind == "owner-diagnostics":
                require(probe["endpoint"] in ("before-attach", "before-writes", "after-writes", "pre-stop")
                        and probe["endpoint"] not in endpoints and probe["status"] == "OBSERVED"
                        and integer(probe["at_ns"]), "missing/duplicate/unavailable owner endpoint")
                observed_owner_work(probe["work"])
                endpoints[probe["endpoint"]] = probe
            elif kind == "base-fact-interval":
                key = probe["needs_external_job_id"]
                require(key not in intervals and probe["status"] == "SUPPLY_RETURNED" and probe["needs_count"] == 1
                        and type(probe["needs_count"]) is int and type(probe["needs_job_index"]) is int
                        and key == hash_bytes(f"{manifest['identity_sha256']}:job:{probe['needs_job_index']}".encode())
                        and probe["scope"] == "actual-shared-client-counters-between-Needs-and-next-round-not-per-id-provider-trace"
                        and probe["private_fact_provider_trace"] == "UNAVAILABLE", "original base-fact interval missing/failure/scope substitution")
                for label in ("client_before", "client_after"):
                    counter_map(probe[label], CLIENT, "actual shared client counters")
                for key_name in CLIENT[:5]:
                    require(probe["client_after"][key_name] >= probe["client_before"][key_name], "shared client cumulative work decreased")
                intervals[key] = probe
            elif kind == "independent-oracle":
                require(oracle is None, "duplicate independent full oracle")
                oracle = probe
            elif kind == "native-fence":
                require(fence is None, "duplicate original native fence")
                fence = probe
            elif kind == "final-binding":
                require(final_binding is None, "duplicate original final Binding")
                final_binding = probe
            elif kind == "application-disposal":
                require(disposal is None, "duplicate original application disposal")
                disposal = probe
            else:
                require(bootstrap is None and probe["scope"] == "original-credited-Binding-Policy-replies-retained-through-record"
                        and all(integer(probe[key]) and probe[key] > 0 for key in ("binding_bytes", "policy_bytes"))
                        and all(isinstance(probe[key], str) and SHA.fullmatch(probe[key]) for key in ("binding_sha256", "policy_sha256")),
                        "original bootstrap Messages missing/substituted")
                bootstrap = probe
        require(set(endpoints) == {"before-attach", "before-writes", "after-writes", "pre-stop"}, "owner accounting endpoint gap")
        labels = ("before-attach", "before-writes", "after-writes", "pre-stop")
        times = [endpoints[key]["at_ns"] for key in labels]
        require(started["closed_ns"] <= times[0] <= times[1] <= times[2] <= times[3] <= outcomes["collector_span_ns"],
                "owner endpoints substituted startup/phase/lifetime boundaries")
        totals = {key: empty_job_sums() for key in ("setup", "writes", "post-writes")}
        state = {"phase": 0, "opens": 0, "close": 0, "gone": 0, "verification_source": None,
                 "verification_acquired": 0, "verification_released": 0, "close_ns": None, "gone_ns": None}
        def observed_writes():
            nonlocal failed
            for index, record in enumerate(streamed_records(directory, streams[1], names[1], streams[1]["records"])):
                original_failed = original_e04_job(record, manifest, index)
                if original_failed:
                    failed = True
                    raise ValueError("original command failure/refusal retained; no success accounting")
                e04_response(record["original_result"])
                phase = ("setup", "writes", "verification", "cleanup").index(record["phase"])
                require(phase >= state["phase"], "original command phase order retreat/replay")
                state["phase"] = phase
                group = record["phase"] if phase < 2 else "post-writes"
                add_original_work(totals[group], record["work"], record["class"])
                low, high = (times[0], times[1]) if phase == 0 else ((times[1], times[2]) if phase == 1 else (times[2], times[3]))
                require(low <= record["opened_ns"] <= record["closed_ns"] <= high, "original job crosses declared accounting endpoint")
                if phase == 1:
                    applied = record["command_kind"] == "Namespace" and record["original_result"]["details"].get("outcome") == "Applied"
                    expected_payload = e04_write_payload(applied, payload_model)
                    require(tuple(record["work"]["payload"][key] for key in ("write_input_bytes", "write_cells", "partial_write_cells", "cell_copy_bytes"))
                            == expected_payload, "original aligned write payload differs from frozen caller bytes/cell work")
                    yield record
                else:
                    require(record["operation_index"] is None, "outside-write job claimed another operation")
                    require(record["route_ns"] is None if phase == 0 else record["route_ns"] == namespace,
                            "original Open input route must be absent" if phase == 0 else "outside-write job claimed another route")
                    if phase == 0:
                        require(record["command_kind"] == "Open" and record["class"] == 3
                                and record["original_result"]["response_kind"] == "Opened"
                                and record["original_result"]["details"] == {"namespace": namespace}, "bootstrap Open original receipt missing")
                        state["opens"] += 1
                    elif phase == 2:
                        if record["command_kind"] == "AcquireBaseSource":
                            require(state["verification_acquired"] == 0 and record["source"] is None, "verification source duplicate/substituted")
                            state["verification_source"] = e04_source(record["original_result"]["details"], namespace, base_root)
                            state["verification_acquired"] = 1
                        else:
                            require(state["verification_acquired"] == 1 and state["verification_released"] == 0
                                    and record["source"] == state["verification_source"], "verification read/release source mismatch")
                            if record["command_kind"] == "ReleaseBaseSource":
                                state["verification_released"] = 1
                            else:
                                require(record["command_kind"] in ("SourceInode", "SourceDentry", "SourceNames", "SourceCell", "SourceRead"),
                                        "unregistered verification command")
                    elif record["command_kind"] == "Close":
                        require(phase == 3 and state["close"] == 0 and record["original_result"]["response_kind"] == "Done", "local Close replay/result mismatch")
                        state["close"] = 1
                        state["close_ns"] = record["closed_ns"]
                    elif record["command_kind"] == "CleanupState":
                        require(phase == 3 and state["close"] == 1 and state["gone"] == 0
                                and record["original_result"]["response_kind"] == "CleanupState"
                                and record["original_result"]["details"] == {"state": "Gone"}, "automatic cleanup outcome missing/substituted")
                        state["gone"] = 1
                        state["gone_ns"] = record["closed_ns"]
        publications, needs = write_protocol(observed_writes(), namespace, base_root, serial)
        require(state["opens"] == state["close"] == state["gone"] == 1, "missing/duplicate Open/Close/Gone original lifecycle")
        require(state["verification_acquired"] == state["verification_released"] == 1, "verification source release coverage missing")
        for a, b, group in ((labels[0], labels[1], "setup"), (labels[1], labels[2], "writes"), (labels[2], labels[3], "post-writes")):
            foreground_equals(endpoints[a]["work"], endpoints[b]["work"], totals[group])
        require(len(intervals) == len(needs), "missing/extra original Needs/base interval")
        for index, job_id, source in needs:
            require(job_id in intervals and intervals[job_id]["operation_index"] == index and intervals[job_id]["source"] == source,
                    "base interval belongs to another Needs/source/operation")
        def traces():
            for index, record in enumerate(streamed_records(directory, streams[3], names[3], E04_WRITES)):
                e04_envelope(record, manifest, "write-operation", index)
                require(record["public_attempt_id"] == hash_bytes(f"{manifest['identity_sha256']}:public-write:{index}".encode())
                        and record["operation_index"] == index and type(record["operation_index"]) is int
                        and record["serial"] == serial and record["source"] == publications[index]["source"]
                        and record["boundary"] == "caller-input-through-write-reply-attempt-and-source-release"
                        and record["mtime_seconds"] == -7 and type(record["mtime_seconds"]) is int
                        and record["mtime_nanoseconds"] == 42 and type(record["mtime_nanoseconds"]) is int,
                        "original public write/source/time boundary differs")
                yield record
        write_trace_consistency(traces(), publications)
        require(oracle is not None and bootstrap is not None and fence is not None, "missing original oracle/bootstrap/fence record")
        require(oracle["status"] == "PASS" and oracle["scope"] == "separate-in-process-independent-full-state-algorithm-not-separate-process-or-performance-admission"
                and oracle["bytes"] == E04_BYTES and type(oracle["bytes"]) is int
                and oracle["expected_sha256"] == oracle["final_sha256"] == expected["expected_sha256"]
                and oracle["file_mode"] == metadata["mode"] and oracle["nlink"] == metadata["nlink"]
                and oracle["mtime_seconds"] == -7 and oracle["mtime_nanoseconds"] == 42
                and all(type(oracle[key]) is int for key in ("file_mode", "nlink", "mtime_seconds", "mtime_nanoseconds"))
                and oracle["old_file_content_root_unchanged"] is True and oracle["binding_unchanged"] is True
                and oracle["namespace_membership_checked"] is True and oracle["root_serial"] == metadata["root_serial"]
                and type(oracle["root_serial"]) is int and oracle["namespace_entries"] == 1
                and type(oracle["namespace_entries"]) is int
                and oracle["binding_scope"] == "captured-runtime-binding-not-current-history-query", "independent full-state oracle failed/substituted")
        summary = exact(outcomes["oracle"], ("status", "bytes", "final_sha256", "expected_sha256", "file_mode", "nlink", "mtime_seconds",
            "mtime_nanoseconds", "old_file_content_root_unchanged", "binding_unchanged", "namespace_membership_checked", "root_serial",
            "namespace_entries", "binding_scope", "eof_checked"), "retained independent oracle summary")
        require(all(summary[key] == oracle[key] and type(summary[key]) is type(oracle[key]) for key in summary if key in oracle)
                and summary["binding_scope"] == "captured-runtime-binding-not-current-history-query" and summary["eof_checked"] is True,
                "retained oracle summary replaced original result/scope/EOF")
        require(fence["scope"] == "attachment-lifetime-framing-native-reassembly-not-exclusive-physical-io"
                and all(type(fence[key]) is int and fence[key] == 0 for key in ("partial_messages", "live_messages", "credited_bytes"))
                and integer(fence["id_copied_bytes"]) and all(isinstance(fence[key], str) and fence[key]
                    for key in ("send_framing_debug", "send_native_debug", "receive_native_debug", "receive_debug")), "original consumer fence/custody missing")
        if v3:
            require(final_binding is not None and disposal is not None, "missing original final Binding/application disposal")
            evidence_disposal.Validator(require, exact, artifact, read_json, integer).validate(
                manifest, outcomes, final_binding, disposal, fence, native, host, root,
                state["close_ns"], state["gone_ns"], times[2], times[3])
            result["application_disposal_consistency"] = "PASS"
        state = exact(outcomes["database_artifact"], ("scope", "status", "path", "logical_bytes", "allocated_bytes", "device", "inode", "links"), "retained E04 database artifact")
        require(state["scope"] == "separate-post-stop-or-original-failure-artifact-stat" and state["status"] == "OBSERVED"
                and Path(state["path"]) == Path(manifest["invocation"]["argv"][5]), "original retained DB input/artifact binding differs")
        if native is not None:
            native_backing().original_artifact(state, native)
            result["cross_domain_physical_stat_comparison"] = "NOT_APPLICABLE-original-guest-only-no-host-equivalence"
        else:
            stat = Path(host[5]).lstat()
            require(Path(host[5]).is_file() and not Path(host[5]).is_symlink()
                and all(integer(state[key]) for key in ("logical_bytes", "allocated_bytes", "device", "inode", "links"))
                and (stat.st_size, stat.st_nlink) == (state["logical_bytes"], state["links"])
                and state["links"] == 1, "retained original E04 database identity/allocation changed")
            if manifest["identity"]["execution_kind"] == "standalone-host":
                require((stat.st_blocks * 512, stat.st_dev, stat.st_ino)
                    == tuple(state[key] for key in ("allocated_bytes", "device", "inode")), "retained same-domain database physical identity changed")
            else:
                result["cross_domain_physical_stat_comparison"] = "UNAVAILABLE-bind-path-does-not-prove-guest-host-stat-equivalence"
        for item, name in zip(streams[4:], names[4:]):
            stream(directory, item, name, item["records"], item["status"], False)
        result["write_window_consistency"] = "PASS"
    except (ValueError, OSError, KeyError, TypeError, StopIteration, json.JSONDecodeError) as error:
        result["errors"].append(str(error))
    if failed:
        result["observation_consistency"] = result["write_window_consistency"] = "FAIL"
        if "application_disposal_consistency" in result:
            result["application_disposal_consistency"] = "FAIL"
    return result


def e04_filesystem(record):
    """E04 v2 alone records the actual descriptor probe and typed refusal."""
    observed = exact(record.get("filesystem"), ("scope", "status", "filesystem_open_calls", "filesystem_identity_calls", "filesystem_probe_calls",
        "linux_filesystem_type", "original_refusal"), "original E04 filesystem observation")
    require(observed["scope"] == "verified-reopened-file-descriptor-before-reservation-and-sqlite",
            "filesystem observation substituted another path or phase")
    count, magic = observed["filesystem_probe_calls"], observed["linux_filesystem_type"]
    opened, identities = observed["filesystem_open_calls"], observed["filesystem_identity_calls"]
    if not record["creation_reported"]:
        require(observed["status"] == "UNAVAILABLE" and opened is None and identities is None and count is None and magic is None
                and observed["original_refusal"] is None, "unreported filesystem work fabricated")
        return
    require(integer(opened) and opened <= 1 and integer(identities) and identities <= 2
            and integer(count) and count <= 1, "filesystem verification/probe missing or replayed")
    require(magic is None or (type(magic) is int and -(1 << 63) <= magic < (1 << 63)),
            "Linux filesystem type is not the actual signed observation")
    require(observed["status"] == ("UNAVAILABLE" if magic is None else "OBSERVED")
            and (magic is None or count == 1), "failed/missing filesystem probe fabricated a type")
    work = record["creation"]["work"]
    require(opened <= work["calls"]["file_create_calls"] and (identities == 0 or opened == 1)
            and (count == 0 or identities == 2), "filesystem probe precedes original reopen and identity verification")
    refusal = observed["original_refusal"]
    require(magic != 0x6A656A63 or refusal is not None, "observed unsupported filesystem lost its typed refusal")
    if record["build_target_os"] == "linux" and magic is None:
        require(all(work["calls"][key] == 0 for key in ("sqlite_open_calls", "connection_configuration_calls", "cache_configuration_calls"))
                and all(value == 0 for value in work["sql"]["total"].values())
                and all(value == 0 for value in work["allocation"].values()),
                "unavailable Linux filesystem verification performed reservation or SQLite work")
    if refusal is not None:
        exact(refusal, ("kind", "linux_magic"), "original filesystem refusal")
        require(refusal == {"kind": "UnsupportedFilesystem", "linux_magic": 0x6A656A63}
                and magic == refusal["linux_magic"] and (opened, identities, count) == (1, 2, 1)
                and record["original_outcome"]["status"] == "FAILED"
                and record["original_outcome"]["error"] == f"Overlay(UnsupportedFilesystem {{ linux_magic: {magic} }})",
                "typed original filesystem refusal replaced")
        require(work["calls"] == {"file_create_calls": 1, "sqlite_open_calls": 0,
                    "connection_configuration_calls": 0, "cache_configuration_calls": 0}
                and all(value == 0 for value in work["sql"]["total"].values())
                and all(value == 0 for value in work["allocation"].values())
                and work["allocation_state"]["status"] == "UNAVAILABLE",
                "unsupported filesystem performed reservation or SQLite work")
    if record["original_outcome"]["status"] == "READY":
        require(record["build_target_os"] == "linux" and (opened, identities, count) == (1, 2, 1) and magic is not None
                and magic != 0x6A656A63 and refusal is None,
                "ready E04 startup lacks supported original filesystem observation")


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


def validate(directory, root=ROOT, docker_mapping=None, *, payload_model=E04_LEGACY_PAYLOAD_MODEL):
    """Read exact retained files. Never invoke a driver, sample or modify evidence."""
    try:
        selected, _ = read_json(Path(directory) / "manifest.json")
        if selected.get("case") == "E04-write-16m":
            return validate_e04(directory, root, docker_mapping, payload_model=payload_model)
        if selected.get("case") == "E05-write-1g":
            return {"schema": "cluster-two-e2-observation-consistency-v1", "case": "E05-write-1g",
                "observation_consistency": "INCOMPLETE", "write_window_consistency": "NOT_RUN",
                "qualification_status": "NOT_EVALUATED", "admission_eligible": False,
                "e1_sample_status": "NOT_RUN", "e1_sample_count": 0, "E05_status": "NOT_RUN",
                "unavailable": ["E05-executing-route-not-implemented"], "errors": ["E05 is not this frozen E04 diagnostic route"]}
    except (ValueError, OSError, TypeError, json.JSONDecodeError):
        pass  # Existing E01 error formatting below retains malformed/missing input.
    result = {"schema": "cluster-two-e2-observation-consistency-v1", "case": "E01-startup",
              "observation_consistency": "INCOMPLETE", "qualification_status": "NOT_EVALUATED",
              "admission_eligible": False, "e1_sample_status": "NOT_RUN", "e1_sample_count": 0,
              "unavailable": list(GAPS), "errors": []}
    failed = False
    try:
        require(docker_mapping is None, "external E04 Docker mapping is not E01 authority")
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
