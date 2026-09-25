"""Check #241 v4 caller and daemon LFT1 from the retained, complete stderr bytes."""

import hashlib
import json
import re

from shared.telemetry import _covered, _tree


def _resource(event):
    status = event.get("resource_status")
    sampled = status == "sampled" and type(event.get("samples")) is int and event["samples"] > 0
    cpu = event.get("cpu_shared_ns") if sampled else None
    rss = event.get("sampled_max_rss") if sampled else None
    return {
        "status": "SAMPLED" if sampled else "UNAVAILABLE",
        "source_status": status,
        "scope": event.get("scope"),
        "clock": event.get("clock"),
        "samples": event.get("samples"),
        "gaps": event.get("gaps"),
        "largest_gap_ns": event.get("largest_gap_ns"),
        "boundary_covered": _covered(event) if sampled else False,
        "cpu_shared_ns": cpu,
        "cpu_status": "SAMPLED" if isinstance(cpu, list) and len(cpu) == 2
        and all(type(part) is int and part >= 0 for part in cpu) else "UNAVAILABLE",
        "sampled_max_rss": rss,
        "rss_status": "SAMPLED" if type(rss) is int and rss > 0 else "UNAVAILABLE",
    }


def _timing(event):
    value = event.get("timing")
    return value if isinstance(value, dict) else {}


def _children(event):
    value = _timing(event).get("children")
    return value if isinstance(value, list) and all(isinstance(child, dict) for child in value) else []


def check(raw_stderr: bytes, driver: dict, telemetry_run: str) -> dict:
    """Return source hashes, local coverage, and a fail-closed telemetry verdict."""
    errors = []
    unavailable = []
    partial = []
    if not re.fullmatch(r"[0-9a-f]{32}", telemetry_run or ""):
        errors.append("invalid telemetry run identity")
    sandbox = driver.get("sandbox")
    if not isinstance(sandbox, str) or not re.fullmatch(r"[0-9a-f]{32}", sandbox):
        errors.append("missing/invalid sandbox identity")
        namespace = None
    else:
        namespace = max(int(sandbox[:16], 16), 1)
    caller_pid = driver.get("caller_pid")
    if type(caller_pid) is not int or caller_pid <= 0:
        errors.append("missing/invalid caller pid")
    if driver.get("telemetry_run") != telemetry_run:
        errors.append("driver telemetry run mismatch")
    if (driver.get("daemon_log_attempted") is not True
            or driver.get("daemon_log_truncated") is not False
            or driver.get("daemon_log_error") != "None"
            or type(driver.get("daemon_log_bytes")) is not int
            or driver["daemon_log_bytes"] <= 0):
        errors.append("daemon log capture missing, truncated, or failed")

    lines = {"host": [], "daemon": []}
    events = {"host": [], "daemon": []}
    daemon_pid = None
    for line in raw_stderr.splitlines(keepends=True):
        if not line.startswith(b"LFT1"):
            continue
        if not line.startswith(b"LFT1 ") or not line.endswith(b"\n"):
            errors.append("malformed/truncated LFT1 line")
            continue
        try:
            event = json.loads(line[5:], parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
        except (ValueError, UnicodeDecodeError):
            errors.append("invalid LFT1 JSON")
            continue
        if not isinstance(event, dict) or event.get("role") not in (1, 2):
            errors.append("invalid LFT1 role/schema")
            continue
        producer = "host" if event["role"] == 1 else "daemon"
        lines[producer].append(line)
        events[producer].append(event)
        if (event.get("v") != 1 or event.get("run") != telemetry_run
                or event.get("kind") not in {"operation", "resource", "resource-unavailable", "run-summary"}):
            errors.append(f"{producer}: event schema/run mismatch")
        pid = event.get("pid")
        if producer == "host":
            if event.get("namespace") != 7 or pid != caller_pid:
                errors.append("host: namespace/pid mismatch")
        else:
            if event.get("namespace") != namespace or type(pid) is not int or pid <= 0:
                errors.append("daemon: namespace/pid mismatch")
            if daemon_pid is None:
                daemon_pid = pid
            elif daemon_pid != pid:
                errors.append("daemon: multiple pids")
        if event.get("kind") == "operation":
            _tree(event.get("timing"), errors, producer)
            name = _timing(event).get("name")
            if event.get("success") is not True and not (name == "Inspect" and event.get("success") is False):
                errors.append(f"{producer}: unsuccessful {name} operation")
            resource = _resource(event)
            if resource["status"] == "SAMPLED":
                if (resource["scope"] != "process-shared" or resource["clock"] != "local-monitor"
                        or type(resource["gaps"]) is not int or resource["gaps"] < 0):
                    errors.append(f"{producer}: invalid sampled resource scope/coverage")

    operations = {name: [event for event in group if event.get("kind") == "operation"]
                  for name, group in events.items()}
    summaries = {name: [event for event in group if event.get("kind") == "run-summary"]
                 for name, group in events.items()}
    for producer, group in summaries.items():
        if len(group) != 1:
            errors.append(f"{producer}: missing/duplicate run summary")
        elif (group[0].get("dropped") != 0 or group[0].get("failed") != 0
              or group[0].get("overflow") is not False):
            errors.append(f"{producer}: telemetry loss")
        if any(event.get("kind") == "resource-unavailable" for event in events[producer]):
            unavailable.append(f"{producer}: resource monitor reported unavailable")
    roots = [event for event in operations["host"] if event.get("key") == 232_000]
    if (len(roots) != 1 or roots[0].get("success") is not True
            or _timing(roots[0]).get("name") != "sdk.edit_commit.fuse"
            or [child.get("name") for child in _children(roots[0])]
            != ["edit", "commit"]):
        errors.append("host: missing/invalid edit_commit root and edit/commit children")
    execs = [event for event in operations["daemon"]
             if _timing(event).get("name") == "WorkspaceExec"]
    if len(execs) != 1 or execs[0].get("success") is not True:
        errors.append("daemon: missing/invalid WorkspaceExec evidence")
    if type(driver.get("daemon_log_bytes")) is int and driver["daemon_log_bytes"] < sum(map(len, lines["daemon"])):
        errors.append("daemon LFT1 exceeds reported raw log bytes")

    selected = {"edit_commit": roots[0] if len(roots) == 1 else None,
                "workspace_exec": execs[0] if len(execs) == 1 else None}
    coverage = {}
    for label, event in selected.items():
        coverage[label] = _resource(event) if event else None
        if event:
            resource = coverage[label]
            if resource["status"] != "SAMPLED" or resource["gaps"] != 0:
                unavailable.append(f"{label}: unsampled or gapped resources")
            if resource["cpu_status"] != "SAMPLED" or resource["rss_status"] != "SAMPLED":
                unavailable.append(f"{label}: CPU or RSS unavailable")
            if not resource["boundary_covered"]:
                partial.append(f"{label}: sampled CPU/RSS window does not cover both boundaries")
    children = _children(roots[0]) if len(roots) == 1 else []
    return {
        "status": "INCOMPLETE" if errors else "UNAVAILABLE" if unavailable else "PASS",
        "reasons": errors + unavailable,
        "errors": errors,
        "unavailable": unavailable,
        "partial": partial,
        "raw_sha256": hashlib.sha256(raw_stderr).hexdigest(),
        "raw_bytes": len(raw_stderr),
        "producers": {name: {"sha256": hashlib.sha256(b"".join(group)).hexdigest(),
                             "bytes": sum(map(len, group)), "lines": len(group),
                             "events": len(events[name]),
                             "resource_unavailable_events": sum(event.get("kind") == "resource-unavailable"
                                                                for event in events[name]),
                             "negative_inspect": sum(event.get("success") is False
                                                     and _timing(event).get("name") == "Inspect"
                                                     for event in operations[name]),
                             "pid": caller_pid if name == "host" else daemon_pid}
                      for name, group in lines.items()},
        "coverage": coverage,
        "edit_commit_elapsed_ns": _timing(roots[0]).get("elapsed_ns") if len(roots) == 1 else None,
        "children": {child.get("name"): {"elapsed_ns": child.get("elapsed_ns"),
                                        "cpu_rss_attribution": "UNAVAILABLE; root is process-shared"}
                     for child in children},
        "workspace_exec_elapsed_ns": _timing(execs[0]).get("elapsed_ns") if len(execs) == 1 else None,
    }
