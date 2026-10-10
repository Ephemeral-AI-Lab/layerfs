"""One real release-daemon lifecycle with thread, mount and resource observations.

The R7 lifecycle (mount, external mutation, independent scoped oracle, Commit,
terminal unmount, fresh mount, the same oracle, terminal unmount) over an
already prepared owned volume, through the R7 runtime's SDK/Sandbox control,
with one observation of the daemon at each phase boundary: before any mount,
while mounted, after Commit, after each Unmounted and Gone. Functional only,
no timing claim, one attempt per operation. The volume is changed by the
Commit, so one prepared volume serves one run.

Judged: the typed outcomes and oracle equality of the R7 proof; per-Workspace
mount and fence threads exist exactly while their Workspace is mounted; the
daemon's thread names after each Unmounted equal those before the first
mount; the kernel mount row is present while mounted and absent afterwards.

Recorded, not judged (phase-boundary samples, not continuous peaks): daemon
RSS, the container's cgroup memory, and the Store and Overlay backing files'
logical and allocated bytes. There is no product observation of native gauges
after Unmounted, so "gauges zero" is not asserted here.
"""
import argparse
import collections
import json
from pathlib import Path
import re
import secrets
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

OVERALL_SECONDS = 60
PER_WORKSPACE = re.compile(r"^layerfs-(mount|fence)-(\d+)$")
STATUS_KEYS = ("VmRSS", "RssAnon", "RssFile", "VmHWM", "Threads")


def parse_snapshot(text):
    """Thread names, daemon memory and backing bytes from one runtime snapshot."""
    section, threads, status, backing, cgroup = None, collections.Counter(), {}, {}, {}
    pending = None
    for line in text.splitlines():
        if line in {"DAEMON_STATUS", "BACKING_BYTES", "OWN_CONTAINER_CGROUP", "DAEMON_THREADS", "CGROUP_CPU"}:
            section, pending = line, None
        elif line.startswith("KERNEL\t"):
            section = None
        elif section == "DAEMON_STATUS":
            key, _, value = line.partition(":")
            if key in STATUS_KEYS:
                status[key] = value.strip()
        elif section == "BACKING_BYTES":
            parts = line.split()
            if len(parts) == 2 and parts[1] == "ABSENT":
                backing[parts[0]] = "ABSENT"
            elif len(parts) == 4:
                backing[parts[0]] = dict(bytes=int(parts[1]), allocated=int(parts[2]) * int(parts[3]))
        elif section == "OWN_CONTAINER_CGROUP":
            if line.startswith("/sys/fs/cgroup/memory.") and not line.endswith("UNAVAILABLE"):
                pending = line.rsplit("/", 1)[1]
            elif pending in {"memory.current", "memory.peak"} and line.isdigit():
                cgroup[pending] = int(line)
            elif pending == "memory.stat" and line.split()[0] in {"anon", "file", "kernel", "shmem"}:
                cgroup["memory.stat." + line.split()[0]] = int(line.split()[1])
        elif section == "DAEMON_THREADS" and line.startswith("THREAD\t"):
            threads[line.split("\t")[2]] += 1
    return dict(threads=dict(sorted(threads.items())), status=status, backing=backing, cgroup=cgroup)


def per_workspace(threads):
    return sorted(name for name in threads if PER_WORKSPACE.match(name))


def judge(points):
    """Thread and mount-row failures over the ordered observation points."""
    failures = []
    first = points[0]
    if per_workspace(first["threads"]):
        failures.append("a per-Workspace thread exists before any mount")
    if first["mount_rows"]:
        failures.append("a Workspace mount row exists before any mount")
    for point in points[1:]:
        names = per_workspace(point["threads"])
        if point["mounted"]:
            kinds = sorted(PER_WORKSPACE.match(name).group(1) for name in names)
            numbers = {PER_WORKSPACE.match(name).group(2) for name in names}
            if kinds != ["fence", "mount"] or len(numbers) != 1 or any(
                    point["threads"][name] != 1 for name in names):
                failures.append(point["label"] + ": not exactly one mount and one fence thread")
            if point["mount_rows"] != [point["directory"]]:
                failures.append(point["label"] + ": the kernel mount row is not exactly the Workspace")
        else:
            if point["threads"] != first["threads"]:
                failures.append(point["label"] + ": daemon threads differ from those before the first mount")
            if point["mount_rows"]:
                failures.append(point["label"] + ": a Workspace mount row survives Unmounted")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    with (args.output / "controller-attempts.jsonl").open("x"):
        pass
    config = json.loads(args.inputs.read_text())
    uid, gid = int(config["uid"]), int(config["gid"])
    mutation, verifier = lifecycle.scripts(args.output, uid, gid)
    result = dict(schema="r8b-lifecycle-controller-v1", status="INCOMPLETE", timing_claim="NOT_MEASURED",
                  global_profile="Disposable/WAL/OFF", overlay_profile="MEMORY/OFF/EXCLUSIVE",
                  durable="NOT_RUN — disabled by owner until explicit reauthorization",
                  construction_workers=1, overall_wall_stop_seconds=OVERALL_SECONDS, operation_attempts=1,
                  volume=config["volume"], image=config["image"], uid=uid, gid=gid,
                  sampling="one observation per phase boundary; not a continuous peak",
                  not_asserted=["native gauges after Unmounted: no product observation exists",
                                "Exec registration: the product has no registration interface to observe"],
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("mutation", mutation),
                          ("oracle", verifier), ("controller", Path(__file__)), ("inputs", args.inputs),
                          ("r7_lifecycle", Path(lifecycle.__file__)))},
                  points=[], phases=[])
    runtime = None
    deadline = time.monotonic() + OVERALL_SECONDS - 2
    failures = []

    def point(label, key, mounted, directory=None):
        start = len(runtime.rows)
        if key:
            lifecycle.attempt(runtime, result, "snapshot", key, None, 5, deadline)
            statuses = [row for row in runtime.rows[start:] if row.get("event") == "workspace_status"]
            if len(statuses) != 1 or statuses[0].get("fields", {}).get("key") != key:
                raise OriginalFailure("snapshot lacks its original Workspace Status event")
            lifecycle.control_bounds(statuses[0], 1)
        else:
            lifecycle.attempt(runtime, result, "observe", "", None, 5, deadline)
        snapshots = [row for row in runtime.rows[start:] if row.get("event") == "external_snapshot"]
        if len(snapshots) != 1:
            raise OriginalFailure("no single original snapshot for " + label)
        observed = parse_snapshot(Path(snapshots[0]["fields"]["stdout"]).read_text())
        rows = checked(["docker", "exec", "--user", "0:0", runtime.container, "sh", "-c",
                        "grep ' /workspaces/' /proc/1/mountinfo | cut -d' ' -f5 || true"]).stdout.decode()
        observed.update(label=label, mounted=mounted, directory=directory, mount_rows=rows.split())
        result["points"].append(observed)

    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"),
                "--uid", str(uid), "--gid", str(gid)]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        origin = next((row for row in runtime.rows if row.get("event") == "control_origin"), None)
        if origin is None or origin.get("fields", {}).get("control_send_records") != 1 or not runtime.container:
            raise OriginalFailure("fresh real serve identity/one-Hello origin unavailable")
        first, fresh = secrets.token_hex(32), secrets.token_hex(32)
        point("before any mount", None, False)
        directory = lifecycle.attempt(runtime, result, "mount", first, None, 10, deadline,
                                      controls=2)["fields"]["directory"]
        point("first Workspace mounted", first, True, directory)
        lifecycle.attempt(runtime, result, "command", first, mutation, 9.5, deadline)
        lifecycle.verify(runtime, result, first, verifier, "before_commit_oracle", deadline)
        commit = lifecycle.attempt(runtime, result, "commit", first, None, 15, deadline, controls=1)
        result["commit_kind"] = commit.get("fields", {}).get("commit_kind")
        if result["commit_kind"] != "Committed":
            raise OriginalFailure("new changed frontier did not produce typed Committed")
        point("after Commit, still mounted", first, True, directory)
        lifecycle.attempt(runtime, result, "unmount", first, None, 5, deadline, controls=1)
        lifecycle.cleanup(runtime, result, first, deadline)
        point("first Workspace Unmounted and Gone", None, False)
        directory = lifecycle.attempt(runtime, result, "mount", fresh, None, 10, deadline,
                                      controls=2)["fields"]["directory"]
        if lifecycle.verify(runtime, result, fresh, verifier, "fresh_mount_oracle",
                            deadline) != result["before_commit_oracle"]:
            raise OriginalFailure("independent pre-Commit and fresh-mount oracle results differ")
        point("fresh Workspace mounted", fresh, True, directory)
        lifecycle.attempt(runtime, result, "unmount", fresh, None, 5, deadline, controls=1)
        lifecycle.cleanup(runtime, result, fresh, deadline)
        point("fresh Workspace Unmounted and Gone", None, False)
        lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
        runtime.process.stdin.close()
        code = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
        if code:
            raise OriginalFailure("original runtime host exit is nonzero: " + str(code))
        result.update(container_stop="ACKNOWLEDGED", host_exit=code)
        failures = judge(result["points"])
    except Exception as original:
        result.update(original_failure_type=type(original).__name__, original_failure=str(original))
        failures.append("controller failure: " + str(original))
        if runtime is None:
            runtime = getattr(original, "event_process", None)
        if runtime is not None:
            result["custody"] = runtime.retain(original)
    finally:
        if runtime is not None:
            result.update(container=runtime.container, daemon_instance=runtime.daemon_instance)
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
        result.update(judged_failures=failures, status="FAIL" if failures else "PASS")
        write(args.output / "result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "phases"}, sort_keys=True))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
