"""Normal Unmount against one real reference held by an ordinary command.

One arrangement per invocation, each on a fresh daemon over an already
prepared owned volume, through the R7 runtime's SDK/Sandbox control: two
Workspaces; a descendant of an unprivileged command holds one reference into
the first (see mount_holder.py); the untouched sibling is unmounted first,
then one normal Unmount of the held Workspace is attempted. Functional only,
no timing claim, one attempt per operation.

Judged against the mount propagation contract: the sibling must reach
Unmounted and Gone while the first is held; the held Unmount must be either
the reversible Busy refusal at `unmount:kernel` with the mount still present,
or a complete Unmounted. Anything else, including retained teardown custody,
is a failure of this arrangement. A Busy refusal ends the runtime's protocol
loop, so the container is then stopped explicitly here and that is recorded.
"""
import argparse
import json
from pathlib import Path
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

HOLD_SECONDS = 9
OVERALL_SECONDS = 45
ARRANGEMENTS = ("cwd", "descriptor", "mapping", "private")
BUSY = ("ControlRefusal { code: Busy, phase: \"unmount:kernel\"", "no terminal effect")


def observe(container, label, pid, output):
    script = ("printf 'DAEMON_MOUNTS\\n'; grep ' /workspaces/' /proc/1/mountinfo | cut -d' ' -f1-7 || true; "
              "printf 'HOLDER\\n'; if test -d /proc/%s; then readlink /proc/%s/cwd; "
              "grep -c ' /workspaces/' /proc/%s/mountinfo || true; else echo exited; fi; "
              "printf 'THREADS\\n'; for t in /proc/1/task/*; do cat $t/comm; done | sort | uniq -c"
              % (pid, pid, pid))
    text = checked(["docker", "exec", "--user", "0:0", container, "sh", "-c", script]).stdout.decode()
    (output / ("observe-" + label + ".txt")).write_text(text)
    return text.split("HOLDER\n")[0]


def judge(result):
    """The arrangement's verdict from recorded outcomes only."""
    failures = []
    if result.get("sibling_unmount") != "Unmounted, Gone observed":
        failures.append("a reference into one Workspace kept its sibling from unmounting")
    held = result.get("unmount_while_held")
    if held == "BUSY":
        if not result.get("held_mount_present_after_busy"):
            failures.append("Busy was reported but the mount is gone")
    elif held == "UNMOUNTED":
        if result.get("held_mount_present_after_unmount"):
            failures.append("Unmounted was reported but the mount is still present")
        if result.get("cleanup_after_holder_exit") != "Gone":
            failures.append("Unmounted was reported but cleanup was not observed Gone")
    else:
        failures.append("normal Unmount was neither the reversible Busy nor a complete Unmounted")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--arrangement", choices=ARRANGEMENTS, required=True)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.inputs.read_text())
    holder = Path(__file__).with_name("mount_holder.py")
    result = dict(schema="r8b-mount-holder-controller-v1", status="INCOMPLETE",
                  timing_claim="NOT_MEASURED", arrangement=args.arrangement,
                  global_profile="Disposable/WAL/OFF", overlay_profile="MEMORY/OFF/EXCLUSIVE",
                  construction_workers=1, hold_seconds=HOLD_SECONDS,
                  overall_wall_stop_seconds=OVERALL_SECONDS, operation_attempts=1,
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("holder", holder),
                          ("controller", Path(__file__)), ("inputs", args.inputs))},
                  volume=config["volume"], image=config["image"], phases=[])
    runtime = container = None
    deadline = time.monotonic() + OVERALL_SECONDS - 2
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"),
                "--uid", str(config["uid"]), "--gid", str(config["gid"])]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        container = runtime.container
        if not container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", container, "mkdir", "-m", "755", "/code"])
        checked(["docker", "cp", str(holder), container + ":/code/mount_holder.py"])
        held, sibling = secrets.token_hex(32), secrets.token_hex(32)
        directory = lifecycle.attempt(runtime, result, "mount", held, None, 5, deadline,
                                      controls=2)["fields"]["directory"]
        lifecycle.attempt(runtime, result, "mount", sibling, None, 5, deadline, controls=2)
        body = args.output / "holder.sh"
        body.write_text("set -euo pipefail\nexec python3 -B /code/mount_holder.py %d %s\n"
                        % (HOLD_SECONDS, args.arrangement))
        event = lifecycle.attempt(runtime, result, "command", held, body, 5, deadline)
        began = time.monotonic()
        report = json.loads(Path(event["fields"]["stdout"]).read_text())
        if report.get("schema") != "r8b-mount-holder-v1" or report.get("mount") != directory:
            raise OriginalFailure("holder report differs from the mounted Workspace")
        result["holder"] = report
        pid = str(report["pid"])
        if " " + directory + " " not in observe(container, "1-held", pid, args.output):
            raise OriginalFailure("held Workspace mount absent before any Unmount")
        try:
            lifecycle.attempt(runtime, result, "unmount", sibling, None, 5, deadline, controls=1)
            lifecycle.cleanup(runtime, result, sibling, deadline)
            result["sibling_unmount"] = "Unmounted, Gone observed"
        except Exception as other:
            result["sibling_unmount"] = "FAILED: " + str(other)
            raise
        started = time.monotonic()
        try:
            lifecycle.attempt(runtime, result, "unmount", held, None, 8, deadline, controls=1)
            result["unmount_while_held"] = "UNMOUNTED"
        except Exception as refused:
            text = str(refused)
            result["unmount_original_failure"] = type(refused).__name__ + ": " + text
            result["unmount_while_held"] = ("BUSY" if all(part in text for part in BUSY)
                                            else "RETAINED" if "Retained(TeardownCustody" in text
                                            else "FAILED")
        result["unmount_seconds_not_a_measurement"] = round(time.monotonic() - started, 3)
        if time.monotonic() - began >= HOLD_SECONDS - 1:
            raise OriginalFailure("the hold ended before the Unmount outcome was observed")
        mounts = observe(container, "2-after-unmount-attempt", pid, args.output)
        present = " " + directory + " " in mounts
        result["held_mount_present_after_" + ("busy" if result["unmount_while_held"] == "BUSY"
                                              else "unmount")] = present
        time.sleep(max(0, HOLD_SECONDS + 1.5 - (time.monotonic() - began)))
        observe(container, "3-after-holder-exit", pid, args.output)
        if result["unmount_while_held"] == "UNMOUNTED":
            lifecycle.cleanup(runtime, result, held, deadline)
            result["cleanup_after_holder_exit"] = "Gone"
            lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
            runtime.process.stdin.close()
            result["host_exit"] = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
            result["container_stop"] = "ACKNOWLEDGED by the runtime"
    except Exception as original:
        result.update(original_failure_type=type(original).__name__, original_failure=str(original))
    finally:
        if runtime is not None:
            if result.get("container_stop") is None:
                result["custody"] = runtime.retain(RuntimeError(result.get(
                    "unmount_original_failure", result.get("original_failure", "protocol loop ended"))))
                if container:
                    stopped = subprocess.run(["docker", "stop", "-t", "1", container],
                                             capture_output=True, timeout=15)
                    result["container_stop"] = ("EXPLICIT docker stop by this controller, exit %d"
                                                % stopped.returncode)
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
        failures = judge(result)
        if "original_failure" in result:
            failures.append("controller failure: " + result["original_failure"])
        result.update(judged_failures=failures, status="FAIL" if failures else "PASS")
        write(args.output / "result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "phases"}, sort_keys=True))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
