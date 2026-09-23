"""Exact LFT1 ingestion from two stderr producers and receipt re-derivation."""
import hashlib
import json
from pathlib import Path

KINDS = {"operation", "resource", "resource-unavailable", "run-summary"}
VERSION = "lft1-ingest-v1"


def _tree(node, errors, label):
    if not isinstance(node, dict) or not isinstance(node.get("name"), str) or not isinstance(node.get("elapsed_ns"), int) or node["elapsed_ns"] < 0:
        errors.append(f"{label}: malformed timing node")
        return
    if node.get("incomplete") is True:
        errors.append(f"{label}: incomplete timing tree")
    children = node.get("children", [])
    if not isinstance(children, list):
        errors.append(f"{label}: malformed timing children")
        return
    for child in children:
        _tree(child, errors, label)


def _timing(event):
    value = event.get("timing")
    return value if isinstance(value, dict) else {}


def _covered(event):
    fields = ("first_ns", "opened_ns", "closed_ns", "last_ns")
    return (all(isinstance(event.get(key), int) for key in fields)
            and event["first_ns"] <= event["opened_ns"] <= event["closed_ns"] <= event["last_ns"]
            and event.get("gaps") == 0)


def inspect(raw: bytes, producers: list[dict], run_hex: str, expected: int = 1,
            labels: list[str] | None = None, allow_final_failure: bool = False):
    """Validate retained bytes; every producer has one run summary and exact calls."""
    errors = []
    operations = {}
    summary = {}
    events = {}
    for producer in producers:
        role = producer["role"]
        name = producer["name"]
        part = raw[producer["start"]:producer["end"]]
        if hashlib.sha256(part).hexdigest() != producer["sha256"] or len(part) != producer["bytes"]:
            errors.append(f"{name}: source range/hash mismatch")
        if part and not part.endswith(b"\n"):
            errors.append(f"{name}: truncated last line")
        parsed = []
        for line in part.splitlines(keepends=True):
            if not line.startswith(b"LFT1 ") or not line.endswith(b"\n"):
                errors.append(f"{name}: malformed LFT1 line")
                continue
            try:
                event = json.loads(line[5:])
            except (ValueError, UnicodeDecodeError):
                errors.append(f"{name}: invalid LFT1 JSON")
                continue
            if not isinstance(event, dict):
                errors.append(f"{name}: LFT1 event is not an object")
                continue
            if (event.get("v") != 1 or event.get("kind") not in KINDS or
                    event.get("run") != run_hex or event.get("role") != role or
                    event.get("namespace") != producer["namespace"] or event.get("pid") != producer["pid"]):
                errors.append(f"{name}: event identity/schema mismatch")
            parsed.append(event)
            if event.get("kind") == "operation":
                _tree(event.get("timing"), errors, name)
        operations[name] = [event for event in parsed if event.get("kind") == "operation"]
        summary[name] = [event for event in parsed if event.get("kind") == "run-summary"]
        events[name] = parsed
        wanted = labels if labels is not None else ["HistoryCommand"] * expected
        if [_timing(op).get("name") for op in operations[name]] != wanted:
            errors.append(f"{name}: operation labels/cardinality mismatch")
        for index, op in enumerate(operations[name]):
            if op.get("success") is not True and not (allow_final_failure and index == len(wanted) - 1):
                errors.append(f"{name}: unsuccessful operation event")
        if len(summary[name]) != 1:
            errors.append(f"{name}: missing/duplicate run summary")
        elif any(summary[name][0].get(field) != 0 for field in ("dropped", "failed")) or summary[name][0].get("overflow") is not False:
            errors.append(f"{name}: telemetry loss")
    roots = {name: [{"label": _timing(op).get("name"), "elapsed_ns": _timing(op).get("elapsed_ns"),
                     "resource_status": op.get("resource_status"), "cpu_shared_ns": op.get("cpu_shared_ns"),
                     "sampled_max_rss": op.get("sampled_max_rss"), "first_ns": op.get("first_ns"),
                     "last_ns": op.get("last_ns"), "opened_ns": op.get("opened_ns"),
                     "closed_ns": op.get("closed_ns"), "samples": op.get("samples"),
                     "gaps": op.get("gaps"), "largest_gap_ns": op.get("largest_gap_ns"),
                     "clock": op.get("clock", "local-process"), "boundary_covered": _covered(op)}
                    for op in group] for name, group in operations.items()}
    service = operations.get("service", [])
    children = _timing(service[0]).get("children", []) if service else []
    return {"status": "PASS" if not errors else "INCOMPLETE", "parser": VERSION,
            "errors": errors, "event_counts": {name: len(group) for name, group in events.items()},
            "operation_counts": {name: len(group) for name, group in operations.items()},
            "m2_local_roots": roots, "m3_service_children": children,
            "m4_c5_duration_ns": None, "m4_reason": "no dedicated C5 timing child"}


def ingest(captures: list[dict], destination: Path, run_hex: str, expected: int = 1,
           labels: list[str] | None = None, allow_final_failure: bool = False):
    raw = bytearray()
    producers = []
    diagnostics = {}
    errors = []
    for capture in captures:
        stderr = Path(capture["path"]).read_bytes()
        if len(stderr) > 16 * 1024 * 1024:
            errors.append(f"{capture['name']}: stderr capture exceeded 16 MiB")
        lines = []
        other = []
        for line in stderr.splitlines(keepends=True):
            (lines if line.startswith(b"LFT1 ") else other).append(line)
        part = b"".join(lines)
        start = len(raw)
        raw.extend(part)
        producers.append({key: capture[key] for key in ("name", "pid", "role", "namespace")}
                         | {"start": start, "end": len(raw), "bytes": len(part),
                            "sha256": hashlib.sha256(part).hexdigest(), "lines": len(lines)})
        diagnostics[capture["name"]] = b"".join(other)[:4096].decode(errors="replace")
        if stderr and not stderr.endswith(b"\n"):
            errors.append(f"{capture['name']}: stderr capture truncated")
    destination.write_bytes(raw)
    parsed = inspect(bytes(raw), producers, run_hex, expected, labels, allow_final_failure)
    parsed["errors"].extend(errors)
    if errors:
        parsed["status"] = "INCOMPLETE"
    parsed.update({"file": destination.name, "sha256": hashlib.sha256(raw).hexdigest(),
                   "producers": producers, "diagnostics": diagnostics})
    return parsed
