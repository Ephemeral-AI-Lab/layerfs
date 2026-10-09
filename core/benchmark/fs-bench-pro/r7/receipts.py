"""Fail-closed R7 envelopes over unmodified original driver observations.

Unavailable instruments are explicit null/status/reason observations, never
invented zeroes. Validators preserve exploratory eligibility and every unrun
selection; they do not turn counts, VmHWM or scoped proof into qualification.
"""
import hashlib
import json
from pathlib import Path

from .registry import CACHE, COUNTERS, IMAGE, PHASES, cases, registry_identity, selections
from . import registry_variant

SCHEMA = "r7-optimization-receipt-v1"
IDENTITIES = ["source_commit", "source_tree", "product", "compilation", "dependency",
              "binary_sha256", "image_id", "harness", "workload", "fixture", "oracle", "report_generator"]
OBSERVATION_STATUSES = {"AVAILABLE", "UNAVAILABLE", "NOT_APPLICABLE"}
ROW_STATUSES = {"NOT_RUN", "INCOMPLETE", "INELIGIBLE", "FAIL", "DIAGNOSTIC"}
OPERATION_FIELDS = ["operation_contract_id", "operation_surface", "operation_entrypoint",
                    "orchestration_executor", "mutation_executor", "implementation_route_status",
                    "timing_boundary_id", "clock_id", "start_event", "end_event"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value):
    return type(value) is int and value >= 0


def unavailable(reason, not_applicable=False):
    require(isinstance(reason, str) and bool(reason), "unavailable reason required")
    return {"status": "NOT_APPLICABLE" if not_applicable else "UNAVAILABLE", "value": None, "reason": reason}


def observation(row, label):
    require(isinstance(row, dict) and row.get("status") in OBSERVATION_STATUSES, label + ": availability status required")
    require("value" in row, label + ": explicit value required")
    if row["status"] == "AVAILABLE":
        require(row["value"] is not None and isinstance(row.get("provenance"), str) and bool(row["provenance"]), label + ": available provenance required")
    else:
        require(row["value"] is None and isinstance(row.get("reason"), str) and bool(row["reason"]), label + ": null and reason required")


def phase(row, label):
    observation(row, label)
    if row["status"] != "AVAILABLE":
        return
    value = row["value"]
    require(isinstance(value, dict), label + ": phase object required")
    require(all(integer(value.get(key)) for key in ("start_ns", "end_ns", "duration_ns")), label + ": monotonic marks required")
    require(value["end_ns"] >= value["start_ns"] and value["duration_ns"] == value["end_ns"] - value["start_ns"], label + ": phase arithmetic")
    require(value.get("clock_id") == "monotonic" and isinstance(value.get("start_event"), str)
            and isinstance(value.get("end_event"), str), label + ": timing events required")


def validate_cache(row, arm, attempts, ineligible=False):
    cls = row.get("class")
    require(cls in CACHE or cls is None, "unregistered cache class")
    require(isinstance(row.get("scopes"), dict) and set(row["scopes"]) == set(PHASES)
            and all(isinstance(value, str) and bool(value) for value in row["scopes"].values()), "per-phase cache scopes required")
    if cls is None:
        require(bool(row.get("reason")), "no-class Commit requires explicit residency/own-write declaration")
        return
    if cls == "B":
        require(arm == "L", "class B is L-only")
        require(integer(row.get("object_demands")) and (row["object_demands"] == 0 or ineligible), "class B measured object demands must be zero or explicitly INELIGIBLE")
        require(row.get("warmup_terminal_unmount") is True, "class B earlier terminal mount required")
        require(bool(row.get("history_read_residency")), "class B paid history residency required")
    if cls == "C":
        require(integer(row.get("warmup_gap_ns")) and (row["warmup_gap_ns"] < 60_000_000_000 or ineligible),
                "class C cache lifetime exceeded or unknown")
        require(row.get("same_mount") is True and bool(row.get("warmup_receipt")), "class C same-mount warm-up required")
    if cls == "A":
        if arm == "P":
            require(row.get("fresh_kernel_connection") is True, "class A passthrough fresh connection required")
        declared = row.get("declared_external_inputs",{})
        auxiliary = row.get("auxiliary_residencies",[])
        require(isinstance(auxiliary,list), "additional cold source predicates must be explicit")
        replay = [item for item in auxiliary if item.get("role") == "replay"]
        semantic = [item for item in auxiliary if item.get("role") == "semantic"]
        require({item.get("root") for item in replay} == set(declared.get("replay_roots",[])), "all declared replay cold roots require independent predicates")
        require(len(semantic) == bool(declared.get("semantic_files",[])), "all declared semantic cold files require independent predicate")
        for component in auxiliary:
            require(component.get("role") in {"replay","semantic"}, "unknown additional cold input role")
            require(component.get("residency",{}).get("status") == "ELIGIBLE" or
                    ineligible and attempts == 0 and component.get("residency",{}).get("status") == "INELIGIBLE",
                    "additional cold predicate unavailable or not eligible")
            if component["role"] == "replay":
                require(component["residency"].get("schema") == "r7-residency-stream-v1" and
                        component["residency"].get("input_manifest",{}).get("root") == component["root"],
                        "additional replay manifest must cover that actual root")
            view = dict(component,scopes=row["scopes"])
            view["class"] = "A"
            # These are additional native file sources. Main L Store/sidecar
            # requirements are validated independently below, never replaced.
            validate_cache(view,"N",attempts,ineligible)
        if semantic:
            require(set(semantic[0].get("required_residency_paths",[])) == set(declared["semantic_files"]), "semantic cold file coverage mismatch")
        residency = row.get("residency")
        require(isinstance(residency, dict) and residency.get("cache_class") == "A", "class A per-file residency required")
        if residency.get("schema") == "r7-residency-stream-v1":
            require(arm in {"N", "P"}, "streamed native inventory is not a Store-sidecar substitute")
            inventory, manifest = residency.get("inventory", {}), residency.get("input_manifest", {})
            require(inventory.get("created") is True and inventory.get("complete") is True, "complete streamed residency inventory required")
            require(inventory.get("sha256") == row.get("stream_inventory_sha256") and bool(row.get("stream_inventory_artifact")), "stream inventory raw artifact identity required")
            require(manifest.get("sha256") == manifest.get("expected_sha256") and bool(manifest.get("sha256")), "sealed native manifest identity mismatch")
            require(integer(inventory.get("rows")) and inventory["rows"] == manifest.get("declared_files")
                    and inventory.get("physical_files") == manifest.get("declared_physical_files"), "streamed residency cardinality mismatch")
            require(integer(residency.get("resident_pages")) and residency.get("eviction_hint_attempts") == inventory.get("physical_files")
                    and residency.get("payload_bytes_read") == 0 and residency.get("attempts") == 0, "actual streamed per-file hint/residency required")
            require(residency["resident_pages"] == 0 or attempts == 0, "resident cold input attempted; must be INELIGIBLE with zero attempts")
            require("mincore" in residency.get("method", "").lower(), "actual streamed mincore required")
            return
        files = residency.get("files")
        require(isinstance(files, list) and bool(files), "class A no checked input files")
        paths = {item.get("path") for item in files}
        require(len(paths) == len(files) and all(isinstance(path, str) and path for path in paths), "residency file inventory mismatch")
        required = row.get("required_residency_paths")
        require(isinstance(required, list) and set(required) == paths, "complete input and sidecar inventory required")
        if arm == "L":
            require(isinstance(row.get("store_path"), str) and bool(row["store_path"]), "complete input and sidecar Store path required")
            sidecars = {row["store_path"] + suffix for suffix in ("", "-wal", "-shm", "-journal")}
            if row.get("overlay_path"):
                sidecars |= {row["overlay_path"] + suffix for suffix in ("", "-wal", "-shm", "-journal")}
            require(sidecars <= paths, "complete input and sidecar inventory required")
        require(residency.get("payload_bytes_read") == 0 and residency.get("attempts") == 0, "residency observer cannot read payload or attempt product")
        require("mincore" in residency.get("method", "").lower(), "actual mincore observation required")
        require(all(type(item.get("present")) is bool and integer(item.get("resident_pages"))
                    and (not item["present"] or item.get("eviction_hint_attempts") == 1) for item in files), "class A eviction and measured residency required")
        resident = sum(item["resident_pages"] for item in files)
        require(residency.get("resident_pages") == resident, "residency total mismatch")
        require(resident == 0 or attempts == 0, "resident cold input attempted; must be INELIGIBLE with zero attempts")
        if arm == "L":
            require(row.get("fresh_daemon_cache") is True and row.get("fresh_kernel_connection") is True, "class A fresh cache state required")
        elif arm == "P":
            require(row.get("fresh_kernel_connection") is True, "class A passthrough fresh connection required")


def validate_receipt(row):
    require(row.get("schema") == SCHEMA and row.get("family_id") == "r7-optimization", "R7 receipt schema/family")
    require(row.get("mode") == "exploratory" and row.get("admission_eligible") is False, "exploratory receipt cannot be promoted")
    variant = row.get("oracle_variant")
    require(variant in {None,registry_variant.NAME}, "unknown prospective oracle variant")
    require(row.get("registry_identity") == (registry_variant.identity() if variant else registry_identity()), "registry identity mismatch")
    selected = next((item for item in selections() if item["selection_id"] == row.get("selection_id")), None)
    require(selected is not None, "unknown selection")
    if variant:
        require(selected["case_id"] in registry_variant.GIT_TREE_CASES, "new Git index oracle variant not selected for this case")
        require(row.get("oracle_contract") == registry_variant.CONTRACT, "prospective Git index scoped oracle contract mismatch")
    require(row.get("row_status") in ROW_STATUSES, "invalid exploratory row status")
    require(integer(row.get("attempted_operation_count")) and integer(row.get("completed_operation_count")), "attempt/completion count required")
    attempts = row["attempted_operation_count"]
    require(attempts <= 1 and row["completed_operation_count"] <= attempts, "one original operation; no replay")
    require(integer(row.get("sample_count")) and row["sample_count"] == attempts, "sample count differs from original attempts")
    if row["row_status"] == "NOT_RUN":
        require(attempts == 0 and isinstance(row.get("reason"), str) and bool(row["reason"]), "unrun selection needs zero attempts and reason")
        return row
    case = next(item for item in cases() if item["case_id"] == selected["case_id"])
    require(row.get("source_arm") == selected["arm"] and row.get("scenario_id") == selected["case_id"]
            and row.get("scenario_version") == 1, "registered scenario/source arm mismatch")
    require(row.get("seed_or_repetition") == 1 and bool(row.get("contract_commit")) and bool(row.get("treatment")),
            "contract/treatment and one repetition required")
    require(not case["not_run_reason"], "owner NOT_RUN case cannot be attempted")
    require(not (case["set"] == "commit" and selected["arm"] != "L"), "Commit control has no authentic Store Commit")
    require(not (selected["cache_class"] == "C" and selected["case_id"] in {"E12", "E13", "C12"}), "non-repeatable identical-call cache warm-up conflict")
    identity = row.get("identities")
    require(isinstance(identity, dict) and all(isinstance(identity.get(key), str) and bool(identity[key]) for key in IDENTITIES), "sealed identity inventory incomplete")
    for key in OPERATION_FIELDS:
        require(isinstance(row.get(key), str) and bool(row[key]), "operation/timing custody missing " + key)
    require(row.get("public_api_call_count") is not None and integer(row["public_api_call_count"]), "observed public call count required")
    require(row.get("forbidden_route_count") == 0 and row.get("fallback_count") == 0, "forbidden/fallback route used")
    require(identity["image_id"] == IMAGE and row.get("build_profile") == "release" and row.get("dirty") is False,
            "pinned release clean arm required")
    require(row.get("construction_workers") == 1 and row.get("store_profile") == "Disposable/WAL/OFF"
            and row.get("overlay_profile") == "MEMORY/OFF/EXCLUSIVE", "owner profile/worker contract")
    require(row.get("exec_registration_count") == 0 and row.get("command_launcher") == "ordinary external bash",
            "external execution ownership required")
    require(integer(row.get("uid")) and integer(row.get("gid")), "actual common uid/gid required")
    require(row.get("setup_method") in {"clone", "qualified-shared-read-only", "empty"}, "declared setup reuse required")
    require(row.get("benchmark_verification_count_in_performance") == 0, "verification contaminated performance")
    require(bool(row.get("declared_interference")) and bool(row.get("exact_command")), "command and interference custody required")
    for key in ("performance_status", "verification_status", "resource_status", "cleanup_status", "custody_status", "failure_class"):
        require(isinstance(row.get(key), str) and bool(row[key]), key + " required")
    validate_cache(row.get("cache", {}), selected["arm"], attempts, row["row_status"] == "INELIGIBLE")
    if selected["cache_class"] == "A" and selected["case_id"] in {"E12","E13"}:
        require(row["cache"].get("declared_external_inputs",{}).get("replay_roots") == ["/replay"], "setup-warm replay must not credit a cold copy/link command")
    if selected["cache_class"] == "A" and selected["case_id"] == "E14":
        require(row["cache"].get("declared_external_inputs",{}).get("semantic_files") == ["/code/node-roots.json"], "setup-warm semantic input must not credit cold remove command")
    require(row["cache"]["class"] == selected["cache_class"], "cache class differs from registration")
    for name in PHASES:
        require(name in row.get("phases", {}), "missing phase " + name)
        phase(row["phases"][name], name)
    for name in COUNTERS:
        require(name in row.get("observations", {}), "missing counter " + name)
        observation(row["observations"][name], name)
    require(row.get("complete_command_wall_stop_ns") == case["complete_command_wall_stop_ns"], "registered wall stop changed")
    require(integer(row.get("complete_command_ns")), "complete command raw ns required")
    if not case["numeric_budget_verdict"]:
        require(row["performance_status"] in {"PRICED_DIAGNOSTIC", "STOPPED", "FAIL", "INCOMPLETE"}, "priced diagnostics have no latency pass/fail verdict")
    require(row.get("oracle_scope") == case["oracle_scope"], "oracle scope differs from registration")
    if selected["case_id"] == "E03":
        require(row["verification_status"] != "PASS", "timed tar stream oracle unavailable; cannot claim PASS from wc-only body")
    if selected["cache_class"] == "A" and row["cache"]["residency"]["resident_pages"]:
        require(row["row_status"] == "INELIGIBLE", "resident input disposition must stay INELIGIBLE")
    require(isinstance(row.get("raw_artifacts"), dict) and bool(row["raw_artifacts"]), "raw artifact manifest required")
    require(all(isinstance(key, str) and bool(key) and isinstance(value, str) and len(value) == 64
                and all(char in "0123456789abcdef" for char in value) for key, value in row["raw_artifacts"].items()), "artifact hash schema")
    if row["resource_status"] == "PASS":
        require(all(row["observations"][name]["status"] == "AVAILABLE" for name in
                    ("scheduler_bytes", "peak_credited_bytes", "immutable_cache_allowance_bytes", "daemon_vmhwm_bytes",
                     "store_logical_bytes", "store_allocated_bytes", "overlay_logical_bytes", "overlay_allocated_bytes")),
                "resource PASS requires actual memory and disk observations")
    return row


def validate_campaign(rows, expected_selection_ids=None):
    """No dropped cells, duplicate treatment or resampled control."""
    expected = set(expected_selection_ids or (item["selection_id"] for item in selections()))
    seen = set()
    for row in rows:
        validate_receipt(row)
        require(row["selection_id"] not in seen, "duplicate/resampled selection")
        seen.add(row["selection_id"])
    require(seen == expected, "missing or unexpected registered selection")
    return {"schema": "r7-optimization-campaign-validation-v1", "rows": len(seen), "mode": "exploratory",
            "admission_eligible": False, "cardinality_status": "PASS"}


def check_artifacts(row, root):
    """Verify linked original bytes; never edits an existing receipt."""
    root = Path(root).resolve()
    for relative, expected in row["raw_artifacts"].items():
        path = (root / relative).resolve()
        require(path.is_relative_to(root), "artifact outside evidence root")
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for part in iter(lambda: stream.read(65536), b""):
                digest.update(part)
        require(digest.hexdigest() == expected, "raw artifact hash mismatch")
    return True
