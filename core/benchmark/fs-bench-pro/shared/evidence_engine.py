"""Independent bounded validation of direct-engine original family receipts.

This successor never edits or relabels old E01/E04 evidence. It validates actual
finite-operation accounting, not numerical latency/RSS or unobserved internals.
"""
import gzip
import json
from pathlib import Path
from .evidence_jobs import (ALLOCATION, PAYLOAD, STATEMENT, counter_map, database,
                            exact, integer, require)

SCHEMA = "cluster-two-engine-job-v1"
SUMMARY = "cluster-two-engine-summary-v1"
WINDOW = 65536
PHASES = {"setup": 20, "wave": 768, "dependency": 256, "release": 12, "verify_close": 4}


def unique(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "duplicate JSON key: " + key)
        value[key] = item
    return value


def rows(path):
    opener = gzip.open if str(path).endswith(".gz") else open
    with opener(path, "rb") as stream:
        while raw := stream.readline(WINDOW + 1):
            require(len(raw) <= WINDOW and raw.endswith(b"\n"), "bounded complete JSONL record required")
            yield json.loads(raw, object_pairs_hook=unique)


def empty(keys):
    return dict.fromkeys(keys, 0)


def add(total, part):
    for key in total:
        total[key] += part[key]


def validate(path):
    sql = [empty(STATEMENT) for _ in range(14)]
    payload, allocation = empty(PAYLOAD), empty(ALLOCATION)
    classes, waves = [0] * 6, [[0] * 6 for _ in range(4)]
    phases = dict.fromkeys(PHASES, 0)
    count = 0
    summary = None
    for row in rows(Path(path)):
        require(summary is None, "record after terminal summary")
        if row.get("schema") == SUMMARY:
            summary = row
            continue
        exact(row, ("schema", "mode", "attempt_count", "index", "workspace", "class", "phase",
                    "outcome", "sql", "payload", "allocation", "parked_turns",
                    "queue_wait_including_parking_ns", "service_inclusive_ns"), "job envelope")
        require(row["schema"] == SCHEMA and row["mode"] == "diagnostic", "job schema/mode")
        require(type(row["attempt_count"]) is int and row["attempt_count"] == 1, "one original attempt")
        require(type(row["index"]) is int and row["index"] == count, "missing/duplicate original job index")
        ws, cls, phase = row["workspace"], row["class"], row["phase"]
        require(type(ws) is int and 0 <= ws < 4 and type(cls) is int and 0 <= cls < 6,
                "Workspace/class inventory")
        require(phase in PHASES, "unregistered phase")
        require(row["outcome"] == "known_success", "original operation did not succeed")
        require(all(integer(row[key]) for key in ("parked_turns", "queue_wait_including_parking_ns", "service_inclusive_ns")), "invalid scoped clock/count")
        database(row["sql"])
        for total, part in zip(sql, row["sql"]["families"]):
            add(total, part)
        add(payload, counter_map(row["payload"], PAYLOAD, "payload"))
        add(allocation, counter_map(row["allocation"], ALLOCATION, "allocation"))
        classes[cls] += 1
        if phase == "wave":
            waves[ws][cls] += 1
        phases[phase] += 1
        count += 1
    require(summary is not None, "missing terminal summary")
    exact(summary, ("schema", "mode", "numeric_acceptance", "workspaces", "classes", "waves", "jobs", "owner", "closed_namespaces", "peak_queued", "scheduler_bytes", "wave_counts"), "terminal summary")
    require(all(integer(summary[key]) for key in ("workspaces", "classes", "waves", "jobs", "closed_namespaces", "peak_queued", "scheduler_bytes")), "summary count types")

    require(phases == PHASES and count == 1060, "whole-operation job/phase inventory")
    require(waves == [[32] * 6 for _ in range(4)], "finite per-Workspace/class service missing")
    require(summary.get("mode") == "diagnostic" and summary.get("numeric_acceptance") == "OWNER_DEFERRED", "numeric acceptance substitution")
    require(summary.get("jobs") == count and summary.get("wave_counts") == waves and
            summary.get("workspaces") == 4 and summary.get("classes") == 6 and summary.get("waves") == 32,
            "summary differs from original arrivals")
    owner = summary["owner"]
    require(owner.get("scope") == "same-owner-cumulative", "owner observation scope")
    require(all(integer(value) for value in owner["counters"].values()), "owner counter types")
    require(len(owner["completed"]) == 6 and all(integer(value) for value in owner["completed"]), "completion counter types")

    database(owner["sql_foreground"])
    database(owner["sql_maintenance"])
    require(owner["sql_foreground"]["families"] == sql, "original family sums differ from foreground")
    require(owner["payload_foreground"] == payload and owner["allocation_foreground"] == allocation,
            "original payload/allocation sums differ")
    require(owner["completed"] == classes and owner["counters"]["admitted"] == count,
            "original completion/admission inventory")
    require(owner["counters"]["outstanding"] == 0 and owner["counters"]["credited_bytes"] == 0,
            "original results still retain credits")
    require(summary.get("closed_namespaces") == 4, "acknowledged terminal drain missing")
    require(owner["counters"]["maintenance_data_bytes"] == 4 * 32 * 128, "operation-record drain bytes")
    require(owner["counters"]["peak_credited_bytes"] <= 8 * 1024 * 1024, "explicit credit budget")
    require(integer(summary.get("peak_queued")) and summary["peak_queued"] <= 4 * 18,
            "configured queue bound")
    return {"schema": "cluster-two-engine-validation-v1", "functional_count_status": "PASS",
            "jobs": count, "phases": phases, "classes": classes, "wave_counts": waves,
            "closed_namespaces": 4, "foreground_statements": owner["sql_foreground"]["total"]["attempts"],
            "maintenance_statements": owner["sql_maintenance"]["total"]["attempts"],
            "numeric_acceptance": "OWNER_DEFERRED", "continuous_phase_peak": "NOT_CLAIMED",
            "limitations": ["statement families are actual work, not per-fingerprint/visited-page attribution",
                            "SQL/service spans overlap; queue wait includes readiness parking",
                            "payload counters exclude internal SQLite/driver/kernel copies",
                            "this stream alone supplies no physical I/O or phase-residency evidence"]}
