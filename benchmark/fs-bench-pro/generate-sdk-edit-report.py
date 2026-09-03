#!/usr/bin/env python3
import csv
import hashlib
import importlib.util
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

MIB = 1024 * 1024
SIZES = (1 * MIB, 10 * MIB, 100 * MIB, 500 * MIB)
METRICS = ("edit_call_ns", "commit_call_ns", "edit_commit_ns")
NOMINAL_NS = (10_000_000, 10_000_000, 20_000_000)
ACCEPTED_NS = (20_000_000, 20_000_000, 30_000_000)


def latency_status(values):
    if any(values[field] > limit for field, limit in zip(METRICS, ACCEPTED_NS)):
        return "fail"
    if any(values[field] > limit for field, limit in zip(METRICS, NOMINAL_NS)):
        return "accepted-with-tolerance"
    return "nominal-pass"

FROZEN_REGISTRIES = {
    "edit_length_preserving": ("daa3bcb8ba94da6dc28f7ca87dc2b27612c9988cf42fe5398cdddb3a5b386324", [0,5,10,3,8]),
    "edit_length_changing": ("b6e8d0ab87a2ed72234623198994a460484bd950a04bb81a99a9aecda06c4390", [0,13,26,7,20]),
    "edit_canonical_chunk_count": ("e76f9b08f7312abf0f30447765e9ff734cecd6c41210788bd4917286059158bf", [0,4,8,1,5]),
}
spec = importlib.util.spec_from_file_location("sdk_edit_custody", Path(__file__).with_name("sdk-edit-custody.py"))
custody = importlib.util.module_from_spec(spec)
spec.loader.exec_module(custody)


def read_jsonl(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def median(rows, field):
    return int(statistics.median(row[field] for row in rows))


def envelope(values):
    low = min(values)
    return max(values) - low <= max(2_000_000, low // 10)


def add(failures, condition, message):
    if not condition:
        failures.append(message)


def custody_validation(root, selected=False, require_ending=False):
    entries = custody.verify_manifest(root, "environment/pre-run.sha256", complete=False)
    required = {f"environment/{name}" for name in (
        "command.txt", "behavior.env", "registry-meta.json", "scenario-registry.tsv", "sample-order.tsv",
        "fixture-manifest.json", "prepared-stores.tsv", "qualification.tsv", "timed-call-graph-manifest.json",
        "operation-route-manifest.json", "edit-conformance-manifest.json", "image.json", "source-identity.json", "host-runtime.json")}
    assert required <= entries.keys(), "preseal missing custody inputs"
    source = json.loads((root / "environment/source-identity.json").read_text())
    assert source["report_generator_sha256"] == custody.sha(Path(__file__)), "report generator identity"
    assert source["custody_helper_sha256"] == custody.sha(Path(custody.__file__)), "custody helper identity"
    registry = list(csv.DictReader((root / "environment/scenario-registry.tsv").open(), delimiter="\t"))
    family = registry[0]["family_id"]
    meta = json.loads((root / "environment/registry-meta.json").read_text())
    digest, rotations = FROZEN_REGISTRIES[family]
    assert custody.sha(root / "environment/scenario-registry.tsv") == meta["registry_manifest_sha256"] == digest
    assert meta["family_id"] == family and meta["rotations"] == rotations
    assert meta["combined_registry_sha256"] == "1773c7b82f739eaf1c2b8a2877f56baaa7e72b26ac8980802bdb82c80e270af6"
    expected_order = []
    for repetition, offset in enumerate(rotations, 1):
        arms = ("baseline", "candidate") if repetition % 2 else ("candidate", "baseline")
        for row in registry[offset:] + registry[:offset]:
            expected_order.append({"ordinal":str(len(expected_order)+1), "repetition":str(repetition),
                                   "scenario_id":row["scenario_id"], "first_arm":arms[0], "second_arm":arms[1]})
    assert list(csv.DictReader((root / "environment/sample-order.tsv").open(), delimiter="\t")) == expected_order, "frozen schedule"
    fixtures = json.loads((root / "environment/fixture-manifest.json").read_text())["fixtures"]
    prepared = list(csv.DictReader((root / "environment/prepared-stores.tsv").open(), delimiter="\t"))
    sizes = {fixture["fixture_bytes"] for fixture in fixtures}
    assert len(sizes) == len(fixtures) == len(prepared) and (selected or sizes == set(SIZES))
    assert {int(row["fixture_bytes"]) for row in prepared} == sizes
    for fixture in fixtures:
        size = fixture["fixture_bytes"]
        cached = json.loads((root / f"environment/prepared-cache-{size}.json").read_text())
        key, key_data = custody.prepared_key(cached["key_data"]["fixture"], cached["key_data"]["preparation_compatibility_sha256"])
        assert cached["key"] == key and cached["key_data"] == key_data and cached["fixture"] == fixture
        assert cached["cache_profile"] == "sdk-edit-prepared-store-cache-v1" and cached["status"] == "pass"
        store_row = next(row for row in prepared if int(row["fixture_bytes"]) == size)
        assert store_row["store_sha256"] == cached["store_sha256"] and int(store_row["store_bytes"]) == cached["store_bytes"]
        if not selected:
            assert cached["producer"]["status"] == "pass", "prepared input producer build"
            producer_dir = root / f"environment/prepared-source-{size}"
            custody.verify_manifest(producer_dir)
            assert json.loads((producer_dir / "build.json").read_text()) == cached["producer"]
            assert cached["key_data"]["preparation_compatibility_sha256"] == source["baseline"]["preparation_compatibility_sha256"] == source["candidate"]["preparation_compatibility_sha256"]
    qualification = list(csv.DictReader((root / "environment/qualification.tsv").open(), delimiter="\t"))
    expected_ids = {row["scenario_id"] for row in registry}
    assert len({row["scenario_id"] for row in qualification}) == len(qualification)
    assert ({row["scenario_id"] for row in qualification} <= expected_ids if selected else {row["scenario_id"] for row in qualification} == expected_ids)
    for row in qualification:
        scenario = next(item for item in registry if item["scenario_id"] == row["scenario_id"])
        fixture = next(item for item in fixtures if item["fixture_bytes"] == int(scenario["fixture_bytes"]))
        assert row["family_id"] == family and row["plan_sha256"] == scenario["plan_sha256"]
        assert row["initial_branch_root"] == fixture["branch_root"] and int(row["initial_extent_count"]) == fixture["extent_count"]
        for field in ("expected_branch_root", "expected_file_root", "expected_mapping_root"):
            assert len(row[field]) == 64 and all(char in "0123456789abcdef" for char in row[field])
        if family == "edit_canonical_chunk_count":
            outcome = scenario["operation_key"].rsplit("-", 1)[-1]
            assert int(row["expected_extent_count"]) == fixture["extent_count"] + {"preserve":0,"increase":1,"decrease":-1}[outcome]
            assert len(row["expected_sha256"]) == 64
    if not selected:
        assert source["source_policy"] == "authentic-directional" and source["harness_diff"] == "none"
        assert source["baseline"]["revision"] != source["candidate"]["revision"]
        assert source["current_revision"] == source["candidate"]["revision"] and source["current_tree"] == source["candidate"]["tree"]
        images = json.loads((root / "environment/image.json").read_text())
        assert len(images) == 2
        for arm, image in zip(("baseline", "candidate"), images):
            build_dir = root / f"environment/build-{arm}"
            custody.verify_manifest(build_dir)
            receipt = json.loads((build_dir / "build.json").read_text())
            assert receipt == source[arm] and receipt["status"] == "pass"
            assert custody.sha(build_dir / "evidence.sha256") == source[f"{arm}_build_manifest_sha256"]
            expected = custody.source_identity(receipt["revision"])
            assert all(receipt[key] == value for key, value in expected.items())
            commands = json.loads((build_dir / "commands.json").read_text())
            assert len(commands) == 5 and all(command["exit_code"] == 0 for command in commands)
            custody.validate_image_binaries(build_dir, receipt)
            for test in ("group_4_invalid_type_range_overflow_and_limits_are_atomic", "group_5_commit_publication_is_exactly_once_and_retry_is_up_to_date"):
                assert "1 passed" in (build_dir / f"{test}.stdout.txt").read_text(), "conformance execution"
            custody.validate_image(image, receipt)
            assert image["Id"] == receipt["image_id"] and source[f"{arm}_binary_sha256"] == receipt["binary_sha256"]
        for key in ("harness_seal", "workload_sha256", "report_generator_sha256", "custody_helper_sha256", "release_generator_sha256", "contract_sha256", "rustc", "cargo", "build_configuration"):
            assert source["baseline"][key] == source["candidate"][key]
        conformance = json.loads((root / "environment/edit-conformance-manifest.json").read_text())
        assert conformance["status"] == "pass-tested"
        assert conformance["source_arm_build_manifests"] == {arm:source[f"{arm}_build_manifest_sha256"] for arm in ("baseline","candidate")}
        assert custody.sha(root / "environment/treatment.patch") == source["treatment_sha256"]
        assert source["treatment_paths"] and all(path.startswith(custody.PRODUCT) for path in source["treatment_paths"])
        if require_ending:
            end = json.loads((root / "environment/source-identity-end.json").read_text())
            assert end.pop("status") == "pass" and all(source["candidate"][key] == value for key, value in end.items())
    return source, fixtures, qualification


def clock_validation(row, failures, row_id):
    try:
        ready,t0,t3,finish = (row[key] for key in ("host_observation_ready_ns","host_t0_ns","host_t3_ns","host_observation_finish_request_ns"))
        start,end = row["cgroup_window_start_ns"],row["cgroup_window_end_ns"]
        add(failures, 0 < ready <= t0 < t3 <= finish, f"{row_id} causal observation bracket")
        add(failures, 0 < start <= end and row["cgroup_window_duration_ns"]==end-start, f"{row_id} native observation window")
        add(failures, row.get("resource_observation_profile")=="ack-window-v1" and row.get("exact_cgroup_phase_attribution")=="unavailable" and row.get("category_peak_scope")=="sampled-window-not-continuous", f"{row_id} observation scope")
        add(failures, row.get("native_cgroup_peak_scope")=="whole-container-lifetime" and row.get("native_process_peak_scope")=="whole-worker-lifetime", f"{row_id} native peak scope")
        add(failures, row.get("clock_sampler_start_ns",0)>0 and row.get("cgroup_sample_count",0)>=2, f"{row_id} available observations")
        upper=max(0,row["cgroup_lifetime_peak_bytes"]-row["cgroup_memory_baseline_bytes"])
        add(failures, row.get("cgroup_incremental_upper_bound_bytes")==upper and upper<=32*MIB, f"{row_id} native incremental bound")
    except (KeyError, TypeError):
        failures.append(f"{row_id} missing observation evidence")


def performance_validation(root, write_summary=True, selected=False):
    registry = list(csv.DictReader((root / "environment/scenario-registry.tsv").open(), delimiter="\t"))
    order = list(csv.DictReader((root / "environment/sample-order.tsv").open(), delimiter="\t"))
    rows = read_jsonl(root / "performance/raw.jsonl")
    family = registry[0]["family_id"]
    registry_by_id = {entry["scenario_id"]: entry for entry in registry}
    failures = []
    try:
        custody_validation(root, selected=selected)
    except (AssertionError, KeyError, ValueError, OSError, StopIteration) as error:
        failures.append(f"custody: {error}")
    add(failures, len(registry_by_id) == len(registry) and len(registry) in (12, 32), "registry cardinality/uniqueness")
    expected_order = []
    for entry in order:
        expected_order.extend((entry["scenario_id"], int(entry["repetition"]), arm) for arm in (entry["first_arm"], entry["second_arm"]))
    actual_order = [(row.get("scenario_id"), row.get("repetition"), row.get("source_arm")) for row in rows]
    if not selected:
        add(failures, actual_order == expected_order, "raw stream differs from presealed sample order")
    add(failures, len(rows) == (1 if selected else len(registry) * 10), "performance cardinality")
    add(failures, len({row.get("row_id") for row in rows}) == len(rows), "performance row IDs")
    timed_sha = hashlib.sha256((root / "environment/timed-call-graph-manifest.json").read_bytes()).hexdigest()
    route_sha = hashlib.sha256((root / "environment/operation-route-manifest.json").read_bytes()).hexdigest()
    source = json.loads((root / "environment/source-identity.json").read_text())
    fixtures = {row["fixture_bytes"]:row for row in json.loads((root / "environment/fixture-manifest.json").read_text())["fixtures"]}
    source_sha = custody.sha(root / "environment/source-identity.json")
    by_cell = defaultdict(list)
    for ordinal, row in enumerate(rows, 1):
        scenario = registry_by_id.get(row.get("scenario_id"))
        row_id = row.get("row_id", "missing-row")
        add(failures, scenario is not None, f"{row_id} registry member")
        if scenario is None:
            continue
        arm = row.get("source_arm")
        add(failures, arm in ("baseline", "candidate") and row.get("repetition") in range(1,6), f"{row_id} source/repetition")
        if arm not in ("baseline", "candidate"):
            continue
        cached = json.loads((root / f"environment/prepared-cache-{scenario['fixture_bytes']}.json").read_text())
        by_cell[(row["scenario_id"], row["source_arm"])].append(row)
        expected_id = f"{family}:{row['scenario_id']}:r{row['repetition']}:{row['source_arm']}"
        exact = {
            "schema": "fs-bench-pro-sdk-edit-performance-v1",
            "family_id": family,
            "row_id": expected_id,
            "fixture_bytes": int(scenario["fixture_bytes"]),
            "initial_file_bytes": int(scenario["fixture_bytes"]),
            "final_file_bytes": int(scenario["final_bytes"]),
            "edit_start": int(scenario["start"]),
            "deleted_bytes": int(scenario["delete_len"]),
            "replacement_kind": scenario["replacement_kind"],
            "replacement_bytes": int(scenario["replacement_len"]),
            "replacement_sha256": scenario["payload_sha256"],
            "edit_plan_sha256": scenario["plan_sha256"],
            "operation_key": scenario["operation_key"],
            "payload_seed": int(scenario["payload_seed"]),
            "mode": "performance", "performance_distribution": True,
            "verification_status": "not-run-performance-mode", "fixture_profile": "sdk-edit-standard-content-v1",
            "initial_branch_root": fixtures[int(scenario["fixture_bytes"])]["branch_root"],
            "source_identity_sha256": source_sha, "source_revision": source[arm]["revision"],
            "contract_commit": source["contract_commit"], "scenario_version": 1,
            "product_identity": source[arm].get("product_seal"), "harness_identity": source[arm].get("harness_seal"),
            "workload_identity": source["workload_sha256"], "report_generator_identity": source["report_generator_sha256"],
            "treatment": source.get("treatment_sha256", "selected-unbound"), "sample_ordinal": ordinal,
            "clone_store_sha256": cached["store_sha256"], "prepared_store_sha256": cached["store_sha256"],
            "clone_bytes": cached["store_bytes"], "cache_profile": cached["cache_profile"], "cache_key": cached["key"],
            "cache_manifest_sha256": cached["cache_manifest_sha256"],
            "operation_entrypoint": "Client::edit_workspace_file_range",
            "operation_contract_id": "sdk-single-range-edit-v1", "timing_boundary_id": "sdk-edit-commit-return-v1",
        }
        for field, expected in exact.items():
            add(failures, row.get(field) == expected, f"{row_id} frozen {field}")
        add(failures, row.get("admission_eligible") is (not selected), f"{row_id} admission eligibility")
        add(failures, row.get("edit_commit_ns") == row.get("edit_call_ns", 0) + row.get("commit_call_ns", 0), f"{row_id} timing equation")
        for field in ("edit_call_ns", "commit_call_ns"):
            add(failures, row.get(field, 2_000_000_001) <= 2_000_000_000, f"{row_id} {field} watchdog")
        for field in ("logical_operation_count", "sdk_edit_member_count", "public_sdk_edit_call_count", "workspace_create_count", "workspace_commit_count", "workspace_end_count", "query_count"):
            add(failures, row.get(field) == 1, f"{row_id} {field}")
        add(failures, row.get("workspace_execution_count") == 0, f"{row_id} workspace execution")
        add(failures, row.get("active_workspace_count_after_end") == 0 and row.get("active_execution_count_after_end") == 0, f"{row_id} active resources")
        add(failures, row.get("operation_surface") == "public-sdk" and row.get("mutation_executor") == "fs-benchmark-pro-sdk", f"{row_id} operation surface")
        add(failures, row.get("timed_call_graph_manifest_status") == "pass" and row.get("timed_call_graph_manifest_sha256") == timed_sha, f"{row_id} timed manifest")
        add(failures, row.get("operation_route_manifest_status") == "pass" and row.get("operation_route_manifest_sha256") == route_sha, f"{row_id} route manifest")
        add(failures, row.get("projection_lifecycle") in (["attach","end"],["attach","end","attach","end"]), f"{row_id} projection lifecycle")
        add(failures, row.get("capture_mode") == "Live" and row.get("captured_files") == 0 and row.get("captured_bytes") == 0, f"{row_id} capture")
        zero = (
            "fuse_kernel_write_requests", "fuse_kernel_write_bytes", "fuse_client_request_copy_bytes",
            "fuse_frame_payload_copy_bytes", "fuse_client_frame_bytes", "fuse_host_frame_bytes",
            "fuse_host_decode_copy_bytes", "spool_write_bytes", "spool_allocated_bytes",
            "physical_spool_high_water_bytes", "spool_live_bytes", "spool_superseded_bytes",
            "swap_bytes", "cgroup_swap_peak_bytes", "cgroup_oom_delta", "cgroup_oom_kill_delta",
            "process_swap_count",
        )
        for field in zero:
            add(failures, row.get(field) == 0, f"{row_id} {field}")
        add(failures, row.get("commit_cdc_bytes_scanned") == row.get("final_live_non_base_bytes") == row.get("replacement_bytes"), f"{row_id} CDC/live bytes")
        add(failures, row.get("candidate_bytes", 1 << 60) <= row.get("final_live_non_base_bytes", 0) + 8 * MIB, f"{row_id} candidate bytes")
        add(failures, row.get("inserted_bytes", 1) <= row.get("candidate_bytes", 0), f"{row_id} inserted bytes")
        add(failures, row.get("max_transaction_objects", 128) <= 127 and row.get("max_transaction_bytes", 4 * MIB) < 4 * MIB, f"{row_id} transaction")
        add(failures, row.get("piece_count", 4) <= 3 and row.get("piece_logical_charge_bytes", 1025) <= 1024, f"{row_id} piece bounds")
        if family == "edit_canonical_chunk_count":
            add(failures, row.get("piece_count") == 3 and row.get("piece_height") == 2 and row.get("piece_logical_charge_bytes") == 384, f"{row_id} canonical piece identity")
        if family == "edit_length_changing":
            add(failures, row.get("commit_payload_bytes_read") == 0, f"{row_id} payload read")
        else:
            add(failures, row.get("commit_payload_bytes_read", 65_537) <= 65_536, f"{row_id} payload read")
        for prefix in ("cgroup", "rss"):
            add(failures, row.get(f"{prefix}_sample_count", 0) >= 2, f"{row_id} {prefix} observations")
            add(failures, row.get(f"{prefix}_sample_interval_ns", 0) > 0 and row.get(f"{prefix}_maximum_sample_gap_ns",0)>0, f"{row_id} {prefix} observed gaps")
            add(failures, row.get(f"{prefix}_coverage_status") == "pass", f"{row_id} {prefix} coverage")
        first, last = row.get("rss_first_sample_ns", -1), row.get("rss_last_sample_ns", -1)
        t0, t3 = row.get("rss_t0_ns", -1), row.get("rss_t3_ns", -1)
        add(failures, 0 <= first <= t0 < t3 <= last, f"{row_id} RSS observation bracket")
        clock_validation(row, failures, row_id)
        add(failures, row.get("cgroup_sampler_thread_count") == 2, f"{row_id} cgroup sampler")
        rss_upper=max(0,row.get("process_lifetime_peak_rss_bytes",1<<60)-row.get("rss_baseline_bytes",0))
        add(failures, row.get("rss_incremental_upper_bound_bytes")==rss_upper and rss_upper<=32*MIB, f"{row_id} native RSS incremental bound")
        add(failures, row.get("rss_incremental_peak_bytes") == max(0, row.get("rss_phase_peak_bytes", 0) - row.get("rss_baseline_bytes", 0)), f"{row_id} RSS formula")
        add(failures, row.get("cgroup_window_incremental_peak_bytes") == max(0, row.get("cgroup_window_peak_bytes", 0) - row.get("cgroup_memory_baseline_bytes", 0)), f"{row_id} cgroup formula")
        add(failures, row.get("dirty_writeback_incremental_peak_bytes") == max(0, row.get("dirty_writeback_peak_bytes", 0) - row.get("dirty_writeback_baseline_bytes", 0)), f"{row_id} dirty formula")
        add(failures, row.get("cgroup_oom_delta") == row.get("cgroup_oom_final", 0) - row.get("cgroup_oom_baseline", 0), f"{row_id} OOM formula")
        add(failures, row.get("cgroup_oom_kill_delta") == row.get("cgroup_oom_kill_final", 0) - row.get("cgroup_oom_kill_baseline", 0), f"{row_id} OOM-kill formula")
        add(failures, row.get("cgroup_window_peak_bytes", 1 << 60) <= 128 * MIB and row.get("cgroup_window_incremental_peak_bytes", 1 << 60) <= 32 * MIB, f"{row_id} cgroup ceilings")
        add(failures, row.get("cgroup_lifetime_peak_bytes", 1 << 60) <= 128 * MIB and row.get("dirty_writeback_incremental_peak_bytes", 1 << 60) <= 8 * MIB, f"{row_id} cgroup lifetime/dirty")
        add(failures, row.get("rss_phase_peak_bytes", 1 << 60) <= 128 * MIB and row.get("rss_incremental_peak_bytes", 1 << 60) <= 32 * MIB and row.get("process_lifetime_peak_rss_bytes", 1 << 60) <= 128 * MIB, f"{row_id} RSS ceilings")
        add(failures, row.get("resource_status") == "pass" and row.get("row_resource_status") == "pass", f"{row_id} resource status")
        add(failures, row.get("cleanup_status") == "pass" and row.get("container_cleanup_status") == "pass" and not row.get("timeout"), f"{row_id} cleanup")

    if selected:
        for row in rows:
            if row.get("source_arm") == "candidate":
                for metric, ceiling in zip(METRICS, ACCEPTED_NS):
                    add(failures, row.get(metric, ceiling+1) <= ceiling, f"selected candidate {metric} no-go")
        summary = {"schema": "fs-bench-pro-sdk-edit-summary-v1", "family_id": family, "performance_rows": len(rows), "failures": failures, "status": "pass-selected-non-admission" if not failures else "no-go-selected-non-admission"}
        summary["candidate_latency_status"] = {row["scenario_id"]:latency_status(row) for row in rows if row.get("source_arm")=="candidate"}
        if write_summary:
            (root / "performance/summary.json").write_text(json.dumps(summary,sort_keys=True,separators=(",", ":"))+"\n")
        return family, registry_by_id, rows, failures, summary

    for scenario_id in registry_by_id:
        for arm in ("baseline", "candidate"):
            cell = by_cell[(scenario_id, arm)]
            add(failures, len(cell) == 5 and {row["repetition"] for row in cell} == {1, 2, 3, 4, 5}, f"{scenario_id} {arm} repetitions")
        candidate = by_cell[(scenario_id, "candidate")]
        if candidate:
            for metric, ceiling in zip(METRICS, ACCEPTED_NS):
                add(failures, median(candidate, metric) <= ceiling, f"{scenario_id} candidate {metric} accepted median")

    operation_rows = defaultdict(lambda: defaultdict(list))
    for row in rows:
        operation = registry_by_id.get(row.get("scenario_id"), {}).get("operation_key")
        if operation:
            operation_rows[(row["source_arm"], operation)][row["fixture_bytes"]].append(row)
    parity = []
    for (arm, operation), sizes in operation_rows.items():
        add(failures, set(sizes) == set(SIZES), f"{arm} {operation} size matrix")
        if set(sizes) != set(SIZES):
            continue
        for field in METRICS:
            values = [median(sizes[size], field) for size in SIZES]
            passed = envelope(values)
            parity.append({"source_arm": arm, "operation_key": operation, "metric": field, "medians": values, "ratios_to_1mib":[value/values[0] if values[0] else None for value in values[1:]], "spread_ns":max(values)-min(values), "allowance_ns":max(2_000_000,min(values)//10), "status": "pass" if passed else "fail-diagnostic" if arm == "baseline" else "fail"})
            if arm == "candidate":
                add(failures, passed, f"candidate {operation} {field} size parity")
        for field in ("process_lifetime_peak_rss_bytes", "cgroup_lifetime_peak_bytes"):
            values = [median(sizes[size], field) for size in SIZES]
            add(failures, max(values) - min(values) <= 16 * MIB, f"{arm} {operation} {field} spread")

    cohorts = (
        ("inline-insert", {"insert-middle-4k", "append-tail-4k", "prepend-head-4k"}),
        ("delete", {"delete-middle-4k", "truncate-tail-4k"}),
        ("overwrite-position", {"overwrite-head-4k", "overwrite-middle-4k", "overwrite-tail-4k"}),
    )
    available = {entry["operation_key"] for entry in registry}
    cross = []
    for name, operations in cohorts:
        if not operations <= available:
            continue
        for size in SIZES:
            for field in METRICS:
                values = []
                for operation in sorted(operations):
                    cell = [row for row in rows if row["source_arm"] == "candidate" and row["fixture_bytes"] == size and registry_by_id[row["scenario_id"]]["operation_key"] == operation]
                    values.append(median(cell, field))
                passed = envelope(values)
                cross.append({"cohort": name, "fixture_bytes": size, "metric": field, "medians": values, "status": "pass" if passed else "fail"})
                add(failures, passed, f"candidate {name} {size} {field}")

    paired = []
    if family == "edit_canonical_chunk_count":
        descriptors=("fixture_bytes","initial_branch_root","edit_start","deleted_bytes","replacement_bytes","replacement_kind","public_sdk_edit_call_count","workspace_create_count","workspace_commit_count","workspace_end_count","query_count","workspace_execution_count","timing_boundary_id","timed_call_graph_manifest_sha256","prepared_store_sha256")
        for arm in ("baseline","candidate"):
            for repetition in range(1,6):
                for size in SIZES:
                    controls={registry_by_id[row["scenario_id"]]["operation_key"].rsplit("-",1)[-1]:row for row in rows if row["source_arm"]==arm and row["repetition"]==repetition and row["fixture_bytes"]==size}
                    passed=set(controls)=={"preserve","increase","decrease"} and len({tuple(row.get(field) for field in descriptors) for row in controls.values()})==1 and len({row["replacement_sha256"] for row in controls.values()})==3
                    paired.append({"source_arm":arm,"repetition":repetition,"fixture_bytes":size,"paired_control_id":controls.get("preserve",{}).get("scenario_id"),"status":"pass" if passed else "fail"})
                    add(failures,passed,f"canonical paired control {arm} r{repetition} {size}")

    summaries = []
    for scenario_id in registry_by_id:
        entry = {"scenario_id": scenario_id, "operation_key": registry_by_id[scenario_id]["operation_key"], "fixture_bytes": int(registry_by_id[scenario_id]["fixture_bytes"])}
        for arm in ("baseline", "candidate"):
            cell = by_cell[(scenario_id, arm)]
            if cell:
                entry[arm] = {field: {"median": median(cell, field), "min": min(row[field] for row in cell), "max": max(row[field] for row in cell), "samples": len(cell)} for field in METRICS + ("rss_phase_peak_bytes", "rss_incremental_peak_bytes", "cgroup_window_peak_bytes", "cgroup_window_incremental_peak_bytes", "dirty_writeback_incremental_peak_bytes", "process_lifetime_peak_rss_bytes", "cgroup_lifetime_peak_bytes", "rss_sample_count", "cgroup_sample_count", "rss_maximum_sample_gap_ns", "cgroup_maximum_sample_gap_ns", "spool_write_bytes", "physical_spool_high_water_bytes", "commit_cdc_bytes_scanned", "candidate_bytes", "clock_sampler_start_ns", "clone_wall_ns", "container_start_ns")}
        if "candidate" in entry:
            entry["candidate_latency_status"] = latency_status({field:entry["candidate"][field]["median"] for field in METRICS})
        summaries.append(entry)
    status = "pass" if not failures else "fail"
    summary = {"schema": "fs-bench-pro-sdk-edit-summary-v1", "family_id": family, "performance_rows": len(rows), "scenarios": summaries, "size_parity": parity, "matched_operation_parity": cross, "paired_controls": paired, "failures": failures, "performance_status": status}
    summary["latency_policy"] = {"nominal_ns":dict(zip(METRICS,NOMINAL_NS)),"accepted_ns":dict(zip(METRICS,ACCEPTED_NS)),"tolerance_authorized":"2026-09-04"}
    if write_summary:
        (root / "performance/summary.json").write_text(json.dumps(summary, sort_keys=True, separators=(",", ":")) + "\n")
    return family, registry_by_id, rows, failures, summary


def verification_validation(root, family, registry, performance, failures, write=True):
    subproofs = read_jsonl(root / "verification/subproofs.jsonl")
    source = json.loads((root / "environment/source-identity.json").read_text())
    fixtures = {row["fixture_bytes"]:row for row in json.loads((root / "environment/fixture-manifest.json").read_text())["fixtures"]}
    qualified = {row["scenario_id"]:row for row in csv.DictReader((root / "environment/qualification.tsv").open(), delimiter="\t")}
    manifests = {name:custody.sha(root / "environment" / filename) for name,filename in {
        "source_identity_sha256":"source-identity.json", "qualification_manifest_sha256":"qualification.tsv",
        "conformance_proof_sha256":"edit-conformance-manifest.json",
        "timed_call_graph_manifest_sha256":"timed-call-graph-manifest.json",
        "operation_route_manifest_sha256":"operation-route-manifest.json"}.items()}
    expected_keys = [(scenario, arm) for scenario in registry for arm in ("baseline", "candidate")]
    add(failures, [(row.get("scenario_id"), row.get("source_arm")) for row in subproofs] == expected_keys, "verification exact ordered membership")
    by_scenario = defaultdict(dict)
    performance_ids = {row["row_id"] for row in performance}
    for row in subproofs:
        scenario_id, arm = row.get("scenario_id"), row.get("source_arm")
        label = f"verification {scenario_id} {arm}"
        if scenario_id not in registry or arm not in ("baseline", "candidate"):
            failures.append(f"{label} unknown member")
            continue
        by_scenario[scenario_id][arm] = row
        scenario, expected = registry[scenario_id], qualified[scenario_id]
        fixture = fixtures[int(scenario["fixture_bytes"])]
        cached = json.loads((root / f"environment/prepared-cache-{scenario['fixture_bytes']}.json").read_text())
        exact = {
            "schema":"fs-bench-pro-sdk-edit-verification-v1", "receipt_kind":"source-arm-subproof",
            "family_id":family, "edit_plan_sha256":scenario["plan_sha256"],
            "initial_file_bytes":int(scenario["fixture_bytes"]), "final_file_bytes":int(scenario["final_bytes"]),
            "initial_branch_root":fixture["branch_root"], "initial_sha256":fixture["fixture_sha256"],
            "observed_branch_root":expected["expected_branch_root"], "expected_branch_root":expected["expected_branch_root"],
            "observed_canonical_file_root":expected["expected_file_root"], "expected_canonical_file_root":expected["expected_file_root"],
            "observed_mapping_root":expected["expected_mapping_root"], "expected_mapping_root":expected["expected_mapping_root"],
            "observed_extent_count":int(expected["expected_extent_count"]), "expected_extent_count":int(expected["expected_extent_count"]),
            "observed_initial_extent_count":int(expected["initial_extent_count"]), "expected_initial_extent_count":int(expected["initial_extent_count"]),
            "performance_distribution":False, "admission_eligible":False,
            "performance_binding_status":"bound-five-performance-rows",
            "fresh_client_reconnect":True, "fresh_store_reconnect":True, "fresh_fuse_reopen":True, "independent_byte_oracle":True,
            "failure_atomicity_status":"sealed-conformance", "retry_status":"sealed-conformance",
            "public_sdk_edit_call_count":1, "workspace_create_count":1, "workspace_commit_count":1, "workspace_end_count":1,
            "query_count":1, "read_only_verifier_execution_count":2,
            "active_workspace_count_after_end":0, "active_execution_count_after_end":0,
            "commit_cdc_bytes_scanned":int(scenario["replacement_len"]), "final_live_non_base_bytes":int(scenario["replacement_len"]),
            "capture_mode":"Live", "captured_files":0, "captured_bytes":0,
            "source_revision":source[arm]["revision"], "product_identity":source[arm]["product_seal"],
            "harness_identity":source[arm]["harness_seal"], "contract_commit":source["contract_commit"],
            "clone_store_sha256":cached["store_sha256"], "prepared_store_sha256":cached["store_sha256"],
            "cache_key":cached["key"], "cache_profile":cached["cache_profile"],
            "cgroup_sampler_thread_count":2,
            "container_exit_code":0, "container_oom_killed":False,
            **manifests,
        }
        for field,value in exact.items():
            add(failures, row.get(field) == value, f"{label} frozen {field}")
        expected_ids = {f"{family}:{scenario_id}:r{repetition}:{arm}" for repetition in range(1,6)}
        add(failures, len(row.get("performance_row_ids", [])) == 5 and set(row.get("performance_row_ids", [])) == expected_ids <= performance_ids, f"{label} performance binding")
        for field in ("status", "resource_status", "cleanup_status", "container_cleanup_status", "materialized_status", "payload_retention_status", "fresh_reopen_status", "operation_route_manifest_status", "timed_call_graph_manifest_status", "cgroup_coverage_status"):
            add(failures, row.get(field) == "pass", f"{label} {field}")
        add(failures, row.get("expected_sha256") == row.get("observed_sha256") == row.get("fuse_sha256") == row.get("materialized_sha256"), f"{label} digest equality")
        add(failures, row.get("materialized_bytes") == int(scenario["final_bytes"]), f"{label} materialized length")
        if expected["expected_sha256"] != "-":
            add(failures, row.get("expected_sha256") == expected["expected_sha256"], f"{label} frozen digest")
        add(failures, row.get("initial_inode_id") == row.get("final_inode_id") and row.get("inode_behavior") == "preserved", f"{label} canonical inode")
        add(failures, row.get("initial_fuse_inode", 0) > 0 and row.get("initial_fuse_inode") == row.get("final_fuse_inode"), f"{label} FUSE inode")
        add(failures, row.get("projection_lifecycle") in (["attach","end"],["attach","end","attach","end"]), f"{label} projection lifecycle")
        for field in ("fuse_kernel_write_requests","fuse_kernel_write_bytes","fuse_client_request_copy_bytes","fuse_frame_payload_copy_bytes",
                      "fuse_client_frame_bytes","fuse_host_frame_bytes","fuse_host_decode_copy_bytes","spool_write_bytes",
                      "spool_allocated_bytes","physical_spool_high_water_bytes","spool_live_bytes","spool_superseded_bytes",
                      "cgroup_swap_baseline_bytes","cgroup_swap_peak_bytes","cgroup_swap_final_bytes","cgroup_oom_delta","cgroup_oom_kill_delta","process_swap_count"):
            add(failures, row.get(field) == 0, f"{label} {field}")
        add(failures, row.get("candidate_bytes",1<<60) <= int(scenario["replacement_len"])+8*MIB and row.get("inserted_bytes",1) <= row.get("candidate_bytes",0), f"{label} candidate bounds")
        add(failures, row.get("max_transaction_objects",128) <= 127 and row.get("max_transaction_bytes",4*MIB) < 4*MIB, f"{label} transaction bounds")
        add(failures, row.get("piece_count",4) <= 3 and row.get("piece_logical_charge_bytes",1025) <= 1024, f"{label} piece bounds")
        add(failures, row.get("commit_payload_bytes_read",65537) <= (0 if family=="edit_length_changing" else 65536), f"{label} payload reads")
        if family == "edit_canonical_chunk_count":
            add(failures, (row.get("piece_count"),row.get("piece_height"),row.get("piece_logical_charge_bytes")) == (3,2,384), f"{label} canonical piece identity")
        if int(scenario["delete_len"]) == 0:
            add(failures, row.get("lost_payload_object_count") == 0, f"{label} insertion retention")
        else:
            add(failures, row.get("lost_payload_object_count",1<<60) <= row.get("initial_payload_object_count",0)-row.get("untouched_payload_object_count",0), f"{label} untouched retention")
        add(failures, row.get("referenced_extent_count") == row.get("observed_extent_count") and row.get("unique_payload_object_count") == row.get("observed_payload_object_count") and 0 < row.get("unique_payload_object_count",0) <= row.get("referenced_extent_count",0), f"{label} extent/object distinction")
        clock_validation(row, failures, label)
        add(failures, row.get("cgroup_sample_count",0)>=2 and row.get("cgroup_sample_interval_ns",0)>0 and row.get("cgroup_maximum_sample_gap_ns",0)>0, f"{label} cgroup observations")
        add(failures, row.get("cgroup_window_incremental_peak_bytes") == max(0,row.get("cgroup_window_peak_bytes",0)-row.get("cgroup_memory_baseline_bytes",0)), f"{label} cgroup formula")
        add(failures, row.get("dirty_writeback_incremental_peak_bytes") == max(0,row.get("dirty_writeback_peak_bytes",0)-row.get("dirty_writeback_baseline_bytes",0)), f"{label} dirty formula")
        for field in ("cgroup_window_peak_bytes","cgroup_lifetime_peak_bytes","process_lifetime_peak_rss_bytes"):
            add(failures, row.get(field,1<<60)<=128*MIB, f"{label} {field} ceiling")
        add(failures, row.get("cgroup_window_incremental_peak_bytes",1<<60)<=32*MIB and row.get("dirty_writeback_incremental_peak_bytes",1<<60)<=8*MIB, f"{label} incremental ceilings")
    receipts = []
    for scenario_id, scenario in registry.items():
        arms = by_scenario[scenario_id]
        add(failures, set(arms)=={"baseline","candidate"}, f"verification {scenario_id} arms")
        if set(arms)!={"baseline","candidate"}:
            continue
        for field in ("expected_sha256","observed_branch_root","observed_canonical_file_root","observed_extent_count","observed_mapping_root","edit_plan_sha256"):
            add(failures, arms["baseline"].get(field)==arms["candidate"].get(field), f"verification {scenario_id} cross-arm {field}")
        encoded = {arm:{"sha256":hashlib.sha256(json.dumps(row,sort_keys=True,separators=(",",":")).encode()).hexdigest(),"receipt":row} for arm,row in arms.items()}
        receipts.append({"schema":"fs-bench-pro-sdk-edit-verification-v1","receipt_kind":"aggregate","family_id":family,
                         "scenario_id":scenario_id,"edit_plan_sha256":scenario["plan_sha256"],"source_subproofs":encoded,
                         "performance_row_count":10,"status":"pass" if not failures else "fail"})
    if failures:
        for receipt in receipts:
            receipt["status"]="fail"
    if write:
        (root/"verification/raw.jsonl").write_text("".join(json.dumps(row,sort_keys=True,separators=(",",":"))+"\n" for row in receipts))
    add(failures, read_jsonl(root/"verification/raw.jsonl")==receipts and len(receipts)==len(registry), "verification aggregate revalidation")
    summary={"schema":"fs-bench-pro-sdk-edit-summary-v1","family_id":family,"aggregate_receipts":len(receipts),
             "source_subproofs":len(subproofs),"verification_status":"pass" if not failures else "fail"}
    if write:
        (root/"verification/summary.json").write_text(json.dumps(summary,sort_keys=True,separators=(",",":"))+"\n")
    return receipts, summary


def write_report(root, family, summary, receipts, failures):
    status = "pass" if not failures else "fail"
    lines = [f"# {family} SDK-only edit benchmark", "", f"Status: **{status.upper()}**", "", f"Raw evidence: [performance JSONL](performance/raw.jsonl), [verification aggregates](verification/raw.jsonl), [source subproofs](verification/subproofs.jsonl).", "", "## Latency", "", "| Operation | Size | Source | Samples | Edit median (min–max) ms | Commit median (min–max) ms | Edit+Commit median (min–max) ms |", "| --- | ---: | --- | ---: | ---: | ---: | ---: |"]
    for item in summary["scenarios"]:
        for arm in ("baseline", "candidate"):
            metrics = item.get(arm)
            if not metrics:
                continue
            cell = lambda name: f"{metrics[name]['median']/1e6:.3f} ({metrics[name]['min']/1e6:.3f}–{metrics[name]['max']/1e6:.3f})"
            lines.append(f"| `{item['operation_key']}` | {item['fixture_bytes']//MIB} MiB | {arm} | {metrics['edit_call_ns']['samples']} | {cell('edit_call_ns')} | {cell('commit_call_ns')} | {cell('edit_commit_ns')} |")
    lines += ["", "Nominal targets are 10/10/20 ms; user-approved accepted ceilings are 20/20/30 ms for Edit/Commit/combined. Combined is independently capped at 30 ms. Parity and resource gates are unchanged.", "", "| Candidate scenario | Latency classification |", "| --- | --- |"]
    lines[-2:-2] = ["Memory profile: ack-window-v1. Cgroup observations cover an acknowledged broader window, not exact T0–T3. Native peaks are whole-worker/container lifetime bounds. Category maxima, dirty/writeback, and transient swap checks are sampled observations; continuous category ceilings cannot be strictly proven. Gaps are reported diagnostically. Native peak/incremental/size-spread limits and zero OOM remain binding.", ""]
    lines += [f"| `{item['scenario_id']}` | {item.get('candidate_latency_status','missing')} |" for item in summary["scenarios"]]
    lines += ["", "## Memory", "", "| Operation | Size | Source | Process phase MiB median (min–max) | Process incremental MiB median (min–max) | Cgroup sampled window MiB median (min–max) | Cgroup sampled window incremental MiB median (min–max) | Dirty/writeback incremental MiB median (min–max) |", "| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |"]
    for item in summary["scenarios"]:
        for arm in ("baseline", "candidate"):
            metrics = item.get(arm)
            if not metrics:
                continue
            def mib(name):
                value = metrics[name]
                return f"{value['median']/MIB:.3f} ({value['min']/MIB:.3f}–{value['max']/MIB:.3f})"
            lines.append(f"| `{item['operation_key']}` | {item['fixture_bytes']//MIB} MiB | {arm} | {mib('rss_phase_peak_bytes')} | {mib('rss_incremental_peak_bytes')} | {mib('cgroup_window_peak_bytes')} | {mib('cgroup_window_incremental_peak_bytes')} | {mib('dirty_writeback_incremental_peak_bytes')} |")
    lines += ["", f"Aggregate verifier receipts: {len(receipts)}.", "", "Candidate size parity, matched-operation parity, route, CDC, spool, transaction, memory, cleanup, and custody gates are admission-binding. Baseline latency parity is diagnostic; baseline correctness, route, resource, cleanup, and custody remain binding."]
    lines += ["", "## Per-sample resource and mechanism guards", "", "All maxima below cover every retained sample, not only medians. Swap/OOM, FUSE mutation bytes, and spool must be zero; coverage and cleanup must pass. The 112 MiB target is diagnostic; 128 MiB is the unchanged hard ceiling.", "", "| Operation | MiB | Arm | Lifetime RSS / cgroup max MiB | RSS / cgroup max gap ms | Minimum RSS / cgroup samples | CDC bytes min–max | Candidate bytes max | Spool bytes max | 112 MiB target |", "| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |"]
    for item in summary["scenarios"]:
        for arm in ("baseline","candidate"):
            m=item.get(arm)
            if not m: continue
            target="target-pass" if max(m["rss_phase_peak_bytes"]["max"],m["cgroup_window_peak_bytes"]["max"])<=112*MIB else "target-miss"
            lines.append(f"| {item['operation_key']} | {item['fixture_bytes']//MIB} | {arm} | {m['process_lifetime_peak_rss_bytes']['max']/MIB:.3f} / {m['cgroup_lifetime_peak_bytes']['max']/MIB:.3f} | {m['rss_maximum_sample_gap_ns']['max']/1e6:.3f} / {m['cgroup_maximum_sample_gap_ns']['max']/1e6:.3f} | {m['rss_sample_count']['min']} / {m['cgroup_sample_count']['min']} | {m['commit_cdc_bytes_scanned']['min']}–{m['commit_cdc_bytes_scanned']['max']} | {m['candidate_bytes']['max']} | {max(m['spool_write_bytes']['max'],m['physical_spool_high_water_bytes']['max'])} | {target} |")
    lines += ["", "## Size parity", "", "Ratios use the 1 MiB median as denominator; spread and allowance are independently evaluated for each metric.", "", "| Operation | Arm | Metric | 10/1 | 100/1 | 500/1 | Spread / allowance ms | Status |", "| --- | --- | --- | ---: | ---: | ---: | ---: | --- |"]
    for row in summary["size_parity"]:
        ratios=[f"{value:.3f}" if value is not None else "unavailable" for value in row["ratios_to_1mib"]]
        lines.append(f"| {row['operation_key']} | {row['source_arm']} | {row['metric']} | {' | '.join(ratios)} | {row['spread_ns']/1e6:.3f} / {row['allowance_ns']/1e6:.3f} | {row['status']} |")
    lines += ["", "## Matched-operation parity", "", "| Cohort | MiB | Metric | Medians ms | Status |", "| --- | ---: | --- | --- | --- |"]
    for row in summary["matched_operation_parity"]:
        lines.append(f"| {row['cohort']} | {row['fixture_bytes']//MIB} | {row['metric']} | {', '.join(f'{value/1e6:.3f}' for value in row['medians'])} | {row['status']} |")
    if summary["paired_controls"]:
        lines += ["", "## Canonical controls", "", "All five repetitions and both arms are checked for fixture/root/range/replacement-length/topology/timing identity. Unique payload objects are not extent count.", "", "| Scenario | C0 | C1 | Delta | Unique payload objects | Mapping nodes / level | Status |", "| --- | ---: | ---: | ---: | ---: | ---: | --- |"]
        for receipt in receipts:
            row=receipt["source_subproofs"]["candidate"]["receipt"]
            lines.append(f"| {receipt['scenario_id']} | {row['observed_initial_extent_count']} | {row['observed_extent_count']} | {row['observed_extent_count']-row['observed_initial_extent_count']:+d} | {row['unique_payload_object_count']} | {row['mapping_node_count']} / {row['mapping_tree_level']} | {receipt['status']} |")
    lines += ["", "## Untimed preparation", "", "| MiB | Cache disposition | Build ms | Validation ms | Acquisition ms | Cache key |", "| ---: | --- | ---: | ---: | ---: | --- |"]
    for path in sorted((root/"environment").glob("prepared-cache-*.json")):
        row=json.loads(path.read_text())
        lines.append(f"| {row['fixture']['fixture_bytes']//MIB} | {row['cache_disposition']} | {row['cache_build_ns']/1e6:.3f} | {row['cache_validation_ns']/1e6:.3f} | {row['cache_acquisition_ns']/1e6:.3f} | {row['key']} |")
    lines += ["", "Qualification and clone setup are retained in [qualification timing](environment/qualification-timing.tsv); each raw row records its clone method/digest/wall, container-start wall, and clock_sampler_start_ns for authenticated connection and sampler warmup. These are never part of edit or Commit latency. Cgroup observation uses an acknowledged broader window with no clock probes. Exact phase attribution and continuous category maxima are unavailable; actual gaps are reported diagnostically.", "", f"Pre-run manifest SHA-256: {custody.sha(root/'environment/pre-run.sha256')}. The enclosing evidence manifest identity is shown by the cross-family report."]
    if failures:
        lines += ["", "## Failures", ""] + [f"- {failure}" for failure in failures]
    (root / "report.md").write_text("\n".join(lines) + "\n")
    status_doc = {"schema": "fs-bench-pro-sdk-edit-status-v1", "family_id": family, "admission_eligible": not failures, "performance_status": "pass" if not failures else "fail", "verification_status": "pass" if not failures else "fail", "resource_status": "pass" if not failures else "fail", "cleanup_status": "pass" if not failures else "fail", "custody_status": "pass" if not failures else "fail", "order_status": "pass" if not failures else "fail", "receipt_completeness_status": "pass" if not failures else "fail", "claim_eligibility_status": "pass" if not failures else "fail", "performance_rows": sum(10 for _ in summary["scenarios"]), "verification_receipts": len(receipts), "source_subproofs": len(receipts) * 2, "failures": failures, "status": "pass" if not failures else "fail"}
    (root / "run-status.json").write_text(json.dumps(status_doc, sort_keys=True, separators=(",", ":")) + "\n")


def self_check():
    row={"resource_observation_profile":"ack-window-v1",
         "exact_cgroup_phase_attribution":"unavailable","category_peak_scope":"sampled-window-not-continuous",
         "native_cgroup_peak_scope":"whole-container-lifetime","native_process_peak_scope":"whole-worker-lifetime",
         "host_observation_ready_ns":1000,"host_t0_ns":2000,"host_t3_ns":30_000_000,"host_observation_finish_request_ns":40_000_000,
         "cgroup_window_start_ns":100,"cgroup_window_end_ns":40_000_100,"cgroup_window_duration_ns":40_000_000,
         "cgroup_sample_count":4,"cgroup_maximum_sample_gap_ns":12_000_000,"clock_sampler_start_ns":1000,
         "cgroup_lifetime_peak_bytes":1024,"cgroup_memory_baseline_bytes":512,"cgroup_incremental_upper_bound_bytes":512}
    failures=[];clock_validation(row,failures,"ack-window-check");assert not failures,failures
    invalid=dict(row,host_observation_ready_ns=3000)
    failures=[];clock_validation(invalid,failures,"late-ready");assert failures
    invalid=dict(row);del invalid["cgroup_lifetime_peak_bytes"]
    failures=[];clock_validation(invalid,failures,"missing-native-peak");assert failures
    invalid=dict(row,resource_observation_profile="old-exact-phase")
    failures=[];clock_validation(invalid,failures,"mixed-profile");assert failures
    assert envelope([1_000_000,3_000_000]) and not envelope([1_000_000,3_000_001])
    assert envelope([30_000_000,33_000_000]) and not envelope([30_000_000,33_000_001])
    assert sum((12,32,12))==56 and 10*sum((12,32,12))==560
    assert latency_status(dict(zip(METRICS,NOMINAL_NS)))=="nominal-pass"
    assert latency_status(dict(zip(METRICS,(20_000_000,10_000_000,30_000_000))))=="accepted-with-tolerance"
    assert latency_status(dict(zip(METRICS,(20_000_000,10_000_001,30_000_001))))=="fail"
    assert latency_status(dict(zip(METRICS,(20_000_001,0,20_000_001))))=="fail"
    print(json.dumps({"schema":"fs-bench-pro-sdk-edit-report-self-check-v1","status":"pass","synthetic_only":True}))


def main():
    if sys.argv[1:]==["--self-check"]:
        self_check()
        return
    if len(sys.argv) not in (2, 3):
        raise SystemExit("usage: generate-sdk-edit-report.py RUN_DIR [--performance-only]")
    root = Path(sys.argv[1])
    family, registry, performance, failures, performance_summary = performance_validation(root)
    if len(sys.argv) == 3:
        if sys.argv[2] != "--performance-only":
            raise SystemExit("unknown mode")
        (root / "performance/gate.json").write_text(json.dumps({"schema": "fs-bench-pro-sdk-edit-status-v1", "family_id": family, "failures": failures, "status": "pass" if not failures else "fail"}, sort_keys=True, separators=(",", ":")) + "\n")
        raise SystemExit(0 if not failures else 1)
    receipts, verification_summary = verification_validation(root, family, registry, performance, failures)
    try:
        custody_validation(root, require_ending=True)
    except (AssertionError, KeyError, ValueError, OSError, StopIteration) as error:
        failures.append(f"ending custody: {error}")
    write_report(root, family, performance_summary, receipts, failures)
    raise SystemExit(0 if not failures else 1)


if __name__ == "__main__":
    main()
