"""One original registered pre-S8 diagnostic child; bounded phase observations.

No performance admission, cache conditioning, automatic retry or lifetime peak.
The existing resource readers retain their supported scopes and limitations.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_resources as resources
from shared.pre_s8_registration import admit, GROWTH_SCHEMA

INTERVAL_NS = 5_000_000
WINDOW = 65536


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        while part := stream.read(WINDOW):
            result.update(part)
    return result.hexdigest()


def fresh_json(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)


def collect(binary, output, database, case, registration, prepared=None, cgroup=None):
    started = time.monotonic_ns()
    binary, output, database = Path(binary).resolve(), Path(output).resolve(), Path(database).resolve()
    growth = case.startswith("growth-")
    if (case not in {"engine-finite", "store-commit"} and not growth) or (case != "engine-finite") != (prepared is not None):
        raise ValueError("exact fixed case/preparation required")
    if os.environ.get("LAYERFS_CONSTRUCTION_WORKERS") != "1":
        raise ValueError("one construction worker required")
    if database.exists():
        raise ValueError("fresh database directory required")
    frozen = json.loads(Path(registration).read_text())
    admitted = admit(frozen, Path(__file__).resolve().parents[4])
    if admitted["status"] != "PASS" or frozen["case"] != case or frozen["binary"] != {"path": str(binary), "sha256": digest(binary)}:
        raise ValueError("original registered selection mismatch: " + str(admitted))
    if prepared is not None and frozen["prepared"] != {name: digest(Path(prepared) / name) for name in frozen["prepared"]}:
        raise ValueError("original closed preparation changed")

    output.mkdir()
    jobs = output / "jobs.jsonl"
    command = [str(binary), str(database), str(jobs)]
    if prepared is not None:
        command.append(str(Path(prepared).resolve()))
    if growth:
        if frozen["schema"] != GROWTH_SCHEMA:
            raise ValueError("growth schema required")
        command.extend(frozen["workload"]["arguments"])
    identity = {"schema": "pre-s8-diagnostic-invocation-v1", "case": case,
                "registration_sha256": digest(registration),
                "mode": "diagnostic", "numeric_acceptance": "OWNER_DEFERRED",
                "admission_eligible": False, "attempt_count": 1,
                "command": command, "binary_sha256": digest(binary),
                "platform": sys.platform, "construction_workers": "1",
                "sampling_interval_ns": INTERVAL_NS, "cache": "natural-functional-no-cold-claim",
                "database_directory": str(database), "sample_count": 0,
                "observer": str(Path(__file__).resolve()), "observer_sha256": digest(__file__),
                "reader_sha256": digest(resources.__file__), "cgroup": cgroup,
                "clock": resources.clock_identity()}
    if prepared is not None:
        identity["prepared"] = {name: digest(Path(prepared) / name) for name in
                                ("prepared.sqlite", "prepared.manifest", "expected-metadata.txt")}
    fresh_json(output / "invocation.json", identity)
    observations = resources.Stream(output / "resources.jsonl")
    stdout = (output / "stdout.txt").open("xb", buffering=0)
    stderr = (output / "stderr.txt").open("xb", buffering=0)
    child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=stderr, bufsize=0)
    selector = selectors.DefaultSelector()
    selector.register(child.stdout, selectors.EVENT_READ)
    phase = None
    sequence = 0
    artifacts = database
    next_sample = time.monotonic_ns()
    phase_order = []
    failure = None
    status = None

    def sample(boundary):
        nonlocal sequence
        process = resources.macos_process(child.pid) if sys.platform == "darwin" else resources.linux_process(child.pid)
        observed = {"schema": "pre-s8-phase-observation-v1", "index": sequence,
                    "phase": phase, "boundary": boundary, "process": process,
                    "cgroup": resources.linux_cgroup(cgroup) if cgroup else None,
                    "artifacts": []}
        for name in ("overlay.sqlite", "store.sqlite", "store.sqlite-wal", "store.sqlite-shm"):
            path = artifacts / name
            if path.exists():
                observed["artifacts"].append(resources.artifact_allocation(path))
            else:
                observed["artifacts"].append({"path": str(path), "status": "ABSENT_AT_OBSERVATION"})
        observations.append(lambda: observed)
        sequence += 1

    try:
        while True:
            now = time.monotonic_ns()
            if now - started >= 15_000_000_000:
                raise TimeoutError("complete diagnostic command exceeds existing15s budget")
            events = selector.select(max(0, min(0.05, (next_sample - now) / 1e9)))
            if events:
                line = child.stdout.readline(WINDOW + 1)
                if not line:
                    break
                if len(line) > WINDOW or not line.endswith(b"\n"):
                    raise ValueError("bounded complete child line required")
                if stdout.write(line) != len(line):
                    raise OSError("original stdout short write")
                text = line.decode("utf-8").strip()
                if text.startswith("STORE_ARTIFACT_DIRECTORY "):
                    candidate = Path(text.split(" ", 1)[1])
                    if not candidate.is_absolute() or not candidate.is_dir():
                        raise ValueError("original artifact directory invalid")
                    artifacts = candidate
                if text.startswith("ENGINE_PHASE "):
                    words = text.split()
                    if len(words) != 3 or words[2] != f"pid={child.pid}":
                        raise ValueError("original phase identity invalid")
                    if phase is not None:
                        sample("final")
                    phase = words[1]
                    phase_order.append(phase)
                    sample("baseline")
                    if child.stdin.write(b"continue\n") != 9:
                        raise OSError("original phase acknowledgement short write")
                    child.stdin.flush()
                    next_sample = time.monotonic_ns() + INTERVAL_NS
                continue
            if phase is not None and phase != "finished" and time.monotonic_ns() >= next_sample:
                sample("interior")
                next_sample = time.monotonic_ns() + INTERVAL_NS
            if child.poll() is not None:
                break
        status = child.wait(timeout=2)
        if status != 0:
            raise RuntimeError(f"original child exit={status}")
        expected = (frozen["workload"]["phases"] if growth else
                    ["startup", "setup", "arrivals", "release", "idle", "stop", "finished"]
                    if case == "engine-finite" else
                    ["clone", "startup", "bind", "write", "commit", "up_to_date", "verify", "release", "idle", "stop", "finished"])
        if phase_order != expected:
            raise ValueError("original phase order incomplete or changed")
    except BaseException as original:
        failure = {"type": type(original).__name__, "message": str(original)}
        if child.poll() is None:
            child.kill()
        status = child.wait(timeout=2)
    finally:
        selector.close()
        child.stdin.close()
        child.stdout.close()
        observations.close()
        stdout.close()
        stderr.close()
    result = {"schema": "pre-s8-diagnostic-outcome-v1", "case": case,
              "status": "RECORDED" if failure is None else "FAILED", "original_failure": failure,
              "child_pid": child.pid, "child_exit": status, "child_joined": True,
              "collector_wall_ns": time.monotonic_ns() - started,
              "admission_eligible": False, "attempt_count": 1, "phases": phase_order,
              "resource_records": observations.records, "artifact_directory": str(artifacts),
              "jobs_sha256": digest(jobs) if jobs.exists() else None,
              "resources_sha256": digest(output / "resources.jsonl"),
              "numeric_acceptance": "OWNER_DEFERRED", "continuous_peak": "NOT_CLAIMED",
              "limitations": ["short phases may have no interior sample; endpoints are not a peak proof",
                              "resource read intervals and observer/phase handshake overhead are included",
                              "macOS ps has no per-file cache, kernel, physical-I/O or pager decomposition",
                              "Linux process/cgroup fields overlap and retain their original kernel scope",
                              "no inferred internal SQLite copies, visited pages or runnable-only wait",
                              "backing artifacts are retained; logical cleanup never means file shrink"]}
    fresh_json(output / "outcome.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--case", required=True)
    parser.add_argument("--registration", required=True)
    parser.add_argument("--prepared")
    parser.add_argument("--cgroup")
    args = parser.parse_args()
    result = collect(args.binary, args.out, args.database, args.case, args.registration, args.prepared, args.cgroup)
    print(json.dumps(result, sort_keys=True, indent=2))
    raise SystemExit(0 if result["status"] == "RECORDED" else 1)
