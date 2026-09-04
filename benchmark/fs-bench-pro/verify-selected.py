#!/usr/bin/env python3
"""Run one identity-pinned verification through the shared Docker runner."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import time
import uuid


HERE = Path(__file__).resolve().parent
HARD_LIMIT_SECONDS = 59.0
WORK_LIMIT_SECONDS = 45.0
PUBLICATION_GUARD_SECONDS = 0.25
FAILURE_LOG_LIMIT = 1024 * 1024
STATUSES = {"PASS", "FAIL", "TIMEOUT", "INCOMPLETE"}
BULK_MARKERS = ("..", ",", "*", "[", "]")


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _json_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def _write(path, data):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o444)
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    finally:
        os.close(descriptor)


def _encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def publish_receipt(output, receipt, clock=time.monotonic, stage_writer=_write, linker=os.link):
    """Durably finalize one owned receipt; a late provisional PASS is replaced."""
    output = Path(output)
    final = output / "verification.json"
    staged = output.parent / f".{output.name}.verification-{uuid.uuid4().hex}.pending"
    hard_deadline = receipt.pop("_hard_deadline")
    started = receipt["monotonic_start_seconds"]
    try:
        receipt["monotonic_end_seconds"] = clock()
        receipt["wall_seconds"] = receipt["monotonic_end_seconds"] - started
        stage_writer(staged, _encoded(receipt))
        linker(staged, final)
        published_at = clock()
        receipt["monotonic_end_seconds"] = published_at
        receipt["wall_seconds"] = published_at - started
        if receipt["status"] == "PASS" and published_at >= hard_deadline - PUBLICATION_GUARD_SECONDS:
            receipt["status"] = "TIMEOUT"
            receipt["error"] = "receipt publication reached the hard 59-second limit"
        # The first exclusive link is provisional until its own duration is
        # represented. Only this invocation's inode is removed and finalized.
        final.unlink()
        staged.unlink()
        _write(staged, _encoded(receipt))
        linker(staged, final)
        final_at = clock()
        if receipt["status"] == "PASS" and final_at >= hard_deadline:
            final.unlink()
            staged.unlink()
            receipt["status"] = "TIMEOUT"
            receipt["error"] = "final receipt publication exceeded the hard 59-second limit"
            receipt["monotonic_end_seconds"] = final_at
            receipt["wall_seconds"] = final_at - started
            _write(staged, _encoded(receipt))
            os.link(staged, final)
    finally:
        staged.unlink(missing_ok=True)
    return receipt


def _identity(value):
    if isinstance(value, str):
        return value.strip() or None
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return value
    return None


def validate_selection(args, selection):
    if not isinstance(selection, dict):
        raise ValueError("runner returned no canonical selection metadata")
    family = selection.get("family", selection.get("family_id"))
    case = selection.get("case", selection.get("scenario_id"))
    for label, requested, resolved in (
        ("family", args.family, family), ("case", args.case, case)
    ):
        if not requested or requested == "all" or any(mark in requested for mark in BULK_MARKERS):
            raise ValueError(f"{label} must identify exactly one registry entry")
        if requested != resolved:
            raise ValueError(f"resolved {label} identity does not match the request")
    for key in ("source_identity", "input_identity", "setup_identity"):
        if _identity(selection.get(key)) is None:
            raise ValueError(f"resolved selection is missing exact {key}")
    if _identity(selection.get("seed")) is None and _identity(selection.get("repetition")) is None:
        raise ValueError("resolved selection is missing seed or inherited repetition identity")
    return selection


def _same_identity(old, selected):
    aliases = (("family", "family_id"), ("case", "scenario_id"))
    for canonical, alias in aliases:
        if old.get(canonical, old.get(alias)) != selected.get(canonical, selected.get(alias)):
            return False
    for key in (
        "seed", "repetition", "source_identity", "input_identity", "setup_identity",
        "product_identity", "harness_identity", "image_identity",
    ):
        old_value = old.get(key, old.get("image") if key == "image_identity" else None)
        selected_value = selected.get(key, selected.get("image") if key == "image_identity" else None)
        if old_value != selected_value:
            return False
    old_environment = old.get("environment_identity")
    selected_environment = selected.get("environment_identity")
    if selected_environment is None and selected.get("environment"):
        selected_environment = _json_digest(selected["environment"])
    if old_environment != selected_environment:
        return False
    return True


def reuse_pass(path, selected):
    path = Path(path).resolve()
    if not path.is_file() or path.stat().st_size > FAILURE_LOG_LIMIT:
        raise ValueError("reused verification receipt is missing or unbounded")
    old = json.loads(path.read_text())
    if (
        old.get("schema") != "layerfs-selected-verification-v2"
        or old.get("status") != "PASS"
        or str(old.get("cleanup", {}).get("status", "")).upper() != "PASS"
        or old.get("wall_seconds", HARD_LIMIT_SECONDS) >= HARD_LIMIT_SECONDS
        or not _same_identity(old, selected)
    ):
        raise ValueError("reused PASS does not exactly match the selected identity")
    return {
        "status": "PASS",
        "checks": [{"check": "exact identity-matched verification PASS", "status": "PASS"}],
        "sampled_paths_or_ranges": [],
        "reused_proof_identities": [{"path": str(path), "sha256": sha256(path)}],
        "omissions": ["execution reused an exact identity-matched PASS"],
        "resource_precision": old.get("resource_precision", {}),
        "cleanup": {"status": "PASS", "required": False},
    }


def normalize_result(result):
    if not isinstance(result, dict):
        raise ValueError("shared runner returned no verification result")
    normalized = dict(result)
    status = str(normalized.get("status", "INCOMPLETE")).upper()
    if status not in STATUSES:
        raise ValueError(f"unknown verification status {status!r}")
    cleanup = normalized.get("cleanup")
    if not isinstance(cleanup, dict):
        cleanup = {"status": "INCOMPLETE", "error": "runner omitted cleanup result"}
    cleanup_status = str(cleanup.get("status", "INCOMPLETE")).upper()
    if status == "PASS" and cleanup_status != "PASS":
        status = "INCOMPLETE"
        normalized.setdefault("error", "verification checks passed but cleanup did not")
    normalized["status"] = status
    normalized["cleanup"] = cleanup
    return normalized


def _bounded_result_fields(result):
    return {
        "phase": result.get("phase"),
        "resources": result.get("resources", {}),
        "setup_observation": result.get("setup"),
        "preparation_wall_ns": result.get("preparation_wall_ns"),
        "command_wall_ns": result.get("command_wall_ns"),
    }


def _sanitized_failure(value):
    text = str(value or "verification did not pass")
    text = re.sub(
        r"(?i)(authorization|password|secret|token)(\s*[:=]\s*)(\S+)",
        r"\1\2<redacted>", text,
    )
    encoded = text.encode("utf-8", "replace")
    marker = b"\n...[truncated to 1 MiB]\n"
    if len(encoded) > FAILURE_LOG_LIMIT:
        encoded = encoded[: FAILURE_LOG_LIMIT - len(marker)] + marker
    return encoded


def _failure_log(output, error):
    path = Path(output) / "failure.log"
    _write(path, _sanitized_failure(error))
    return {"path": str(path), "sha256": sha256(path), "bytes": path.stat().st_size}


def _add_reuse_argument(parser):
    if not any("--reuse-pass" in action.option_strings for action in parser._actions):
        parser.add_argument("--reuse-pass", help="reuse one exact identity-matched verification.json PASS")


def _parse(runner, argv):
    parser = runner.build_parser(include_modes=True)
    _add_reuse_argument(parser)
    raw = list(argv)
    if any(flag in raw for flag in ("--perf-fast", "--perf-samples", "--smoke")):
        parser.error("verify-selected.py rejects performance modes and implicit selection")
    if "--verification" not in raw:
        raw.insert(0, "--verification")
    if not any(flag in raw for flag in ("--seed", "--repetition")):
        parser.error("verification requires an explicit --seed or --repetition identity")
    args = parser.parse_args(raw)
    if not args.case or not args.source or not args.input or not args.image:
        parser.error("verification requires exact case, source, input, and image identities")
    if any(value == "all" or any(mark in value for mark in BULK_MARKERS)
           for value in (args.family, args.case)):
        parser.error("verification accepts exactly one family/case, never a range or expansion")
    if args.seed is not None and args.repetition is not None:
        parser.error("verification accepts a seed or inherited repetition, not both")
    if getattr(args, "list", False) or getattr(args, "prepare_only", False):
        parser.error("verification cannot list or prepare a matrix")
    return args


def run(runner, argv=None, clock=time.monotonic, publisher=publish_receipt):
    started = clock()
    work_deadline = started + WORK_LIMIT_SECONDS
    hard_deadline = started + HARD_LIMIT_SECONDS
    args = _parse(runner, sys.argv[1:] if argv is None else argv)
    output = Path(args.output).resolve()
    if output.exists():
        raise SystemExit("output already exists; verification receipts are immutable")
    output.mkdir(parents=True)
    status = "INCOMPLETE"
    error = None
    selected = None
    result = {"cleanup": {"status": "INCOMPLETE", "required": True}}
    lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
    lock = lock_path.open("a")
    try:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as cause:
            raise RuntimeError("another benchmark owns the measurement lock") from cause
        selected = validate_selection(args, runner.resolve_selection(args, deadline=work_deadline))
        if selected.get("verification_supported") is False:
            result = {
                "status": "INCOMPLETE",
                "checks": [],
                "sampled_paths_or_ranges": [],
                "reused_proof_identities": [],
                "omissions": [selected.get("unsupported_reason") or "selected proof is unsupported by bounded verification"],
                "resource_precision": {},
                "cleanup": {"status": "PASS", "required": False},
            }
        elif clock() >= work_deadline:
            raise TimeoutError("selection authentication consumed the 45-second work allowance")
        else:
            reused = getattr(args, "reuse_pass", None)
            result = reuse_pass(reused, selected) if reused else runner.execute_selected(
                args, deadline=work_deadline, verification=True
            )
        result = normalize_result(result)
        status, error = result["status"], result.get("error")
    except TimeoutError as cause:
        status, error = "TIMEOUT", str(cause)
        result = normalize_result({"status": status, "cleanup": result.get("cleanup", {})})
    except Exception as cause:
        status, error = "INCOMPLETE", f"{type(cause).__name__}: {cause}"
        result = normalize_result({"status": status, "cleanup": result.get("cleanup", {})})

    now = clock()
    if now >= hard_deadline:
        status, error = "TIMEOUT", error or "hard 59-second end-to-end limit reached"
    if status != "PASS":
        try:
            failure = _failure_log(output, error or result.get("error") or status)
        except Exception as cause:
            failure = None
            status, error = "INCOMPLETE", f"failure log publication failed: {cause}"
    else:
        failure = None

    selected = selected or {
        "family": getattr(args, "family", None), "case": getattr(args, "case", None),
        "seed": getattr(args, "seed", None), "repetition": getattr(args, "repetition", None),
        "source_identity": getattr(args, "source", None),
        "input_identity": getattr(args, "input", None),
        "setup_identity": getattr(args, "setup", None),
    }
    environment = selected.get("environment")
    receipt = {
        "schema": "layerfs-selected-verification-v2",
        "companion_sha256": sha256(__file__),
        "status": status,
        "family": selected.get("family", selected.get("family_id")),
        "case": selected.get("case", selected.get("scenario_id")),
        "seed": selected.get("seed"),
        "repetition": selected.get("repetition"),
        "source_identity": selected.get("source_identity"),
        "input_identity": selected.get("input_identity"),
        "setup_identity": selected.get("setup_identity"),
        "product_identity": selected.get("product_identity"),
        "harness_identity": selected.get("harness_identity"),
        "image_identity": selected.get("image_identity", selected.get("image")),
        "environment_identity": selected.get("environment_identity") or (_json_digest(environment) if environment else None),
        "recipe_route": selected.get("recipe_route", selected.get("route", selected.get("operation"))),
        "checks": result.get("records", result.get("checks", [])),
        "sampled_paths_or_ranges": result.get("sampled_paths_or_ranges", []),
        "reused_proof_identities": result.get("reused_proof_identities", []),
        "omissions": ["no exhaustive Phase 1 replay", *result.get("omissions", [])],
        "resource_precision": result.get("resource_precision", {}),
        **_bounded_result_fields(result),
        "cleanup": result.get("cleanup", {"status": "INCOMPLETE"}),
        "monotonic_start_seconds": started,
        "work_deadline_seconds": work_deadline,
        "hard_limit_seconds": HARD_LIMIT_SECONDS,
        "evidence_path": str(output / "verification.json"),
        "failure_log": failure,
        "error": error,
        "_hard_deadline": hard_deadline,
    }
    try:
        receipt = publisher(output, receipt, clock=clock)
    finally:
        lock.close()
    return 0 if receipt["status"] == "PASS" else 1


def main():
    try:
        from shared import runner
    except ImportError as cause:
        raise SystemExit(f"shared runner is unavailable: {cause}") from cause
    return run(runner)


if __name__ == "__main__":
    sys.exit(main())
