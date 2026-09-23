#!/usr/bin/env python3
"""Run one #231 diagnostic with declared whole-source residency evidence."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import time

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / "benchmark/fs-bench-pro/shared"))
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro"))
import runner  # noqa: E402
import cold  # noqa: E402


def fixed_operation_identity(case, manifest_sha256):
    """Pin only the public stack and scope; transport/session keys stay fresh."""
    seed = b"layerfs-issue237-fixed-operation-v1\0" + case.encode() + bytes.fromhex(manifest_sha256)
    return {"method": "sha256-fixture-case-v1", "case": case,
            "manifest_sha256": manifest_sha256,
            "stack_hex": hashlib.sha256(seed + b"/stack").digest()[:16].hex(),
            "scope_seed_hex": hashlib.sha256(seed + b"/scope").hexdigest()}


def cold_source(source, manifest):
    backend = cold.Residency()
    started = time.monotonic_ns()
    result = {"method": cold.METHOD, "self_check": backend.self_check(),
              "source": str(source), "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "files": 0, "directories": 0, "bytes": 0, "pages": 0, "resident_pages": 0}
    rows = [line.split("\t") for line in manifest.read_text().splitlines()]
    files = []
    for relative, kind, mode, mtime_ns, size, sha in rows:
        path = source / relative
        metadata = path.lstat()
        if (metadata.st_mode & 0o7777 != int(mode) or metadata.st_mtime_ns != int(mtime_ns)
                or (kind == "f" and not stat.S_ISREG(metadata.st_mode))
                or (kind == "d" and not stat.S_ISDIR(metadata.st_mode))):
            raise ValueError(f"source metadata drift: {path}")
        if kind == "d":
            result["directories"] += 1
            continue
        if kind != "f" or metadata.st_size != int(size):
            raise ValueError(f"source file drift: {path}")
        files.append((path, int(size), int(mtime_ns)))
        with path.open("rb") as file:
            digest = hashlib.sha256()
            for block in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(block)
            if digest.hexdigest() != sha:
                raise ValueError(f"source hash drift: {path}")
            os.fsync(file.fileno())
            backend.check(file.fileno(), int(size), evict=True)
    for path, expected, mtime_ns in files:
        with path.open("rb") as file:
            metadata = os.fstat(file.fileno())
            if metadata.st_size != expected or metadata.st_mtime_ns != mtime_ns:
                raise ValueError(f"source changed after invalidation: {path}")
            pages, resident = backend.check(file.fileno(), expected)
            result["files"] += 1
            result["bytes"] += expected
            result["pages"] += pages
            result["resident_pages"] += resident
    result["finished_ns"] = time.monotonic_ns()
    result["wall_ns"] = result["finished_ns"] - started
    result["status"] = "VERIFIED_COLD" if not result["resident_pages"] else "INELIGIBLE"
    return result


def recheck_source(source, manifest):
    backend = cold.Residency()
    started = time.monotonic_ns()
    result = {"method": cold.METHOD, "files": 0, "bytes": 0,
              "pages": 0, "resident_pages": 0}
    for row in (line.split("\t") for line in manifest.read_text().splitlines()):
        if row[1] != "f":
            continue
        path = source / row[0]
        with path.open("rb") as file:
            metadata = os.fstat(file.fileno())
            if (not stat.S_ISREG(metadata.st_mode) or metadata.st_size != int(row[4])
                    or metadata.st_mtime_ns != int(row[3])
                    or metadata.st_mode & 0o7777 != int(row[2])):
                raise ValueError(f"source changed after cold preflight: {path}")
            pages, resident = backend.check(file.fileno(), metadata.st_size)
            result["files"] += 1
            result["bytes"] += metadata.st_size
            result["pages"] += pages
            result["resident_pages"] += resident
    result["finished_ns"] = time.monotonic_ns()
    result["wall_ns"] = result["finished_ns"] - started
    result["status"] = "VERIFIED_COLD" if not result["resident_pages"] else "INELIGIBLE"
    return result


def run_100k(out):
    """Research-only bypass of #231's intentional 100k NOT_RUN selector."""
    case = runner.init.CASES["namespace-100000"]
    if (case.files, case.directories, case.logical_bytes) != (100_000, 1_000, 500_000_000):
        raise ValueError("100k case declaration changed")
    target = runner.target_path()
    identity = runner.identities()
    out.mkdir(parents=True)
    started = time.monotonic_ns()
    built = runner.build(out, target, identity)
    runner.write_json(out / "build.json", built)
    blocked = None if built["status"] == "PASS" else built["status"]
    if blocked is None:
        with (runner.RESULTS / ".run.lock").open("a+b") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                blocked = "same-worktree run active"
            if blocked is None:
                runner._case(out, case, built["binaries"], identity, full_verify=False)
    if blocked:
        runner.write_json(out / "blocked.json", {"status": "NOT_RUN", "reason": blocked, "identity": identity})
        folder = out / "daemon-host/init_namespace" / case.id
        folder.mkdir(parents=True)
        runner.write_json(folder / "receipt.json", {"case": case.id, "status": "NOT_RUN",
                                                    "sample_count": 0, "reason": blocked})
    runner._fill_not_run(out, case.id, blocked)
    runner.write_json(out / "run.json", {"schema": "issue237-native-100k-research-v1",
                                       "selection": case.id, "full_verification_requested": False,
                                       "identity": identity,
                                       "complete_run_wall_ns": time.monotonic_ns() - started})
    (out / "report.txt").write_text(runner.report(out))
    runner.manifest_run(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", choices=("namespace-1000-compact-v3", "namespace-10000", "namespace-100000"), required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--fixed-operation-identity", action="store_true",
                        help="derive one stack and scope seed from the sealed case/manifest")
    args = parser.parse_args()
    out = runner.owned(args.out)
    original_prepare = runner.init.prepare
    original_import = runner.init.public_import
    state = {}

    def prepare(case, root):
        state["fixture"] = original_prepare(case, root)
        folder = out / "daemon-host/init_namespace" / case.id
        try:
            cold_result = cold_source(Path(state["fixture"]["source"]), Path(state["fixture"]["manifest"]))
        except Exception as error:
            cold_result = {"status": "INELIGIBLE", "error": repr(error)}
        (folder / "cold-preflight.json").write_text(json.dumps(cold_result, sort_keys=True, indent=2) + "\n")
        if (cold_result["status"] != "VERIFIED_COLD" or cold_result["files"] != case.files
                or cold_result["bytes"] != case.logical_bytes
                or cold_result["directories"] != case.directories + 1):
            raise ValueError("source cold preflight failed; no timed call")
        state["cold"] = cold_result
        if args.fixed_operation_identity:
            identity = fixed_operation_identity(case.id, state["fixture"]["manifest_sha256"])
            state["operation_identity"] = identity
            (folder / "operation-identity.json").write_text(json.dumps(identity, sort_keys=True, indent=2) + "\n")
        return state["fixture"]

    def public_import(daemon, case, stack, scope_seed):
        cold_result = state["cold"]
        folder = out / "daemon-host/init_namespace" / case.id
        try:
            recheck = recheck_source(Path(state["fixture"]["source"]), Path(state["fixture"]["manifest"]))
        except Exception as error:
            recheck = {"status": "INELIGIBLE", "error": repr(error)}
        (folder / "cold-recheck.json").write_text(json.dumps(recheck, sort_keys=True, indent=2) + "\n")
        if (recheck["status"] != "VERIFIED_COLD" or recheck["files"] != case.files
                or recheck["bytes"] != case.logical_bytes
                or time.monotonic_ns() - recheck["finished_ns"] > cold.MAX_LAUNCH_GAP_NS):
            raise ValueError("source residency recheck failed; no timed call")
        if args.fixed_operation_identity:
            identity = state["operation_identity"]
            stack = bytes.fromhex(identity["stack_hex"])
            scope_seed = bytes.fromhex(identity["scope_seed_hex"])
        result = original_import(daemon, case, stack, scope_seed)
        gap = result["started_ns"] - recheck["finished_ns"]
        (folder / "cold-launch.json").write_text(json.dumps({
            "preflight_sha256": hashlib.sha256((folder / "cold-preflight.json").read_bytes()).hexdigest(),
            "recheck_sha256": hashlib.sha256((folder / "cold-recheck.json").read_bytes()).hexdigest(),
            "preflight_to_timer_ns": result["started_ns"] - cold_result["finished_ns"],
            "launch_gap_ns": gap,
            "status": "VERIFIED_COLD" if gap <= cold.MAX_LAUNCH_GAP_NS else "INELIGIBLE",
        }, sort_keys=True, indent=2) + "\n")
        return result

    runner.init.prepare = prepare
    runner.init.public_import = public_import
    if args.case == "namespace-100000":
        if args.verify:
            parser.error("100k research lane is performance-only")
        run_100k(out)
    else:
        runner.run(args.case, out, full_verify=args.verify)
    print(runner.report(out), end="")


if __name__ == "__main__":
    main()
