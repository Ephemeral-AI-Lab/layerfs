"""One actual protected-path and alias matrix for an ordinary Sandbox command.

Reuses the R7 runtime's SDK/Sandbox control and original event custody over an
already prepared owned volume: two Workspaces of one daemon, one unprivileged
command started in the first, a root observer for the protected objects' real
ownership and modes, and an explicit terminal teardown. Functional only; no
timing claim. One attempt per operation; a failure keeps the original custody.

Judged: no protected cell may be allowed, the command holds no inherited
descriptor, its identity and capability sets are the declared ones, it cannot
create a user and mount namespace (the Sandbox denies it since owner decision
C-1, 2026-10-10), both Workspaces and the controller's own Status keep
answering afterwards. Recorded and not judged: sibling visibility and an
unauthenticated connect to the control listener, both declared as they are by
owner decision C-11, and name metadata.
"""
import argparse
import json
from pathlib import Path
import secrets
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

OVERALL_SECONDS = 60
PROBE_SECONDS = 20
PORT = 30421
DENIED_PROCESS_READ = "EPERM"
OPEN_DECLARATIONS = {
    "sibling": "DECLARED (C-11): Workspaces of one daemon are visible to each other's commands",
    "control_listener": "DECLARED (C-11): a connect gives nothing without the authenticated channel",
}


def observer(container, script, timeout=10):
    """Root observer in the daemon's container; reads only."""
    return checked(["docker", "exec", "--user", "0:0", container, "sh", "-c", script],
                   timeout=timeout).stdout.decode()


def judge(report, directories, uid, gid):
    failures = []
    for cell in report["cells"]:
        if cell["protected"] and cell["outcome"] == "ALLOWED":
            failures.append("protected route allowed: %(group)s %(target)s %(action)s" % cell)
        if cell["action"] == "process_vm_readv" and cell["outcome"] != DENIED_PROCESS_READ:
            failures.append("daemon memory read was not refused at the access decision: " + cell["outcome"])
    identity = report["identity"]
    status = identity["status"]
    if (identity["uid"], identity["gid"]) != (uid, gid):
        failures.append("command identity differs from the declared one")
    for name in ("CapInh", "CapPrm", "CapEff", "CapAmb"):
        if int(status.get(name, "1"), 16):
            failures.append("nonempty capability set: " + name)
    if status.get("NoNewPrivs") != "1":
        failures.append("no-new-privileges is not set")
    for name, target in report["own_descriptors"].items():
        if name not in {"0", "1", "2"}:
            failures.append("inherited descriptor %s -> %s" % (name, target))
    inside = report["namespace"]
    if inside.get("unshare_user_mount") == "ALLOWED":
        failures.append("the command created a user and mount namespace")
    for key, value in inside.items():
        if key.startswith("inside:/") and value == "ALLOWED":
            failures.append("protected path readable inside a private namespace: " + key)
    if report["cwd"] != directories[0] or report["sibling"] != directories[1]:
        failures.append("probe ran outside the declared Workspaces")
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
    config = json.loads(args.inputs.read_text())
    uid, gid = int(config["uid"]), int(config["gid"])
    probe = Path(__file__).with_name("confinement_probe.py")
    result = dict(schema="r8b-confinement-controller-v1", status="INCOMPLETE",
                  timing_claim="NOT_MEASURED", global_profile="Disposable/WAL/OFF",
                  overlay_profile="MEMORY/OFF/EXCLUSIVE", construction_workers=1,
                  overall_wall_stop_seconds=OVERALL_SECONDS, operation_attempts=1,
                  volume=config["volume"], image=config["image"], uid=uid, gid=gid,
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("probe", probe),
                          ("controller", Path(__file__)), ("inputs", args.inputs))},
                  not_judged=OPEN_DECLARATIONS, phases=[])
    runtime = None
    deadline = time.monotonic() + OVERALL_SECONDS - 2
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"),
                "--uid", str(uid), "--gid", str(gid)]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        if not runtime.container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", runtime.container, "mkdir", "-m", "755", "/code"])
        checked(["docker", "cp", str(probe), runtime.container + ":/code/confinement_probe.py"])
        keys = [secrets.token_hex(32), secrets.token_hex(32)]
        directories = [lifecycle.attempt(runtime, result, "mount", key, None, 5, deadline,
                                         controls=2)["fields"]["directory"] for key in keys]
        (args.output / "container-inspect.json").write_bytes(
            checked(["docker", "inspect", runtime.container]).stdout)
        objects = " ".join(["/layerfs-store", "/layerfs-store/global", "/layerfs-store/global/*",
                            "/layerfs-local", "/layerfs-local/*", "/layerfs-local/*/*",
                            "/dev/fuse", "/workspaces", *directories])
        (args.output / "observer-before.txt").write_text(observer(
            runtime.container, "stat -c '%n %F %a %u %g' " + objects
            + "; printf 'DAEMON\\n'; grep -E '^(Uid|Gid|CapEff|NoNewPrivs|Seccomp):' /proc/1/status"
            + "; cat /proc/sys/kernel/yama/ptrace_scope 2>/dev/null || echo yama-absent"
            + "; printf 'MOUNTINFO\\n'; cat /proc/1/mountinfo"))
        body = args.output / "probe.sh"
        body.write_text("set -euo pipefail\nexec timeout --kill-after=1s %ds python3 -B "
                        "/code/confinement_probe.py %s %d\n" % (PROBE_SECONDS, directories[1], PORT))
        event = lifecycle.attempt(runtime, result, "command", keys[0], body, PROBE_SECONDS + 2, deadline)
        report = json.loads(Path(event["fields"]["stdout"]).read_text())
        if report.get("schema") != "r8b-confinement-probe-v1":
            raise OriginalFailure("probe report schema differs")
        result["report"] = report
        result["cells"] = dict(total=len(report["cells"]),
                               protected=sum(cell["protected"] for cell in report["cells"]),
                               recorded=sum(cell["record"] for cell in report["cells"]))
        failures = judge(report, directories, uid, gid)
        after = observer(runtime.container,
                         "test ! -e /tmp/confinement-fuse-twin && echo twin-absent; "
                         "printf 'MOUNTINFO\\n'; cat /proc/1/mountinfo")
        (args.output / "observer-after.txt").write_text(after)
        if "twin-absent" not in after:
            failures.append("a device node twin exists after the probe")
        for directory in directories:
            if " " + directory + " " not in after:
                failures.append("Workspace mount missing after the probe: " + directory)
        serving = args.output / "serving.sh"
        serving.write_text("set -euo pipefail\ntest -d . && ls -A | wc -l\n")
        for key in keys:
            lifecycle.attempt(runtime, result, "command", key, serving, 5, deadline)
        start = len(runtime.rows)
        lifecycle.attempt(runtime, result, "snapshot", keys[0], None, 5, deadline)
        statuses = [row for row in runtime.rows[start:] if row.get("event") == "workspace_status"]
        if len(statuses) != 1:
            failures.append("the controller's own Status did not answer after the probe")
        else:
            lifecycle.control_bounds(statuses[0], 1)
        for key in keys:
            lifecycle.attempt(runtime, result, "unmount", key, None, 5, deadline, controls=1)
            lifecycle.cleanup(runtime, result, key, deadline)
        lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
        runtime.process.stdin.close()
        code = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
        if code:
            raise OriginalFailure("runtime exit differs: " + str(code))
        result.update(container_stop="ACKNOWLEDGED", host_exit=code, judged_failures=failures,
                      terminal="both Workspaces Unmounted, Gone observed, stop acknowledged",
                      status="FAIL" if failures else "PASS")
    except Exception as original:
        result.update(status="FAIL", original_failure_type=type(original).__name__,
                      original_failure=str(original))
        if runtime is None:
            runtime = getattr(original, "event_process", None)
        if runtime is not None:
            result["custody"] = runtime.retain(original)
    finally:
        if runtime is not None:
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
        write(args.output / "result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key not in {"phases", "report"}},
                     sort_keys=True))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
