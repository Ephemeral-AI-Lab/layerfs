"""Exploratory: what a normal Unmount does while a descendant holds the mount.

Two arrangements, each over a fresh daemon on the same owned volume: the
descendant's working directory is in the daemon's own mount namespace
("shared"), or in a private user and mount namespace it created ("private").
Records the actual Unmount outcome, the daemon's mounts and threads before,
during and after the hold, whether the untouched sibling Workspace still
unmounts, and the terminal state. No verdict, no timing claim, one attempt per
operation. A failed Unmount ends the runtime's protocol loop; the container is
then stopped explicitly by this controller and that stop is recorded.
"""
import argparse
import json
from pathlib import Path
import secrets
import sys
import time

ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(ROOT / "core/benchmark/r8-tools"))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

HOLD = 14


def observe(container, label, pid, output):
    script = ("printf 'DAEMON_MOUNTS\\n'; grep -c ' /workspaces/' /proc/1/mountinfo || true; "
              "grep ' /workspaces/' /proc/1/mountinfo | cut -d' ' -f1-7 || true; "
              "printf 'HOLDER\\n'; if test -d /proc/%s; then readlink /proc/%s/cwd; "
              "grep -c ' /workspaces/' /proc/%s/mountinfo || true; else echo exited; fi; "
              "printf 'THREADS\\n'; for t in /proc/1/task/*; do cat $t/comm; done | sort | uniq -c"
              % (pid, pid, pid))
    text = checked(["docker", "exec", "--user", "0:0", container, "sh", "-c", script]).stdout.decode()
    (output / ("observe-" + label + ".txt")).write_text(text)
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--arrangement", choices=("shared", "private"), required=True)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.inputs.read_text())
    holder = Path(__file__).with_name("holder.py")
    result = dict(schema="r8b-namespace-holder-controller-v1", status="INCOMPLETE", eligible=False,
                  verdict="NONE: exploratory observation", arrangement=args.arrangement,
                  global_profile="Disposable/WAL/OFF", construction_workers=1, hold_seconds=HOLD,
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("holder", holder),
                          ("controller", Path(__file__)), ("inputs", args.inputs))},
                  volume=config["volume"], phases=[], operation_attempts=1)
    runtime = container = None
    deadline = time.monotonic() + 55
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"), "--uid", "501", "--gid", "20"]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        container = runtime.container
        if not container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", container, "mkdir", "-m", "755", "/code"])
        checked(["docker", "cp", str(holder), container + ":/code/holder.py"])
        held, sibling = secrets.token_hex(32), secrets.token_hex(32)
        for key in (held, sibling):
            lifecycle.attempt(runtime, result, "mount", key, None, 5, deadline, controls=2)
        body = args.output / "holder.sh"
        body.write_text("set -euo pipefail\nexec python3 -B /code/holder.py %d %s\n"
                        % (HOLD, args.arrangement))
        event = lifecycle.attempt(runtime, result, "command", held, body, 5, deadline)
        began = time.monotonic()
        report = json.loads(Path(event["fields"]["stdout"]).read_text())
        result["holder"] = report
        pid = str(report["pid"])
        observe(container, "1-held-before-unmount", pid, args.output)
        started = time.monotonic()
        try:
            lifecycle.attempt(runtime, result, "unmount", held, None, 9, deadline, controls=1)
            result["unmount_while_held"] = "RETURNED_UNMOUNTED"
        except Exception as refused:
            result["unmount_while_held"] = "FAILED"
            result["unmount_original_failure"] = type(refused).__name__ + ": " + str(refused)
        result["unmount_seconds"] = round(time.monotonic() - started, 3)
        observe(container, "2-after-unmount-attempt", pid, args.output)
        if result["unmount_while_held"] == "RETURNED_UNMOUNTED":
            row = lifecycle.attempt(runtime, result, "cleanup", held, None, 5, deadline, controls=1)
            result["cleanup_while_held"] = row.get("fields", {}).get("state")
            try:
                lifecycle.attempt(runtime, result, "unmount", sibling, None, 9, deadline, controls=1)
                lifecycle.cleanup(runtime, result, sibling, deadline)
                result["sibling_unmount"] = "Unmounted, Gone observed"
            except Exception as other:
                result["sibling_unmount"] = "FAILED: " + str(other)
        time.sleep(max(0, HOLD + 1.5 - (time.monotonic() - began)))
        observe(container, "3-after-holder-exit", pid, args.output)
        if runtime.process.poll() is None and result.get("sibling_unmount", "").startswith("Unmounted"):
            if result.get("cleanup_while_held") != "Gone":
                row = lifecycle.attempt(runtime, result, "cleanup", held, None, 5, deadline, controls=1)
                result["cleanup_after_holder_exit"] = row.get("fields", {}).get("state")
            lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
            runtime.process.stdin.close()
            result["host_exit"] = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
            result["container_stop"] = "ACKNOWLEDGED by the runtime"
        result["status"] = "COMPLETE"
    except Exception as original:
        result.update(status="FAILED", original_failure_type=type(original).__name__,
                      original_failure=str(original))
    finally:
        if runtime is not None:
            if result.get("container_stop") is None:
                result["custody"] = runtime.retain(RuntimeError(result.get(
                    "unmount_original_failure", result.get("original_failure", "protocol loop ended"))))
                if container:
                    stopped = __import__("subprocess").run(["docker", "stop", "-t", "1", container],
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
        write(args.output / "result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "phases"}, sort_keys=True))


if __name__ == "__main__":
    main()
