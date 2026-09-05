#!/usr/bin/env python3
"""Selected Docker/Linux benchmarks. One compact log, independent samples."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
REPO = BENCH.parent.parent
sys.path.insert(0, str(HERE))
import runtime


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def harness_identity():
    paths = [HERE / "runner.py", HERE / "runtime.py", BENCH / "verify-selected.py"]
    return digest({str(path.relative_to(BENCH)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths})


def build_parser(include_modes=True):
    p = argparse.ArgumentParser(description="Full Docker/FUSE workloads; fast means one full sample, never reduced work.")
    p.add_argument("--family", required=True)
    p.add_argument("--case")
    p.add_argument("--seed", type=int)
    p.add_argument("--repetition", type=int)
    p.add_argument("--setup", choices=("fresh", "clone"))
    if include_modes:
        mode = p.add_mutually_exclusive_group()
        mode.add_argument("--perf-fast", action="store_true")
        mode.add_argument("--perf-samples", type=int)
        mode.add_argument("--verification", action="store_true")
    p.add_argument("--prepare-only", action="store_true")
    p.add_argument("--smoke", action="store_true", help="Select the smallest registered supported case; performance/setup only")
    p.add_argument("--list", action="store_true", help="Print registered selections without running workloads")
    p.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    p.add_argument("--source", "--source-identity", dest="source")
    p.add_argument("--input", "--input-identity", dest="input")
    p.add_argument("--output", default=str(REPO / "benchmark-results" / "infra" / ("run-" + uuid.uuid4().hex[:12])))
    p.add_argument("--timeout", type=float, default=15, help="Selected product command budget in seconds; slow cases stop, not scale up")
    p.add_argument("--setup-timeout", type=float, default=120)
    p.add_argument("--cpus", type=int, default=2)
    p.add_argument("--memory-mib", type=int, default=2048)
    return p


def _deadline(end):
    return runtime.Deadline(end)


def _command(argv, end, **kw):
    return runtime.run(argv, deadline=_deadline(end), **kw)


def _text(value):
    return value.decode() if isinstance(value, bytes) else value


def records(output):
    result = []
    for line in _text(output).splitlines():
        start = line.find("{")
        if start >= 0:
            try:
                value = json.loads(line[start:])
                if isinstance(value, dict):
                    result.append(value)
            except json.JSONDecodeError:
                pass
    return result


def initialization_diagnostics(output):
    return [{"kind": "initialization-debug-text", "details": line}
            for line in _text(output).splitlines()
            if line.startswith(("layerfs-initialization-diagnostic-",
                                "layerfs-initialization-producer-",
                                "layerfs-initialization-commits-"))]


def source_build_args():
    def git(*argv):
        return _text(runtime.run(["git", "-C", str(REPO), *argv], deadline=runtime.Deadline.after(10)).stdout).strip()
    paths = sorted(p for root in (REPO / "crates", REPO / "tools", BENCH)
                   for p in root.rglob("*") if p.is_file() and p.suffix in (".rs", ".toml", ".sh", ".py")
                   and "target" not in p.parts and "__pycache__" not in p.parts)
    paths += [REPO / "Cargo.toml", REPO / "Cargo.lock", BENCH / "Dockerfile.layerfs"]
    source = hashlib.sha256()
    product = hashlib.sha256()
    for path in paths:
        part = str(path.relative_to(REPO)).encode() + b"\0" + path.read_bytes()
        source.update(part)
        if "crates" in path.relative_to(REPO).parts:
            product.update(part)
    return {"LAYERFS_SOURCE_COMMIT": git("rev-parse", "HEAD"),
            "LAYERFS_SOURCE_TREE": git("rev-parse", "HEAD^{tree}"),
            "LAYERFS_SOURCE_DIRTY": "true", "LAYERFS_SOURCE_SEAL": source.hexdigest(),
            "LAYERFS_PRODUCT_SEAL": product.hexdigest(),
            "WORKLOAD_SOURCE_SHA256": hashlib.sha256((BENCH / "workload.rs").read_bytes()).hexdigest()}


def image_info(image, deadline):
    value = json.loads(_text(_command(["docker", "image", "inspect", image], deadline).stdout))[0]
    if value.get("Config", {}).get("Volumes"):
        raise ValueError("image-declared volumes are forbidden")
    return value


def resolve_selection(args, deadline):
    if getattr(args, "_selection", None):
        return args._selection
    if not args.image:
        raise ValueError("select a built Linux image with --image or LAYERFS_BENCH_IMAGE; use shared/runner.py --build-image separately")
    if (not 1 <= args.cpus <= 8 or args.memory_mib <= 0
            or not math.isfinite(args.timeout) or args.timeout <= 0
            or not math.isfinite(args.setup_timeout) or args.setup_timeout <= 0):
        raise ValueError("invalid resource/budget selection")
    if getattr(args, "perf_samples", None) is not None and args.perf_samples <= 0:
        raise ValueError("--perf-samples must be a positive integer")
    info = image_info(args.image, deadline)
    identity = info.get("Config", {}).get("Labels", {}) or {}
    source = identity.get("dev.layerfs.source-seal")
    if not source:
        raise ValueError("image lacks source seal")
    result = _command(["docker", "run", "--rm", "--network", "none", "--cpus", "1", "--memory", "256m",
                       "--entrypoint", "/usr/local/bin/fs-benchmark-pro", info["Id"], "infra-list", args.family]
                      + ([args.case] if args.case else []), deadline)
    rows = [row for row in records(result.stdout) if row.get("family_id") == args.family]
    if not rows:
        raise ValueError("unknown or archival family: " + args.family)
    if args.list:
        return {"rows": rows, "source_identity": source, "image": info["Id"]}
    if not args.case and args.smoke:
        rows = [row for row in rows if row.get("supported", True) and row.get("smoke_supported", True)]
        if not getattr(args, "verification", False):
            rows = [row for row in rows if not row.get("proof_only")]
        rows.sort(key=lambda r: (r.get("fixture_bytes") or 0, r.get("tier") or 0, r["scenario_id"]))
        if rows:
            args.case = rows[0]["scenario_id"]
        else:
            raise ValueError("no bounded low-tier smoke case: explicit large/proof selections are not run automatically")
    matches = [row for row in rows if row.get("scenario_id") == args.case]
    if len(matches) != 1:
        raise ValueError("select exactly one registered --case (or --smoke for performance)")
    row = matches[0]
    if not row.get("supported", True):
        raise ValueError(row.get("unsupported_reason", "historical/unsupported selection"))
    if args.seed is not None and args.repetition is not None:
        raise ValueError("choose seed or inherited repetition, not both")
    inherited = row.get("inherited", row.get("route") == "sdk" or args.family == "edit_length_changing_capped")
    if inherited and args.seed is not None:
        raise ValueError("inherited cases use --repetition, not --seed")
    if not inherited and args.repetition is not None:
        raise ValueError("this case uses --seed, not --repetition")
    seed = args.repetition if args.repetition is not None else (args.seed if args.seed is not None else 1)
    if not row.get("seed_min", 1) <= seed <= row.get("seed_max", 3):
        raise ValueError("invalid registered seed/repetition")
    fresh = row.get("setup_policy") == "fresh-output"
    if fresh and args.setup == "clone":
        raise ValueError("initialization requires a fresh output Store; clone is not applicable")
    setup = "fresh-output" if fresh else (args.setup or "clone")
    input_identity = digest({"family": args.family, "case": args.case, "seed": seed,
                             "source": source, "recipe": row})
    if args.source and args.source != source:
        raise ValueError("selected source identity does not match image")
    if args.input and args.input != input_identity:
        raise ValueError("selected input identity does not match recipe")
    selection = {**row, "family": args.family, "case": args.case, "seed": seed,
                 "repetition": args.repetition, "source_identity": source,
                 "input_identity": input_identity, "setup_identity": setup,
                 "image": info["Id"], "runtime_image": info["Id"],
                 "product_identity": identity.get("dev.layerfs.product-seal"),
                 "harness_identity": harness_identity(), "environment": {"os": info.get("Os"), "architecture": info.get("Architecture")},
                 "verification_supported": row.get("verification_supported", True)}
    args._selection = selection
    return selection


def _acquire(args, selection, deadline):
    key = digest({"input": selection["input_identity"], "runtime": selection["image"], "setup": selection["setup_identity"]})
    fresh = selection["setup_identity"] == "fresh"
    if fresh:
        key = digest({"key": key, "one_shot": uuid.uuid4().hex})
    cache = runtime.PreparedCache("layerfs-bench-infra")
    labels = {"family": selection["family"], "source": selection["source_identity"], "input": selection["input_identity"]}
    prepared = runtime.create_prepared_image(selection["image"], key,
        ["infra-prepare", selection["family"], selection["case"], str(selection["seed"]), "/var/lib/fs-bench/prepared"],
        labels, deadline=_deadline(deadline), cache=cache, retain=not fresh)
    return prepared


def cgroup_snapshot(sample, deadline):
    command = _command(["docker", "exec", sample.id, "sh", "-c",
        "cat /sys/fs/cgroup/cpu.stat; printf 'memory_peak '; cat /sys/fs/cgroup/memory.peak; "
        "printf 'memory_current '; cat /sys/fs/cgroup/memory.current; "
        "printf 'swap_current '; cat /sys/fs/cgroup/memory.swap.current; "
        "cat /sys/fs/cgroup/memory.events"], deadline)
    result = {}
    for line in _text(command.stdout).splitlines():
        fields = line.split()
        if len(fields) == 2:
            result[fields[0]] = int(fields[1])
    return result


def execute_selected(args, *, deadline, verification=False):
    """No receipt files here: the caller publishes after this function cleans up."""
    started = time.monotonic_ns()
    selection = resolve_selection(args, deadline)
    if selection.get("proof_only") and not verification and not args.prepare_only:
        raise ValueError("proof-only case does not support performance")
    if verification and not selection["verification_supported"]:
        return {"status": "INCOMPLETE", "identities": selection, "omissions": ["proof cannot fit bounded verification"], "cleanup": {"status": "PASS", "not_started": True}}
    sample = None
    sample_name = None
    prepared = None
    result = {"status": "INCOMPLETE", "identities": selection, "checks": [], "omissions": [],
              "phase": "preparation",
              "sampled_paths_or_ranges": [], "reused_proof_identities": [],
              "resource_precision": "sample-container lifetime peak; command-window CPU/IO deltas"}
    work_end = deadline - 4
    try:
        setup_started = time.monotonic_ns()
        prepared = _acquire(args, selection, work_end)
        result["preparation"] = prepared
        if args.prepare_only:
            result["status"] = "PASS"
            return result
        name = "layerfs-infra-sample-" + uuid.uuid4().hex[:12]
        sample_name = name
        result["phase"] = "runtime-start"
        sample = runtime.start_sample(prepared["image"], name,
            {"family": selection["family"], "run": name}, deadline=_deadline(work_end),
            cpus=args.cpus, memory_bytes=args.memory_mib * 1024**2)
        result["phase"] = "sample-setup"
        result["setup"] = runtime.prepare_sample(sample, mode="clone" if selection["setup_identity"] == "fresh" else selection["setup_identity"], deadline=_deadline(work_end),
            reuse_prepared_input=selection["family"] in ("dedup_cross_file", "dedup_cdc_locality"))
        result["preparation_wall_ns"] = time.monotonic_ns() - setup_started
        before = cgroup_snapshot(sample, work_end)
        run_started = time.monotonic_ns()
        result["phase"] = "product-command"
        command_end = min(work_end, time.monotonic() + (45 if verification else args.timeout))
        command_env = {"LAYERFS_V013_IMAGE": selection["image"]}
        if selection["family"] in ("dedup_cross_file", "dedup_cdc_locality"):
            command_env["LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE"] = selection["input_identity"][:16]
        command = sample.exec_coordinator(["infra-run", selection["family"], selection["case"], str(selection["seed"]),
                     "verify" if verification else "performance", "/var/lib/fs-bench/sample", sample.id],
                     deadline=_deadline(command_end), output_limit=1024**2,
                     env=command_env)
        result["command_wall_ns"] = time.monotonic_ns() - run_started
        result["records"] = records(command.stdout)
        result["records"].extend(initialization_diagnostics(command.stderr))
        if command.truncated:
            raise RuntimeError("selected command output exceeded compact receipt limit")
        result["phase"] = "resource-finalization"
        after = cgroup_snapshot(sample, work_end)
        result["resources"] = {"command_window_cpu_ns": (after["usage_usec"] - before["usage_usec"]) * 1000,
            "sample_container_lifetime_peak_bytes": after["memory_peak"],
            "memory_current_bytes": after["memory_current"], "swap_current_bytes": after["swap_current"],
            "oom_kill_delta": after.get("oom_kill", 0) - before.get("oom_kill", 0),
            "measurement_scope": "inclusive container command window including coordinator; not operation-only peak"}
        result["status"] = "PASS" if command.returncode == 0 and result["records"] and not result["resources"]["oom_kill_delta"] else "FAIL"
        result["slow"] = result["command_wall_ns"] >= 5_000_000_000
        result["checks"] = [r for r in result["records"] if "verif" in str(r.get("kind", "")) or "proof" in str(r.get("kind", ""))]
        if result["status"] != "PASS":
            result["error"] = _text(command.stderr)[-8192:]
        else:
            result["phase"] = "complete"
    except Exception as error:
        result["status"] = "TIMEOUT" if isinstance(error, TimeoutError) or "timeout" in str(error).lower() or "deadline" in str(error).lower() else "FAIL"
        failed_command = getattr(error, "result", None)
        detail = _text(failed_command.stderr)[-8192:] if failed_command is not None else ""
        result["error"] = (str(error) + "\n" + detail)[-8192:]
        if failed_command is not None:
            result.setdefault("records", records(failed_command.stdout))
            if result["phase"] == "product-command":
                result["command_wall_ns"] = failed_command.wall_ns
        result["slow"] = result["status"] == "TIMEOUT"
    finally:
        cleanup_started = time.monotonic_ns()
        try:
            if sample is not None:
                sample.remove(deadline=_deadline(deadline))
            elif sample_name is not None:
                remaining = _command(["docker", "container", "inspect", sample_name], deadline, check=False)
                if remaining.returncode == 0:
                    item = json.loads(_text(remaining.stdout))[0]
                    labels = item.get("Config", {}).get("Labels") or {}
                    if labels.get(runtime.OWNER_LABEL) != runtime.OWNER or labels.get("run") != sample_name:
                        raise RuntimeError("startup cleanup refused mismatched ownership")
                    _command(["docker", "rm", "--force", item["Id"]], deadline)
                elif "No such" not in _text(remaining.stderr):
                    raise RuntimeError("cannot confirm failed-start container cleanup")
            if prepared and prepared.get("one_shot"):
                _command(["docker", "image", "rm", prepared.get("cache_tag", prepared["image"])], deadline)
            result["cleanup"] = {"status": "PASS", "wall_ns": time.monotonic_ns() - cleanup_started}
        except Exception as error:
            result["cleanup"] = {"status": "FAIL", "error": str(error)[-2048:]}
            result["status"] = "INCOMPLETE"
        result["wall_ns"] = time.monotonic_ns() - started
    return result


def _timer(row):
    for record in reversed(row.get("records", [])):
        keys = ("layerstack_init_ns",) if row.get("identities", {}).get("route") == "namespace" else (
            "edit_commit_ns", "edit_commit_end_ns", "pure_call_sum_ns", "initialize_ns", "execution_ns", "complete_ns", "complete_lifecycle_ns")
        for key in keys:
            if isinstance(record.get(key), (float, int)):
                return key, record[key]
    return "command_wall_ns", row.get("command_wall_ns")


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv == ["--build-image"]:
        lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
        with lock_path.open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError("another benchmark owns the measurement lock") from error
            values = source_build_args()
            tag = "layerfs-bench-infra:" + values["LAYERFS_SOURCE_SEAL"][:16]
            try:
                result = runtime.build_image(REPO, tag, values, deadline=runtime.Deadline.after(900), jobs=2)
            except runtime.CommandFailure as error:
                print(_text(error.result.stderr)[-16384:], file=sys.stderr)
                return error.result.returncode or 1
        if result.returncode:
            print(_text(result.stderr)[-16384:], file=sys.stderr)
            return result.returncode
        print(tag)
        return 0
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.verification:
        parser.error("use the family verify.sh or verify-selected.py for bounded verification")
    if args.prepare_only and (args.perf_fast or args.perf_samples is not None):
        parser.error("preparation-only cannot also select a performance mode")
    # ponytail: one process lock serializes this benchmark; no concurrent resource-sensitive samples.
    lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
    with lock_path.open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("another benchmark owns the measurement lock")
        selection = resolve_selection(args, time.monotonic() + 20)
        if args.list:
            print(json.dumps(selection, sort_keys=True))
            return 0
        if args.prepare_only:
            result = execute_selected(args, deadline=time.monotonic() + args.setup_timeout)
            print(json.dumps(result, sort_keys=True))
            return 0 if result["status"] == "PASS" else 1
        if selection.get("proof_only"):
            parser.error("proof-only case cannot run performance")
        count = args.perf_samples or 1
        output = Path(args.output)
        output.mkdir(parents=True, exist_ok=True)
        path = output / "perf.jsonl"
        samples = []
        with path.open("x") as stream:
            def emit(row):
                stream.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
                stream.flush()
            emit({"kind": "header", "schema": "layerfs-perf-v1", "identities": selection,
                  "requested_samples": count, "full_workload": True, "cpus": args.cpus,
                  "memory_mib": args.memory_mib, "verification_status": "NOT_RUN"})
            for index in range(1, count + 1):
                row = execute_selected(args, deadline=time.monotonic() + args.setup_timeout + args.timeout + 10)
                row.update(kind="sample", sample_index=index)
                emit(row)
                samples.append(row)
                key, value = _timer(row)
                print(f"{args.family} {args.case} sample={index} {row['status']} {key}={value} slow={row.get('slow', False)}", flush=True)
                if row["status"] != "PASS":
                    (output / "failure.log").write_text(str(row.get("error", row.get("cleanup")))[:1024**2])
                    break
            valid = [row for row in samples if row["status"] == "PASS"]
            times = [value for row in valid if (value := _timer(row)[1]) is not None]
            summary = {"kind": "summary", "requested": count, "attempted": len(samples), "valid": len(valid),
                       "status": "PASS" if len(valid) == count else "INCOMPLETE", "verification_status": "NOT_RUN",
                       "timer": _timer(valid[0])[0] if valid else None,
                       "median_ns": statistics.median(times) if times else None,
                       "min_ns": min(times) if times else None, "max_ns": max(times) if times else None}
            emit(summary)
        return 0 if summary["status"] == "PASS" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, TimeoutError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
